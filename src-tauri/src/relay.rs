//! The RomM server as relay with Android controllers that only have a
//! Bluetooth link to this computer (no shared network): the phone signs in
//! to RomM itself (client token, see phone_pairing_code) and reads there
//! what AetherPlay publishes.
//! * "Now playing" (RomM's activity heartbeat): the running game, repeated
//!   every minute during the game and cleared at the end; RomM shows it too.
//! * Player numbers: `ui_settings.aetherplay.players` of the user, keyed by
//!   the phone's Bluetooth name.

use crate::{config, romm::Client, shell};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::AppHandle;

/// Bumped when a game ends, to stop its heartbeat loop.
static SESSION: AtomicU64 = AtomicU64::new(0);

/// This computer's RomM device id (registered once, kept in config.json).
async fn device_id(app: &AppHandle, client: &Client) -> Result<String, String> {
    let mut cfg = config::load(app);
    if let Some(id) = cfg.romm_device_id.clone().filter(|s| !s.is_empty()) {
        return Ok(id);
    }
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let id = client.register_device(&format!("AetherPlay ({host})"), &host, crate::emulators::CURRENT_OS).await?;
    cfg.romm_device_id = Some(id.clone());
    config::save(app, &cfg)?;
    Ok(id)
}

/// A game started: "now playing" on RomM until `game_ended`.
pub fn game_started(app: AppHandle, client: Client, rom_id: u64) {
    let session = SESSION.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn(async move {
        let device = match device_id(&app, &client).await {
            Ok(d) => d,
            Err(e) => {
                shell::log(&app, &format!("relais RomM : appareil non enregistré ({e})"));
                return;
            }
        };
        let mut logged = false;
        while SESSION.load(Ordering::SeqCst) == session {
            match client.heartbeat(rom_id, &device).await {
                Ok(()) if !logged => {
                    shell::log(&app, "relais RomM : « en train de jouer » publié");
                    logged = true;
                }
                Ok(()) => {}
                Err(e) => shell::log(&app, &format!("relais RomM : {e}")),
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

pub fn game_ended(app: AppHandle, client: Client) {
    SESSION.fetch_add(1, Ordering::SeqCst);
    tauri::async_runtime::spawn(async move {
        if let Some(device) = config::load(&app).romm_device_id {
            let _ = client.clear_activity(&device).await;
        }
    });
}

/// Player numbers of the phones linked by Bluetooth, for the phones to read.
pub fn publish_players(app: AppHandle, client: Client, players: Vec<(String, u8)>) {
    if players.is_empty() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let result: Result<(), String> = async {
            let me = client.me().await?;
            let id = me["id"].as_u64().ok_or("utilisateur inconnu")?;
            // ui_settings holds RomM's own interface settings: keep them.
            let mut settings: Value = match &me["ui_settings"] {
                Value::Object(_) => me["ui_settings"].clone(),
                Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
                _ => json!({}),
            };
            let map: serde_json::Map<String, Value> = players.iter().map(|(n, p)| (n.clone(), json!(p))).collect();
            settings["aetherplay"] = json!({ "players": map, "at": chrono::Utc::now().to_rfc3339() });
            client.set_ui_settings(id, &settings).await
        }
        .await;
        match result {
            Ok(()) => shell::log(&app, &format!("relais RomM : joueurs publiés {players:?}")),
            Err(e) => shell::log(&app, &format!("relais RomM : joueurs non publiés ({e})")),
        }
    });
}
