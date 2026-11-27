mod bt_phone;
mod cache;
mod config;
mod controller;
mod dualscreen;
mod emulators;
mod gamepad;
mod import;
mod install;
mod phone;
mod players;
mod romm;
mod roms;
mod saves;
mod shell;
mod updater;

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
    fullscreen: bool,
    quit_combo: bool,
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
        fullscreen: cfg.fullscreen,
        quit_combo: cfg.quit_combo,
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
    let home = p.home_dir().map_err(e)?;
    Ok(emulators::PathVars {
        documents: p.document_dir().unwrap_or_else(|_| home.join("Documents")),
        appdata: p.data_dir().map_err(e)?,
        config: p.config_dir().map_err(e)?,
        home,
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
async fn launch(app: AppHandle, rom_id: u64, fullscreen: bool) -> Result<LaunchResult, LaunchError> {
    let fail = |code: &'static str, message: String| {
        shell::log(&app, &format!("launch rom={rom_id}: échec {code} : {message}"));
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
    // RetroTogether: virtual controllers recreated in player order (J1 = port 1).
    let _ = tauri::async_runtime::spawn_blocking(phone::order_players).await;

    // Controllers as SDL sees them: Android controllers over Bluetooth get an
    // SDL mapping (passed to the emulator), and Eden/Eglantine, which start
    // keyboard-only, get player 1 pointed at the connected pad.
    let retroarch = cfg.emulator_paths.get("retroarch").map(PathBuf::from);
    let mut phone_mappings = Vec::new();
    match controller::find_sdl(std::path::Path::new(exe), retroarch.as_deref()) {
        Some(lib) => {
            let ini = emu.config_ini(exe, &vars);
            let (emu_id, exe_path) = (emu.id.clone(), PathBuf::from(exe));
            let save_dir = emu.save_dirs(exe, Some(&target), &vars, &cfg.save_dirs).into_iter().next();
            let res = tauri::async_runtime::spawn_blocking(move || {
                let found = controller::detect(&lib)?;
                // Bluetooth phones get player numbers in the emulator's order.
                bt_phone::order(found.pads.iter().map(|p| p.path.clone()).collect());
                // RetroTogether: each controller becomes a player in the emulator.
                let players = match (emu_id.as_str(), &ini, &save_dir) {
                    (_, Some(ini), _) => controller::apply(ini, &found.pads)?,
                    ("pcsx2", _, Some(dir)) => Some(players::pcsx2(dir, &found.pads)?.join(", ")),
                    ("duckstation", _, Some(dir)) => Some(players::duckstation(dir, &found.pads)?.join(", ")),
                    ("dolphin", _, Some(dir)) => Some(players::dolphin(dir, &found.pads)?.join(", ")),
                    ("retroarch", _, _) => {
                        players::retroarch(&exe_path, &found.phones)?;
                        None
                    }
                    _ => None,
                }
                .filter(|s| !s.is_empty());
                Ok::<_, String>((found.phone_mappings, players))
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
            match res {
                Ok((mappings, applied)) => {
                    if !mappings.is_empty() {
                        shell::log(&app, "manette : manette Android (Bluetooth) détectée");
                    }
                    if let Some(name) = applied {
                        shell::log(&app, &format!("manette : {name}"));
                    }
                    phone_mappings = mappings;
                }
                Err(e) => shell::log(&app, &format!("manette : {e}")),
            }
        }
        None => shell::log(&app, "manette : SDL3 introuvable, configuration inchangée"),
    }
    let save_dirs = emu.save_dirs(exe, Some(&target), &vars, &cfg.save_dirs);
    shell::log(&app, &format!("launch rom={rom_id} ({}) : {} via {}, saves {:?}", rom.fs_name, rom.platform_slug, emu.name, save_dirs));
    let data_dir = app.path().app_local_data_dir().map_err(|e| fail("launch-failed", e.to_string()))?;
    let mut index = saves::Index::load(data_dir.join("saves-index.json"));
    // RetroArch keeps one `<rom stem>.srm` per game: RomM's other saves fit.
    let single_file_as = (!emu.cores.is_empty())
        .then(|| target.file_stem().map(|s| format!("{}.srm", s.to_string_lossy())))
        .flatten();
    let mut deferred = false;
    // Local files replaced by RomM's copy are kept here first.
    let backup = data_dir
        .join("save-backups")
        .join(format!("{rom_id}-{}", emu.id))
        .join(chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string());
    // A previous upload failed: the local save is newer than RomM's. Send it
    // first; while that fails, keep it (never replace it with RomM's copy).
    let mut keep_local = false;
    if !save_dirs.is_empty() && index.pending(rom.id, &emu.id) {
        let core = emu.core(&rom.platform_slug);
        match saves::upload(&client, &rom, &emu.id, core, &save_dirs, &dest, None, &mut index).await {
            Ok(_) => {
                let _ = index.set_pending(rom.id, &emu.id, false);
                shell::log(&app, &format!("sync rom={rom_id}: envoi en attente réussi"));
            }
            Err(e) => {
                keep_local = true;
                shell::log(&app, &format!("sync rom={rom_id}: envoi en attente toujours impossible ({e}) : copie locale gardée"));
            }
        }
    }
    if !save_dirs.is_empty() && !keep_local {
        launch_state(&app, rom_id, json!({ "status": "downloading", "stage": "save" }));
        // A failed restore must not block playing; the page is told.
        let action = match saves::restore(&client, &rom, &emu.id, &save_dirs, single_file_as.as_deref(), &backup, &mut index).await {
            Ok(restored) => {
                shell::log(&app, &format!("sync rom={rom_id}: restauration : {}", restored.message));
                deferred = restored.deferred;
                json!({ "action": "downloaded", "slot": saves::SLOT })
            }
            Err(e) => {
                shell::log(&app, &format!("sync rom={rom_id}: restauration échouée : {e}"));
                json!({ "action": "failed", "slot": saves::SLOT, "detail": e })
            }
        };
        launch_state(&app, rom_id, json!({ "status": "sync", "sync": action }));
    }

    if let Some(core) = emu.core_file(exe, &rom.platform_slug) {
        let name = core.file_stem().map(|s| s.to_string_lossy().into_owned());
        launch_state(&app, rom_id, json!({ "status": "downloading", "stage": "core", "core": name }));
        install::ensure_core(&core).await.map_err(|e| fail("download-failed", e))?;
    }

    // Second screen (DS bottom screen, Wii U GamePad) on the phone, Windows.
    let mut dual = false;
    if dualscreen::supported(&emu.id) {
        let on = phone::any_connected();
        let mut c = config::load(&app);
        let was_on = c.dual_screen.contains(&emu.id);
        if on || was_on {
            match dualscreen::prepare(&emu.id, std::path::Path::new(exe), on) {
                Ok(_) => {
                    c.dual_screen.retain(|e| e != &emu.id);
                    if on {
                        c.dual_screen.push(emu.id.clone());
                    }
                    let _ = config::save(&app, &c);
                    dual = on;
                }
                Err(e) => shell::log(&app, &format!("second écran : {e}")),
            }
        }
    }

    let started = std::time::SystemTime::now();
    let mut child = emulators::command(emu, exe, &rom.platform_slug, &target, fullscreen || cfg.fullscreen, save_dirs.first().map(PathBuf::as_path))
        .and_then(|mut c| {
            if !phone_mappings.is_empty() {
                let mut all = std::env::var("SDL_GAMECONTROLLERCONFIG").unwrap_or_default();
                for m in &phone_mappings {
                    all.push('\n');
                    all.push_str(m);
                }
                c.env("SDL_GAMECONTROLLERCONFIG", all.trim());
            }
            c.spawn().map_err(|e| format!("Impossible de lancer {} : {e}", emu.name))
        })
        .map_err(|e| fail("launch-failed", e))?;
    gamepad::set_running(child.id());
    launch_state(&app, rom_id, json!({ "status": "running" }));
    if dual && let Some(pid) = child.id() {
        shell::log(&app, "second écran : envoi vers le téléphone");
        dualscreen::start(&emu.id, pid);
    }
    // The Android controller switches to this console's layout.
    phone::game_started(&rom, client.cover(&rom).await);

    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let code = child.wait().await.ok().and_then(|s| s.code());
        gamepad::set_running(None);
        if dual {
            dualscreen::stop();
        }
        phone::game_ended();
        // Back to the library, ready for the next pick with the controller.
        if let Some(win) = app2.get_webview_window("main") {
            let _ = win.set_focus();
        }
        launch_state(&app2, rom_id, json!({ "status": "exited", "exitCode": code }));
        if save_dirs.is_empty() {
            return;
        }
        if deferred {
            // First run of the game on this machine: the emulator profile now
            // exists, so put RomM's save in place over the blank one, and do not
            // upload that blank session.
            let what = match saves::restore(&client, &rom, &emu.id, &save_dirs, single_file_as.as_deref(), &backup, &mut index).await {
                Ok(r) => r.message,
                Err(e) => format!("échec : {e}"),
            };
            shell::log(&app2, &format!("sync rom={rom_id}: restauration reportée : {what}"));
            launch_state(&app2, rom_id, json!({ "status": "sync", "sync": { "action": "downloaded", "slot": saves::SLOT } }));
            return;
        }
        let core = emu.core(&rom.platform_slug);
        let action = match saves::upload(&client, &rom, &emu.id, core, &save_dirs, &dest, Some(started), &mut index).await {
            Ok(true) => {
                let _ = index.set_pending(rom.id, &emu.id, false);
                shell::log(&app2, &format!("sync rom={rom_id}: sauvegarde envoyée sur RomM"));
                Some(json!({ "action": "uploaded", "slot": saves::SLOT }))
            }
            Ok(false) => {
                let dirs: Vec<String> = save_dirs.iter().map(|d| d.display().to_string()).collect();
                shell::log(&app2, &format!("sync rom={rom_id}: aucun fichier modifié pendant la partie dans {dirs:?}"));
                None
            }
            Err(e) => {
                // Retried before the next session; until then RomM's older copy
                // never replaces this one.
                let _ = index.set_pending(rom.id, &emu.id, true);
                shell::log(&app2, &format!("sync rom={rom_id}: envoi échoué, réessayé au prochain lancement : {e}"));
                Some(json!({ "action": "failed", "slot": saves::SLOT, "detail": e }))
            }
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
    launch(app, rom_id, request["fullscreen"].as_bool().unwrap_or(false)).await
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

/// Boolean app options shown on the AetherPlay settings page.
#[tauri::command]
fn set_option(app: AppHandle, name: String, value: bool) -> Result<(), String> {
    let mut cfg = config::load(&app);
    match name.as_str() {
        "fullscreen" => cfg.fullscreen = value,
        "quit_combo" => cfg.quit_combo = value,
        _ => return Err(format!("Option inconnue : {name}")),
    }
    config::save(&app, &cfg)
}

/// Native file/folder picker for pages that cannot use the dialog plugin's JS API.
#[tauri::command]
async fn pick_path(app: AppHandle, directory: bool, title: String) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let dialog = app.dialog().file().set_title(title);
        let picked = if directory { dialog.blocking_pick_folder() } else { dialog.blocking_pick_file() };
        picked.map(|p| p.to_string())
    })
    .await
    .ok()
    .flatten()
}

/// Imports a Switch save archive into Eden / Eglantine (see import.rs).
#[tauri::command]
async fn import_save(app: AppHandle, id: String, path: String) -> Result<import::ImportReport, String> {
    let emu = emulators::catalog().iter().find(|e| e.id == id).ok_or("Émulateur inconnu")?;
    if emu.config_ini.is_empty() {
        return Err(format!("L'import de sauvegarde n'est pas disponible pour {}", emu.name));
    }
    let cfg = config::load(&app);
    let exe = cfg.emulator_paths.get(&id).cloned().unwrap_or_default();
    let roots = emu.save_dirs(&exe, None, &path_vars(&app)?, &cfg.save_dirs);
    tauri::async_runtime::spawn_blocking(move || import::import_switch_save(std::path::Path::new(&path), &roots))
        .await
        .map_err(|e| e.to_string())?
}

/// Shows the diagnostic log (aetherplay.log) in the file manager.
#[tauri::command]
fn open_log(app: AppHandle) -> Result<(), String> {
    let file = app.path().app_log_dir().map_err(|e| e.to_string())?.join("aetherplay.log");
    if !file.exists() {
        return Err("Le journal est vide pour l'instant.".into());
    }
    tauri_plugin_opener::reveal_item_in_dir(file).map_err(|e| e.to_string())
}

/// Uploads from the interface (saves, states, covers…): webviews do not hand
/// file contents to the `romm://` handler (RomM received 0-byte files), so
/// bridge.js sends the encoded multipart body here, relayed like any request.
#[tauri::command]
async fn proxy_upload(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<serde_json::Value, String> {
    use base64::Engine;
    let tauri::ipc::InvokeBody::Raw(body) = request.body() else {
        return Err("corps de requête attendu".into());
    };
    let headers = request.headers();
    let get = |k: &str| headers.get(k).and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    let url = get("x-aep-url");
    if !url.starts_with("/api/") {
        return Err("adresse refusée".into());
    }
    let page_headers: std::collections::HashMap<String, String> =
        serde_json::from_str(&get("x-aep-headers")).unwrap_or_default();
    let mut req = tauri::http::Request::builder().method(get("x-aep-method").as_str()).uri(url.as_str());
    for (k, v) in &page_headers {
        if !k.eq_ignore_ascii_case("content-type") {
            req = req.header(k.as_str(), v.as_str());
        }
    }
    let content_type = get("x-aep-content-type");
    if !content_type.is_empty() {
        req = req.header("content-type", content_type);
    }
    let req = req.body(body.clone()).map_err(|e| e.to_string())?;
    shell::log(&app, &format!("envoi depuis l'interface : {url} ({} octets)", body.len()));
    let res = shell::handle(&app, req).await;
    let headers: serde_json::Map<String, serde_json::Value> = res
        .headers()
        .iter()
        .filter_map(|(k, v)| Some((k.to_string(), serde_json::Value::String(v.to_str().ok()?.to_string()))))
        .collect();
    Ok(serde_json::json!({
        "status": res.status().as_u16(),
        "headers": headers,
        "body": base64::engine::general_purpose::STANDARD.encode(res.body()),
    }))
}

/// The update to offer in the pop-up, if any.
#[tauri::command]
fn update_status() -> Option<serde_json::Value> {
    updater::status()
}

/// "Plus tard" in the update pop-up.
#[tauri::command]
fn update_later() {
    updater::postpone();
}

/// "Installer et redémarrer" in the update pop-up.
#[tauri::command]
async fn update_install(app: AppHandle) -> Result<(), String> {
    updater::install(&app).await
}

/// Size of the cover / screenshot cache (bytes).
#[tauri::command]
fn image_cache_size(app: AppHandle) -> u64 {
    shell::image_cache_dir(&app)
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Empties the cover / screenshot cache (they are downloaded again as needed).
#[tauri::command]
fn image_cache_clear(app: AppHandle) -> Result<(), String> {
    match shell::image_cache_dir(&app) {
        Some(dir) if dir.exists() => std::fs::remove_dir_all(dir).map_err(|e| e.to_string()),
        _ => Ok(()),
    }
}

/// Android controller: pairing code, port, connected phones.
#[tauri::command]
fn phone_status() -> serde_json::Value {
    phone::status()
}

/// Windows: installs the ViGEmBus driver (virtual controller for Wi-Fi input).
#[tauri::command]
async fn install_vigem(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        let dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("downloads");
        install::install_vigem(&dir).await?;
        shell::log(&app, "manette Android : pilote ViGEmBus installé");
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("Uniquement sur Windows".into())
    }
}

/// Forgets every paired phone (they will need the code again).
#[tauri::command]
fn phone_forget(app: AppHandle) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.phones.clear();
    config::save(&app, &cfg)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // `romm://` serves the bundled RomM interface and relays its API calls.
        .register_asynchronous_uri_scheme_protocol("romm", |ctx, req, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move { responder.respond(shell::handle(&app, req).await) });
        })
        .setup(|app| {
            shell::create_main_window(app.handle())?;
            gamepad::spawn_watcher(app.handle().clone());
            phone::start(app.handle().clone());
            updater::spawn(app.handle().clone());
            bt_phone::spawn(app.handle().clone());
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
            open_romm,
            set_option,
            pick_path,
            import_save,
            open_log,
            phone_status,
            phone_forget,
            image_cache_size,
            image_cache_clear,
            proxy_upload,
            update_status,
            update_later,
            update_install,
            install_vigem
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
