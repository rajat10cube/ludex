//! Download (pausable / resumable) + extract/install a game, then report it.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::config::{self, AppConfig, DownloadRecord, InstallEntry, InstallState};
use crate::launch;
use crate::server::Server;

const JUNK_EXE: &[&str] = &[
    "unins", "setup", "install", "redist", "vcredist", "vc_redist", "dxsetup", "dxwebsetup",
    "dotnet", "oalinst", "crashhandler", "unitycrashhandler", "crashreport", "crashpad",
    "touchup", "cleanup", "activate", "register",
];

/// Disc-image extensions we know how to mount + run.
const ISO_EXTS: &[&str] = &["iso", "mds", "mdf", "nrg"];
/// Directory names on a scene disc that usually hold the crack to copy after setup.
const CRACK_DIR_NAMES: &[&str] = &[
    "crack", "crackfix", "cracked", "fix", "reloaded", "skidrow", "codex", "plaza", "prophet",
    "hoodlum", "razor1911", "rld", "razordox", "tenoke",
];
/// Where a staged crack is copied inside the game's Ludex folder.
const CRACK_STAGE_DIR: &str = "_Ludex-Crack";

// download control states (shared atomic per active install)
pub const RUN: u8 = 0;
pub const PAUSE: u8 = 1;
pub const CANCEL: u8 = 2;

/// Tracks the control flag of each active download so pause/cancel can reach it.
#[derive(Default)]
pub struct InstallManager {
    controls: Mutex<HashMap<String, Arc<AtomicU8>>>,
}

