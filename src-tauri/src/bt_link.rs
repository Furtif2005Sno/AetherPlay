//! Bluetooth serial channel with the Android app (Windows), beside the phone's
//! Bluetooth controller: no Wi-Fi needed for the running game (console
//! layout, cover), the player number and the "quit game" button.
//!
//! Every few seconds AetherPlay looks at the Bluetooth devices connected to
//! this computer and tries the app's RFCOMM service on them (only the phone
//! has it); a linked phone gets the same JSON messages as over Wi-Fi, one
//! per line. The controller itself is untouched: if this channel fails, the
//! controller keeps working.

#[cfg(not(windows))]
pub fn spawn(_app: tauri::AppHandle) {}
#[cfg(not(windows))]
pub fn order(_positions: Vec<usize>) {}
#[cfg(not(windows))]
pub fn connected_phones() -> Vec<String> {
    Vec::new()
}

#[cfg(windows)]
pub use win::{connected_phones, order, spawn};

/// One message line: the JSON text and a newline.
#[cfg_attr(not(windows), allow(dead_code))]
fn line(msg: &str) -> Vec<u8> {
    let mut v = msg.replace('\n', " ").into_bytes();
    v.push(b'\n');
    v
}

#[cfg(windows)]
mod win {
    use super::line;
    use crate::{gamepad, phone, shell};
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use std::sync::{Mutex, mpsc};
    use std::time::{Duration, Instant};
    use tauri::AppHandle;
    use windows::Win32::Devices::Bluetooth::{
        AF_BTH, BLUETOOTH_DEVICE_INFO, BLUETOOTH_DEVICE_SEARCH_PARAMS, BTHPROTO_RFCOMM, BluetoothFindDeviceClose,
        BluetoothFindFirstDevice, BluetoothFindNextDevice, SOCKADDR_BTH,
    };
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Networking::WinSock::{
        SEND_RECV_FLAGS, SO_RCVTIMEO, SOCK_STREAM, SOCKADDR, SOCKET, SOL_SOCKET, WSADATA, WSAStartup, closesocket,
        connect, recv, send, setsockopt, socket,
    };

    /// The app's service (android/…/bt/BtLink.kt): the standard Serial Port
    /// Profile, which Windows has a driver for. Other devices may offer it
    /// too, so nothing is sent before the app's hello.
    const SERVICE: windows::core::GUID = windows::core::GUID::from_u128(0x00001101_0000_1000_8000_00805f9b34fb);

    /// Waits up to 3 s for the app's `{"t":"hello","v":1,…}` line; its name.
    fn handshake(s: SOCKET) -> Option<String> {
        let mut pending = Vec::new();
        let mut buf = [0u8; 512];
        for _ in 0..12 {
            // SAFETY: reading into a local buffer from our socket (250 ms timeout).
            let n = unsafe { recv(s, &mut buf, SEND_RECV_FLAGS(0)) };
            if n == 0 {
                return None;
            }
            if n > 0 {
                pending.extend_from_slice(&buf[..n as usize]);
                if let Some(i) = pending.iter().position(|b| *b == b'\n') {
                    let msg: Value = serde_json::from_slice(&pending[..i]).ok()?;
                    return (msg["t"] == "hello" && msg["v"].as_u64().is_some())
                        .then(|| msg["name"].as_str().unwrap_or("Android").to_string());
                }
                if pending.len() > 4096 {
                    return None;
                }
            }
        }
        None
    }

    /// Linked phones: Bluetooth address -> channel to their thread.
    static LINKS: Mutex<Vec<(u64, mpsc::Sender<u8>)>> = Mutex::new(Vec::new());

    /// Before a game: phones get the player number of their controller in the
    /// emulator's order (`positions`: indexes of the Android controllers among
    /// all controllers), in the order they were linked.
    pub fn order(positions: Vec<usize>) {
        let links = LINKS.lock().expect("bt links");
        for ((_, tx), pos) in links.iter().zip(positions) {
            let _ = tx.send((pos + 1) as u8);
        }
    }

