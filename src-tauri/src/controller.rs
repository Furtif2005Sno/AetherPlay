//! Maps the connected controller to player 1 in yuzu-family emulators
//! (Eden, Eglantine), which otherwise start with the keyboard only.
//!
//! Their bindings name the device by its SDL GUID and raw button / axis / hat
//! numbers, so they are computed exactly as the emulator's "Input device" menu
//! does (src/input_common/drivers/sdl_driver.cpp), using the emulator's own
//! SDL3 library: same version and same hints give the same identifiers.

use libloading::{Library, Symbol};
use std::collections::HashSet;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::path::{Path, PathBuf};

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid([u8; 16]);

/// SDL_GamepadBinding: input type + union, output type + union (all 32-bit).
#[repr(C)]
#[derive(Clone, Copy)]
struct Binding {
    input_type: c_int,
    input: [c_int; 3],
    output_type: c_int,
    output: [c_int; 3],
}

const BIND_BUTTON: c_int = 1;
const BIND_AXIS: c_int = 2;
const BIND_HAT: c_int = 3;

// SDL_GamepadButton / SDL_GamepadAxis values.
const SOUTH: c_int = 0;
const EAST: c_int = 1;
const WEST: c_int = 2;
const NORTH: c_int = 3;
const BACK: c_int = 4;
const GUIDE: c_int = 5;
const START: c_int = 6;
const LEFT_STICK: c_int = 7;
const RIGHT_STICK: c_int = 8;
const LEFT_SHOULDER: c_int = 9;
const RIGHT_SHOULDER: c_int = 10;
const DPAD_UP: c_int = 11;
const DPAD_DOWN: c_int = 12;
const DPAD_LEFT: c_int = 13;
const DPAD_RIGHT: c_int = 14;
const MISC1: c_int = 15;
const AXIS_LEFTX: c_int = 0;
const AXIS_LEFTY: c_int = 1;
const AXIS_RIGHTX: c_int = 2;
const AXIS_RIGHTY: c_int = 3;
const AXIS_LEFT_TRIGGER: c_int = 4;
const AXIS_RIGHT_TRIGGER: c_int = 5;
const SENSOR_GYRO: c_int = 2;

enum Source {
    Button(c_int),
    Axis(c_int),
}

/// Switch button (config key) -> SDL gamepad button, as GetDefaultButtonBinding.
const BUTTONS: [(&str, Source); 22] = [
    ("button_a", Source::Button(EAST)),
    ("button_b", Source::Button(SOUTH)),
    ("button_x", Source::Button(NORTH)),
    ("button_y", Source::Button(WEST)),
    ("button_lstick", Source::Button(LEFT_STICK)),
    ("button_rstick", Source::Button(RIGHT_STICK)),
    ("button_l", Source::Button(LEFT_SHOULDER)),
    ("button_r", Source::Button(RIGHT_SHOULDER)),
    ("button_zl", Source::Axis(AXIS_LEFT_TRIGGER)),
    ("button_zr", Source::Axis(AXIS_RIGHT_TRIGGER)),
    ("button_plus", Source::Button(START)),
    ("button_minus", Source::Button(BACK)),
    ("button_dleft", Source::Button(DPAD_LEFT)),
    ("button_dup", Source::Button(DPAD_UP)),
    ("button_dright", Source::Button(DPAD_RIGHT)),
    ("button_ddown", Source::Button(DPAD_DOWN)),
    ("button_slleft", Source::Button(LEFT_SHOULDER)),
    ("button_srleft", Source::Button(RIGHT_SHOULDER)),
    ("button_home", Source::Button(GUIDE)),
    ("button_screenshot", Source::Button(MISC1)),
    ("button_slright", Source::Button(LEFT_SHOULDER)),
    ("button_srright", Source::Button(RIGHT_SHOULDER)),
];

/// A connected controller as the emulator will see it.
pub struct Pad {
    pub name: String,
    guid: String,
    port: usize,
    bindings: Vec<Binding>,
    gyro: bool,
}

