//! Une "instance" = un dossier de jeu isolé (mods/, resourcepacks/, shaderpacks/,
//! saves/, config/...) associé à une version MC + un loader. C'est le modèle central
//! de Modrinth App (par opposition aux anciens launchers "un seul .minecraft").

use crate::paths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Loader {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    pub project_id: String,
    pub version_id: String,
    pub filename: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    pub icon: Option<String>,
    pub memory_min_mb: u32,
    pub memory_max_mb: u32,
    pub java_path: Option<String>,
    pub mods: Vec<InstalledMod>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Instance {
    pub fn new(name: &str, mc_version: &str, loader: Loader, loader_version: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            mc_version: mc_version.to_string(),
            loader,
            loader_version,
            icon: None,
            memory_min_mb: 1024,
            memory_max_mb: 4096,
            java_path: None,
            mods: vec![],
            created_at: chrono::Utc::now(),
        }
    }

    pub fn dir(&self) -> std::path::PathBuf {
        paths::instance_dir(&self.id)
    }

    pub fn mods_dir(&self) -> std::path::PathBuf {
        self.dir().join("mods")
    }

    pub fn resourcepacks_dir(&self) -> std::path::PathBuf {
        self.dir().join("resourcepacks")
    }

    pub fn shaderpacks_dir(&self) -> std::path::PathBuf {
        self.dir().join("shaderpacks")
    }

    fn manifest_path(&self) -> std::path::PathBuf {
        self.dir().join("instance.json")
    }

    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(self.dir())?;
        std::fs::write(self.manifest_path(), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn load(id: &str) -> Result<Self> {
        let path = paths::instance_dir(id).join("instance.json");
        let data = std::fs::read_to_string(&path).with_context(|| format!("instance introuvable: {id}"))?;
        Ok(serde_json::from_str(&data)?)
    }

    pub fn delete(&self) -> Result<()> {
        let dir = self.dir();
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}

pub fn list_instances() -> Result<Vec<Instance>> {
    let dir = paths::instances_dir();
    let mut out = vec![];
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(id) = entry.file_name().to_str() {
                if let Ok(instance) = Instance::load(id) {
                    out.push(instance);
                }
            }
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

/// Installe un mod Modrinth dans l'instance : télécharge le fichier, l'ajoute au
/// dossier mods/ et met à jour le manifeste instance.json.
pub async fn install_mod(
    modrinth: &crate::modrinth::ModrinthClient,
    instance: &mut Instance,
    project_id: &str,
    version: &crate::modrinth::ProjectVersion,
    display_name: &str,
) -> Result<()> {
    let dest = modrinth.download_version_file(version, &instance.mods_dir()).await?;
    let filename = dest.file_name().and_then(|f| f.to_str()).unwrap_or_default().to_string();

    instance.mods.retain(|m| m.project_id != project_id); // remplace si déjà installé
    instance.mods.push(InstalledMod {
        project_id: project_id.to_string(),
        version_id: version.id.clone(),
        filename,
        name: display_name.to_string(),
    });
    instance.save()?;
    Ok(())
}

pub fn remove_mod(instance: &mut Instance, project_id: &str) -> Result<()> {
    if let Some(pos) = instance.mods.iter().position(|m| m.project_id == project_id) {
        let m = instance.mods.remove(pos);
        let path = instance.mods_dir().join(&m.filename);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        instance.save()?;
    }
    Ok(())
}
