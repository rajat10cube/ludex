//! Persisted config (app data dir) + install state + OS-keyring credentials.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const KEYRING_SERVICE: &str = "ludex-desktop";

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub server: String,
    pub username: String,
    pub install_dir: String,
    pub device: String,
    #[serde(default)]
    pub is_admin: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct InstallEntry {
    pub title: String,
    pub version: Option<String>,
    pub path: String,
    pub exe: Option<String>,
    pub setup_type: String,
    pub hypervisor: bool,
}

pub type InstallState = HashMap<String, InstallEntry>;

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn load_config(app: &AppHandle) -> AppConfig {
    match data_dir(app) {
        Ok(d) => fs::read_to_string(d.join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

pub fn save_config(app: &AppHandle, cfg: &AppConfig) -> Result<(), String> {
    let path = data_dir(app)?.join("config.json");
    fs::write(path, serde_json::to_string_pretty(cfg).unwrap()).map_err(|e| e.to_string())
}

pub fn load_state(app: &AppHandle) -> InstallState {
    match data_dir(app) {
        Ok(d) => fs::read_to_string(d.join("state.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        Err(_) => InstallState::default(),
    }
}

pub fn save_state(app: &AppHandle, state: &InstallState) -> Result<(), String> {
    let path = data_dir(app)?.join("state.json");
    fs::write(path, serde_json::to_string_pretty(state).unwrap()).map_err(|e| e.to_string())
}

pub fn covers_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let d = data_dir(app)?.join("covers");
    fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

/// A download in progress or paused — persisted so it survives app restarts and
/// can be resumed from `bytes`.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct DownloadRecord {
    pub slug: String,
    pub title: String,
    pub download_kind: String, // "tar" | "file"
    pub download_name: String,
    pub setup_type: String,
    pub exe_hint: Option<String>,
    pub payload_path: Option<String>,
    pub hypervisor: bool,
    pub version: Option<String>,
    pub size_hint: u64,
    pub dest: String,
    pub temp_path: String,
    pub bytes: u64,
    pub paused: bool,
}

pub type Downloads = HashMap<String, DownloadRecord>;

pub fn load_downloads(app: &AppHandle) -> Downloads {
    match data_dir(app) {
        Ok(d) => fs::read_to_string(d.join("downloads.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        Err(_) => Downloads::default(),
    }
}

fn save_downloads(app: &AppHandle, dl: &Downloads) {
    if let Ok(d) = data_dir(app) {
        let _ = fs::write(d.join("downloads.json"), serde_json::to_string_pretty(dl).unwrap());
    }
}

pub fn save_download(app: &AppHandle, record: &DownloadRecord) {
    let mut dl = load_downloads(app);
    dl.insert(record.slug.clone(), record.clone());
    save_downloads(app, &dl);
}

pub fn remove_download(app: &AppHandle, slug: &str) {
    let mut dl = load_downloads(app);
    if dl.remove(slug).is_some() {
        save_downloads(app, &dl);
    }
}

// --- credentials (Windows Credential Manager via keyring) ---
pub fn set_password(account: &str, password: &str) -> Result<(), String> {
    keyring::Entry::new(KEYRING_SERVICE, account)
        .and_then(|e| e.set_password(password))
        .map_err(|e| e.to_string())
}

pub fn get_password(account: &str) -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, account)
        .ok()
        .and_then(|e| e.get_password().ok())
}

pub fn delete_password(account: &str) {
    if let Ok(e) = keyring::Entry::new(KEYRING_SERVICE, account) {
        let _ = e.delete_password();
    }
}
