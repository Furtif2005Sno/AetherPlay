//! Updates from the GitHub releases. Shortly after start, then every 6 hours,
//! AetherPlay checks `latest.json`; a newer, correctly signed version is
//! offered in a pop-up of the interface (bridge.js): nothing is downloaded or
//! installed until the user says so, and never during a game.

use crate::{gamepad, shell};
use serde_json::json;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};

/// The update found by the last check, waiting for the user's answer.
static AVAILABLE: Mutex<Option<Update>> = Mutex::new(None);
/// "Plus tard": not offered again until the next check.
static POSTPONED: Mutex<bool> = Mutex::new(false);

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            if let Err(e) = check(&app).await {
                shell::log(&app, &format!("mise à jour : {e}"));
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

async fn check(app: &AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(());
    };
    shell::log(app, &format!("mise à jour : {} disponible", update.version));
    *AVAILABLE.lock().expect("update lock") = Some(update);
    *POSTPONED.lock().expect("update lock") = false;
    // Offered once the current game is over.
    while gamepad::is_running() {
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
    if let Some(offer) = status() {
        let _ = app.emit("aep-update", offer);
    }
    Ok(())
}

/// The update to offer now, if any (the page asks again after a reload).
pub fn status() -> Option<serde_json::Value> {
    if *POSTPONED.lock().expect("update lock") || gamepad::is_running() {
        return None;
    }
    let available = AVAILABLE.lock().expect("update lock");
    let update = available.as_ref()?;
    Some(json!({ "version": update.version, "current": update.current_version, "notes": update.body }))
}

pub fn postpone() {
    *POSTPONED.lock().expect("update lock") = true;
}

/// Downloads and installs the offered update ("aep-update-progress" events),
/// then restarts AetherPlay.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let update = AVAILABLE.lock().expect("update lock").clone().ok_or("Aucune mise à jour à installer")?;
    shell::log(app, &format!("mise à jour : installation de {}", update.version));
    let mut done = 0u64;
    let progress_app = app.clone();
    let bytes = update
        .download(
            move |chunk, total| {
                done += chunk as u64;
                let _ = progress_app.emit("aep-update-progress", json!({ "done": done, "total": total }));
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    update.install(bytes).map_err(|e| e.to_string())?;
    // Windows: the installer closes and relaunches AetherPlay itself.
    app.restart();
}
