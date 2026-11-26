//! Controllers, read on a background thread with gilrs:
//! * holding Select + Start quits the running game, so a couch session never
//!   needs the keyboard (works while the emulator has focus: XInput on Windows);
//! * on macOS and Linux the interface is fed the pad state ("aep-gamepad"
//!   events, turned into navigator.getGamepads() by bridge.js), since their
//!   webviews do not reliably expose controllers. Windows' WebView2 does.

use crate::config;
use gilrs::{Axis, Button, Gamepad, Gilrs};
use serde::Serialize;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

/// Process id of the emulator currently running, if any.
static RUNNING: Mutex<Option<u32>> = Mutex::new(None);
/// Android controllers' state by connection (buttons in W3C order, axes), see phone.rs.
static REMOTE: Mutex<Vec<(u64, u32, [f32; 4])>> = Mutex::new(Vec::new());

pub fn set_remote(phone: u64, state: Option<(u32, [f32; 4])>) {
    let mut all = REMOTE.lock().expect("remote lock");
    all.retain(|(id, _, _)| *id != phone);
    if let Some((b, a)) = state {
        all.push((phone, b, a));
    }
}

fn remote() -> Vec<(u32, [f32; 4])> {
    REMOTE.lock().expect("remote lock").iter().map(|(_, b, a)| (*b, *a)).collect()
}

/// Closes the running game (the phone's "Quitter le jeu" button).
pub fn quit_game() {
    kill_running();
}
const HOLD: Duration = Duration::from_millis(1500);

pub fn set_running(pid: Option<u32>) {
    *RUNNING.lock().expect("running lock") = pid;
}

fn is_running() -> bool {
    RUNNING.lock().expect("running lock").is_some()
}

/// Ends the running emulator and its child processes.
fn kill_running() {
    let Some(pid) = RUNNING.lock().expect("running lock").take() else { return };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    #[cfg(not(windows))]
    {
        // The emulator leads its own process group (see emulators::command).
        let _ = std::process::Command::new("kill").args(["-TERM", "--", &format!("-{pid}")]).status();
    }
}

/// W3C "standard" gamepad layout, as RomM's interface reads it.
#[derive(Serialize, PartialEq, Clone, Default)]
struct PadState {
    connected: bool,
    id: String,
    buttons: Vec<bool>,
    axes: Vec<f32>,
}

const STANDARD_BUTTONS: [Button; 17] = [
    Button::South,
    Button::East,
    Button::West,
    Button::North,
    Button::LeftTrigger,
    Button::RightTrigger,
    Button::LeftTrigger2,
    Button::RightTrigger2,
    Button::Select,
    Button::Start,
    Button::LeftThumb,
    Button::RightThumb,
    Button::DPadUp,
    Button::DPadDown,
    Button::DPadLeft,
    Button::DPadRight,
    Button::Mode,
];

fn state_of(g: &Gamepad) -> PadState {
    // Rounded so stick noise does not flood the interface with events.
    let axis = |a: Axis, sign: f32| (g.value(a) * sign * 20.0).round() / 20.0;
    PadState {
        connected: true,
        id: g.name().to_string(),
        buttons: STANDARD_BUTTONS.iter().map(|b| g.is_pressed(*b)).collect(),
        // gilrs Y axes point up, the W3C ones down.
        axes: vec![
            axis(Axis::LeftStickX, 1.0),
            axis(Axis::LeftStickY, -1.0),
            axis(Axis::RightStickX, 1.0),
            axis(Axis::RightStickY, -1.0),
        ],
    }
}

/// What the interface sees: every controller merged into one, the phone included.
fn ui_state(gilrs: &Gilrs, live: bool) -> PadState {
    let mut pads: Vec<PadState> = gilrs.gamepads().map(|(_, g)| state_of(&g)).collect();
    for (b, axes) in remote() {
        pads.push(PadState {
            connected: true,
            id: "AetherPlay Android".into(),
            buttons: (0..STANDARD_BUTTONS.len()).map(|i| b & (1 << i) != 0).collect(),
            axes: axes.iter().map(|a| (a * 20.0).round() / 20.0).collect(),
        });
    }
    let Some(first) = pads.first() else { return PadState::default() };
    let mut merged = PadState { connected: true, id: first.id.clone(), ..PadState::default() };
    if !live {
        return merged;
    }
    merged.buttons = vec![false; STANDARD_BUTTONS.len()];
    merged.axes = vec![0.0; 4];
    for p in &pads {
        for (m, b) in merged.buttons.iter_mut().zip(&p.buttons) {
            *m |= *b;
        }
        for (m, a) in merged.axes.iter_mut().zip(&p.axes) {
            if a.abs() > m.abs() {
                *m = *a;
            }
        }
    }
    merged
}

pub fn spawn_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        let Ok(mut gilrs) = Gilrs::new() else { return };
        let feed_ui = !cfg!(windows);
        let mut held_since: Option<Instant> = None;
        let mut sent = PadState::default();
        loop {
            while gilrs.next_event_blocking(Some(Duration::from_millis(16))).is_some() {}
            let phone_combo = remote().iter().any(|(b, _)| b & (1 << 8) != 0 && b & (1 << 9) != 0);
            let combo = phone_combo
                || gilrs.gamepads().any(|(_, g)| g.is_pressed(Button::Select) && g.is_pressed(Button::Start));
            match (combo, held_since) {
                (true, None) => held_since = Some(Instant::now()),
                (true, Some(t)) if t.elapsed() >= HOLD => {
                    held_since = None;
                    if is_running() && config::load(&app).quit_combo {
                        kill_running();
                    }
                }
                (false, _) => held_since = None,
                _ => {}
            }

            if feed_ui {
                // Only while the interface is in front: never drive it from a game.
                let focused = app.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
                let state = ui_state(&gilrs, focused && !is_running());
                if state != sent {
                    let _ = app.emit_to("main", "aep-gamepad", &state);
                    sent = state;
                }
            }
        }
    });
}
