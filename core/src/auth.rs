//! Flux d'authentification Microsoft -> Xbox Live -> XSTS -> Minecraft Services.
//! Basé sur le "device authorization grant" (OAuth 2.0), adapté au flux public
//! documenté par Microsoft pour les launchers tiers.
//!
//! Étapes :
//! 1. Demander un device code (l'utilisateur va sur microsoft.com/link et saisit un code)
//! 2. Poller le token endpoint jusqu'à ce que l'utilisateur ait validé
//! 3. Échanger le token MS contre un token Xbox Live
//! 4. Échanger le token Xbox Live contre un token XSTS
//! 5. Échanger le token XSTS contre un token Minecraft (Bearer)
//! 6. Récupérer le profil Minecraft (uuid, pseudo, skin)

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Remplace par ton propre client_id Azure AD (App registration, type "Public client/native").
/// Voir https://learn.microsoft.com/fr-fr/entra/identity-platform/quickstart-register-app
pub const CLIENT_ID: &str = "REMPLACE_MOI_PAR_TON_CLIENT_ID_AZURE";
const SCOPE: &str = "XboxLive.signin offline_access";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftAccount {
    pub mc_uuid: String,
    pub mc_username: String,
    pub mc_access_token: String,
    pub ms_refresh_token: String,
    pub skin_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Étape 1 : demande un code à afficher/faire saisir à l'utilisateur.
pub async fn request_device_code(client: &reqwest::Client) -> Result<DeviceCodeResponse> {
    let resp = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")
        .form(&[("client_id", CLIENT_ID), ("scope", SCOPE)])
        .send()
        .await
        .context("échec de la requête devicecode")?
        .error_for_status()?
        .json::<DeviceCodeResponse>()
        .await?;
    Ok(resp)
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    #[allow(dead_code)]
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    error: String,
}

/// Étape 2 : poll jusqu'à validation par l'utilisateur (ou expiration).
/// À appeler en boucle depuis l'UI (ex: toutes les `interval` secondes).
pub async fn poll_device_token(
    client: &reqwest::Client,
    device_code: &str,
) -> Result<Option<(String, String)>> {
    // Retourne None si "authorization_pending" (continuer à poller),
    // Some((access_token, refresh_token)) si succès, Err si erreur définitive.
    let resp = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", CLIENT_ID),
            ("device_code", device_code),
        ])
        .send()
        .await?;

    if resp.status().is_success() {
        let tok = resp.json::<TokenResponse>().await?;
        Ok(Some((tok.access_token, tok.refresh_token)))
    } else {
        let err = resp.json::<TokenErrorResponse>().await?;
        match err.error.as_str() {
            "authorization_pending" | "slow_down" => Ok(None),
            other => Err(anyhow!("échec auth Microsoft: {other}")),
        }
    }
}

#[derive(Debug, Deserialize)]
struct XblAuthResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XblDisplayClaims,
}

#[derive(Debug, Deserialize)]
struct XblDisplayClaims {
    xui: Vec<std::collections::HashMap<String, String>>,
}

/// Étape 3 : token Xbox Live
async fn auth_xbox_live(client: &reqwest::Client, ms_access_token: &str) -> Result<(String, String)> {
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={ms_access_token}")
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });
    let resp: XblAuthResponse = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let user_hash = resp
        .display_claims
        .xui
        .first()
        .and_then(|m| m.get("uhs"))
        .cloned()
        .ok_or_else(|| anyhow!("uhs manquant dans la réponse Xbox Live"))?;
    Ok((resp.token, user_hash))
}

/// Étape 4 : token XSTS (nécessaire pour se connecter aux services Minecraft)
async fn auth_xsts(client: &reqwest::Client, xbl_token: &str) -> Result<String> {
    let body = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });
    let resp = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(&body)
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        // XErr connu : 2148916233 = pas de compte Xbox, 2148916238 = compte enfant, etc.
        return Err(anyhow!(
            "compte Xbox invalide ou restreint (vérifie qu'un profil Xbox existe pour ce compte)"
        ));
    }
    let resp: XblAuthResponse = resp.error_for_status()?.json().await?;
    Ok(resp.token)
}

#[derive(Debug, Deserialize)]
struct MinecraftAuthResponse {
    access_token: String,
}

/// Étape 5 : échange XSTS -> token Minecraft
async fn auth_minecraft(client: &reqwest::Client, xsts_token: &str, user_hash: &str) -> Result<String> {
    let body = serde_json::json!({
        "identityToken": format!("XBL3.0 x={user_hash};{xsts_token}")
    });
    let resp: MinecraftAuthResponse = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(resp.access_token)
}

#[derive(Debug, Deserialize)]
struct MinecraftProfile {
    id: String,
    name: String,
    #[serde(default)]
    skins: Vec<MinecraftSkin>,
}

#[derive(Debug, Deserialize)]
struct MinecraftSkin {
    url: String,
    state: String,
}

/// Étape 6 : profil Minecraft (pseudo, uuid, skin actif)
async fn fetch_minecraft_profile(client: &reqwest::Client, mc_token: &str) -> Result<MinecraftProfile> {
    let resp = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(mc_token)
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(anyhow!("ce compte Microsoft ne possède pas Minecraft"));
    }
    Ok(resp.error_for_status()?.json().await?)
}

/// Orchestration complète : à appeler une fois qu'on a (ms_access_token, ms_refresh_token)
/// obtenus via `poll_device_token`.
pub async fn complete_login(
    client: &reqwest::Client,
    ms_access_token: &str,
    ms_refresh_token: &str,
) -> Result<MinecraftAccount> {
    let (xbl_token, user_hash) = auth_xbox_live(client, ms_access_token).await?;
    let xsts_token = auth_xsts(client, &xbl_token).await?;
    let mc_token = auth_minecraft(client, &xsts_token, &user_hash).await?;
    let profile = fetch_minecraft_profile(client, &mc_token).await?;

    let skin_url = profile
        .skins
        .into_iter()
        .find(|s| s.state == "ACTIVE")
        .map(|s| s.url);

    Ok(MinecraftAccount {
        mc_uuid: profile.id,
        mc_username: profile.name,
        mc_access_token: mc_token,
        ms_refresh_token: ms_refresh_token.to_string(),
        skin_url,
    })
}

/// Rafraîchit un token Microsoft expiré via le refresh_token stocké, puis relance
/// tout le flux Xbox/XSTS/Minecraft pour obtenir un nouveau token Minecraft.
pub async fn refresh_account(client: &reqwest::Client, account: &MinecraftAccount) -> Result<MinecraftAccount> {
    let resp = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", CLIENT_ID),
            ("refresh_token", &account.ms_refresh_token),
            ("scope", SCOPE),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<TokenResponse>()
        .await?;

    complete_login(client, &resp.access_token, &resp.refresh_token).await
}

pub fn load_accounts() -> Result<Vec<MinecraftAccount>> {
    let path = crate::paths::accounts_file();
    if !path.exists() {
        return Ok(vec![]);
    }
    let data = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&data)?)
}

pub fn save_account(account: &MinecraftAccount) -> Result<()> {
    let mut accounts = load_accounts()?;
    accounts.retain(|a| a.mc_uuid != account.mc_uuid);
    accounts.push(account.clone());
    let path = crate::paths::accounts_file();
    std::fs::write(path, serde_json::to_string_pretty(&accounts)?)?;
    Ok(())
}

pub fn poll_interval(resp: &DeviceCodeResponse) -> Duration {
    Duration::from_secs(resp.interval.max(2))
}
