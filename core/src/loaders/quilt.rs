//! Quilt est un fork de Fabric, l'API meta.quiltmc.org suit exactement le même
//! format que meta.fabricmc.net. On réutilise la même logique d'installation.

use crate::paths;
use anyhow::Result;
use serde::Deserialize;

const META_BASE: &str = "https://meta.quiltmc.org/v3";

#[derive(Debug, Deserialize)]
pub struct LoaderVersion {
    pub loader: LoaderInfo,
}

#[derive(Debug, Deserialize)]
pub struct LoaderInfo {
    pub version: String,
}

pub async fn list_loader_versions(client: &reqwest::Client, mc_version: &str) -> Result<Vec<LoaderVersion>> {
    let url = format!("{META_BASE}/versions/loader/{mc_version}");
    let versions = client.get(url).send().await?.error_for_status()?.json().await?;
    Ok(versions)
}

pub async fn install(
    client: &reqwest::Client,
    mc_version: &str,
    loader_version: &str,
) -> Result<(String, Vec<std::path::PathBuf>)> {
    let url = format!("{META_BASE}/versions/loader/{mc_version}/{loader_version}/profile/json");
    let profile: serde_json::Value = client.get(url).send().await?.error_for_status()?.json().await?;

    let main_class = profile["mainClass"].as_str().unwrap_or("org.quiltmc.loader.impl.launch.knot.KnotClient").to_string();

    let mut jar_paths = vec![];
    if let Some(libs) = profile["libraries"].as_array() {
        for lib in libs {
            if let Some(name) = lib["name"].as_str() {
                let parts: Vec<&str> = name.split(':').collect();
                if parts.len() == 3 {
                    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
                    let group_path = group.replace('.', "/");
                    let rel = format!("{group_path}/{artifact}/{version}/{artifact}-{version}.jar");
                    let base_url = lib["url"].as_str().unwrap_or("https://maven.quiltmc.org/repository/release/");
                    let full_url = format!("{}/{}", base_url.trim_end_matches('/'), rel);
                    let dest = paths::libraries_dir().join(&rel);
                    crate::minecraft::download_file(client, &full_url, &dest, None).await.ok();
                    jar_paths.push(dest);
                }
            }
        }
    }

    Ok((main_class, jar_paths))
}
