//! Hosts RomM's own web interface (bundled from `romm-ui/`) in the main
//! window through the `romm://` scheme:
//! - static files come from the bundled build (SPA fallback to index.html),
//! - everything else (`/api`, server assets…) is relayed to the user's RomM
//!   server with their credentials, so the page is same-origin and needs no CORS,
//! - `window.rommNative` (RomM's desktop-shell contract) is injected so the
//!   page's Play buttons launch games natively.

use crate::config;
use base64::Engine;
use reqwest::cookie::{CookieStore, Jar};
use std::{
    io::Write,
    path::PathBuf,
    sync::{Arc, LazyLock, RwLock},
};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, http};

/// Origin the `romm` scheme is served from (WebView2 maps custom schemes to http).
pub const ORIGIN: &str = if cfg!(windows) { "http://romm.localhost" } else { "romm://localhost" };

const BRIDGE: &str = include_str!("bridge.js");

/// HTTP client with its own cookie jar: RomM's session cookie is kept here, in
/// the proxy, so being logged in never depends on the webview storing cookies
/// (custom schemes cannot, on macOS) nor on the keychain.
static SESSION: LazyLock<RwLock<(reqwest::Client, Arc<Jar>)>> = LazyLock::new(|| RwLock::new(new_session()));

fn new_session() -> (reqwest::Client, Arc<Jar>) {
    let jar = Arc::new(Jar::default());
    let client = reqwest::Client::builder()
        .user_agent("aetherplay")
        .cookie_provider(jar.clone())
        .build()
        .expect("http client");
    (client, jar)
}

fn session() -> (reqwest::Client, Arc<Jar>) {
    SESSION.read().expect("session lock").clone()
}

fn reset_session() {
    *SESSION.write().expect("session lock") = new_session();
}

fn cookie_value(cookies: &str, name: &str) -> Option<String> {
    cookies.split(';').filter_map(|c| c.trim().split_once('=')).find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
}

/// Appends one line to proxy.log in the app's log folder (no secrets: method,
/// path, status and which authentication was used).
pub fn log(app: &AppHandle, line: &str) {
    let Ok(dir) = app.path().app_log_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("aetherplay.log");
    if std::fs::metadata(&file).is_ok_and(|m| m.len() > 1 << 20) {
        let _ = std::fs::remove_file(&file);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let _ = writeln!(f, "{secs} {line}");
    }
}

/// Request headers never forwarded: the proxy owns auth, cookies and encoding.
const DROP_REQUEST: &[&str] =
    &["host", "cookie", "origin", "referer", "content-length", "connection", "accept-encoding", "x-csrftoken"];
const DROP_RESPONSE: &[&str] = &["set-cookie", "transfer-encoding", "connection", "content-length", "content-encoding"];

fn respond(status: u16, content_type: &str, body: Vec<u8>) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, content_type)
        .body(body)
        .expect("valid response")
}

fn ui_root(app: &AppHandle) -> Option<PathBuf> {
    let bundled = app.path().resource_dir().ok()?.join("romm-ui");
    if bundled.join("index.html").exists() {
        return Some(bundled);
    }
    // `tauri dev` before resources are copied: use the build output directly.
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../romm-ui/dist");
    dev.join("index.html").exists().then_some(dev)
}

