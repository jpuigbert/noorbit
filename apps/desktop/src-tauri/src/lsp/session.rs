//! Gestor de SESSIONS LSP PERSISTENTS (Fase 3c).
//!
//! Una sola sessió activa a la vegada, amb clau (llenguatge, arrel de
//! projecte). Canviar de llenguatge o de projecte MATA la sessió anterior;
//! sense consultes durant 120 s, el vigilant la mata sola (RAM prioritària).
//! `kill_all` en tancar l'app (mateix patró que `term::kill_all`).
//!
//! Dos protocols sota el mateix sostre:
//!   • LSP estàndard (JSON-RPC «Content-Length») per als servidors detectats
//!     de la taula (rust-analyzer, phpactor/intelephense, pylsp/pyright…).
//!   • tsserver (JSON line-delimited) per a TS/JS via el VSCode instal·lat.
//!
//! Sostre de memòria: els servidors node arrenquen amb
//! `--max-old-space-size=512` (NODE_OPTIONS o flag directe de node).

use super::{defloc_from, parse_definition, path_to_uri, read_lsp_message, send_msg, uri_to_path, DefLoc};
use super::tsserver::{self, TsLaunch};
use once_cell::sync::Lazy;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};
#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Inactivitat màxima abans de matar la sessió (RAM per a la IA nativa).
pub const IDLE_LIMIT: Duration = Duration::from_secs(120);
/// Temps límit d'una consulta (completion/hover/…): degradació honesta.
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(8);
/// Arrencada + initialize: rust-analyzer en fred pot trigar (indexació).
const INIT_TIMEOUT: Duration = Duration::from_secs(20);

/// Diagnòstic amb el contracte exacte de la UI (línia/columna 1-based).
#[derive(Debug, Clone, Serialize)]
pub struct Diag {
    pub line: usize,
    pub col: usize,
    pub severity: String, // "error" | "warning" | "info"
    pub message: String,
    pub source: String,
}

/// Entrada de completion amb el contracte exacte de la UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionEntry {
    pub label: String,
    pub insert_text: String,
    pub detail: String,
    /// CompletionItemKind numèric (LSP/Monaco).
    pub kind: u32,
}

/// Referència amb el contracte exacte de la UI (ruta relativa, 1-based).
#[derive(Debug, Clone, Serialize)]
pub struct RefLoc {
    pub path: String,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Proto {
    Lsp,
    Ts,
}

/// Una sessió viva: procés fill + fil lector + mapes compartits.
pub struct Session {
    pub language: String,
    pub root: PathBuf,
    pub server: String,
    pub(crate) proto: Proto,
    /// Cua d'escriptura cap al fil escriptor dedicat (mpsc). El fil lector
    /// MAI pren un lock d'escriptor per respondre peticions del servidor:
    /// només encola bytes. Això trenca el deadlock circular amb payloads
    /// grans (escriptor bloquejat en un stdin ple mentre el servidor espera
    /// una resposta abans de llegir més).
    writer_tx: Arc<Mutex<Option<std::sync::mpsc::Sender<Vec<u8>>>>>,
    child: Mutex<Option<Child>>,
    /// Respostes pendents: id/seq (com a text) → canal d'un sol ús.
    pending: Arc<Mutex<HashMap<String, std::sync::mpsc::Sender<Value>>>>,
    /// Diagnòstics rebuts (publishDiagnostics / syntaxDiag+semanticDiag).
    /// Clau LSP: ruta absoluta. Clau Ts: «ruta#syntax» / «ruta#semantic».
    diags: Arc<Mutex<HashMap<String, Vec<Diag>>>>,
    /// Generació de diagnòstics per clau (per saber si ha arribat un publish
    /// NOU després d'un didChange, no tornar els vells de seguida).
    diag_gen: Arc<Mutex<HashMap<String, u64>>>,
    /// Ts: màscara d'esdeveniments rebuts per fitxer (1=syntax, 2=semantic).
    ts_seen: Arc<Mutex<HashMap<String, u8>>>,
    /// Documents oberts al servidor: ruta → (versió, text).
    docs: Mutex<HashMap<String, (i64, String)>>,
    next_id: AtomicI64,
    last: Mutex<Instant>,
    alive: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
}

impl Session {
    /// Arrenca el fil lector i embolcalla el procés. Sense handshake: el
    /// cridador (`spawn_session`) fa l'initialize si és LSP.
    fn start(
        proto: Proto,
        language: &str,
        root: &Path,
        server: &str,
        mut child: Child,
    ) -> Arc<Session> {
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        // FIL ESCRIPTOR DEDICAT (mpsc): rep bytes i els escriu al stdin del
        // servidor. Cap lock compartit es reté durant la IO bloquejant; el
        // lector i les peticions només encolen (send no bloquejant).
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let writer_tx = Arc::new(Mutex::new(Some(tx)));
        if let Some(mut stdin) = stdin {
            std::thread::spawn(move || {
                while let Ok(bytes) = rx.recv() {
                    if stdin.write_all(&bytes).is_err() {
                        break;
                    }
                    if stdin.flush().is_err() {
                        break;
                    }
                }
                // Tots els senders deixats anar (kill) o error d'escriptura:
                // drop de stdin → el servidor rep EOF.
                drop(stdin);
            });
        }
        let pending: Arc<Mutex<HashMap<String, std::sync::mpsc::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let diags: Arc<Mutex<HashMap<String, Vec<Diag>>>> = Arc::new(Mutex::new(HashMap::new()));
        let diag_gen = Arc::new(Mutex::new(HashMap::new()));
        let ts_seen = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let sess = Arc::new(Session {
            language: language.to_string(),
            root: root.to_path_buf(),
            server: server.to_string(),
            proto,
            writer_tx: writer_tx.clone(),
            child: Mutex::new(Some(child)),
            pending: pending.clone(),
            diags: diags.clone(),
            diag_gen: diag_gen.clone(),
            ts_seen: ts_seen.clone(),
            docs: Mutex::new(HashMap::new()),
            next_id: AtomicI64::new(1),
            last: Mutex::new(Instant::now()),
            alive: alive.clone(),
            ready: Arc::new(AtomicBool::new(false)),
        });
        if let Some(stdout) = stdout {
            let server_name = server.to_string();
            let alive2 = alive.clone();
            std::thread::spawn(move || {
                let mut r = BufReader::new(stdout);
                match proto {
                    Proto::Lsp => {
                        while let Some(msg) = read_lsp_message(&mut r) {
                            route_lsp(&msg, &writer_tx, &pending, &diags, &diag_gen, &server_name);
                        }
                    }
                    Proto::Ts => {
                        let mut line = String::new();
                        loop {
                            line.clear();
                            match r.read_line(&mut line) {
                                Ok(0) | Err(_) => break,
                                Ok(_) => {}
                            }
                            if let Ok(msg) = serde_json::from_str::<Value>(line.trim()) {
                                route_ts(&msg, &pending, &diags, &diag_gen, &ts_seen, &server_name);
                            }
                        }
                    }
                }
                // EOF: el servidor ha mort. Alliberem els qui esperen.
                alive2.store(false, Ordering::SeqCst);
                if let Ok(mut p) = pending.lock() {
                    p.clear();
                }
            });
        } else {
            sess.alive.store(false, Ordering::SeqCst);
        }
        sess
    }

