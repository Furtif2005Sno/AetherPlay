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
pub struct Platform {
    pub id: u64,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub rom_count: u64,
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
    #[serde(default)]
    pub multi: bool,
}

/// `/api/roms` is paginated on recent RomM versions, a plain list on older ones.
#[derive(Deserialize)]
#[serde(untagged)]
enum RomList {
    Page { items: Vec<Rom> },
    List(Vec<Rom>),
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

    pub async fn check_auth(&self) -> Result<(), String> {
        self.send(self.get(self.url(&["api", "users", "me"]))).await.map(|_| ())
    }

    pub async fn platforms(&self) -> Result<Vec<Platform>, String> {
        self.json(self.url(&["api", "platforms"])).await
    }

    pub async fn roms(&self, platform_id: u64) -> Result<Vec<Rom>, String> {
        let mut url = self.url(&["api", "roms"]);
        let id = platform_id.to_string();
        url.query_pairs_mut()
            .append_pair("platform_ids", &id)
            .append_pair("platform_id", &id)
            .append_pair("order_by", "name")
            .append_pair("limit", "10000");
        Ok(match self.json(url).await? {
            RomList::Page { items } => items,
            RomList::List(v) => v,
        })
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

    pub async fn upload_save(&self, up: SaveUpload<'_>, data: Vec<u8>) -> Result<(), String> {
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
        self.send(req).await.map(|_| ())
    }

    /// Fetches a RomM resource (cover art…) as (content-type, bytes).
    pub async fn resource(&self, path_and_query: &str) -> Result<(String, Vec<u8>), String> {
        // RomM 5 sends full paths (`/assets/romm/resources/...`); older
        // versions sent them relative to the resources folder.
        let rel = path_and_query.trim_start_matches('/');
        let rel =
            if rel.starts_with("assets/") { rel.to_string() } else { format!("assets/romm/resources/{rel}") };
        let url = self.base.join(&rel).map_err(err)?;
        let res = self.send(self.get(url)).await?;
        let ct = res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        Ok((ct, res.bytes().await.map_err(err)?.to_vec()))
    }
}
