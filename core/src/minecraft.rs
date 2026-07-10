//! Récupération du manifeste de versions Mojang, téléchargement du client/libs/assets,
//! et construction de la commande java de lancement.

use crate::auth::MinecraftAccount;
use crate::paths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

const VERSION_MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
    pub url: String,
}

pub async fn fetch_version_manifest(client: &reqwest::Client) -> Result<VersionManifest> {
    let manifest = client
        .get(VERSION_MANIFEST_URL)
        .send()
        .await?
        .error_for_status()?
        .json::<VersionManifest>()
        .await?;
    Ok(manifest)
}

/// Le JSON détaillé d'une version (libs, assetIndex, mainClass, downloads...).
/// On garde une valeur JSON brute + quelques champs extraits pour rester flexible
/// face aux évolutions du format Mojang.
#[derive(Debug, Clone)]
pub struct VersionDetails {
    pub raw: serde_json::Value,
    pub id: String,
    pub main_class: String,
    pub asset_index_url: String,
    pub asset_index_id: String,
    pub client_jar_url: String,
    pub client_jar_sha1: String,
}

pub async fn fetch_version_details(client: &reqwest::Client, version_url: &str) -> Result<VersionDetails> {
    let raw: serde_json::Value = client.get(version_url).send().await?.error_for_status()?.json().await?;

    let id = raw["id"].as_str().unwrap_or_default().to_string();
    let main_class = raw["mainClass"].as_str().unwrap_or_default().to_string();
    let asset_index_url = raw["assetIndex"]["url"].as_str().unwrap_or_default().to_string();
    let asset_index_id = raw["assetIndex"]["id"].as_str().unwrap_or_default().to_string();
    let client_jar_url = raw["downloads"]["client"]["url"].as_str().unwrap_or_default().to_string();
    let client_jar_sha1 = raw["downloads"]["client"]["sha1"].as_str().unwrap_or_default().to_string();

    Ok(VersionDetails {
        raw,
        id,
        main_class,
        asset_index_url,
        asset_index_id,
        client_jar_url,
        client_jar_sha1,
    })
}

/// Télécharge un fichier avec vérification sha1 optionnelle (si déjà présent et valide, skip).
pub async fn download_file(client: &reqwest::Client, url: &str, dest: &PathBuf, expected_sha1: Option<&str>) -> Result<()> {
    use sha1::{Digest, Sha1};

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if dest.exists() {
        if let Some(expected) = expected_sha1 {
            let bytes = std::fs::read(dest)?;
            let mut hasher = Sha1::new();
            hasher.update(&bytes);
            let actual = hex::encode(hasher.finalize());
            if actual == expected {
                return Ok(()); // déjà présent et valide
            }
        } else {
            return Ok(());
        }
    }

    let bytes = client.get(url).send().await?.error_for_status()?.bytes().await?;
    std::fs::write(dest, &bytes)?;
    Ok(())
}

// petit module hex maison pour éviter une dépendance de plus
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Télécharge le client.jar + toutes les librairies requises par la plateforme courante.
/// Note : pour rester lisible, on télécharge toutes les libs listées sans filtrage fin
/// des "rules" OS (à affiner : voir `raw["libraries"][i]["rules"]` pour un filtrage complet
/// windows/linux/osx si tu veux réduire la taille du téléchargement).
pub async fn ensure_version_installed(client: &reqwest::Client, details: &VersionDetails) -> Result<PathBuf> {
    let version_dir = paths::versions_dir().join(&details.id);
    std::fs::create_dir_all(&version_dir)?;

    let client_jar = version_dir.join(format!("{}.jar", details.id));
    download_file(client, &details.client_jar_url, &client_jar, Some(&details.client_jar_sha1)).await?;

    if let Some(libs) = details.raw["libraries"].as_array() {
        for lib in libs {
            if let Some(artifact) = lib["downloads"]["artifact"].as_object() {
                let url = artifact.get("url").and_then(|v| v.as_str()).unwrap_or_default();
                let path = artifact.get("path").and_then(|v| v.as_str()).unwrap_or_default();
                let sha1 = artifact.get("sha1").and_then(|v| v.as_str());
                if !url.is_empty() && !path.is_empty() {
                    let dest = paths::libraries_dir().join(path);
                    download_file(client, url, &dest, sha1).await?;
                }
            }
        }
    }

    // Assets : on télécharge l'index puis les objets (limité — pour un vrai launcher,
    // paralléliser avec un pool de tâches tokio).
    if !details.asset_index_url.is_empty() {
        let index_dest = paths::assets_dir().join("indexes").join(format!("{}.json", details.asset_index_id));
        download_file(client, &details.asset_index_url, &index_dest, None).await?;
    }

    Ok(client_jar)
}

pub struct LaunchOptions {
    pub java_path: String,
    pub memory_min_mb: u32,
    pub memory_max_mb: u32,
    pub game_dir: PathBuf,
    pub width: u32,
    pub height: u32,
    /// Classpath additionnel (ex: jars du mod loader) préfixé avant les libs vanilla.
    pub extra_classpath: Vec<PathBuf>,
    /// Si Some, remplace le mainClass vanilla (ex: KnotClient pour Fabric).
    pub override_main_class: Option<String>,
}

/// Construit et lance le processus `java` pour démarrer Minecraft.
/// Retourne le `Child` process pour permettre à l'UI de suivre les logs stdout/stderr.
pub fn launch(
    details: &VersionDetails,
    account: &MinecraftAccount,
    opts: &LaunchOptions,
) -> Result<std::process::Child> {
    let version_dir = paths::versions_dir().join(&details.id);
    let client_jar = version_dir.join(format!("{}.jar", details.id));

    let mut classpath: Vec<String> = vec![];
    for extra in &opts.extra_classpath {
        classpath.push(extra.display().to_string());
    }
    if let Some(libs) = details.raw["libraries"].as_array() {
        for lib in libs {
            if let Some(path) = lib["downloads"]["artifact"]["path"].as_str() {
                classpath.push(paths::libraries_dir().join(path).display().to_string());
            }
        }
    }
    classpath.push(client_jar.display().to_string());

    let cp_sep = if cfg!(windows) { ";" } else { ":" };
    let classpath_str = classpath.join(cp_sep);

    let main_class = opts
        .override_main_class
        .clone()
        .unwrap_or_else(|| details.main_class.clone());

    std::fs::create_dir_all(&opts.game_dir)?;

    let child = Command::new(&opts.java_path)
        .current_dir(&opts.game_dir)
        .arg(format!("-Xms{}M", opts.memory_min_mb))
        .arg(format!("-Xmx{}M", opts.memory_max_mb))
        .arg("-Djava.library.path=".to_string() + &version_dir.join("natives").display().to_string())
        .arg("-cp")
        .arg(&classpath_str)
        .arg(&main_class)
        .arg("--username").arg(&account.mc_username)
        .arg("--uuid").arg(&account.mc_uuid)
        .arg("--accessToken").arg(&account.mc_access_token)
        .arg("--version").arg(&details.id)
        .arg("--gameDir").arg(opts.game_dir.display().to_string())
        .arg("--assetsDir").arg(paths::assets_dir().display().to_string())
        .arg("--assetIndex").arg(&details.asset_index_id)
        .arg("--userType").arg("msa")
        .arg("--width").arg(opts.width.to_string())
        .arg("--height").arg(opts.height.to_string())
        .spawn()
        .context("échec du lancement du processus java (vérifie java_path)")?;

    Ok(child)
}
