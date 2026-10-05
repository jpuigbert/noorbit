//! Detecció i arrencada d'Unreal Engine 5.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time::sleep;

const DEFAULT_RC_PORT: u16 = 30010;

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

    // Habilita Remote Control API i Python Editor Script
    cmd.arg("-EnablePlugins=PythonScriptPlugin,RemoteControl");

    // Script d'inicialització que obre el servidor HTTP de Remote Control
    if let Some(init_script) = init_script_path() {
        cmd.arg(format!("-ExecCmds=python {}", init_script.to_string_lossy()));
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

    launch_editor(&editor, project, &[]).await?;

    let ok = wait_for_rc(DEFAULT_RC_PORT, timeout).await?;
    if !ok {
        return Err(anyhow!(
            "Unreal Editor s'ha obert però la Remote Control API no ha respost en {} s.\n\
             Comprova que els plugins 'Python Editor Script' i 'Remote Control API' estiguin activats.",
            timeout
        ));
    }

    Ok(LaunchReport {
        engine_path: engine_root.to_string_lossy().to_string(),
        project_path: project.to_string_lossy().to_string(),
        launched: true,
        message: "Unreal Engine arrencat i connectat".into(),
    })
}
