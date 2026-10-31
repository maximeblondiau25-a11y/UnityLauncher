//! Connexion Microsoft -> Xbox Live -> XSTS -> Minecraft (flux "device code").
use crate::paths;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const KEYRING_SERVICE: &str = "PokemonUnityLauncher";
const KEYRING_USER: &str = "ms_refresh_token";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Compte affiché dans le launcher (aucun secret ici).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub uuid: String,
}

/// Session de jeu : contient le jeton Minecraft, jamais écrit sur disque.
#[derive(Clone, Debug)]
pub struct Session {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
}

pub struct MsTokens {
    pub access: String,
    pub refresh: String,
}

fn check_client_id(client_id: &str) -> Result<()> {
    if client_id.trim().is_empty() || client_id.starts_with("REMPLACE") {
        bail!("Identifiant Azure manquant : renseigne ton client id dans Paramètres > Avancé (voir le README).");
    }
    Ok(())
}

pub async fn request_device_code(client: &reqwest::Client, client_id: &str) -> Result<DeviceCode> {
    check_client_id(client_id)?;
    let v: Value = client
        .post(DEVICE_URL)
        .form(&[("client_id", client_id), ("scope", SCOPE)])
        .send()
        .await?
        .json()
        .await?;
    if let Some(err) = v["error_description"].as_str() {
        bail!("Microsoft : {err}");
    }
    Ok(DeviceCode {
        user_code: v["user_code"].as_str().unwrap_or_default().to_string(),
        device_code: v["device_code"].as_str().unwrap_or_default().to_string(),
        verification_uri: v["verification_uri"]
            .as_str()
            .unwrap_or("https://microsoft.com/link")
            .to_string(),
        expires_in: v["expires_in"].as_u64().unwrap_or(900),
        interval: v["interval"].as_u64().unwrap_or(5).max(1),
    })
}

/// Attend que l'utilisateur valide le code dans son navigateur.
pub async fn poll_device_code(
    client: &reqwest::Client,
    client_id: &str,
    code: &DeviceCode,
) -> Result<MsTokens> {
    let deadline = SystemTime::now() + Duration::from_secs(code.expires_in);
    let mut interval = code.interval;
    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        if SystemTime::now() > deadline {
            bail!("Le code a expiré, relance la connexion.");
        }
        let v: Value = client
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id),
                ("device_code", code.device_code.as_str()),
            ])
            .send()
            .await?
            .json()
            .await?;
        if let Some(access) = v["access_token"].as_str() {
            return Ok(MsTokens {
                access: access.to_string(),
                refresh: v["refresh_token"].as_str().unwrap_or_default().to_string(),
            });
        }
        match v["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            Some("authorization_declined") => bail!("Connexion refusée."),
            Some("expired_token") => bail!("Le code a expiré, relance la connexion."),
            _ => bail!(
                "Microsoft : {}",
                v["error_description"].as_str().unwrap_or("erreur inconnue")
            ),
        }
    }
}

pub async fn refresh_tokens(
    client: &reqwest::Client,
    client_id: &str,
    refresh: &str,
) -> Result<MsTokens> {
    let v: Value = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh),
            ("scope", SCOPE),
        ])
        .send()
        .await?
        .json()
        .await?;
    match v["access_token"].as_str() {
        Some(access) => Ok(MsTokens {
            access: access.to_string(),
            refresh: v["refresh_token"].as_str().unwrap_or(refresh).to_string(),
        }),
        None => bail!("Session expirée, reconnecte-toi."),
    }
}

/// Microsoft -> Xbox Live -> XSTS -> Minecraft -> profil.
pub async fn minecraft_session(client: &reqwest::Client, ms_access: &str) -> Result<Session> {
    // 1. Xbox Live
    let xbl: Value = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .header("Accept", "application/json")
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await?
        .error_for_status()
        .context("authentification Xbox Live refusée")?
        .json()
        .await?;
    let xbl_token = xbl["Token"].as_str().ok_or_else(|| anyhow!("jeton Xbox manquant"))?;
    let uhs = xbl["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .ok_or_else(|| anyhow!("hash utilisateur Xbox manquant"))?
        .to_string();

    // 2. XSTS
    let resp = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("Accept", "application/json")
        .json(&json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await?;
    let status = resp.status();
    let xsts: Value = resp.json().await?;
    if !status.is_success() {
        let msg = match xsts["XErr"].as_u64() {
            Some(2148916233) => "Ce compte Microsoft n'a pas de profil Xbox. Crée-en un sur xbox.com.",
            Some(2148916235) => "Xbox Live n'est pas disponible dans ton pays.",
            Some(2148916236) | Some(2148916237) => "Vérification d'âge requise sur ton compte Xbox.",
            Some(2148916238) => "Compte enfant : un adulte doit l'ajouter à une famille Microsoft.",
            _ => "Autorisation XSTS refusée.",
        };
        bail!("{msg}");
    }
    let xsts_token = xsts["Token"].as_str().ok_or_else(|| anyhow!("jeton XSTS manquant"))?;

    // 3. Minecraft
    let mc: Value = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }))
        .send()
        .await?
        .error_for_status()
        .context("connexion Minecraft refusée (l'application Azure est-elle approuvée par Mojang ?)")?
        .json()
        .await?;
    let access_token = mc["access_token"]
        .as_str()
        .ok_or_else(|| anyhow!("jeton Minecraft manquant"))?
        .to_string();

    // 4. Profil (404 = le compte ne possède pas Minecraft Java)
    let resp = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(&access_token)
        .send()
        .await?;
    if resp.status().as_u16() == 404 {
        bail!("Ce compte ne possède pas Minecraft Java Edition.");
    }
    let profile: Value = resp.error_for_status()?.json().await?;
    Ok(Session {
        name: profile["name"].as_str().unwrap_or("Joueur").to_string(),
        uuid: profile["id"].as_str().unwrap_or_default().to_string(),
        access_token,
    })
}

// ---------------------------------------------------------------------------
// Stockage : le refresh token va dans le coffre du système (Windows Credential
// Manager / Keychain / Secret Service), le nom et l'UUID dans un petit JSON.
// ---------------------------------------------------------------------------

fn entry() -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?)
}

pub fn save_account(account: &Account, refresh: &str) -> Result<()> {
    std::fs::create_dir_all(paths::base_dir())?;
    std::fs::write(paths::account_file(), serde_json::to_string(account)?)?;
    entry()?.set_password(refresh)?;
    Ok(())
}

pub fn load_account() -> Option<Account> {
    std::fs::read_to_string(paths::account_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub fn load_refresh_token() -> Option<String> {
    entry().ok()?.get_password().ok()
}

pub fn store_refresh_token(refresh: &str) {
    if let Ok(e) = entry() {
        let _ = e.set_password(refresh);
    }
}

pub fn logout() {
    let _ = std::fs::remove_file(paths::account_file());
    if let Ok(e) = entry() {
        let _ = e.delete_credential();
    }
}

/// Renouvelle toute la chaîne d'authentification juste avant le lancement.
pub async fn fresh_session(client: &reqwest::Client, client_id: &str) -> Result<Session> {
    check_client_id(client_id)?;
    let refresh = load_refresh_token().ok_or_else(|| anyhow!("Tu n'es pas connecté."))?;
    let tokens = refresh_tokens(client, client_id, &refresh).await?;
    if !tokens.refresh.is_empty() {
        store_refresh_token(&tokens.refresh);
    }
    minecraft_session(client, &tokens.access).await
}

#[allow(dead_code)]
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
