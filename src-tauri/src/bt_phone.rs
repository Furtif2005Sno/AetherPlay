//! Android controllers connected over Bluetooth only (no Wi-Fi link, e.g. on
//! a network that isolates devices): AetherPlay still tells them the player
//! number and the running game, through the controller itself.
//!
//! The app's HID descriptor (android/…/HidGamepad.kt) has two extra reports:
//! * feature report 3: a 16-byte signature, read to recognise the phone among
//!   the computer's controllers;
//! * output report 2 (31 bytes): version, player number, platform slug (16
//!   bytes ASCII), game name (13 bytes UTF-8): the phone switches to that
//!   console's layout. Sent only when it changes (and once to a new phone).
//!
//! Everything runs on one thread (hidapi handles are not `Send`); the rest of
//! AetherPlay talks to it through a channel.

use crate::shell;
use hidapi::{HidApi, HidDevice};
use std::collections::HashSet;
use std::ffi::CString;
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};
use tauri::AppHandle;

const SIGNATURE: &[u8; 16] = b"AETHERPLAY-PAD01";
const SIGNATURE_REPORT: u8 = 3;
const INFO_REPORT: u8 = 2;
const INFO_SIZE: usize = 31;

enum Command {
    /// Player numbers from the emulator's controller order: HID paths, in order.
    Order(Vec<String>),
    /// The running game (platform slug, name), or None in the menus.
    Game(Option<(String, String)>),
}

static CHANNEL: Mutex<Option<mpsc::Sender<Command>>> = Mutex::new(None);
/// Player numbers of the Bluetooth phones, for the settings page.
static PLAYERS: Mutex<Vec<u8>> = Mutex::new(Vec::new());

fn send(cmd: Command) {
    if let Some(tx) = CHANNEL.lock().expect("bt channel").as_ref() {
        let _ = tx.send(cmd);
    }
}

/// Before a game: number the phones like the emulator orders controllers
/// (`paths`: every controller's HID path, in SDL's order).
pub fn order(paths: Vec<String>) {
    send(Command::Order(paths));
}

pub fn game(platform: &str, name: &str) {
    send(Command::Game(Some((platform.to_string(), name.to_string()))));
}

pub fn idle() {
    send(Command::Game(None));
}

/// Player numbers of the phones connected over Bluetooth.
pub fn players() -> Vec<u8> {
    PLAYERS.lock().expect("bt players").clone()
}

struct Phone {
    path: String,
    device: HidDevice,
    player: u8,
    /// Last report sent, to send only changes.
    sent: Vec<u8>,
}

/// The info report: [id, version, player, platform…, name…].
fn report(player: u8, game: &Option<(String, String)>) -> Vec<u8> {
    let mut r = vec![0u8; INFO_SIZE + 1];
    r[0] = INFO_REPORT;
    r[1] = 1;
    r[2] = player;
    if let Some((platform, name)) = game {
        let p = platform.as_bytes();
        r[3..3 + p.len().min(15)].copy_from_slice(&p[..p.len().min(15)]);
        // Cut the name on a character boundary.
        let mut end = name.len().min(INFO_SIZE - 18);
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        r[19..19 + end].copy_from_slice(&name.as_bytes()[..end]);
    }
    r
}

fn is_phone(device: &HidDevice) -> bool {
    let mut buf = [0u8; 17];
    buf[0] = SIGNATURE_REPORT;
    matches!(device.get_feature_report(&mut buf), Ok(n) if n >= 17 && &buf[1..17] == SIGNATURE)
}

/// Whether the phone controller at HID `path` answers the computer's request
/// for its state, i.e. the AetherPlay app runs on the phone. Windows may keep
/// listing the controller of a phone whose app is closed; it then took a
/// player's place. When the device cannot even be opened, it is kept.
#[cfg(windows)]
pub fn answers(path: &str) -> bool {
    let path = path.to_string();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| {
            let api = HidApi::new().ok()?;
            let device = api.open_path(&CString::new(path).ok()?).ok()?;
            let mut buf = [0u8; 8];
            buf[0] = 1;
            Some(device.get_input_report(&mut buf).is_ok())
        })();
        let _ = tx.send(result);
    });
    match rx.recv_timeout(Duration::from_millis(2500)) {
        Ok(Some(alive)) => alive,
        Ok(None) => true,
        Err(_) => false,
    }
}