impl InstallManager {
    pub fn register(&self, slug: &str) -> Arc<AtomicU8> {
        let ctrl = Arc::new(AtomicU8::new(RUN));
        self.controls.lock().unwrap().insert(slug.to_string(), ctrl.clone());
        ctrl
    }
    /// Returns true if a download for `slug` was active (and got the signal).
    pub fn signal(&self, slug: &str, state: u8) -> bool {
        match self.controls.lock().unwrap().get(slug) {
            Some(c) => {
                c.store(state, Ordering::SeqCst);
                true
            }
            None => false,
        }
    }
    pub fn unregister(&self, slug: &str) {
        self.controls.lock().unwrap().remove(slug);
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct InstallProgress<'a> {
    slug: &'a str,
    phase: &'a str,
    received: u64,
    total: u64,
    message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallStatus {
    pub slug: String,
    pub status: String, // "installed" | "paused" | "cancelled"
    pub install_path: Option<String>,
    pub exe: Option<String>,
    pub note: Option<String>,
}

/// What `finalize` did with the payload, so the caller can decide whether to
/// hunt for a launchable exe and what to tell the user afterwards.
#[derive(Default)]
struct Finalized {
    ran_setup: bool,       // an installer/disc setup ran; the game lives elsewhere
    note: Option<String>,  // a post-install instruction (e.g. staged crack)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRecordView {
    pub slug: String,
    pub title: String,
    pub bytes: u64,
    pub total: u64,
}

fn emit(app: &AppHandle, slug: &str, phase: &str, received: u64, total: u64, message: Option<&str>) {
    let _ = app.emit(
        "install-progress",
        InstallProgress { slug, phase, received, total, message: message.map(|m| m.to_string()) },
    );
}

/// Leftover download records (paused or interrupted) that can be resumed.
pub fn list_paused(app: &AppHandle) -> Vec<DownloadRecordView> {
    config::load_downloads(app)
        .into_values()
        .map(|r| {
            let total = r.size_hint.max(r.bytes);
            DownloadRecordView { slug: r.slug, title: r.title, bytes: r.bytes, total }
        })
        .collect()
}

/// Delete a paused download's partial file + record (used by cancel when idle).
pub fn discard_paused(app: &AppHandle, slug: &str) {
    if let Some(r) = config::load_downloads(app).get(slug) {
        let _ = fs::remove_file(&r.temp_path);
    }
    config::remove_download(app, slug);
}

fn build_record(server: &Server, cfg: &AppConfig, slug: &str) -> Result<DownloadRecord, String> {
    let game = server.game(slug)?;
    let is_tar = game["downloadKind"].as_str().unwrap_or("file") == "tar";
    let dest = PathBuf::from(&cfg.install_dir).join(slug);
    let download_name = game["downloadName"].as_str().unwrap_or("download.bin").to_string();
    let temp_path = if is_tar {
        std::env::temp_dir().join(format!("ludex-{slug}.tar"))
    } else {
        dest.join(&download_name)
    };
    Ok(DownloadRecord {
        slug: slug.to_string(),
        title: game["title"].as_str().unwrap_or(slug).to_string(),
        download_kind: game["downloadKind"].as_str().unwrap_or("file").to_string(),
        download_name,
        setup_type: game["setupType"].as_str().unwrap_or("portable").to_string(),
        exe_hint: game["exeHint"].as_str().map(String::from),
        payload_path: game["payloadPath"].as_str().map(String::from),
        hypervisor: game["requiresHypervisor"].as_bool().unwrap_or(false),
        version: game["version"].as_str().map(String::from),
        size_hint: game["sizeBytes"].as_u64().unwrap_or(0),
        dest: dest.to_string_lossy().to_string(),
        temp_path: temp_path.to_string_lossy().to_string(),
        bytes: 0,
        paused: false,
    })
}

pub fn run(
    app: &AppHandle,
    server: &Server,
    cfg: &AppConfig,
    slug: &str,
    resume: bool,
    control: Arc<AtomicU8>,
) -> Result<InstallStatus, String> {
    let mut record = if resume {
        config::load_downloads(app)
            .remove(slug)
            .ok_or("No paused download to resume")?
    } else {
        build_record(server, cfg, slug)?
    };

    let is_tar = record.download_kind == "tar";
    let dest = PathBuf::from(&record.dest);
    if !resume {
        // Fresh install: wipe any prior contents so stale files (e.g. a previous
        // manual RAR/ISO extraction) can't confuse disc/archive detection later.
        let _ = fs::remove_dir_all(&dest);
    }
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let download_path = PathBuf::from(&record.temp_path);

    // resume from the actual partial size (or start over if it vanished)
    let resume_from = if resume {
        fs::metadata(&download_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };
    record.bytes = resume_from;
    config::save_download(app, &record); // persist so a crash stays resumable

    let mut file = if resume_from > 0 {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&download_path)
            .map_err(|e| e.to_string())?
    } else {
        if let Some(p) = download_path.parent() {
            let _ = fs::create_dir_all(p);
        }
        File::create(&download_path).map_err(|e| e.to_string())?
    };

    // --- download loop (pausable) ---
    emit(app, slug, "download", resume_from, record.size_hint.max(resume_from + 1), Some("Downloading…"));
    let mut resp = server.download(slug, resume_from, is_tar)?;
    let total = if is_tar {
        record.size_hint.max(resume_from + 1)
    } else {
        resp.content_length()
            .map(|c| resume_from + c)
            .unwrap_or(record.size_hint.max(resume_from + 1))
    };

    let mut received = resume_from;
    let mut buf = vec![0u8; 256 * 1024];
    let mut last_emit = Instant::now();
    let mut last_save = Instant::now();
    loop {
        match control.load(Ordering::SeqCst) {
            PAUSE => {
                let _ = file.flush();
                record.bytes = received;
                record.paused = true;
                config::save_download(app, &record);
                emit(app, slug, "paused", received, total, None);
                return Ok(InstallStatus {
                    slug: slug.to_string(),
                    status: "paused".to_string(),
                    install_path: None,
                    exe: None,
                    note: None,
                });
            }
            CANCEL => {
                drop(file);
                let _ = fs::remove_file(&download_path);
                config::remove_download(app, slug);
                emit(app, slug, "cancelled", received, total, None);
                return Ok(InstallStatus {
                    slug: slug.to_string(),
                    status: "cancelled".to_string(),
                    install_path: None,
                    exe: None,
                    note: None,
                });
            }
            _ => {}
        }
        let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        received += n as u64;
        if last_emit.elapsed() > Duration::from_millis(150) {
            emit(app, slug, "download", received, total.max(received), None);
            last_emit = Instant::now();
        }
        if last_save.elapsed() > Duration::from_secs(3) {
            record.bytes = received;
            config::save_download(app, &record);
            last_save = Instant::now();
        }
    }
    let _ = file.flush();
    drop(file);
    emit(app, slug, "download", received, total.max(received), None);

    // --- extract / place / run installer ---
    let outcome = match finalize(app, slug, &record, &dest, is_tar, &download_path) {
        Ok(o) => o,
        Err(e) => {
            // download is done but installing failed — clear the record + partial so
            // it doesn't linger as a bogus "resumable" download; a retry starts clean.
            config::remove_download(app, slug);
            let _ = fs::remove_file(&download_path);
            emit(app, slug, "error", received, total, Some(&e));
            return Err(e);
        }
    };

    // Games installed by their own setup (disc/installer/RAR->ISO) live wherever
    // setup put them, not under dest — so don't offer a bogus exe to launch.
    let exe = if outcome.ran_setup {
        None
    } else {
        find_exe(&dest, record.exe_hint.as_deref())
    };
    let mut state = config::load_state(app);
    state.insert(
        slug.to_string(),
        InstallEntry {
            title: record.title.clone(),
            version: record.version.clone(),
            path: dest.to_string_lossy().to_string(),
            exe: exe.clone(),
            setup_type: record.setup_type.clone(),
            hypervisor: record.hypervisor,
            note: outcome.note.clone(),
        },
    );
    config::save_state(app, &state)?;
    report_installed(server, cfg, &state);
    config::remove_download(app, slug);

    emit(app, slug, "done", total, total, None);
    Ok(InstallStatus {
        slug: slug.to_string(),
        status: "installed".to_string(),
        install_path: Some(dest.to_string_lossy().to_string()),
        exe,
        note: outcome.note,
    })
}

fn finalize(
    app: &AppHandle,
    slug: &str,
    record: &DownloadRecord,
    dest: &Path,
    is_tar: bool,
    download_path: &Path,
) -> Result<Finalized, String> {
    if is_tar {
        emit(app, slug, "extract", 0, 1, Some("Extracting…"));
        extract_tar(app, slug, download_path, dest)?;
        let _ = fs::remove_file(download_path);
        return run_payload(app, slug, record, dest);
    }

    // A single downloaded file (loose, not a folder tar).
    let ext = download_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "zip" => {
            emit(app, slug, "extract", 0, 1, Some("Extracting…"));
            extract_zip(app, slug, download_path, dest)?;
            let _ = fs::remove_file(download_path);
            run_payload(app, slug, record, dest)
        }
        "rar" => run_payload(app, slug, record, dest),
        e if ISO_EXTS.contains(&e) => run_iso(app, slug, download_path, dest),
        "exe" | "msi" => {
            emit(app, slug, "install", 0, 0, Some("Running installer…"));
            launch::run_and_wait(download_path, dest, false)?;
            Ok(Finalized { ran_setup: true, note: None })
        }
        _ => Ok(Finalized::default()),
    }
}

/// After the payload is on disk under `dest`, drive it to a runnable state:
/// unpack any split-RAR set, then mount+run a disc image, else run an installer.
fn run_payload(
    app: &AppHandle,
    slug: &str,
    record: &DownloadRecord,
    dest: &Path,
) -> Result<Finalized, String> {
    // 1) split-RAR scene set → extract in place, then drop the volumes.
    let rar_first = record
        .payload_path
        .as_ref()
        .filter(|_| record.setup_type == "rar")
        .map(|p| dest.join(p.replace('/', "\\")))
        .filter(|p| p.exists())
        .or_else(|| find_rar_first_volume(dest));
    if let Some(first) = rar_first {
        emit(app, slug, "extract", 0, 1, Some("Extracting RAR archive…"));
        extract_rar(app, slug, &first, dest)?;
        delete_rar_set(&first);
    }

    // 2) a disc image (often what the RAR held) → mount + run its setup.
    if let Some(iso) = find_largest_iso(dest) {
        return run_iso(app, slug, &iso, dest);
    }

    // 3) an installer in the tree → run it.
    let installer = record
        .payload_path
        .as_ref()
        .filter(|_| record.setup_type == "installer")
        .map(|p| dest.join(p.replace('/', "\\")))
        .filter(|p| p.exists())
        .or_else(|| find_installer(dest));
    if let Some(inst) = installer {
        emit(app, slug, "install", 0, 0, Some("Running installer…"));
        let dir = inst.parent().unwrap_or(dest).to_path_buf();
        launch::run_and_wait(&inst, &dir, false)?;
        return Ok(Finalized { ran_setup: true, note: None });
    }

    // Nothing to run — a portable/extracted game. Caller will find the exe.
    Ok(Finalized::default())
}

/// Mount an ISO, run its setup, and (while still mounted) stage any crack for the
/// user to copy — Ludex won't overwrite the installed game itself.
///
/// A disc setup is the one interactive, finicky link in the chain, so a failure
/// here is never fatal: the .iso and staged crack are left in place and the note
/// tells the user exactly how to finish by hand.
fn run_iso(app: &AppHandle, slug: &str, iso: &Path, dest: &Path) -> Result<Finalized, String> {
    emit(app, slug, "install", 0, 0, Some("Mounting disc and running setup…"));
    let drive = launch::mount_iso(iso)?;
    // Run setup from a writable working dir (never the read-only disc root).
    let setup_res = launch::run_setup_on_drive(&drive, dest);
    // Copy the crack out before we lose access to the mounted volume.
    let crack = stage_crack(Path::new(&drive), dest);
    launch::dismount_iso(iso);

    let mut parts: Vec<String> = Vec::new();
    match setup_res {
        Ok(true) => {}
        Ok(false) => parts.push(format!(
            "No installer was found on the disc image, so nothing was run. The disc image is \
             kept at:\n{}\nMount it and run its setup yourself if needed.",
            iso.display()
        )),
        Err(e) => parts.push(format!(
            "Ludex couldn't run the disc's setup automatically ({e}). To finish: open\n{}\n\
             (double-click to mount it), run its setup, and install the game.",
            iso.display()
        )),
    }
    if let Some(c) = crack {
        parts.push(c);
    }
    let note = if parts.is_empty() { None } else { Some(parts.join("\n\n")) };
    Ok(Finalized { ran_setup: true, note })
}

/// Extract a (possibly multi-volume) RAR, reporting cumulative unpacked bytes.
/// `first_volume` is the first part; unrar follows the rest automatically.
fn extract_rar(app: &AppHandle, slug: &str, first_volume: &Path, dest: &Path) -> Result<(), String> {
    // Pass 1: total unpacked size for a real progress bar (headers only).
    let total: u64 = unrar::Archive::new(first_volume)
        .open_for_listing()
        .map_err(|e| format!("Couldn't read the RAR: {e}"))?
        .flatten()
        .map(|h| h.unpacked_size)
        .sum::<u64>()
        .max(1);

    let mut archive = unrar::Archive::new(first_volume)
        .open_for_processing()
        .map_err(|e| format!("Couldn't open the RAR: {e}"))?;
    let mut done: u64 = 0;
    let mut last = Instant::now();
    while let Some(header) = archive.read_header().map_err(|e| e.to_string())? {
        let entry = header.entry();
        let is_file = entry.is_file();
        let size = entry.unpacked_size;
        let out = safe_join(dest, &entry.filename); // also blocks path traversal
        archive = if is_file {
            if let Some(parent) = out.parent() {
                let _ = fs::create_dir_all(parent);
            }
            header
                .extract_to(&out)
                .map_err(|e| format!("RAR extraction failed: {e}"))?
        } else {
            header.skip().map_err(|e| e.to_string())?
        };
        if is_file {
            done += size;
            if last.elapsed() > Duration::from_millis(200) {
                emit(app, slug, "extract", done.min(total), total, Some("Extracting RAR archive…"));
                last = Instant::now();
            }
        }
    }
    emit(app, slug, "extract", total, total, Some("Extracting RAR archive…"));
    Ok(())
}

/// A Read wrapper that reports how far through the archive we are (throttled).
struct ProgressReader<'a, R: Read> {
    inner: R,
    read: u64,
    total: u64,
    last: Instant,
    app: &'a AppHandle,
    slug: &'a str,
}

