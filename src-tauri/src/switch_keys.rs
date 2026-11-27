//! Switch decryption keys (`prod.keys`, `title.keys`) for the yuzu-family
//! emulators (Eden, Eglantine). They come from the user's own console; without
//! them games do not start ("decryption keys are missing"). AetherPlay
//! installs a file the user picks into every Switch emulator, and before a
//! launch gives an emulator without keys those of another one on this computer.

use std::path::{Path, PathBuf};

const FILES: &[&str] = &["prod.keys", "title.keys"];

/// `<data>/keys` from the emulator's save root `<data>/nand/user/save`.
pub fn keys_dir(save_root: &Path) -> Option<PathBuf> {
    Some(save_root.parent()?.parent()?.parent()?.join("keys"))
}

pub fn has_keys(dir: &Path) -> bool {
    dir.join("prod.keys").is_file()
}

/// A keys file: `name = hex` lines, with the header key for prod.keys.
fn looks_like_keys(text: &str, prod: bool) -> bool {
    let pairs = text
        .lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(k, v)| !k.trim().is_empty() && v.trim().chars().all(|c| c.is_ascii_hexdigit()) && !v.trim().is_empty())
        .count();
    pairs > 0 && (!prod || text.contains("header_key"))
}

/// Installs `src` (the user's prod.keys, any file name) and the `title.keys`
/// next to it, if any, into every folder of `dirs`. Returns how many files
/// were copied per folder.
pub fn import(src: &Path, dirs: &[PathBuf]) -> Result<usize, String> {
    let prod = std::fs::read_to_string(src).map_err(|e| format!("Lecture de {} : {e}", src.display()))?;
    if !looks_like_keys(&prod, true) {
        return Err("Ce fichier ne ressemble pas à un prod.keys (il doit contenir header_key = …).".into());
    }
    let title = src
        .parent()
        .map(|d| d.join("title.keys"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .filter(|t| looks_like_keys(t, false));
    for dir in dirs {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("prod.keys"), &prod).map_err(|e| e.to_string())?;
        if let Some(t) = &title {
            std::fs::write(dir.join("title.keys"), t).map_err(|e| e.to_string())?;
        }
    }
    Ok(if title.is_some() { 2 } else { 1 })
}

/// Before a launch: folders of `dirs` without prod.keys get the keys of the
/// first one that has them. Returns the folders filled.
pub fn share(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let Some(from) = dirs.iter().find(|d| has_keys(d)) else { return Vec::new() };
    let mut filled = Vec::new();
    for dir in dirs.iter().filter(|d| !has_keys(d)) {
        let ok = std::fs::create_dir_all(dir).is_ok()
            && FILES.iter().filter(|f| from.join(f).is_file()).all(|f| std::fs::copy(from.join(f), dir.join(f)).is_ok());
        if ok {
            filled.push(dir.clone());
        }
    }
    filled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_imported_then_shared() {
        let root = std::env::temp_dir().join(format!("aep-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (eden, eglantine, src) = (root.join("eden/keys"), root.join("eglantine/keys"), root.join("dump"));
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("bad.keys"), "hello").unwrap();
        assert!(import(&src.join("bad.keys"), std::slice::from_ref(&eden)).is_err());
        std::fs::write(src.join("prod.keys"), "header_key = 00ff\nmaster_key_00 = abcd\n").unwrap();
        std::fs::write(src.join("title.keys"), "0100000000010000 = 1234\n").unwrap();
        assert_eq!(import(&src.join("prod.keys"), std::slice::from_ref(&eglantine)).unwrap(), 2);
        assert!(!has_keys(&eden));
        assert_eq!(share(&[eden.clone(), eglantine.clone()]), vec![eden.clone()]);
        assert!(eden.join("prod.keys").is_file() && eden.join("title.keys").is_file());
        assert_eq!(keys_dir(Path::new("/a/eden/nand/user/save")), Some(PathBuf::from("/a/eden/keys")));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
