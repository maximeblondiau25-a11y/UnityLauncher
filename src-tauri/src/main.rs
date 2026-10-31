#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth;
mod install;
mod launch;
mod paths;
mod remote;
mod settings;

use install::{Emit, Installed, Progress};
use serde::Serialize;
use settings::Settings;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Notify;

struct AppState {
    settings: Mutex<Settings>,
    client: reqwest::Client,
    busy: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    stop: Arc<Notify>,
}

fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e:#}")
}

fn cfg(state: &State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

// ---------------------------------------------------------------------------
// Paramètres
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    cfg(&state)
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    settings.save().map_err(err)?;
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[derive(Serialize)]
struct SystemInfo {
    total_ram_mb: u64,
    os: String,
    data_dir: String,
    version: String,
}

#[tauri::command]
fn system_info() -> SystemInfo {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    SystemInfo {
        total_ram_mb: sys.total_memory() / 1024 / 1024,
        os: install::os_name().to_string(),
        data_dir: paths::base_dir().to_string_lossy().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

// ---------------------------------------------------------------------------
// Compte Microsoft
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_account() -> Option<auth::Account> {
    auth::load_account()
}

#[tauri::command]
fn logout() {
    auth::logout();
}

/// Démarre la connexion : renvoie le code à afficher, puis attend la validation en arrière-plan.
/// Événements émis : "login-done" (compte) ou "login-error" (message).
#[tauri::command]
async fn login_begin(app: AppHandle, state: State<'_, AppState>) -> Result<auth::DeviceCode, String> {
    let client = state.client.clone();
    let client_id = cfg(&state).ms_client_id;
    let code = auth::request_device_code(&client, &client_id).await.map_err(err)?;
    let _ = app.opener().open_url(code.verification_uri.clone(), None::<&str>);

    let code_for_task = code.clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = async {
            let tokens = auth::poll_device_code(&client, &client_id, &code_for_task).await?;
            let session = auth::minecraft_session(&client, &tokens.access).await?;
            let account = auth::Account {
                name: session.name.clone(),
                uuid: session.uuid.clone(),
            };
            auth::save_account(&account, &tokens.refresh)?;
            anyhow::Ok(account)
        }
        .await;
        match result {
            Ok(account) => {
                let _ = app2.emit("login-done", account);
            }
            Err(e) => {
                let _ = app2.emit("login-error", err(e));
            }
        }
    });
    Ok(code)
}

// ---------------------------------------------------------------------------
// Infos distantes et statut serveur
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct RemoteResult {
    info: remote::RemoteInfo,
    /// true si les infos viennent du cache (hors ligne).
    offline: bool,
}

#[tauri::command]
async fn fetch_remote(state: State<'_, AppState>) -> Result<RemoteResult, String> {
    let url = cfg(&state).remote_url;
    match remote::fetch_remote(&state.client, &url).await {
        Ok(info) => Ok(RemoteResult { info, offline: false }),
        Err(e) => match remote::load_cached_remote() {
            Some(info) => Ok(RemoteResult { info, offline: true }),
            None => Err(err(e)),
        },
    }
}

#[tauri::command]
async fn server_status(address: String) -> remote::ServerStatus {
    remote::ping_server(&address).await.unwrap_or_default()
}

#[derive(Serialize)]
struct InstallState {
    installed: Installed,
    minecraft_ready: bool,
    neoforge_ready: bool,
    running: bool,
}

#[tauri::command]
fn install_state(state: State<'_, AppState>, minecraft: String, neoforge: String) -> InstallState {
    InstallState {
        installed: Installed::load(),
        minecraft_ready: install::minecraft_ready(&minecraft),
        neoforge_ready: install::neoforge_ready(&neoforge),
        running: state.running.load(Ordering::SeqCst),
    }
}

// ---------------------------------------------------------------------------
// Installer / mettre à jour / jouer
// ---------------------------------------------------------------------------

fn make_emit(app: &AppHandle) -> Emit {
    let app = app.clone();
    Arc::new(move |p: Progress| {
        let _ = app.emit("progress", p);
    })
}

async fn do_install(
    app: &AppHandle,
    client: &reqwest::Client,
    s: &Settings,
    info: &remote::RemoteInfo,
) -> anyhow::Result<std::path::PathBuf> {
    let e = make_emit(app);
    let java = install::ensure_java(client, &s.java_path, &e).await?;
    if !install::minecraft_ready(&info.minecraft) {
        install::install_minecraft(client, &info.minecraft, &e).await?;
    }
    install::install_neoforge(client, &java, &info.neoforge, &e).await?;
    let installed = Installed::load();
    if installed.modpack_version != info.modpack.version {
        install::install_modpack(client, &info.modpack, &e).await?;
    }
    Ok(java)
}

#[tauri::command]
async fn install_or_update(
    app: AppHandle,
    state: State<'_, AppState>,
    info: remote::RemoteInfo,
) -> Result<(), String> {
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("Une opération est déjà en cours.".into());
    }
    let s = cfg(&state);
    let r = do_install(&app, &state.client, &s, &info).await;
    state.busy.store(false, Ordering::SeqCst);
    r.map(|_| ()).map_err(err)
}