#[cfg(test)]
impl Pad {
    /// A pad with no bindings, for tests of other modules.
    pub fn test(name: &str) -> Self {
        Pad { name: name.into(), guid: "00".into(), port: 0, bindings: Vec::new(), gyro: false }
    }
}

/// SDL3 shipped with the emulator (macOS bundle, or next to the exe),
/// else RetroArch's copy when it is installed.
pub fn find_sdl(emulator: &Path, retroarch: Option<&Path>) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if emulator.extension().is_some_and(|e| e == "app") {
        candidates.push(emulator.join("Contents/Frameworks/libSDL3.0.dylib"));
    } else if let Some(dir) = emulator.parent() {
        candidates.push(dir.join("SDL3.dll"));
    }
    if let Some(ra) = retroarch {
        if ra.extension().is_some_and(|e| e == "app") {
            candidates.push(ra.join("Contents/Frameworks/libSDL3.0.dylib"));
        } else if let Some(dir) = ra.parent() {
            candidates.push(dir.join("SDL3.dll"));
        }
    }
    candidates.into_iter().find(|p| p.is_file()).or_else(|| {
        // AppImages keep their SDL inside the image: use the system's (SteamOS has one).
        cfg!(target_os = "linux").then(|| PathBuf::from("libSDL3.so.0"))
    })
}

/// SDL mapping of the AetherPlay Android controller over Bluetooth: its HID
/// report lists 17 buttons in W3C standard order, then 4 stick axes.
const PHONE_MAPPING: &str = "a:b0,b:b1,x:b2,y:b3,leftshoulder:b4,rightshoulder:b5,lefttrigger:b6,righttrigger:b7,\
back:b8,start:b9,leftstick:b10,rightstick:b11,dpup:b12,dpdown:b13,dpleft:b14,dpright:b15,guide:b16,\
leftx:a0,lefty:a1,rightx:a2,righty:a3";

pub fn phone_mapping(guid: &str) -> String {
    format!("{guid},AetherPlay Manette,{PHONE_MAPPING},")
}

/// What SDL sees: the gamepads, and SDL mappings for Android controllers
/// connected over Bluetooth (SDL does not know them; the emulators get these
/// through SDL_GAMECONTROLLERCONFIG).
pub struct Detected {
    pub pads: Vec<Pad>,
    pub phone_mappings: Vec<String>,
    /// Android controllers over Bluetooth, as the system names them (often
    /// after the phone) — RetroArch needs that name in its profile.
    pub phones: Vec<PhoneDevice>,
}

pub struct PhoneDevice {
    pub name: String,
    pub vendor: u16,
    pub product: u16,
}

