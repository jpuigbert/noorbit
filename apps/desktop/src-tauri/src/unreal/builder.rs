//! Compilació i empaquetat amb RunUAT / BuildGraph.

use super::launcher::runuat_binary;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    pub success: bool,
    pub output: String,
    pub output_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageOptions {
    pub project: String,
    pub platform: String,       // "Mac", "Win64", "Linux"
    pub configuration: String,  // "Development", "Shipping"
    pub output_dir: Option<String>,
}

/// Compila el projecte (BuildCookRun) amb RunUAT.
pub async fn package_game(
    engine_root: &PathBuf,
    opts: &PackageOptions,
    app: &tauri::AppHandle,
) -> Result<BuildResult> {
    use tauri::Emitter;

    let runuat = runuat_binary(engine_root)
        .ok_or_else(|| anyhow!("RunUAT no trobat a {:?}", engine_root))?;

    let output_dir = opts
        .output_dir
        .clone()
        .unwrap_or_else(|| {
            let project_dir = PathBuf::from(&opts.project)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".into());
            format!("{}/Build", project_dir)
        });

    let mut cmd = Command::new(&runuat);
    cmd.arg("BuildCookRun")
        .arg(format!("-project={}", opts.project))
        .arg(format!("-platform={}", opts.platform))
        .arg(format!("-clientconfig={}", opts.configuration))
        .arg("-cook")
        .arg("-build")
        .arg("-stage")
        .arg("-package")
        .arg("-archive")
        .arg(format!("-archivedirectory={}", output_dir))
        .arg("-nop4")
        .arg("-utf8output")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow!("No s'ha pogut arrencar RunUAT: {}", e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let all_output = std::sync::Arc::new(std::sync::Mutex::new(String::new()));

    if let Some(out) = stdout {
        let app_clone = app.clone();
        let store = all_output.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(out).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                {
                    let mut s = store.lock().unwrap();
                    s.push_str(&line);
                    s.push('\n');
                }
                let _ = app_clone.emit("unreal://build-output", format!("{}\n", line));
            }
        });
    }

    if let Some(err) = stderr {
        let app_clone = app.clone();
        let store = all_output.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(err).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                {
                    let mut s = store.lock().unwrap();
                    s.push_str(&format!("[err] {}\n", line));
                }
                let _ = app_clone.emit("unreal://build-output", format!("[err] {}\n", line));
            }
        });
    }

    let status = child.wait().await?;

    let final_output = {
        let guard = all_output.lock().unwrap();
        guard.clone()
    };

    Ok(BuildResult {
        success: status.success(),
        output: final_output,
        output_path: if status.success() { Some(output_dir) } else { None },
    })
}
