mod cache;
mod config;
mod emulators;
mod install;
mod romm;
mod roms;
mod saves;
mod shell;

use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf};
use tauri::{AppHandle, Emitter, Manager};

fn client(app: &AppHandle) -> Result<romm::Client, String> {
    let cfg = config::load(app);
    if cfg.server_url.is_empty() {
        return Err("Serveur non configuré".into());
    }
    let pass = config::get_password(&cfg.username).ok_or("Mot de passe introuvable")?;
    romm::Client::new(&cfg.server_url, &cfg.username, &pass)
}

#[derive(Serialize)]
struct Settings {
    server_url: String,
    username: String,
    configured: bool,
    emulator_paths: HashMap<String, String>,
    platform_emulators: HashMap<String, String>,
    save_dirs: HashMap<String, String>,
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    let cfg = config::load(&app);
    Settings {
        configured: !cfg.server_url.is_empty(),
        server_url: cfg.server_url,
        username: cfg.username,
        emulator_paths: cfg.emulator_paths,
        platform_emulators: cfg.platform_emulators,
        save_dirs: cfg.save_dirs,
    }
}

/// Saves the RomM server address after checking it answers. Logging in then
/// happens in RomM's own form, which the proxy remembers.
#[tauri::command]
async fn set_server(app: AppHandle, server_url: String) -> Result<(), String> {
    let url = server_url.trim().trim_end_matches('/').to_string();
    let res = reqwest::get(format!("{url}/api/heartbeat")).await.map_err(|e| format!("Serveur injoignable : {e}"))?;
    if !res.status().is_success() {
        return Err(format!("Ce n'est pas un serveur RomM ({})", res.status()));
    }
    let mut cfg = config::load(&app);
    cfg.server_url = url;
    config::save(&app, &cfg)
}

#[tauri::command]
fn catalog() -> &'static [emulators::Emulator] {
    emulators::catalog()
}

#[tauri::command]
fn set_emulator_path(app: AppHandle, id: String, path: Option<String>) -> Result<(), String> {
    let mut cfg = config::load(&app);
    match path {
        Some(p) => cfg.emulator_paths.insert(id, p),
        None => cfg.emulator_paths.remove(&id),
    };
    config::save(&app, &cfg)
}

#[tauri::command]
fn set_platform_emulator(app: AppHandle, platform: String, id: Option<String>) -> Result<(), String> {
    let mut cfg = config::load(&app);
    match id {
        Some(id) => cfg.platform_emulators.insert(platform, id),
        None => cfg.platform_emulators.remove(&platform),
    };
    config::save(&app, &cfg)
}

#[tauri::command]
fn set_save_dir(app: AppHandle, id: String, path: Option<String>) -> Result<(), String> {
    let mut cfg = config::load(&app);
    match path {
        Some(p) => cfg.save_dirs.insert(id, p),
        None => cfg.save_dirs.remove(&id),
    };
    config::save(&app, &cfg)
}

/// Save folders in use per emulator (override or default), for display.
#[tauri::command]
fn save_dirs(app: AppHandle) -> Result<HashMap<String, Vec<String>>, String> {
    let cfg = config::load(&app);
    let vars = path_vars(&app)?;
    Ok(emulators::catalog()
        .iter()
        .map(|e| {
            let exe = cfg.emulator_paths.get(&e.id).map(String::as_str).unwrap_or("");
            let dirs = e.save_dirs(exe, None, &vars, &cfg.save_dirs);
            (e.id.clone(), dirs.iter().map(|d| d.to_string_lossy().into_owned()).collect())
        })
        .collect())
}

fn cache_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("roms"))
}

#[tauri::command]
fn cache_list(app: AppHandle) -> Result<Vec<cache::Entry>, String> {
    Ok(cache::list(&cache_root(&app)?))
}

#[tauri::command]
fn cache_delete(app: AppHandle, path: String) -> Result<(), String> {
    cache::delete(&cache_root(&app)?, &path)
}

#[tauri::command]
fn cache_clear(app: AppHandle) -> Result<(), String> {
    cache::clear(&cache_root(&app)?)
}

#[tauri::command]
fn installable() -> Vec<String> {
    install::installable()
}

#[derive(Serialize, Clone)]
struct InstallProgress {
    id: String,
    done: u64,
    total: u64,
}

/// Downloads and installs an emulator, then registers its path.
#[tauri::command]
async fn install_emulator(app: AppHandle, id: String) -> Result<String, String> {
    let root = app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("emulators");
    tokio::fs::create_dir_all(&root).await.map_err(|e| e.to_string())?;
    let path = install::install(&id, &root, |done, total| {
        let _ = app.emit("install-progress", InstallProgress { id: id.clone(), done, total });
    })
    .await?;
    let path = path.to_string_lossy().into_owned();
    let mut cfg = config::load(&app);
    cfg.emulator_paths.insert(id, path.clone());
    config::save(&app, &cfg)?;
    Ok(path)
}


