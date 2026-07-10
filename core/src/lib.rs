//! launcher-core
//!
//! Logique métier du launcher : auth Microsoft/Xbox/Minecraft,
//! téléchargement & lancement du jeu, intégration Modrinth,
//! gestion d'instances, mod loaders (Fabric/Quilt/Forge/NeoForge).

pub mod auth;
pub mod instance;
pub mod minecraft;
pub mod modrinth;
pub mod paths;
pub mod server;

pub mod loaders {
    pub mod fabric;
    pub mod quilt;
    pub mod forge;
}

pub use anyhow::{Error, Result};
