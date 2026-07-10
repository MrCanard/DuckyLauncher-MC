use std::path::PathBuf;

/// Dossier racine de données de l'app, ex: ~/.local/share/duckyluncher (Linux),
/// %APPDATA%/duckyluncher (Windows), ~/Library/Application Support/duckyluncher (macOS).
pub fn app_data_dir() -> PathBuf {
    let base = dirs::data_dir().expect("impossible de déterminer le dossier de données");
    let dir = base.join("duckyluncher");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn instances_dir() -> PathBuf {
    let dir = app_data_dir().join("instances");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn instance_dir(instance_id: &str) -> PathBuf {
    let dir = instances_dir().join(instance_id);
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn meta_dir() -> PathBuf {
    // Stockage partagé : versions vanilla, librairies, assets (déduplication entre instances)
    let dir = app_data_dir().join("meta");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn versions_dir() -> PathBuf {
    let dir = meta_dir().join("versions");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn libraries_dir() -> PathBuf {
    let dir = meta_dir().join("libraries");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn assets_dir() -> PathBuf {
    let dir = meta_dir().join("assets");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn accounts_file() -> PathBuf {
    app_data_dir().join("accounts.json")
}

/// Dossier contenant les serveurs Minecraft locaux hébergés depuis le PC de l'utilisateur.
pub fn servers_dir() -> PathBuf {
    let dir = app_data_dir().join("servers");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn server_dir(server_id: &str) -> PathBuf {
    let dir = servers_dir().join(server_id);
    std::fs::create_dir_all(&dir).ok();
    dir
}