    pub fn touch(&self) {
        if let Ok(mut l) = self.last.lock() {
            *l = Instant::now();
        }
    }

    pub fn is_alive(&self) -> bool {
        if !self.alive.load(Ordering::SeqCst) {
            return false;
        }
        // El procés pot haver mort sense que el lector ho haja vist encara.
        match self.child.lock() {
            Ok(mut g) => match g.as_mut() {
                Some(c) => c.try_wait().map(|s| s.is_none()).unwrap_or(false),
                None => false,
            },
            Err(_) => false,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst) && self.is_alive()
    }

    /// Tanca el conducte (EOF per al servidor) i mata el procés: cap orfe.
    pub fn kill(&self) {
        self.alive.store(false, Ordering::SeqCst);
        if let Ok(mut g) = self.writer_tx.lock() {
            *g = None; // drop del Sender → el fil escriptor ix → stdin EOF
        }
        if let Ok(mut g) = self.child.lock() {
            if let Some(mut c) = g.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
        if let Ok(mut p) = self.pending.lock() {
            p.clear();
        }
    }

    fn write(&self, f: impl FnOnce(&mut Vec<u8>) -> Result<(), String>) -> Result<(), String> {
        // Serialitza al buffer i ENCOLA cap al fil escriptor: cap IO sota
        // cap lock compartit (el send d'un canal unbounded no bloqueja).
        let mut buf = Vec::new();
        f(&mut buf)?;
        let g = self.writer_tx.lock().map_err(|_| "sessió enverinada")?;
        let tx = g.as_ref().ok_or("la sessió LSP s'ha tancat")?;
        tx.send(buf).map_err(|_| "la sessió LSP s'ha tancat".to_string())
    }

    /// Petició LSP amb resposta esperada (idempotent en ids).
    fn lsp_request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = std::sync::mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| "sessió enverinada")?
            .insert(id.to_string(), tx);
        let msg = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        let wr = self.write(|w| send_msg(w, &msg));
        if let Err(e) = wr {
            let _ = self.pending.lock().map(|mut p| p.remove(&id.to_string()));
            return Err(e);
        }
        match rx.recv_timeout(timeout) {
            Ok(resp) => {
                if let Some(err) = resp.get("error").filter(|e| !e.is_null()) {
                    return Err(format!("error del servidor: {err}"));
                }
                Ok(resp.get("result").cloned().unwrap_or(Value::Null))
            }
            Err(e) => {
                let _ = self.pending.lock().map(|mut p| p.remove(&id.to_string()));
                Err(match e {
                    std::sync::mpsc::RecvTimeoutError::Timeout => {
                        "el servidor LSP no ha respost a temps".into()
                    }
                    std::sync::mpsc::RecvTimeoutError::Disconnected => {
                        "el servidor LSP s'ha tancat".into()
                    }
                })
            }
        }
    }

    fn lsp_notify(&self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({"jsonrpc":"2.0","method":method,"params":params});
        self.write(|w| send_msg(w, &msg))
    }

