//! Link with the AetherPlay Android app (controller on the phone).
//!
//! * Announced on the local network over mDNS (`_aetherplay._tcp`), WebSocket
//!   server on port 47820. Protocol: AetherPlay-Android/PROTOCOL.md.
//! * Pairing: the phone types once the 6-digit code shown in the AetherPlay
//!   settings and gets a token, kept in config.json.
//! * AetherPlay tells the phones which game runs (console, name, cover): the
//!   phone switches to that console's controller layout.
//! * The phone's state always drives the AetherPlay menus. Games get it over
//!   Bluetooth (the phone is a Bluetooth gamepad) or, without Bluetooth,
//!   through a virtual Xbox 360 controller (uinput on Linux, ViGEmBus on
//!   Windows; none on macOS).

use crate::{config, gamepad, romm::Rom, shell};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::{LazyLock, Mutex};
use tauri::AppHandle;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

pub const PORT: u16 = 47820;
const SERVICE: &str = "_aetherplay._tcp.local.";

struct Shared {
    /// Code to type on a new phone; changes at each start of AetherPlay.
    pin: String,
    /// Last "game" / "idle" message, sent to phones as they connect.
    current: Mutex<String>,
    events: broadcast::Sender<String>,
    /// Second-screen images (JPEG), see dualscreen.rs.
    frames: broadcast::Sender<std::sync::Arc<Vec<u8>>>,
    /// Current "screen" message, sent to phones as they connect.
    screen: Mutex<Option<String>>,
    /// Connected phones (RetroTogether: one player each).
    phones: Mutex<Vec<Phone>>,
}

#[derive(Clone, serde::Serialize)]
struct Phone {
    #[serde(skip)]
    id: u64,
    name: String,
    /// Player number shown on the phone: 1, 2, 3…
    player: u8,
}

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Registers a phone under the lowest free player number.
fn join(name: &str) -> Phone {
    let mut phones = SHARED.phones.lock().expect("phones");
    let player = (1..=u8::MAX).find(|n| phones.iter().all(|p| p.player != *n)).unwrap_or(u8::MAX);
    let phone = Phone { id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed), name: name.to_string(), player };
    phones.push(phone.clone());
    phones.sort_by_key(|p| p.player);
    phone
}

fn leave(id: u64) {
    SHARED.phones.lock().expect("phones").retain(|p| p.id != id);
}

static SHARED: LazyLock<Shared> = LazyLock::new(|| Shared {
    pin: format!("{:06}", random_u64() % 1_000_000),
    current: Mutex::new(json!({ "t": "idle" }).to_string()),
    events: broadcast::channel(16).0,
    frames: broadcast::channel(4).0,
    screen: Mutex::new(None),
    phones: Mutex::new(Vec::new()),
});

fn random_u64() -> u64 {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).expect("random");
    u64::from_le_bytes(b)
}

fn token() -> String {
    let mut b = [0u8; 24];
    getrandom::fill(&mut b).expect("random");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Pairing code and connected phones, for the settings page.
pub fn status() -> Value {
    json!({
        "pin": SHARED.pin,
        "port": PORT,
        "phones": SHARED.phones.lock().expect("phones").clone(),
        // Can Wi-Fi input reach the games (virtual controller)?
        "virtual": virtual_pad::available(),
        // To type on the phone when it cannot find AetherPlay by itself.
        "addresses": addresses(),
        "os": crate::emulators::CURRENT_OS,
    })
}

fn publish(msg: String) {
    *SHARED.current.lock().expect("current") = msg.clone();
    let _ = SHARED.events.send(msg);
}

/// A game started: phones switch to its console's controller.
pub fn game_started(rom: &Rom, cover: Option<Vec<u8>>) {
    let cover = cover.map(|c| base64::engine::general_purpose::STANDARD.encode(c));
    publish(
        json!({
            "t": "game",
            "platform": rom.platform_slug,
            "name": rom.name.as_deref().unwrap_or(&rom.fs_name),
            "cover": cover,
        })
        .to_string(),
    );
}

/// The game's second screen is streamed (`Some(width, height)`) or not.
pub fn screen(size: Option<(i32, i32)>) {
    let msg = match size {
        Some((w, h)) => json!({ "t": "screen", "on": true, "w": w, "h": h }),
        None => json!({ "t": "screen", "on": false }),
    }
    .to_string();
    *SHARED.screen.lock().expect("screen") = size.map(|_| msg.clone());
    let _ = SHARED.events.send(msg);
}

/// One image of the second screen, for every connected phone.
pub fn frame(jpeg: Vec<u8>) {
    let _ = SHARED.frames.send(std::sync::Arc::new(jpeg));
}

/// Whether a phone is connected (the second screen needs one).
pub fn any_connected() -> bool {
    !SHARED.phones.lock().expect("phones").is_empty()
}

/// Back to the menus.
pub fn game_ended() {
    publish(json!({ "t": "idle" }).to_string());
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::bind(("0.0.0.0", PORT)).await {
            Ok(l) => l,
            Err(e) => {
                shell::log(&app, &format!("manette Android : port {PORT} indisponible ({e})"));
                return;
            }
        };
        let _mdns = announce(&app);
        loop {
            let Ok((stream, addr)) = listener.accept().await else { continue };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = serve(&app, stream).await {
                    shell::log(&app, &format!("manette Android {addr} : {e}"));
                }
            });
        }
    });
}

