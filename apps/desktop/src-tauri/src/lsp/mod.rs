//! Mòdul LSP: client del Language Server Protocol sobre «stdio» amb SESSIONS
//! PERSISTENTS (Fase 3c).
//!
//! NoOrbit no inclou cap servidor de llenguatge (serien gegants: rust-analyzer
//! fa ~120 MB). El que fem és **detectar-ne** un d'instal·lat al sistema i, si
//! hi és, parlar-hi el protocol estàndard amb una sessió viva compartida per
//! totes les consultes (definició, referències, completions, hover i
//! diagnòstics reals). Per a TypeScript/JS reutilitzem el `tsserver` del
//! VSCode instal·lat (protocol propi, vegeu `tsserver.rs`). Si no n'hi ha cap,
//! o bé no respon, el cridador cau en la heurística de text pla
//! (`commands::fs::find_definition`) o degrada a llistes buides: MAI es trenca
//! l'editor per un servidor absent.
//!
//! Implementació: JSON-RPC amb capçalera `Content-Length`, procés fill amb
//! stdin/stdout pipejats i un fil lector que reparteix respostes i
//! notificacions (vegeu `session.rs`). Totes les esperes tenen temps límit
//! perquè un servidor penjat no bloquegi mai la interfície, i les sessions
//! es moren soles per inactivitat (120 s) o en canviar de llenguatge/projecte.

pub mod commands;
pub mod session;
pub mod tsserver;

use serde::Serialize;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct LspServer {
    pub language: String,
    pub binary: String,
    pub installed: bool,
}

/// Llista els servidors LSP coneguts i si són al sistema (PATH o, per a
/// TypeScript, el tsserver del VSCode instal·lat). És el catàleg que la UI
/// consulta per decidir quins llenguatges tenen IntelliSense real.
pub fn known_servers() -> Vec<LspServer> {
    let mut list: Vec<LspServer> = [
        ("rust", "rust-analyzer"),
        ("typescript", "typescript-language-server"),
        ("python", "pylsp"),
        ("c", "clangd"),
        ("cpp", "clangd"),
        ("go", "gopls"),
        ("java", "jdtls"),
        ("kotlin", "kotlin-language-server"),
        ("csharp", "omnisharp"),
        ("ruby", "solargraph"),
        ("php", "intelephense"),
        ("swift", "sourcekit-lsp"),
        ("scala", "metals"),
        ("elixir", "elixir-ls"),
        ("dart", "dart"),
        ("lua", "lua-language-server"),
        ("zig", "zls"),
        ("haskell", "haskell-language-server-wrapper"),
        ("bash", "bash-language-server"),
        ("yaml", "yaml-language-server"),
        ("terraform", "terraform-ls"),
        ("vue", "vue-language-server"),
        ("svelte", "svelte-language-server"),
        ("sql", "sql-language-server"),
        ("dockerfile", "docker-langserver"),
    ]
    .iter()
    .map(|(lang, bin)| LspServer {
        language: lang.to_string(),
        binary: bin.to_string(),
        installed: which::which(bin).is_ok(),
    })
    .collect();
    // TypeScript/JS: el tsserver del VSCode instal·lat també compta (és el
    // camí preferent: no cal instal·lar res més). El nom es mostra honest.
    if let Some(entry) = list.iter_mut().find(|s| s.language == "typescript") {
        if !entry.installed {
            if let Some((_, name)) = tsserver::detect(Path::new(".")) {
                entry.installed = true;
                entry.binary = name;
            }
        }
    }
    // PHP: intelephense (propietari, mai embegut) o phpactor (lliure).
    if let Some(entry) = list.iter_mut().find(|s| s.language == "php") {
        if !entry.installed {
            if which::which("phpactor").is_ok() {
                entry.installed = true;
                entry.binary = "phpactor".to_string();
            }
        }
    }
    // Python: pyright és una alternativa comuna a pylsp.
    if let Some(entry) = list.iter_mut().find(|s| s.language == "python") {
        if !entry.installed {
            if which::which("pyright-langserver").is_ok() {
                entry.installed = true;
                entry.binary = "pyright-langserver".to_string();
            }
        }
    }
    list
}

/// Una localització (fitxer + línia/columna, 1-based) dins del projecte.
#[derive(Debug, Clone, Serialize)]
pub struct DefLoc {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub preview: String,
}

/// Converteix un camí absolut en un «file://» URI mínim (escape d'espais).
pub(crate) fn path_to_uri(p: &Path) -> String {
    let s = p.to_string_lossy().replace(' ', "%20");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        // Windows: C:\… → file:///c%3A/…
        format!("file:///{s}")
    }
}

