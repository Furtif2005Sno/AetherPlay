//! Remote play (RetroArch netplay) between AetherPlay users of the same RomM
//! server. Each player runs the game; only controller inputs travel.
//!
//! * The host starts RetroArch as netplay host through a libretro relay
//!   server (no port to open), announced in the libretro lobby with a random
//!   password. AetherPlay finds its room there (by a random nickname) to learn
//!   the relay session.
//! * The invitation is a public note on the game in RomM ("now playing"
//!   already tells who plays what): any user of the server can read it, which
//!   RomM notifications do not allow (only admins notify others). It is
//!   deleted when the game ends.
//! * The guest's AetherPlay lists the notes of games other users are playing,
//!   and joins with RetroArch `--connect` + `--mitm-session`.
//!
//! Both download the ROM from the same RomM, so the content matches.

use crate::romm::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// Title of the invitation note.
pub const NOTE_TITLE: &str = "AetherPlay · partie à distance";
/// libretro relay server used by the host (Europe).
const RELAY: &str = "madrid";
const LOBBY: &str = "http://lobby.libretro.com/list/";

/// How the next launch runs.
#[derive(Clone, Debug)]
pub enum Mode {
    /// `to`: usernames invited (empty: every user of the server).
    Host { to: Vec<String> },
    Join(Session),
}

/// What a guest needs to join (the invitation's content).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Session {
    pub ip: String,
    pub port: u16,
    pub session: String,
    pub password: String,
    /// RetroArch and core versions of the host, shown if they differ.
    #[serde(default)]
    pub retroarch: String,
    #[serde(default)]
    pub core: String,
    /// Usernames invited; empty: everyone. (The note stays readable by every
    /// user of the server: this only decides who is offered to join.)
    #[serde(default)]
    pub to: Vec<String>,
}

static NEXT: Mutex<Option<Mode>> = Mutex::new(None);

/// The next launch is a remote play one.
pub fn prepare(mode: Mode) {
    *NEXT.lock().expect("netplay") = Some(mode);
}

/// Taken by the launch (once).
pub fn take() -> Option<Mode> {
    NEXT.lock().expect("netplay").take()
}

fn random_hex(len: usize) -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    while out.len() < len {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out.truncate(len);
    out
}

/// RetroArch arguments and the extra config file for `mode`. Returns the
/// arguments and, for a host, the nickname to look for in the lobby.
/// A host's room to look for in the lobby: (nickname, password).
pub type Room = (String, String);

pub fn arguments(mode: &Mode, dir: &Path) -> Result<(Vec<String>, Option<Room>), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let cfg = dir.join("netplay.cfg");
    match mode {
        Mode::Host { .. } => {
            let nick = format!("AetherPlay-{}", random_hex(6));
            let password = random_hex(10);
            let text = format!(
                "netplay_use_mitm_server = \"true\"\nnetplay_mitm_server = \"{RELAY}\"\nnetplay_public_announce = \"true\"\n\
                 netplay_password = \"{password}\"\nnetplay_nat_traversal = \"true\"\n"
            );
            std::fs::write(&cfg, text).map_err(|e| e.to_string())?;
            let args = vec!["--appendconfig".into(), path(&cfg), "-H".into(), "--nick".into(), nick.clone()];
            Ok((args, Some((nick, password))))
        }
        Mode::Join(s) => {
            let text = format!("netplay_password = \"{}\"\nnetplay_use_mitm_server = \"false\"\n", s.password);
            std::fs::write(&cfg, text).map_err(|e| e.to_string())?;
            let args = vec![
                "--appendconfig".into(),
                path(&cfg),
                "-C".into(),
                s.ip.clone(),
                "--port".into(),
                s.port.to_string(),
                "--mitm-session".into(),
                s.session.clone(),
                "--nick".into(),
                format!("AetherPlay-{}", random_hex(6)),
            ];
            Ok((args, None))
        }
    }
}