fn mime(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "json" | "webmanifest" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn is_server_path(path: &str) -> bool {
    ["/api/", "/openapi.json", "/ws", "/netplay"].iter().any(|p| path.starts_with(p))
}

/// Bundled interface files, read from disk once.
static STATIC: LazyLock<std::sync::Mutex<std::collections::HashMap<PathBuf, Arc<Vec<u8>>>>> =
    LazyLock::new(Default::default);

fn read_static(file: &std::path::Path) -> Option<Arc<Vec<u8>>> {
    let mut cache = STATIC.lock().expect("static lock");
    if let Some(bytes) = cache.get(file) {
        return Some(bytes.clone());
    }
    let bytes = Arc::new(std::fs::read(file).ok()?);
    cache.insert(file.to_path_buf(), bytes.clone());
    Some(bytes)
}

fn static_response(file: &std::path::Path, content_type: &str, immutable: bool) -> Option<http::Response<Vec<u8>>> {
    let bytes = read_static(file)?;
    http::Response::builder()
        .status(200)
        .header(http::header::CONTENT_TYPE, content_type)
        // Build files carry a content hash in their name: cache them for good.
        .header(http::header::CACHE_CONTROL, if immutable { "public, max-age=31536000, immutable" } else { "no-cache" })
        .body(bytes.as_ref().clone())
        .ok()
}

/// A file of the bundled interface, or index.html for client-side routes.
fn serve_static(app: &AppHandle, path: &str) -> Option<http::Response<Vec<u8>>> {
    let root = ui_root(app)?;
    let rel = percent_decode(path.trim_start_matches('/'));
    if rel.split('/').any(|seg| seg == "..") {
        return None;
    }
    let file = if rel.is_empty() { root.join("index.html") } else { root.join(&rel) };
    if file.is_file() {
        return static_response(&file, mime(&file), rel.starts_with("assets/"));
    }
    // Client-side route (no extension, not a server asset): the SPA shell.
    let last = rel.rsplit('/').next().unwrap_or_default();
    if !rel.starts_with("assets/") && !last.contains('.') {
        return static_response(&root.join("index.html"), "text/html; charset=utf-8", false);
    }
    None
}

/// Server images (covers, screenshots…) kept on disk: a library of thousands
/// of games is scrolled without downloading its covers again. RomM puts a
/// `?ts=` version in their URLs, part of the cache key.
fn image_cache_file(app: &AppHandle, path: &str, path_and_query: &str) -> Option<PathBuf> {
    if !path.starts_with("/assets/romm/resources/") {
        return None;
    }
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path_and_query.hash(&mut h);
    let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("bin");
    Some(image_cache_dir(app)?.join(format!("{:016x}.{ext}", h.finish())))
}

pub fn image_cache_dir(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_cache_dir().ok()?.join("images"))
}

fn image_response(path: &str, bytes: Vec<u8>) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(200)
        .header(http::header::CONTENT_TYPE, mime(std::path::Path::new(path)))
        .header(http::header::CACHE_CONTROL, "public, max-age=604800")
        .body(bytes)
        .expect("valid response")
}

