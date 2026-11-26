//! Automatic emulator installation from official release sources
//! (GitHub releases or direct URLs, see `sources.json`).

use crate::emulators::CURRENT_OS;
use futures_util::StreamExt;
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
    sync::LazyLock,
};
use tokio::io::AsyncWriteExt;

#[derive(Deserialize)]
struct Source {
    /// Windows executable file name pattern (macOS uses the first `.app`).
    #[serde(default)]
    exe: String,
    #[serde(flatten)]
    per_os: HashMap<String, OsSource>,
}

#[derive(Deserialize)]
struct OsSource {
    #[serde(default)]
    github: Option<String>,
    #[serde(default)]
    asset: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    url: Option<String>,
    /// Forgejo/Gitea repo as `host/owner/repo` (same release API shape as GitHub).
    #[serde(default)]
    forgejo: Option<String>,
    /// HTML download page: first link matching `asset`.
    #[serde(default)]
    page: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

static SOURCES: LazyLock<HashMap<String, Source>> =
    LazyLock::new(|| serde_json::from_str(include_str!("../sources.json")).expect("sources.json invalide"));

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Emulator ids that can be installed automatically on this OS.
pub fn installable() -> Vec<String> {
    SOURCES.iter().filter(|(_, s)| s.per_os.contains_key(CURRENT_OS)).map(|(k, _)| k.clone()).collect()
}

fn http() -> reqwest::Client {
    reqwest::Client::builder().user_agent("aetherplay").build().expect("http client")
}

/// Finds the download URL and file name for `src`.
async fn locate(src: &OsSource) -> Result<(String, String), String> {
    if let Some(url) = &src.url {
        let name = url.rsplit('/').next().unwrap_or("download").to_string();
        return Ok((url.clone(), name));
    }
    let pattern = Regex::new(src.asset.as_deref().unwrap_or(".")).map_err(err)?;
    if let Some(page) = &src.page {
        let html = http().get(page).send().await.map_err(err)?.text().await.map_err(err)?;
        let links = Regex::new(r#"https?://[^"'\s<>]+"#).expect("regex");
        return links
            .find_iter(&html)
            .map(|m| m.as_str())
            .find(|u| pattern.is_match(u))
            .map(|u| (u.to_string(), u.rsplit('/').next().unwrap_or("download").to_string()))
            .ok_or_else(|| format!("Aucun lien compatible sur {page}"));
    }
    let (api, repo) = match (&src.github, &src.forgejo) {
        (Some(repo), _) => (format!("https://api.github.com/repos/{repo}/releases?per_page=10"), repo.as_str()),
        (None, Some(path)) => {
            let (host, repo) = path.split_once('/').ok_or("Source invalide")?;
            (format!("https://{host}/api/v1/repos/{repo}/releases?limit=10"), repo)
        }
        _ => return Err("Source invalide".into()),
    };
    let skip = Regex::new(r"(?i)dbg|debug|symbols|\.pdb|\.sha\d*|\.sig$").expect("regex");
    let releases: Vec<Release> = http()
        .get(api)
        .send()
        .await
        .map_err(err)?
        .error_for_status()
        .map_err(|e| format!("GitHub ({repo}) : {e}"))?
        .json()
        .await
        .map_err(err)?;
    releases
        .into_iter()
        .filter(|r| src.prerelease || !r.prerelease)
        .flat_map(|r| r.assets)
        .find(|a| pattern.is_match(&a.name) && !skip.is_match(&a.name))
        .map(|a| (a.browser_download_url, a.name))
        .ok_or_else(|| format!("Aucun fichier compatible dans les releases de {repo}"))
}

async fn download(url: &str, dest: &Path, mut progress: impl FnMut(u64, u64)) -> Result<(), String> {
    let res = http().get(url).send().await.map_err(err)?.error_for_status().map_err(err)?;
    let total = res.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(dest).await.map_err(err)?;
    let mut stream = res.bytes_stream();
    let (mut done, mut last) = (0u64, 0u64);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(err)?;
        file.write_all(&chunk).await.map_err(err)?;
        done += chunk.len() as u64;
        if done - last >= 1 << 20 || done == total {
            last = done;
            progress(done, total);
        }
    }
    file.flush().await.map_err(err)
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(err)?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Extracts zip / 7z / tar.xz / dmg archives into `dest`.
fn extract(archive: &Path, name: &str, dest: &Path) -> Result<(), String> {
    let lower = name.to_lowercase();
    if lower.ends_with(".7z") {
        sevenz_rust2::decompress_file(archive, dest).map_err(err)
    } else if lower.ends_with(".zip") {
        if cfg!(target_os = "macos") {
            // ditto keeps the symlinks and permissions .app bundles rely on.
            run(Command::new("ditto").arg("-x").arg("-k").arg(archive).arg(dest))
        } else {
            let file = std::fs::File::open(archive).map_err(err)?;
            zip::ZipArchive::new(file).map_err(err)?.extract(dest).map_err(err)
        }
    } else if lower.ends_with(".tar.xz") || lower.ends_with(".tar.gz") {
        run(Command::new("tar").arg("-xf").arg(archive).arg("-C").arg(dest))
    } else if lower.ends_with(".appimage") {
        // Already the program: keep it as is.
        std::fs::copy(archive, dest.join(name)).map(|_| ()).map_err(err)
    } else if lower.ends_with(".dmg") {
        let mount = dest.join(".dmg-mount");
        run(Command::new("hdiutil")
            .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
            .arg(&mount)
            .arg(archive))?;
        let app = find_app(&mount).ok_or_else(|| "Aucune application dans le .dmg".to_string());
        let copied = app.and_then(|app| {
            run(Command::new("ditto").arg(&app).arg(dest.join(app.file_name().unwrap_or_default())))
        });
        let _ = run(Command::new("hdiutil").args(["detach", "-quiet"]).arg(&mount));
        let _ = std::fs::remove_dir(&mount);
        copied
    } else {
        Err(format!("Format d'archive non supporté : {name}"))
    }
}

/// First `.app` bundle under `dir` (bundles are not searched into).
fn find_app(dir: &Path) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).ok()?.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "app") {
                return Some(p);
            }
            if p.is_dir() && !p.is_symlink() {
                stack.push(p);
            }
        }
    }
    None
}

