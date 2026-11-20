//! Comandes Tauri per a Unreal Engine 5.

use super::builder::{self, PackageOptions};
use super::client::UnrealClient;
use super::launcher;
use super::python;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{command, AppHandle, Emitter, State};

use crate::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct UnrealConfig {
    pub host: String,
    pub port: u16,
}

impl Default for UnrealConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 30010,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UnrealStatus {
    pub connected: bool,
    pub engine_version: Option<String>,
    pub project_name: Option<String>,
}

/// Client per a la configuració rebuda de la UI.
///
/// La UI sempre envia el port per defecte (`DEFAULT_UNREAL_CONFIG`): si no es
/// resolgués aquí, un usuari amb `unreal.rc_port` canviat connectaria al port
/// vell i no rebia mai resposta (el mateix error silenciós que `ports.rs`
/// vol evitar).
fn client_for(config: &UnrealConfig) -> UnrealClient {
    UnrealClient::with_port(super::client::resolve_port(config.port))
}

/// Cau de la sonda d'informació: el watchdog de la UI pregunta l'estat cada
/// 5 s, i cada pregunta seria un script Python executat a l'editor.
static INFO_CACHE: std::sync::LazyLock<std::sync::Mutex<Option<(u16, std::time::Instant, python::EditorInfo)>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

fn cached_info(port: u16) -> Option<python::EditorInfo> {
    let guard = INFO_CACHE.lock().ok()?;
    let (p, at, info) = guard.as_ref()?;
    if *p == port && at.elapsed() < std::time::Duration::from_secs(60) {
        return Some(info.clone());
    }
    None
}

async fn editor_info(client: &UnrealClient) -> python::EditorInfo {
    let port = client.port();
    if let Some(hit) = cached_info(port) {
        return hit;
    }
    let info = python::probe_info(client).await;
    if let Ok(mut guard) = INFO_CACHE.lock() {
        *guard = Some((port, std::time::Instant::now(), info.clone()));
    }
    info
}

#[command]
pub async fn unreal_ping(config: UnrealConfig) -> Result<bool, String> {
    client_for(&config).ping().await.map_err(|e| e.to_string())
}

#[command]
pub async fn unreal_info(config: UnrealConfig) -> Result<UnrealStatus, String> {
    let client = client_for(&config);
    let connected = client.ping().await.unwrap_or(false);
    if !connected {
        if let Ok(mut guard) = INFO_CACHE.lock() {
            *guard = None;
        }
        return Ok(UnrealStatus {
            connected: false,
            engine_version: None,
            project_name: None,
        });
    }
    // `GET /remote/info` només llista les rutes HTTP: la versió i el projecte
    // s'han de demanar a l'editor en Python (amb el HTTP com a reserva).
    let http = client.info().await.ok();
    let probe = editor_info(&client).await;
    Ok(UnrealStatus {
        connected: true,
        engine_version: probe
            .engine_version
            .or_else(|| http.as_ref().and_then(|h| h.engine_version.clone())),
        project_name: probe
            .project_name
            .or_else(|| http.as_ref().and_then(|h| h.project_name.clone())),
    })
}

#[command]
pub async fn unreal_detect_installations() -> Result<Vec<String>, String> {
    Ok(launcher::detect_engine_installations()
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}

/// Instal·lacions A MITGES: l'Epic Games Launcher encara està baixant el
/// motor (o la descàrrega es va cancel·lar). No serveixen per connectar,
/// però l'UI les mostra perquè l'usuari sap on les té i per què NoOrbit diu
/// que no hi ha cap motor.
#[command]
pub async fn unreal_detect_pending_installations() -> Result<Vec<String>, String> {
    Ok(launcher::detect_pending_installations()
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}

/// Cerca un fitxer `.uproject` a la carpeta del projecte obert (arrel + un
/// nivell de subcarpetes): el fa servir la UI per AUTO-OMPLIR el projecte
/// sense que l'usuari hagui de seleccionar-lo a mà.
#[command]
pub async fn unreal_find_uproject(root: String) -> Result<Option<String>, String> {
    let dir = PathBuf::from(&root);
    if !dir.is_dir() {
        return Ok(None);
    }
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };
    let mut nested: Option<PathBuf> = None;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) == Some("uproject") {
            return Ok(Some(p.to_string_lossy().to_string()));
        }
        if p.is_dir() && nested.is_none() {
            if let Ok(sub) = std::fs::read_dir(&p) {
                for s in sub.flatten() {
                    let sp = s.path();
                    if sp.extension().and_then(|x| x.to_str()) == Some("uproject") {
                        nested = Some(sp);
                        break;
                    }
                }
            }
        }
    }
    Ok(nested.map(|p| p.to_string_lossy().to_string()))
}

