//! Detecció i arrencada d'Unreal Engine 5.

use super::client::UnrealClient;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time::sleep;

const DEFAULT_RC_PORT: u16 = crate::ports::UNREAL_RC_PORT;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchReport {
    pub engine_path: String,
    pub project_path: String,
    pub launched: bool,
    pub message: String,
}

/// Comprova si el port de la Remote Control API està obert.
pub async fn is_rc_open(port: u16) -> bool {
    tokio::time::timeout(
        Duration::from_millis(500),
        TcpStream::connect(format!("127.0.0.1:{}", port)),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false)
}

/// Cerca instal·lacions d'Unreal Engine al sistema.
pub fn detect_engine_installations() -> Vec<PathBuf> {
    let mut found = Vec::new();

    if cfg!(target_os = "windows") {
        // C:\Program Files\Epic Games\UE_5.x
        let base = PathBuf::from(r"C:\Program Files\Epic Games");
        if base.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&base) {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with("UE_") {
                        found.push(e.path());
                    }
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        // /Users/Shared/Epic Games/UE_5.x
        let base = PathBuf::from("/Users/Shared/Epic Games");
        if base.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&base) {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with("UE_") {
                        found.push(e.path());
                    }
                }
            }
        }
        // /Applications/Epic Games/UE_5.x
        let base2 = PathBuf::from("/Applications/Epic Games");
        if base2.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&base2) {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with("UE_") {
                        found.push(e.path());
                    }
                }
            }
        }
    } else {
        // Linux: /opt/UnrealEngine o ~/UnrealEngine
        if let Some(home) = dirs::home_dir() {
            let p = home.join("UnrealEngine");
            if p.is_dir() {
                found.push(p);
            }
        }
        if Path::new("/opt/UnrealEngine").is_dir() {
            found.push(PathBuf::from("/opt/UnrealEngine"));
        }
    }

    found.sort();
    found.reverse(); // Les més noves primer
    found
}

/// Retorna el binari de l'editor per a una instal·lació.
pub fn editor_binary(engine_root: &Path) -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        let p = engine_root
            .join("Engine")
            .join("Binaries")
            .join("Win64")
            .join("UnrealEditor.exe");
        if p.exists() { return Some(p); }
    } else if cfg!(target_os = "macos") {
        let p = engine_root
            .join("Engine")
            .join("Binaries")
            .join("Mac")
            .join("UnrealEditor.app")
            .join("Contents")
            .join("MacOS")
            .join("UnrealEditor");
        if p.exists() { return Some(p); }
    } else {
        let p = engine_root
            .join("Engine")
            .join("Binaries")
            .join("Linux")
            .join("UnrealEditor");
        if p.exists() { return Some(p); }
    }
    None
}

/// Assegura que el projecte té els plugins que necessita NoOrbit
/// (PythonScriptPlugin + RemoteControl) ACTIVATS al fitxer `.uproject`.
///
/// Sense això, molts projectes recents (i tots els creats sense marcar-los)
/// arriben amb el servidor Remote Control apagat: l'editor s'obre però el
/// port 30010 no respon mai, i és la causa més freqüent de «no connecta».
/// Retorna `true` si ha HAGUT de modificar el fitxer.
pub fn ensure_project_plugins(uproject: &Path) -> Result<bool> {
    let raw = std::fs::read_to_string(uproject)
        .map_err(|e| anyhow!("No puc llegir el .uproject: {}", e))?;
    let mut v: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| anyhow!("El .uproject no és JSON vàlid: {}", e))?;

    let required = ["PythonScriptPlugin", "RemoteControl"];
    let plugins = v
        .as_object_mut()
        .ok_or_else(|| anyhow!("El .uproject no és un objecte"))?
        .entry("Plugins")
        .or_insert_with(|| serde_json::json!([]));
    if !plugins.is_array() {
        *plugins = serde_json::json!([]);
    }

    let mut changed = false;
    for name in required {
        let already = plugins
            .as_array()
            .map(|a| {
                a.iter().any(|p| {
                    p.get("Name").and_then(|n| n.as_str()) == Some(name)
                        && p.get("Enabled").and_then(|e| e.as_bool()) == Some(true)
                })
            })
            .unwrap_or(false);
        if !already {
            plugins
                .as_array_mut()
                .ok_or_else(|| anyhow!("Plugins no és una llista"))?
                .push(serde_json::json!({ "Name": name, "Enabled": true }));
            changed = true;
        }
    }

    if changed {
        // Còpia de seguretat d'una sola escriptura (la primera vegada que
        // toquem el fitxer): l'usuari pot tornar-ho endarrere a mà.
        let backup = uproject.with_extension("uproject.bak");
        if !backup.exists() {
            let _ = std::fs::copy(uproject, &backup);
        }
        let pretty = serde_json::to_string_pretty(&v)
            .map_err(|e| anyhow!("No puc tornar a escriure el .uproject: {}", e))?;
        std::fs::write(uproject, pretty)
            .map_err(|e| anyhow!("No puc escriure el .uproject: {}", e))?;
    }
    Ok(changed)
}