fn path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// The host's room in the libretro lobby (announced a few seconds after the
/// game starts): its relay session.
pub async fn find_room(nick: &str, password: &str, cancelled: impl Fn() -> bool) -> Result<Session, String> {
    let http = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().map_err(|e| e.to_string())?;
    for _ in 0..30 {
        if cancelled() {
            return Err("partie terminée".into());
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
        let Ok(res) = http.get(LOBBY).send().await else { continue };
        let Ok(rooms) = res.json::<Vec<Value>>().await else { continue };
        if let Some(s) = room_session(&rooms, nick, password) {
            return Ok(s);
        }
    }
    Err("la partie n'apparaît pas dans le salon RetroArch (relais injoignable ?)".into())
}

fn room_session(rooms: &[Value], nick: &str, password: &str) -> Option<Session> {
    let f = rooms.iter().map(|r| &r["fields"]).find(|f| f["username"] == nick)?;
    let session = f["mitm_session"].as_str().unwrap_or_default().to_string();
    let (ip, port) = if session.is_empty() {
        // No relay (direct connection): the host's public address.
        (f["ip"].as_str()?.to_string(), f["port"].as_u64()? as u16)
    } else {
        (f["mitm_ip"].as_str()?.to_string(), f["mitm_port"].as_u64()? as u16)
    };
    Some(Session {
        ip,
        port,
        session,
        password: password.to_string(),
        retroarch: f["retroarch_version"].as_str().unwrap_or_default().to_string(),
        core: format!("{} {}", f["core_name"].as_str().unwrap_or_default(), f["core_version"].as_str().unwrap_or_default())
            .trim()
            .to_string(),
        to: Vec::new(),
    })
}

/// A remote play another user of the server offers.
#[derive(Serialize, Clone, Debug)]
pub struct Invite {
    pub rom_id: u64,
    pub rom_name: String,
    pub platform: String,
    pub username: String,
    pub session: Session,
}

/// Invitations of the other users currently playing.
pub async fn invites(client: &Client) -> Result<Vec<Invite>, String> {
    let me_info = client.me().await?;
    let me = me_info["id"].as_u64().unwrap_or(0);
    let my_name = me_info["username"].as_str().unwrap_or_default().to_lowercase();
    let mut out = Vec::new();
    for a in client.activity().await? {
        let Some(user) = a["user_id"].as_u64().filter(|u| *u != me) else { continue };
        let Some(rom_id) = a["rom_id"].as_u64() else { continue };
        let Ok(notes) = client.public_notes(rom_id).await else { continue };
        let found = notes
            .iter()
            .filter(|n| n["title"] == NOTE_TITLE && n["user_id"].as_u64() == Some(user))
            .filter_map(|n| serde_json::from_str::<Session>(n["content"].as_str()?).ok())
            .rfind(|s| s.to.is_empty() || s.to.iter().any(|u| u.to_lowercase() == my_name));
        if let Some(session) = found {
            out.push(Invite {
                rom_id,
                rom_name: a["rom_name"].as_str().unwrap_or_default().to_string(),
                platform: a["platform_slug"].as_str().unwrap_or_default().to_string(),
                username: a["username"].as_str().unwrap_or_default().to_string(),
                session,
            });
        }
    }
    Ok(out)
}

/// Publishes the invitation; returns the note id (deleted at the end).
pub async fn publish(client: &Client, rom_id: u64, session: &Session) -> Result<u64, String> {
    // Notes left by an earlier session that ended badly.
    withdraw_all(client, rom_id).await;
    let body = json!({
        "title": NOTE_TITLE,
        "content": serde_json::to_string(session).map_err(|e| e.to_string())?,
        "is_public": true,
        "tags": ["aetherplay"],
    });
    let note = client.create_note(rom_id, &body).await?;
    note["id"].as_u64().ok_or_else(|| "note sans identifiant".into())
}

/// Removes this user's invitation notes on the game.
pub async fn withdraw_all(client: &Client, rom_id: u64) {
    let Ok(me) = client.me().await else { return };
    let me = me["id"].as_u64();
    if let Ok(notes) = client.public_notes(rom_id).await {
        for n in notes.iter().filter(|n| n["title"] == NOTE_TITLE && n["user_id"].as_u64() == me) {
            if let Some(id) = n["id"].as_u64() {
                let _ = client.delete_note(rom_id, id).await;
            }
        }
    }
}

/// Folder for the generated RetroArch config.
pub fn config_dir(data: &Path) -> PathBuf {
    data.join("netplay")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_is_found_by_nickname() {
        let rooms = vec![
            json!({ "fields": { "username": "someone", "ip": "1.2.3.4", "port": 55435, "mitm_session": "" } }),
            json!({ "fields": { "username": "AetherPlay-abc123", "ip": "", "port": 0, "mitm_ip": "5.6.7.8",
                "mitm_port": 55436, "mitm_session": "deadbeef", "retroarch_version": "1.22.2",
                "core_name": "Mupen64Plus-Next", "core_version": "2.6" } }),
        ];
        let s = room_session(&rooms, "AetherPlay-abc123", "pw").unwrap();
        assert_eq!((s.ip.as_str(), s.port, s.session.as_str(), s.password.as_str()), ("5.6.7.8", 55436, "deadbeef", "pw"));
        assert_eq!(s.core, "Mupen64Plus-Next 2.6");
        assert!(room_session(&rooms, "AetherPlay-zzz", "pw").is_none());
        let direct = room_session(&rooms, "someone", "pw").unwrap();
        assert_eq!((direct.ip.as_str(), direct.port), ("1.2.3.4", 55435));
    }

    #[test]
    fn host_and_join_arguments() {
        let dir = std::env::temp_dir().join(format!("aep-netplay-{}", std::process::id()));
        let (args, room) = arguments(&Mode::Host { to: Vec::new() }, &dir).unwrap();
        let (nick, password) = room.unwrap();
        assert!(args.contains(&"-H".to_string()) && args.contains(&nick));
        let cfg = std::fs::read_to_string(dir.join("netplay.cfg")).unwrap();
        assert!(cfg.contains(&format!("netplay_password = \"{password}\"")) && cfg.contains("netplay_use_mitm_server = \"true\""));
        let s = Session {
            ip: "5.6.7.8".into(),
            port: 55436,
            session: "deadbeef".into(),
            password: "pw".into(),
            retroarch: String::new(),
            core: String::new(),
            to: Vec::new(),
        };
        let (args, room) = arguments(&Mode::Join(s), &dir).unwrap();
        assert!(room.is_none());
        let joined = args.join(" ");
        assert!(joined.contains("-C 5.6.7.8 --port 55436 --mitm-session deadbeef"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
