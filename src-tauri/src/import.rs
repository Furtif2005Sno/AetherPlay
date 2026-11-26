//! Importing a Switch save made elsewhere (console dump, NX2Apple, another
//! emulator) into a yuzu-family emulator (Eden, Eglantine).
//!
//! Those emulators keep a game's save in
//! `<save root>/<zeros>/<user id>/<title id>/`. The archive is searched for a
//! folder named like a title id (16 hex digits); its content replaces the
//! content of the matching folder the emulator created on the game's first
//! launch. The previous content is moved aside, never deleted.

use serde::Serialize;
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
};

#[derive(Serialize, Debug)]
pub struct ImportReport {
    pub title_id: String,
    pub destinations: Vec<String>,
    pub files: usize,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn is_title_id(s: &str) -> bool {
    s.len() == 16 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Archive noise: macOS resource forks, shader caches, temporary copies.
fn is_junk(rel: &Path) -> bool {
    rel.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "__MACOSX" || s.starts_with("._") || s.starts_with("pso_") || s.ends_with(".work") || s == ".DS_Store"
    })
}

/// Folders named `title_id` under `root` (up to 4 levels deep).
fn find_title_dirs(root: &Path, title_id: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut level = vec![root.to_path_buf()];
    for _ in 0..4 {
        let mut next = Vec::new();
        for dir in &level {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = e.path();
                if !p.is_dir() {
                    continue;
                }
                if p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(title_id)) {
                    found.push(p);
                } else {
                    next.push(p);
                }
            }
        }
        level = next;
    }
    found
}

/// Imports `zip_path` into the emulator save roots `roots`.
pub fn import_switch_save(zip_path: &Path, roots: &[PathBuf]) -> Result<ImportReport, String> {
    let file = std::fs::File::open(zip_path).map_err(err)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Ce n'est pas un zip lisible : {e}"))?;

    // Locate the title folder: the first path component that looks like a title id.
    let mut prefix: Option<(PathBuf, String)> = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(err)?;
        let Some(path) = entry.enclosed_name() else { continue };
        if is_junk(&path) {
            continue;
        }
        let mut acc = PathBuf::new();
        for c in path.components() {
            let Component::Normal(name) = c else { break };
            acc.push(name);
            let name = name.to_string_lossy();
            if is_title_id(&name) && acc != path {
                prefix = Some((acc.clone(), name.to_uppercase()));
                break;
            }
        }
        if prefix.is_some() {
            break;
        }
    }
    let (prefix, title_id) = prefix.ok_or(
        "Aucun dossier de jeu trouvé dans le zip (un dossier nommé comme 0400A3A01EE4C000 est attendu).",
    )?;

    let destinations: Vec<PathBuf> = roots.iter().flat_map(|r| find_title_dirs(r, &title_id)).collect();
    if destinations.is_empty() {
        return Err(format!(
            "Le dossier de sauvegarde du jeu {title_id} n'existe pas encore. Lance le jeu une fois depuis AetherPlay, quitte-le, puis recommence l'import."
        ));
    }

    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut files = 0;
    for dest in &destinations {
        // Keep the emulator's own save aside rather than deleting it.
        let backup = dest.with_file_name(format!("{title_id}.avant-import-{stamp}"));
        std::fs::rename(dest, &backup).map_err(|e| format!("Mise de côté de l'ancienne sauvegarde : {e}"))?;
        std::fs::create_dir_all(dest).map_err(err)?;
        files = 0;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(err)?;
            let Some(path) = entry.enclosed_name() else { continue };
            let Ok(rel) = path.strip_prefix(&prefix) else { continue };
            if rel.as_os_str().is_empty() || is_junk(rel) {
                continue;
            }
            let target = dest.join(rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&target).map_err(err)?;
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(err)?;
            }
            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut buf).map_err(err)?;
            std::fs::write(&target, buf).map_err(err)?;
            files += 1;
        }
    }
    Ok(ImportReport {
        title_id,
        destinations: destinations.iter().map(|d| d.to_string_lossy().into_owned()).collect(),
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn imports_title_folder_and_skips_junk() {
        let base = std::env::temp_dir().join(format!("romm-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        // Emulator side: the folder created on first launch, with an old save.
        let title = base.join("nand/user/save/0000000000000000/ABCDEF/0400A3A01EE4C000");
        std::fs::create_dir_all(&title).unwrap();
        std::fs::write(title.join("old.bin"), b"old").unwrap();

        // Archive shaped like the NX2Apple export.
        let zip_path = base.join("save.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in [
            ("save/0400A3A01EE4C000/G00/STGDAT01.BIN", &b"game"[..]),
            ("save/0400A3A01EE4C000/S00/SYDAT.BIN", b"sys"),
            ("save/0400A3A01EE4C000.work/G00/STGDAT01.BIN", b"tmp"),
            ("save/pso_archive/00083.mtl4a", b"shader"),
            ("__MACOSX/save/0400A3A01EE4C000/G00/._STGDAT01.BIN", b"fork"),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();

        let report = import_switch_save(&zip_path, &[base.join("nand/user/save")]).unwrap();
        assert_eq!(report.title_id, "0400A3A01EE4C000");
        assert_eq!(report.files, 2);
        assert_eq!(std::fs::read(title.join("G00/STGDAT01.BIN")).unwrap(), b"game");
        assert_eq!(std::fs::read(title.join("S00/SYDAT.BIN")).unwrap(), b"sys");
        assert!(!title.join("old.bin").exists());
        let backups = std::fs::read_dir(title.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("avant-import"))
            .count();
        assert_eq!(backups, 1);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn explains_when_the_game_was_never_launched() {
        let base = std::env::temp_dir().join(format!("romm-import-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("root")).unwrap();
        let zip_path = base.join("save.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        zip.start_file("save/0400A3A01EE4C000/G00/A.BIN", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"x").unwrap();
        zip.finish().unwrap();
        let e = import_switch_save(&zip_path, &[base.join("root")]).unwrap_err();
        assert!(e.contains("Lance le jeu une fois"));
        std::fs::remove_dir_all(&base).unwrap();
    }
}

#[cfg(test)]
mod real {
    /// `IMPORT_ZIP=<zip> cargo test import_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn import_real() {
        let zip = std::env::var("IMPORT_ZIP").expect("IMPORT_ZIP");
        let root = std::env::temp_dir().join("romm-import-real");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("0000000000000000/USER/0400A3A01EE4C000")).unwrap();
        let report = super::import_switch_save(std::path::Path::new(&zip), std::slice::from_ref(&root));
        println!("{report:?}");
        for e in walk(&root.join("0000000000000000/USER/0400A3A01EE4C000")) {
            println!("  {e}");
        }
        std::fs::remove_dir_all(&root).unwrap();
        assert!(report.is_ok());
    }

    fn walk(dir: &std::path::Path) -> Vec<String> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() { out.extend(walk(&p)) } else { out.push(format!("{} ({} o)", p.display(), p.metadata().map(|m| m.len()).unwrap_or(0))) }
        }
        out
    }
}
