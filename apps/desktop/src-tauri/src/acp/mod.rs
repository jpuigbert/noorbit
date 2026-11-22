//! Client ACP (Agent Client Protocol) per al motor de GitHub Copilot.
//!
//! ACP és JSON-RPC 2.0 transportat en **NDJSON** (una línia JSON per
//! missatge) sobre l'entrada i la sortida estàndard del procés
//! `copilot --acp --stdio`. NoOrbit fa de CLIENT: obre la sessió
//! (`initialize` + `session/new`), mana el prompt (`session/prompt`) i atén
//! les peticions que l'agent fa al client:
//!
//! - `fs/read_text_file`  → llegim dins l'espai de treball del projecte.
//! - `fs/write_text_file` → escrivim dins l'espai de treball del projecte.
//! - `session/request_permission` → ho preguntem a l'usuari (la interfície
//!   mostra la petició i ens contesta amb el botó d'aprovar o rebutjar).
//!
//! El protocol és el mateix que usen altres IDE (Zed, etc.), per això el
//! client viu separat del motor de xat: `ai::AiManager` només consumeix els
//! esdeveniments que arriben.

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

/// Identificador del motor dins de NoOrbit (el que veu la UI).
pub const PROVIDER_ID: &str = "copilot";
/// Nom que es mostra a l'interfície.
pub const PROVIDER_NAME: &str = "GitHub Copilot";
/// Versió del protocol ACP que parlem.
pub const PROTOCOL_VERSION: u32 = 1;

/// Temps que esperem la resposta de l'usuari a una petició de permís.
const PERMISSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

// ── Esdeveniments que l'agent envia al client ───────────────────────────────

/// Allò que l'agent de Copilot ens va comunicant durant un torn.
#[derive(Debug, Clone, PartialEq)]
pub enum AcpEvent {
    /// Fragment del RAONAMENT de l'agent (no és la resposta final).
    Thought(String),
    /// Fragment de la RESPOSTA final (aquest és el text que veu l'usuari).
    Message(String),
    /// L'agent està usant una eina (llegeix un fitxer, executa una ordre…).
    Tool {
        id: String,
        title: String,
        kind: String,
        status: String,
        text: String,
    },
    /// Avís o error del servidor ACP.
    Notice(String),
}

// ── Missatges JSON-RPC que arriben del servidor ─────────────────────────────

/// Un missatge rebut per NDJSON, ja classificat.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// Resposta a una petició nostra.
    Response { id: Value, ok: bool, payload: Value },
    /// Notificació de l'agent (`session/update`…).
    Notification { method: String, params: Value },
    /// Peticio de l'agent al client (`fs/read_text_file`…).
    Request { id: Value, method: String, params: Value },
}

/// Classifica una línia NDJSON. `None` si no és JSON-RPC vàlid (línia en
/// blanc, sorra del CLI, etc.): el soroll no ha de fer caure la sessió.
pub fn parse_line(line: &str) -> Option<Incoming> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if !v.is_object() {
        return None;
    }
    let method = v
        .get("method")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let params = v.get("params").cloned().unwrap_or(Value::Null);
    let id = v.get("id").cloned().filter(|i| !i.is_null());
    match (method, id) {
        (Some(method), Some(id)) => Some(Incoming::Request { id, method, params }),
        (Some(method), None) => Some(Incoming::Notification { method, params }),
        (None, Some(id)) => {
            if let Some(err) = v.get("error") {
                Some(Incoming::Response {
                    id,
                    ok: false,
                    payload: err.clone(),
                })
            } else {
                Some(Incoming::Response {
                    id,
                    ok: true,
                    payload: v.get("result").cloned().unwrap_or(Value::Null),
                })
            }
        }
        (None, None) => None,
    }
}

/// Clau textual d'un id JSON-RPC (per a la taula de peticions pendents).
fn id_key(id: &Value) -> String {
    match id {
        Value::String(s) => format!("s:{s}"),
        other => other.to_string(),
    }
}

