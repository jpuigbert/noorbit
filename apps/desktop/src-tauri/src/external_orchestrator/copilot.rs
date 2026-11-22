//! Detecció i instal·lació autogestionada del CLI de GitHub Copilot
//! (paquet npm `@github/copilot`), que és el motor que parla ACP.
//!
//! Mateixa filosofia que el DeepSeek Harness (dsh): si l'usuari ja el té
//! (instal·lació global de npm, Homebrew…), el fem servir tal qual; si no,
//! NoOrbit l'instal·la DINS de les seves pròpies dades — sense permisos
//! d'administrador i sense tocar res del sistema, de manera que en mode
//! portàtil (USB) l'agent viatja amb l'aplicació.

use anyhow::{anyhow, Result};
use std::path::PathBuf;

use super::dsh;

/// Carpeta on npm instal·la el paquet `@github/copilot`.
pub fn copilot_dir() -> Option<PathBuf> {
    dsh::root_dir().map(|d| d.join("copilot"))
}

/// Binari `copilot` del sistema (npm global, Homebrew…).
pub fn system_copilot() -> Option<PathBuf> {
    dsh::find_bin("copilot")
}

/// Binari `copilot` de la instal·lació LOCAL de NoOrbit.
pub fn local_copilot() -> Option<PathBuf> {
    let bin = copilot_dir()?.join("node_modules").join(".bin");
    #[cfg(windows)]
    let cand = bin.join("copilot.cmd");
    #[cfg(not(windows))]
    let cand = bin.join("copilot");
    cand.exists().then_some(cand)
}

/// El `copilot` a fer servir: primer el del sistema, després el local.
pub fn copilot_binary() -> Option<PathBuf> {
    system_copilot().or_else(local_copilot)
}

/// Hi ha un `copilot` funcional (del sistema o instal·lat per NoOrbit)?
pub fn copilot_installed() -> bool {
    copilot_binary().is_some()
}

/// Versió del CLI instal·lat (si n'hi ha).
pub fn version() -> Option<String> {
    let bin = copilot_binary()?;
    let out = std::process::Command::new(&bin)
        .arg("--version")
        .env("PATH", dsh::augmented_path())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let txt = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!txt.is_empty()).then_some(txt)
}

/// Comando `copilot --acp --stdio`: el client ACP parla amb l'agent per
/// l'entrada i la sortida estàndard d'aquest procés.
pub fn acp_command() -> Result<std::process::Command> {
    let bin = copilot_binary().ok_or_else(|| {
        anyhow!(
            "El CLI de GitHub Copilot no està instal·lat. Prem «Instal·la GitHub \
             Copilot» al Gestor de proveïdors (s'instal·la sol dins les dades de \
             NoOrbit, sense permisos d'administrador)."
        )
    })?;
    let mut cmd = if cfg!(windows) && bin.extension().map(|e| e == "cmd").unwrap_or(false) {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(&bin);
        c
    } else {
        std::process::Command::new(&bin)
    };
    cmd.args(["--acp", "--stdio"])
        .env("PATH", dsh::augmented_path());
    Ok(cmd)
}

/// Instal·la el CLI de GitHub Copilot tot dins de les dades de NoOrbit:
/// si no hi ha Node.js al sistema, en baixa l'LTS oficial (com fa amb dsh),
/// i després fa `npm install @github/copilot` al mateix racó.
pub async fn install(app: tauri::AppHandle) -> Result<String> {
    let report = {
        let app = app.clone();
        move |detail: &str| {
            use tauri::Emitter;
            let _ = app.emit(
                "agent://progress",
                serde_json::json!({
                    "label": "GitHub Copilot",
                    "detail": detail,
                    "ok": true,
                }),
            );
        }
    };
    let http = reqwest::Client::builder()
        .user_agent("NoOrbit")
        .build()
        .unwrap_or_default();

    // 1) Node.js: si ja n'hi ha (sistema o local), no el tornem a baixar.
    if dsh::node_binary().is_none() {
        dsh::install_node(&http, &report).await?;
    } else {
        report("Node.js ja és al sistema — el reuse per a instal·lar Copilot.");
    }

    // 2) npm install @github/copilot --prefix <dades>/copilot
    report("Instal·lant el CLI de GitHub Copilot (@github/copilot)…");
    let prefix = copilot_dir().ok_or_else(|| anyhow!("Sense carpeta de dades de NoOrbit"))?;
    std::fs::create_dir_all(&prefix)
        .map_err(|e| anyhow!("No es pot crear la carpeta de Copilot: {e}"))?;
    let (prog, mut args) = dsh::npm_command()?;
    args.extend([
        "install".to_string(),
        "@github/copilot".to_string(),
        "--prefix".to_string(),
        prefix.to_string_lossy().into_owned(),
        "--no-audit".to_string(),
        "--no-fund".to_string(),
    ]);
    let path_env = dsh::augmented_path();
    let prefix_for_run = prefix.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let out = std::process::Command::new(&prog)
            .args(&args)
            .env("PATH", &path_env)
            .current_dir(&prefix_for_run)
            .output()
            .map_err(|e| format!("No s'ha pogut executar npm: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!(
                "npm ha fallat: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    })
    .await
    .map_err(|e| anyhow!("Error en la instal·lació: {e}"))?
    .map_err(|e| anyhow!(e))?;
    if !copilot_installed() {
        return Err(anyhow!(
            "La instal·lació ha acabat però no trobe el binari de Copilot"
        ));
    }
    report("GitHub Copilot instal·lat i llest.");
    Ok("El CLI de GitHub Copilot ja és instal·lat dins de les dades de NoOrbit (amb el seu propi Node.js si al sistema no n'hi havia). Pots triar-lo com a motor del xat. Si el CLI demana entrar, fes «copilot login» una vegada des d'una terminal.".into())
}
