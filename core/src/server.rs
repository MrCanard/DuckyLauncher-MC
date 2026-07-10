//! Hébergement d'un serveur Minecraft directement sur la machine de l'utilisateur
//! (comme dans Modrinth App : pas de service payant, juste `java -jar server.jar`
//! dans un dossier dédié + ouverture d'un port). L'utilisateur doit lui-même
//! rediriger son port sur sa box/routeur (port forwarding) s'il veut que des amis
//! rejoignent depuis l'extérieur — le launcher ne fait pas de NAT traversal.

use crate::minecraft::download_file;
use crate::paths;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServerSoftware {
    Vanilla,
    Paper,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalServer {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub software: ServerSoftware,
    pub port: u16,
    pub memory_min_mb: u32,
    pub memory_max_mb: u32,
    pub java_path: String,
    pub eula_accepted: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl LocalServer {
    pub fn new(name: &str, mc_version: &str, software: ServerSoftware) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            mc_version: mc_version.to_string(),
            software,
            port: 25565,
            memory_min_mb: 1024,
            memory_max_mb: 4096,
            java_path: "java".to_string(),
            eula_accepted: false,
            created_at: chrono::Utc::now(),
        }
    }

    pub fn dir(&self) -> PathBuf {
        paths::server_dir(&self.id)
    }

    pub fn jar_path(&self) -> PathBuf {
        self.dir().join("server.jar")
    }

    fn manifest_path(&self) -> PathBuf {
        self.dir().join("server.json")
    }

    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(self.dir())?;
        std::fs::write(self.manifest_path(), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn load(id: &str) -> Result<Self> {
        let path = paths::server_dir(id).join("server.json");
        let data = std::fs::read_to_string(&path).with_context(|| format!("serveur introuvable: {id}"))?;
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

pub fn list_servers() -> Result<Vec<LocalServer>> {
    let dir = paths::servers_dir();
    let mut out = vec![];
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(id) = entry.file_name().to_str() {
                if let Ok(server) = LocalServer::load(id) {
                    out.push(server);
                }
            }
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

/// Télécharge le jar vanilla officiel pour `mc_version` (depuis le manifeste Mojang).
async fn download_vanilla_jar(client: &reqwest::Client, mc_version: &str, dest: &PathBuf) -> Result<()> {
    let manifest = crate::minecraft::fetch_version_manifest(client).await?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == mc_version)
        .context("version Minecraft introuvable")?;
    let details = crate::minecraft::fetch_version_details(client, &entry.url).await?;
    let server_url = details.raw["downloads"]["server"]["url"]
        .as_str()
        .context("pas de jar serveur officiel pour cette version")?;
    let sha1 = details.raw["downloads"]["server"]["sha1"].as_str();
    download_file(client, server_url, dest, sha1).await
}

#[derive(Debug, Deserialize)]
struct PaperVersionBuilds {
    builds: Vec<u32>,
}

#[derive(Debug, Deserialize)]
struct PaperBuildInfo {
    downloads: PaperBuildDownloads,
}

#[derive(Debug, Deserialize)]
struct PaperBuildDownloads {
    application: PaperDownloadEntry,
}

#[derive(Debug, Deserialize)]
struct PaperDownloadEntry {
    name: String,
}

/// Télécharge le dernier build stable de Paper (https://api.papermc.io/v2) pour la version donnée.
/// Paper = fork de Spigot très populaire pour l'optimisation et les plugins (Bukkit API).
async fn download_paper_jar(client: &reqwest::Client, mc_version: &str, dest: &PathBuf) -> Result<()> {
    let builds_url = format!("https://api.papermc.io/v2/projects/paper/versions/{mc_version}/builds");
    let builds: PaperVersionBuilds = client.get(&builds_url).send().await?.error_for_status()?.json().await?;
    let latest_build = *builds.builds.last().context("aucun build Paper disponible pour cette version")?;

    let build_info_url = format!("https://api.papermc.io/v2/projects/paper/versions/{mc_version}/builds/{latest_build}");
    let build_info: PaperBuildInfo = client.get(&build_info_url).send().await?.error_for_status()?.json().await?;
    let filename = &build_info.downloads.application.name;

    let jar_url = format!(
        "https://api.papermc.io/v2/projects/paper/versions/{mc_version}/builds/{latest_build}/downloads/{filename}"
    );
    download_file(client, &jar_url, dest, None).await
}

/// Télécharge le jar serveur (vanilla ou Paper selon `software`), écrit `eula.txt`
/// (l'utilisateur doit avoir explicitement accepté l'EULA Mojang via `eula_accepted`)
/// et un `server.properties` minimal avec le port choisi.
pub async fn provision(client: &reqwest::Client, server: &LocalServer) -> Result<()> {
    if !server.eula_accepted {
        bail!("l'EULA Minecraft (https://aka.ms/MinecraftEULA) doit être acceptée avant l'installation");
    }

    let jar_path = server.jar_path();
    match server.software {
        ServerSoftware::Vanilla => download_vanilla_jar(client, &server.mc_version, &jar_path).await?,
        ServerSoftware::Paper => download_paper_jar(client, &server.mc_version, &jar_path).await?,
    }

    std::fs::write(server.dir().join("eula.txt"), "eula=true\n")?;

    let properties = format!(
        "server-port={}\nonline-mode=true\nmotd=Serveur {} (via DuckyLauncher)\n",
        server.port, server.name
    );
    let props_path = server.dir().join("server.properties");
    if !props_path.exists() {
        std::fs::write(props_path, properties)?;
    }

    Ok(())
}

/// Démarre le serveur en mode headless (`nogui`) dans son dossier. Le process reste
/// géré côté UI (lecture stdout pour les logs, envoi de commandes via stdin, arrêt
/// propre en écrivant "stop\n" sur le stdin plutôt qu'un kill brutal).
pub fn start(server: &LocalServer) -> Result<Child> {
    if !server.jar_path().exists() {
        bail!("le jar du serveur n'est pas installé — appelle provision() d'abord");
    }

    let child = Command::new(&server.java_path)
        .current_dir(server.dir())
        .arg(format!("-Xms{}M", server.memory_min_mb))
        .arg(format!("-Xmx{}M", server.memory_max_mb))
        .arg("-jar")
        .arg("server.jar")
        .arg("nogui")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("échec du lancement du serveur (vérifie java_path)")?;

    Ok(child)
}

/// Arrêt propre : écrit "stop" sur le stdin du process (équivalent à taper /stop
/// dans la console serveur), pour laisser Minecraft sauvegarder le monde avant de quitter.
pub fn stop_gracefully(child: &mut Child) -> Result<()> {
    use std::io::Write;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(b"stop\n")?;
    }
    Ok(())
}