/// Retorna el binari de RunUAT per a una instal·lació.
pub fn runuat_binary(engine_root: &Path) -> Option<PathBuf> {
    let batch = engine_root
        .join("Engine")
        .join("Build")
        .join("BatchFiles");

    if cfg!(target_os = "windows") {
        let p = batch.join("RunUAT.bat");
        if p.exists() { return Some(p); }
    } else {
        let p = batch.join("RunUAT.sh");
        if p.exists() { return Some(p); }
    }
    None
}

/// Llança l'editor d'Unreal amb un projecte `.uproject`.
pub async fn launch_editor(
    editor: &Path,
    project: &Path,
    extra_args: &[String],
) -> Result<()> {
    let mut cmd = Command::new(editor);
    cmd.arg(project.to_string_lossy().to_string());

    for arg in extra_args {
        cmd.arg(arg);
    }

    // El port CONFIGURAT (Configuració ▸ Unreal) s'injecta per entorn: el
    // bootstrap.py el llegeix i obri AQUELL port. Abans el Python sempre
    // obria el 30010 i la configuració era ignorada.
    let rc_port = crate::config::AppConfig::load()
        .ok()
        .map(|c| c.unreal.rc_port)
        .filter(|p| *p != 0)
        .unwrap_or(crate::ports::UNREAL_RC_PORT);
    cmd.env(crate::ports::ENV_UNREAL_PORT, rc_port.to_string());

    // Habilita Remote Control API i Python Editor Script (per si el .uproject
    // no ha pogut ser parchejat; l'un no treu l'altre).
    cmd.arg("-EnablePlugins=PythonScriptPlugin,RemoteControl");

    // Script d'inicialització que obre el servidor HTTP de Remote Control.
    // EXECUTEPYTHONSCRIPT és el mètode FIABLE: s'executa un cop el motor
    // inicialitzat i admet rutes amb espais. L'antic «-ExecCmds=python <ruta>»
    // es trencava amb rutes amb espais («/Users/Shared/Epic Games/…») i
    // s'executava abans que el plugin Python estigués llest.
    if let Some(init_script) = ensure_bootstrap_script() {
        cmd.arg(format!(
            "-ExecutePythonScript={}",
            init_script.to_string_lossy()
        ));
    }

    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    cmd.spawn()
        .map_err(|e| anyhow!("No s'ha pogut arrencar Unreal Editor: {}", e))?;

    Ok(())
}