/// Shallowest file under `dir` whose name matches `pattern`.
fn find_exe(dir: &Path, pattern: &str) -> Option<PathBuf> {
    let re = Regex::new(&format!("(?i){pattern}")).ok()?;
    let mut level = vec![dir.to_path_buf()];
    for _ in 0..6 {
        let mut next = Vec::new();
        let mut hits: Vec<PathBuf> = Vec::new();
        for d in &level {
            for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    next.push(p);
                } else if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| re.is_match(n)) {
                    hits.push(p);
                }
            }
        }
        // Prefer the shortest name, e.g. `eglantine.exe` over `eglantine-cmd.exe`.
        if let Some(best) = hits.into_iter().min_by_key(|p| p.as_os_str().len()) {
            return Some(best);
        }
        level = next;
    }
    None
}

/// Downloads and installs emulator `id` under `root/<id>`; returns the path to
/// launch (exe on Windows, `.app` on macOS).
pub async fn install(
    id: &str,
    root: &Path,
    progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    let source = SOURCES.get(id).ok_or("Installation automatique indisponible")?;
    let os_src = source.per_os.get(CURRENT_OS).ok_or("Indisponible sur ce système")?;
    let (url, name) = locate(os_src).await?;

    let dir = root.join(id);
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.map_err(err)?;
    let archive = root.join(format!("{id}-{name}"));
    download(&url, &archive, progress).await?;

    let (a, n, d, exe) = (archive.clone(), name.clone(), dir.clone(), source.exe.clone());
    let result = tokio::task::spawn_blocking(move || {
        extract(&a, &n, &d)?;
        let found = if cfg!(target_os = "macos") {
            find_app(&d)
        } else if cfg!(target_os = "linux") {
            find_exe(&d, r"\.appimage$")
        } else {
            find_exe(&d, &exe)
        };
        let found = found.ok_or_else(|| "Exécutable introuvable après extraction".to_string())?;
        #[cfg(unix)]
        if !cfg!(target_os = "macos") {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&found, std::fs::Permissions::from_mode(0o755)).map_err(err)?;
        }
        Ok(found)
    })
    .await
    .map_err(err)?;
    let _ = tokio::fs::remove_file(&archive).await;
    result
}

