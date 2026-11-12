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

/// Nom del binari segons el sistema operatiu.
pub fn exe_name() -> &'static str {
    if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

/// Carpeta on NoOrbit deixa la llama.cpp quan la instal·la ell mateix
/// (binaris oficials): així funciona fins i tot on Homebrew no té suport
/// (macOS vell) i on l'usuari no vol安装了 res a mà.
pub fn install_dir() -> Option<PathBuf> {
    // Si l'usuari ha triat un DISC EXTERN per al motor local, el binari hi
    // viu (i el model es pot servir des d'allà).
    if let Ok(cfg) = crate::config::AppConfig::load() {
        let d = cfg.ai.llamacpp_dir.trim();
        if !d.is_empty() {
            return Some(PathBuf::from(d));
        }
    }
    crate::config::AppConfig::data_dir().map(|d| d.join("llamacpp"))
}

/// Carpeta on es desen els models GGUF que fa servir el motor local (pot ser
/// un disc extern).
pub fn models_dir() -> PathBuf {
    crate::ai::hf_bay::models_dir()
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
    // La instal·lació feta per NoOrbit (binaris oficials): hi pot haver una
    // carpeta amb el tag dins (p. ex. «llama-b12318/llama-server»).
    if let Some(dir) = install_dir() {
        let exe = exe_name();
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = e.path().join(exe);
            if p.exists() {
                return Some(p);
            }
            // Pot ser un nivell més avall (l'arxiu traeu una carpeta) i a
            // Windows els DLL són al mateix lloc: el servidor el troba sol.
            for s in std::fs::read_dir(e.path()).into_iter().flatten().flatten() {
                let p = s.path().join(exe);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Nom de l'asset oficial de llama.cpp per a aquesta plataforma.
fn asset_guion() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        if cfg!(target_arch = "aarch64") {
            "bin-macos-arm64"
        } else {
            "bin-macos-x64"
        }
    }
    #[cfg(target_os = "linux")]
    {
        if cfg!(target_arch = "aarch64") {
            "bin-ubuntu-arm64"
        } else {
            "bin-ubuntu-x64"
        }
    }
    #[cfg(windows)]
    {
        if cfg!(target_arch = "aarch64") {
            "bin-win-cpu-arm64"
        } else {
            "bin-win-cpu-x64"
        }
    }
}

/// Instal·la la llama.cpp amb els BINARIS OFICIALS (GitHub releases): és la
/// via que funciona a tot arreu, també on Homebrew no té binaris (macOS vell)
/// i on l'usuari no té permisos d'instal·lar.
///
/// Retorna la ruta del «llama-server» instal·lat.
pub async fn install_from_github(
    http: &reqwest::Client,
    app: Option<&tauri::AppHandle>,
) -> Result<PathBuf> {
    use tauri::Emitter;
    let dir = install_dir().ok_or_else(|| anyhow!("No puc resoldre la carpeta de dades"))?;
    std::fs::create_dir_all(&dir)?;

    // 1) Última versió amb binaris per a la nostra plataforma.
    let releases = http
        .get("https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=5")
        .header("User-Agent", "NoOrbit")
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| anyhow!("No s'ha pogut contactar amb GitHub: {}", e))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| anyhow!("Resposta de GitHub ill·legible: {}", e))?;

    let mut url = None;
    let mut nom = String::new();
    if let Some(arr) = releases.as_array() {
        for r in arr {
            let Some(assets) = r.get("assets").and_then(|a| a.as_array()) else {
                continue;
            };
            for a in assets {
                let n = a.get("name").and_then(|v| v.as_str()).unwrap_or_default();
                if n.contains(asset_guion()) && (n.ends_with(".tar.gz") || n.ends_with(".zip")) {
                    url = a.get("browser_download_url").and_then(|v| v.as_str()).map(|s| s.to_string());
                    nom = n.to_string();
                    break;
                }
            }
            if url.is_some() {
                break;
            }
        }
    }
    let url = url.ok_or_else(|| {
        anyhow!(
            "No he trobat cap binari oficial de llama.cpp per a aquesta plataforma              ({})",
            asset_guion()
        )
    })?;

    if let Some(app) = app {
        let _ = app.emit(
            "ai://pull",
            serde_json::json!({ "model": "llama.cpp", "message": format!("descarregant {}…", nom) }),
        );
    }

    // 2) Descarrega l'arquiu.
    let arxiu = dir.join(&nom);
    let resp = http
        .get(&url)
        .header("User-Agent", "NoOrbit")
        .timeout(std::time::Duration::from_secs(600))
        .send()
        .await
        .map_err(|e| anyhow!("No s'ha pogut baixar {}: {}", nom, e))?;
    if !resp.status().is_success() {
        return Err(anyhow!("GitHub ha respost {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut out = std::fs::File::create(&arxiu)?;
    let mut stream = resp.bytes_stream();
    use futures::StreamExt;
    use std::io::Write;
    let mut written: u64 = 0;
    let mut last_pct: i64 = -1;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        out.write_all(&chunk)?;
        written += chunk.len() as u64;
        if total > 0 {
            let pct = (written * 100 / total) as i64;
            if pct != last_pct {
                last_pct = pct;
                if let Some(app) = app {
                    let _ = app.emit(
                        "ai://pull",
                        serde_json::json!({
                            "model": "llama.cpp",
                            "message": format!("descarregant llama.cpp — {}%", pct)
                        }),
                    );
                }
            }
        }
    }
    let _ = std::io::Write::flush(&mut out);
    drop(out);

    // 3) Extreu (tar.gz o zip). El «tar» del sistema està a tothom.
    let ok = if nom.ends_with(".zip") {
        #[cfg(windows)]
        {
            // A Windows no hi ha «unzip»: hi ha PowerShell.
            std::process::Command::new("powershell")
                .arg("-NoProfile")
                .arg("-Command")
                .arg(format!(
                    "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                    arxiu.to_string_lossy(),
                    dir.to_string_lossy()
                ))
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new("unzip")
                .arg("-o")
                .arg(&arxiu)
                .arg("-d")
                .arg(&dir)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
    } else {
        std::process::Command::new("tar")
            .arg("-xzf")
            .arg(&arxiu)
            .arg("-C")
            .arg(&dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    let _ = std::fs::remove_file(&arxiu);
    if !ok {
        return Err(anyhow!("No s'ha pogut descomprimir {}", nom));
    }

    // 4) Marca el binari com a executable.
    let bin = binary().ok_or_else(|| {
        anyhow!("He descomprimit {} però no hi trobo el «llama-server»", nom)
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755));
    }
    Ok(bin)
}

/// És instal·lada la llama.cpp?
pub fn installed() -> bool {
    binary().is_some()
}

/// Models GGUF ja baixats al disc (per poder-los engegar sense Ollama).
pub fn downloaded_models() -> Vec<(String, u64)> {
    let mut out = Vec::new();
    // La carpeta pot ser un DISC EXTERN: si no hi és muntat, no hi ha models
    // (i el model no se'l pot endur d'un lloc a un altre sense el disc).
    let dir = models_dir();
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

    // Engega el servidor. Preferim «--no-webui» (evita obrir el navegador),
    // però les versions antigues de llama.cpp no el coneixen: si el procés
    // mor tot seguit, ho tornem a provar sense ell.
    let engega = |amb_webui: bool| -> std::io::Result<std::process::Child> {
        let mut cmd = std::process::Command::new(&bin);
        cmd.arg("-m")
            .arg(&path)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("-c")
            .arg(ctx.to_string());
        if !amb_webui {
            cmd.arg("--no-webui");
        }
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW: engegar el servidor no obre cap consola
            // negra per davant de l'usuari.
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        cmd.spawn()
    };

    let mut child = engega(false)
        .map_err(|e| anyhow!("No s'ha pogut engegar llama-server: {}", e))?;
    // Si ha mort de seguida (opció no reconeguda), ho intentem sense ella.
    if let Ok(Some(_)) = child.try_wait() {
        child = engega(true)
            .map_err(|e| anyhow!("No s'ha pogut engegar llama-server: {}", e))?;
    }
    let pid = child.id();

    // Espera que l'API OpenAI-compatible respongui (fins a 3 minuts: en CPU
    // pot trigar a carregar el model).
    let url = format!("http://127.0.0.1:{}/v1/models", port);
    let mut model = String::new();
    for _ in 0..90 {
        if let Ok(resp) = http.get(&url).timeout(std::time::Duration::from_secs(3)).send().await {
            if resp.status().is_success() {
                if let Ok(v) = resp.json::<serde_json::Value>().await {
                    // Dos formats: «data:[{id}]» (OpenAI estàndard) i
                    // «models:[{name}]» (el que retorna llama-server avui).
                    let primer = v
                        .get("data")
                        .and_then(|d| d.as_array())
                        .and_then(|a| a.first())
                        .or_else(|| {
                            v.get("models")
                                .and_then(|d| d.as_array())
                                .and_then(|a| a.first())
                        });
                    if let Some(p) = primer {
                        let nom = p
                            .get("id")
                            .or_else(|| p.get("name"))
                            .and_then(|i| i.as_str())
                            .unwrap_or("model")
                            .to_string();
                        // El nom pot ser la ruta completa del GGUF: ens quedem
                        // amb el nom del fitxer (el model que veu l'usuari).
                        model = nom
                            .rsplit('/')
                            .next()
                            .unwrap_or(&nom)
                            .trim_end_matches(".gguf")
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

/// El procés del PID és realment el nostre `llama-server`? Els PIDs es
/// reciclen: sense aquesta comprovació, un PID desat podria pertànyer a un
/// altre procés (i el mataríem nosaltres, sense voler).
fn is_our_server(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/NH"])
            .output();
        match out {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout).to_lowercase();
                s.contains("llama-server")
            }
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        let out = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output();
        match out {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout).to_lowercase();
                s.contains("llama-server")
            }
            Err(_) => false,
        }
    }
}

/// Atura TOTS els servidors locals que NoOrbit hagi engegat. S'crida en
/// sortir de l'app: sense això, els `llama-server` queviuen com a processos
/// orfes (i el port quedava ocupat per a la sessió següent).
pub fn stop_all() {
    for s in servers() {
        let _ = stop(s.port);
    }
}

/// Neteja els registres de servidors ORFES (l'app va morir de cop i el
/// `llama-server` es va quedar viu, o al revés: el registre es va quedar però
/// el procés ja no hi és). Es crida en arrencar: si no, el port apareix
/// «ocupat» per un servei que ja no existeix i el motor local no arrenca.
pub fn cleanup_stale() {
    let mut list = servers();
    let before = list.len();
    list.retain(|s| is_our_server(s.pid));
    if list.len() != before {
        save_servers(&list);
    }
}

/// Atura un servidor local.
pub fn stop(port: u16) -> bool {
    let mut list = servers();
    let Some(s) = list.iter().position(|s| s.port == port) else {
        return false;
    };
    let srv = list.remove(s);
    // No matem un PID que ja no és el nostre (PID reciclat).
    if is_our_server(srv.pid) {
        #[cfg(windows)]
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &srv.pid.to_string(), "/F"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        #[cfg(not(windows))]
        let _ = std::process::Command::new("kill")
            .arg(srv.pid.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
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

#[cfg(test)]
mod proves {
    use super::*;

    /// El nom del binari ha de portar «.exe» a Windows (altrament el motor
    /// no s'en troba) i el guió de l'asset oficial ha de triar l'arquitectura
    /// correcta de la plataforma.
    #[test]
    fn noms_calculats_per_plataforma() {
        #[cfg(windows)]
        assert_eq!(exe_name(), "llama-server.exe");
        #[cfg(not(windows))]
        assert_eq!(exe_name(), "llama-server");

        let g = asset_guion();
        assert!(g.starts_with("bin-"), "guió d'asset inesperat: {}", g);
        #[cfg(target_os = "macos")]
        assert!(g.contains("macos"), "hauria de ser macOS: {}", g);
        #[cfg(target_os = "linux")]
        assert!(g.contains("ubuntu"), "hauria de ser ubuntu: {}", g);
        #[cfg(windows)]
        assert!(g.contains("win"), "hauria de ser windows: {}", g);
    }

    /// El nom del model que es veu ha de ser el NOM DEL FITXER, no la ruta
    /// completa del GGUF (que és el que retorna llama-server).
    #[test]
    fn el_nom_del_model_es_legible() {
        let brut = "/Users/jo/models/SmolLM2-135M-Q4_K_M.gguf";
        let net = brut.rsplit('/').next().unwrap().trim_end_matches(".gguf");
        assert_eq!(net, "SmolLM2-135M-Q4_K_M");
    }
}
