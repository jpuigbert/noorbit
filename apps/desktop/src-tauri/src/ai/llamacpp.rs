//! Motor local **sense Ollama**: `llama.cpp`.
//!
//! Ollama és còmode però no és necessari. `llama.cpp` (el projecte de GGUF)
//! té el binari `llama-server`, que exposa una API **compatible amb
//! OpenAI** (`/v1/chat/completions`). Amb ell es pot:
//!
//! 1. Descarregar un model GGUF (Hugging Bay / Hugging Face) sense Ollama.
//! 2. Engegar-lo en local amb `llama-server -m model.gguf`.
//! 3. Parlar-hi des del xat com un proveïdor més (registrat automàticament
//!    a «Proveïdors» com un servei local, sense cap clau).
//!
//! Tot és local: el model no surt de l'ordinador.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Port per defecte del servidor local (diferent del 11434 d'Ollama).
pub const DEFAULT_PORT: u16 = 11435;

/// Un servidor local en marxa (el que ha engegat NoOrbit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalServer {
    /// Port on escolta.
    pub port: u16,
    /// Nom del model exposat per l'API.
    pub model: String,
    /// Ruta del fitxer GGUF.
    pub path: String,
    /// PID del procés (per aturar-lo).
    pub pid: u32,
}

/// On es desen les dades dels servidors locals.
fn state_path() -> Option<PathBuf> {
    crate::config::AppConfig::data_dir().map(|d| d.join("llamacpp.json"))
}

/// Llista els servidors locals que NoOrbit ha engegat.
pub fn servers() -> Vec<LocalServer> {
    let Some(p) = state_path() else {
        return Vec::new();
    };
    let Ok(txt) = std::fs::read_to_string(p) else {
        return Vec::new();
    };
    serde_json::from_str(&txt).unwrap_or_default()
}

/// Desa la llista de servidors.
fn save_servers(list: &[LocalServer]) {
    if let Some(p) = state_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(txt) = serde_json::to_string_pretty(list) {
            let _ = std::fs::write(p, txt);
        }
    }
}

/// Binari `llama-server` (si l'usuari té instal·lada llama.cpp).
pub fn binary() -> Option<PathBuf> {
    if let Ok(p) = which::which("llama-server") {
        return Some(p);
    }
    let mut cands: Vec<PathBuf> = Vec::new();
    // Homebrew (Apple Silicon i Intel) i les rutes habituals.
    for d in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/home/linuxbrew/.linuxbrew/bin",
    ] {
        cands.push(PathBuf::from(d).join("llama-server"));
    }
    // Instal·lacions des del codi font.
    cands.push(PathBuf::from("/usr/bin/llama-server"));
    for c in cands {
        if c.exists() {
            return Some(c);
        }
    }
    None
}

/// És instal·lada la llama.cpp?
pub fn installed() -> bool {
    binary().is_some()
}

/// Models GGUF ja baixats al disc (per poder-los engegar sense Ollama).
pub fn downloaded_models() -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let Some(dir) = crate::config::AppConfig::data_dir().map(|d| d.join("models")) else {
        return out;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()).unwrap_or("").eq_ignore_ascii_case("gguf") {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            out.push((path.to_string_lossy().to_string(), size));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Engega `llama-server` amb un model GGUF i espera que l'API respongui.
///
/// Retorna el proveïdor creat (id, nom, base i model) perquè el xat l pugui
/// fer servir com un servei local més.
pub async fn start(
    http: &reqwest::Client,
    model_path: &str,
    port: u16,
    ctx: u32,
) -> Result<(String, String, String)> {
    let bin = binary().ok_or_else(|| {
        anyhow!(
            "No trobo «llama-server». Instal·la la llama.cpp: al macOS, «brew install \
             llama.cpp»; a Linux, «cmake -B build && cmake --build build --config Release»."
        )
    })?;
    let path = PathBuf::from(model_path);
    if !path.exists() {
        return Err(anyhow!("No trobo el model: {}", model_path));
    }
    // Si ja hi ha un servidor en aquest port, no engegem cap més.
    if let Some(s) = servers().into_iter().find(|s| s.port == port) {
        return Ok((
            format!("llamacpp_{}", s.port),
            s.model,
            format!("http://127.0.0.1:{}/v1", s.port),
        ));
    }

    let mut cmd = std::process::Command::new(&bin);
    cmd.arg("-m")
        .arg(&path)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("-c")
        .arg(ctx.to_string())
        .arg("--no-webui")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Procés independent: sobreviu al tancament de NoOrbit.
    let child = cmd
        .spawn()
        .map_err(|e| anyhow!("No s'ha pogut engegar llama-server: {}", e))?;
    let pid = child.id();

    // Espera que l'API OpenAI-compatible respongui (fins a 3 minuts: en CPU
    // pot trigar a carregar el model).
    let url = format!("http://127.0.0.1:{}/v1/models", port);
    let mut model = String::new();
    for _ in 0..90 {
        if let Ok(resp) = http.get(&url).timeout(std::time::Duration::from_secs(3)).send().await {
            if resp.status().is_success() {
                if let Ok(v) = resp.json::<serde_json::Value>().await {
                    if let Some(first) = v
                        .get("data")
                        .and_then(|d| d.as_array())
                        .and_then(|a| a.first())
                    {
                        model = first
                            .get("id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("model")
                            .to_string();
                    }
                }
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    if model.is_empty() {
        // El servidor potser ha arrencat igual: fem un intent final.
        model = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "model".to_string());
    }

    let mut list = servers();
    list.push(LocalServer {
        port,
        model: model.clone(),
        path: model_path.to_string(),
        pid,
    });
    save_servers(&list);

    Ok((
        format!("llamacpp_{}", port),
        format!("llama.cpp · {}", model),
        format!("http://127.0.0.1:{}/v1", port),
    ))
}

/// Atura un servidor local.
pub fn stop(port: u16) -> bool {
    let mut list = servers();
    let Some(s) = list.iter().position(|s| s.port == port) else {
        return false;
    };
    let srv = list.remove(s);
    let _ = std::process::Command::new("kill")
        .arg(srv.pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    save_servers(&list);
    true
}

/// El servidor encara viu? (per si l'ha matat l'usuari).
pub fn is_alive(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", port)
            .parse()
            .unwrap_or_else(|_| std::net::SocketAddr::from(([127, 0, 0, 1], 11435))),
        std::time::Duration::from_millis(400),
    )
    .is_ok()
}
