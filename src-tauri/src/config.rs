use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
use tauri::{AppHandle, Manager};

const KEYRING_SERVICE: &str = "romm-desktop";

/// Persisted settings (the password lives in the OS keychain, never in this file).
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server_url: String,
    pub username: String,
    /// Emulator id -> executable (or .app bundle on macOS).
    #[serde(default)]
    pub emulator_paths: HashMap<String, String>,
}

fn file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("config.json"))
}

pub fn load(app: &AppHandle) -> Config {
    file(app)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(file(app)?, json).map_err(|e| e.to_string())
}

fn entry(user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, user).map_err(|e| e.to_string())
}

pub fn set_password(user: &str, password: &str) -> Result<(), String> {
    entry(user)?.set_password(password).map_err(|e| e.to_string())
}

pub fn get_password(user: &str) -> Option<String> {
    entry(user).ok()?.get_password().ok()
}