/// Inverteix `path_to_uri`: lleve l'esquema i decodifica els escapes bàsics.
pub(crate) fn uri_to_path(uri: &str) -> Option<PathBuf> {
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
/// Genèric sobre `Write` per poder provar el framing sense processos.
pub(crate) fn send_msg(w: &mut impl Write, value: &serde_json::Value) -> Result<(), String> {
    let body = serde_json::to_string(value).map_err(|e| e.to_string())?;
    write!(w, "Content-Length: {}\r\n\r\n", body.len()).map_err(|e| e.to_string())?;
    w.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Llegeix UN missatge «Content-Length» del stream. `None` en EOF o error
/// (el servidor ha mort). Genèric sobre `BufRead` per als tests de framing.
pub(crate) fn read_lsp_message(r: &mut impl BufRead) -> Option<serde_json::Value> {
    // Capçaleres fins a la línia en blanc que obri el cos.
    let mut content_len: Option<usize> = None;
    loop {
        let mut line = String::new();
        match r.read_line(&mut line) {
            Ok(0) | Err(_) => return None, // EOF o error: el servidor ha mort
            Ok(_) => {}
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some(v) = trimmed.to_ascii_lowercase().strip_prefix("content-length:") {
            if let Ok(n) = v.trim().parse::<usize>() {
                content_len = Some(n);
            }
        }
    }
    let n = content_len?;
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).ok()?;
    let s = String::from_utf8(buf).ok()?;
    serde_json::from_str::<serde_json::Value>(&s).ok()
}

/// Extrau la primera ubicació d'una resposta `definition` (Location,
/// Location[] o LocationLink[]). Treballa sobre el JSON cru de la resposta.
pub(crate) fn parse_definition<'a>(
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

/// Construeix un `DefLoc` relatiu a `root` amb previsualització de la línia
/// (el mateix format que consumix la UI des de la versió heurística).
pub(crate) fn defloc_from(root: &Path, abs: &Path, line0: usize, char0: usize) -> DefLoc {
    let rel = abs
        .strip_prefix(root)
        .map(|x| x.to_string_lossy().trim_start_matches('/').to_string())
        .unwrap_or_else(|_| abs.display().to_string());
    let preview = std::fs::read_to_string(abs)
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
    DefLoc {
        path: rel,
        line: line0 + 1,
        column: char0 + 1,
        preview,
    }
}

/// Consulta la definició al servidor LSP corresponent al tipus de fitxer,
/// reutilitzant la SESSIÓ PERSISTENT (abans es spawnava un servidor per
/// consulta; ara la sessió viu entre consultes i mor per inactivitat).
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
    let sess = session::ensure_session(root, abs_path)?;
    sess.definition(abs_path, line, column, Duration::from_secs(12))?
        .ok_or_else(|| "el servidor no ha trobat cap definició".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    /// Framing JSON-RPC: el que s'escriu ha de portar Content-Length exacte
    /// (en BYTES, no caràcters) i el lector ho ha de desencaixar igual.
    #[test]
    fn framing_anada_i_tornada() {
        let msg = json!({"jsonrpc":"2.0","id":7,"method":"initialize","params":{"x":"ç"}});
        let mut buf: Vec<u8> = Vec::new();
        send_msg(&mut buf, &msg).unwrap();
        let text = String::from_utf8(buf.clone()).unwrap();
        let body = serde_json::to_string(&msg).unwrap();
        assert!(
            text.starts_with(&format!("Content-Length: {}\r\n\r\n", body.len())),
            "capçalera incorrecta: {text}"
        );
        let mut r = Cursor::new(buf);
        let got = read_lsp_message(&mut r).unwrap();
        assert_eq!(got, msg);
        // Després del missatge no queda res: EOF net.
        assert!(read_lsp_message(&mut r).is_none());
    }

    #[test]
    fn framing_multiples_missatges_seguits() {
        let a = json!({"jsonrpc":"2.0","id":1,"result":null});
        let b = json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{}});
        let mut buf: Vec<u8> = Vec::new();
        send_msg(&mut buf, &a).unwrap();
        send_msg(&mut buf, &b).unwrap();
        let mut r = Cursor::new(buf);
        assert_eq!(read_lsp_message(&mut r).unwrap(), a);
        assert_eq!(read_lsp_message(&mut r).unwrap(), b);
        assert!(read_lsp_message(&mut r).is_none());
    }

    #[test]
    fn framing_capcalera_extra_i_cos_unicode() {
        // Content-Type opcional + cos amb accents (longitud en bytes).
        let body = r#"{"jsonrpc":"2.0","id":3,"result":{"msg":"definició"}}"#;
        let raw = format!(
            "Content-Length: {}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{}",
            body.len(),
            body
        );
        let mut r = Cursor::new(raw.into_bytes());
        let got = read_lsp_message(&mut r).unwrap();
        assert_eq!(got["result"]["msg"], "definició");
    }

    #[test]
    fn framing_truncat_o_invalid_no_peta() {
        // Cos tallat a mitges: el lector ha de morir en pau (None).
        let mut r = Cursor::new(b"Content-Length: 50\r\n\r\n{\"id\":1".to_vec());
        assert!(read_lsp_message(&mut r).is_none());
        // Sense Content-Length: línia en blanc i cos JSON → no és LSP vàlid.
        let mut r2 = Cursor::new(b"\r\n{\"id\":1}\r\n".to_vec());
        assert!(read_lsp_message(&mut r2).is_none());
    }

    #[test]
    fn uri_anada_i_tornada_amb_espais() {
        let p = Path::new("/Users/x/el meu projecte/src/main.rs");
        let uri = path_to_uri(p);
        assert_eq!(uri, "file:///Users/x/el%20meu%20projecte/src/main.rs");
        assert_eq!(uri_to_path(&uri).unwrap(), p);
    }

    #[test]
    fn parse_definition_location_i_locationlink() {
        let loc = json!({"result":{"uri":"file:///p/a.rs","range":{"start":{"line":4,"character":2}}}});
        assert_eq!(parse_definition(&loc).unwrap(), ("file:///p/a.rs", 4, 2));
        let link = json!({"result":[{"targetUri":"file:///p/b.rs","targetSelectionRange":{"start":{"line":9,"character":0}}}]});
        assert_eq!(parse_definition(&link).unwrap(), ("file:///p/b.rs", 9, 0));
        let buit = json!({"result":null});
        assert!(parse_definition(&buit).is_err());
        let llista = json!({"result":[]});
        assert!(parse_definition(&llista).is_err());
    }
}