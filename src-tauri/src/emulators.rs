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
    /// Roaming/config dir: %APPDATA%, ~/Library/Application Support, ~/.config.
    pub config: PathBuf,
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
            .replace("{appdata}", &s(&self.appdata))
            .replace("{config}", &s(&self.config));
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
        if let Some(user) = self.portable_root(exe_path) {
            return vec![user.join("nand").join("user").join("save")];
        }
        self.saves.get(CURRENT_OS).into_iter().flatten().map(|t| vars.expand(t, exe_path, rom)).collect()
    }

    pub fn config_ini(&self, exe_path: &str, vars: &PathVars) -> Option<PathBuf> {
        if let Some(user) = self.portable_root(exe_path) {
            return Some(user.join("config").join("qt-config.ini"));
        }
        self.config_ini.get(CURRENT_OS).map(|t| vars.expand(t, exe_path, None))
    }

    /// yuzu-family emulators (Eden, Eglantine) run portable when a `user`
    /// folder sits next to the executable: their data and settings live there.
    fn portable_root(&self, exe_path: &str) -> Option<PathBuf> {
        if self.config_ini.is_empty() || exe_path.is_empty() {
            return None;
        }
        let user = executable(exe_path).ok()?.parent()?.join("user");
        user.is_dir().then_some(user)
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

pub const CURRENT_OS: &str = if cfg!(target_os = "macos") {
    "macos"
} else if cfg!(target_os = "linux") {
    "linux"
} else {
    "windows"
};

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

/// Adds `dir` to the external content folders of a yuzu-family
/// `qt-config.ini`, so updates/DLC stored there are loaded with the game
/// without installing them to NAND. The emulator keeps them in section `[UI]`
/// as the Qt array `Paths\external_content_dirs` (seen on Eglantine:
/// `Paths\external_content_dirs\size=…`, entries `…\<n>\path=…`, like
/// `Paths\gamedirs`). Entries an older AetherPlay wrote in a `[Paths]`
/// section, which the emulator never read, are dropped.
pub fn register_external_content(ini: &Path, dir: &Path) -> Result<(), String> {
    const KEY: &str = r"Paths\external_content_dirs\";
    let dir = dir.to_string_lossy().replace('\\', "/");
    let text = std::fs::read_to_string(ini).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let section_of = |lines: &[String], name: &str| {
        let start = lines.iter().position(|l| l.trim() == name)?;
        let end = lines[start + 1..].iter().position(|l| l.starts_with('[')).map_or(lines.len(), |i| start + 1 + i);
        Some((start, end))
    };

    if let Some((start, end)) = section_of(&lines, "[Paths]") {
        let kept: Vec<String> =
            lines[start + 1..end].iter().filter(|l| !l.starts_with(r"external_content_dirs\")).cloned().collect();
        if kept.iter().all(|l| l.trim().is_empty()) {
            lines.drain(start..end);
        } else {
            lines.splice(start + 1..end, kept);
        }
    }

    let (start, end) = match section_of(&lines, "[UI]") {
        Some(s) => s,
        None => {
            lines.push("[UI]".into());
            (lines.len() - 1, lines.len())
        }
    };
    let section = &lines[start + 1..end];
    let size_key = format!("{KEY}size=");
    let already = section
        .iter()
        .any(|l| l.split_once('=').is_some_and(|(k, v)| k.starts_with(KEY) && k.ends_with(r"\path") && v == dir));
    if !already {
        let size = section
            .iter()
            .find_map(|l| l.strip_prefix(size_key.as_str()))
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let new_size = size + 1;
        let mut section: Vec<String> = section.iter().filter(|l| !l.starts_with(&size_key)).cloned().collect();
        section.push(format!(r"{KEY}{new_size}\path={dir}"));
        section.push(format!("{size_key}{new_size}"));
        lines.splice(start + 1..end, section);
    }
    if let Some(parent) = ini.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(ini, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

/// On macOS users pick the `.app` bundle; run the binary inside it.
fn executable(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    if p.extension().is_some_and(|e| e == "app") {
        return bundle_executable(p);
    }
    Ok(p.to_path_buf())
}

/// The binary a `.app` runs: `CFBundleExecutable` from Info.plist, else the
/// file named after the bundle. `Contents/MacOS` may hold helpers and text
/// files too (Eglantine ships `eden-cli` and `nx2_switches.txt` next to it).
fn bundle_executable(app: &Path) -> Result<PathBuf, String> {
    let macos = app.join("Contents/MacOS");
    let declared = std::fs::read_to_string(app.join("Contents/Info.plist")).ok().and_then(|plist| {
        let re = regex::Regex::new(r"<key>CFBundleExecutable</key>\s*<string>([^<]+)</string>").ok()?;
        Some(re.captures(&plist)?[1].trim().to_string())
    });
    let stem = app.file_stem().map(|s| s.to_string_lossy().into_owned());
    for name in declared.into_iter().chain(stem) {
        let f = macos.join(&name);
        if f.is_file() {
            return Ok(f);
        }
    }
    std::fs::read_dir(&macos)
        .map_err(|e| format!("{}: {e}", macos.display()))?
        .filter_map(Result::ok)
        .map(|d| d.path())
        .filter(|f| f.is_file())
        .find(|f| {
            let n = f.file_name().unwrap_or_default().to_string_lossy();
            !n.starts_with('.') && !n.contains('.')
        })
        .ok_or_else(|| format!("Aucun exécutable dans {}", macos.display()))
}

#[cfg(test)]
mod bundle_tests {
    #[test]
    fn picks_declared_executable_over_helpers() {
        let app = std::env::temp_dir().join(format!("aep-test-{}/eglantine.app", std::process::id()));
        let macos = app.join("Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        for f in ["nx2_switches.txt", "._nx2_switches.txt", "eden-cli", "eglantine"] {
            std::fs::write(macos.join(f), b"x").unwrap();
        }
        std::fs::write(
            app.join("Contents/Info.plist"),
            "<dict>\n<key>CFBundleExecutable</key>\n  <string>eglantine</string>\n</dict>",
        )
        .unwrap();
        assert_eq!(super::bundle_executable(&app).unwrap(), macos.join("eglantine"));
        let _ = std::fs::remove_dir_all(app.parent().unwrap());
    }
}

fn core_path(exe: &Path, core: &str) -> PathBuf {
    if cfg!(target_os = "macos") {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home)
            .join("Library/Application Support/RetroArch/cores")
            .join(format!("{core}_libretro.dylib"))
    } else {
        let ext = if cfg!(windows) { "dll" } else { "so" };
        exe.parent().unwrap_or(Path::new(".")).join("cores").join(format!("{core}_libretro.{ext}"))
    }
}

/// Builds the launch command for `rom` on `platform`.
pub fn command(
    emu: &Emulator,
    path: &str,
    platform: &str,
    rom: &Path,
    fullscreen: bool,
    save_dir: Option<&Path>,
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
    // `{saves}` pins the emulator's save folder to the one the sync watches.
    let saves = save_dir.map(|d| {
        let _ = std::fs::create_dir_all(d);
        d.to_string_lossy().into_owned()
    });
    let mut cmd = tokio::process::Command::new(&exe);
    if fullscreen {
        cmd.args(&emu.fullscreen_args);
    }
    cmd.args(emu.args.iter().map(|a| {
        a.replace("{rom}", &rom).replace("{core}", &core).replace("{saves}", saves.as_deref().unwrap_or("."))
    }));
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    // Own process group, so quitting also ends what it spawned (an AppImage
    // runs the emulator as a child of its runtime).
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(target_os = "linux")]
    for (key, value) in appimage_env_cleanup(std::env::vars()) {
        match value {
            Some(v) => cmd.env(key, v),
            None => cmd.env_remove(key),
        };
    }
    Ok(cmd)
}

/// AetherPlay's AppImage points library / GTK / GStreamer variables into its
/// own image; inherited, they break the emulators (often AppImages too).
/// Returns the variables to change: entries inside the image are dropped,
/// and a variable left empty is removed.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn appimage_env_cleanup(vars: impl Iterator<Item = (String, String)>) -> Vec<(String, Option<String>)> {
    let vars: Vec<(String, String)> = vars.collect();
    let Some(appdir) = vars.iter().find(|(k, _)| k == "APPDIR").map(|(_, v)| v.clone()).filter(|v| !v.is_empty())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, value) in vars {
        if ["APPDIR", "APPIMAGE", "ARGV0", "OWD"].contains(&key.as_str()) {
            out.push((key, None));
        } else if value.contains(&appdir) {
            let kept: Vec<&str> = value.split(':').filter(|p| !p.is_empty() && !p.starts_with(&appdir)).collect();
            out.push((key, (!kept.is_empty()).then(|| kept.join(":"))));
        }
    }
    out
}

#[cfg(test)]
mod appimage_tests {
    #[test]
    fn strips_appimage_paths_from_child_environment() {
        let vars = [
            ("APPDIR", "/tmp/.mount_Aether"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_Aether/usr/lib"),
            ("XDG_DATA_DIRS", "/tmp/.mount_Aether/usr/share:/usr/local/share:/usr/share"),
            ("HOME", "/home/deck"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let mut out = super::appimage_env_cleanup(vars.into_iter());
        out.sort();
        assert_eq!(
            out,
            vec![
                ("APPDIR".into(), None),
                ("LD_LIBRARY_PATH".into(), None),
                ("XDG_DATA_DIRS".into(), Some("/usr/local/share:/usr/share".into())),
            ]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_content_is_added_once_and_keeps_other_settings() {
        let ini = std::env::temp_dir().join(format!("romm-qt-{}.ini", std::process::id()));
        std::fs::write(
            &ini,
            "[UI]\na=1\nPaths\\external_content_dirs\\size=1\nPaths\\external_content_dirs\\1\\path=C:/old\n\n\
             [Paths]\nexternal_content_dirs\\1\\path=C:/x\nexternal_content_dirs\\size=1\nfoo=bar\n[Other]\nb=2\n",
        )
        .unwrap();
        register_external_content(&ini, Path::new("C:\\roms\\switch\\Game")).unwrap();
        register_external_content(&ini, Path::new("C:\\roms\\switch\\Game")).unwrap();
        let text = std::fs::read_to_string(&ini).unwrap();
        std::fs::remove_file(&ini).unwrap();
        assert_eq!(text.matches("C:/roms/switch/Game").count(), 1);
        assert!(text.contains("Paths\\external_content_dirs\\2\\path=C:/roms/switch/Game"));
        assert!(text.contains("Paths\\external_content_dirs\\size=2") && !text.contains("size=1"));
        // The old misplaced entries are gone, other settings kept.
        assert!(text.contains("C:/old") && !text.contains("C:/x") && text.contains("[Paths]\nfoo=bar"));
        assert!(text.contains("[Other]\nb=2") && text.contains("[UI]\na=1"));
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