fn rom_path(app: &AppHandle, rom: &romm::Rom) -> Result<PathBuf, String> {
    let dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("roms").join(&rom.platform_slug).join(&rom.fs_name))
}

fn path_vars(app: &AppHandle) -> Result<emulators::PathVars, String> {
    let p = app.path();
    let e = |e: tauri::Error| e.to_string();
    Ok(emulators::PathVars {
        home: p.home_dir().map_err(e)?,
        documents: p.document_dir().map_err(e)?,
        appdata: p.data_dir().map_err(e)?,
    })
}

/// Emits a `LaunchState` (RomM desktop bridge contract) for `rom_id`.
fn launch_state(app: &AppHandle, rom_id: u64, mut state: Value) {
    state["romId"] = json!(rom_id);
    let _ = app.emit("launch-state", state);
}

/// Rejection of `rommNative.launch`: `{ code, message }` as the contract names them.
#[derive(Serialize)]
struct LaunchError {
    code: &'static str,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchResult {
    rom_id: u64,
    emulator: String,
}

/// Downloads the ROM if needed, restores its save, then starts the matching
/// emulator; uploads the save once it exits. Progress goes out as `launch-state`.
async fn launch(app: AppHandle, rom_id: u64) -> Result<LaunchResult, LaunchError> {
    let fail = |code: &'static str, message: String| {
        launch_state(&app, rom_id, json!({ "status": "failed", "error": { "code": code, "message": message } }));
        LaunchError { code, message }
    };
    let client = client(&app).map_err(|e| fail("session-expired", e))?;
    let rom = client.rom(rom_id).await.map_err(|e| fail("invalid-request", e))?;
    let cfg = config::load(&app);
    let (emu, exe) = emulators::resolve(&rom.platform_slug, &cfg.emulator_paths, &cfg.platform_emulators)
        .ok_or_else(|| fail("no-emulator-configured", format!("Aucun émulateur configuré pour « {} »", rom.platform_slug)))?;

    // The ROM file, or for multi-file ROMs the folder they are extracted into.
    let dest = rom_path(&app, &rom).map_err(|e| fail("download-failed", e))?;
    let cached = if rom.multi {
        dest.join(roms::MARKER).exists()
    } else {
        std::fs::metadata(&dest).is_ok_and(|m| rom.fs_size_bytes == 0 || m.len() == rom.fs_size_bytes)
    };
    if !cached {
        // Multi-file ROMs arrive as a zip, extracted below.
        let download_to =
            if rom.multi { dest.with_file_name(format!("{}.download.zip", rom.fs_name)) } else { dest.clone() };
        let mut last = 0u64;
        client
            .download(&rom, &download_to, |done, total| {
                // Throttle events to roughly every 1 MiB.
                if done - last >= 1 << 20 || done == total {
                    last = done;
                    let progress = if total > 0 { done as f64 / total as f64 } else { 0.0 };
                    launch_state(&app, rom_id, json!({ "status": "downloading", "stage": "rom",
                        "progress": progress, "received": done, "total": total }));
                }
            })
            .await
            .map_err(|e| fail("download-failed", e))?;
        if rom.multi {
            let (zip, dir) = (download_to.clone(), dest.clone());
            let res = tauri::async_runtime::spawn_blocking(move || roms::extract(&zip, &dir)).await;
            let _ = std::fs::remove_file(&download_to);
            res.map_err(|e| e.to_string())
                .and_then(|r| r)
                .map_err(|e| fail("download-failed", format!("Extraction de la ROM : {e}")))?;
        }
    }
    let target = if rom.multi { roms::launch_target(&dest, &rom.fs_name) } else { Ok(dest.clone()) }
        .map_err(|e| fail("launch-failed", e))?;

    let vars = path_vars(&app).map_err(|e| fail("launch-failed", e))?;
    // Switch: let Eden/Eglantine pick up updates/DLC shipped next to the base game.
    if rom.multi && let Some(ini) = emu.config_ini(exe, &vars) {
        emulators::register_external_content(&ini, &dest)
            .map_err(|e| fail("launch-failed", format!("Configuration des mises à jour/DLC : {e}")))?;
    }
    let save_dirs = emu.save_dirs(exe, Some(&target), &vars, &cfg.save_dirs);
    let data_dir = app.path().app_local_data_dir().map_err(|e| fail("launch-failed", e.to_string()))?;
    let mut index = saves::Index::load(data_dir.join("saves-index.json"));
    if !save_dirs.is_empty() {
        launch_state(&app, rom_id, json!({ "status": "downloading", "stage": "save" }));
        // A failed restore must not block playing; the page is told.
        let action = match saves::restore(&client, &rom, &emu.id, &save_dirs, &mut index).await {
            Ok(()) => json!({ "action": "downloaded", "slot": saves::SLOT }),
            Err(e) => json!({ "action": "failed", "slot": saves::SLOT, "detail": e }),
        };
        launch_state(&app, rom_id, json!({ "status": "sync", "sync": action }));
    }

    if let Some(core) = emu.core_file(exe, &rom.platform_slug) {
        let name = core.file_stem().map(|s| s.to_string_lossy().into_owned());
        launch_state(&app, rom_id, json!({ "status": "downloading", "stage": "core", "core": name }));
        install::ensure_core(&core).await.map_err(|e| fail("download-failed", e))?;
    }

    let started = std::time::SystemTime::now();
    let mut child = emulators::command(emu, exe, &rom.platform_slug, &target)
        .and_then(|mut c| c.spawn().map_err(|e| format!("Impossible de lancer {} : {e}", emu.name)))
        .map_err(|e| fail("launch-failed", e))?;
    launch_state(&app, rom_id, json!({ "status": "running" }));

    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let code = child.wait().await.ok().and_then(|s| s.code());
        launch_state(&app2, rom_id, json!({ "status": "exited", "exitCode": code }));
        if save_dirs.is_empty() {
            return;
        }
        let core = emu.core(&rom.platform_slug);
        let action = match saves::upload(&client, &rom, &emu.id, core, &save_dirs, &dest, started, &mut index).await {
            Ok(true) => Some(json!({ "action": "uploaded", "slot": saves::SLOT })),
            Ok(false) => None,
            Err(e) => Some(json!({ "action": "failed", "slot": saves::SLOT, "detail": e })),
        };
        if let Some(sync) = action {
            launch_state(&app2, rom_id, json!({ "status": "sync", "sync": sync }));
        }
    });
    Ok(LaunchResult { rom_id, emulator: emu.name.clone() })
}

