//! Save sync with RomM.
//!
//! Before a session the latest save for (rom, emulator) is restored into the
//! emulator's save directories, unless the local copy is newer. After the
//! session every file modified meanwhile is added to the game's save set and
//! the set is uploaded: as-is when it is a single top-level file (e.g. a
//! RetroArch `.srm`, usable by RomM's web player), otherwise as a zip whose
//! entries are `<save dir index>/<relative path>`.

use crate::romm::{Client, Rom, SaveUpload};
use std::{
    collections::{BTreeSet, HashMap},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

pub const SLOT: &str = "romm-desktop";
const ZIP_PREFIX: &str = "romm-desktop-";
const MAX_DEPTH: usize = 8;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Remembers which save files belong to which game, so a later session that
/// only touches one of them still uploads the whole set.
pub struct Index {
    path: PathBuf,
    map: HashMap<String, BTreeSet<String>>,
}

impl Index {
    pub fn load(path: PathBuf) -> Self {
        let map = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { path, map }
    }

    fn save(&self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(err)?;
        }
        std::fs::write(&self.path, serde_json::to_vec(&self.map).map_err(err)?).map_err(err)
    }

    fn entry(&mut self, rom_id: u64, emu: &str) -> &mut BTreeSet<String> {
        self.map.entry(format!("{rom_id}:{emu}")).or_default()
    }
}

/// `"<idx>/<a/b/c>"` -> absolute path inside `dirs[idx]`, rejecting traversal.
fn resolve(dirs: &[PathBuf], key: &str) -> Option<PathBuf> {
    let (idx, rel) = key.split_once('/')?;
    let root = dirs.get(idx.parse::<usize>().ok()?)?;
    let rel = Path::new(rel);
    rel.components()
        .all(|c| matches!(c, std::path::Component::Normal(_)))
        .then(|| root.join(rel))
}

fn modified(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).ok()?.modified().ok()
}

/// Writes `data` to `target` unless the local file is newer than the remote save.
fn write_if_newer(target: &Path, data: &[u8], remote: SystemTime) -> Result<bool, String> {
    if modified(target).is_some_and(|local| local > remote) {
        return Ok(false);
    }
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(err)?;
    }
    std::fs::write(target, data).map_err(err)?;
    Ok(true)
}

/// Downloads the latest RomM save for this game/emulator and puts it in place.
pub async fn restore(
    client: &Client,
    rom: &Rom,
    emu: &str,
    dirs: &[PathBuf],
    index: &mut Index,
) -> Result<(), String> {
    let latest = client
        .saves(rom.id)
        .await?
        .into_iter()
        .filter(|s| s.emulator.as_deref() == Some(emu) && s.slot.as_deref() == Some(SLOT))
        .max_by_key(|s| s.updated_at);
    let Some(save) = latest else { return Ok(()) };
    let data = client.save_content(save.id).await?;
    let remote: SystemTime = save.updated_at.into();
    let mut keys = Vec::new();

    if save.file_name.starts_with(ZIP_PREFIX) && save.file_name.ends_with(".zip") {
        let mut archive = zip::ZipArchive::new(Cursor::new(data)).map_err(err)?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(err)?;
            if file.is_dir() {
                continue;
            }
            let key = file.name().map_err(err)?.replace('\\', "/");
            let Some(target) = resolve(dirs, &key) else { continue };
            let mut buf = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut buf).map_err(err)?;
            write_if_newer(&target, &buf, remote)?;
            keys.push(key);
        }
    } else {
        let name = Path::new(&save.file_name)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Nom de sauvegarde invalide")?;
        let key = format!("0/{name}");
        if let Some(target) = resolve(dirs, &key) {
            write_if_newer(&target, &data, remote)?;
            keys.push(key);
        }
    }
    index.entry(rom.id, emu).extend(keys);
    index.save()
}

/// Files under `dirs` modified since `since`, as index keys.
fn changed_since(dirs: &[PathBuf], since: SystemTime) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, depth: usize, since: SystemTime, out: &mut Vec<String>, idx: usize) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let path = e.path();
            if ft.is_dir() && depth < MAX_DEPTH {
                walk(root, &path, depth + 1, since, out, idx);
            } else if ft.is_file()
                && path.extension().is_none_or(|x| x != "part")
                && modified(&path).is_some_and(|m| m >= since)
                && let Ok(rel) = path.strip_prefix(root)
            {
                out.push(format!("{idx}/{}", rel.to_string_lossy().replace('\\', "/")));
            }
        }
    }
    let mut out = Vec::new();
    for (idx, root) in dirs.iter().enumerate() {
        walk(root, root, 0, since, &mut out, idx);
    }
    out
}

/// Uploads the game's save set if the session changed anything.
/// Returns `false` when there was nothing to upload.
#[allow(clippy::too_many_arguments)]
pub async fn upload(
    client: &Client,
    rom: &Rom,
    emu: &str,
    core: Option<&str>,
    dirs: &[PathBuf],
    rom_root: &Path,
    started: SystemTime,
    index: &mut Index,
) -> Result<bool, String> {
    // File timestamps are coarser than SystemTime::now() on Windows (~16 ms ticks).
    let since = started - std::time::Duration::from_secs(1);
    let changed: Vec<String> = changed_since(dirs, since)
        .into_iter()
        // Never treat the ROM itself (file or extracted folder) as a save.
        .filter(|k| resolve(dirs, k).is_some_and(|p| !p.starts_with(rom_root)))
        .collect();
    if changed.is_empty() {
        return Ok(false);
    }
    let set = index.entry(rom.id, emu);
    set.extend(changed);
    set.retain(|k| resolve(dirs, k).is_some_and(|p| p.is_file()));
    let keys: Vec<String> = set.iter().cloned().collect();
    index.save()?;

    let (file_name, data) = match keys.as_slice() {
        [single] if single.starts_with("0/") && !single[2..].contains('/') => {
            let path = resolve(dirs, single).ok_or("Chemin invalide")?;
            (single[2..].to_string(), std::fs::read(path).map_err(err)?)
        }
        _ => {
            let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for key in &keys {
                let path = resolve(dirs, key).ok_or("Chemin invalide")?;
                zip.start_file(key.as_str(), opts).map_err(err)?;
                zip.write_all(&std::fs::read(path).map_err(err)?).map_err(err)?;
            }
            (format!("{ZIP_PREFIX}{emu}.zip"), zip.finish().map_err(err)?.into_inner())
        }
    };
    client
        .upload_save(SaveUpload { rom_id: rom.id, emulator: emu, core, slot: SLOT, file_name: &file_name }, data)
        .await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_rejects_traversal_and_bad_index() {
        let dirs = vec![PathBuf::from("root0"), PathBuf::from("root1")];
        assert_eq!(resolve(&dirs, "1/a/b.sav"), Some(PathBuf::from("root1").join("a/b.sav")));
        assert_eq!(resolve(&dirs, "0/../evil"), None);
        assert_eq!(resolve(&dirs, "2/x"), None);
        assert_eq!(resolve(&dirs, "nope"), None);
    }

    #[test]
    fn detects_only_files_changed_during_session() {
        let root = std::env::temp_dir().join(format!("romm-saves-test-{}", std::process::id()));
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("old.sav"), b"old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let since = SystemTime::now();
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(root.join("sub/new.sav"), b"new").unwrap();
        std::fs::write(root.join("dl.part"), b"tmp").unwrap();
        let changed = changed_since(std::slice::from_ref(&root), since);
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(changed, vec!["0/sub/new.sav".to_string()]);
    }
}
