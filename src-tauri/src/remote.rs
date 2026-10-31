use crate::paths;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

// ---------------------------------------------------------------------------
// Fichier distant : version du modpack, actualités, World Boss
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModpackInfo {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub changelog: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerInfo {
    pub name: String,
    pub address: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NewsItem {
    pub id: String,
    pub title: String,
    pub date: String,
    pub tag: String,
    pub summary: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Boss {
    pub id: String,
    pub name: String,
    pub form: String,
    pub level: u32,
    /// "active" | "upcoming" | "defeated"
    pub status: String,
    /// Vie restante en pourcentage (0 à 100) pour un boss actif.
    pub hp_percent: f32,
    pub players: u32,
    /// Date ISO 8601 de fin (actif) ou de début (à venir). Vide si inconnue.
    pub ends_at: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteInfo {
    pub minecraft: String,
    pub neoforge: String,
    pub modpack: ModpackInfo,
    pub server: ServerInfo,
    pub news: Vec<NewsItem>,
    pub bosses: Vec<Boss>,
    /// Message de maintenance : s'il n'est pas vide, le bouton Jouer est bloqué.
    pub maintenance: String,
}

pub async fn fetch_remote(client: &reqwest::Client, url: &str) -> Result<RemoteInfo> {
    let info: RemoteInfo = client
        .get(url)
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .context("connexion au serveur d'infos impossible")?
        .error_for_status()?
        .json()
        .await
        .context("fichier d'infos invalide")?;
    let _ = std::fs::create_dir_all(paths::cache_dir());
    let _ = std::fs::write(
        paths::remote_cache_file(),
        serde_json::to_string(&info).unwrap_or_default(),
    );
    Ok(info)
}

pub fn load_cached_remote() -> Option<RemoteInfo> {
    std::fs::read_to_string(paths::remote_cache_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

// ---------------------------------------------------------------------------
// Statut du serveur Minecraft (Server List Ping)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize)]
pub struct ServerStatus {
    pub online: bool,
    pub players: u32,
    pub max: u32,
    pub ping_ms: u64,
    pub version: String,
    pub motd: String,
}

fn split_host_port(addr: &str) -> (String, u16) {
    match addr.rsplit_once(':') {
        Some((h, p)) => match p.parse::<u16>() {
            Ok(port) => (h.to_string(), port),
            Err(_) => (addr.to_string(), 25565),
        },
        None => (addr.to_string(), 25565),
    }
}

fn write_varint(buf: &mut Vec<u8>, mut v: u32) {
    loop {
        if v & !0x7F == 0 {
            buf.push(v as u8);
            return;
        }
        buf.push(((v & 0x7F) | 0x80) as u8);
        v >>= 7;
    }
}

async fn read_varint(s: &mut TcpStream) -> Result<u32> {
    let mut result = 0u32;
    for i in 0..5 {
        let b = s.read_u8().await?;
        result |= ((b & 0x7F) as u32) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(result);
        }
    }
    bail!("varint trop long")
}

async fn send_packet(s: &mut TcpStream, payload: &[u8]) -> Result<()> {
    let mut out = Vec::with_capacity(payload.len() + 5);
    write_varint(&mut out, payload.len() as u32);
    out.extend_from_slice(payload);
    s.write_all(&out).await?;
    Ok(())
}

pub async fn ping_server(addr: &str) -> Result<ServerStatus> {
    if addr.trim().is_empty() {
        bail!("aucune adresse de serveur");
    }
    let (host, port) = split_host_port(addr.trim());
    let started = Instant::now();
    let mut s = timeout(
        Duration::from_secs(4),
        TcpStream::connect((host.as_str(), port)),
    )
    .await??;

    // Handshake (protocole 767 = 1.21.1) puis requête de statut.
    let mut hs = Vec::new();
    write_varint(&mut hs, 0);
    write_varint(&mut hs, 767);
    write_varint(&mut hs, host.len() as u32);
    hs.extend_from_slice(host.as_bytes());
    hs.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut hs, 1);
    send_packet(&mut s, &hs).await?;
    send_packet(&mut s, &[0x00]).await?;

    let _len = timeout(Duration::from_secs(4), read_varint(&mut s)).await??;
    let _id = read_varint(&mut s).await?;
    let slen = read_varint(&mut s).await? as usize;
    if slen == 0 || slen > 1_000_000 {
        bail!("réponse de statut invalide");
    }
    let mut body = vec![0u8; slen];
    timeout(Duration::from_secs(4), s.read_exact(&mut body)).await??;
    let ping_ms = started.elapsed().as_millis() as u64;

    let v: serde_json::Value = serde_json::from_slice(&body)?;
    let motd = match &v["description"] {
        serde_json::Value::String(t) => t.clone(),
        d => d["text"].as_str().unwrap_or("").to_string(),
    };
    Ok(ServerStatus {
        online: true,
        players: v["players"]["online"].as_u64().unwrap_or(0) as u32,
        max: v["players"]["max"].as_u64().unwrap_or(0) as u32,
        ping_ms,
        version: v["version"]["name"].as_str().unwrap_or("").to_string(),
        motd,
    })
}