    /// Petició tsserver (seq numèric, JSON line-delimited).
    fn ts_request(&self, command: &str, arguments: Value, timeout: Duration) -> Result<Value, String> {
        let seq = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = std::sync::mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| "sessió enverinada")?
            .insert(seq.to_string(), tx);
        let msg = json!({"seq":seq,"type":"request","command":command,"arguments":arguments});
        let body = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
        let wr = self.write(|w| {
            writeln!(w, "{body}").map_err(|e| e.to_string())?;
            w.flush().map_err(|e| e.to_string())
        });
        if let Err(e) = wr {
            let _ = self.pending.lock().map(|mut p| p.remove(&seq.to_string()));
            return Err(e);
        }
        match rx.recv_timeout(timeout) {
            Ok(resp) => {
                if resp.get("success").and_then(|v| v.as_bool()) != Some(true) {
                    let m = resp.get("message").and_then(|v| v.as_str()).unwrap_or("sense detall");
                    return Err(format!("tsserver: {m}"));
                }
                Ok(resp.get("body").cloned().unwrap_or(Value::Null))
            }
            Err(e) => {
                let _ = self.pending.lock().map(|mut p| p.remove(&seq.to_string()));
                Err(match e {
                    std::sync::mpsc::RecvTimeoutError::Timeout => {
                        "el tsserver no ha respost a temps".into()
                    }
                    std::sync::mpsc::RecvTimeoutError::Disconnected => {
                        "el tsserver s'ha tancat".into()
                    }
                })
            }
        }
    }

    /// Notificació tsserver: `open`, `close` i `geterr` NO reben resposta
    /// (el protocol les marca «responseRequired: false»); esperar-la penjaria
    /// la consulta fins al temps límit. Els diagnòstics arriben com a EVENTS.
    fn ts_notify(&self, command: &str, arguments: Value) -> Result<(), String> {
        let seq = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = json!({"seq":seq,"type":"request","command":command,"arguments":arguments});
        let body = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
        self.write(|w| {
            writeln!(w, "{body}").map_err(|e| e.to_string())?;
            w.flush().map_err(|e| e.to_string())
        })
    }

    /// Obre (o actualitza) el document al servidor amb el text del DISC.
    /// LSP: didOpen/didChange (sincronització completa). Ts: close+open.
    fn ensure_doc(&self, abs: &Path, text: &str) -> Result<(), String> {
        let key = abs.to_string_lossy().to_string();
        let mut docs = self.docs.lock().map_err(|_| "sessió enverinada")?;
        if let Some((_, old)) = docs.get(&key) {
            if old == text {
                return Ok(());
            }
        }
        match self.proto {
            Proto::Lsp => {
                let uri = path_to_uri(abs);
                if let Some((ver, _)) = docs.get(&key) {
                    let v = ver + 1;
                    self.lsp_notify(
                        "textDocument/didChange",
                        json!({
                            "textDocument": {"uri": uri, "version": v},
                            "contentChanges": [{"text": text}]
                        }),
                    )?;
                    docs.insert(key, (v, text.to_string()));
                } else {
                    self.lsp_notify(
                        "textDocument/didOpen",
                        json!({
                            "textDocument": {
                                "uri": uri,
                                "languageId": lsp_lang_id(abs),
                                "version": 1,
                                "text": text
                            }
                        }),
                    )?;
                    docs.insert(key, (1, text.to_string()));
                }
            }
            Proto::Ts => {
                if docs.contains_key(&key) {
                    let _ = self.ts_notify("close", json!({"file": key}));
                }
                self.ts_notify("open", json!({"file": key, "fileContent": text}))?;
                docs.insert(key, (1, text.to_string()));
            }
        }
        Ok(())
    }

    fn position_params(&self, abs: &Path, line: usize, col: usize) -> Value {
        json!({
            "textDocument": {"uri": path_to_uri(abs)},
            "position": {"line": line.saturating_sub(1), "character": col.saturating_sub(1)}
        })
    }

    pub fn completion(
        &self,
        abs: &Path,
        line: usize,
        col: usize,
    ) -> Result<Vec<CompletionEntry>, String> {
        self.touch();
        let text = std::fs::read_to_string(abs).map_err(|e| e.to_string())?;
        self.ensure_doc(abs, &text)?;
        match self.proto {
            Proto::Lsp => {
                let res =
                    self.lsp_request("textDocument/completion", self.position_params(abs, line, col), QUERY_TIMEOUT)?;
                let items: Vec<lsp_types::CompletionItem> = if res.is_array() {
                    serde_json::from_value(res).unwrap_or_default()
                } else {
                    serde_json::from_value(res.get("items").cloned().unwrap_or(Value::Null))
                        .unwrap_or_default()
                };
                Ok(items
                    .into_iter()
                    .map(|it| {
                        let kind = it
                            .kind
                            .and_then(|k| serde_json::to_value(k).ok())
                            .and_then(|v| v.as_u64())
                            .unwrap_or(1) as u32;
                        CompletionEntry {
                            insert_text: it.insert_text.clone().unwrap_or_else(|| it.label.clone()),
                            detail: it.detail.unwrap_or_default(),
                            label: it.label,
                            kind,
                        }
                    })
                    .collect())
            }
            Proto::Ts => {
                let key = abs.to_string_lossy().to_string();
                let body = self.ts_request(
                    "completions",
                    json!({"file": key, "line": line, "offset": col}),
                    QUERY_TIMEOUT,
                )?;
                Ok(tsserver::map_completions(&body))
            }
        }
    }

    pub fn hover(&self, abs: &Path, line: usize, col: usize) -> Result<Option<String>, String> {
        self.touch();
        let text = std::fs::read_to_string(abs).map_err(|e| e.to_string())?;
        self.ensure_doc(abs, &text)?;
        match self.proto {
            Proto::Lsp => {
                let res =
                    self.lsp_request("textDocument/hover", self.position_params(abs, line, col), QUERY_TIMEOUT)?;
                if res.is_null() {
                    return Ok(None);
                }
                let hover: lsp_types::Hover = match serde_json::from_value(res) {
                    Ok(h) => h,
                    Err(_) => return Ok(None),
                };
                Ok(Some(flatten_hover(hover.contents)))
            }
            Proto::Ts => {
                let key = abs.to_string_lossy().to_string();
                let body = self.ts_request(
                    "quickinfo",
                    json!({"file": key, "line": line, "offset": col}),
                    QUERY_TIMEOUT,
                )?;
                Ok(tsserver::map_quickinfo(&body))
            }
        }
    }

    pub fn references(&self, abs: &Path, line: usize, col: usize) -> Result<Vec<RefLoc>, String> {
        self.touch();
        let text = std::fs::read_to_string(abs).map_err(|e| e.to_string())?;
        self.ensure_doc(abs, &text)?;
        let raw: Vec<(String, usize, usize)> = match self.proto {
            Proto::Lsp => {
                let mut params = self.position_params(abs, line, col);
                params["context"] = json!({"includeDeclaration": true});
                let res =
                    self.lsp_request("textDocument/references", params, QUERY_TIMEOUT)?;
                let locs: Vec<lsp_types::Location> =
                    serde_json::from_value(res).unwrap_or_default();
                locs.into_iter()
                    .filter_map(|l| {
                        let p = l.uri.to_file_path().ok()?;
                        Some((
                            p.to_string_lossy().to_string(),
                            l.range.start.line as usize + 1,
                            l.range.start.character as usize + 1,
                        ))
                    })
                    .collect()
            }
            Proto::Ts => {
                let key = abs.to_string_lossy().to_string();
                let body = self.ts_request(
                    "references",
                    json!({"file": key, "line": line, "offset": col}),
                    QUERY_TIMEOUT,
                )?;
                tsserver::map_references(&body)
            }
        };
        Ok(raw
            .into_iter()
            .map(|(file, line, col)| {
                let p = PathBuf::from(&file);
                let rel = p
                    .strip_prefix(&self.root)
                    .map(|x| x.to_string_lossy().trim_start_matches('/').to_string())
                    .unwrap_or(file);
                RefLoc { path: rel, line, col }
            })
            .collect())
    }

    pub fn definition(
        &self,
        abs: &Path,
        line: usize,
        col: usize,
        timeout: Duration,
    ) -> Result<Option<DefLoc>, String> {
        self.touch();
        let text = std::fs::read_to_string(abs).map_err(|e| e.to_string())?;
        self.ensure_doc(abs, &text)?;
        match self.proto {
            Proto::Lsp => {
                let res =
                    self.lsp_request("textDocument/definition", self.position_params(abs, line, col), timeout)?;
                let resp = json!({"result": res});
                match parse_definition(&resp) {
                    Ok((uri, line0, char0)) => {
                        let dest = uri_to_path(uri).ok_or("URI de destinació invàlida")?;
                        Ok(Some(defloc_from(&self.root, &dest, line0, char0)))
                    }
                    Err(_) => Ok(None),
                }
            }
            Proto::Ts => {
                let key = abs.to_string_lossy().to_string();
                let body = self.ts_request(
                    "definition",
                    json!({"file": key, "line": line, "offset": col}),
                    timeout,
                )?;
                Ok(tsserver::map_definition(&body).map(|(file, line1, col1)| {
                    defloc_from(&self.root, Path::new(&file), line1 - 1, col1 - 1)
                }))
            }
        }
    }

    /// Diagnòstics REALS del fitxer. LSP: espera el publishDiagnostics nou
    /// (didOpen/didChange el dispara). Ts: «geterr» (syntactic + semantic).
    /// Si el servidor no respon a temps, retorna el que hi ha (o buit):
    /// degradació honesta, mai un error que trenque l'editor.
    pub fn diagnostics(&self, abs: &Path, timeout: Duration) -> Result<Vec<Diag>, String> {
        self.touch();
        let text = std::fs::read_to_string(abs).map_err(|e| e.to_string())?;
        let key = abs.to_string_lossy().to_string();
        let gen_before = self.diag_gen.lock().map(|g| g.get(&key).copied().unwrap_or(0)).unwrap_or(0);
        self.ensure_doc(abs, &text)?;
        let deadline = Instant::now() + timeout;
        match self.proto {
            Proto::Lsp => {
                // Espera un publish NOU (generació canviada) o el límit.
                loop {
                    let changed = self
                        .diag_gen
                        .lock()
                        .map(|g| g.get(&key).copied().unwrap_or(0) != gen_before)
                        .unwrap_or(true);
                    if changed || Instant::now() >= deadline || !self.is_alive() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(60));
                }
                Ok(self.diags.lock().map(|d| d.get(&key).cloned().unwrap_or_default()).unwrap_or_default())
            }
            Proto::Ts => {
                // Reinicia la collita d'aquest fitxer i demana geterr.
                if let Ok(mut s) = self.ts_seen.lock() {
                    s.insert(key.clone(), 0);
                }
                if let Ok(mut d) = self.diags.lock() {
                    d.remove(&format!("{key}#syntax"));
                    d.remove(&format!("{key}#semantic"));
                }
                self.ts_notify("geterr", json!({"files": [key], "delay": 0}))?;
                loop {
                    let done = self.ts_seen.lock().map(|s| s.get(&key).copied().unwrap_or(0) == 3).unwrap_or(true);
                    if done || Instant::now() >= deadline || !self.is_alive() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(60));
                }
                let d = self.diags.lock().map_err(|_| "sessió enverinada")?;
                let mut out = d.get(&format!("{key}#syntax")).cloned().unwrap_or_default();
                out.extend(d.get(&format!("{key}#semantic")).cloned().unwrap_or_default());
                Ok(out)
            }
        }
    }
}

