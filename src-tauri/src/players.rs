//! RetroTogether: each connected controller becomes a player in the
//! emulator's own configuration, in SDL's order (player 1 = first pad…).
//! Eden / Eglantine are handled by controller::apply; here: RetroArch,
//! PCSX2, DuckStation and Dolphin (GameCube). A player the user already set
//! to a controller is left alone; keyboard bindings are kept.

use crate::controller::{Pad, PhoneDevice};
use std::path::{Path, PathBuf};

/// `[section]` lines of an INI text (without the header), and its bounds.
fn section_bounds(lines: &[String], name: &str) -> Option<(usize, usize)> {
    let header = format!("[{name}]");
    let start = lines.iter().position(|l| l.trim() == header)?;
    let end = lines[start + 1..].iter().position(|l| l.trim_start().starts_with('[')).map_or(lines.len(), |i| start + 1 + i);
    Some((start, end))
}

/// Applies `f` to section `name` (created at the end if missing).
fn edit_section(lines: &mut Vec<String>, name: &str, f: impl FnOnce(Vec<String>) -> Vec<String>) {
    let (start, end) = section_bounds(lines, name).unwrap_or_else(|| {
        if lines.last().is_some_and(|l| !l.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push(format!("[{name}]"));
        (lines.len() - 1, lines.len())
    });
    let mut body: Vec<String> = lines[start + 1..end].to_vec();
    while body.last().is_some_and(|l| l.trim().is_empty()) {
        body.pop();
    }
    let mut body = f(body);
    body.push(String::new());
    lines.splice(start + 1..end, body);
}

fn key_of(line: &str) -> Option<&str> {
    line.split_once('=').map(|(k, _)| k.trim())
}

fn value_of<'a>(body: &'a [String], key: &str) -> Option<&'a str> {
    body.iter().find_map(|l| l.split_once('=').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim()))
}

fn read_lines(path: &Path) -> Option<Vec<String>> {
    std::fs::read_to_string(path).ok().map(|t| t.lines().map(str::to_owned).collect())
}