/// Text d'un bloc de contingut d'ACP.
fn block_text(block: &Value) -> String {
    match block.get("type").and_then(Value::as_str) {
        Some("text") => block
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        Some("diff") => {
            let path = block.get("path").and_then(Value::as_str).unwrap_or("");
            let nou = block.get("newText").and_then(Value::as_str).unwrap_or("");
            if path.is_empty() {
                nou.to_string()
            } else {
                format!("{path}\n{nou}")
            }
        }
        _ => String::new(),
    }
}

/// Text d'un camp `content` d'ACP: pot ser un bloc simple o una llista de
/// blocs (cada element pot ser `content`, `diff`…).
fn content_text(content: &Value) -> String {
    match content {
        Value::Array(items) => items
            .iter()
            .map(block_text)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(_) => {
            let inner = content.get("content");
            match inner {
                Some(v) if v.is_object() || v.is_array() => content_text(v),
                _ => block_text(content),
            }
        }
        _ => String::new(),
    }
}

/// Converteix els paràmetres d'una notificació `session/update` en un
/// esdeveniment per a la interfície.
pub fn update_event(params: &Value) -> Option<AcpEvent> {
    let update = params.get("update")?;
    let kind = update.get("sessionUpdate")?.as_str()?;
    match kind {
        "agent_message_chunk" => {
            let t = update.get("content").map(content_text).unwrap_or_default();
            (!t.is_empty()).then_some(AcpEvent::Message(t))
        }
        "agent_thought_chunk" => {
            let t = update.get("content").map(content_text).unwrap_or_default();
            (!t.is_empty()).then_some(AcpEvent::Thought(t))
        }
        "tool_call" | "tool_call_update" => Some(AcpEvent::Tool {
            id: update
                .get("toolCallId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            title: update
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            kind: update
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            status: update
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            text: update.get("content").map(content_text).unwrap_or_default(),
        }),
        "session_notice" => {
            let t = update.get("content").map(content_text).unwrap_or_default();
            (!t.is_empty()).then_some(AcpEvent::Notice(t))
        }
        _ => None,
    }
}

// ── Fitxers: tot dins de l'espai de treball ─────────────────────────────────

/// Resol un camí dins del root de la sessió. `None` si surt de l'espai de
/// treball (camí absolut exterior, «..», enllaç simbòlic cap fora…).
pub fn resolve_in_root(root: &Path, raw: &str) -> Option<PathBuf> {
    if raw.trim().is_empty() {
        return None;
    }
    let p = Path::new(raw);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    };
    // Normalització lexica: treiem «..» i «.» sense tocar el disc.
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    if !(out == *root || out.starts_with(root)) {
        return None;
    }
    // Seguritat extra: el pare REAL (després de seguir enllaços simbòlics)
    // ha de continuar dins del root.
    if let Some(parent) = out.parent() {
        if let Ok(real) = parent.canonicalize() {
            if !real.starts_with(root) {
                return None;
            }
        }
    }
    Some(out)
}

/// Llegeix un fitxer (o una porció: des de la línia `line`, `limit` línies).
fn read_slice(
    path: &Path,
    line: Option<u64>,
    limit: Option<u64>,
) -> std::result::Result<String, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("No s'ha pogut llegir {}: {e}", path.display()))?;
    if line.is_none() && limit.is_none() {
        return Ok(text);
    }
    let skip = line.unwrap_or(1).saturating_sub(1) as usize;
    let take = limit.unwrap_or(u64::MAX) as usize;
    Ok(text
        .lines()
        .skip(skip)
        .take(take)
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Escriu un fitxer (creant les carpetes que calguin).
fn write_text_file(path: &Path, content: &str) -> std::result::Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("No s'ha pogut crear {}: {e}", parent.display()))?;
    }
    std::fs::write(path, content)
        .map_err(|e| format!("No s'ha pogut escriure {}: {e}", path.display()))
}

// ── Permisos: l'agent pregunta, l'usuari respon ────────────────────────────

/// Peticions de permís pendents de resposta, indexades per la clau de
/// l'id JSON-RPC. La interfície hi respon amb el comandament
/// `acp_permission_reply`.
static PERMISSIONS: Lazy<Mutex<HashMap<String, std_mpsc::Sender<Option<String>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Contesta una petició de permís. `option_id` és l'opció triada per
/// l'usuari; `None` (o cap resposta) rebutja l'operació.
pub fn answer_permission(id: &str, option_id: Option<String>) -> bool {
    let tx = PERMISSIONS.lock().ok().and_then(|mut g| g.remove(id));
    match tx {
        Some(tx) => tx.send(option_id).is_ok(),
        None => false,
    }
}

