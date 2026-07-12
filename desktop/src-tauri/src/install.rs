//! Download + extract/install a game, then report it to the server.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::config::{self, AppConfig, InstallEntry, InstallState};
use crate::launch;
use crate::server::Server;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const JUNK_EXE: &[&str] = &[
    "unins", "setup", "install", "redist", "vcredist", "vc_redist", "dxsetup", "dxwebsetup",
    "dotnet", "oalinst", "crashhandler", "unitycrashhandler", "crashreport", "crashpad",
    "touchup", "cleanup", "activate", "register",
];

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
pub struct InstallResult {
    pub slug: String,
    pub install_path: String,
    pub exe: Option<String>,
}

fn emit(app: &AppHandle, slug: &str, phase: &str, received: u64, total: u64, message: Option<&str>) {
    let _ = app.emit(
        "install-progress",
        InstallProgress { slug, phase, received, total, message: message.map(|m| m.to_string()) },
    );
}

pub fn install(
    app: &AppHandle,
    server: &Server,
    cfg: &AppConfig,
    slug: &str,
) -> Result<InstallResult, String> {
    let game = server.game(slug)?;
    let title = game["title"].as_str().unwrap_or(slug).to_string();
    let download_kind = game["downloadKind"].as_str().unwrap_or("file").to_string();
    let download_name = game["downloadName"].as_str().unwrap_or("download.bin").to_string();
    let setup_type = game["setupType"].as_str().unwrap_or("portable").to_string();
    let exe_hint = game["exeHint"].as_str().map(|s| s.to_string());
    let payload_path = game["payloadPath"].as_str().map(|s| s.to_string());
    let hypervisor = game["requiresHypervisor"].as_bool().unwrap_or(false);
    let version = game["version"].as_str().map(|s| s.to_string());
    let size_hint = game["sizeBytes"].as_u64().unwrap_or(0);

    let dest = PathBuf::from(&cfg.install_dir).join(slug);
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    // --- download ---
    emit(app, slug, "download", 0, size_hint, Some("Starting download…"));
    let mut resp = server.download(slug)?;
    let total = resp.content_length().unwrap_or(size_hint);
    let is_tar = download_kind == "tar";
    let download_path = if is_tar {
        std::env::temp_dir().join(format!("ludex-{slug}.tar"))
    } else {
        dest.join(&download_name)
    };
    {
        let mut file = File::create(&download_path).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 256 * 1024];
        let mut received: u64 = 0;
        let mut last = Instant::now();
        loop {
            let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            received += n as u64;
            if last.elapsed() > Duration::from_millis(150) {
                emit(app, slug, "download", received, total.max(received), None);
                last = Instant::now();
            }
        }
        emit(app, slug, "download", received, total.max(received), None);
    }

    // --- extract / place / run installer ---
    if is_tar {
        emit(app, slug, "extract", 0, 0, Some("Extracting…"));
        extract_archive(&download_path, &dest)?;
        let _ = fs::remove_file(&download_path);
        if let Some(rel) = &payload_path {
            let payload = dest.join(rel.replace('/', "\\"));
            if setup_type == "iso" && payload.exists() {
                emit(app, slug, "install", 0, 0, Some("Running disc setup…"));
                launch::run_installer_from_iso(&payload)?;
            } else if setup_type == "installer" && payload.exists() {
                emit(app, slug, "install", 0, 0, Some("Running installer…"));
                let dir = payload.parent().unwrap_or(&dest).to_path_buf();
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
                emit(app, slug, "extract", 0, 0, Some("Extracting…"));
                extract_archive(&download_path, &dest)?;
                let _ = fs::remove_file(&download_path);
            }
            "iso" => {
                emit(app, slug, "install", 0, 0, Some("Running disc setup…"));
                launch::run_installer_from_iso(&download_path)?;
            }
            "exe" | "msi" => {
                emit(app, slug, "install", 0, 0, Some("Running installer…"));
                launch::run_and_wait(&download_path, &dest, false)?;
            }
            _ => {}
        }
    }

    let exe = find_exe(&dest, exe_hint.as_deref());

    let mut state = config::load_state(app);
    state.insert(
        slug.to_string(),
        InstallEntry {
            title,
            version,
            path: dest.to_string_lossy().to_string(),
            exe: exe.clone(),
            setup_type,
            hypervisor,
        },
    );
    config::save_state(app, &state)?;
    report_installed(server, cfg, &state);

    emit(app, slug, "done", total, total, None);
    Ok(InstallResult {
        slug: slug.to_string(),
        install_path: dest.to_string_lossy().to_string(),
        exe,
    })
}

pub fn extract_archive(archive: &Path, dest: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    // Windows ships bsdtar (tar.exe), which also extracts .zip.
    let status = std::process::Command::new("tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(dest)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("Could not run tar.exe (needs Windows 10 1803+): {e}"))?;
    if !status.success() {
        return Err("Extraction failed".into());
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
