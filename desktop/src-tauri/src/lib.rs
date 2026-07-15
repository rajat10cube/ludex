//! Ludex desktop — Tauri command layer.

mod config;
mod install;
mod launch;
mod server;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use config::AppConfig;
use server::Server;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Session {
    server: String,
    username: String,
    install_dir: String,
    connected: bool,
    is_admin: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlayResult {
    slug: String,
    seconds: u64,
    launched: bool,
}

fn account(user: &str, server: &str) -> String {
    format!("{user}@{server}")
}

fn default_install_dir() -> String {
    let base = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into());
    format!("{base}\\Games\\Ludex")
}

/// Build a Server from stored config + keyring password.
fn build_server(app: &AppHandle) -> Result<(Server, AppConfig), String> {
    let cfg = config::load_config(app);
    if cfg.server.is_empty() || cfg.username.is_empty() {
        return Err("Not connected".into());
    }
    let pass = config::get_password(&account(&cfg.username, &cfg.server))
        .ok_or("Saved login not found — please reconnect")?;
    let srv = Server::new(&cfg.server, &cfg.username, &pass);
    Ok((srv, cfg))
}

/// Run blocking (network / filesystem / process) work off the UI thread, so a
/// long download or a running game never freezes the window.
async fn blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn connect(
    app: AppHandle,
    server: String,
    username: String,
    password: String,
) -> Result<Session, String> {
    blocking(move || {
        let base = server.trim_end_matches('/').to_string();
        let srv = Server::new(&base, &username, &password);
        let me = srv.me()?; // verify credentials before saving anything

        let mut cfg = config::load_config(&app);
        cfg.server = base.clone();
        cfg.username = username.clone();
        cfg.is_admin = me["isAdmin"].as_bool().unwrap_or(false);
        if cfg.install_dir.is_empty() {
            cfg.install_dir = default_install_dir();
        }
        if cfg.device.is_empty() {
            cfg.device = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "windows-pc".into());
        }
        let _ = std::fs::create_dir_all(&cfg.install_dir);
        config::set_password(&account(&username, &base), &password)?;
        config::save_config(&app, &cfg)?;
        srv.hello(&cfg.device);

        Ok(Session {
            server: cfg.server,
            username: cfg.username,
            install_dir: cfg.install_dir,
            connected: true,
            is_admin: cfg.is_admin,
        })
    })
    .await
}

#[tauri::command]
async fn get_session(app: AppHandle) -> Result<Option<Session>, String> {
    blocking(move || {
        let mut cfg = config::load_config(&app);
        if cfg.server.is_empty() || cfg.username.is_empty() {
            return Ok(None);
        }
        let connected = config::get_password(&account(&cfg.username, &cfg.server)).is_some();

        // Re-check admin rights against the server (they may have changed, and
        // configs written by older builds don't have the flag at all). If the
        // server is unreachable, fall back to whatever we last saw.
        if connected {
            if let Ok((srv, _)) = build_server(&app) {
                if let Ok(me) = srv.me() {
                    let is_admin = me["isAdmin"].as_bool().unwrap_or(false);
                    if is_admin != cfg.is_admin {
                        cfg.is_admin = is_admin;
                        let _ = config::save_config(&app, &cfg);
                    }
                }
            }
        }

        Ok(Some(Session {
            server: cfg.server,
            username: cfg.username,
            install_dir: cfg.install_dir,
            connected,
            is_admin: cfg.is_admin,
        }))
    })
    .await
}

#[tauri::command]
fn disconnect(app: AppHandle) -> Result<(), String> {
    let cfg = config::load_config(&app);
    if !cfg.username.is_empty() {
        config::delete_password(&account(&cfg.username, &cfg.server));
    }
    Ok(())
}

#[tauri::command]
fn set_install_dir(app: AppHandle, path: String) -> Result<Session, String> {
    let mut cfg = config::load_config(&app);
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    cfg.install_dir = path;
    config::save_config(&app, &cfg)?;
    let connected = config::get_password(&account(&cfg.username, &cfg.server)).is_some();
    Ok(Session {
        server: cfg.server,
        username: cfg.username,
        install_dir: cfg.install_dir,
        connected,
        is_admin: cfg.is_admin,
    })
}

#[tauri::command]
fn pick_install_dir(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().blocking_pick_folder();
    Ok(picked.map(|p| p.to_string()))
}