// ── Client ACP ──────────────────────────────────────────────────────────────

/// Estat compartit entre el client i el fil que llegeix la sortida.
struct Shared {
    writer: Mutex<std::process::ChildStdin>,
    next_id: AtomicI64,
    pending: Mutex<HashMap<String, oneshot::Sender<std::result::Result<Value, String>>>>,
    root: PathBuf,
}

impl Shared {
    /// Escriu un missatge NDJSON (JSON + salt de línia) i buida el buffer.
    fn write_msg(&self, msg: &Value) -> Result<()> {
        let mut line = serde_json::to_string(msg)?;
        line.push('\n');
        let mut w = self
            .writer
            .lock()
            .map_err(|_| anyhow!("La connexió ACP amb Copilot està trencada"))?;
        w.write_all(line.as_bytes())
            .map_err(|e| anyhow!("No s'ha pogut escriure a Copilot: {e}"))?;
        let _ = w.flush();
        Ok(())
    }

    /// Peticio JSON-RPC que espera la resposta.
    async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let key = id_key(&json!(id));
        let (tx, rx) = oneshot::channel();
        {
            let mut g = self
                .pending
                .lock()
                .map_err(|_| anyhow!("La connexió ACP amb Copilot està trencada"))?;
            g.insert(key, tx);
        }
        if let Err(e) = self.write_msg(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        })) {
            if let Ok(mut g) = self.pending.lock() {
                g.remove(&id_key(&json!(id)));
            }
            return Err(e);
        }
        match rx.await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(anyhow!(e)),
            Err(_) => Err(anyhow!("Copilot ha tancat la connexió ACP")),
        }
    }

    /// Notificació JSON-RPC (sense resposta).
    fn notify(&self, method: &str, params: Value) {
        let _ = self.write_msg(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }
}

/// Client ACP connectat a un agent (el CLI de GitHub Copilot).
pub struct AcpClient {
    shared: Arc<Shared>,
    events: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AcpEvent>>>,
    /// Només un torn alhora per sessió: dos prompts alhora es barrejarien.
    torn: tokio::sync::Mutex<()>,
    session_id: Mutex<Option<String>>,
    child: Mutex<Option<std::process::Child>>,
}

impl Drop for AcpClient {
    fn drop(&mut self) {
        // En tancar el client, l'agent de Copilot també s'ha d'acabar.
        if let Ok(mut g) = self.child.lock() {
            if let Some(mut c) = g.take() {
                let _ = c.kill();
            }
        }
    }
}