/// Reparteix un missatge LSP entrant: respostes → pending; publishDiagnostics
/// → mapa global; peticions del servidor → resposta automàtica (si no, es
/// queda esperant per sempre: rust-analyzer en fa unes quantes).
fn route_lsp(
    msg: &Value,
    writer_tx: &Arc<Mutex<Option<std::sync::mpsc::Sender<Vec<u8>>>>>,
    pending: &Arc<Mutex<HashMap<String, std::sync::mpsc::Sender<Value>>>>,
    diags: &Arc<Mutex<HashMap<String, Vec<Diag>>>>,
    diag_gen: &Arc<Mutex<HashMap<String, u64>>>,
    server: &str,
) {
    let method = msg.get("method").and_then(|m| m.as_str());
    if let Some(id) = msg.get("id").filter(|v| !v.is_null()) {
        if method.is_some() {
            // Petició servidor→client: resposta buida honesta per no penjar-lo.
            let result = match method.unwrap_or("") {
                "workspace/configuration" => {
                    let n = msg
                        .get("params")
                        .and_then(|p| p.get("items"))
                        .and_then(|i| i.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    json!(vec![Value::Null; n])
                }
                "workspace/applyEdit" => json!({"applied": false}),
                _ => Value::Null,
            };
            let reply = json!({"jsonrpc":"2.0","id":id,"result":result});
            // Resposta des del lector: només s'ENCOLA (send no bloquejant);
            // el fil escriptor dedicat fa la IO. El lector MAI escriu directament.
            let mut buf = Vec::new();
            if send_msg(&mut buf, &reply).is_ok() {
                if let Ok(g) = writer_tx.lock() {
                    if let Some(tx) = g.as_ref() {
                        let _ = tx.send(buf);
                    }
                }
            }
        } else if let Ok(mut p) = pending.lock() {
            if let Some(tx) = p.remove(&id.to_string()) {
                let _ = tx.send(msg.clone());
            }
        }
        return;
    }
    if method == Some("textDocument/publishDiagnostics") {
        if let Some(params) = msg.get("params") {
            if let Ok(p) =
                serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(params.clone())
            {
                if let Ok(path) = p.uri.to_file_path() {
                    let key = path.to_string_lossy().to_string();
                    let list: Vec<Diag> = p
                        .diagnostics
                        .iter()
                        .map(|d| {
                            let severity = match d.severity {
                                Some(lsp_types::DiagnosticSeverity::ERROR) => "error",
                                Some(lsp_types::DiagnosticSeverity::WARNING) => "warning",
                                _ => "info",
                            };
                            Diag {
                                line: d.range.start.line as usize + 1,
                                col: d.range.start.character as usize + 1,
                                severity: severity.to_string(),
                                message: d.message.clone(),
                                source: d.source.clone().unwrap_or_else(|| server.to_string()),
                            }
                        })
                        .collect();
                    if let Ok(mut d) = diags.lock() {
                        d.insert(key.clone(), list);
                    }
                    if let Ok(mut g) = diag_gen.lock() {
                        *g.entry(key).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    // Resta de notificacions (progress, showMessage, serverStatus…): fora.
}

/// Reparteix un missatge tsserver: respostes → pending; syntaxDiag/semanticDiag
/// → mapa global (per parts, es combinen en llegir-les).
fn route_ts(
    msg: &Value,
    pending: &Arc<Mutex<HashMap<String, std::sync::mpsc::Sender<Value>>>>,
    diags: &Arc<Mutex<HashMap<String, Vec<Diag>>>>,
    diag_gen: &Arc<Mutex<HashMap<String, u64>>>,
    ts_seen: &Arc<Mutex<HashMap<String, u8>>>,
    server: &str,
) {
    match msg.get("type").and_then(|t| t.as_str()) {
        Some("response") => {
            if let Some(seq) = msg.get("request_seq") {
                if let Ok(mut p) = pending.lock() {
                    if let Some(tx) = p.remove(&seq.to_string()) {
                        let _ = tx.send(msg.clone());
                    }
                }
            }
        }
        Some("event") => {
            let ev = msg.get("event").and_then(|e| e.as_str()).unwrap_or("");
            let bit = match ev {
                "syntaxDiag" => 1u8,
                "semanticDiag" => 2u8,
                _ => 0,
            };
            if bit != 0 {
                if let Some(body) = msg.get("body") {
                    if let Some(file) = body.get("file").and_then(|f| f.as_str()) {
                        let kind = if bit == 1 { "syntax" } else { "semantic" };
                        let key = format!("{file}#{kind}");
                        let list = tsserver::map_diagnostics(body, server);
                        if let Ok(mut d) = diags.lock() {
                            d.insert(key.clone(), list);
                        }
                        if let Ok(mut g) = diag_gen.lock() {
                            *g.entry(key).or_insert(0) += 1;
                        }
                        if let Ok(mut s) = ts_seen.lock() {
                            *s.entry(file.to_string()).or_insert(0) |= bit;
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Aplana el contingut d'un hover LSP a un sol text (markdown).
fn flatten_hover(contents: lsp_types::HoverContents) -> String {
    use lsp_types::{HoverContents as H, MarkedString as M};
    match contents {
        H::Scalar(M::String(s)) => s,
        H::Scalar(M::LanguageString(ls)) => format!("```{}\n{}\n```", ls.language, ls.value),
        H::Markup(m) => m.value,
        H::Array(arr) => arr
            .into_iter()
            .map(|m| match m {
                M::String(s) => s,
                M::LanguageString(ls) => format!("```{}\n{}\n```", ls.language, ls.value),
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  DETECCIÓ: família de llenguatge + servidor instal·lat (mai s'instal·la res)
// ─────────────────────────────────────────────────────────────────────────────

/// Família de llenguatge (clau de sessió) segons l'extensió. TS/JS comparteixen
/// sessió (el mateix tsserver serveix els dos).
pub(crate) fn lang_of(abs: &Path) -> Option<&'static str> {
    let ext = abs.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "rs" => Some("rust"),
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => Some("typescript"),
        "py" | "pyi" => Some("python"),
        "php" => Some("php"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" => Some("cpp"),
        "go" => Some("go"),
        "java" => Some("java"),
        "kt" | "kts" => Some("kotlin"),
        "cs" => Some("csharp"),
        "rb" => Some("ruby"),
        "swift" => Some("swift"),
        "scala" | "sc" => Some("scala"),
        "ex" | "exs" => Some("elixir"),
        "dart" => Some("dart"),
        "lua" => Some("lua"),
        "zig" => Some("zig"),
        "hs" => Some("haskell"),
        "sh" | "bash" => Some("bash"),
        "yaml" | "yml" => Some("yaml"),
        "tf" | "tfvars" => Some("terraform"),
        "vue" => Some("vue"),
        "svelte" => Some("svelte"),
        "sql" => Some("sql"),
        "dockerfile" => Some("dockerfile"),
        _ => None,
    }
}

/// Identificador LSP del document (per al didOpen; tsx/jsx tenen el seu).
fn lsp_lang_id(abs: &Path) -> &'static str {
    let ext = abs.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "ts" => "typescript",
        "tsx" => "typescriptreact",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "javascriptreact",
        "rs" => "rust",
        "py" | "pyi" => "python",
        "php" => "php",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "cs" => "csharp",
        "rb" => "ruby",
        "swift" => "swift",
        "scala" | "sc" => "scala",
        "ex" | "exs" => "elixir",
        "dart" => "dart",
        "lua" => "lua",
        "zig" => "zig",
        "hs" => "haskell",
        "sh" | "bash" => "shellscript",
        "yaml" | "yml" => "yaml",
        "tf" | "tfvars" => "terraform",
        "vue" => "vue",
        "svelte" => "svelte",
        "sql" => "sql",
        "dockerfile" => "dockerfile",
        _ => "plaintext",
    }
}

/// Servidor detectat per a una família: LSP genèric o tsserver.
enum Spec {
    Lsp { bin: PathBuf, args: &'static [&'static str], name: String },
    Ts(TsLaunch, String),
}

/// Candidats per família (el primer que siga al PATH guanya). Els args són
/// els de la taula històrica de NoOrbit; els servidors node reben el sostre
/// de memòria via NODE_OPTIONS en arrencar.
fn detect(root: &Path, lang: &str) -> Option<Spec> {
    if lang == "typescript" {
        if let Some((launch, name)) = tsserver::detect(root) {
            return Some(Spec::Ts(launch, name));
        }
    }
    let candidates: &[(&str, &[&str])] = match lang {
        "rust" => &[("rust-analyzer", &[])],
        "typescript" => &[("typescript-language-server", &["--stdio"])],
        "python" => &[("pylsp", &[]), ("pyright-langserver", &["--stdio"])],
        "php" => &[("intelephense", &["--stdio"]), ("phpactor", &["language-server"])],
        "c" | "cpp" => &[("clangd", &[])],
        "go" => &[("gopls", &[])],
        "java" => &[("jdtls", &[])],
        "kotlin" => &[("kotlin-language-server", &[])],
        "csharp" => &[("omnisharp", &["-lsp"])],
        "ruby" => &[("solargraph", &["stdio"])],
        "swift" => &[("sourcekit-lsp", &[])],
        "scala" => &[("metals", &[])],
        "elixir" => &[("elixir-ls", &[])],
        "dart" => &[("dart", &["language-server", "--protocol=lsp"])],
        "lua" => &[("lua-language-server", &[])],
        "zig" => &[("zls", &[])],
        "haskell" => &[("haskell-language-server-wrapper", &["--lsp"])],
        "bash" => &[("bash-language-server", &["start"])],
        "yaml" => &[("yaml-language-server", &["--stdio"])],
        "terraform" => &[("terraform-ls", &["serve"])],
        "vue" => &[("vue-language-server", &["--stdio"])],
        "svelte" => &[("svelte-language-server", &["--stdio"])],
        "sql" => &[("sql-language-server", &["up", "--method", "stdio"])],
        "dockerfile" => &[("docker-langserver", &["--stdio"])],
        _ => return None,
    };
    for (bin, args) in candidates {
        if let Ok(p) = which::which(bin) {
            return Some(Spec::Lsp { bin: p, args, name: bin.to_string() });
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
//  GESTOR GLOBAL: límit 1 sessió, canvi = kill, inactivitat = kill, kill_all
// ─────────────────────────────────────────────────────────────────────────────

static MANAGER: Lazy<Mutex<Option<Arc<Session>>>> = Lazy::new(|| Mutex::new(None));
/// Serialitza les ARRENCADES (initialize pot trigar 20 s; no volem dos
/// servidors nascuts alhora ni bloquejar l'estat mentre un arrenca).
static SPAWN_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static WATCHDOG: Once = Once::new();

fn current() -> Option<Arc<Session>> {
    MANAGER.lock().ok().and_then(|g| g.clone())
}

/// Substitueix la sessió activa (matant l'anterior): el nucli del límit 1.
fn swap(sess: Arc<Session>) -> Arc<Session> {
    let mut g = MANAGER.lock().expect("gestor LSP enverinat");
    if let Some(old) = g.take() {
        old.kill();
    }
    *g = Some(sess.clone());
    sess
}

/// Mata la sessió si fa més de `limit` que no rep consultes. El vigilant la
/// crida amb IDLE_LIMIT; els tests, amb límits curts.
pub fn reap_idle(limit: Duration) {
    let mut g = match MANAGER.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let expired = g
        .as_ref()
        .map(|s| s.last.lock().map(|l| l.elapsed() > limit).unwrap_or(true))
        .unwrap_or(false);
    if expired {
        if let Some(s) = g.take() {
            s.kill();
        }
    }
}

fn start_watchdog() {
    WATCHDOG.call_once(|| {
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_secs(5));
            reap_idle(IDLE_LIMIT);
        });
    });
}

/// Arrenca un servidor (initialize inclòs si és LSP) i el deixa com a sessió
/// activa. Cap servidor instal·lat → Err honest (el cridador degrada).
pub fn ensure_session(root: &Path, abs: &Path) -> Result<Arc<Session>, String> {
    let lang = lang_of(abs).ok_or("tipus de fitxer sense servidor LSP")?;
    // Camí ràpid: sessió viva per a esta (llenguatge, arrel).
    if let Some(s) = current() {
        if s.language == lang && s.root == root && s.is_alive() {
            s.touch();
            return Ok(s);
        }
    }
    start_watchdog();
    let _guard = SPAWN_LOCK.lock().map_err(|_| "gestor LSP enverinat")?;
    // Doble comprovació: una altra consulta pot haver-la creada mentrestant.
    if let Some(s) = current() {
        if s.language == lang && s.root == root && s.is_alive() {
            s.touch();
            return Ok(s);
        }
    }
    let spec = detect(root, lang)
        .ok_or_else(|| format!("cap servidor LSP instal·lat per a «{lang}»"))?;
    let sess = spawn_session(root, lang, spec)?;
    Ok(swap(sess))
}

/// Consulta BARATA per al bucle de l'agent: la sessió activa si cobrix este
/// (llenguatge, arrel), sense spawnar res mai.
pub fn active_for(root: &Path, abs: &Path) -> Option<Arc<Session>> {
    let lang = lang_of(abs)?;
    let g = MANAGER.lock().ok()?;
    let s = g.as_ref()?;
    if s.language == lang && s.root == root && s.is_alive() {
        Some(s.clone())
    } else {
        None
    }
}

/// Estat honest per a la UI: servidor actiu per a l'arrel demanada (o res).
pub fn status(root: &Path) -> (Option<String>, Option<String>, bool) {
    match current() {
        Some(s) if s.root == root && s.is_alive() => {
            (Some(s.server.clone()), Some(s.language.clone()), s.is_ready())
        }
        _ => (None, None, false),
    }
}

/// Tanca la sessió activa (tancament de l'app): cap servidor orfe.
pub fn kill_all() {
    if let Ok(mut g) = MANAGER.lock() {
        if let Some(s) = g.take() {
            s.kill();
        }
    }
}

/// Arrenca el procés i (si és LSP) fa el handshake initialize/initialized.
fn spawn_session(root: &Path, lang: &str, spec: Spec) -> Result<Arc<Session>, String> {
    #[allow(unused_mut)]
    let mut cmd = match &spec {
        Spec::Lsp { bin, args, .. } => {
            let mut c = Command::new(bin);
            c.args(*args);
            // Sostre de RAM per als servidors node (intelephense, pyright,
            // typescript-language-server…); els binaris natius l'ignoren.
            c.env("NODE_OPTIONS", "--max-old-space-size=512");
            c
        }
        Spec::Ts(launch, _) => match launch {
            TsLaunch::Script { node, script } => {
                let mut c = Command::new(node);
                c.arg("--max-old-space-size=512").arg(script);
                c
            }
            TsLaunch::Bin(bin) => {
                let mut c = Command::new(bin);
                c.env("NODE_OPTIONS", "--max-old-space-size=512");
                c
            }
        },
    };
    cmd.current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let (proto, name) = match &spec {
        Spec::Lsp { name, .. } => (Proto::Lsp, name.clone()),
        Spec::Ts(_, name) => (Proto::Ts, name.clone()),
    };
    let child = cmd
        .spawn()
        .map_err(|e| format!("no s'ha pogut arrencar {name}: {e}"))?;
    let sess = Session::start(proto, lang, root, &name, child);
    if proto == Proto::Lsp {
        let root_uri = path_to_uri(root);
        let init = sess.lsp_request(
            "initialize",
            json!({
                "processId": null,
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "synchronization": {"didSave": true},
                        "completion": {"completionItem": {"snippetSupport": false}},
                        "hover": {"contentFormat": ["markdown", "plaintext"]},
                        "definition": {"linkSupport": true},
                        "references": {},
                        "publishDiagnostics": {"relatedInformation": false}
                    },
                    "workspace": {"applyEdit": true, "configuration": true, "workspaceFolders": true}
                },
                "workspaceFolders": [{"uri": root_uri, "name": root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()}]
            }),
            INIT_TIMEOUT,
        );
        if let Err(e) = init {
            sess.kill();
            return Err(format!("{name} no ha inicialitzat: {e}"));
        }
        if let Err(e) = sess.lsp_notify("initialized", json!({})) {
            sess.kill();
            return Err(e);
        }
    }
    sess.ready.store(true, Ordering::SeqCst);
    sess.touch();
    Ok(sess)
}

// ─────────────────────────────────────────────────────────────────────────────
//  TESTS: cicle de vida del gestor (idle, canvi, límit 1) + tsserver REAL
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Els tests del gestor comparteixen l'estat GLOBAL: es serialitzen.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Sessió de prova amb un procés mut i barat (`sleep`): viu, no parla,
    /// perfecte per provar el cicle de vida sense servidors reals.
    #[cfg(unix)]
    fn dummy_session(language: &str, root: &Path) -> Arc<Session> {
        let child = Command::new("sleep")
            .arg("60")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("sleep ha d'existir en unix");
        let s = Session::start(Proto::Lsp, language, root, "dummy", child);
        s.ready.store(true, Ordering::SeqCst);
        s
    }

    #[cfg(unix)]
    #[test]
    fn limit_1_i_canvi_de_llenguatge_mata_lanterior() {
        let _g = TEST_LOCK.lock().unwrap();
        kill_all();
        let root = PathBuf::from("/tmp");
        let a = dummy_session("rust", &root);
        swap(a.clone());
        assert!(a.is_alive());
        assert_eq!(current().unwrap().language, "rust");
        // Canvi de llenguatge: la vella mor i la nova mana.
        let b = dummy_session("php", &root);
        swap(b.clone());
        assert!(!a.is_alive(), "la sessió anterior ha de morir en canviar");
        assert!(b.is_alive());
        assert_eq!(current().unwrap().language, "php");
        // Canvi de projecte (mateixa llengua): també mata.
        let c = dummy_session("php", Path::new("/tmp/altre"));
        swap(c.clone());
        assert!(!b.is_alive());
        kill_all();
        assert!(!c.is_alive());
        assert!(current().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn inactivitat_mata_la_sessio_i_touch_la_salva() {
        let _g = TEST_LOCK.lock().unwrap();
        kill_all();
        let s = dummy_session("rust", Path::new("/tmp"));
        swap(s.clone());
        // Consulta recent: amb un límit gran NO mor.
        s.touch();
        reap_idle(Duration::from_secs(3600));
        assert!(s.is_alive());
        // Sense consultes des de fa «massa»: el vigilant la mata.
        {
            let mut l = s.last.lock().unwrap();
            *l = Instant::now() - Duration::from_secs(200);
        }
        reap_idle(IDLE_LIMIT);
        assert!(!s.is_alive(), "120 s sense consultes ha de matar la sessió");
        assert!(current().is_none());
        kill_all();
    }

    #[test]
    fn active_for_no_spawna_i_respecta_llenguatge_i_arrel() {
        let _g = TEST_LOCK.lock().unwrap();
        kill_all();
        let root = PathBuf::from("/tmp");
        // Sense sessió: mai no en crea una (consulta barata per a l'agent).
        assert!(active_for(&root, Path::new("/tmp/a.rs")).is_none());
        #[cfg(unix)]
        {
            let s = dummy_session("rust", &root);
            swap(s.clone());
            assert!(active_for(&root, Path::new("/tmp/a.rs")).is_some());
            // Altre llenguatge o altra arrel: no és la seua sessió.
            assert!(active_for(&root, Path::new("/tmp/a.php")).is_none());
            assert!(active_for(Path::new("/tmp/altre"), Path::new("/tmp/a.rs")).is_none());
            kill_all();
        }
    }

    #[test]
    fn status_honest_sense_sessio() {
        let _g = TEST_LOCK.lock().unwrap();
        kill_all();
        let (server, language, ready) = status(Path::new("/tmp"));
        assert!(server.is_none() && language.is_none() && !ready);
    }

    /// INTEGRACIÓ REAL amb el tsserver del VSCode instal·lat (soft-skip si
    /// no hi és): open + geterr d'un TS amb un error deliberat de tipus.
    #[test]
    fn tsserver_real_diagnostica_un_error_de_tipus() {
        let _g = TEST_LOCK.lock().unwrap();
        kill_all();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        if tsserver::detect(&root).is_none() {
            eprintln!("tsserver no disponible en aquesta màquina: test omès");
            return;
        }
        let file = root.join("error.ts");
        // Error deliberat: cadena assignada a number (TS2322).
        std::fs::write(&file, "export const x: number = \"això no és un número\";\n").unwrap();
        let sess = match ensure_session(&root, &file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("tsserver no ha arrencat ({e}): test omès");
                return;
            }
        };
        assert_eq!(sess.proto, Proto::Ts);
        let diags = sess.diagnostics(&file, Duration::from_secs(30)).unwrap();
        kill_all();
        assert!(
            diags.iter().any(|d| d.severity == "error" && d.message.contains("2322")),
            "el tsserver real ha de detectar TS2322: {diags:?}"
        );
        assert!(diags.iter().all(|d| d.source.contains("tsserver")));
    }
}