#[tauri::command]
async fn list_games(app: AppHandle) -> Result<Value, String> {
    blocking(move || {
        let (srv, _) = build_server(&app)?;
        let mut data = srv.games()?;
        let state = config::load_state(&app);
        if let Some(arr) = data["games"].as_array_mut() {
            for g in arr.iter_mut() {
                if let Some(slug) = g["slug"].as_str().map(|s| s.to_string()) {
                    g["installed"] = Value::Bool(state.contains_key(&slug));
                }
            }
        }
        Ok(data)
    })
    .await
}

#[tauri::command]
async fn game_detail(app: AppHandle, slug: String) -> Result<Value, String> {
    blocking(move || {
        let (srv, _) = build_server(&app)?;
        let mut g = srv.game(&slug)?;
        let state = config::load_state(&app);
        match state.get(&slug) {
            Some(entry) => {
                g["installed"] = Value::Bool(true);
                // a local post-install instruction (e.g. staged crack to copy)
                g["installNote"] = entry.note.clone().map(Value::String).unwrap_or(Value::Null);
            }
            None => g["installed"] = Value::Bool(false),
        }
        Ok(g)
    })
    .await
}

#[tauri::command]
async fn cover_data_url(app: AppHandle, slug: String) -> Result<Option<String>, String> {
    blocking(move || {
        let dir = config::covers_dir(&app)?;
        // serve from disk cache if present
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.file_stem().and_then(|s| s.to_str()) == Some(slug.as_str()) {
                    if let Ok(bytes) = std::fs::read(&p) {
                        let mime =
                            mime_from_ext(p.extension().and_then(|e| e.to_str()).unwrap_or("jpg"));
                        return Ok(Some(data_url(&mime, &bytes)));
                    }
                }
            }
        }
        let (srv, _) = build_server(&app)?;
        match srv.cover(&slug)? {
            None => Ok(None),
            Some((bytes, ct)) => {
                let _ = std::fs::write(dir.join(format!("{slug}.{}", ext_from_mime(&ct))), &bytes);
                Ok(Some(data_url(&ct, &bytes)))
            }
        }
    })
    .await
}

// --- server library management (admin) ---

#[tauri::command]
async fn list_libraries(app: AppHandle) -> Result<Value, String> {
    blocking(move || build_server(&app)?.0.libraries()).await
}

#[tauri::command]
async fn add_library(app: AppHandle, path: String, name: Option<String>) -> Result<Value, String> {
    blocking(move || build_server(&app)?.0.add_library(&path, name)).await
}

#[tauri::command]
async fn remove_library(app: AppHandle, id: i64) -> Result<(), String> {
    blocking(move || build_server(&app)?.0.delete_library(id)).await
}

/// Kick off a rescan of every library folder on the server.
#[tauri::command]
async fn scan_libraries(app: AppHandle) -> Result<Value, String> {
    blocking(move || build_server(&app)?.0.scan()).await
}

#[tauri::command]
async fn scan_status(app: AppHandle) -> Result<Value, String> {
    blocking(move || build_server(&app)?.0.scan_status()).await
}

/// Browse folders on the *server* (the container sees /mnt/games, not D:\).
#[tauri::command]
async fn browse_server(app: AppHandle, path: String) -> Result<Value, String> {
    blocking(move || build_server(&app)?.0.browse(&path)).await
}

#[tauri::command]
async fn artwork_settings(app: AppHandle) -> Result<Value, String> {
    blocking(move || build_server(&app)?.0.artwork_settings()).await
}

#[tauri::command]
async fn save_artwork_settings(app: AppHandle, body: Value) -> Result<Value, String> {
    blocking(move || build_server(&app)?.0.save_artwork_settings(body)).await
}

