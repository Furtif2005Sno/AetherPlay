//! Emulator catalog, resolution (platform -> emulator) and process launch.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::LazyLock,
};

#[derive(Serialize, Deserialize, Clone)]
pub struct Emulator {
    pub id: String,
    pub name: String,
    pub platforms: Vec<String>,
    pub args: Vec<String>,
    pub os: Vec<String>,
    #[serde(default)]
    pub experimental: bool,
    /// RetroArch only: platform slug -> libretro core name.
    #[serde(default)]
    pub cores: HashMap<String, String>,
    /// OS -> save directories, with `{exe_dir}`, `{rom_dir}`, `{home}`,
    /// `{documents}` and `{appdata}` placeholders. No entry = no save sync.
    #[serde(default)]
    pub saves: HashMap<String, Vec<String>>,
    /// OS -> Qt `qt-config.ini` of yuzu-family emulators (Eden, Eglantine), used
    /// to register folders holding Switch updates/DLC as external content.
    #[serde(default)]
    pub config_ini: HashMap<String, String>,
    /// Arguments that start the emulator fullscreen.
    #[serde(default)]
    pub fullscreen_args: Vec<String>,
}

/// Values substituted into catalog paths.
pub struct PathVars {
    pub home: PathBuf,
    pub documents: PathBuf,
    pub appdata: PathBuf,
}

impl PathVars {
    /// Expands `{exe_dir}`, `{rom_dir}`, `{home}`, `{documents}`, `{appdata}`.
    /// `rom` may be `None` to leave `{rom_dir}` as is (for display).
    pub fn expand(&self, template: &str, exe_path: &str, rom: Option<&Path>) -> PathBuf {
        let exe = executable(exe_path).unwrap_or_else(|_| PathBuf::from(exe_path));
        let s = |p: &Path| p.to_string_lossy().into_owned();
        let mut out = template
            .replace("{exe_dir}", &s(exe.parent().unwrap_or(Path::new("."))))
            .replace("{home}", &s(&self.home))
            .replace("{documents}", &s(&self.documents))
            .replace("{appdata}", &s(&self.appdata));
        if let Some(rom) = rom {
            out = out.replace("{rom_dir}", &s(rom.parent().unwrap_or(Path::new("."))));
        }
        PathBuf::from(out)
    }
}

impl Emulator {
    /// Save directories on the current OS: the user's override, else the catalog default.
    pub fn save_dirs(
        &self,
        exe_path: &str,
        rom: Option<&Path>,
        vars: &PathVars,
        overrides: &HashMap<String, String>,
    ) -> Vec<PathBuf> {
        if let Some(dir) = overrides.get(&self.id).filter(|d| !d.is_empty()) {
            return vec![vars.expand(dir, exe_path, rom)];
        }
        self.saves.get(CURRENT_OS).into_iter().flatten().map(|t| vars.expand(t, exe_path, rom)).collect()
    }

    pub fn config_ini(&self, exe_path: &str, vars: &PathVars) -> Option<PathBuf> {
        self.config_ini.get(CURRENT_OS).map(|t| vars.expand(t, exe_path, None))
    }

    pub fn core(&self, platform: &str) -> Option<&str> {
        self.cores.get(platform).map(String::as_str)
    }

    /// Expected location of the libretro core for `platform`, if one is needed.
    pub fn core_file(&self, exe_path: &str, platform: &str) -> Option<PathBuf> {
        let core = self.cores.get(platform)?;
        Some(core_path(&executable(exe_path).ok()?, core))
    }
}

pub const CURRENT_OS: &str = if cfg!(target_os = "macos") { "macos" } else { "windows" };

static CATALOG: LazyLock<Vec<Emulator>> = LazyLock::new(|| {
    serde_json::from_str::<Vec<Emulator>>(include_str!("../catalog.json"))
        .expect("catalog.json invalide")
        .into_iter()
        .filter(|e| e.os.iter().any(|o| o == CURRENT_OS))
        .collect()
});

pub fn catalog() -> &'static [Emulator] {
    &CATALOG
}

/// Emulator for `platform`: the user's choice if it is configured, otherwise
/// the first configured one in catalog order.
pub fn resolve<'a>(
    platform: &str,
    paths: &'a HashMap<String, String>,
    preferred: &HashMap<String, String>,
) -> Option<(&'static Emulator, &'a String)> {
    let candidates = || catalog().iter().filter(|e| e.platforms.iter().any(|p| p == platform));
    let configured = |e: &'static Emulator| paths.get(&e.id).filter(|p| !p.is_empty()).map(|p| (e, p));
    preferred
        .get(platform)
        .and_then(|id| candidates().find(|e| &e.id == id))
        .and_then(configured)
        .or_else(|| candidates().find_map(configured))
}

