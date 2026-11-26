mod cache;
mod config;
mod emulators;
mod install;
mod romm;
mod roms;
mod saves;

use serde::Serialize;
use std::{collections::HashMap, path::PathBuf};
use tauri::{AppHandle, Emitter, Manager, http};

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
    logged_in: bool,
    emulator_paths: HashMap<String, String>,
    platform_emulators: HashMap<String, String>,
    save_dirs: HashMap<String, String>,
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    let cfg = config::load(&app);
    Settings {
        logged_in: !cfg.server_url.is_empty() && config::get_password(&cfg.username).is_some(),
        server_url: cfg.server_url,
        username: cfg.username,
        emulator_paths: cfg.emulator_paths,
        platform_emulators: cfg.platform_emulators,
        save_dirs: cfg.save_dirs,
    }
}

#[tauri::command]
async fn login(app: AppHandle, server_url: String, username: String, password: String) -> Result<(), String> {
    romm::Client::new(&server_url, &username, &password)?.check_auth().await?;
    config::set_password(&username, &password)?;
    let mut cfg = config::load(&app);
    cfg.server_url = server_url.trim().trim_end_matches('/').to_string();
    cfg.username = username;
    config::save(&app, &cfg)
}

#[tauri::command]
async fn platforms(app: AppHandle) -> Result<Vec<romm::Platform>, String> {
    client(&app)?.platforms().await
}

#[tauri::command]
async fn roms(app: AppHandle, platform_id: u64) -> Result<Vec<romm::Rom>, String> {
    client(&app)?.roms(platform_id).await
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

#[derive(Serialize, Clone)]
struct Progress {
    rom_id: u64,
    done: u64,
    total: u64,
}

#[derive(Serialize, Clone)]
struct Exited {
    rom_id: u64,
    code: Option<i32>,
}

/// Downloads the ROM if needed, then starts the matching emulator.
#[tauri::command]
async fn play(app: AppHandle, rom_id: u64) -> Result<(), String> {
    let client = client(&app)?;
    let rom = client.rom(rom_id).await?;
    let cfg = config::load(&app);
    let (emu, exe) = emulators::resolve(&rom.platform_slug, &cfg.emulator_paths, &cfg.platform_emulators)
        .ok_or_else(|| format!("Aucun émulateur configuré pour « {} »", rom.platform_slug))?;

    // The ROM file, or for multi-file ROMs the folder they are extracted into.
    let dest = rom_path(&app, &rom)?;
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
                    let _ = app.emit("download-progress", Progress { rom_id, done, total });
                }
            })
            .await?;
        if rom.multi {
            let (zip, dir) = (download_to.clone(), dest.clone());
            let res = tauri::async_runtime::spawn_blocking(move || roms::extract(&zip, &dir)).await;
            let _ = std::fs::remove_file(&download_to);
            res.map_err(|e| e.to_string())?.map_err(|e| format!("Extraction de la ROM : {e}"))?;
        }
    }
    let target = if rom.multi { roms::launch_target(&dest, &rom.fs_name)? } else { dest.clone() };

    let vars = path_vars(&app)?;
    // Switch: let Eden/Eglantine pick up updates/DLC shipped next to the base game.
    if rom.multi && let Some(ini) = emu.config_ini(exe, &vars) {
        emulators::register_external_content(&ini, &dest)
            .map_err(|e| format!("Configuration des mises à jour/DLC : {e}"))?;
    }
    let save_dirs = emu.save_dirs(exe, Some(&target), &vars, &cfg.save_dirs);
    let mut index = saves::Index::load(app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("saves-index.json"));
    if !save_dirs.is_empty() {
        sync_event(&app, rom_id, "restoring", None);
        match saves::restore(&client, &rom, &emu.id, &save_dirs, &mut index).await {
            Ok(()) => sync_event(&app, rom_id, "restored", None),
            // A failed restore must not block playing; the user is told.
            Err(e) => sync_event(&app, rom_id, "error", Some(format!("Restauration de la sauvegarde : {e}"))),
        }
    }

    if let Some(core) = emu.core_file(exe, &rom.platform_slug) {
        install::ensure_core(&core).await?;
    }

    let started = std::time::SystemTime::now();
    let mut child = emulators::command(emu, exe, &rom.platform_slug, &target)?
        .spawn()
        .map_err(|e| format!("Impossible de lancer {} : {e}", emu.name))?;
    tauri::async_runtime::spawn(async move {
        let code = child.wait().await.ok().and_then(|s| s.code());
        let _ = app.emit("game-exited", Exited { rom_id, code });
        if save_dirs.is_empty() {
            return;
        }
        sync_event(&app, rom_id, "uploading", None);
        let core = emu.core(&rom.platform_slug);
        match saves::upload(&client, &rom, &emu.id, core, &save_dirs, &dest, started, &mut index).await {
            Ok(true) => sync_event(&app, rom_id, "uploaded", None),
            Ok(false) => sync_event(&app, rom_id, "unchanged", None),
            Err(e) => sync_event(&app, rom_id, "error", Some(format!("Envoi de la sauvegarde : {e}"))),
        }
    });
    Ok(())
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

#[derive(Serialize, Clone)]
struct SyncStatus {
    rom_id: u64,
    state: &'static str,
    message: Option<String>,
}

fn sync_event(app: &AppHandle, rom_id: u64, state: &'static str, message: Option<String>) {
    let _ = app.emit("save-sync", SyncStatus { rom_id, state, message });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // `romm://` proxies RomM resources (covers) with the user's credentials.
        .register_asynchronous_uri_scheme_protocol("romm", |ctx, req, responder| {
            let app = ctx.app_handle().clone();
            let path = req.uri().path_and_query().map(|p| p.as_str().to_string()).unwrap_or_default();
            tauri::async_runtime::spawn(async move {
                let res = match async { client(&app)?.resource(&path).await }.await {
                    Ok((ct, body)) => http::Response::builder()
                        .header(http::header::CONTENT_TYPE, ct)
                        .header("Cache-Control", "max-age=86400")
                        .body(body),
                    Err(e) => http::Response::builder().status(404).body(e.into_bytes()),
                };
                responder.respond(res.expect("valid response"));
            });
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            login,
            platforms,
            roms,
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
            play
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
