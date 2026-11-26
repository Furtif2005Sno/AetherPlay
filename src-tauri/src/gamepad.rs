//! Controller hotkey: holding Select + Start quits the running game, so a
//! couch session never needs the keyboard. Runs on its own thread and reads
//! controllers in the background (XInput on Windows), even while the emulator
//! has focus.

use crate::config;
use gilrs::{Button, Gilrs};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::AppHandle;

/// Process id of the emulator currently running, if any.
static RUNNING: Mutex<Option<u32>> = Mutex::new(None);
const HOLD: Duration = Duration::from_millis(1500);

pub fn set_running(pid: Option<u32>) {
    *RUNNING.lock().expect("running lock") = pid;
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
        let _ = std::process::Command::new("kill").args(["-TERM", &pid.to_string()]).status();
    }
}

pub fn spawn_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        let Ok(mut gilrs) = Gilrs::new() else { return };
        let mut held_since: Option<Instant> = None;
        loop {
            while gilrs.next_event_blocking(Some(Duration::from_millis(100))).is_some() {}
            let combo = gilrs.gamepads().any(|(_, g)| g.is_pressed(Button::Select) && g.is_pressed(Button::Start));
            match (combo, held_since) {
                (true, None) => held_since = Some(Instant::now()),
                (true, Some(t)) if t.elapsed() >= HOLD => {
                    held_since = None;
                    if RUNNING.lock().expect("running lock").is_some() && config::load(&app).quit_combo {
                        kill_running();
                    }
                }
                (false, _) => held_since = None,
                _ => {}
            }
        }
    });
}