/// Lists gamepads through `lib` with the hints Eden/Eglantine set by default.
pub fn detect(lib: &Path) -> Result<Detected, String> {
    // SAFETY: the symbols are SDL3's public C API with the signatures below.
    unsafe {
        let sdl = Library::new(lib).map_err(|e| format!("SDL3 : {e}"))?;
        macro_rules! sym {
            ($name:literal, $t:ty) => {{
                let s: Symbol<$t> = sdl.get($name).map_err(|e| format!("SDL3 : {e}"))?;
                s
            }};
        }
        let set_hint = sym!(b"SDL_SetHint\0", unsafe extern "C" fn(*const c_char, *const c_char) -> bool);
        let init = sym!(b"SDL_Init\0", unsafe extern "C" fn(u32) -> bool);
        let quit = sym!(b"SDL_Quit\0", unsafe extern "C" fn());
        let pump = sym!(b"SDL_PumpEvents\0", unsafe extern "C" fn());
        let get_gamepads = sym!(b"SDL_GetGamepads\0", unsafe extern "C" fn(*mut c_int) -> *mut u32);
        let open = sym!(b"SDL_OpenGamepad\0", unsafe extern "C" fn(u32) -> *mut c_void);
        let close = sym!(b"SDL_CloseGamepad\0", unsafe extern "C" fn(*mut c_void));
        let name_of = sym!(b"SDL_GetGamepadName\0", unsafe extern "C" fn(*mut c_void) -> *const c_char);
        let guid_of = sym!(b"SDL_GetJoystickGUIDForID\0", unsafe extern "C" fn(u32) -> Guid);
        let bindings_of =
            sym!(b"SDL_GetGamepadBindings\0", unsafe extern "C" fn(*mut c_void, *mut c_int) -> *mut *mut Binding);
        let has_sensor = sym!(b"SDL_GamepadHasSensor\0", unsafe extern "C" fn(*mut c_void, c_int) -> bool);
        let free = sym!(b"SDL_free\0", unsafe extern "C" fn(*mut c_void));
        let get_joysticks = sym!(b"SDL_GetJoysticks\0", unsafe extern "C" fn(*mut c_int) -> *mut u32);
        let joystick_name = sym!(b"SDL_GetJoystickNameForID\0", unsafe extern "C" fn(u32) -> *const c_char);
        let is_gamepad = sym!(b"SDL_IsGamepad\0", unsafe extern "C" fn(u32) -> bool);
        let guid_to_string = sym!(b"SDL_GUIDToString\0", unsafe extern "C" fn(Guid, *mut c_char, c_int));
        let add_mapping = sym!(b"SDL_AddGamepadMapping\0", unsafe extern "C" fn(*const c_char) -> c_int);
        let open_joystick = sym!(b"SDL_OpenJoystick\0", unsafe extern "C" fn(u32) -> *mut c_void);
        let close_joystick = sym!(b"SDL_CloseJoystick\0", unsafe extern "C" fn(*mut c_void));
        let count_buttons = sym!(b"SDL_GetNumJoystickButtons\0", unsafe extern "C" fn(*mut c_void) -> c_int);
        let count_axes = sym!(b"SDL_GetNumJoystickAxes\0", unsafe extern "C" fn(*mut c_void) -> c_int);
        let count_hats = sym!(b"SDL_GetNumJoystickHats\0", unsafe extern "C" fn(*mut c_void) -> c_int);
        let vendor_of = sym!(b"SDL_GetJoystickVendorForID\0", unsafe extern "C" fn(u32) -> u16);
        let product_of = sym!(b"SDL_GetJoystickProductForID\0", unsafe extern "C" fn(u32) -> u16);

        for (k, v) in [
            ("SDL_APP_NAME", "Eden"),
            ("SDL_JOYSTICK_RAWINPUT", "0"),
            ("SDL_JOYSTICK_HIDAPI_STEAM", "1"),
            ("SDL_GAMECONTROLLER_SENSOR_FUSION", "1"),
            ("SDL_JOYSTICK_ENHANCED_REPORTS", "1"),
            ("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1"),
            // enable_joycon_driver = true, enable_procon_driver = false.
            ("SDL_JOYSTICK_HIDAPI_JOY_CONS", "0"),
            ("SDL_JOYSTICK_HIDAPI_SWITCH", "1"),
            ("SDL_JOYSTICK_HIDAPI_XBOX", "0"),
        ] {
            let (k, v) = (format!("{k}\0"), format!("{v}\0"));
            set_hint(k.as_ptr().cast(), v.as_ptr().cast());
        }
        if !init(0x200 | 0x2000) {
            return Err("SDL3 : initialisation impossible".into());
        }
        // Android controllers over Bluetooth: give SDL their mapping first.
        let mut phone_mappings = Vec::new();
        let mut phones = Vec::new();
        pump();
        let mut n = 0;
        let list = get_joysticks(&mut n);
        if !list.is_null() {
            for &id in std::slice::from_raw_parts(list, n.max(0) as usize) {
                let name = joystick_name(id);
                let name = if name.is_null() { String::new() } else { CStr::from_ptr(name).to_string_lossy().into_owned() };
                if is_gamepad(id) {
                    continue;
                }
                // Named after the phone by Linux / Windows: recognise its report
                // layout instead (17 buttons, 4 axes, no hat).
                let looks_like_phone = name.contains("AetherPlay") || {
                    let j = open_joystick(id);
                    let shape = !j.is_null() && count_buttons(j) == 17 && count_axes(j) == 4 && count_hats(j) == 0;
                    if !j.is_null() {
                        close_joystick(j);
                    }
                    shape
                };
                if !looks_like_phone {
                    continue;
                }
                phones.push(PhoneDevice { name: name.clone(), vendor: vendor_of(id), product: product_of(id) });
                let mut buf = [0 as c_char; 33];
                guid_to_string(guid_of(id), buf.as_mut_ptr(), buf.len() as c_int);
                let guid = CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned();
                let mapping = phone_mapping(&guid);
                let c = format!("{mapping}\0");
                add_mapping(c.as_ptr().cast());
                phone_mappings.push(mapping);
            }
            free(list.cast());
        }
        // Bluetooth / HIDAPI pads show up shortly after init.
        let mut ids = Vec::new();
        for _ in 0..8 {
            pump();
            let mut n = 0;
            let list = get_gamepads(&mut n);
            if !list.is_null() {
                ids = std::slice::from_raw_parts(list, n.max(0) as usize).to_vec();
                free(list.cast());
            }
            if !ids.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        let mut pads: Vec<Pad> = Vec::new();
        for id in ids {
            let mut raw = guid_of(id).0;
            raw[2] = 0; // Eden clears the name CRC.
            raw[3] = 0;
            let guid: String = raw.iter().map(|b| format!("{b:02x}")).collect();
            let gp = open(id);
            if gp.is_null() {
                continue;
            }
            let name = {
                let p = name_of(gp);
                if p.is_null() { "Manette".into() } else { CStr::from_ptr(p).to_string_lossy().into_owned() }
            };
            let mut n = 0;
            let list = bindings_of(gp, &mut n);
            let mut bindings = Vec::new();
            if !list.is_null() {
                for &b in std::slice::from_raw_parts(list, n.max(0) as usize) {
                    if !b.is_null() {
                        bindings.push(*b);
                    }
                }
                free(list.cast());
            }
            let gyro = has_sensor(gp, SENSOR_GYRO);
            close(gp);
            let port = pads.iter().filter(|p| p.guid == guid).count();
            pads.push(Pad { name, guid, port, bindings, gyro });
        }
        quit();
        Ok(Detected { pads, phone_mappings, phones })
    }
}

impl Pad {
    fn base(&self) -> String {
        format!("engine:sdl,guid:{},port:{}", self.guid, self.port)
    }

    fn binding(&self, src: &Source) -> Option<&Binding> {
        self.bindings.iter().find(|b| match src {
            Source::Button(btn) => b.output_type == BIND_BUTTON && b.output[0] == *btn,
            Source::Axis(axis) => b.output_type == BIND_AXIS && b.output[0] == *axis,
        })
    }

    fn param(&self, b: &Binding) -> Option<String> {
        let base = self.base();
        Some(match b.input_type {
            BIND_BUTTON => format!("{base},button:{}", b.input[0]),
            BIND_AXIS => format!("{base},axis:{},invert:+,threshold:0.5", b.input[0]),
            BIND_HAT => {
                let dir = match b.input[1] {
                    1 => "up",
                    2 => "right",
                    4 => "down",
                    8 => "left",
                    _ => return None,
                };
                format!("{base},direction:{dir},hat:{}", b.input[0])
            }
            _ => return None,
        })
    }

    fn stick(&self, x: c_int, y: c_int) -> Option<String> {
        let ax = self.binding(&Source::Axis(x)).filter(|b| b.input_type == BIND_AXIS)?.input[0];
        let ay = self.binding(&Source::Axis(y)).filter(|b| b.input_type == BIND_AXIS)?.input[0];
        Some(format!(
            "{},axis_x:{ax},axis_y:{ay},invert_x:+,invert_y:+,offset_x:0.000000,offset_y:0.000000",
            self.base()
        ))
    }

    /// `player_0_*` config keys and values for this pad.
    fn mapping(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = BUTTONS
            .iter()
            .filter_map(|(key, src)| Some((key.to_string(), self.param(self.binding(src)?)?)))
            .collect();
        if let Some(s) = self.stick(AXIS_LEFTX, AXIS_LEFTY) {
            out.push(("lstick".into(), s));
        }
        if let Some(s) = self.stick(AXIS_RIGHTX, AXIS_RIGHTY) {
            out.push(("rstick".into(), s));
        }
        if self.gyro {
            let m = format!("{},motion:0", self.base());
            out.push(("motionleft".into(), m.clone()));
            out.push(("motionright".into(), m));
        }
        out
    }
}

/// Players handled for multiplayer (RetroTogether): Eden/Eglantine know 8,
/// four cover couch play.
const MAX_PLAYERS: usize = 4;

/// Gives each connected pad a player, in SDL's order: player 1 the first pad,
/// player 2 the next one… A player already set to a connected pad, or to a
/// device SDL does not handle (the user's choice), is kept. Returns a summary
/// like "J1 Pro Controller, J2 AetherPlay Manette" if anything changed.
pub fn apply(ini: &Path, pads: &[Pad]) -> Result<Option<String>, String> {
    if pads.is_empty() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(ini).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();

    let start = lines.iter().position(|l| l.trim() == "[Controls]");
    let (start, end) = match start {
        Some(s) => {
            let e = lines[s + 1..].iter().position(|l| l.trim_start().starts_with('[')).map_or(lines.len(), |i| s + 1 + i);
            (s, e)
        }
        None => {
            lines.push("[Controls]".into());
            (lines.len() - 1, lines.len())
        }
    };
    let get = |key: &str| {
        lines[start + 1..end].iter().find_map(|l| l.strip_prefix(&format!("{key}=")).map(|v| v.trim_matches('"').to_string()))
    };
    let pad_of = |value: &str| {
        let field = |k: &str| value.split(',').find_map(|kv| kv.strip_prefix(k)).map(str::to_owned);
        let (guid, port) = (field("guid:")?, field("port:").and_then(|p| p.parse::<usize>().ok()).unwrap_or(0));
        pads.iter().position(|p| p.guid == guid && p.port == port)
    };

    // Which pad each player uses now, and which players keep their setting.
    let mut taken = vec![false; pads.len()];
    let mut keep = [false; MAX_PLAYERS];
    for (player, kept) in keep.iter_mut().enumerate() {
        if get(&format!("player_{player}_button_a\\default")).is_none_or(|d| d != "false") {
            continue;
        }
        let Some(current) = get(&format!("player_{player}_button_a")) else { continue };
        if current.contains("engine:sdl") {
            if let Some(i) = pad_of(&current) {
                taken[i] = true;
                *kept = true;
            }
        } else if !current.contains("engine:keyboard") {
            *kept = true;
        }
    }

    let mut writes: Vec<(String, String)> = Vec::new();
    let mut summary = Vec::new();
    let mut free = (0..pads.len()).filter(|i| !taken[*i]);
    for (player, kept) in keep.iter().enumerate() {
        if *kept {
            continue;
        }
        let Some(i) = free.next() else { break };
        let mapping = pads[i].mapping();
        if mapping.is_empty() {
            continue;
        }
        writes.extend(mapping.into_iter().map(|(k, v)| (format!("player_{player}_{k}"), format!("\"{v}\""))));
        if player > 0 {
            writes.push((format!("player_{player}_connected"), "true".into()));
        }
        summary.push(format!("J{} {}", player + 1, pads[i].name));
    }
    if writes.is_empty() {
        return Ok(None);
    }

    let keys: HashSet<String> = writes.iter().flat_map(|(k, _)| [k.clone(), format!("{k}\\default")]).collect();
    let mut section: Vec<String> = lines[start + 1..end]
        .iter()
        .filter(|l| l.split_once('=').is_none_or(|(k, _)| !keys.contains(k)))
        .cloned()
        .collect();
    while section.last().is_some_and(|l| l.trim().is_empty()) {
        section.pop();
    }
    for (k, v) in &writes {
        section.push(format!("{k}\\default=false"));
        section.push(format!("{k}={v}"));
    }
    section.push(String::new());
    lines.splice(start + 1..end, section);
    if let Some(parent) = ini.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(ini, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    Ok(Some(summary.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad() -> Pad {
        let b = |it, i, ot, o| Binding { input_type: it, input: [i, 0, 0], output_type: ot, output: [o, 0, 0] };
        let hat = |mask, o| Binding { input_type: BIND_HAT, input: [0, mask, 0], output_type: BIND_BUTTON, output: [o, 0, 0] };
        Pad {
            name: "Pro Controller".into(),
            guid: "0500000000000000000000000000ff00".into(),
            port: 0,
            bindings: vec![
                b(BIND_BUTTON, 1, BIND_BUTTON, EAST),
                b(BIND_BUTTON, 0, BIND_BUTTON, SOUTH),
                b(BIND_AXIS, 4, BIND_AXIS, AXIS_LEFT_TRIGGER),
                b(BIND_AXIS, 0, BIND_AXIS, AXIS_LEFTX),
                b(BIND_AXIS, 1, BIND_AXIS, AXIS_LEFTY),
                hat(1, DPAD_UP),
            ],
            gyro: false,
        }
    }

    /// SDL_LIB=<path to SDL3> cargo test detect_real -- --ignored --nocapture
    #[test]
    #[ignore]
    fn detect_real() {
        let lib = std::env::var("SDL_LIB").expect("SDL_LIB");
        let found = detect(Path::new(&lib)).unwrap();
        println!("mappings Android : {:?}", found.phone_mappings);
        for p in found.pads {
            println!("{} guid={} port={}", p.name, p.guid, p.port);
            for (k, v) in p.mapping() {
                println!("  player_0_{k}={v}");
            }
        }
    }

    #[test]
    fn maps_keyboard_default_to_pad_and_keeps_other_settings() {
        let ini = std::env::temp_dir().join(format!("aep-ctrl-{}.ini", std::process::id()));
        std::fs::write(
            &ini,
            "[Controls]\nplayer_0_button_a\\default=true\nplayer_0_button_a=\"engine:keyboard,code:67\"\nvibration_enabled=true\n[UI]\ntheme=dark\n",
        )
        .unwrap();
        assert_eq!(apply(&ini, &[pad()]).unwrap().as_deref(), Some("J1 Pro Controller"));
        let out = std::fs::read_to_string(&ini).unwrap();
        assert!(out.contains("player_0_button_a\\default=false\nplayer_0_button_a=\"engine:sdl,guid:0500000000000000000000000000ff00,port:0,button:1\""));
        assert!(out.contains("player_0_button_zl=\"engine:sdl,guid:0500000000000000000000000000ff00,port:0,axis:4,invert:+,threshold:0.5\""));
        assert!(out.contains("player_0_button_dup=\"engine:sdl,guid:0500000000000000000000000000ff00,port:0,direction:up,hat:0\""));
        assert!(out.contains("player_0_lstick=\"engine:sdl,guid:0500000000000000000000000000ff00,port:0,axis_x:0,axis_y:1,"));
        assert!(out.contains("vibration_enabled=true") && out.contains("[UI]\ntheme=dark"));
        assert!(!out.contains("engine:keyboard"));
        // Second launch with the same pad: left alone.
        assert_eq!(apply(&ini, &[pad()]).unwrap(), None);
        // A second controller arrives (RetroTogether): it becomes player 2.
        let mut phone = pad();
        phone.name = "AetherPlay Manette".into();
        phone.guid = "05000000aaaa00000000000000000000".into();
        assert_eq!(apply(&ini, &[pad(), phone]).unwrap().as_deref(), Some("J2 AetherPlay Manette"));
        let out = std::fs::read_to_string(&ini).unwrap();
        assert!(out.contains("player_1_connected\\default=false\nplayer_1_connected=true"));
        assert!(out.contains("player_1_button_a=\"engine:sdl,guid:05000000aaaa00000000000000000000,port:0,button:1\""));
        assert!(out.contains("player_0_button_a=\"engine:sdl,guid:0500000000000000000000000000ff00,port:0,button:1\""));
        let _ = std::fs::remove_file(&ini);
    }
}
