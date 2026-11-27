//! Save sync with RomM.
//!
//! RomM holds the reference copy of every save, on every OS:
//! * before a session the latest save for (rom, emulator) replaces the local
//!   files when they differ; each replaced local file is first copied to
//!   `save-backups/` in AetherPlay's data folder;
//! * after the session every file modified meanwhile is added to the game's
//!   save set and the set is uploaded: as-is when it is a single top-level file
//!   (e.g. a RetroArch `.srm`, usable by RomM's web player), otherwise as a zip
//!   whose entries are `<save dir index>/<relative path>`; the size RomM
//!   stored is checked;
//! * an upload that failed stays pending: it is retried before the next
//!   session, and until it succeeds the local save is never replaced (it is
//!   newer than RomM's).

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

    /// Games whose last upload failed (kept under a reserved key, so older
    /// index files still load).
    pub fn pending(&self, rom_id: u64, emu: &str) -> bool {
        self.map.get(PENDING).is_some_and(|s| s.contains(&format!("{rom_id}:{emu}")))
    }

    pub fn set_pending(&mut self, rom_id: u64, emu: &str, pending: bool) -> Result<(), String> {
        let set = self.map.entry(PENDING.to_string()).or_default();
        let key = format!("{rom_id}:{emu}");
        if pending {
            set.insert(key);
        } else {
            set.remove(&key);
        }
        self.save()
    }
}

const PENDING: &str = "__pending_uploads__";

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

/// Inverse of `to_portable` for this machine: the placeholder becomes the
/// emulator's active profile folder (see `switch_profile`), or `None` when the
/// emulator has no profile yet (never started here).
fn from_portable(key: &str, dirs: &[PathBuf]) -> Option<String> {
    let parts: Vec<&str> = key.splitn(4, '/').collect();
    let [idx, zeros, profile, rest] = parts.as_slice() else { return Some(key.to_string()) };
    if *profile != PROFILE {
        return Some(key.to_string());
    }
    let local = switch_profile(dirs.get(idx.parse::<usize>().ok()?)?, zeros)?;
    Some(format!("{idx}/{zeros}/{local}/{rest}"))
}

/// Profile folders already holding saves under `<save root>/<zeros>`, most
/// recently modified first.
fn profile_folders(save_root: &Path, zeros: &str) -> Vec<String> {
    let mut found: Vec<(Option<SystemTime>, String)> = std::fs::read_dir(save_root.join(zeros))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            is_hex(&name, 32).then(|| (modified(&e.path()), name))
        })
        .collect();
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    found.into_iter().map(|(_, n)| n).collect()
}

/// Users of a yuzu-family `profiles.dat` (0x10 header, then 8 entries of
/// 0xC8 bytes starting with the 16-byte UUID), as save folder names: the UUID
/// read as two little-endian u64, high one first (`{:016X}{:016X}`).
fn profile_ids(raw: &[u8]) -> Vec<String> {
    (0..8)
        .filter_map(|i| {
            let at = 0x10 + i * 0xC8;
            let b = raw.get(at..at + 16)?;
            if b.iter().all(|x| *x == 0) {
                return None;
            }
            let lo = u64::from_le_bytes(b[0..8].try_into().ok()?);
            let hi = u64::from_le_bytes(b[8..16].try_into().ok()?);
            Some(format!("{hi:016X}{lo:016X}"))
        })
        .collect()
}

/// `current_user` in the `[System]` section of the emulator's qt-config.ini
/// (the profile selected in the emulator), next to the data folder (Windows,
/// portable installs) or in ~/.config/<name> (macOS, Linux).
fn current_user(data: &Path) -> Option<usize> {
    let mut candidates = vec![data.join("config").join("qt-config.ini")];
    if let (Some(name), Some(home)) = (data.file_name(), std::env::var_os("HOME")) {
        let config =
            std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(home).join(".config"));
        candidates.push(config.join(name).join("qt-config.ini"));
    }
    let text = candidates.iter().find_map(|p| std::fs::read_to_string(p).ok())?;
    let mut in_system = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_system = line == "[System]";
        } else if in_system && let Some(v) = line.strip_prefix("current_user=") {
            return v.trim().parse().ok();
        }
    }
    None
}

