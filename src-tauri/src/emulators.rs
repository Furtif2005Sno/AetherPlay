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
}

/// Values substituted into `saves` paths.
pub struct PathVars {
    pub home: PathBuf,
    pub documents: PathBuf,
    pub appdata: PathBuf,
}

impl Emulator {
    /// Resolved save directories for this emulator on the current OS.
    pub fn save_dirs(&self, exe_path: &str, rom: &Path, vars: &PathVars) -> Vec<PathBuf> {
        let exe = executable(exe_path).unwrap_or_else(|_| PathBuf::from(exe_path));
        let exe_dir = exe.parent().unwrap_or(Path::new("."));
        let rom_dir = rom.parent().unwrap_or(Path::new("."));
        let s = |p: &Path| p.to_string_lossy().into_owned();
        self.saves
            .get(CURRENT_OS)
            .into_iter()
            .flatten()
            .map(|t| {
                PathBuf::from(
                    t.replace("{exe_dir}", &s(exe_dir))
                        .replace("{rom_dir}", &s(rom_dir))
                        .replace("{home}", &s(&vars.home))
                        .replace("{documents}", &s(&vars.documents))
                        .replace("{appdata}", &s(&vars.appdata)),
                )
            })
            .collect()
    }

    pub fn core(&self, platform: &str) -> Option<&str> {
        self.cores.get(platform).map(String::as_str)
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

/// First configured emulator (catalog order = priority) able to run `platform`.
pub fn resolve<'a>(
    platform: &str,
    paths: &'a HashMap<String, String>,
) -> Option<(&'static Emulator, &'a String)> {
    catalog()
        .iter()
        .filter(|e| e.platforms.iter().any(|p| p == platform))
        .find_map(|e| paths.get(&e.id).filter(|p| !p.is_empty()).map(|p| (e, p)))
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
    cmd.args(emu.args.iter().map(|a| a.replace("{rom}", &rom).replace("{core}", &core)));
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    Ok(cmd)
}