/// Windows: installs ViGEmBus, the free driver behind the virtual Xbox
/// controller the Android app uses over Wi-Fi. Its installer asks for
/// administrator rights (UAC) and shows its own window.
#[cfg(windows)]
pub async fn install_vigem(dir: &Path) -> Result<(), String> {
    let src = OsSource {
        github: Some("nefarius/ViGEmBus".into()),
        asset: Some(r"^ViGEmBus_.*\.exe$".into()),
        prerelease: false,
        url: None,
        forgejo: None,
        page: None,
    };
    let (url, name) = locate(&src).await?;
    tokio::fs::create_dir_all(dir).await.map_err(err)?;
    let exe = dir.join(name);
    download(&url, &exe, |_, _| {}).await?;
    let exe2 = exe.clone();
    let status = tokio::task::spawn_blocking(move || {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // Start-Process -Verb RunAs: the installer requires elevation.
        Command::new("powershell")
            .args(["-NoProfile", "-Command", "Start-Process", "-FilePath"])
            .arg(format!("'{}'", exe2.display()))
            .args(["-Verb", "RunAs", "-Wait"])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
    })
    .await
    .map_err(err)?
    .map_err(err)?;
    let _ = tokio::fs::remove_file(&exe).await;
    if status.success() { Ok(()) } else { Err("Installation annulée".into()) }
}

/// Downloads a missing RetroArch core from the libretro buildbot.
pub async fn ensure_core(core_path: &Path) -> Result<(), String> {
    if core_path.exists() {
        return Ok(());
    }
    let file = core_path.file_name().and_then(|n| n.to_str()).ok_or("Core invalide")?;
    let base = if cfg!(target_os = "macos") {
        "https://buildbot.libretro.com/nightly/apple/osx/arm64/latest"
    } else if cfg!(target_os = "linux") {
        "https://buildbot.libretro.com/nightly/linux/x86_64/latest"
    } else {
        "https://buildbot.libretro.com/nightly/windows/x86_64/latest"
    };
    let dir = core_path.parent().ok_or("Core invalide")?;
    tokio::fs::create_dir_all(dir).await.map_err(err)?;
    let zip_path = dir.join(format!("{file}.zip"));
    download(&format!("{base}/{file}.zip"), &zip_path, |_, _| {})
        .await
        .map_err(|e| format!("Téléchargement du core {file} : {e}"))?;
    let (z, d) = (zip_path.clone(), dir.to_path_buf());
    let res = tokio::task::spawn_blocking(move || {
        let f = std::fs::File::open(&z).map_err(err)?;
        zip::ZipArchive::new(f).map_err(err)?.extract(&d).map_err(err)
    })
    .await
    .map_err(err)?;
    let _ = tokio::fs::remove_file(&zip_path).await;
    res
}

#[cfg(test)]
mod tests {
    /// Real downloads; run with `cargo test -- --ignored install_real`.
    #[test]
    #[ignore]
    fn install_real() {
        let root = std::env::temp_dir().join("romm-install-test");
        let ids = std::env::var("EMUS").unwrap_or_else(|_| "melonds,pcsx2,eglantine".into());
        for id in ids.split(',') {
            let res = tauri::async_runtime::block_on(super::install(id, &root, |_, _| {}));
            println!("{id}: {res:?}");
            assert!(res.is_ok_and(|p| p.exists()), "{id} failed");
        }
    }
}
