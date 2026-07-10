//! Installation du mod loader Fabric via l'API meta.fabricmc.net.
//! Fabric ne modifie pas le jar vanilla : il fournit un profil JSON qui référence
//! des libs supplémentaires + une mainClass (KnotClient) qui charge le jeu par-dessus.

use crate::paths;
use anyhow::Result;
use serde::Deserialize;

const META_BASE: &str = "https://meta.fabricmc.net/v2";

#[derive(Debug, Deserialize)]
pub struct LoaderVersion {
    pub loader: LoaderInfo,
}

#[derive(Debug, Deserialize)]
pub struct LoaderInfo {
    pub version: String,
    pub stable: bool,
}

pub async fn list_loader_versions(client: &reqwest::Client, mc_version: &str) -> Result<Vec<LoaderVersion>> {
    let url = format!("{META_BASE}/versions/loader/{mc_version}");
    let versions = client.get(url).send().await?.error_for_status()?.json().await?;
    Ok(versions)
}

/// Récupère le profil de lancement Fabric (JSON "launcher meta") pour une combinaison
/// mc_version + loader_version, et télécharge les librairies qu'il référence.
/// Retourne (main_class, chemins des jars fabric à ajouter au classpath).
pub async fn install(
    client: &reqwest::Client,
    mc_version: &str,
    loader_version: &str,
) -> Result<(String, Vec<std::path::PathBuf>)> {
    let url = format!("{META_BASE}/versions/loader/{mc_version}/{loader_version}/profile/json");
    let profile: serde_json::Value = client.get(url).send().await?.error_for_status()?.json().await?;

    let main_class = profile["mainClass"].as_str().unwrap_or("net.fabricmc.loader.impl.launch.knot.KnotClient").to_string();

    let mut jar_paths = vec![];
    if let Some(libs) = profile["libraries"].as_array() {
        for lib in libs {
            if let Some(name) = lib["name"].as_str() {
                // name au format "group:artifact:version" -> chemin maven standard
                let parts: Vec<&str> = name.split(':').collect();
                if parts.len() == 3 {
                    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
                    let group_path = group.replace('.', "/");
                    let rel = format!("{group_path}/{artifact}/{version}/{artifact}-{version}.jar");
                    let base_url = lib["url"].as_str().unwrap_or("https://maven.fabricmc.net/");
                    let full_url = format!("{}{}", base_url.trim_end_matches('/'), format!("/{rel}"));
                    let dest = paths::libraries_dir().join(&rel);
                    crate::minecraft::download_file(client, &full_url, &dest, None).await.ok();
                    jar_paths.push(dest);
                }
            }
        }
    }

    Ok((main_class, jar_paths))
}