impl<R: Read> Read for ProgressReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        if self.last.elapsed() > Duration::from_millis(150) {
            emit(self.app, self.slug, "extract", self.read, self.total, None);
            self.last = Instant::now();
        }
        Ok(n)
    }
}

/// Join `rel` under `dest`, replacing characters that are illegal in Windows
/// file names (e.g. the ':' in "Game: Deluxe Edition") so extraction can't fail
/// on names that were legal on the Linux server / inside the archive.
fn safe_join(dest: &Path, rel: &Path) -> PathBuf {
    let mut out = dest.to_path_buf();
    for comp in rel.components() {
        if let std::path::Component::Normal(seg) = comp {
            let cleaned: String = seg
                .to_string_lossy()
                .chars()
                .map(|c| match c {
                    ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                    c if (c as u32) < 0x20 => '_',
                    c => c,
                })
                .collect();
            // Windows also forbids a trailing space or dot on a name.
            let cleaned = cleaned.trim_end_matches(|c| c == ' ' || c == '.');
            out.push(if cleaned.is_empty() { "_" } else { cleaned });
        }
    }
    out
}

/// Extract an (uncompressed) tar, streaming byte progress off the archive read.
fn extract_tar(app: &AppHandle, slug: &str, archive: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| e.to_string())?;
    let total = file.metadata().map(|m| m.len()).unwrap_or(0).max(1);
    let reader = ProgressReader { inner: file, read: 0, total, last: Instant::now(), app, slug };
    let mut archive = tar::Archive::new(reader);
    archive.set_overwrite(true);
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let rel = entry.path().map_err(|e| e.to_string())?.into_owned();
        let out = safe_join(dest, &rel);
        if let Some(parent) = out.parent() {
            let _ = fs::create_dir_all(parent);
        }
        entry.unpack(&out).map_err(|e| format!("Extraction failed: {e}"))?;
    }
    emit(app, slug, "extract", total, total, None);
    Ok(())
}

