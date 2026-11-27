//! Automatic updates from the GitHub releases: shortly after start, then
//! every 6 hours, AetherPlay checks `latest.json`; a newer, correctly signed
//! version is downloaded, installed and AetherPlay restarts. Never while a
//! game is running (checked again right before installing).

use crate::{gamepad, shell};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

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
    // Wait for the current game to end.
    while gamepad::is_running() {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(());
    };
    shell::log(app, &format!("mise à jour : {} disponible, téléchargement", update.version));
    let bytes = update.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    // A game may have been started during the download.
    while gamepad::is_running() {
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
    shell::log(app, &format!("mise à jour : installation de {}", update.version));
    update.install(bytes).map_err(|e| e.to_string())?;
    // Windows: the installer closes and relaunches AetherPlay itself.
    app.restart();
}