#[cfg(not(windows))]
pub fn answers(_path: &str) -> bool {
    true
}

pub fn spawn(app: AppHandle) {
    let (tx, rx) = mpsc::channel();
    *CHANNEL.lock().expect("bt channel") = Some(tx);
    std::thread::spawn(move || {
        let Ok(mut api) = HidApi::new() else {
            shell::log(&app, "manette Bluetooth : accès HID impossible");
            return;
        };
        let mut phones: Vec<Phone> = Vec::new();
        // Controllers already checked and not ours (checked again after a minute).
        let mut others: HashSet<String> = HashSet::new();
        let mut others_since = Instant::now();
        let mut game: Option<(String, String)> = None;
        let mut order: Vec<String> = Vec::new();
        loop {
            match rx.recv_timeout(Duration::from_secs(3)) {
                Ok(Command::Game(g)) => game = g,
                Ok(Command::Order(o)) => order = o,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
            if others_since.elapsed() > Duration::from_secs(60) {
                others.clear();
                others_since = Instant::now();
            }

            // New and gone phones.
            let _ = api.refresh_devices();
            let present: Vec<String> = api
                .device_list()
                .filter(|d| d.usage_page() == 0x01 && d.usage() == 0x05)
                .map(|d| d.path().to_string_lossy().into_owned())
                .collect();
            let before = phones.len();
            phones.retain(|p| present.contains(&p.path));
            for path in &present {
                if phones.iter().any(|p| &p.path == path) || others.contains(path) {
                    continue;
                }
                let Ok(c) = CString::new(path.clone()) else { continue };
                match api.open_path(&c) {
                    Ok(device) if is_phone(&device) => {
                        shell::log(&app, "manette Android connectée en Bluetooth");
                        phones.push(Phone { path: path.clone(), device, player: 0, sent: Vec::new() });
                    }
                    _ => {
                        others.insert(path.clone());
                    }
                }
            }

            // Player numbers: the emulator's order when known, else connection order.
            let mut numbers: Vec<u8> = phones
                .iter()
                .map(|p| {
                    order
                        .iter()
                        .position(|o| o.eq_ignore_ascii_case(&p.path))
                        .map_or(0, |i| (i + 1) as u8)
                })
                .collect();
            if numbers.contains(&0) {
                numbers = (1..=phones.len() as u8).collect();
            }
            if phones.len() != before {
                shell::log(&app, &format!("manettes Android en Bluetooth : {}", phones.len()));
            }
            for (p, n) in phones.iter_mut().zip(&numbers) {
                p.player = *n;
            }
            *PLAYERS.lock().expect("bt players") = numbers;
            for p in &mut phones {
                let r = report(p.player, &game);
                if r != p.sent && p.device.write(&r).is_ok() {
                    p.sent = r;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_report_layout() {
        let r = report(2, &Some(("snes".into(), "Super Mario Kart".into())));
        assert_eq!(r.len(), 32);
        assert_eq!(&r[..3], &[INFO_REPORT, 1, 2]);
        assert_eq!(&r[3..7], b"snes");
        assert_eq!(r[7], 0);
        // 13 bytes of name.
        assert_eq!(&r[19..32], b"Super Mario K");
        // Menus: no platform.
        assert_eq!(report(1, &None)[3], 0);
        // Long names are cut on a character boundary.
        let long = "é".repeat(40);
        let r = report(1, &Some(("nds".into(), long)));
        assert!(std::str::from_utf8(r[19..].split(|b| *b == 0).next().unwrap()).is_ok());
    }
}
