#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use launcher_core::{auth, instance, minecraft, modrinth, server};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::State;
use tauri_plugin_updater::UpdaterExt;

struct AppState {
    http: reqwest::Client,
    // device_code courant en attente de validation utilisateur
    pending_device_code: Mutex<Option<String>>,
    // processus des serveurs locaux actuellement démarrés, par id de serveur
    running_servers: Mutex<HashMap<String, std::process::Child>>,
}

/// Vérifie s'il existe une nouvelle version publiée (via le manifeste JSON pointé
/// par `plugins.updater.endpoints` dans tauri.conf.json).
/// - Rien à faire si on est déjà à jour (retourne simplement, aucune action visible).
/// - Sinon : téléchargement + vérification de signature + installation silencieuse,
///   puis redémarrage automatique de l'app sur la nouvelle version.
/// Ce n'est PAS l'installeur .exe lui-même (ça, c'est le bundle NSIS généré par
/// `cargo tauri build`) : c'est ce qui tourne UNE FOIS L'APP DÉJÀ INSTALLÉE, à
/// chaque démarrage, pour garder les utilisateurs à jour sans repasser par le site.
async fn check_for_update(app: tauri::AppHandle) {
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            eprintln!("updater non disponible: {e}");
            return;
        }
    };

    match updater.check().await {
        Ok(Some(update)) => {
            println!("Mise à jour {} disponible, installation...", update.version);
            let result = update
                .download_and_install(
                    |_chunk_len, _total| { /* on pourrait émettre un event de progression vers l'UI ici */ },
                    || println!("téléchargement terminé, installation en cours..."),
                )
                .await;

            match result {
                Ok(()) => {
                    println!("Mise à jour installée, redémarrage de DuckyLauncher.");
                    app.restart(); // fourni par tauri-plugin-process
                }
                Err(e) => eprintln!("échec de l'installation de la mise à jour: {e}"),
            }
        }
        Ok(None) => {
            // Déjà à jour : on ne fait strictement rien, l'app démarre normalement.
        }
        Err(e) => eprintln!("échec de la vérification de mise à jour: {e}"),
    }
}

#[tauri::command]
async fn start_login(state: State<'_, AppState>) -> Result<auth::DeviceCodeResponse, String> {
    let resp = auth::request_device_code(&state.http).await.map_err(|e| e.to_string())?;
    *state.pending_device_code.lock().unwrap() = Some(resp.device_code.clone());
    Ok(resp)
}

/// À appeler périodiquement (toutes les `interval` secondes renvoyées par start_login)
/// depuis le frontend, jusqu'à obtenir un compte non-null.
#[tauri::command]
async fn poll_login(state: State<'_, AppState>) -> Result<Option<auth::MinecraftAccount>, String> {
    let device_code = state
        .pending_device_code
        .lock()
        .unwrap()
        .clone()
        .ok_or("aucune connexion en cours")?;

    match auth::poll_device_token(&state.http, &device_code).await.map_err(|e| e.to_string())? {
        None => Ok(None), // toujours en attente
        Some((access_token, refresh_token)) => {
            let account = auth::complete_login(&state.http, &access_token, &refresh_token)
                .await
                .map_err(|e| e.to_string())?;
            auth::save_account(&account).map_err(|e| e.to_string())?;
            *state.pending_device_code.lock().unwrap() = None;
            Ok(Some(account))
        }
    }
}

