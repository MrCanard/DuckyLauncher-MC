//! Forge (et NeoForge, même principe) sont nettement plus complexes que Fabric/Quilt :
//! il n'y a pas d'API meta simple retournant un profil JSON prêt à l'emploi.
//! Le processus officiel est :
//!
//! 1. Télécharger l'installer :
//!    https://maven.minecraftforge.net/net/minecraftforge/forge/{mc_version}-{forge_version}/forge-{mc_version}-{forge_version}-installer.jar
//!    (NeoForge : https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar)
//! 2. Lancer l'installer en mode headless :
//!    `java -jar forge-installer.jar --installClient <chemin_vers_.minecraft>`
//!    (Forge fournit un mode "ClientInstall" pilotable en ligne de commande depuis les
//!    versions récentes ; pour les très vieilles versions il faut parfois piloter l'UI Swing.)
//! 3. L'installer génère un profil de version (`versions/<id>/​<id>.json`) et télécharge
//!    lui-même ses librairies dans le dossier `libraries/` standard — on peut donc
//!    réutiliser `minecraft::VersionDetails` / `minecraft::launch` une fois l'install faite.
//! 4. Pour NeoForge, le principe est identique (l'installer est un fork de celui de Forge).
//!
//! Ci-dessous : le squelette fonctionnel (téléchargement + exécution de l'installer).
//! À compléter selon la version ciblée (le comportement de l'installer a changé plusieurs
//! fois au fil des années de Minecraft).

use crate::minecraft::download_file;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;

pub enum LoaderKind {
    Forge,
    NeoForge,
}

pub fn installer_url(kind: &LoaderKind, mc_version: &str, loader_version: &str) -> String {
    match kind {
        LoaderKind::Forge => format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{mc_version}-{loader_version}/forge-{mc_version}-{loader_version}-installer.jar"
        ),
        LoaderKind::NeoForge => format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"
        ),
    }
}

/// Télécharge puis exécute l'installer en mode client. `java_path` doit pointer vers
/// un JRE valide. `mc_root` est le dossier `.minecraft` partagé (paths::app_data_dir()
/// ou équivalent) où l'installer ira écrire versions/ et libraries/.
pub async fn install(
    client: &reqwest::Client,
    kind: LoaderKind,
    mc_version: &str,
    loader_version: &str,
    java_path: &str,
    mc_root: &PathBuf,
) -> Result<()> {
    let url = installer_url(&kind, mc_version, loader_version);
    let installer_path = std::env::temp_dir().join(format!("forge-installer-{loader_version}.jar"));
    download_file(client, &url, &installer_path, None).await?;

    let status = Command::new(java_path)
        .arg("-jar")
        .arg(&installer_path)
        .arg("--installClient")
        .arg(mc_root)
        .status()
        .context("échec du lancement de l'installer Forge/NeoForge (java_path correct ?)")?;

    if !status.success() {
        bail!("l'installer Forge/NeoForge a échoué (code {:?})", status.code());
    }

    Ok(())
}
