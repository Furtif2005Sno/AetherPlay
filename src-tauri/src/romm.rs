//! Minimal RomM REST client (HTTP Basic auth).

use futures_util::StreamExt;
use reqwest::{RequestBuilder, Url};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::Path;
use tokio::io::AsyncWriteExt;

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: Url,
    user: String,
    pass: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Rom {
    pub id: u64,
    pub platform_id: u64,
    pub platform_slug: String,
    #[serde(default)]
    pub name: Option<String>,
    pub fs_name: String,
    #[serde(default)]
    pub fs_size_bytes: u64,
    #[serde(default)]
    pub path_cover_small: Option<String>,
    #[serde(default)]
    pub path_cover_large: Option<String>,
    /// RomM 5 names it `has_multiple_files`; older versions `multi`.
    #[serde(default, alias = "has_multiple_files")]
    pub multi: bool,
}

#[derive(Deserialize, Clone)]
pub struct Save {
    pub id: u64,
    pub file_name: String,
    #[serde(default)]
    pub emulator: Option<String>,
    #[serde(default)]
    pub slot: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SaveList {
    Page { items: Vec<Save> },
    List(Vec<Save>),
}

pub struct SaveUpload<'a> {
    pub rom_id: u64,
    pub emulator: &'a str,
    pub core: Option<&'a str>,
    pub slot: &'a str,
    pub file_name: &'a str,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl Client {
    pub fn new(base: &str, user: &str, pass: &str) -> Result<Self, String> {
        let mut base = Url::parse(base.trim()).map_err(|e| format!("URL invalide : {e}"))?;
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        Ok(Self { http: reqwest::Client::new(), base, user: user.into(), pass: pass.into() })
    }

    /// Builds `base/<segments...>` with each segment percent-encoded.
    fn url(&self, segments: &[&str]) -> Url {
        let mut url = self.base.clone();
        url.path_segments_mut().expect("http url").pop_if_empty().extend(segments);
        url
    }

    fn get(&self, url: Url) -> RequestBuilder {
        self.http.get(url).basic_auth(&self.user, Some(&self.pass))
    }