/// Ruta del script Python d'arrencada (unreal-scripts/bootstrap.py).
fn init_script_path() -> Option<PathBuf> {
    // 1) costat de l'executable (bundle resources)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("unreal-scripts").join("bootstrap.py");
            if p.exists() {
                return Some(p);
            }
            // .app de macOS: Contents/MacOS/<exe> -> Contents/Resources/unreal-scripts
            if let Some(contents) = dir.parent() {
                let p = contents
                    .join("Resources")
                    .join("unreal-scripts")
                    .join("bootstrap.py");
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    // 2) carpeta de treball (en desenvolupament)
    let p = PathBuf::from("unreal-scripts").join("bootstrap.py");
    if p.exists() {
        return Some(p);
    }
    None
}

/// Bootstrap d'emergència: si el bundle no porta unreal-scripts (execucions
/// en desenvolupament), l'ESCRIU a la carpeta temporal del sistema perquè
/// `-ExecutePythonScript` sempre tingui un fitxer vàlid que obrir el RC.
const BOOTSTRAP_FALLBACK: &str = r#"import unreal
PORT = 30010
def _log(m): unreal.log("[NoOrbit/bootstrap] " + m)
try:
    ctx = unreal.EditorLevelLibrary.get_editor_world()
except Exception:
    ctx = None
for cmd in ("RemoteControl.EnableServer", "rc.EnableServer 1",
            "RemoteControl.SetHttpPort {}".format(PORT)):
    try:
        unreal.SystemLibrary.execute_console_command(ctx, cmd)
    except Exception:
        pass
_log("Remote Control demanat al port {}".format(PORT))
"#;

pub fn ensure_bootstrap_script() -> Option<PathBuf> {
    if let Some(p) = init_script_path() {
        return Some(p);
    }
    let dir = std::env::temp_dir().join("noorbit");
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    let p = dir.join("bootstrap.py");
    std::fs::write(&p, BOOTSTRAP_FALLBACK).ok()?;
    Some(p)
}

/// Espera que la Remote Control API estigui disponible.
pub async fn wait_for_rc(port: u16, timeout_secs: u64) -> Result<bool> {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    while std::time::Instant::now() < deadline {
        if is_rc_open(port).await {
            sleep(Duration::from_millis(800)).await;
            return Ok(true);
        }
        sleep(Duration::from_millis(1000)).await;
    }
    Ok(false)
}

/// Flux complet: detecta, arrenca si cal, espera, retorna estat.
pub async fn auto_connect(project: &Path, timeout: u64) -> Result<LaunchReport> {
    if is_rc_open(DEFAULT_RC_PORT).await {
        return Ok(LaunchReport {
            engine_path: String::new(),
            project_path: project.to_string_lossy().to_string(),
            launched: false,
            message: "Unreal Editor ja estava obert".into(),
        });
    }

    let installs = detect_engine_installations();
    if installs.is_empty() {
        return Err(anyhow!(
            "Unreal Engine 5 no està instal·lat.\n\
             Descarrega'l des de https://www.unrealengine.com/download"
        ));
    }

    let engine_root = &installs[0];
    let editor = editor_binary(engine_root)
        .ok_or_else(|| anyhow!("No s'ha trobat el binari de l'editor a {:?}", engine_root))?;

    if !project.exists() {
        return Err(anyhow!(
            "El fitxer de projecte '{}' no existeix",
            project.display()
        ));
    }

    // ACTIVA els plugins que necessitem al .uproject ABANS de dur el projecte
    // a l'editor: sense PythonScriptPlugin+RemoteControl el port 30010 no
    // respondria mai i l'espera acabaria en temps exhaurit.
    let patched = ensure_project_plugins(project)?;

    launch_editor(&editor, project, &[]).await?;

    let ok = wait_for_rc(DEFAULT_RC_PORT, timeout).await?;
    if !ok {
        return Err(anyhow!(
            "Unreal Editor s'ha obert però la Remote Control API no ha respost en {} s.\n\
             Els plugins «Python Editor Script» i «Remote Control API» ja estan activats \
             al projecte; si no connecta, obre l'editor amb el mateix projecte i activa \
             «Window → Virtual Production → Remote Control».\n\
             També pots tornar-ho a provar (la primera arrencada pot trigar més: \
             compilació de shaders).",
            timeout
        ));
    }

    // Assegura els dos servidors de Remote Control (HTTP ja respon; el WS és
    // el que la intefície web de RC fara servir més endavant). Best-effort:
    // si no es pot, el HTTP que ja hi ha n'és suficient per treballar.
    let client = UnrealClient::with_port(DEFAULT_RC_PORT);
    let _ = super::remote_control::start_websocket(&client, 30020).await;
    let _ = super::remote_control::start_http(&client, DEFAULT_RC_PORT).await;

    Ok(LaunchReport {
        engine_path: engine_root.to_string_lossy().to_string(),
        project_path: project.to_string_lossy().to_string(),
        launched: true,
        message: if patched {
            "Unreal Engine arrencat i connectat (plugins Python+RemoteControl activats al projecte)".into()
        } else {
            "Unreal Engine arrencat i connectat".into()
        },
    })
}