#[tauri::command]
async fn play(app: AppHandle, state: State<'_, AppState>, info: remote::RemoteInfo) -> Result<(), String> {
    if state.running.load(Ordering::SeqCst) {
        return Err("Le jeu est déjà lancé.".into());
    }
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("Une opération est déjà en cours.".into());
    }
    let s = cfg(&state);
    let client = state.client.clone();
    let result = async {
        if !info.maintenance.is_empty() {
            anyhow::bail!("Maintenance : {}", info.maintenance);
        }
        // Installation / mise à jour si nécessaire
        let java = do_install(&app, &client, &s, &info).await?;

        let e = make_emit(&app);
        e(Progress {
            stage: "launch".into(),
            label: "Connexion au compte Microsoft".into(),
            current: 0,
            total: 0,
        });
        let session = auth::fresh_session(&client, &s.ms_client_id).await?;

        launch::write_options(&s)?;
        let plan = launch::build(&s, &session, &info.minecraft, &info.neoforge, &info.server.address)?;

        e(Progress {
            stage: "launch".into(),
            label: "Démarrage du jeu".into(),
            current: 0,
            total: 0,
        });
        let mut cmd = Command::new(&java);
        cmd.args(&plan.args)
            .current_dir(&plan.game_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let mut child = cmd.spawn()?;
        Ok::<_, anyhow::Error>((child.stdout.take(), child.stderr.take(), child))
    }
    .await;
    state.busy.store(false, Ordering::SeqCst);

    let (stdout, stderr, mut child) = result.map_err(err)?;
    state.running.store(true, Ordering::SeqCst);
    let _ = app.emit("game-started", ());

    // Journal du jeu
    if let Some(out) = stdout {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app.emit("game-log", line);
            }
        });
    }
    if let Some(errout) = stderr {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(errout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app.emit("game-log", line);
            }
        });
    }

    // Surveillance du processus
    let running = state.running.clone();
    let stop = state.stop.clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let code = tokio::select! {
            status = child.wait() => status.ok().and_then(|s| s.code()),
            _ = stop.notified() => {
                let _ = child.kill().await;
                Some(-1)
            }
        };
        running.store(false, Ordering::SeqCst);
        let _ = app2.emit("game-exit", code);
    });

    if s.close_on_launch {
        let app3 = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(6)).await;
            app3.exit(0);
        });
    }
    Ok(())
}

#[tauri::command]
fn stop_game(state: State<'_, AppState>) {
    // Ne notifie que si un jeu tourne, sinon la permission resterait en attente
    // et arrêterait le prochain lancement.
    if state.running.load(Ordering::SeqCst) {
        state.stop.notify_one();
    }
}

#[tauri::command]
fn open_data_folder(app: AppHandle) -> Result<(), String> {
    let dir = paths::base_dir();
    std::fs::create_dir_all(&dir).map_err(err)?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(err)
}

#[tauri::command]
fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("lien non autorisé".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(err)
}

fn main() {
    let client = reqwest::Client::builder()
        .user_agent(concat!("PokemonUnityLauncher/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .expect("client HTTP");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            settings: Mutex::new(Settings::load()),
            client,
            busy: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(Notify::new()),
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            system_info,
            get_account,
            logout,
            login_begin,
            fetch_remote,
            server_status,
            install_state,
            install_or_update,
            play,
            stop_game,
            open_data_folder,
            open_url,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Pokémon Unity");
}
