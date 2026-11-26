use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
use tauri::{AppHandle, Manager};

const KEYRING_SERVICE: &str = "aetherplay";

/// Persisted settings (the password lives in the OS keychain, never in this file).
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub server_url: String,
    pub username: String,
    /// Emulator id -> executable (or .app bundle on macOS).
    #[serde(default)]
    pub emulator_paths: HashMap<String, String>,
    /// Platform slug -> preferred emulator id (otherwise catalog order).
    #[serde(default)]
    pub platform_emulators: HashMap<String, String>,
    /// Emulator id -> save folder overriding the catalog default.
    #[serde(default)]
    pub save_dirs: HashMap<String, String>,
    /// Start emulators fullscreen.
    #[serde(default)]
    pub fullscreen: bool,
    /// Holding Select + Start on a controller quits the running game.
    #[serde(default = "default_true")]
    pub quit_combo: bool,
    /// Paired Android controllers: token -> phone name.
    #[serde(default)]
    pub phones: HashMap<String, String>,
    /// Emulators whose second-screen window AetherPlay turned on (to undo it).
    #[serde(default)]
    pub dual_screen: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            username: String::new(),
            emulator_paths: HashMap::new(),
            platform_emulators: HashMap::new(),
            save_dirs: HashMap::new(),
            fullscreen: false,
            quit_combo: true,
            phones: HashMap::new(),
            dual_screen: Vec::new(),
        }
    }
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

#[cfg(not(target_os = "linux"))]
fn entry(user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, user).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn set_password(user: &str, password: &str) -> Result<(), String> {
    entry(user)?.set_password(password).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn clear_password(user: &str) {
    if let Ok(e) = entry(user) {
        let _ = e.delete_credential();
    }
}

#[cfg(not(target_os = "linux"))]
pub fn get_password(user: &str) -> Option<String> {
    entry(user).ok()?.get_password().ok()
}

// SteamOS Game Mode has no Secret Service: keep the password in a file only
// the user can read, ~/.local/share/aetherplay/credentials.json (mode 600).
#[cfg(target_os = "linux")]
fn credentials_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join(KEYRING_SERVICE).join("credentials.json"))
}

#[cfg(target_os = "linux")]
fn read_credentials() -> HashMap<String, String> {
    credentials_file()
        .and_then(|f| std::fs::read(f).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn write_credentials(all: &HashMap<String, String>) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let file = credentials_file().ok_or("HOME introuvable")?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec(all).map_err(|e| e.to_string())?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&file)
        .map_err(|e| e.to_string())?;
    f.write_all(&json).map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
pub fn set_password(user: &str, password: &str) -> Result<(), String> {
    let mut all = read_credentials();
    all.insert(user.to_string(), password.to_string());
    write_credentials(&all)
}

#[cfg(target_os = "linux")]
pub fn clear_password(user: &str) {
    let mut all = read_credentials();
    if all.remove(user).is_some() {
        let _ = write_credentials(&all);
    }
}

#[cfg(target_os = "linux")]
pub fn get_password(user: &str) -> Option<String> {
    read_credentials().remove(user)
}