/// Extract a .zip, reporting progress by cumulative compressed bytes.
fn extract_zip(app: &AppHandle, slug: &str, archive: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| e.to_string())?;
    let total = file.metadata().map(|m| m.len()).unwrap_or(0).max(1);
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("Bad zip: {e}"))?;
    let mut done: u64 = 0;
    let mut last = Instant::now();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match entry.enclosed_name() {
            Some(p) => safe_join(dest, &p),
            None => continue,
        };
        if entry.is_dir() {
            let _ = fs::create_dir_all(&outpath);
        } else {
            if let Some(parent) = outpath.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let mut out = File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
        done += entry.compressed_size();
        if last.elapsed() > Duration::from_millis(150) {
            emit(app, slug, "extract", done.min(total), total, None);
            last = Instant::now();
        }
    }
    emit(app, slug, "extract", total, total, None);
    Ok(())
}

/// Files under `dir` down to `max_depth` (0 = only `dir` itself), skipping our
/// own staging folder.
fn walk(dir: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else { continue };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = p.file_name().unwrap_or_default().to_string_lossy();
                if depth < max_depth && name != CRACK_STAGE_DIR {
                    stack.push((p, depth + 1));
                }
            } else {
                found.push(p);
            }
        }
    }
    found
}

/// The shared stem of a split-RAR volume, or None. Mirrors the server classifier:
/// `foo.rar`/`foo.r07`/`foo.part03.rar` all share base `foo`.
fn rar_base(name: &str) -> Option<String> {
    let low = name.to_lowercase();
    if let Some(pos) = low.rfind(".part") {
        // ...".partNN.rar"
        let tail = &low[pos..];
        if tail.ends_with(".rar") && tail[5..tail.len() - 4].chars().all(|c| c.is_ascii_digit()) {
            return Some(name[..pos].to_string());
        }
    }
    if low.ends_with(".rar") {
        return Some(name[..name.len() - 4].to_string());
    }
    // ".rNN" (2-3 digit) old-style continuation volume
    if let Some(dot) = low.rfind('.') {
        let ext = &low[dot + 1..];
        if ext.starts_with('r') && ext.len() >= 3 && ext[1..].chars().all(|c| c.is_ascii_digit()) {
            return Some(name[..dot].to_string());
        }
    }
    None
}