#[tauri::command]
fn list_accounts() -> Result<Vec<auth::MinecraftAccount>, String> {
    auth::load_accounts().map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_mc_versions(state: State<'_, AppState>) -> Result<minecraft::VersionManifest, String> {
    minecraft::fetch_version_manifest(&state.http).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn list_instances() -> Result<Vec<instance::Instance>, String> {
    instance::list_instances().map_err(|e| e.to_string())
}

#[tauri::command]
fn create_instance(
    name: String,
    mc_version: String,
    loader: String,
    loader_version: Option<String>,
) -> Result<instance::Instance, String> {
    let loader = match loader.as_str() {
        "fabric" => instance::Loader::Fabric,
        "quilt" => instance::Loader::Quilt,
        "forge" => instance::Loader::Forge,
        "neoforge" => instance::Loader::NeoForge,
        _ => instance::Loader::Vanilla,
    };
    let inst = instance::Instance::new(&name, &mc_version, loader, loader_version);
    inst.save().map_err(|e| e.to_string())?;
    Ok(inst)
}

#[tauri::command]
fn delete_instance(instance_id: String) -> Result<(), String> {
    let inst = instance::Instance::load(&instance_id).map_err(|e| e.to_string())?;
    inst.delete().map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_mods(
    state: State<'_, AppState>,
    query: String,
    project_type: String,
    game_version: Option<String>,
    loader: Option<String>,
) -> Result<modrinth::SearchResult, String> {
    let client = modrinth::ModrinthClient::new().map_err(|e| e.to_string())?;
    let _ = &state.http; // client Modrinth a son propre reqwest::Client interne
    client
        .search(modrinth::SearchParams {
            query: &query,
            project_type: &project_type,
            game_version: game_version.as_deref(),
            loader: loader.as_deref(),
            limit: 20,
            offset: 0,
        })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn install_mod_to_instance(instance_id: String, project_id: String, version_id: String) -> Result<instance::Instance, String> {
    let mut inst = instance::Instance::load(&instance_id).map_err(|e| e.to_string())?;
    let client = modrinth::ModrinthClient::new().map_err(|e| e.to_string())?;
    let version = client.get_version(&version_id).await.map_err(|e| e.to_string())?;
    instance::install_mod(&client, &mut inst, &project_id, &version, &version.version_number.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(inst)
}

#[tauri::command]
async fn launch_instance(state: State<'_, AppState>, instance_id: String, account_uuid: String) -> Result<u32, String> {
    let inst = instance::Instance::load(&instance_id).map_err(|e| e.to_string())?;
    let accounts = auth::load_accounts().map_err(|e| e.to_string())?;
    let account = accounts
        .into_iter()
        .find(|a| a.mc_uuid == account_uuid)
        .ok_or("compte introuvable")?;

    let manifest = minecraft::fetch_version_manifest(&state.http).await.map_err(|e| e.to_string())?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == inst.mc_version)
        .ok_or("version Minecraft introuvable")?;
    let details = minecraft::fetch_version_details(&state.http, &entry.url).await.map_err(|e| e.to_string())?;
    minecraft::ensure_version_installed(&state.http, &details).await.map_err(|e| e.to_string())?;

    let mut extra_classpath = vec![];
    let mut override_main_class = None;

    match inst.loader {
        instance::Loader::Fabric => {
            if let Some(lv) = &inst.loader_version {
                let (main_class, jars) = launcher_core::loaders::fabric::install(&state.http, &inst.mc_version, lv)
                    .await
                    .map_err(|e| e.to_string())?;
                override_main_class = Some(main_class);
                extra_classpath = jars;
            }
        }
        instance::Loader::Quilt => {
            if let Some(lv) = &inst.loader_version {
                let (main_class, jars) = launcher_core::loaders::quilt::install(&state.http, &inst.mc_version, lv)
                    .await
                    .map_err(|e| e.to_string())?;
                override_main_class = Some(main_class);
                extra_classpath = jars;
            }
        }
        // Forge/NeoForge : voir loaders::forge (installation lourde via installer.jar,
        // à déclencher séparément avant le lancement plutôt qu'à chaque launch).
        _ => {}
    }

    let opts = minecraft::LaunchOptions {
        java_path: inst.java_path.clone().unwrap_or_else(|| "java".to_string()),
        memory_min_mb: inst.memory_min_mb,
        memory_max_mb: inst.memory_max_mb,
        game_dir: inst.dir(),
        width: 854,
        height: 480,
        extra_classpath,
        override_main_class,
    };

    let child = minecraft::launch(&details, &account, &opts).map_err(|e| e.to_string())?;
    Ok(child.id())
}

// ---------- Serveurs locaux (hébergés gratuitement sur le PC de l'utilisateur) ----------

#[tauri::command]
fn list_servers() -> Result<Vec<server::LocalServer>, String> {
    server::list_servers().map_err(|e| e.to_string())
}

#[tauri::command]
fn create_server(name: String, mc_version: String, software: String, port: u16) -> Result<server::LocalServer, String> {
    let sw = match software.as_str() {
        "paper" => server::ServerSoftware::Paper,
        _ => server::ServerSoftware::Vanilla,
    };
    let mut srv = server::LocalServer::new(&name, &mc_version, sw);
    srv.port = port;
    srv.save().map_err(|e| e.to_string())?;
    Ok(srv)
}

/// À appeler après que l'utilisateur a explicitement coché "j'accepte l'EULA Minecraft"
/// dans l'UI (lien affiché : https://aka.ms/MinecraftEULA). Télécharge ensuite le jar.
#[tauri::command]
async fn install_server(state: State<'_, AppState>, server_id: String) -> Result<(), String> {
    let mut srv = server::LocalServer::load(&server_id).map_err(|e| e.to_string())?;
    srv.eula_accepted = true;
    srv.save().map_err(|e| e.to_string())?;
    server::provision(&state.http, &srv).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn start_server(state: State<'_, AppState>, server_id: String) -> Result<(), String> {
    let srv = server::LocalServer::load(&server_id).map_err(|e| e.to_string())?;
    let child = server::start(&srv).map_err(|e| e.to_string())?;
    state.running_servers.lock().unwrap().insert(server_id, child);
    Ok(())
}

#[tauri::command]
fn stop_server(state: State<'_, AppState>, server_id: String) -> Result<(), String> {
    // On retire le child de la map tout de suite : "stopped/stopping" du point de vue
    // de l'UI, puis on l'attend en arrière-plan pour ne pas laisser de zombie process
    // (le vrai arrêt Minecraft, sauvegarde du monde incluse, prend quelques secondes).
    let mut child = {
        let mut running = state.running_servers.lock().unwrap();
        running.remove(&server_id)
    }
    .ok_or("ce serveur n'est pas démarré")?;

    server::stop_gracefully(&mut child).map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[tauri::command]
fn is_server_running(state: State<'_, AppState>, server_id: String) -> bool {
    state.running_servers.lock().unwrap().contains_key(&server_id)
}

#[tauri::command]
fn delete_server(server_id: String) -> Result<(), String> {
    let srv = server::LocalServer::load(&server_id).map_err(|e| e.to_string())?;
    srv.delete().map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState {
            http: reqwest::Client::new(),
            pending_device_code: Mutex::new(None),
            running_servers: Mutex::new(HashMap::new()),
        })
        .setup(|app| {
            // Vérif de mise à jour en tâche de fond, ne bloque pas l'ouverture de l'UI.
            // Silencieux si déjà à jour ; installe + redémarre automatiquement sinon.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                check_for_update(handle).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_login,
            poll_login,
            list_accounts,
            list_mc_versions,
            list_instances,
            create_instance,
            delete_instance,
            search_mods,
            install_mod_to_instance,
            launch_instance,
            list_servers,
            create_server,
            install_server,
            start_server,
            stop_server,
            is_server_running,
            delete_server,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application Tauri");
}
