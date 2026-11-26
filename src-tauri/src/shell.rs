//! Hosts RomM's own web interface (bundled from `romm-ui/`) in the main
//! window through the `romm://` scheme:
//! - static files come from the bundled build (SPA fallback to index.html),
//! - everything else (`/api`, server assets…) is relayed to the user's RomM
//!   server with their credentials, so the page is same-origin and needs no CORS,
//! - `window.rommNative` (RomM's desktop-shell contract) is injected so the
//!   page's Play buttons launch games natively.

use crate::config;
use base64::Engine;
use std::{path::PathBuf, sync::LazyLock};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, http};

/// Origin the `romm` scheme is served from (WebView2 maps custom schemes to http).
pub const ORIGIN: &str = if cfg!(windows) { "http://romm.localhost" } else { "romm://localhost" };

const BRIDGE: &str = include_str!("bridge.js");

static HTTP: LazyLock<reqwest::Client> =
    LazyLock::new(|| reqwest::Client::builder().user_agent("aetherplay").build().expect("http client"));

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

/// A file of the bundled interface, or index.html for client-side routes.
fn serve_static(app: &AppHandle, path: &str) -> Option<http::Response<Vec<u8>>> {
    let root = ui_root(app)?;
    let rel = percent_decode(path.trim_start_matches('/'));
    if rel.split('/').any(|seg| seg == "..") {
        return None;
    }
    let file = if rel.is_empty() { root.join("index.html") } else { root.join(&rel) };
    if file.is_file() {
        return Some(respond(200, mime(&file), std::fs::read(&file).ok()?));
    }
    // Client-side route (no extension, not a server asset): the SPA shell.
    let last = rel.rsplit('/').next().unwrap_or_default();
    if !rel.starts_with("assets/") && !last.contains('.') {
        return Some(respond(200, "text/html; charset=utf-8", std::fs::read(root.join("index.html")).ok()?));
    }
    None
}

async fn proxy(app: &AppHandle, req: http::Request<Vec<u8>>) -> Result<http::Response<Vec<u8>>, String> {
    let cfg = config::load(app);
    if cfg.server_url.is_empty() {
        return Ok(respond(503, "text/plain", b"Serveur RomM non configure".to_vec()));
    }
    let path = req.uri().path().to_string();
    let pq = req.uri().path_and_query().map_or(path.clone(), |p| p.as_str().to_string());
    let method = reqwest::Method::from_bytes(req.method().as_str().as_bytes()).map_err(|e| e.to_string())?;
    let mut rb = HTTP.request(method.clone(), format!("{}{pq}", cfg.server_url));
    for (name, value) in req.headers() {
        if !DROP_REQUEST.contains(&name.as_str()) {
            rb = rb.header(name.as_str(), value.as_bytes());
        }
    }
    // The page's own Authorization (its login form) wins over the stored one.
    let page_auth = req.headers().get(http::header::AUTHORIZATION).and_then(|v| v.to_str().ok()).map(str::to_owned);
    if page_auth.is_none()
        && let Some(pass) = config::get_password(&cfg.username)
    {
        rb = rb.basic_auth(&cfg.username, Some(pass));
    }
    let res = rb.body(req.into_body()).send().await.map_err(|e| e.to_string())?;
    let status = res.status();

    if status.is_success() && method == reqwest::Method::POST {
        if path == "/api/login"
            && let Some((user, pass)) = page_auth.as_deref().and_then(decode_basic)
        {
            // Logging in from RomM's own form: remember it for the proxy.
            config::set_password(&user, &pass)?;
            let mut cfg = config::load(app);
            cfg.username = user;
            config::save(app, &cfg)?;
        } else if path == "/api/logout" {
            config::clear_password(&cfg.username);
        }
    }

    let mut out = http::Response::builder().status(status.as_u16());
    for (name, value) in res.headers() {
        if !DROP_RESPONSE.contains(&name.as_str()) {
            out = out.header(name.as_str(), value.as_bytes());
        }
    }
    let body = res.bytes().await.map_err(|e| e.to_string())?.to_vec();
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
        .initialization_script(BRIDGE)
        .build()?;
    Ok(())
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
