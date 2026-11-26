//! Second screen on the phone (Windows): the Nintendo DS bottom screen
//! (melonDS) or the Wii U GamePad (Cemu) is shown on the Android controller,
//! and its touches come back as clicks.
//!
//! * Before launch, with a phone connected, the emulator is told to open that
//!   screen in its own window (melonDS: `Instance0.Window1`, bottom screen
//!   only, the main window keeping the top one; Cemu: "GamePad View").
//!   Without a phone the previous layout is put back.
//! * During the game that window is captured (PrintWindow, also when it is
//!   behind other windows), JPEG-encoded and sent to the phones.
//! * Touches are posted to the window as mouse messages: the cursor and the
//!   focus stay where they are.

use std::path::{Path, PathBuf};

/// Emulators with a second screen to show on the phone.
pub fn supported(emu: &str) -> bool {
    cfg!(windows) && matches!(emu, "melonds" | "cemu")
}

// ---------------------------------------------------------------- configuration

/// Sets `key = value` in `[section]` of a TOML / INI text, adding either.
fn set_key(lines: &mut Vec<String>, section: &str, key: &str, value: &str) {
    let header = format!("[{section}]");
    let line = format!("{key} = {value}");
    let Some(start) = lines.iter().position(|l| l.trim() == header) else {
        if lines.last().is_some_and(|l| !l.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push(header);
        lines.push(line);
        return;
    };
    let end = lines[start + 1..].iter().position(|l| l.trim_start().starts_with('[')).map_or(lines.len(), |i| start + 1 + i);
    match lines[start + 1..end].iter().position(|l| l.split_once('=').is_some_and(|(k, _)| k.trim() == key)) {
        Some(i) => lines[start + 1 + i] = line,
        None => {
            // Before the blank lines that close the section.
            let mut at = end;
            while at > start + 1 && lines[at - 1].trim().is_empty() {
                at -= 1;
            }
            lines.insert(at, line);
        }
    }
}

/// melonDS settings file: `portable/` next to the exe, the exe folder
/// (portable build), or the user's config folder; for a fresh install, a new
/// `portable/` folder (nothing to lose).
fn melonds_config(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let mut candidates = vec![dir.join("portable").join("melonDS.toml"), dir.join("melonDS.toml")];
    for var in ["LOCALAPPDATA", "APPDATA"] {
        if let Some(base) = std::env::var_os(var) {
            candidates.push(PathBuf::from(base).join("melonDS").join("melonDS.toml"));
        }
    }
    Some(candidates.iter().find(|p| p.is_file()).cloned().unwrap_or_else(|| candidates[0].clone()))
}

/// Cemu settings: `portable/`, the exe folder (old installs), or %APPDATA%\Cemu.
fn cemu_config(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let mut candidates = vec![dir.join("portable").join("settings.xml"), dir.join("settings.xml")];
    if let Some(base) = std::env::var_os("APPDATA") {
        candidates.push(PathBuf::from(base).join("Cemu").join("settings.xml"));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Opens (`on`) or closes the emulator's second-screen window for the next
/// launch. Returns whether something was written.
pub fn prepare(emu: &str, exe: &Path, on: bool) -> Result<bool, String> {
    match emu {
        "melonds" => {
            let Some(file) = melonds_config(exe) else { return Ok(false) };
            let mut lines: Vec<String> =
                std::fs::read_to_string(&file).map(|t| t.lines().map(str::to_owned).collect()).unwrap_or_default();
            // ScreenSizing: 0 even, 4 top only, 5 bottom only.
            set_key(&mut lines, "Instance0.Window1", "Enabled", if on { "true" } else { "false" });
            set_key(&mut lines, "Instance0.Window1", "ScreenSizing", "5");
            set_key(&mut lines, "Instance0.Window0", "ScreenSizing", if on { "4" } else { "0" });
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(&file, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
            Ok(true)
        }
        "cemu" => {
            let Some(file) = cemu_config(exe) else { return Ok(false) };
            let xml = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
            let value = format!("<open_pad>{on}</open_pad>");
            let re = regex::Regex::new(r"<open_pad>\s*\w*\s*</open_pad>").expect("regex");
            let new = if re.is_match(&xml) {
                re.replace(&xml, value.as_str()).into_owned()
            } else if let Some(i) = xml.find("<content>") {
                format!("{}\n\t{value}{}", &xml[..i + "<content>".len()], &xml[i + "<content>".len()..])
            } else {
                return Ok(false);
            };
            std::fs::write(&file, new).map_err(|e| e.to_string())?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Where the emulated screen sits in a captured window, in its client pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The largest `aw:ah` rectangle centered in `w × h`, offset by `top`.
fn fit(w: i32, h: i32, top: i32, aw: i32, ah: i32) -> Area {
    let avail = (h - top).max(1);
    let (fw, fh) = if w * ah <= avail * aw { (w, w * ah / aw) } else { (avail * aw / ah, avail) };
    Area { x: (w - fw) / 2, y: top + (avail - fh) / 2, w: fw.max(1), h: fh.max(1) }
}

/// melonDS window: Qt menu bar on top, then the bottom screen (4:3) fitted
/// in the rest. Right after start the window is sized to the screen exactly,
/// which gives the menu bar height; otherwise it is estimated from the DPI.
fn melonds_area(w: i32, h: i32, dpi: u32) -> Area {
    let exact = h - w * 3 / 4;
    let menu = if (8..=100).contains(&exact) { exact } else { (25 * dpi as i32) / 96 };
    fit(w, h, menu, 4, 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn melonds_second_window_is_enabled_and_restored() {
        let dir = std::env::temp_dir().join(format!("aep-dual-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("portable")).unwrap();
        let toml = dir.join("portable/melonDS.toml");
        std::fs::write(&toml, "LimitFPS = true\n\n[Instance0]\nJoystickID = 0\n\n[Instance0.Window1]\nEnabled = false\n\n[Instance0.Window0]\nEnabled = true\nScreenFilter = false\n").unwrap();
        let exe = dir.join("melonDS.exe");
        assert!(prepare("melonds", &exe, true).unwrap());
        let out = std::fs::read_to_string(&toml).unwrap();
        assert!(out.contains("[Instance0.Window1]\nEnabled = true\nScreenSizing = 5\n"));
        assert!(out.contains("[Instance0.Window0]\nEnabled = true\nScreenFilter = false\nScreenSizing = 4"));
        assert!(out.contains("LimitFPS = true"));
        prepare("melonds", &exe, false).unwrap();
        let out = std::fs::read_to_string(&toml).unwrap();
        assert!(out.contains("Enabled = false") && out.contains("ScreenSizing = 0"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn screen_area_skips_the_menu_bar() {
        // 256x217 right after start: 25 px of menu, then 256x192.
        assert_eq!(melonds_area(256, 217, 96), Area { x: 0, y: 25, w: 256, h: 192 });
        // Widened window: the screen is centered below the menu.
        assert_eq!(melonds_area(800, 500, 96), Area { x: 83, y: 25, w: 633, h: 475 });
    }
}

// ---------------------------------------------------------------- capture, Windows

#[cfg(windows)]
pub use win::{start, stop, touch};

#[cfg(not(windows))]
pub fn start(_emu: &str, _pid: u32) {}
#[cfg(not(windows))]
pub fn stop() {}
#[cfg(not(windows))]
pub fn touch(_x: f32, _y: f32, _down: bool) {}

#[cfg(windows)]
mod win {
    use super::{Area, fit, melonds_area};
    use std::sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    };
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject,
    };
    use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GW_OWNER, GetClientRect, GetWindow, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible, PostMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    };

    /// The window being streamed and where the screen is in it.
    struct Target {
        hwnd: isize,
        area: Area,
        down: bool,
    }

    static TARGET: Mutex<Option<Target>> = Mutex::new(None);
    /// wParam flag: left button held.
    const MK_LBUTTON: usize = 0x0001;
    /// Bumped to end the running capture thread.
    static SESSION: AtomicU64 = AtomicU64::new(0);

    fn windows_of(pid: u32) -> Vec<HWND> {
        unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
            // SAFETY: `data` is the Vec passed by windows_of below.
            let list = unsafe { &mut *(data.0 as *mut Vec<HWND>) };
            list.push(hwnd);
            true.into()
        }
        let mut all: Vec<HWND> = Vec::new();
        // SAFETY: the callback only pushes into `all`, alive for the call.
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut all as *mut _ as isize));
        }
        all.into_iter()
            .filter(|h| unsafe {
                let mut owner = 0u32;
                GetWindowThreadProcessId(*h, Some(&mut owner));
                owner == pid && IsWindowVisible(*h).as_bool()
            })
            .collect()
    }

    fn children_of(hwnd: HWND) -> Vec<HWND> {
        unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
            // SAFETY: `data` is the Vec passed by children_of below.
            let list = unsafe { &mut *(data.0 as *mut Vec<HWND>) };
            list.push(hwnd);
            true.into()
        }
        let mut all: Vec<HWND> = Vec::new();
        // SAFETY: as above.
        unsafe {
            let _ = EnumChildWindows(Some(hwnd), Some(collect), LPARAM(&mut all as *mut _ as isize));
        }
        all
    }

    fn client_size(hwnd: HWND) -> (i32, i32) {
        let mut r = RECT::default();
        // SAFETY: plain query on a window handle.
        let _ = unsafe { GetClientRect(hwnd, &mut r) };
        (r.right - r.left, r.bottom - r.top)
    }

    fn title(hwnd: HWND) -> String {
        let mut buf = [0u16; 256];
        // SAFETY: the buffer outlives the call.
        let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    /// The second-screen window of `emu` and the screen area inside it.
    fn find(emu: &str, pid: u32) -> Option<(HWND, Area)> {
        let windows = windows_of(pid);
        match emu {
            // The extra window is owned by the main one.
            "melonds" => windows.into_iter().find(|h| unsafe { GetWindow(*h, GW_OWNER) }.is_ok_and(|o| !o.is_invalid())).map(|h| {
                let (w, ht) = client_size(h);
                // SAFETY: plain query.
                let dpi = unsafe { GetDpiForWindow(h) }.max(96);
                (h, melonds_area(w, ht, dpi))
            }),
            // "GamePad View": draw into its biggest child (the render canvas).
            "cemu" => {
                let frame = windows.into_iter().find(|h| title(*h).contains("GamePad"))?;
                let canvas = children_of(frame)
                    .into_iter()
                    .max_by_key(|c| {
                        let (w, h) = client_size(*c);
                        w * h
                    })
                    .unwrap_or(frame);
                let (w, h) = client_size(canvas);
                Some((canvas, fit(w, h, 0, 854, 480)))
            }
            _ => None,
        }
    }

    /// The window's client area as top-down BGRA pixels.
    fn capture(hwnd: HWND) -> Option<(i32, i32, Vec<u8>)> {
        let (w, h) = client_size(hwnd);
        if w <= 0 || h <= 0 {
            return None;
        }
        // SAFETY: GDI objects are created, used and released in this block.
        unsafe {
            let screen = GetDC(Some(hwnd));
            let mem = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(mem, bitmap.into());
            // PW_CLIENTONLY | PW_RENDERFULLCONTENT (GPU-drawn content too).
            let _ = PrintWindow(hwnd, mem, PRINT_WINDOW_FLAGS(3));
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = vec![0u8; (w * h * 4) as usize];
            let lines = GetDIBits(mem, bitmap, 0, h as u32, Some(pixels.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
            SelectObject(mem, old);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(mem);
            ReleaseDC(Some(hwnd), screen);
            (lines > 0).then_some((w, h, pixels))
        }
    }

    /// Crops `area` out of a BGRA frame (halving it while wider than 900 px)
    /// and encodes it as JPEG.
    fn encode(w: i32, pixels: &[u8], area: Area) -> Option<Vec<u8>> {
        let mut step = 1;
        while area.w / step > 900 {
            step *= 2;
        }
        let (ow, oh) = (area.w / step, area.h / step);
        let mut out = Vec::with_capacity((ow * oh * 4) as usize);
        for y in 0..oh {
            let row = ((area.y + y * step) * w) as usize;
            for x in 0..ow {
                let i = (row + (area.x + x * step) as usize) * 4;
                out.extend_from_slice(pixels.get(i..i + 4)?);
            }
        }
        let mut jpeg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpeg, 80)
            .encode(&out, ow as u16, oh as u16, jpeg_encoder::ColorType::Bgra)
            .ok()?;
        Some(jpeg)
    }

    /// Streams the second screen of the game `pid` to the phones until the
    /// game ends (stop) or its window disappears.
    pub fn start(emu: &str, pid: u32) {
        let session = SESSION.fetch_add(1, Ordering::SeqCst) + 1;
        let emu = emu.to_string();
        std::thread::spawn(move || {
            let alive = || SESSION.load(Ordering::SeqCst) == session;
            // The window shows up once the emulator has started.
            let deadline = Instant::now() + Duration::from_secs(60);
            let (mut hwnd, mut area) = loop {
                if !alive() || Instant::now() > deadline {
                    return;
                }
                if let Some(found) = find(&emu, pid) {
                    break found;
                }
                std::thread::sleep(Duration::from_millis(500));
            };
            crate::phone::screen(Some((area.w, area.h)));
            let mut last_lookup = Instant::now();
            while alive() {
                let started = Instant::now();
                // Follow resizes (and a recreated window) about once a second.
                if last_lookup.elapsed() > Duration::from_secs(1) {
                    last_lookup = Instant::now();
                    match find(&emu, pid) {
                        Some(found) => (hwnd, area) = found,
                        None => break,
                    }
                }
                {
                    let mut target = TARGET.lock().expect("target");
                    let down = target.as_ref().is_some_and(|t| t.down);
                    *target = Some(Target { hwnd: hwnd.0 as isize, area, down });
                }
                if let Some((w, _, pixels)) = capture(hwnd)
                    && let Some(jpeg) = encode(w, &pixels, area)
                {
                    crate::phone::frame(jpeg);
                }
                // About 30 images per second.
                if let Some(rest) = Duration::from_millis(33).checked_sub(started.elapsed()) {
                    std::thread::sleep(rest);
                }
            }
            if alive() {
                *TARGET.lock().expect("target") = None;
                crate::phone::screen(None);
            }
        });
    }

    pub fn stop() {
        SESSION.fetch_add(1, Ordering::SeqCst);
        *TARGET.lock().expect("target") = None;
        crate::phone::screen(None);
    }

    /// MELON_PID=<pid> OUT=<file.jpg> cargo test real_capture -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_capture() {
        let pid: u32 = std::env::var("MELON_PID").unwrap().parse().unwrap();
        let (hwnd, area) = find("melonds", pid).expect("window");
        println!("area {area:?}");
        let (w, h, pixels) = capture(hwnd).expect("capture");
        println!("client {w}x{h}");
        let jpeg = encode(w, &pixels, area).expect("jpeg");
        std::fs::write(std::env::var("OUT").unwrap(), jpeg).unwrap();
    }

    /// A touch on the phone, `x` / `y` in 0..1 over the screen image.
    pub fn touch(x: f32, y: f32, down: bool) {
        let mut target = TARGET.lock().expect("target");
        let Some(t) = target.as_mut() else { return };
        let px = t.area.x + (x.clamp(0.0, 1.0) * (t.area.w - 1) as f32) as i32;
        let py = t.area.y + (y.clamp(0.0, 1.0) * (t.area.h - 1) as f32) as i32;
        let pos = LPARAM(((py & 0xFFFF) << 16 | (px & 0xFFFF)) as isize);
        let hwnd = HWND(t.hwnd as *mut _);
        let (msg, buttons) = match (down, t.down) {
            (true, false) => (WM_LBUTTONDOWN, MK_LBUTTON),
            (true, true) => (WM_MOUSEMOVE, MK_LBUTTON),
            (false, _) => (WM_LBUTTONUP, 0),
        };
        t.down = down;
        // SAFETY: posting a message to a window handle; invalid handles fail.
        let _ = unsafe { PostMessageW(Some(hwnd), msg, WPARAM(buttons), pos) };
    }
}