fn write_lines(path: &Path, lines: &[String]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- PCSX2 / DuckStation

/// Pad binding -> SDL gamepad input, as both emulators name them
/// (`SDL-<n>/<input>`); face buttons differ between the two.
fn ps_bindings(face: [&'static str; 4]) -> [(&'static str, &'static str); 25] {
    [
        ("Up", "DPadUp"), ("Down", "DPadDown"), ("Left", "DPadLeft"), ("Right", "DPadRight"),
        ("Cross", face[0]), ("Circle", face[1]), ("Square", face[2]), ("Triangle", face[3]),
        ("Select", "Back"), ("Start", "Start"), ("L1", "LeftShoulder"), ("R1", "RightShoulder"),
        ("L2", "+LeftTrigger"), ("R2", "+RightTrigger"), ("L3", "LeftStick"), ("R3", "RightStick"),
        ("LUp", "-LeftY"), ("LDown", "+LeftY"), ("LLeft", "-LeftX"), ("LRight", "+LeftX"),
        ("RUp", "-RightY"), ("RDown", "+RightY"), ("RLeft", "-RightX"), ("RRight", "+RightX"),
        ("Analog", "Guide"),
    ]
}

/// PCSX2 (`inis/PCSX2.ini`) or DuckStation (`settings.ini`): Pad1 and Pad2.
/// Only edits an existing file, so the emulator's first-run setup still runs.
fn playstation(ini: &Path, pads: &[Pad], pad_type: &str, face: [&'static str; 4]) -> Result<Vec<String>, String> {
    let Some(mut lines) = read_lines(ini) else { return Ok(Vec::new()) };
    let mut changed = Vec::new();
    for (i, pad) in pads.iter().take(2).enumerate() {
        let section = format!("Pad{}", i + 1);
        let current = section_bounds(&lines, &section).map(|(s, e)| lines[s + 1..e].to_vec()).unwrap_or_default();
        if current.iter().any(|l| key_of(l) == Some("Cross") && l.contains("SDL-")) {
            continue;
        }
        edit_section(&mut lines, &section, |body| {
            let mut body: Vec<String> = body.into_iter().filter(|l| key_of(l) != Some("Type") || i == 0).collect();
            if i > 0 || value_of(&body, "Type").is_none() {
                body.insert(0, format!("Type = {pad_type}"));
            }
            for (key, input) in ps_bindings(face) {
                body.push(format!("{key} = SDL-{i}/{input}"));
            }
            body
        });
        changed.push(format!("J{} {}", i + 1, pad.name));
    }
    if !changed.is_empty() {
        write_lines(ini, &lines)?;
    }
    Ok(changed)
}

pub fn pcsx2(save_dir: &Path, pads: &[Pad]) -> Result<Vec<String>, String> {
    // Saves live in <data>/memcards, settings in <data>/inis/PCSX2.ini.
    let ini = save_dir.parent().unwrap_or(save_dir).join("inis").join("PCSX2.ini");
    playstation(&ini, pads, "DualShock2", ["FaceSouth", "FaceEast", "FaceWest", "FaceNorth"])
}

pub fn duckstation(save_dir: &Path, pads: &[Pad]) -> Result<Vec<String>, String> {
    let ini = save_dir.parent().unwrap_or(save_dir).join("settings.ini");
    playstation(&ini, pads, "AnalogController", ["A", "B", "X", "Y"])
}

// ---------------------------------------------------------------- Dolphin

const GC_BINDINGS: [(&str, &str); 20] = [
    ("Buttons/A", "Button S"), ("Buttons/B", "Button W"), ("Buttons/X", "Button E"), ("Buttons/Y", "Button N"),
    ("Buttons/Z", "Shoulder R"), ("Buttons/Start", "Start"),
    ("Main Stick/Up", "Left Y+"), ("Main Stick/Down", "Left Y-"), ("Main Stick/Left", "Left X-"), ("Main Stick/Right", "Left X+"),
    ("C-Stick/Up", "Right Y+"), ("C-Stick/Down", "Right Y-"), ("C-Stick/Left", "Right X-"), ("C-Stick/Right", "Right X+"),
    ("Triggers/L", "Trigger L"), ("Triggers/R", "Trigger R"),
    ("D-Pad/Up", "Pad N"), ("D-Pad/Down", "Pad S"), ("D-Pad/Left", "Pad W"), ("D-Pad/Right", "Pad E"),
];

/// Dolphin device ids: `SDL/<n>/<name>`, n counting pads with the same name.
fn dolphin_devices(pads: &[Pad]) -> Vec<String> {
    pads.iter()
        .enumerate()
        .map(|(i, p)| format!("SDL/{}/{}", pads[..i].iter().filter(|q| q.name == p.name).count(), p.name))
        .collect()
}

/// GameCube ports 1-4 (`Config/GCPadNew.ini`) and their plugged controller
/// type (`Config/Dolphin.ini`, `SIDevice<n> = 6`: standard controller).
pub fn dolphin(save_dir: &Path, pads: &[Pad]) -> Result<Vec<String>, String> {
    // Saves live in <user>/GC, settings in <user>/Config.
    let config = save_dir.parent().unwrap_or(save_dir).join("Config");
    let pad_ini = config.join("GCPadNew.ini");
    let mut lines = read_lines(&pad_ini).unwrap_or_default();
    let devices = dolphin_devices(pads);
    let mut changed = Vec::new();
    let mut plugged = Vec::new();
    for (i, device) in devices.iter().take(4).enumerate() {
        let section = format!("GCPad{}", i + 1);
        let current = section_bounds(&lines, &section)
            .and_then(|(s, e)| value_of(&lines[s + 1..e], "Device").map(str::to_owned));
        if current.as_deref().is_some_and(|d| devices.iter().any(|x| x == d)) {
            continue;
        }
        edit_section(&mut lines, &section, |_| {
            let mut body = vec![format!("Device = {device}")];
            for (key, input) in GC_BINDINGS {
                body.push(format!("{key} = `{input}`"));
            }
            body.push("Triggers/L-Analog = `Trigger L`".into());
            body.push("Triggers/R-Analog = `Trigger R`".into());
            body
        });
        changed.push(format!("J{} {}", i + 1, pads[i].name));
        plugged.push(i);
    }
    if changed.is_empty() {
        return Ok(changed);
    }
    write_lines(&pad_ini, &lines)?;

    let core_ini = config.join("Dolphin.ini");
    let mut core = read_lines(&core_ini).unwrap_or_default();
    edit_section(&mut core, "Core", |body| {
        let mut body = body;
        for i in plugged {
            let key = format!("SIDevice{i}");
            let empty = value_of(&body, &key).is_none_or(|v| v == "0");
            if empty {
                body.retain(|l| key_of(l) != Some(key.as_str()));
                body.push(format!("{key} = 6"));
            }
        }
        body
    });
    write_lines(&core_ini, &core)?;
    Ok(changed)
}

// ---------------------------------------------------------------- RetroArch

/// RetroArch assigns controllers to players by itself, but does not know the
/// Android controller over Bluetooth: give it a profile under the name and
/// ids the system uses. Buttons follow the W3C order of its HID report.
pub fn retroarch(exe: &Path, phones: &[PhoneDevice]) -> Result<usize, String> {
    let Some(dir) = retroarch_autoconfig_dir(exe) else { return Ok(0) };
    // (joypad driver, index of the right stick axes)
    let drivers: &[(&str, u32, u32)] = if cfg!(windows) {
        &[("dinput", 3, 4), ("xinput", 3, 4)]
    } else if cfg!(target_os = "macos") {
        &[("hid", 2, 3)]
    } else {
        &[("udev", 2, 3)]
    };
    let mut written = 0;
    for phone in phones {
        let file_name: String = phone.name.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        for (driver, rx, ry) in drivers {
            let buttons = [
                ("b", 0), ("a", 1), ("y", 2), ("x", 3), ("l", 4), ("r", 5), ("l2", 6), ("r2", 7), ("select", 8),
                ("start", 9), ("l3", 10), ("r3", 11), ("up", 12), ("down", 13), ("left", 14), ("right", 15),
                ("menu_toggle", 16),
            ];
            let mut cfg = vec![
                format!("input_driver = \"{driver}\""),
                format!("input_device = \"{}\"", phone.name.replace('"', "")),
                format!("input_vendor_id = \"{}\"", phone.vendor),
                format!("input_product_id = \"{}\"", phone.product),
                "input_device_display_name = \"AetherPlay (Android)\"".into(),
            ];
            cfg.extend(buttons.iter().map(|(name, i)| format!("input_{name}_btn = \"{i}\"")));
            for (stick, x, y) in [("l", 0, 1), ("r", *rx, *ry)] {
                cfg.push(format!("input_{stick}_x_plus_axis = \"+{x}\""));
                cfg.push(format!("input_{stick}_x_minus_axis = \"-{x}\""));
                cfg.push(format!("input_{stick}_y_plus_axis = \"+{y}\""));
                cfg.push(format!("input_{stick}_y_minus_axis = \"-{y}\""));
            }
            let path = dir.join(driver).join(format!("AetherPlay_{file_name}.cfg"));
            if read_lines(&path).as_deref() != Some(&cfg[..]) {
                write_lines(&path, &cfg)?;
                written += 1;
            }
        }
    }
    Ok(written)
}

fn retroarch_autoconfig_dir(exe: &Path) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        return Some(PathBuf::from(home).join("Library/Application Support/RetroArch/autoconfig"));
    }
    if cfg!(windows) {
        return Some(exe.parent()?.join("autoconfig"));
    }
    // Linux AppImage: a "<name>.AppImage.home" folder next to it is its HOME.
    let mut portable = exe.as_os_str().to_owned();
    portable.push(".home");
    let home = PathBuf::from(portable);
    let home = if home.is_dir() { home } else { PathBuf::from(std::env::var_os("HOME")?) };
    Some(home.join(".config/retroarch/autoconfig"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("aep-players-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn pcsx2_gets_two_players_and_keeps_the_keyboard() {
        let root = tmp("pcsx2");
        let memcards = root.join("memcards");
        let ini = root.join("inis/PCSX2.ini");
        write_lines(&ini, &["[UI]".into(), "Theme = dark".into(), "[Pad1]".into(), "Type = DualShock2".into(), "Cross = Keyboard/X".into()]).unwrap();
        let pads = [Pad::test("Xbox 360 Controller"), Pad::test("AetherPlay Manette")];
        assert_eq!(pcsx2(&memcards, &pads).unwrap(), vec!["J1 Xbox 360 Controller", "J2 AetherPlay Manette"]);
        let out = std::fs::read_to_string(&ini).unwrap();
        assert!(out.contains("Cross = Keyboard/X\nUp = SDL-0/DPadUp"));
        assert!(out.contains("Cross = SDL-0/FaceSouth"));
        assert!(out.contains("[Pad2]\nType = DualShock2\nUp = SDL-1/DPadUp"));
        assert!(out.contains("[UI]\nTheme = dark"));
        // Already set up: untouched.
        assert!(pcsx2(&memcards, &pads).unwrap().is_empty());
    }

    #[test]
    fn dolphin_plugs_one_gamecube_controller_per_player() {
        let root = tmp("dolphin");
        let gc = root.join("GC");
        let pads = [Pad::test("AetherPlay Manette"), Pad::test("AetherPlay Manette")];
        assert_eq!(dolphin(&gc, &pads).unwrap().len(), 2);
        let pad_ini = std::fs::read_to_string(root.join("Config/GCPadNew.ini")).unwrap();
        assert!(pad_ini.contains("[GCPad1]\nDevice = SDL/0/AetherPlay Manette\nButtons/A = `Button S`"));
        assert!(pad_ini.contains("[GCPad2]\nDevice = SDL/1/AetherPlay Manette"));
        assert!(pad_ini.contains("Main Stick/Up = `Left Y+`"));
        let core = std::fs::read_to_string(root.join("Config/Dolphin.ini")).unwrap();
        assert!(core.contains("SIDevice0 = 6") && core.contains("SIDevice1 = 6"));
        assert!(dolphin(&gc, &pads).unwrap().is_empty());
    }
}
