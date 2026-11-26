//! Downloaded ROM cache: `roms/<platform>/<file or folder>`.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
pub struct Entry {
    pub platform: String,
    pub name: String,
    pub path: String,
    pub bytes: u64,
}

fn size(p: &Path) -> u64 {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| size(&e.path())).sum(),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

/// Every cached ROM, biggest first.
pub fn list(root: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    for platform in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let platform_name = platform.file_name().to_string_lossy().into_owned();
        for item in std::fs::read_dir(platform.path()).into_iter().flatten().flatten() {
            let path = item.path();
            out.push(Entry {
                platform: platform_name.clone(),
                name: item.file_name().to_string_lossy().into_owned(),
                bytes: size(&path),
                path: path.to_string_lossy().into_owned(),
            });
        }
    }
    out.sort_by_key(|e| std::cmp::Reverse(e.bytes));
    out
}

/// Deletes one cached ROM; refuses anything outside `root`.
pub fn delete(root: &Path, path: &str) -> Result<(), String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let target: PathBuf = Path::new(path).canonicalize().map_err(|e| e.to_string())?;
    if target == root || !target.starts_with(&root) {
        return Err("Chemin hors du cache".into());
    }
    if target.is_dir() { std::fs::remove_dir_all(&target) } else { std::fs::remove_file(&target) }
        .map_err(|e| e.to_string())
}

pub fn clear(root: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(root) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}
