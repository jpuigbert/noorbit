//! Instal·lació autogestionada de DeepSeek Harness (dsh) i del seu
//! requisit, Node.js.
//!
//! Filosofia com la d'Ollama/ComfyUI: l'usuari prem un botó i NoOrbit ho
//! fa tot SOL, però SENSI demanar permisos d'administrador:
//! 1. Si no hi ha Node.js al sistema, en descarrega l'última versió LTS
//!    oficial (nodejs.org) i la desen DINS de la carpeta de dades de
//!    NoOrbit (al costat de la configuració; en mode portàtil, a
//!    NoOrbitData/, dins l'USB). No es toca res del sistema.
//! 2. Amb eixe Node es instal·la el paquet oficial `@deepseek-ai/dsh`
//!    (github.com/deepseek-ai/deepseek-harness) amb npm al mateix racó.
//! 3. A partir d'ací, la detecció (discovery), la delegació headless
//!    (adapter) i el navegador intern usen eixa instal·lació local o la del
//!    sistema si l'usuari ja la tenia.
//!
//! Cap executable del sistema no s'escriu ni s'esborra: si NoOrbit marxa,
//! la carpeta de dades (o l'USB) conserva Node + dsh per a sempre.

use anyhow::{anyhow, Result};
use std::path::PathBuf;

/// Carpeta arrel on viuen les instal·lacions locals (dins les dades de
/// NoOrbit: portàtil → NoOrbitData/, si no → ~/Library/Application Support
/// /no-orbit o l'equivalent de cada SO).
pub fn root_dir() -> Option<PathBuf> {
    crate::config::AppConfig::data_dir()
}

/// Carpeta del Node.js local descarregat per NoOrbit.
pub fn node_dir() -> Option<PathBuf> {
    root_dir().map(|d| d.join("node"))
}

/// Carpeta on npm instal·la el paquet @deepseek-ai/dsh.
pub fn dsh_dir() -> Option<PathBuf> {
    root_dir().map(|d| d.join("dsh"))
}

/// Bins habituals que un app gràfica (llançada des del Finder o l'escriptori)
/// NO sol tindre al PATH: els afegim per trobar-hi node/npm/dsh del sistema.
fn extra_path_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    #[cfg(unix)]
    {
        v.push(PathBuf::from("/opt/homebrew/bin")); // Homebrew (Apple Silicon)
        v.push(PathBuf::from("/usr/local/bin")); // Homebrew (Intel) / instal·ladors
        v.push(PathBuf::from("/usr/bin"));
        if let Some(home) = dirs::home_dir() {
            v.push(home.join(".local/bin"));
            v.push(home.join("n/bin"));
        }
    }
    #[cfg(windows)]
    {
        if let Some(p) = std::env::var_os("ProgramFiles") {
            v.push(std::path::Path::new(&p).join("nodejs"));
        }
    }
    // Les nostres pròpies carpetes locals (node i bin de dsh).
    if let Some(n) = node_dir() {
        #[cfg(windows)]
        v.push(n.clone());
        #[cfg(not(windows))]
        v.push(n.join("bin"));
    }
    if let Some(d) = dsh_dir() {
        v.push(d.join("node_modules").join(".bin"));
    }
    v
}

/// PATH augmentat per a llançar processos: l'actual + els bins coneguts.
/// Així les binaries amb shebang «#!/usr/bin/env node» també troben node.
pub fn augmented_path() -> std::ffi::OsString {
    let sep = if cfg!(windows) { ';' } else { ':' };
    let base = std::env::var_os("PATH").unwrap_or_default();
    let mut parts: Vec<String> = std::env::split_paths(&base)
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    for d in extra_path_dirs() {
        let s = d.to_string_lossy().into_owned();
        if !parts.iter().any(|p| p == &s) {
            parts.push(s);
        }
    }
    std::ffi::OsString::from(parts.join(&sep.to_string()))
}

