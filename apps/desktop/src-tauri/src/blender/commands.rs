//! Comandes Tauri per a Blender.

use super::{
    detect_blender_binary, install_addon, install_mcp_addon, launch_with_bridge,
    run_blender_headless, status, BlenderSocketClient, DEFAULT_SOCKET_PORT,
};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{command, AppHandle, Emitter};

static LIVE_RUNNING: AtomicBool = AtomicBool::new(false);
static WATCHER: Mutex<Option<notify::RecommendedWatcher>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize)]
pub struct BlenderLiveFrame {
    pub image: String, // base64 PNG
}

pub struct BlenderState {
    pub port: u16,
}

impl BlenderState {
    pub fn new() -> Self {
        Self {
            port: DEFAULT_SOCKET_PORT,
        }
    }
}

impl Default for BlenderState {
    fn default() -> Self {
        Self::new()
    }
}

fn client() -> BlenderSocketClient {
    BlenderSocketClient::new(DEFAULT_SOCKET_PORT)
}

#[command]
pub async fn blender_ping() -> Result<bool, String> {
    Ok(client().ping())
}

#[command]
pub async fn blender_info() -> Result<super::BlenderInfo, String> {
    client().info().map_err(|e| e.to_string())
}

#[command]
pub async fn blender_execute(code: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || client().execute_auto(&code))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[command]
pub async fn blender_run_code(code: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || run_blender_headless(&code))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[command]
pub async fn blender_detect_path() -> Result<Option<String>, String> {
    Ok(detect_blender_binary())
}

/// Estat: instal·lat (binari) + connectat (port obert).
#[command]
pub async fn blender_status() -> Result<super::BlenderStatus, String> {
    Ok(status())
}

/// Arrenca Blender amb el pont perquè obri el port 9876.
#[command]
pub async fn blender_launch(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || launch_with_bridge(&app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Instal·la i activa l'add-on de Blender (headless).
#[command]
pub async fn blender_install_addon(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || install_addon(&app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Descarrega i instal·la automàticament l'add-on comunitari blender-mcp.
#[command]
pub async fn blender_install_mcp_addon() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(install_mcp_addon)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[command]
pub async fn blender_port_open(port: u16) -> Result<bool, String> {
    Ok(
        std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            std::time::Duration::from_millis(500),
        )
        .is_ok(),
    )
}

#[command]
pub async fn blender_auto_connect(port: u16) -> Result<bool, String> {
    let ok = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(800),
    )
    .is_ok();
    Ok(ok)
}

// ── Connexió automàtica (vigilant de fons) ────────────────────────────────
// L'usuari només ha d'obrir Blender; NoOrbit el detecta i connecta sol.

/// Arrenca el vigilant que connecta Blender automàticament.
#[command]
pub async fn blender_auto_connect_start(app: AppHandle) -> Result<(), String> {
    super::auto_connect::spawn_auto_connect(app);
    Ok(())
}

/// Atura el vigilant.
#[command]
pub async fn blender_auto_connect_stop() -> Result<(), String> {
    super::auto_connect::stop();
    Ok(())
}

/// Diu si el vigilant està actiu (per reflectir-ho a la UI).
#[command]
pub async fn blender_auto_connect_status() -> Result<bool, String> {
    Ok(super::auto_connect::is_running())
}

/// Obre el bucle de vista en viu: envia frames "blender://frame" cada 2 s.
#[command]
pub async fn blender_live_start(app: AppHandle) -> Result<(), String> {
    if LIVE_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(()); // ja actiu
    }
    std::thread::spawn(move || {
        let c = client();
        while LIVE_RUNNING.load(Ordering::SeqCst) {
            match c.screenshot() {
                Ok(Some(img)) => {
                    let _ = app.emit(
                        "blender://frame",
                        BlenderLiveFrame { image: img },
                    );
                }
                _ => {}
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
    Ok(())
}

#[command]
pub async fn blender_live_stop() -> Result<(), String> {
    LIVE_RUNNING.store(false, Ordering::SeqCst);
    Ok(())
}

/// Observa la carpeta del workspace per canvis a fitxers .blend.
#[command]
pub async fn blender_watch_workspace(app: AppHandle, path: String) -> Result<(), String> {
    use notify::{Event, EventKind, RecursiveMode, Watcher};

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                for p in event.paths {
                    if p.extension().map(|e| e == "blend").unwrap_or(false) {
                        let _ = app.emit(
                            "blender://file-changed",
                            p.to_string_lossy().to_string(),
                        );
                    }
                }
            }
        }
    })
    .map_err(|e| e.to_string())?;

    watcher
        .watch(std::path::Path::new(&path), RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    *WATCHER.lock().unwrap() = Some(watcher);
    Ok(())
}