/// The profile folder a Switch save goes to on this machine: the profile
/// selected in the emulator (`profiles.dat` + `current_user`) when its folder
/// exists, or when no profile folder exists at all yet (then it is created, so
/// the first session is not played on a blank save); otherwise the folder
/// used most recently (the old behaviour).
fn switch_profile(save_root: &Path, zeros: &str) -> Option<String> {
    let existing = profile_folders(save_root, zeros);
    let nand = save_root.parent()?.parent()?;
    let selected = std::fs::read(nand.join("system/save/8000000000000010/su/avators/profiles.dat")).ok().and_then(|raw| {
        let ids = profile_ids(&raw);
        let current = nand.parent().and_then(current_user).unwrap_or(0);
        ids.get(current).or(ids.first()).cloned()
    });
    if let Some(id) = selected {
        if let Some(same) = existing.iter().find(|n| n.eq_ignore_ascii_case(&id)) {
            return Some(same.clone());
        }
        if existing.is_empty() {
            return Some(id);
        }
    }
    existing.into_iter().next()
}

fn modified(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).ok()?.modified().ok()
}

/// What putting RomM's copy of one file in place did.
#[derive(Debug, PartialEq)]
enum Placed {
    /// Same content already there.
    Same,
    /// Written where there was no file.
    Created,
    /// Local file replaced (its old content copied to the backup folder).
    Replaced,
}

/// Puts RomM's `data` at `target`. A different local file is first copied to
/// `backup/<key>`, so nothing is lost if RomM's copy was the wrong one.
fn place(target: &Path, key: &str, data: &[u8], backup: &Path) -> Result<Placed, String> {
    let existing = std::fs::read(target).ok();
    if existing.as_deref() == Some(data) {
        return Ok(Placed::Same);
    }
    if let Some(old) = &existing {
        let copy = backup.join(key.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(dir) = copy.parent() {
            std::fs::create_dir_all(dir).map_err(err)?;
        }
        std::fs::write(&copy, old).map_err(err)?;
    }
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(err)?;
    }
    // Written beside then renamed: a crash never leaves half a save.
    let tmp = target.with_extension("aetherplay-tmp");
    std::fs::write(&tmp, data).map_err(err)?;
    std::fs::rename(&tmp, target).map_err(err)?;
    Ok(if existing.is_some() { Placed::Replaced } else { Placed::Created })
}