/// The first volume of the largest split-RAR set under `dir`, if any.
fn find_rar_first_volume(dir: &Path) -> Option<PathBuf> {
    let mut groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for p in walk(dir, 2) {
        let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
        if let Some(base) = rar_base(&name) {
            let key = format!("{}|{}", p.parent().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default(), base.to_lowercase());
            groups.entry(key).or_default().push(p);
        }
    }
    let parts = groups.into_values().max_by_key(|v| v.len())?;
    if parts.len() < 2 {
        return None; // a lone .rar isn't a split set we auto-handle
    }
    // First volume: the ".rar" (old-style) or lowest ".partNN.rar" (new-style).
    parts
        .into_iter()
        .min_by_key(|p| {
            let low = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            if let Some(pos) = low.rfind(".part") {
                let n: u32 = low[pos + 5..low.len() - 4].parse().unwrap_or(0);
                (0u8, n)
            } else if low.ends_with(".rar") {
                (0u8, 0) // old-style first volume sorts before .r00
            } else {
                (1u8, 0)
            }
        })
}

/// Delete a split-RAR set (the first volume + all its sibling parts).
fn delete_rar_set(first_volume: &Path) {
    let (Some(dir), Some(name)) = (first_volume.parent(), first_volume.file_name()) else {
        return;
    };
    let Some(base) = rar_base(&name.to_string_lossy()) else { return };
    let base_low = base.to_lowercase();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let n = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            if rar_base(&n).map(|b| b.to_lowercase()) == Some(base_low.clone()) {
                let _ = fs::remove_file(&p);
            }
        }
    }
}

