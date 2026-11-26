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

pub const SLOT: &str = "aetherplay";
const ZIP_PREFIX: &str = "aetherplay-";
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

/// Placeholder for the emulator user profile in uploaded archive entries.
const PROFILE: &str = "_profile_";

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// yuzu-family emulators (Eden, Eglantine) keep a game's save under
/// `<16 hex>/<32 hex profile id>/<title id>/`, and the profile id is random per
/// install. Archive entries replace it with a placeholder so a save made on
/// one machine lands in the right profile on another.
fn to_portable(key: &str) -> String {
    let parts: Vec<&str> = key.splitn(4, '/').collect();
    match parts.as_slice() {
        [idx, zeros, profile, rest] if is_hex(zeros, 16) && is_hex(profile, 32) => {
            format!("{idx}/{zeros}/{PROFILE}/{rest}")
        }
        _ => key.to_string(),
    }
}

/// Inverse of `to_portable` for this machine: the placeholder becomes the local
/// profile folder (the one already holding saves), or `None` when the emulator
/// has no profile folder yet (no game was ever started here).
fn from_portable(key: &str, dirs: &[PathBuf]) -> Option<String> {
    let parts: Vec<&str> = key.splitn(4, '/').collect();
    let [idx, zeros, profile, rest] = parts.as_slice() else { return Some(key.to_string()) };
    if *profile != PROFILE {
        return Some(key.to_string());
    }
    let root = dirs.get(idx.parse::<usize>().ok()?)?.join(zeros);
    let local = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| is_hex(n, 32))?;
    Some(format!("{idx}/{zeros}/{local}/{rest}"))
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
///
/// single_file_as: for emulators keeping one save file per game named after
/// the ROM (RetroArch: <rom stem>.srm), any raw save RomM holds for the game
/// (e.g. from its web player) is accepted too, and written under that name.
/// orce writes even over newer local files (used for a deferred restore,
/// over the blank save a first session created).
pub struct Restored {
    /// One-line description, for the diagnostic log.
    pub message: String,
    /// The save targets an emulator profile that does not exist yet on this
    /// machine: restore again, forced, once the game has run once.
    pub deferred: bool,
}

pub async fn restore(
    client: &Client,
    rom: &Rom,
    emu: &str,
    dirs: &[PathBuf],
    single_file_as: Option<&str>,
    force: bool,
    index: &mut Index,
) -> Result<Restored, String> {
    let saves = client.saves(rom.id).await?;
    let total = saves.len();
    let ours = |s: &crate::romm::Save| s.emulator.as_deref() == Some(emu) && s.slot.as_deref() == Some(SLOT);
    let latest = saves
        .into_iter()
        .filter(|s| ours(s) || (single_file_as.is_some() && !s.file_name.starts_with(ZIP_PREFIX)))
        .max_by_key(|s| s.updated_at);
    let Some(save) = latest else {
        return Ok(Restored { message: format!("aucune sauvegarde utilisable sur RomM ({total} pour ce jeu)"), deferred: false });
    };
    let data = client.save_content(save.id).await?;
    let remote: SystemTime = save.updated_at.into();
    let mut keys = Vec::new();
    let mut written = 0;
    let mut no_profile = 0;
    let write = |target: &Path, data: &[u8]| -> Result<bool, String> {
        if force {
            if let Some(dir) = target.parent() {
                std::fs::create_dir_all(dir).map_err(err)?;
            }
            std::fs::write(target, data).map_err(err)?;
            Ok(true)
        } else {
            write_if_newer(target, data, remote)
        }
    };

    if save.file_name.starts_with(ZIP_PREFIX) && save.file_name.ends_with(".zip") {
        let mut archive = zip::ZipArchive::new(Cursor::new(data)).map_err(err)?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(err)?;
            if file.is_dir() {
                continue;
            }
            let portable = file.name().map_err(err)?.replace('\\', "/");
            // Switch saves: map the profile placeholder onto this machine's profile.
            let Some(key) = from_portable(&portable, dirs) else {
                no_profile += 1;
                continue;
            };
            let Some(target) = resolve(dirs, &key) else { continue };
            let mut buf = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut buf).map_err(err)?;
            written += usize::from(write(&target, &buf)?);
            keys.push(key);
        }
    } else {
        let name = match single_file_as {
            Some(name) => name.to_string(),
            None => Path::new(&save.file_name)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("Nom de sauvegarde invalide")?
                .to_string(),
        };
        let key = format!("0/{name}");
        if let Some(target) = resolve(dirs, &key) {
            written += usize::from(write(&target, &data)?);
            keys.push(key);
        }
    }
    index.entry(rom.id, emu).extend(keys);
    index.save()?;
    let message = if no_profile > 0 {
        format!("« {} » : profil de l'émulateur absent ici, restauration reportée à la fin de la partie", save.file_name)
    } else if written > 0 {
        format!("« {} » ({}) restaurée : {written} fichier(s)", save.file_name, save.emulator.unwrap_or_default())
    } else {
        format!("« {} » plus ancienne que la copie locale : gardée telle quelle", save.file_name)
    };
    Ok(Restored { message, deferred: no_profile > 0 })
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
                zip.start_file(to_portable(key), opts).map_err(err)?;
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
    fn switch_saves_move_between_emulator_profiles() {
        let mac_profile = "A1B2C3D4E5F60718293A4B5C6D7E8F90";
        let key = format!("0/0000000000000000/{mac_profile}/0400A3A01EE4C000/G00/STGDAT01.BIN");
        let portable = to_portable(&key);
        assert_eq!(portable, "0/0000000000000000/_profile_/0400A3A01EE4C000/G00/STGDAT01.BIN");
        // Non-Switch keys are left alone.
        assert_eq!(to_portable("0/Game.srm"), "0/Game.srm");

        let root = std::env::temp_dir().join(format!("romm-profile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = vec![root.clone()];
        // No profile folder yet on this machine: cannot place it.
        assert_eq!(from_portable(&portable, &dirs), None);
        let pc_profile = "0F1E2D3C4B5A69788796A5B4C3D2E1F0";
        std::fs::create_dir_all(root.join("0000000000000000").join(pc_profile)).unwrap();
        assert_eq!(
            from_portable(&portable, &dirs).unwrap(),
            format!("0/0000000000000000/{pc_profile}/0400A3A01EE4C000/G00/STGDAT01.BIN")
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

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
