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
    if let Err(e) = finalize(app, slug, &record, &dest, is_tar, &download_path) {
        // download is done but installing failed — clear the record + partial so it
        // doesn't linger as a bogus "resumable" download; a retry starts clean.
        config::remove_download(app, slug);
        let _ = fs::remove_file(&download_path);
        emit(app, slug, "error", received, total, Some(&e));
        return Err(e);
    }

    let exe = find_exe(&dest, record.exe_hint.as_deref());
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
    })
}

fn finalize(
    app: &AppHandle,
    slug: &str,
    record: &DownloadRecord,
    dest: &Path,
    is_tar: bool,
    download_path: &Path,
) -> Result<(), String> {
    if is_tar {
        emit(app, slug, "extract", 0, 1, Some("Extracting…"));
        extract_tar(app, slug, download_path, dest)?;
        let _ = fs::remove_file(download_path);
        if let Some(rel) = &record.payload_path {
            let payload = dest.join(rel.replace('/', "\\"));
            if record.setup_type == "iso" && payload.exists() {
                emit(app, slug, "install", 0, 0, Some("Running disc setup…"));
                launch::run_installer_from_iso(&payload)?;
            } else if record.setup_type == "installer" && payload.exists() {
                emit(app, slug, "install", 0, 0, Some("Running installer…"));
                let dir = payload.parent().unwrap_or(dest).to_path_buf();
                launch::run_and_wait(&payload, &dir, false)?;
            }
        }
    } else {
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
            }
            "iso" => {
                emit(app, slug, "install", 0, 0, Some("Running disc setup…"));
                launch::run_installer_from_iso(download_path)?;
            }
            "exe" | "msi" => {
                emit(app, slug, "install", 0, 0, Some("Running installer…"));
                launch::run_and_wait(download_path, dest, false)?;
            }
            _ => {}
        }
    }
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
    use super::safe_join;
    use std::path::Path;

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
}