/// Cerca un binari al PATH (augminat) — com `which`, però tbé als racons
/// que les apps gràfiques no veuen.
fn find_bin(name: &str) -> Option<PathBuf> {
    let path = augmented_path();
    which::which_in(name, Some(path), PathBuf::new()).ok()
}

/// Node.js disponible: el del sistema si n'hi ha, si no el local de NoOrbit.
pub fn node_binary() -> Option<PathBuf> {
    if let Some(p) = find_bin("node") {
        return Some(p);
    }
    let dir = node_dir()?;
    #[cfg(windows)]
    let cand = dir.join("node.exe");
    #[cfg(not(windows))]
    let cand = dir.join("bin").join("node");
    cand.exists().then_some(cand)
}

/// Binari `dsh` instal·lat pel sistema (npm global, brew…).
pub fn system_dsh() -> Option<PathBuf> {
    find_bin("dsh")
}

/// Binari `dsh` de la instal·lació LOCAL de NoOrbit.
pub fn local_dsh() -> Option<PathBuf> {
    let bin = dsh_dir()?.join("node_modules").join(".bin");
    #[cfg(windows)]
    let cand = bin.join("dsh.cmd");
    #[cfg(not(windows))]
    let cand = bin.join("dsh");
    cand.exists().then_some(cand)
}

/// El `dsh` a usar: primer el del sistema, després el local.
pub fn dsh_binary() -> Option<PathBuf> {
    system_dsh().or_else(local_dsh)
}

/// Hi ha un `dsh` funcional (del sistema o instal·lat per NoOrbit)?
pub fn dsh_installed() -> bool {
    dsh_binary().is_some()
}

/// Nom de l'arxiu oficial de Node segons SO/arquitectura (SO/destapats).
fn node_platform_file(version: &str) -> Option<String> {
    let arch = std::env::consts::ARCH; // "aarch64" | "x86_64"
    match (std::env::consts::OS, arch) {
        ("macos", "aarch64") => Some(format!("node-{version}-darwin-arm64.tar.gz")),
        ("macos", "x86_64") => Some(format!("node-{version}-darwin-x64.tar.gz")),
        ("linux", "x86_64") => Some(format!("node-{version}-linux-x64.tar.xz")),
        ("linux", "aarch64") => Some(format!("node-{version}-linux-arm64.tar.gz")),
        ("windows", "x86_64") => Some(format!("node-{version}-win-x64.zip")),
        ("windows", "aarch64") => Some(format!("node-{version}-win-arm64.zip")),
        _ => None,
    }
}

