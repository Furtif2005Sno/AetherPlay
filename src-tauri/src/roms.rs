//! Multi-file ROMs: RomM serves them as one zip. We extract it into a folder
//! and pick the file to hand to the emulator.

use std::path::{Path, PathBuf};

/// Written once extraction succeeded, so an interrupted one is redone.
pub const MARKER: &str = ".romm-complete";

/// Launchable extensions, most preferred first (playlists, then disc
/// descriptors, then images, then single-file formats).
const PRIORITY: &[&str] = &[
    "m3u", "cue", "gdi", "ccd", "mds", "chd", "iso", "cso", "rvz", "gcm", "wbfs", "wua", "wud", "rpx",
    "xci", "nsp", "xcz", "nsz", "3ds", "cci", "cia", "nds", "pbp", "elf", "bin", "img",
];
/// Base game + updates/DLC share these: see `switch_role`.
const LARGEST_WINS: &[&str] = &["xci", "nsp", "xcz", "nsz"];

/// What a Switch file is, from its name: 0 base game, 1 unknown, 2 update or
/// DLC. Dump names carry the title id (`[0100ABCD12340000]`: base ids end in
/// 000, updates in 800, DLC in their index) and often a tag
/// (`[UPD]`, `[DLC]`, `[v131072]` for an update version).
fn switch_role(p: &Path) -> u8 {
    let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let ids: Vec<&str> = name
        .split(|c: char| !c.is_ascii_hexdigit())
        .filter(|w| w.len() == 16 && w.starts_with("01"))
        .collect();
    if let Some(id) = ids.first() {
        return if id.ends_with("000") { 0 } else { 2 };
    }
    let words: Vec<&str> = name.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    let update_version = words.iter().any(|w| w.strip_prefix('v').is_some_and(|v| !v.is_empty() && v != "0" && v.chars().all(|c| c.is_ascii_digit())));
    if update_version || words.iter().any(|w| matches!(*w, "upd" | "update" | "dlc" | "patch" | "addon")) {
        return 2;
    }
    if words.iter().any(|w| matches!(*w, "base" | "v0")) { 0 } else { 1 }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub fn extract(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(dest);
    std::fs::create_dir_all(dest).map_err(err)?;
    let file = std::fs::File::open(zip_path).map_err(err)?;
    // `extract` rejects entries escaping `dest`.
    zip::ZipArchive::new(file).map_err(err)?.extract(dest).map_err(err)?;
    std::fs::write(dest.join(MARKER), b"").map_err(err)
}

fn files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() && depth < 6 {
            files(&p, depth + 1, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

fn ext(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase()
}

/// Picks the file to launch in an extracted multi-file ROM.
pub fn launch_target(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let mut all = Vec::new();
    files(dir, 0, &mut all);
    all.sort();

    // PS3 disc/folder games: RPCS3 boots the EBOOT.BIN.
    if let Some(eboot) = all.iter().find(|p| p.ends_with("PS3_GAME/USRDIR/EBOOT.BIN")) {
        return Ok(eboot.clone());
    }

    // Multi-disc without playlist: write one so emulators can swap discs.
    let discs: Vec<&PathBuf> = all.iter().filter(|p| matches!(ext(p).as_str(), "cue" | "chd" | "gdi")).collect();
    if discs.len() > 1 && !all.iter().any(|p| ext(p) == "m3u") {
        let m3u = dir.join(format!("{name}.m3u"));
        let body: Vec<String> = discs
            .iter()
            .filter_map(|p| p.strip_prefix(dir).ok())
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        std::fs::write(&m3u, body.join("\n")).map_err(err)?;
        return Ok(m3u);
    }

    for wanted in PRIORITY {
        let mut hits = all.iter().filter(|p| ext(p) == *wanted);
        let pick = if LARGEST_WINS.contains(wanted) {
            // The file named as the base game, else the biggest (base games
            // are usually larger than their updates and DLC).
            hits.min_by_key(|p| (switch_role(p), std::cmp::Reverse(p.metadata().map(|m| m.len()).unwrap_or(0))))
        } else {
            hits.next()
        };
        if let Some(p) = pick {
            return Ok(p.clone());
        }
    }
    // Unknown layout: hand the folder itself to the emulator.
    Ok(dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str, files: &[(&str, usize)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("romm-multi-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (f, size) in files {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, vec![0u8; *size]).unwrap();
        }
        root
    }

    #[test]
    fn single_disc_prefers_cue_over_bin() {
        let d = tree("cue", &[("Game.bin", 10), ("Game.cue", 1)]);
        assert_eq!(launch_target(&d, "Game").unwrap(), d.join("Game.cue"));
    }

    #[test]
    fn multi_disc_gets_a_playlist() {
        let d = tree("m3u", &[("G (Disc 2).cue", 1), ("G (Disc 1).cue", 1), ("G (Disc 1).bin", 9)]);
        let t = launch_target(&d, "G").unwrap();
        assert_eq!(t, d.join("G.m3u"));
        assert_eq!(std::fs::read_to_string(t).unwrap(), "G (Disc 1).cue\nG (Disc 2).cue");
    }

    #[test]
    fn switch_picks_base_game_and_ps3_picks_eboot() {
        let d = tree("nsp", &[("Base.nsp", 100), ("Update.nsp", 10)]);
        assert_eq!(launch_target(&d, "x").unwrap(), d.join("Base.nsp"));
        // An update bigger than the base game: the names decide.
        let d = tree(
            "nsp-ids",
            &[
                ("LEGO Horizon [0100D8A01C2D2000][v0].nsp", 50),
                ("LEGO Horizon [0100D8A01C2D2800][v262144].nsp", 90),
                ("LEGO Horizon DLC [0100D8A01C2D3001].nsp", 5),
                ("LEGO Horizon DLC [0100D8A01C2D3002].nsp", 5),
            ],
        );
        assert_eq!(launch_target(&d, "x").unwrap(), d.join("LEGO Horizon [0100D8A01C2D2000][v0].nsp"));
        let d = tree("nsp-tags", &[("Game [UPD].nsp", 90), ("Game.nsp", 50), ("Game [DLC].nsp", 5)]);
        assert_eq!(launch_target(&d, "x").unwrap(), d.join("Game.nsp"));
        let d = tree("ps3", &[("G/PS3_GAME/USRDIR/EBOOT.BIN", 5), ("G/PS3_DISC.SFB", 1)]);
        assert_eq!(launch_target(&d, "x").unwrap(), d.join("G/PS3_GAME/USRDIR/EBOOT.BIN"));
    }
}