impl AcpClient {
    /// Arrenca el procés de l'agent i connecta els canals.
    pub fn spawn(
        mut cmd: std::process::Command,
        root: PathBuf,
        app: Option<tauri::AppHandle>,
    ) -> Result<Self> {
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow!("No s'ha pogut arrencar el motor de Copilot: {e}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Copilot sense entrada estàndard"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Copilot sense sortida estàndard"))?;
        let stderr = child.stderr.take();
        let root = root.canonicalize().unwrap_or(root);
        let shared = Arc::new(Shared {
            writer: Mutex::new(stdin),
            next_id: AtomicI64::new(1),
            pending: Mutex::new(HashMap::new()),
            root,
        });
        let (events_tx, events_rx) = mpsc::unbounded_channel();

        // L'error estàndard del CLI NO és el canal del protocol: el llegim
        // en un fil a part perquè el soroll no barregui el NDJSON.
        if let Some(err) = stderr {
            std::thread::spawn(move || {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    eprintln!("[copilot] {line}");
                }
            });
        }

        // Fil lector: classifica cada missatge i el reparteix.
        let shared_thread = shared.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(msg) = parse_line(&line) {
                    handle_message(&shared_thread, &events_tx, msg, app.clone());
                }
            }
            // EOF: el procés ha sortit.
            let _ = events_tx.send(AcpEvent::Notice(
                "Copilot ha tancat la connexió ACP.".into(),
            ));
        });

        Ok(Self {
            shared,
            events: Arc::new(tokio::sync::Mutex::new(events_rx)),
            torn: tokio::sync::Mutex::new(()),
            session_id: Mutex::new(None),
            child: Mutex::new(Some(child)),
        })
    }

    /// `initialize` + `session/new`: deixa el client a punt per a un prompt.
    pub async fn start_session(&self, cwd: &Path) -> Result<String> {
        self.shared
            .call(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "clientCapabilities": {
                        "fs": { "readTextFile": true, "writeTextFile": true }
                    }
                }),
            )
            .await
            .map_err(|e| anyhow!("Copilot no ha sortit del «initialize»: {e}"))?;
        // Els MCP de NoOrbit són HTTP (Unreal, Blender) i ACP demana mcpServers
        // stdio: no els ofereixem encara, així que la llista és buida.
        let sess = self
            .shared
            .call(
                "session/new",
                json!({
                    "cwd": cwd.to_string_lossy(),
                    "mcpServers": []
                }),
            )
            .await
            .map_err(|e| anyhow!("Copilot no ha obert la sessió: {e}"))?;
        let sid = sess
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Copilot no ha retornat cap identificador de sessió"))?
            .to_string();
        if let Ok(mut g) = self.session_id.lock() {
            *g = Some(sid.clone());
        }
        Ok(sid)
    }

    /// Identificador de la sessió actual (si ja s'ha obert).
    pub fn session_id(&self) -> Option<String> {
        self.session_id.lock().ok().and_then(|g| g.clone())
    }

    /// Avisa l'agent que el torn s'ha cancel·lat.
    pub fn cancel(&self, session_id: &str) {
        self.shared
            .notify("session/cancel", json!({ "sessionId": session_id }));
    }

    /// Mana el prompt i espera que acabi el torn, retornant el text final.
    /// Cada esdeveniment de l'agent es passa a `on_event` (per a l'streaming
    /// cap a la interfície).
    pub async fn prompt<F>(
        &self,
        session_id: &str,
        blocks: &[Value],
        mut on_event: F,
    ) -> Result<String>
    where
        F: FnMut(AcpEvent) + Send,
    {
        // Un torn alhora per sessió.
        let _torn = self.torn.lock().await;
        let peticio =
            self.shared
                .call("session/prompt", json!({ "sessionId": session_id, "prompt": blocks }));
        tokio::pin!(peticio);
        let mut text = String::new();
        loop {
            tokio::select! {
                res = &mut peticio => {
                    return match res {
                        Ok(_) => Ok(text),
                        Err(e) => Err(e),
                    };
                }
                ev = self.next_event() => match ev {
                    Some(AcpEvent::Message(t)) => {
                        text.push_str(&t);
                        on_event(AcpEvent::Message(t));
                    }
                    Some(altra) => on_event(altra),
                    None => {
                        // La connexió s'ha tancat: el prompt no retornarà res.
                        return Err(anyhow!("Copilot ha tancat la connexió ACP"));
                    }
                },
            }
        }
    }

    /// Espera el següent esdeveniment de l'agent.
    async fn next_event(&self) -> Option<AcpEvent> {
        let mut rx = self.events.lock().await;
        rx.recv().await
    }
}

// ── Repartiment dels missatges ──────────────────────────────────────────────

/// Sessions ACP viues, indexades pel xat de NoOrbit (perquè el fil del
/// diàleg es conservi entre torns).
struct SessionEntry {
    client: Arc<AcpClient>,
}