async fn proxy(app: &AppHandle, req: http::Request<Vec<u8>>) -> Result<http::Response<Vec<u8>>, String> {
    let cfg = config::load(app);
    if cfg.server_url.is_empty() {
        return Ok(respond(503, "text/plain", b"Serveur RomM non configure".to_vec()));
    }
    let path = req.uri().path().to_string();
    let pq = req.uri().path_and_query().map_or(path.clone(), |p| p.as_str().to_string());
    let method = reqwest::Method::from_bytes(req.method().as_str().as_bytes()).map_err(|e| e.to_string())?;
    let cached_image = (method == reqwest::Method::GET).then(|| image_cache_file(app, &path, &pq)).flatten();
    if let Some(file) = &cached_image
        && let Ok(bytes) = tokio::fs::read(file).await
    {
        return Ok(image_response(&path, bytes));
    }
    let (http, jar) = session();
    let url = format!("{}{pq}", cfg.server_url);
    let mut rb = http.request(method.clone(), &url);
    for (name, value) in req.headers() {
        if !DROP_REQUEST.contains(&name.as_str()) {
            rb = rb.header(name.as_str(), value.as_bytes());
        }
    }
    let cookies = url.parse().ok().and_then(|u| jar.cookies(&u)).and_then(|v| v.to_str().ok().map(str::to_owned)).unwrap_or_default();
    let has_session = cookie_value(&cookies, "romm_session").is_some();
    // The page's own Authorization (its login form) wins; then the session the
    // proxy holds; then the credentials remembered from the last login.
    let page_auth = req.headers().get(http::header::AUTHORIZATION).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let mut auth = if page_auth.is_some() { "page" } else if has_session { "session" } else { "none" };
    if page_auth.is_none()
        && !has_session
        && let Some(pass) = config::get_password(&cfg.username)
    {
        rb = rb.basic_auth(&cfg.username, Some(pass));
        auth = "stored";
    }
    // Session-authenticated writes need RomM's CSRF token, which lives in the jar.
    if !matches!(method, reqwest::Method::GET | reqwest::Method::HEAD | reqwest::Method::OPTIONS)
        && let Some(token) = cookie_value(&cookies, "romm_csrftoken").or_else(|| cookie_value(&cookies, "csrftoken"))
    {
        rb = rb.header("x-csrftoken", token);
    }
    let res = rb.body(req.into_body()).send().await.map_err(|e| e.to_string())?;
    let status = res.status();
    if path == "/api/login" || path == "/api/logout" || path == "/api/users/me" || (status.as_u16() >= 400 && path.starts_with("/api/")) {
        log(app, &format!("{method} {path} -> {} (auth: {auth})", status.as_u16()));
    }

    if status.is_success() && method == reqwest::Method::POST {
        if path == "/api/login"
            && let Some((user, pass)) = page_auth.as_deref().and_then(decode_basic)
        {
            // Logging in from RomM's own form: remember it for the next start.
            // Never fatal: the session cookie in the jar already logs us in.
            if let Err(e) = config::set_password(&user, &pass) {
                log(app, &format!("keychain: could not store the password: {e}"));
            }
            let mut cfg = config::load(app);
            cfg.username = user;
            if let Err(e) = config::save(app, &cfg) {
                log(app, &format!("config: could not save: {e}"));
            }
        } else if path == "/api/logout" {
            config::clear_password(&cfg.username);
            reset_session();
        }
    }

    let mut out = http::Response::builder().status(status.as_u16());
    for (name, value) in res.headers() {
        if !DROP_RESPONSE.contains(&name.as_str()) {
            out = out.header(name.as_str(), value.as_bytes());
        }
    }
    let body = res.bytes().await.map_err(|e| e.to_string())?.to_vec();
    if status == reqwest::StatusCode::OK
        && let Some(file) = cached_image
    {
        if let Some(dir) = file.parent() {
            let _ = tokio::fs::create_dir_all(dir).await;
        }
        let _ = tokio::fs::write(&file, &body).await;
        return Ok(image_response(&path, body));
    }
    out.body(body).map_err(|e| e.to_string())
}

fn decode_basic(header: &str) -> Option<(String, String)> {
    let encoded = header.strip_prefix("Basic ").or_else(|| header.strip_prefix("basic "))?;
    let raw = base64::engine::general_purpose::STANDARD.decode(encoded.trim()).ok()?;
    let (user, pass) = String::from_utf8(raw).ok()?.split_once(':').map(|(u, p)| (u.to_owned(), p.to_owned()))?;
    Some((user, pass))
}

/// `romm://` handler.
pub async fn handle(app: &AppHandle, req: http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
    let path = req.uri().path().to_string();
    if !is_server_path(&path)
        && let Some(res) = serve_static(app, &path)
    {
        return res;
    }
    proxy(app, req)
        .await
        .unwrap_or_else(|e| respond(502, "text/plain; charset=utf-8", format!("AetherPlay : {e}").into_bytes()))
}

pub fn create_main_window(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("AetherPlay")
        .inner_size(1280.0, 800.0)
        .min_inner_size(900.0, 600.0)
        .fullscreen(game_mode())
        .initialization_script(BRIDGE)
        .build()?;
    Ok(())
}

/// Running under Steam's Game Mode (SteamOS, gamescope): no desktop around.
pub fn game_mode() -> bool {
    ["SteamGamepadUI", "SteamDeck", "GAMESCOPE_WAYLAND_DISPLAY"].iter().any(|v| std::env::var_os(v).is_some())
}

pub fn open_romm(app: &AppHandle) -> tauri::Result<()> {
    if let Some(win) = app.get_webview_window("main") {
        win.navigate(format!("{ORIGIN}/").parse().expect("valid url"))?;
    }
    Ok(())
}

pub fn open_settings(app: &AppHandle) -> tauri::Result<()> {
    if let Some(win) = app.get_webview_window("settings") {
        return win.set_focus();
    }
    WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html#settings".into()))
        .title("AetherPlay : réglages")
        .inner_size(1000.0, 720.0)
        .build()?;
    Ok(())
}
