//! Mòdul LSP: client mínim del Language Server Protocol sobre «stdio».
//!
//! NoOrbit no inclou cap servidor de llenguatge (serien gegants: rust-analyzer
//! fa ~120 MB). El que fem és **detectar-ne** un d'instal·lat al sistema i, si
//! hi és, parlar-hi el protocol estàndard per resoldre «Ves a la definició» amb
//! precisió real. Si no n'hi ha cap, o bé no respon, el cridador cau en la
//! heurística de text pla (`commands::fs::find_definition`). Així aprofitem el
//! mateix motor que fa VS Code quan existeix, sense obligar a instal·lar res.
//!
//! Implementació: JSON-RPC amb capçalera `Content-Length`, procés fill amb
//! stdin/stdout pipejats, i un fil lector que lliura els missatges per canal.
//! Totes les esperen tenen temps límit perquè un servidor penjat no bloquegi
//! mai la interfície.

use serde::Serialize;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, Serialize)]
pub struct LspServer {
    pub language: String,
    pub binary: String,
    pub installed: bool,
}

/// Llista els servidors LSP coneguts i si són al PATH.
pub fn known_servers() -> Vec<LspServer> {
    [
        ("rust", "rust-analyzer"),
        ("typescript", "typescript-language-server"),
        ("python", "pylsp"),
        ("c", "clangd"),
    ]
    .iter()
    .map(|(lang, bin)| LspServer {
        language: lang.to_string(),
        binary: bin.to_string(),
        installed: which::which(bin).is_ok(),
    })
    .collect()
}

/// (identificador de llenguatge per a LSP, binari del servidor, args d'arrencada)
/// segons l'extensió del fitxer.
fn server_for(ext: &str) -> Option<(&'static str, &'static str, &'static [&'static str])> {
    match ext {
        "rs" => Some(("rust", "rust-analyzer", &[])),
        "ts" => Some(("typescript", "typescript-language-server", &["--stdio"])),
        "tsx" => Some(("typescriptreact", "typescript-language-server", &["--stdio"])),
        "js" | "jsx" | "mjs" | "cjs" => {
            Some(("javascript", "typescript-language-server", &["--stdio"]))
        }
        "py" | "pyi" => Some(("python", "pylsp", &[])),
        "c" | "h" => Some(("c", "clangd", &[])),
        "cpp" | "cc" | "cxx" | "hpp" => Some(("cpp", "clangd", &[])),
        _ => None,
    }
}

/// Una localització (fitxer + línia/columna, 1-based) dins del projecte.
pub struct DefLoc {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub preview: String,
}

/// Converteix un camí absolut en un «file://» URI mínim (escape d'espais).
fn path_to_uri(p: &Path) -> String {
    let s = p.to_string_lossy().replace(' ', "%20");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        // Windows: C:\… → file:///c%3A/…
        format!("file:///{s}")
    }
}

/// Inverteix `path_to_uri`: lletra l'esquema i decodifica els escapes bàsics.
fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let decoded = rest.replace("%20", " ");
    // En Windows l'URI porta una barra inicial de més (file:///C:/…).
    let cleaned = if let Some(stripped) = decoded.strip_prefix('/') {
        if stripped.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false)
            && stripped.get(1..3) == Some(":")
        {
            stripped.to_string()
        } else {
            decoded.clone()
        }
    } else {
        decoded.clone()
    };
    Some(PathBuf::from(cleaned))
}

/// Escriu un missatge JSON-RPC amb la seva capçalera de longitud.
fn send_msg(stdin: &mut std::process::ChildStdin, value: &serde_json::Value) -> Result<(), String> {
    let body = serde_json::to_string(value).map_err(|e| e.to_string())?;
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).map_err(|e| e.to_string())?;
    stdin.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
    stdin.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Fil lector: desencaixa missatges «Content-Length» i els envia pel canal.
fn reader_loop(mut r: BufReader<ChildStdout>, tx: mpsc::Sender<serde_json::Value>) {
    loop {
        // Capçalteres fins a la línia en blanc que obri el cos.
        let mut content_len: Option<usize> = None;
        loop {
            let mut line = String::new();
            match r.read_line(&mut line) {
                Ok(0) | Err(_) => return, // EOF o error: el servidor ha mort
                Ok(_) => {}
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break;
            }
            if let Some(v) = trimmed
                .to_ascii_lowercase()
                .strip_prefix("content-length:")
            {
                if let Ok(n) = v.trim().parse::<usize>() {
                    content_len = Some(n);
                }
            }
        }
        let n = match content_len {
            Some(n) => n,
            None => continue,
        };
        let mut buf = vec![0u8; n];
        if r.read_exact(&mut buf).is_err() {
            return;
        }
        if let Ok(s) = String::from_utf8(buf) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                let _ = tx.send(v);
            }
        }
    }
}

/// Espera una resposta amb aquest `id`, ignorant notificacions i altres
/// missatges. Isca per temps límit o perquè el canal s'ha tancat.
fn wait_for_id(
    rx: &mpsc::Receiver<serde_json::Value>,
    id: i64,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err("el servidor LSP no ha respost a temps".into());
        }
        match rx.recv_timeout(deadline - now) {
            Ok(msg) => {
                if msg.get("id").and_then(|v| v.as_i64()) == Some(id) {
                    return Ok(msg);
                }
                // Notificacions (p. ex. publishDiagnostics) i altres ids: fora.
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err("el servidor LSP no ha respost a temps".into());
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("el servidor LSP s'ha tancat".into());
            }
        }
    }
}