#[tauri::command]
async fn refresh_artwork(app: AppHandle) -> Result<Value, String> {
    blocking(move || {
        let (srv, _) = build_server(&app)?;
        let res = srv.refresh_artwork()?;
        // covers are cached on disk by slug — drop them so new art is picked up
        if let Ok(dir) = config::covers_dir(&app) {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(res)
    })
    .await
}

async fn start_install(
    app: AppHandle,
    slug: String,
    resume: bool,
) -> Result<install::InstallStatus, String> {
    // Register the control up-front so pause/cancel can target it immediately.
    let control = app.state::<install::InstallManager>().register(&slug);
    let app2 = app.clone();
    let slug2 = slug.clone();
    let res = blocking(move || {
        let (srv, cfg) = build_server(&app2)?;
        install::run(&app2, &srv, &cfg, &slug2, resume, control)
    })
    .await;
    app.state::<install::InstallManager>().unregister(&slug);
    res
}

#[tauri::command]
async fn install(app: AppHandle, slug: String) -> Result<install::InstallStatus, String> {
    start_install(app, slug, false).await
}

#[tauri::command]
async fn resume_install(app: AppHandle, slug: String) -> Result<install::InstallStatus, String> {
    start_install(app, slug, true).await
}

#[tauri::command]
fn pause_install(app: AppHandle, slug: String) -> Result<(), String> {
    app.state::<install::InstallManager>().signal(&slug, install::PAUSE);
    Ok(())
}

#[tauri::command]
fn cancel_install(app: AppHandle, slug: String) -> Result<(), String> {
    let signalled = app.state::<install::InstallManager>().signal(&slug, install::CANCEL);
    if !signalled {
        // not actively downloading (paused / interrupted) — clean up on disk
        install::discard_paused(&app, &slug);
    }
    Ok(())
}

#[tauri::command]
fn paused_downloads(app: AppHandle) -> Result<Vec<install::DownloadRecordView>, String> {
    Ok(install::list_paused(&app))
}

#[tauri::command]
async fn play(app: AppHandle, slug: String) -> Result<PlayResult, String> {
    blocking(move || {
        let (srv, cfg) = build_server(&app)?;
        let state = config::load_state(&app);
        let entry = state.get(&slug).ok_or("This game isn't installed")?;

        let exe = entry
            .exe
            .clone()
            .filter(|e| std::path::Path::new(e).exists())
            .or_else(|| install::find_exe(std::path::Path::new(&entry.path), None));
        let exe = match exe {
            Some(e) => e,
            None => {
                if entry.setup_type == "iso" || entry.setup_type == "installer" {
                    return Err(
                        "This game was installed by its own setup — launch it from the Start Menu."
                            .into(),
                    );
                }
                return Err("Couldn't find an executable to launch.".into());
            }
        };

        let exe_path = std::path::Path::new(&exe);
        let workdir = exe_path
            .parent()
            .unwrap_or(std::path::Path::new(&entry.path))
            .to_path_buf();
        let seconds = launch::run_and_wait(exe_path, &workdir, entry.hypervisor)?;
        if seconds > 5 {
            srv.report_session(&cfg.device, &slug, seconds);
        }
        Ok(PlayResult { slug, seconds, launched: true })
    })
    .await
}

#[tauri::command]
async fn uninstall(app: AppHandle, slug: String) -> Result<(), String> {
    blocking(move || {
        let (srv, cfg) = build_server(&app)?;
        let mut state = config::load_state(&app);
        if let Some(entry) = state.remove(&slug) {
            let _ = std::fs::remove_dir_all(&entry.path);
            config::save_state(&app, &state)?;
            install::report_installed(&srv, &cfg, &state);
        }
        Ok(())
    })
    .await
}

#[tauri::command]
fn open_install_dir(app: AppHandle, slug: String) -> Result<(), String> {
    let state = config::load_state(&app);
    if let Some(entry) = state.get(&slug) {
        launch::open_folder(std::path::Path::new(&entry.path));
    }
    Ok(())
}

/// Open the staged-crack folder for a game (the files to copy over the install).
#[tauri::command]
fn open_crack_dir(app: AppHandle, slug: String) -> Result<(), String> {
    let state = config::load_state(&app);
    let entry = state.get(&slug).ok_or("This game isn't installed")?;
    let crack = std::path::Path::new(&entry.path).join("_Ludex-Crack");
    if !crack.is_dir() {
        return Err("No crack folder was staged for this game.".into());
    }
    launch::open_folder(&crack);
    Ok(())
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

fn ext_from_mime(m: &str) -> &'static str {
    if m.contains("png") {
        "png"
    } else if m.contains("webp") {
        "webp"
    } else {
        "jpg"
    }
}

fn mime_from_ext(e: &str) -> String {
    match e {
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/jpeg",
    }
    .to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(install::InstallManager::default())
        .invoke_handler(tauri::generate_handler![
            connect,
            get_session,
            disconnect,
            set_install_dir,
            pick_install_dir,
            list_games,
            game_detail,
            cover_data_url,
            list_libraries,
            add_library,
            remove_library,
            scan_libraries,
            scan_status,
            browse_server,
            artwork_settings,
            save_artwork_settings,
            refresh_artwork,
            install,
            resume_install,
            pause_install,
            cancel_install,
            paused_downloads,
            play,
            uninstall,
            open_install_dir,
            open_crack_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running Ludex");
}
