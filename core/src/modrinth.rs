//! Client pour l'API Modrinth v2 (https://docs.modrinth.com/api/).
//! Couvre mods, modpacks, resource packs et shaders : ce sont tous des
//! "projects" côté API, filtrés par `project_type` / `categories`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const API_BASE: &str = "https://api.modrinth.com/v2";
// Header requis par les CG de Modrinth : identifie ton launcher auprès de leur API.
const USER_AGENT: &str = "example-dev/duckyluncher/0.1.0 (contact@example.com)";

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    pub total_hits: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub categories: Vec<String>,
    pub project_type: String, // "mod" | "modpack" | "resourcepack" | "shader"
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProjectVersion {
    pub id: String,
    pub project_id: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<VersionFile>,
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub hashes: FileHashes,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FileHashes {
    pub sha1: String,
    pub sha512: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Dependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub dependency_type: String, // "required" | "optional" | "incompatible" | "embedded"
}

pub struct ModrinthClient {
    http: reqwest::Client,
}

/// `project_type` filtré : "mod", "modpack", "resourcepack", "shader"
pub struct SearchParams<'a> {
    pub query: &'a str,
    pub project_type: &'a str,
    pub game_version: Option<&'a str>,
    pub loader: Option<&'a str>,
    pub limit: u32,
    pub offset: u32,
}

impl ModrinthClient {
    pub fn new() -> Result<Self> {
        let http = reqwest::Client::builder().user_agent(USER_AGENT).build()?;
        Ok(Self { http })
    }

    pub async fn search(&self, params: SearchParams<'_>) -> Result<SearchResult> {
        let mut facets: Vec<Vec<String>> = vec![vec![format!("project_type:{}", params.project_type)]];
        if let Some(gv) = params.game_version {
            facets.push(vec![format!("versions:{gv}")]);
        }
        if let Some(loader) = params.loader {
            facets.push(vec![format!("categories:{loader}")]);
        }
        let facets_json = serde_json::to_string(&facets)?;

        let resp = self
            .http
            .get(format!("{API_BASE}/search"))
            .query(&[
                ("query", params.query),
                ("facets", &facets_json),
                ("limit", &params.limit.to_string()),
                ("offset", &params.offset.to_string()),
            ])
            .send()
            .await?
            .error_for_status()
            .context("échec de la recherche Modrinth")?
            .json::<SearchResult>()
            .await?;

        Ok(resp)
    }

    pub async fn project_versions(
        &self,
        project_id_or_slug: &str,
        game_version: Option<&str>,
        loader: Option<&str>,
    ) -> Result<Vec<ProjectVersion>> {
        let mut query: Vec<(&str, String)> = vec![];
        if let Some(gv) = game_version {
            query.push(("game_versions", serde_json::to_string(&[gv])?));
        }
        if let Some(l) = loader {
            query.push(("loaders", serde_json::to_string(&[l])?));
        }

        let versions = self
            .http
            .get(format!("{API_BASE}/project/{project_id_or_slug}/version"))
            .query(&query)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<ProjectVersion>>()
            .await?;

        Ok(versions)
    }

    pub async fn get_version(&self, version_id: &str) -> Result<ProjectVersion> {
        let v = self
            .http
            .get(format!("{API_BASE}/version/{version_id}"))
            .send()
            .await?
            .error_for_status()?
            .json::<ProjectVersion>()
            .await?;
        Ok(v)
    }

    /// Télécharge le fichier primaire d'une version dans `dest_dir` (ex: dossier mods/
    /// de l'instance) et retourne le chemin final.
    pub async fn download_version_file(&self, version: &ProjectVersion, dest_dir: &std::path::Path) -> Result<std::path::PathBuf> {
        let file = version
            .files
            .iter()
            .find(|f| f.primary)
            .or_else(|| version.files.first())
            .context("aucun fichier disponible pour cette version")?;

        std::fs::create_dir_all(dest_dir)?;
        let dest = dest_dir.join(&file.filename);

        let bytes = self.http.get(&file.url).send().await?.error_for_status()?.bytes().await?;
        std::fs::write(&dest, &bytes)?;

        Ok(dest)
    }
}