/// The largest disc image under `dir` (that's the release payload).
fn find_largest_iso(dir: &Path) -> Option<PathBuf> {
    walk(dir, 2)
        .into_iter()
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| ISO_EXTS.contains(&e.to_lowercase().as_str()))
                .unwrap_or(false)
        })
        .max_by_key(|p| fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

/// A setup.exe / installer .msi in the tree (not the game itself).
fn find_installer(dir: &Path) -> Option<PathBuf> {
    walk(dir, 2).into_iter().find(|p| {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if ext != "exe" && ext != "msi" {
            return false;
        }
        let stem = p.file_stem().unwrap_or_default().to_string_lossy().to_lowercase();
        stem.contains("setup") || stem.contains("install") || stem == "autorun"
    })
}

/// Look for a crack folder on the mounted disc and copy it into the game's Ludex
/// folder so the user can apply it. Returns the instruction to show, or None.
/// Ludex never overwrites the installed game itself — see the DRM stance.
fn stage_crack(drive_root: &Path, dest: &Path) -> Option<String> {
    let crack_src = find_crack_dir(drive_root)?;
    let staging = dest.join(CRACK_STAGE_DIR);
    let _ = fs::remove_dir_all(&staging); // start clean on re-install
    if copy_dir_all(&crack_src, &staging).is_err() {
        return None;
    }
    Some(format!(
        "This release needs a crack applied after the installer finishes. Ludex copied it to:\n\
         {}\n\n\
         When setup is done, copy everything from that folder into the folder where you \
         installed the game, replacing files when asked. (Ludex won't overwrite game files \
         for you.)",
        staging.display()
    ))
}

/// A directory on the disc whose name looks like a scene crack folder.
fn find_crack_dir(drive_root: &Path) -> Option<PathBuf> {
    let mut stack = vec![(drive_root.to_path_buf(), 0usize)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else { continue };
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            if CRACK_DIR_NAMES.contains(&name.as_str()) {
                return Some(p);
            }
            if depth < 2 {
                stack.push((p, depth + 1));
            }
        }
    }
    None
}