/// Adds `dir` to the `external_content_dirs` array of a yuzu-family
/// `qt-config.ini` (section `[Paths]`), so updates/DLC stored there are
/// loaded without installing them to NAND.
pub fn register_external_content(ini: &Path, dir: &Path) -> Result<(), String> {
    let dir = dir.to_string_lossy().replace('\\', "/");
    let text = std::fs::read_to_string(ini).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let start = lines.iter().position(|l| l.trim() == "[Paths]");
    let start = match start {
        Some(i) => i,
        None => {
            lines.push("[Paths]".into());
            lines.len() - 1
        }
    };
    let end = lines[start + 1..].iter().position(|l| l.starts_with('[')).map_or(lines.len(), |i| start + 1 + i);
    let section = &lines[start + 1..end];
    let already = section.iter().any(|l| {
        l.split_once('=')
            .is_some_and(|(k, v)| k.starts_with("external_content_dirs\\") && k.ends_with("\\path") && v == dir)
    });
    if already {
        return Ok(());
    }
    let size = section
        .iter()
        .find_map(|l| l.strip_prefix("external_content_dirs\\size="))
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let new_size = size + 1;
    let mut section: Vec<String> = section.iter().filter(|l| !l.starts_with("external_content_dirs\\size=")).cloned().collect();
    section.push(format!("external_content_dirs\\{new_size}\\path={dir}"));
    section.push(format!("external_content_dirs\\size={new_size}"));
    lines.splice(start + 1..end, section);
    if let Some(parent) = ini.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(ini, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

/// On macOS users pick the `.app` bundle; run the binary inside it.
fn executable(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    if p.extension().is_some_and(|e| e == "app") {
        let macos = p.join("Contents/MacOS");
        return std::fs::read_dir(&macos)
            .map_err(|e| format!("{}: {e}", macos.display()))?
            .filter_map(Result::ok)
            .map(|d| d.path())
            .find(|f| f.is_file())
            .ok_or_else(|| format!("Aucun exécutable dans {}", macos.display()));
    }
    Ok(p.to_path_buf())
}

fn core_path(exe: &Path, core: &str) -> PathBuf {
    if cfg!(target_os = "macos") {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home)
            .join("Library/Application Support/RetroArch/cores")
            .join(format!("{core}_libretro.dylib"))
    } else {
        exe.parent().unwrap_or(Path::new(".")).join("cores").join(format!("{core}_libretro.dll"))
    }
}

/// Builds the launch command for `rom` on `platform`.
pub fn command(
    emu: &Emulator,
    path: &str,
    platform: &str,
    rom: &Path,
    fullscreen: bool,
) -> Result<tokio::process::Command, String> {
    let exe = executable(path)?;
    let core = match emu.cores.get(platform) {
        Some(c) => {
            let p = core_path(&exe, c);
            if !p.exists() {
                return Err(format!("Core RetroArch manquant : {}", p.display()));
            }
            p.to_string_lossy().into_owned()
        }
        None => String::new(),
    };
    let rom = rom.to_string_lossy();
    let mut cmd = tokio::process::Command::new(&exe);
    if fullscreen {
        cmd.args(&emu.fullscreen_args);
    }
    cmd.args(emu.args.iter().map(|a| a.replace("{rom}", &rom).replace("{core}", &core)));
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_content_is_added_once_and_keeps_other_settings() {
        let ini = std::env::temp_dir().join(format!("romm-qt-{}.ini", std::process::id()));
        std::fs::write(&ini, "[UI]\na=1\n\n[Paths]\nexternal_content_dirs\\1\\path=C:/old\nexternal_content_dirs\\size=1\nfoo=bar\n[Other]\nb=2\n").unwrap();
        register_external_content(&ini, Path::new("C:\\roms\\switch\\Game")).unwrap();
        register_external_content(&ini, Path::new("C:\\roms\\switch\\Game")).unwrap();
        let text = std::fs::read_to_string(&ini).unwrap();
        std::fs::remove_file(&ini).unwrap();
        assert_eq!(text.matches("C:/roms/switch/Game").count(), 1);
        assert!(text.contains("external_content_dirs\\2\\path=C:/roms/switch/Game"));
        assert!(text.contains("external_content_dirs\\size=2") && !text.contains("size=1"));
        assert!(text.contains("foo=bar") && text.contains("[Other]\nb=2") && text.contains("[UI]\na=1"));
    }

    #[test]
    fn preferred_emulator_wins_only_when_configured() {
        let mut paths = HashMap::from([("duckstation".to_string(), "d.exe".to_string()), ("retroarch".to_string(), "r.exe".to_string())]);
        let pref = HashMap::from([("psx".to_string(), "retroarch".to_string())]);
        assert_eq!(resolve("psx", &paths, &HashMap::new()).unwrap().0.id, "duckstation");
        assert_eq!(resolve("psx", &paths, &pref).unwrap().0.id, "retroarch");
        paths.remove("retroarch");
        assert_eq!(resolve("psx", &paths, &pref).unwrap().0.id, "duckstation");
    }
}