/// `rommNative.launch`. The request also carries the download path and cores,
/// but the shell resolves the ROM and emulator itself from the ROM id.
#[tauri::command]
async fn native_launch(app: AppHandle, request: Value) -> Result<LaunchResult, LaunchError> {
    let rom_id = request["romId"].as_u64().ok_or(LaunchError { code: "invalid-request", message: "romId manquant".into() })?;
    launch(app, rom_id).await
}

#[tauri::command]
fn native_cancel(_rom_id: u64) {}

/// `rommNative.getPlatformSupportAll`: platform slug -> PlatformSupport.
#[tauri::command]
fn native_platform_support(app: AppHandle, queries: Vec<Value>) -> HashMap<String, Value> {
    let cfg = config::load(&app);
    queries
        .iter()
        .filter_map(|q| q["platformSlug"].as_str())
        .map(|slug| {
            let answer = match emulators::resolve(slug, &cfg.emulator_paths, &cfg.platform_emulators) {
                Some((emu, _)) => json!({ "supported": true, "emulator": emu.name }),
                None if emulators::catalog().iter().any(|e| e.platforms.iter().any(|p| p == slug)) => {
                    json!({ "supported": false, "reason": "no-emulator-configured" })
                }
                None => json!({ "supported": false, "reason": "unsupported-platform" }),
            };
            (slug.to_string(), answer)
        })
        .collect()
}

/// `rommNative.openSettings`: the app's own settings in a separate window.
#[tauri::command]
fn open_settings(app: AppHandle) -> Result<(), String> {
    shell::open_settings(&app).map_err(|e| e.to_string())
}

/// Shows the RomM interface in the main window (after login / first-run setup).
#[tauri::command]
fn open_romm(app: AppHandle) -> Result<(), String> {
    shell::open_romm(&app).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // `romm://` serves the bundled RomM interface and relays its API calls.
        .register_asynchronous_uri_scheme_protocol("romm", |ctx, req, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move { responder.respond(shell::handle(&app, req).await) });
        })
        .setup(|app| {
            shell::create_main_window(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_server,
            catalog,
            set_emulator_path,
            installable,
            set_platform_emulator,
            set_save_dir,
            save_dirs,
            cache_list,
            cache_delete,
            cache_clear,
            install_emulator,
            native_launch,
            native_cancel,
            native_platform_support,
            open_settings,
            open_romm
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