/// Última versió LTS oficial: el primer «lts» de l'índex de nodejs.org.
/// Si la xarxa falla, torna una LTS coneguda perquè la instal·lació no
/// quede bloquejada per un URLs caigudes (l'usuari pot reintentar).
async fn latest_lts_version(http: &reqwest::Client) -> String {
    const FALLBACK: &str = "v22.14.0";
    let Ok(resp) = http
        .get("https://nodejs.org/dist/index.json")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
    else {
        return FALLBACK.into();
    };
    let Ok(entries) = resp.json::<serde_json::Value>().await else {
        return FALLBACK.into();
    };
    entries
        .as_array()
        .and_then(|list| {
            list.iter()
                .find(|e| e.get("lts").map(|l| !l.is_null() && l.as_bool() != Some(false)).unwrap_or(false))
                .and_then(|e| e.get("version"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| FALLBACK.into())
}

/// Baixar i desempaquetar el Node LTS a la carpeta de dades de NoOrbit.
/// Tot dins del nostre espai: cap escritura al sistema, cap administrador.
async fn install_node<F: Fn(&str) + Send + Sync>(
    http: &reqwest::Client,
    on_progress: &F,
) -> Result<PathBuf> {
    let root = root_dir().ok_or_else(|| anyhow!("Sense carpeta de dades de NoOrbit"))?;
    let version = latest_lts_version(http).await;
    let file = node_platform_file(&version)
        .ok_or_else(|| anyhow!("Aquesta arquitectura no té Node.js oficial disponible"))?;
    let url = format!("https://nodejs.org/dist/{version}/{file}");
    on_progress(&format!("Baixant Node.js {version} (oficial, LTS)…"));
    let resp = http
        .get(&url)
        .timeout(std::time::Duration::from_secs(600))
        .send()
        .await
        .map_err(|e| anyhow!("No s'ha pogut baixar Node.js: {e}"))?;
    if !resp.status().is_success() {
        return Err(anyhow!("Node.js {}: HTTP {}", version, resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| anyhow!("Descàrrega de Node.js tallada: {e}"))?;
    let tmp = root.join("node-download");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| anyhow!("Sense carpeta temporal: {e}"))?;
    let archive = tmp.join(&file);
    std::fs::write(&archive, &bytes).map_err(|e| anyhow!("No s'ha pogut escriure l'arxiu: {e}"))?;

    on_progress("Desempquetant Node.js…");
    // El tarball crea una carpeta «node-vX-«plataforma»» eixamplada.
    let inner = file.trim_end_matches(".tar.gz").trim_end_matches(".tar.xz").trim_end_matches(".zip").to_string();
    let tmp_for_rm = tmp.clone(); // el clojure el mou: en guardem una còpia per netejar
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        #[cfg(windows)]
        {
            let status = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    "Expand-Archive",
                    "-LiteralPath",
                    &archive.to_string_lossy(),
                    "-DestinationPath",
                    &tmp.to_string_lossy(),
                ])
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() {
                Ok(())
            } else {
                Err("Expand-Archive ha fallat".to_string())
            }
        }
        #[cfg(not(windows))]
        {
            let status = std::process::Command::new("tar")
                .arg("-xf")
                .arg(&archive)
                .arg("-C")
                .arg(&tmp)
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() {
                Ok(())
            } else {
                Err("tar ha fallat desempquetant Node.js".to_string())
            }
        }
    })
    .await
    .map_err(|e| anyhow!("Error en el desempaquetat: {e}"))?
    .map_err(|e| anyhow!(e))?;

    // MOU (no copia: conserva els enllaços simbòlics interns de npm) al
    // destí final. Si ja n'hi havia un d'anterior, el substitueix.
    let extracted = tmp_for_rm.join(&inner);
    let dest = node_dir().ok_or_else(|| anyhow!("Sense carpeta de dades"))?;
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&extracted, &dest).map_err(|e| {
        anyhow!("No s'ha pogut col·locar Node.js a la carpeta de dades: {e}")
    })?;
    let _ = std::fs::remove_dir_all(&tmp_for_rm);
    Ok(dest)
}

/// npm (o npm-cli.js invocada pel nostre node) preparat per a ser executat.
/// Retorna (programa, args inicials).
fn npm_command() -> Result<(PathBuf, Vec<String>)> {
    let node = node_binary().ok_or_else(|| anyhow!("Node.js no disponible"))?;
    // Ruta de npm-cli.js segons on siga node:
    //  - local NoOrbit unix: <node_dir>/lib/node_modules/npm/bin/npm-cli.js
    //  - local NoOrbit win : <node_dir>/node_modules/npm/bin/npm-cli.js
    //  - sistema           : npm al mateix directori que node.
    if let Some(dir) = node_dir() {
        let is_local = node.starts_with(&dir);
        if is_local {
            let cli = if cfg!(windows) {
                dir.join("node_modules")
                    .join("npm")
                    .join("bin")
                    .join("npm-cli.js")
            } else {
                dir.join("lib")
                    .join("node_modules")
                    .join("npm")
                    .join("bin")
                    .join("npm-cli.js")
            };
            if !cli.exists() {
                return Err(anyhow!("npm no és dins el Node.js local de NoOrbit"));
            }
            return Ok((node, vec![cli.to_string_lossy().into_owned()]));
        }
    }
    let npm_name = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let npm = find_bin(npm_name).or_else(|| node.parent().map(|p| p.join(npm_name)).filter(|p| p.exists()))
        .ok_or_else(|| anyhow!("No trobe npm al costat de node"))?;
    Ok((npm, vec![]))
}