/// Conecta (i arrenca si cal) l'editor amb un projecte.
///
/// `engine_root` és opcional: si l'usuari ha triat a mà la carpeta del motor
/// (per exemple un Unreal instal·lat en un DISC EXTERN que la detecció
/// automàtica no ha vist), es fa servir aquesta; si no, es detecta sol.
#[command]
pub async fn unreal_auto_connect(
    project: String,
    timeout: u64,
    engine_root: Option<String>,
) -> Result<launcher::LaunchReport, String> {
    let root = engine_root
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from);
    launcher::auto_connect(&PathBuf::from(&project), timeout, root.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Port EFECTIU de la Remote Control. La UI el mostra perquè, quan l'editor
/// n'usa un altre (més d'un editor obert, 30010 ocupat…), la connexió no
/// falla «en silenci»: es veu contra quin port s'està intentant.
#[command]
pub async fn unreal_get_rc_port() -> Result<u16, String> {
    Ok(super::client::configured_port())
}

/// Desa el port a `config.toml` (`[unreal] rc_port`) i actualitza la
/// configuració en memòria. Torna el port efectiu (0 = el per defecte).
/// Efecte immediat: els clients es creen a cada crida i resolen el port
/// llavors (i el bootstrap llegeix la variable d'entorn en arrencar l'editor).
#[command]
pub async fn unreal_set_rc_port(
    state: State<'_, AppState>,
    port: u16,
) -> Result<u16, String> {
    let port = if port == 0 { crate::ports::UNREAL_RC_PORT } else { port };
    let mut cfg = crate::config::AppConfig::load().map_err(|e| e.to_string())?;
    cfg.unreal.rc_port = port;
    cfg.save().map_err(|e| e.to_string())?;
    // La còpia en memòria (State) s'usa des d'altres fluxos: si no
    // s'actualitza, la propera escriptura de la configuració el perdria.
    state.config.write().await.unreal.rc_port = port;
    Ok(port)
}

#[command]
pub async fn unreal_run_python(
    config: UnrealConfig,
    code: String,
) -> Result<python::PythonResult, String> {
    let client = client_for(&config);
    python::run_script(&client, &code)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn unreal_create_blueprint(
    config: UnrealConfig,
    name: String,
    parent_class: String,
) -> Result<python::PythonResult, String> {
    let code = python::script_create_blueprint(&name, &parent_class);
    let client = client_for(&config);
    python::run_script(&client, &code)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn unreal_spawn_actor(
    config: UnrealConfig,
    class_path: String,
    location: [f64; 3],
) -> Result<python::PythonResult, String> {
    let code = python::script_spawn_actor(&class_path, location);
    let client = client_for(&config);
    python::run_script(&client, &code)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn unreal_import_asset(
    config: UnrealConfig,
    file_path: String,
    dest_path: String,
) -> Result<python::PythonResult, String> {
    let code = python::script_import_asset(&file_path, &dest_path);
    let client = client_for(&config);
    python::run_script(&client, &code)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn unreal_build_lighting(
    config: UnrealConfig,
    quality: String,
) -> Result<python::PythonResult, String> {
    let code = python::script_build_lighting(&quality);
    let client = client_for(&config);
    python::run_script(&client, &code)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn unreal_package_game(
    app: AppHandle,
    engine_root: String,
    options: PackageOptions,
) -> Result<builder::BuildResult, String> {
    builder::package_game(&PathBuf::from(&engine_root), &options, &app)
        .await
        .map_err(|e| e.to_string())
}

// ── Agent MCP (plugin oficial d'Unreal 5.8+) ─────────────────────────────

/// ORQUESTRADOR «Activar Agent Unreal»: Ollama, plugins MCP al .uproject,
/// auto-arrencada del servidor (ini), arrencada de l'editor si es demana i
/// handshake MCP amb el catàleg d'eines. Idempotent i amb registre de passos.
#[command]
pub async fn unreal_agent_setup(
    app: AppHandle,
    project: String,
    launch: bool,
) -> Result<super::agent::SetupReport, String> {
    super::agent::run_agent_setup(&app, &PathBuf::from(project), launch)
        .await
        .map_err(|e| e.to_string())
}

/// Executa UNA tasca de l'usuari contra l'editor: el model local tria les
/// eines MCP rellevants i les crida en un bucle fins a la resposta final.
///
/// L'error també s'emet com a esdeveniment (`unreal://agent-task`, fase
/// `error`): la UI el mostra al moment, sense dependre que la promesa
/// resolgui (si el webview es tanca, l'esdeveniment ja és fora).
#[command]
pub async fn unreal_agent_task(
    app: AppHandle,
    state: State<'_, AppState>,
    prompt: String,
) -> Result<String, String> {
    let model = state.ai_manager.effective_model().await;
    match super::agent::run_agent_task(&app, &prompt, Some(model)).await {
        Ok(answer) => Ok(answer),
        Err(e) => {
            let _ = app.emit(
                "unreal://agent-task",
                serde_json::json!({ "phase": "error", "text": e.to_string() }),
            );
            Err(e.to_string())
        }
    }
}

/// Catàleg d'eines MCP de l'editor (perquè la UI el pugui mostrar).
#[command]
pub async fn unreal_mcp_tools() -> Result<Vec<super::mcp::McpTool>, String> {
    let sess = super::mcp::open_session().await.map_err(|e| e.to_string())?;
    Ok(sess.tools)
}

/// Executa una eina MCP concreta (per depurar o per ordres puntuals).
#[command]
pub async fn unreal_mcp_call(
    name: String,
    args: serde_json::Value,
) -> Result<super::mcp::McpCallResult, String> {
    let sess = super::mcp::open_session().await.map_err(|e| e.to_string())?;
    sess.client
        .tools_call(&name, args)
        .await
        .map_err(|e| e.to_string())
}