static SESSIONS: Lazy<Mutex<HashMap<String, SessionEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Client ja connectat per a aquest xat (si n'hi ha).
pub fn existing_session(key: &str) -> Option<Arc<AcpClient>> {
    SESSIONS.lock().ok()?.get(key).map(|e| e.client.clone())
}

/// Descarta la sessió (per quan l'agent mor o l'usuari tanca el xat).
pub fn forget(key: &str) {
    if let Ok(mut g) = SESSIONS.lock() {
        g.remove(key);
    }
}

/// Obre una sessió ACP nova per a un xat i la desa al registre.
pub async fn connect(
    key: &str,
    cmd: std::process::Command,
    root: PathBuf,
    app: tauri::AppHandle,
) -> Result<Arc<AcpClient>> {
    let client = Arc::new(AcpClient::spawn(cmd, root.clone(), Some(app))?);
    client.start_session(&root).await?;
    if let Ok(mut g) = SESSIONS.lock() {
        g.insert(
            key.to_string(),
            SessionEntry {
                client: client.clone(),
            },
        );
    }
    Ok(client)
}

/// Text d'error d'una resposta JSON-RPC fallida.
fn error_message(payload: &Value) -> String {
    payload
        .get("message")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .unwrap_or_else(|| payload.to_string())
}

/// Respon una petició de l'agent.
fn respond(shared: &Shared, id: &Value, result: std::result::Result<Value, String>) {
    let msg = match result {
        Ok(v) => json!({ "jsonrpc": "2.0", "id": id, "result": v }),
        Err(e) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32603, "message": e }
        }),
    };
    let _ = shared.write_msg(&msg);
}

/// Reparteix un missatge rebut: resposta, notificació o petició.
fn handle_message(
    shared: &Arc<Shared>,
    events_tx: &mpsc::UnboundedSender<AcpEvent>,
    msg: Incoming,
    app: Option<tauri::AppHandle>,
) {
    match msg {
        Incoming::Response { id, ok, payload } => {
            let key = id_key(&id);
            let sender = shared
                .pending
                .lock()
                .ok()
                .and_then(|mut g| g.remove(&key));
            if let Some(tx) = sender {
                let _ = tx.send(if ok {
                    Ok(payload)
                } else {
                    Err(error_message(&payload))
                });
            }
        }
        Incoming::Notification { method, params } => {
            if method == "session/update" {
                if let Some(ev) = update_event(&params) {
                    let _ = events_tx.send(ev);
                }
            }
        }
        Incoming::Request { id, method, params } => match method.as_str() {
            "fs/read_text_file" => {
                let path = params.get("path").and_then(Value::as_str).unwrap_or("");
                let line = params.get("line").and_then(Value::as_u64);
                let limit = params.get("limit").and_then(Value::as_u64);
                let res = match resolve_in_root(&shared.root, path) {
                    Some(p) => read_slice(&p, line, limit).map(|content| json!({ "content": content })),
                    None => Err(format!(
                        "El camí «{path}» no és dins de l'espai de treball del projecte"
                    )),
                };
                respond(shared, &id, res);
            }
            "fs/write_text_file" => {
                let path = params.get("path").and_then(Value::as_str).unwrap_or("");
                let content = params
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let res = match resolve_in_root(&shared.root, path) {
                    Some(p) => write_text_file(&p, content).map(|()| json!({})),
                    None => Err(format!(
                        "El camí «{path}» no és dins de l'espai de treball del projecte"
                    )),
                };
                respond(shared, &id, res);
            }
            "session/request_permission" => {
                request_permission(shared, &id, &params, app.as_ref());
            }
            _ => respond(
                shared,
                &id,
                Err(format!("Mètode ACP no suportat per NoOrbit: {method}")),
            ),
        },
    }
}