/// Downloads the latest RomM save for this game/emulator and puts it in place.
///
/// single_file_as: for emulators keeping one save file per game named after
/// the ROM (RetroArch: <rom stem>.srm), any raw save RomM holds for the game
/// (e.g. from its web player) is accepted too, and written under that name.
/// backup: folder receiving the local files RomM's copy replaces.
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
    backup: &Path,
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
    if data.is_empty() {
        return Err(format!("« {} » est vide sur RomM : copie locale gardée", save.file_name));
    }
    let mut keys = Vec::new();
    let (mut written, mut replaced) = (0, 0);
    let mut no_profile = 0;
    let mut write = |target: &Path, key: &str, data: &[u8]| -> Result<(), String> {
        match place(target, key, data, backup)? {
            Placed::Same => {}
            Placed::Created => written += 1,
            Placed::Replaced => {
                written += 1;
                replaced += 1;
            }
        }
        Ok(())
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
            write(&target, &key, &buf)?;
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
            write(&target, &key, &data)?;
            keys.push(key);
        }
    }
    index.entry(rom.id, emu).extend(keys);
    index.save()?;
    let message = if no_profile > 0 {
        format!("« {} » : profil de l'émulateur absent ici, restauration reportée à la fin de la partie", save.file_name)
    } else if written > 0 {
        let backed = if replaced > 0 { format!(", {replaced} copie(s) locale(s) mise(s) de côté dans {}", backup.display()) } else { String::new() };
        format!("« {} » ({}) restaurée : {written} fichier(s){backed}", save.file_name, save.emulator.unwrap_or_default())
    } else {
        format!("« {} » : copie locale déjà identique", save.file_name)
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

/// Uploads the game's save set if the session changed anything (`started`:
/// session start), or the whole set as it is (`None`: retry of a failed
/// upload). Returns `false` when there was nothing to upload.
#[allow(clippy::too_many_arguments)]
pub async fn upload(
    client: &Client,
    rom: &Rom,
    emu: &str,
    core: Option<&str>,
    dirs: &[PathBuf],
    rom_root: &Path,
    started: Option<SystemTime>,
    index: &mut Index,
) -> Result<bool, String> {
    let set = if let Some(started) = started {
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
        set
    } else {
        index.entry(rom.id, emu)
    };
    set.retain(|k| resolve(dirs, k).is_some_and(|p| p.is_file()));
    let keys: Vec<String> = set.iter().cloned().collect();
    index.save()?;
    if keys.is_empty() {
        return Ok(false);
    }

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
    let size = data.len() as u64;
    let stored = client
        .upload_save(SaveUpload { rom_id: rom.id, emulator: emu, core, slot: SLOT, file_name: &file_name }, data)
        .await?;
    // RomM answers with what it stored: an empty or truncated file is a failure.
    if let Some(stored) = stored
        && stored != size
    {
        return Err(format!("RomM a reçu {stored} octets au lieu de {size}"));
    }
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
    fn switch_profile_comes_from_profiles_dat() {
        let root = std::env::temp_dir().join(format!("aep-nand-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let save = root.join("nand/user/save");
        let sys = root.join("nand/system/save/8000000000000010/su/avators");
        std::fs::create_dir_all(&sys).unwrap();
        // Two users; the second is selected in the emulator.
        let mut raw = vec![0u8; 0x10 + 8 * 0xC8];
        raw[0x10..0x20].copy_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]);
        raw[0x10 + 0xC8..0x20 + 0xC8].copy_from_slice(&[0x11, 0x22, 0, 0, 0, 0, 0, 0xAB, 0x33, 0, 0, 0, 0, 0, 0, 0xCD]);
        std::fs::write(sys.join("profiles.dat"), &raw).unwrap();
        assert_eq!(profile_ids(&raw), ["00000000000000020000000000000001", "CD00000000000033AB00000000002211"]);
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(root.join("config/qt-config.ini"), "[UI]\ncurrent_user=0\n[System]\ncurrent_user=1\n").unwrap();
        // Fresh machine: no save folder yet, the selected profile is used.
        assert_eq!(switch_profile(&save, "0000000000000000").as_deref(), Some("CD00000000000033AB00000000002211"));
        // Its folder exists (lowercase here): used as is.
        std::fs::create_dir_all(save.join("0000000000000000/cd00000000000033ab00000000002211")).unwrap();
        assert_eq!(switch_profile(&save, "0000000000000000").as_deref(), Some("cd00000000000033ab00000000002211"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn server_copy_replaces_local_and_keeps_a_backup() {
        let root = std::env::temp_dir().join(format!("aep-place-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (target, backup) = (root.join("saves/Game.srm"), root.join("backup"));
        // No local file: created.
        assert_eq!(place(&target, "0/Game.srm", b"server v1", &backup).unwrap(), Placed::Created);
        // Same content: untouched, nothing backed up.
        assert_eq!(place(&target, "0/Game.srm", b"server v1", &backup).unwrap(), Placed::Same);
        assert!(!backup.exists());
        // Different local content: backed up, then replaced by the server copy.
        std::fs::write(&target, b"local progress").unwrap();
        assert_eq!(place(&target, "0/Game.srm", b"server v2", &backup).unwrap(), Placed::Replaced);
        assert_eq!(std::fs::read(&target).unwrap(), b"server v2");
        assert_eq!(std::fs::read(backup.join("0").join("Game.srm")).unwrap(), b"local progress");
        assert!(!target.with_extension("aetherplay-tmp").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn failed_uploads_stay_pending_across_restarts() {
        let file = std::env::temp_dir().join(format!("aep-index-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let mut index = Index::load(file.clone());
        index.entry(7, "retroarch").insert("0/Game.srm".into());
        index.set_pending(7, "retroarch", true).unwrap();
        let reloaded = Index::load(file.clone());
        assert!(reloaded.pending(7, "retroarch"));
        assert!(!reloaded.pending(7, "eden"));
        let mut reloaded = reloaded;
        reloaded.set_pending(7, "retroarch", false).unwrap();
        assert!(!Index::load(file.clone()).pending(7, "retroarch"));
        std::fs::remove_file(&file).unwrap();
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