    async fn send(&self, req: RequestBuilder) -> Result<reqwest::Response, String> {
        let res = req.send().await.map_err(err)?;
        match res.status() {
            s if s.is_success() => Ok(res),
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                Err("Identifiants refusés par le serveur".into())
            }
            s => Err(format!("Erreur serveur ({s}) sur {}", res.url().path())),
        }
    }

    async fn json<T: DeserializeOwned>(&self, url: Url) -> Result<T, String> {
        self.send(self.get(url)).await?.json().await.map_err(err)
    }

    fn json_request(&self, method: reqwest::Method, segments: &[&str]) -> RequestBuilder {
        self.http.request(method, self.url(segments)).basic_auth(&self.user, Some(&self.pass))
    }

    /// Registers this computer as a RomM device (once; RomM returns the
    /// existing one if asked again); its id is used for "now playing".
    pub async fn register_device(&self, name: &str, hostname: &str, os: &str) -> Result<String, String> {
        let body = serde_json::json!({
            "name": name,
            "platform": os,
            "client": "AetherPlay",
            "client_version": env!("CARGO_PKG_VERSION"),
            "hostname": hostname,
            "allow_existing": true,
        });
        let res: serde_json::Value = self
            .send(self.json_request(reqwest::Method::POST, &["api", "devices"]).json(&body))
            .await?
            .json()
            .await
            .map_err(err)?;
        res["device_id"].as_str().map(str::to_owned).ok_or_else(|| "RomM n'a pas renvoyé d'appareil".into())
    }

    /// RomM's "now playing": this device plays `rom_id` (repeated during the game).
    pub async fn heartbeat(&self, rom_id: u64, device_id: &str) -> Result<(), String> {
        let body = serde_json::json!({ "rom_id": rom_id, "device_id": device_id });
        self.send(self.json_request(reqwest::Method::POST, &["api", "activity", "heartbeat"]).json(&body))
            .await
            .map(|_| ())
    }

    /// The game is over for this device.
    pub async fn clear_activity(&self, device_id: &str) -> Result<(), String> {
        let req = self.json_request(reqwest::Method::DELETE, &["api", "activity", "heartbeat"]).query(&[("device_id", device_id)]);
        self.send(req).await.map(|_| ())
    }

    /// The logged-in user (id, ui_settings…).
    pub async fn me(&self) -> Result<serde_json::Value, String> {
        self.json(self.url(&["api", "users", "me"])).await
    }

    /// Replaces the user's ui_settings (callers merge with the current ones).
    pub async fn set_ui_settings(&self, user_id: u64, settings: &serde_json::Value) -> Result<(), String> {
        let form = [("ui_settings", settings.to_string())];
        self.send(self.json_request(reqwest::Method::PUT, &["api", "users", &user_id.to_string()]).form(&form))
            .await
            .map(|_| ())
    }

    /// A read-only access for an Android controller, and a short code the
    /// phone exchanges for it (the user's password never reaches the phone).
    pub async fn phone_pairing_code(&self, name: &str) -> Result<(String, u64), String> {
        let body = serde_json::json!({
            "name": name,
            "scopes": ["me.read", "roms.read", "platforms.read", "devices.read"],
        });
        let token: serde_json::Value = self
            .send(self.json_request(reqwest::Method::POST, &["api", "client-tokens"]).json(&body))
            .await?
            .json()
            .await
            .map_err(err)?;
        let id = token["id"].as_u64().ok_or("RomM n'a pas créé l'accès")?;
        let pair: serde_json::Value = self
            .send(self.json_request(reqwest::Method::POST, &["api", "client-tokens", &id.to_string(), "pair"]))
            .await?
            .json()
            .await
            .map_err(err)?;
        let code = pair["code"].as_str().ok_or("RomM n'a pas donné de code")?.to_string();
        Ok((code, pair["expires_in"].as_u64().unwrap_or(0)))
    }

    /// Small cover image bytes (shown on the Android controller), if RomM has one.
    pub async fn cover(&self, rom: &Rom) -> Option<Vec<u8>> {
        let path = rom.path_cover_small.as_deref().filter(|p| !p.is_empty())?;
        let url = if path.starts_with('/') {
            self.base.join(path).ok()?
        } else {
            self.base.join(&format!("assets/romm/resources/{path}")).ok()?
        };
        let res = self.send(self.get(url)).await.ok()?;
        res.bytes().await.ok().map(|b| b.to_vec()).filter(|b| b.len() < 2 << 20)
    }

    pub async fn rom(&self, id: u64) -> Result<Rom, String> {
        self.json(self.url(&["api", "roms", &id.to_string()])).await
    }

    /// Streams the ROM file to `dest`, calling `progress(done, total)` along the way.
    pub async fn download(
        &self,
        rom: &Rom,
        dest: &Path,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<(), String> {
        let url = self.url(&["api", "roms", &rom.id.to_string(), "content", &rom.fs_name]);
        let res = self.send(self.get(url)).await?;
        let total = res.content_length().unwrap_or(rom.fs_size_bytes);
        if let Some(dir) = dest.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(err)?;
        }
        let tmp = dest.with_extension("part");
        let mut file = tokio::fs::File::create(&tmp).await.map_err(err)?;
        let mut stream = res.bytes_stream();
        let mut done = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(err)?;
            file.write_all(&chunk).await.map_err(err)?;
            done += chunk.len() as u64;
            progress(done, total);
        }
        file.flush().await.map_err(err)?;
        drop(file);
        tokio::fs::rename(&tmp, dest).await.map_err(err)
    }

    pub async fn saves(&self, rom_id: u64) -> Result<Vec<Save>, String> {
        let mut url = self.url(&["api", "saves"]);
        url.query_pairs_mut().append_pair("rom_id", &rom_id.to_string());
        Ok(match self.json(url).await? {
            SaveList::Page { items } => items,
            SaveList::List(v) => v,
        })
    }

    pub async fn save_content(&self, id: u64) -> Result<Vec<u8>, String> {
        let url = self.url(&["api", "saves", &id.to_string(), "content"]);
        Ok(self.send(self.get(url)).await?.bytes().await.map_err(err)?.to_vec())
    }

    /// Uploads a save; returns the size RomM says it stored, when it says.
    pub async fn upload_save(&self, up: SaveUpload<'_>, data: Vec<u8>) -> Result<Option<u64>, String> {
        let mut url = self.url(&["api", "saves"]);
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("rom_id", &up.rom_id.to_string())
                .append_pair("emulator", up.emulator)
                .append_pair("slot", up.slot)
                .append_pair("overwrite", "true")
                .append_pair("autocleanup", "true")
                .append_pair("autocleanup_limit", "10");
            if let Some(core) = up.core {
                q.append_pair("core", core);
            }
        }
        let part = reqwest::multipart::Part::bytes(data).file_name(up.file_name.to_string());
        let form = reqwest::multipart::Form::new().part("saveFile", part);
        let req = self.http.post(url).basic_auth(&self.user, Some(&self.pass)).multipart(form);
        let res = self.send(req).await?;
        let body: serde_json::Value = res.json().await.unwrap_or_default();
        Ok(body.get("file_size_bytes").and_then(serde_json::Value::as_u64))
    }

}