/// L'agent demana permís: ho passem a la interfície i esperem la resposta.
fn request_permission(shared: &Shared, id: &Value, params: &Value, app: Option<&tauri::AppHandle>) {
    let key = id_key(id);
    let (tx, rx) = std_mpsc::channel::<Option<String>>();
    if let Ok(mut g) = PERMISSIONS.lock() {
        g.insert(key.clone(), tx);
    }
    let tool = params.get("toolCall");
    let title = tool
        .and_then(|t| t.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let kind = tool
        .and_then(|t| t.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let options: Vec<Value> = params
        .get("options")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(app) = app {
        use tauri::Emitter;
        let _ = app.emit(
            "agent://acp-permission",
            json!({
                "id": key,
                "title": title,
                "kind": kind,
                "options": options,
            }),
        );
    }
    // Si l'usuari no contesta, rebutjem (mai no s'aprova en silenci).
    let answer = rx.recv_timeout(PERMISSION_TIMEOUT).ok().flatten();
    if let Ok(mut g) = PERMISSIONS.lock() {
        g.remove(&key);
    }
    let result = match answer {
        Some(option_id) => json!({
            "outcome": { "outcome": "selected", "optionId": option_id }
        }),
        None => json!({ "outcome": { "outcome": "cancelled" } }),
    };
    let _ = shared.write_msg(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
}

// ── Proves ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_line_ho_les_tres_formes_de_missatge() {
        let resp = parse_line(r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#).unwrap();
        match resp {
            Incoming::Response { id, ok, payload } => {
                assert_eq!(id, json!(1));
                assert!(ok);
                assert_eq!(payload["ok"], json!(true));
            }
            altra => panic!("s'esperava una resposta: {altra:?}"),
        }
        let noti = parse_line(r#"{"jsonrpc":"2.0","method":"session/update","params":{}}"#).unwrap();
        assert_eq!(
            noti,
            Incoming::Notification {
                method: "session/update".into(),
                params: json!({})
            }
        );
        let pet = parse_line(
            r#"{"jsonrpc":"2.0","id":7,"method":"fs/read_text_file","params":{"path":"a"}}"#,
        )
        .unwrap();
        match pet {
            Incoming::Request { id, method, params } => {
                assert_eq!(id, json!(7));
                assert_eq!(method, "fs/read_text_file");
                assert_eq!(params["path"], json!("a"));
            }
            altra => panic!("s'esperava una peticio: {altra:?}"),
        }
    }

    #[test]
    fn parse_line_ignora_el_soroll() {
        assert!(parse_line("").is_none());
        assert!(parse_line("Copilot CLI v1.0.114").is_none());
        assert!(parse_line(r#"{"jsonrpc":"2.0"}"#).is_none());
    }

    #[test]
    fn el_fragment_de_missatge_arriba_com_a_text() {
        let params = json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "Hola" }
            }
        });
        assert_eq!(
            update_event(&params),
            Some(AcpEvent::Message("Hola".into()))
        );
    }

    #[test]
    fn el_raonament_va_apart_del_text_final() {
        let params = json!({
            "update": {
                "sessionUpdate": "agent_thought_chunk",
                "content": { "type": "text", "text": "penso" }
            }
        });
        assert_eq!(update_event(&params), Some(AcpEvent::Thought("penso".into())));
    }

    #[test]
    fn la_cridada_de_eina_es_reconeix() {
        let params = json!({
            "update": {
                "sessionUpdate": "tool_call",
                "toolCallId": "call_1",
                "title": "Llegint la configuracio",
                "kind": "read",
                "status": "pending"
            }
        });
        match update_event(&params) {
            Some(AcpEvent::Tool {
                id, title, kind, status, ..
            }) => {
                assert_eq!(id, "call_1");
                assert_eq!(title, "Llegint la configuracio");
                assert_eq!(kind, "read");
                assert_eq!(status, "pending");
            }
            altra => panic!("s'esperava una eina: {altra:?}"),
        }
    }

    #[test]
    fn els_camins_fora_de_l_espai_no_passen() {
        let root = Path::new("/tmp/projecte");
        assert_eq!(
            resolve_in_root(root, "src/main.rs"),
            Some(PathBuf::from("/tmp/projecte/src/main.rs"))
        );
        assert_eq!(
            resolve_in_root(root, "/tmp/projecte/src/main.rs"),
            Some(PathBuf::from("/tmp/projecte/src/main.rs"))
        );
        assert_eq!(resolve_in_root(root, "../fora"), None);
        assert_eq!(resolve_in_root(root, "/etc/passwd"), None);
        assert_eq!(resolve_in_root(root, ""), None);
    }

    #[test]
    fn es_llegeix_una_porcio_del_fitxer() {
        let dir = tempfile::tempdir().unwrap();
        let fitxer = dir.path().join("a.txt");
        std::fs::write(&fitxer, "un\ndos\ntres\nquatre\n").unwrap();
        let tot = read_slice(&fitxer, None, None).unwrap();
        assert_eq!(tot, "un\ndos\ntres\nquatre\n");
        let dos = read_slice(&fitxer, Some(2), Some(2)).unwrap();
        assert_eq!(dos, "dos\ntres");
    }

    #[test]
    fn el_permis_rebutat_es_cancela() {
        // Cap petició viva: la resposta és falsa, però no falla mai.
        assert!(!answer_permission("no-existeix", Some("allow-once".into())));
    }
}