/// Instal·la Node.js (si falta) + @deepseek-ai/dsh, tot dins de les dades
/// de NoOrbit. `on_progress` s'usa per a informar la UI pas a pas.
pub async fn install(app: tauri::AppHandle) -> Result<String> {
    let report = {
        let app = app.clone();
        move |detail: &str| {
            use tauri::Emitter;
            let _ = app.emit(
                "agent://progress",
                serde_json::json!({
                    "label": "DeepSeek Harness",
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
    if node_binary().is_none() {
        install_node(&http, &report).await?;
    } else {
        report("Node.js ja és al sistema — el reuse per a instal·lar dsh.");
    }

    // 2) npm install @deepseek-ai/dsh --prefix <dades>/dsh
    report("Instal·lando DeepSeek Harness (@deepseek-ai/dsh)…");
    let prefix = dsh_dir().ok_or_else(|| anyhow!("Sense carpeta de dades"))?;
    std::fs::create_dir_all(&prefix).map_err(|e| anyhow!("No es pot crear la carpeta dsh: {e}"))?;
    let (prog, mut args) = npm_command()?;
    args.extend([
        "install".to_string(),
        "@deepseek-ai/dsh".to_string(),
        "--prefix".to_string(),
        prefix.to_string_lossy().into_owned(),
        "--no-audit".to_string(),
        "--no-fund".to_string(),
    ]);
    let path_env = augmented_path();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let out = std::process::Command::new(&prog)
            .args(&args)
            .env("PATH", &path_env)
            .current_dir(&prefix)
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
    if !dsh_installed() {
        return Err(anyhow!(
            "La instal·lació ha acabat però no trobe el binari dsh"
        ));
    }
    report("DeepSeek Harness instal·lat i llest.");
    Ok("DeepSeek Harness (dsh) s'ha instal·lat amb el seu Node.js dins de les dades de NoOrbit: ja el pots usar (delegació headless, detecció com a IA local externa i interfície al port 3080).".into())
}

/// Construeix el comando `dsh …` apuntant al binari trobat (sistema o
/// local), amb el PATH augmentat perquè el seu shebang trobe node.
pub fn command(args: &[&str]) -> Option<std::process::Command> {
    let bin = dsh_binary()?;
    let mut cmd = if cfg!(windows) && bin.extension().map(|e| e == "cmd").unwrap_or(false) {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(&bin);
        c
    } else {
        std::process::Command::new(&bin)
    };
    cmd.args(args).env("PATH", augmented_path());
    Some(cmd)
}

/// Executa `dsh` i n'espera la sortida (per al mode headless de l'adapter).
pub fn run_blocking(args: &[&str]) -> Result<std::process::Output> {
    let mut cmd = command(args).ok_or_else(|| {
        anyhow!(
            "dsh no està instal·lat — prem «Instal·la DeepSeek Harness» al Gestor \
             de proveïdors (instal·la també Node.js automàticament, sense permisos \
             d'administrador)"
        )
    })?;
    // La comanda ja porta el PATH augmentat: node del sistema o el nostre
    // local, perquè el shebang «#!/usr/bin/env node» del binari funcione.
    cmd.output()
        .map_err(|e| anyhow!("No s'ha pogut llançar dsh: {e}"))
}

/// Arrenca la interfície web de dsh («dsh web») en segon pla, desconnectada,
/// perquè l'usuari la configuri (models, claus) i la faça servir.
pub fn start_web() -> Result<()> {
    let mut cmd = command(&["web"]).ok_or_else(|| anyhow!("dsh no és instal·lat"))?;
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| anyhow!("No s'ha pogut arrencar «dsh web»: {e}"))?;
    Ok(())
}