/// Makes AetherPlay findable by the phone; kept alive by the returned daemon.
fn announce(app: &AppHandle) -> Option<mdns_sd::ServiceDaemon> {
    let daemon = mdns_sd::ServiceDaemon::new().ok()?;
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let host = if host.is_empty() { "AetherPlay".to_string() } else { host };
    let info = mdns_sd::ServiceInfo::new(SERVICE, &host, &format!("{host}.local."), "", PORT, &[("v", "1")][..])
        .ok()?
        .enable_addr_auto();
    match daemon.register(info) {
        Ok(()) => Some(daemon),
        Err(e) => {
            shell::log(app, &format!("manette Android : annonce réseau impossible ({e})"));
            None
        }
    }
}

async fn serve(app: &AppHandle, stream: TcpStream) -> Result<(), String> {
    let ws = tokio_tungstenite::accept_async(stream).await.map_err(|e| e.to_string())?;
    let (mut tx, mut rx) = ws.split();
    let send = |v: Value| Message::text(v.to_string());

    // Pairing: a known token, or the code shown in AetherPlay.
    let name = loop {
        // The phone pings every few seconds: wait for the code past those.
        let text = match rx.next().await {
            Some(Ok(Message::Text(text))) => text,
            Some(Ok(Message::Close(_)) | Err(_)) | None => return Ok(()),
            Some(Ok(_)) => continue,
        };
        let msg: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if msg["t"] != "hello" {
            continue;
        }
        let name = msg["name"].as_str().unwrap_or("Android").chars().take(60).collect::<String>();
        let mut cfg = config::load(app);
        if msg["token"].as_str().is_some_and(|t| cfg.phones.contains_key(t)) {
            tx.send(send(json!({ "t": "welcome", "server": host_name() }))).await.map_err(|e| e.to_string())?;
            break name;
        }
        match msg["pin"].as_str() {
            Some(pin) if pin == SHARED.pin => {
                let t = token();
                cfg.phones.insert(t.clone(), name.clone());
                config::save(app, &cfg)?;
                tx.send(send(json!({ "t": "welcome", "server": host_name(), "token": t }))).await.map_err(|e| e.to_string())?;
                break name;
            }
            Some(_) => tx.send(send(json!({ "t": "denied", "retry": true, "reason": "Code incorrect" }))).await,
            None => tx.send(send(json!({ "t": "pin" }))).await,
        }
        .map_err(|e| e.to_string())?;
    };
    let me = join(&name);
    shell::log(app, &format!("manette Android connectée : {name} (joueur {})", me.player));
    tx.send(send(json!({ "t": "player", "n": me.player }))).await.map_err(|e| e.to_string())?;

    let mut events = SHARED.events.subscribe();
    let mut frames = SHARED.frames.subscribe();
    let current = SHARED.current.lock().expect("current").clone();
    let screen_state = SHARED.screen.lock().expect("screen").clone();
    tx.send(Message::text(current)).await.map_err(|e| e.to_string())?;
    if let Some(screen) = screen_state {
        tx.send(Message::text(screen)).await.map_err(|e| e.to_string())?;
    }
    // One game controller per phone: a virtual one when this computer can
    // create it (players then get ports in order), else the phone's Bluetooth.
    let virtual_ok = pads_add(me.id, me.player);
    tx.send(send(json!({ "t": "virtual", "ok": virtual_ok }))).await.map_err(|e| e.to_string())?;
    tx.send(send(json!({ "t": "route", "bluetooth": !virtual_ok }))).await.map_err(|e| e.to_string())?;
    if !virtual_ok {
        shell::log(app, "manette Android : pas de manette virtuelle, Bluetooth nécessaire pour les jeux");
    }
    let mut last_try = std::time::Instant::now();
    let result: Result<(), String> = async {
        loop {
            tokio::select! {
                msg = rx.next() => {
                    let Some(Ok(msg)) = msg else { return Ok(()) };
                    let Message::Text(text) = msg else { continue };
                    let Ok(msg) = serde_json::from_str::<Value>(&text) else { continue };
                    match msg["t"].as_str() {
                        Some("pad") => {
                            let buttons = msg["b"].as_u64().unwrap_or(0) as u32;
                            let mut axes = [0f32; 4];
                            for (i, a) in msg["a"].as_array().into_iter().flatten().take(4).enumerate() {
                                axes[i] = a.as_f64().unwrap_or(0.0).clamp(-1.0, 1.0) as f32;
                            }
                            let bluetooth = msg["bt"].as_bool().unwrap_or(false);
                            // Driver installed meanwhile (Windows): switch the phone to it.
                            if !pads_has(me.id) && last_try.elapsed() > std::time::Duration::from_secs(5) {
                                last_try = std::time::Instant::now();
                                if pads_add(me.id, me.player) {
                                    tx.send(send(json!({ "t": "virtual", "ok": true }))).await.map_err(|e| e.to_string())?;
                                    tx.send(send(json!({ "t": "route", "bluetooth": false }))).await.map_err(|e| e.to_string())?;
                                }
                            }
                            let has_pad = pads_update(me.id, buttons, axes);
                            // On Windows the interface sees Bluetooth and virtual pads
                            // by itself; otherwise AetherPlay hands it the phone state.
                            let ui = !cfg!(windows) || (!bluetooth && !has_pad);
                            gamepad::set_remote(me.id, Some((buttons, axes)), ui);
                        }
                        Some("quit") => gamepad::quit_game(),
                        Some("touch") => crate::dualscreen::touch(
                            msg["x"].as_f64().unwrap_or(0.0) as f32,
                            msg["y"].as_f64().unwrap_or(0.0) as f32,
                            msg["down"].as_bool().unwrap_or(false),
                        ),
                        _ => {}
                    }
                }
                event = events.recv() => {
                    if let Ok(event) = event {
                        tx.send(Message::text(event)).await.map_err(|e| e.to_string())?;
                    }
                }
                // Images skipped when the phone lags behind (Lagged): only the latest matters.
                frame = frames.recv() => {
                    if let Ok(jpeg) = frame {
                        tx.send(Message::binary(jpeg.to_vec())).await.map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }
    .await;

    gamepad::set_remote(me.id, None, false);
    pads_remove(me.id);
    leave(me.id);
    shell::log(app, &format!("manette Android déconnectée : {name}"));
    result
}

/// Virtual controllers by phone connection, with their player number.
static PADS: Mutex<Vec<(u64, u8, Option<virtual_pad::VirtualPad>)>> = Mutex::new(Vec::new());

/// Gives phone `id` a virtual controller; false if this computer cannot.
fn pads_add(id: u64, player: u8) -> bool {
    let pad = virtual_pad::VirtualPad::new().ok();
    let ok = pad.is_some();
    let mut pads = PADS.lock().expect("pads");
    pads.retain(|p| p.0 != id);
    pads.push((id, player, pad));
    ok
}

fn pads_has(id: u64) -> bool {
    PADS.lock().expect("pads").iter().any(|p| p.0 == id && p.2.is_some())
}

/// Feeds phone `id`'s virtual controller; false if it has none.
fn pads_update(id: u64, buttons: u32, axes: [f32; 4]) -> bool {
    let mut pads = PADS.lock().expect("pads");
    match pads.iter_mut().find(|p| p.0 == id).and_then(|p| p.2.as_mut()) {
        Some(pad) => {
            pad.update(buttons, axes);
            true
        }
        None => false,
    }
}

fn pads_remove(id: u64) {
    PADS.lock().expect("pads").retain(|p| p.0 != id);
}

/// RetroTogether: before a game starts, recreate the virtual controllers in
/// player order, so the emulator numbers them J1, J2… (it assigns ports in
/// the order controllers appeared).
pub fn order_players() {
    let mut pads = PADS.lock().expect("pads");
    if pads.iter().filter(|p| p.2.is_some()).count() < 2 {
        return;
    }
    pads.sort_by_key(|p| p.1);
    for p in pads.iter_mut() {
        p.2 = None;
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    for p in pads.iter_mut() {
        p.2 = virtual_pad::VirtualPad::new().ok();
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
}

/// This computer's IPv4 addresses (Wi-Fi, Ethernet, USB tethering…).
fn addresses() -> Vec<String> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(ip) if !ip.is_link_local() => Some(ip.to_string()),
            _ => None,
        })
        .collect()
}

fn host_name() -> String {
    gethostname::gethostname().to_string_lossy().into_owned()
}

/// Virtual Xbox 360 controller fed by the phone over Wi-Fi.
#[cfg_attr(target_os = "macos", allow(dead_code))]
mod virtual_pad {
    // W3C standard indices, as sent by the phone.
    const SOUTH: u32 = 0;
    const EAST: u32 = 1;
    const WEST: u32 = 2;
    const NORTH: u32 = 3;
    const LB: u32 = 4;
    const RB: u32 = 5;
    const LT: u32 = 6;
    const RT: u32 = 7;
    const SELECT: u32 = 8;
    const START: u32 = 9;
    const L3: u32 = 10;
    const R3: u32 = 11;
    const UP: u32 = 12;
    const DOWN: u32 = 13;
    const LEFT: u32 = 14;
    const RIGHT: u32 = 15;
    const HOME: u32 = 16;

    fn on(b: u32, i: u32) -> bool {
        b & (1 << i) != 0
    }

    #[cfg(windows)]
    pub fn available() -> bool {
        vigem_client::Client::connect().is_ok()
    }

    #[cfg(target_os = "linux")]
    pub fn available() -> bool {
        std::fs::OpenOptions::new().write(true).open("/dev/uinput").is_ok()
    }

    #[cfg(target_os = "macos")]
    pub fn available() -> bool {
        false
    }

    #[cfg(windows)]
    pub struct VirtualPad(vigem_client::Xbox360Wired<vigem_client::Client>);

    #[cfg(windows)]
    impl VirtualPad {
        /// Needs the ViGEmBus driver (free, https://github.com/nefarius/ViGEmBus).
        pub fn new() -> Result<Self, String> {
            let client = vigem_client::Client::connect().map_err(|e| format!("pilote ViGEmBus absent ({e:?})"))?;
            let mut pad = vigem_client::Xbox360Wired::new(client, vigem_client::TargetId::XBOX360_WIRED);
            pad.plugin().map_err(|e| format!("{e:?}"))?;
            pad.wait_ready().map_err(|e| format!("{e:?}"))?;
            Ok(Self(pad))
        }

        pub fn update(&mut self, b: u32, axes: [f32; 4]) {
            use vigem_client::XButtons as X;
            let map = [
                (SOUTH, X::A), (EAST, X::B), (WEST, X::X), (NORTH, X::Y), (LB, X::LB), (RB, X::RB),
                (SELECT, X::BACK), (START, X::START), (L3, X::LTHUMB), (R3, X::RTHUMB),
                (UP, X::UP), (DOWN, X::DOWN), (LEFT, X::LEFT), (RIGHT, X::RIGHT), (HOME, X::GUIDE),
            ];
            let raw = map.iter().filter(|(i, _)| on(b, *i)).fold(0u16, |acc, (_, x)| acc | x);
            let axis = |v: f32| (v * 32767.0) as i16;
            let _ = self.0.update(&vigem_client::XGamepad {
                buttons: X { raw },
                left_trigger: if on(b, LT) { 255 } else { 0 },
                right_trigger: if on(b, RT) { 255 } else { 0 },
                thumb_lx: axis(axes[0]),
                // XInput Y points up.
                thumb_ly: axis(-axes[1]),
                thumb_rx: axis(axes[2]),
                thumb_ry: axis(-axes[3]),
            });
        }
    }

    #[cfg(target_os = "linux")]
    pub struct VirtualPad(evdev::uinput::VirtualDevice);

    #[cfg(target_os = "linux")]
    impl VirtualPad {
        /// Same identity and layout as the xpad driver gives an Xbox 360 pad, so
        /// SDL (emulators) and gilrs (AetherPlay) map it without configuration.
        pub fn new() -> Result<Self, String> {
            use evdev::{AbsInfo, AbsoluteAxisCode as A, AttributeSet, BusType, InputId, KeyCode as K, UinputAbsSetup};
            let mut keys = AttributeSet::<K>::new();
            for k in [
                K::BTN_SOUTH, K::BTN_EAST, K::BTN_NORTH, K::BTN_WEST, K::BTN_TL, K::BTN_TR,
                K::BTN_SELECT, K::BTN_START, K::BTN_MODE, K::BTN_THUMBL, K::BTN_THUMBR,
            ] {
                keys.insert(k);
            }
            let stick = AbsInfo::new(0, -32768, 32767, 16, 128, 0);
            let trigger = AbsInfo::new(0, 0, 255, 0, 0, 0);
            let hat = AbsInfo::new(0, -1, 1, 0, 0, 0);
            let device = evdev::uinput::VirtualDevice::builder()
                .map_err(|e| format!("/dev/uinput : {e}"))?
                .name("AetherPlay Manette (Wi-Fi)")
                .input_id(InputId::new(BusType::BUS_USB, 0x045e, 0x028e, 0x0110))
                .with_keys(&keys)
                .and_then(|b| b.with_absolute_axis(&UinputAbsSetup::new(A::ABS_X, stick)))
                .and_then(|b| b.with_absolute_axis(&UinputAbsSetup::new(A::ABS_Y, stick)))
                .and_then(|b| b.with_absolute_axis(&UinputAbsSetup::new(A::ABS_Z, trigger)))
                .and_then(|b| b.with_absolute_axis(&UinputAbsSetup::new(A::ABS_RX, stick)))
                .and_then(|b| b.with_absolute_axis(&UinputAbsSetup::new(A::ABS_RY, stick)))
                .and_then(|b| b.with_absolute_axis(&UinputAbsSetup::new(A::ABS_RZ, trigger)))
                .and_then(|b| b.with_absolute_axis(&UinputAbsSetup::new(A::ABS_HAT0X, hat)))
                .and_then(|b| b.with_absolute_axis(&UinputAbsSetup::new(A::ABS_HAT0Y, hat)))
                .and_then(|b| b.build())
                .map_err(|e| format!("/dev/uinput : {e}"))?;
            Ok(Self(device))
        }

        pub fn update(&mut self, b: u32, axes: [f32; 4]) {
            use evdev::{AbsoluteAxisCode as A, EventType, InputEvent, KeyCode as K};
            let key = |k: K, i: u32| InputEvent::new(EventType::KEY.0, k.0, on(b, i) as i32);
            let abs = |a: A, v: i32| InputEvent::new(EventType::ABSOLUTE.0, a.0, v);
            let axis = |v: f32| (v * 32767.0) as i32;
            let dir = |neg: u32, pos: u32| on(b, pos) as i32 - on(b, neg) as i32;
            let events = [
                // xpad: BTN_NORTH is X (left face), BTN_WEST is Y (top face).
                key(K::BTN_SOUTH, SOUTH), key(K::BTN_EAST, EAST), key(K::BTN_NORTH, WEST), key(K::BTN_WEST, NORTH),
                key(K::BTN_TL, LB), key(K::BTN_TR, RB), key(K::BTN_SELECT, SELECT), key(K::BTN_START, START),
                key(K::BTN_MODE, HOME), key(K::BTN_THUMBL, L3), key(K::BTN_THUMBR, R3),
                abs(A::ABS_X, axis(axes[0])), abs(A::ABS_Y, axis(axes[1])),
                abs(A::ABS_RX, axis(axes[2])), abs(A::ABS_RY, axis(axes[3])),
                abs(A::ABS_Z, if on(b, LT) { 255 } else { 0 }), abs(A::ABS_RZ, if on(b, RT) { 255 } else { 0 }),
                abs(A::ABS_HAT0X, dir(LEFT, RIGHT)), abs(A::ABS_HAT0Y, dir(UP, DOWN)),
            ];
            let _ = self.0.emit(&events);
        }
    }

    #[cfg(target_os = "macos")]
    pub struct VirtualPad;

    #[cfg(target_os = "macos")]
    impl VirtualPad {
        /// macOS offers no virtual controllers to apps: games need Bluetooth.
        pub fn new() -> Result<Self, String> {
            Err("impossible sur macOS".into())
        }

        pub fn update(&mut self, _b: u32, _axes: [f32; 4]) {}
    }
}