    /// Bluetooth devices connected to this computer: (address, name).
    fn connected_devices() -> Vec<(u64, String)> {
        let mut out = Vec::new();
        // SAFETY: plain structs filled by the Bluetooth API; the search handle is closed.
        unsafe {
            let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
                dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
                fReturnAuthenticated: true.into(),
                fReturnRemembered: false.into(),
                fReturnUnknown: false.into(),
                fReturnConnected: true.into(),
                fIssueInquiry: false.into(),
                cTimeoutMultiplier: 0,
                hRadio: HANDLE::default(),
            };
            let mut info: BLUETOOTH_DEVICE_INFO = std::mem::zeroed();
            info.dwSize = std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
            let Ok(find) = BluetoothFindFirstDevice(&params, &mut info) else { return out };
            loop {
                if info.fConnected.as_bool() {
                    let end = info.szName.iter().position(|c| *c == 0).unwrap_or(info.szName.len());
                    out.push((info.Address.Anonymous.ullLong, String::from_utf16_lossy(&info.szName[..end])));
                }
                if BluetoothFindNextDevice(find, &mut info).is_err() {
                    break;
                }
            }
            let _ = BluetoothFindDeviceClose(find);
        }
        out
    }

    /// Names of the phones connected to this computer by Bluetooth (major
    /// device class 2 = phone), in Windows' order.
    pub fn connected_phones() -> Vec<String> {
        connected_devices_with_class().into_iter().filter(|(_, _, class)| (class >> 8) & 0x1F == 0x02).map(|(_, n, _)| n).collect()
    }

    fn connected_devices_with_class() -> Vec<(u64, String, u32)> {
        let mut out = Vec::new();
        // SAFETY: same as connected_devices.
        unsafe {
            let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
                dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
                fReturnAuthenticated: true.into(),
                fReturnRemembered: false.into(),
                fReturnUnknown: false.into(),
                fReturnConnected: true.into(),
                fIssueInquiry: false.into(),
                cTimeoutMultiplier: 0,
                hRadio: HANDLE::default(),
            };
            let mut info: BLUETOOTH_DEVICE_INFO = std::mem::zeroed();
            info.dwSize = std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
            let Ok(find) = BluetoothFindFirstDevice(&params, &mut info) else { return out };
            loop {
                if info.fConnected.as_bool() {
                    let end = info.szName.iter().position(|c| *c == 0).unwrap_or(info.szName.len());
                    out.push((info.Address.Anonymous.ullLong, String::from_utf16_lossy(&info.szName[..end]), info.ulClassofDevice));
                }
                if BluetoothFindNextDevice(find, &mut info).is_err() {
                    break;
                }
            }
            let _ = BluetoothFindDeviceClose(find);
        }
        out
    }

    /// Opens the app's channel on device `addr` (Windows looks the service up).
    fn open(addr: u64) -> Option<SOCKET> {
        // SAFETY: Winsock calls on a socket owned here, closed on failure.
        unsafe {
            let s = socket(AF_BTH as i32, SOCK_STREAM, BTHPROTO_RFCOMM as i32).ok()?;
            let sa = SOCKADDR_BTH { addressFamily: AF_BTH, btAddr: addr, serviceClassId: SERVICE, port: 0 };
            if connect(s, &sa as *const _ as *const SOCKADDR, std::mem::size_of::<SOCKADDR_BTH>() as i32) != 0 {
                closesocket(s);
                return None;
            }
            // Short reads, so the thread also gets to send.
            let timeout: u32 = 250;
            setsockopt(s, SOL_SOCKET, SO_RCVTIMEO, Some(&timeout.to_le_bytes()));
            Some(s)
        }
    }

    fn write(s: SOCKET, msg: &str) -> bool {
        let data = line(msg);
        let mut sent = 0;
        while sent < data.len() {
            // SAFETY: sending from a live buffer on our socket.
            let n = unsafe { send(s, &data[sent..], SEND_RECV_FLAGS(0)) };
            if n <= 0 {
                return false;
            }
            sent += n as usize;
        }
        true
    }

    /// One linked phone, until the link drops.
    fn serve(app: AppHandle, addr: u64, name: String, s: SOCKET, players: mpsc::Receiver<u8>) {
        // `name`: the phone's model from its hello.
        let (id, mut player) = phone::bluetooth_join(&name);
        shell::log(&app, &format!("manette Android liée en Bluetooth : {name} (joueur {player})"));
        let mut events = phone::subscribe();
        let mut ok = write(s, &phone::current_message()) && write(s, &json!({ "t": "player", "n": player }).to_string());
        let mut pending = Vec::new();
        let mut buf = [0u8; 1024];
        while ok {
            while let Ok(p) = players.try_recv() {
                player = p;
                phone::set_player(id, p);
                ok &= write(s, &json!({ "t": "player", "n": p }).to_string());
            }
            loop {
                match events.try_recv() {
                    Ok(msg) => ok &= write(s, &msg),
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            // SAFETY: reading into a local buffer from our socket (250 ms timeout).
            let n = unsafe { recv(s, &mut buf, SEND_RECV_FLAGS(0)) };
            if n == 0 {
                break;
            }
            if n > 0 {
                pending.extend_from_slice(&buf[..n as usize]);
                while let Some(i) = pending.iter().position(|b| *b == b'\n') {
                    let text: Vec<u8> = pending.drain(..=i).collect();
                    if let Ok(msg) = serde_json::from_slice::<Value>(&text)
                        && msg["t"] == "quit"
                    {
                        gamepad::quit_game();
                    }
                }
            } else {
                // Timeout (WSAETIMEDOUT) is the normal case; anything else ends the link.
                let err = unsafe { windows::Win32::Networking::WinSock::WSAGetLastError() };
                if err.0 != 10060 {
                    break;
                }
            }
        }
        // SAFETY: closing our socket once.
        unsafe { closesocket(s) };
        phone::bluetooth_leave(id);
        LINKS.lock().expect("bt links").retain(|(a, _)| *a != addr);
        shell::log(&app, &format!("liaison Bluetooth terminée : {name} (joueur {player})"));
    }

    /// cargo test list_bluetooth -- --ignored --nocapture
    #[test]
    #[ignore]
    fn list_bluetooth() {
        for (addr, name) in connected_devices() {
            println!("{addr:012x} {name}");
        }
        println!("liste terminée");
    }

    pub fn spawn(app: AppHandle) {
        std::thread::spawn(move || {
            // SAFETY: Winsock initialisation (2.2) for this process.
            unsafe {
                let mut data = WSADATA::default();
                WSAStartup(0x0202, &mut data);
            }
            // Devices without the service, tried again after a minute.
            let mut tried: HashMap<u64, Instant> = HashMap::new();
            loop {
                for (addr, _bt_name) in connected_devices() {
                    if LINKS.lock().expect("bt links").iter().any(|(a, _)| *a == addr) {
                        continue;
                    }
                    if tried.get(&addr).is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
                        continue;
                    }
                    tried.insert(addr, Instant::now());
                    if let Some(s) = open(addr) {
                        // Only the AetherPlay app introduces itself.
                        let Some(name) = handshake(s) else {
                            // SAFETY: closing the socket we opened.
                            unsafe { closesocket(s) };
                            continue;
                        };
                        tried.remove(&addr);
                        let (tx, rx) = mpsc::channel();
                        LINKS.lock().expect("bt links").push((addr, tx));
                        let app = app.clone();
                        std::thread::spawn(move || serve(app, addr, name, s, rx));
                    }
                }
                std::thread::sleep(Duration::from_secs(5));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn messages_are_single_lines() {
        assert_eq!(super::line("{\"t\":\"idle\"}"), b"{\"t\":\"idle\"}\n");
        assert_eq!(super::line("a\nb"), b"a b\n");
    }
}