/// Recursively copy `src` into `dst`.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Best-effort main-executable detection: honour the server's hint, else the
/// largest non-junk .exe under the install dir.
pub fn find_exe(root: &Path, hint: Option<&str>) -> Option<String> {
    if let Some(h) = hint {
        let p = root.join(h.replace('/', "\\"));
        if p.exists() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    let mut best: Option<(u64, String)> = None;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let is_exe = path
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("exe"))
                .unwrap_or(false);
            if !is_exe {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_lowercase();
            if JUNK_EXE.iter().any(|j| name.contains(j)) {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if best.as_ref().map(|(s, _)| size > *s).unwrap_or(true) {
                best = Some((size, path.to_string_lossy().to_string()));
            }
        }
    }
    best.map(|(_, p)| p)
}

pub fn report_installed(server: &Server, cfg: &AppConfig, state: &InstallState) {
    let games: Vec<Value> = state
        .iter()
        .map(|(slug, e)| json!({ "slug": slug, "version": e.version, "install_path": e.path }))
        .collect();
    server.report_installed(&cfg.device, json!(games));
}

#[cfg(test)]
mod tests {
    use super::{delete_rar_set, find_rar_first_volume, rar_base, safe_join};
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn sanitizes_windows_illegal_chars() {
        let out = safe_join(
            Path::new("D:\\Games\\slug"),
            Path::new("Assassin's Creed: Black Flag/bin/what?.exe"),
        );
        let tail = out.to_string_lossy().replace("D:\\Games\\slug\\", "");
        assert_eq!(tail, "Assassin's Creed_ Black Flag\\bin\\what_.exe");
        // no illegal char survived in the joined-under-dest portion
        assert!(!tail.contains(':'));
        assert!(!tail.contains('?'));
    }

    #[test]
    fn trims_trailing_dots_and_spaces() {
        let out = safe_join(Path::new("D:\\d"), Path::new("name. /ok"));
        assert_eq!(out.to_string_lossy().replace("D:\\d\\", ""), "name\\ok");
    }

    #[test]
    fn rar_base_recognises_volume_shapes() {
        assert_eq!(rar_base("rld-smsd.rar").as_deref(), Some("rld-smsd"));
        assert_eq!(rar_base("rld-smsd.r00").as_deref(), Some("rld-smsd"));
        assert_eq!(rar_base("rld-smsd.r77").as_deref(), Some("rld-smsd"));
        assert_eq!(rar_base("game.part03.rar").as_deref(), Some("game"));
        // not split-RAR volumes
        assert_eq!(rar_base("setup.exe"), None);
        assert_eq!(rar_base("readme.txt"), None);
        assert_eq!(rar_base("cover.r"), None); // too short to be .rNN
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ludex-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn picks_first_volume_and_deletes_the_whole_set() {
        let dir = scratch("rar");
        fs::write(dir.join("rld-smsd.rar"), b"x").unwrap();
        for i in 0..5 {
            fs::write(dir.join(format!("rld-smsd.r{i:02}")), b"x").unwrap();
        }
        fs::write(dir.join("Leeme.txt"), b"notes").unwrap(); // must be left alone

        let first = find_rar_first_volume(&dir).expect("should find the set");
        assert_eq!(first.file_name().unwrap().to_string_lossy(), "rld-smsd.rar");

        delete_rar_set(&first);
        assert!(!dir.join("rld-smsd.rar").exists());
        assert!(!dir.join("rld-smsd.r03").exists());
        assert!(dir.join("Leeme.txt").exists()); // untouched
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lone_rar_is_not_treated_as_a_split_set() {
        let dir = scratch("lone");
        fs::write(dir.join("extras.rar"), b"x").unwrap();
        assert!(find_rar_first_volume(&dir).is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