/// Consulta la definició al servidor LSP corresponent al tipus de fitxer.
///
/// `abs_path` és la ruta absoluta del fitxer obert; `line`/`column` són 1-based
/// (com els de Monaco). Retorna la ubicació de la definició en termes relatius
/// a `root` perquè la UI la carregue igual que la versió heurística.
pub fn definition(
    root: &Path,
    abs_path: &Path,
    line: usize,
    column: usize,
) -> Result<DefLoc, String> {
    let ext = abs_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (lang_id, binary, args) = server_for(&ext).ok_or("tipus de fitxer sense servidor LSP")?;
    let bin_path = which::which(binary).map_err(|_| format!("«{binary}» no és al PATH"))?;

    let text = std::fs::read_to_string(abs_path).map_err(|e| e.to_string())?;
    let doc_uri = path_to_uri(abs_path);
    let root_uri = path_to_uri(root);

    #[allow(unused_mut)]
    let mut cmd = Command::new(bin_path);
    cmd.args(args)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let mut child = cmd.spawn().map_err(|e| format!("no s'ha pogut arrencar {binary}: {e}"))?;

    let stdin = child.stdin.take().ok_or("sense stdin per al servidor")?;
    let stdout = child.stdout.take().ok_or("sense stdout per al servidor")?;
    let (tx, rx) = mpsc::channel::<serde_json::Value>();
    let reader = std::thread::spawn(move || reader_loop(BufReader::new(stdout), tx));

    let result = run_session(stdin, &rx, &root_uri, &doc_uri, lang_id, &text, line, column);

    // `run_session` ja ha descartat el seu `stdin` (es tanca el conducte i el
    // servidor rep EOF). Asegurem la mort del procés i recollim el fil lector.
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();

    let resp = result?;
    parse_definition(root, &resp).and_then(|(uri, line0, char0)| {
        let abs = uri_to_path(uri).ok_or("URI de destinació invàlida")?;
        let rel = abs
            .strip_prefix(root)
            .map(|x| x.to_string_lossy().trim_start_matches('/').to_string())
            .unwrap_or_else(|_| abs.display().to_string());
        let line1 = line0 + 1;
        let col1 = char0 + 1;
        let preview = std::fs::read_to_string(&abs)
            .ok()
            .and_then(|t| {
                t.lines().nth(line0).map(|l| {
                    let l = l.trim();
                    if l.chars().count() > 160 {
                        l.chars().take(160).collect::<String>() + "…"
                    } else {
                        l.to_string()
                    }
                })
            })
            .unwrap_or_default();
        Ok(DefLoc {
            path: rel,
            line: line1,
            column: col1,
            preview,
        })
    })
}

/// Executa el intercanvi: initialize → initialized → didOpen → definition.
/// Deixa el stdin (`stdin`) al titular perquè el cridador puga tancar després.
fn run_session(
    mut stdin: std::process::ChildStdin,
    rx: &mpsc::Receiver<serde_json::Value>,
    root_uri: &str,
    doc_uri: &str,
    lang_id: &str,
    text: &str,
    line: usize,
    column: usize,
) -> Result<serde_json::Value, String> {
    // 1) initialize
    send_msg(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": null,
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "definition": { "linkSupport": true }
                    },
                    "workspace": { "applyEdit": true }
                }
            }
        }),
    )?;
    // rust-analyzer/pylsp poden trigar a arrencar; donem marge.
    wait_for_id(rx, 1, Duration::from_secs(15))?;

    // 2) initialized (notificació)
    send_msg(
        &mut stdin,
        &json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    )?;

    // 3) didOpen — el servidor només resol fitxers que li hem «obert».
    send_msg(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": doc_uri,
                    "languageId": lang_id,
                    "version": 1,
                    "text": text
                }
            }
        }),
    )?;

    // 4) definition — Monaco dona línia/columna 1-based; LSP vol 0-based.
    let line0 = line.saturating_sub(1);
    let char0 = column.saturating_sub(1);
    send_msg(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": doc_uri },
                "position": { "line": line0, "character": char0 }
            }
        }),
    )?;
    wait_for_id(rx, 2, Duration::from_secs(12))
}

/// Extrau la primera ubicació d'una resposta `definition` (Location,
/// Location[] o LocationLink[]).
fn parse_definition<'a>(
    _root: &Path,
    resp: &'a serde_json::Value,
) -> Result<(&'a str, usize, usize), String> {
    let result = resp
        .get("result")
        .filter(|r| !r.is_null())
        .ok_or("el servidor no ha trobat cap definició")?;
    let obj = match result.as_array() {
        Some(arr) => arr.first().ok_or("llista de definicions buida")?,
        None => result,
    };
    let uri = obj
        .get("uri")
        .or_else(|| obj.get("targetUri"))
        .and_then(|v| v.as_str())
        .ok_or("definició sense URI")?;
    let range = obj
        .get("range")
        .or_else(|| obj.get("targetSelectionRange"))
        .or_else(|| obj.get("targetRange"))
        .ok_or("definició sense abast")?;
    let start = range.get("start").ok_or("abast sense inici")?;
    let line = start.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let character = start.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    Ok((uri, line, character))
}
