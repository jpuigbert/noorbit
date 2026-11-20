//! Client MCP per al plugin oficial d'Unreal Engine (ModelContextProtocol,
//! UE 5.8+) — transport «Streamable HTTP» a `http://127.0.0.1:8000/mcp`.
//!
//! El protocol és JSON-RPC 2.0: `initialize` → `notifications/initialized` →
//! `tools/list` → `tools/call`. El servidor pot respondre amb JSON pla o amb
//! `text/event-stream` (SSE), i demanar un `mcp-session-id` que cal tornar a
//! cada petició següent. Aquí ho suportem tot; el parser d'SSE és una funció
//! pura per poder-la provar sense servidor.
//!
//! Lliçó apresa del pont Remote Control: MAI donar una crida per bona només
//! perquè la resposta HTTP no porta error — aquí exigim una resposta JSON-RPC
//! amb l'`id` esperat, i si el servidor contesta amb `isError` o un error
//! JSON-RPC, retornem Err (mai «èxit fals»).

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub const MCP_DEFAULT_PORT: u16 = crate::ports::UNREAL_MCP_PORT;
/// Protocol MCP que declara el client (el servidor negocia el seu).
const PROTOCOL_VERSION: &str = "2025-06-18";
/// Les crides d'eines poden trigar minuts dins de l'editor (imports, builds).
const TOOL_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCallResult {
    /// Text llegible del resultat (content[].text o structuredContent).
    pub text: String,
    /// El servidor ha marcat la crida com a error.
    pub is_error: bool,
}

/// Recull TOTS els missatges JSON de les línies `data: …` d'un cos SSE
/// (poden arribar pings abans de la resposta; el caller tria pel `id`).
pub fn sse_messages(body: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        let data = line
            .strip_prefix("data:")
            .map(str::trim)
            .unwrap_or_default();
        if data.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
            out.push(v);
        }
    }
    out
}

/// Primer missatge JSON d'un cos SSE (per als casos d'una sola resposta).
pub fn parse_sse_json(body: &str) -> Option<serde_json::Value> {
    sse_messages(body).into_iter().next()
}

/// Client del servidor MCP d'Unreal. Crea un de nou per a cada conversa
/// (el `session_id` viu aquí dins; no és compartible entre clients).
pub struct McpClient {
    base: String,
    http: reqwest::Client,
    session: Mutex<Option<String>>,
    next_id: AtomicU64,
}

impl McpClient {
    pub fn new(port: u16) -> Self {
        Self {
            base: format!("http://127.0.0.1:{}/mcp", port),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(TOOL_TIMEOUT_SECS))
                .build()
                .unwrap_or_default(),
            session: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    /// Diu si el port del servidor MCP està obert (TCP).
    pub async fn port_open(port: u16) -> bool {
        tokio::time::timeout(
            std::time::Duration::from_millis(600),
            tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)),
        )
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
    }

    fn take_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Una petició JSON-RPC. Accepta resposta JSON pla o SSE, desa el
    /// `mcp-session-id` que el servidor pugui retornar i l'envia a les
    /// peticions següents. Sense resposta amb l'`id` esperat: Err.
    async fn request(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let id = self.take_id();
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut req = self
            .http
            .post(&self.base)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("User-Agent", "NoOrbit");
        if let Some(sid) = self.session.lock().unwrap_or_else(|g| g.into_inner()).clone() {
            req = req.header("mcp-session-id", sid);
        }
        let resp = req
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("El servidor MCP d'Unreal no respon (port {}): {}", self.port(), e))?;
        if let Some(sid) = resp.headers().get("mcp-session-id") {
            if let Ok(v) = sid.to_str() {
                *self.session.lock().unwrap_or_else(|g| g.into_inner()) = Some(v.to_string());
            }
        }
        if !resp.status().is_success() {
            return Err(anyhow!("El servidor MCP ha respost {}", resp.status()));
        }
        let ct = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_lowercase)
            .unwrap_or_default();
        let text = resp.text().await?;
        let v: serde_json::Value = if ct.contains("text/event-stream") {
            sse_messages(&text)
                .into_iter()
                .find(|m| m.get("id").and_then(|i| i.as_u64()) == Some(id))
                .ok_or_else(|| anyhow!("Resposta SSE del servidor MCP sense missatge per a la petició {}", method))?
        } else {
            serde_json::from_str(&text)
                .map_err(|e| anyhow!("El servidor MCP no ha respost JSON vàlid: {}", e))?
        };
        if let Some(err) = v.get("error") {
            return Err(anyhow!(
                "El servidor MCP ha donat error a «{}»: {}",
                method,
                err.get("message").and_then(|m| m.as_str()).unwrap_or("desconegut")
            ));
        }
        if v.get("id").and_then(|i| i.as_u64()) != Some(id) {
            return Err(anyhow!("El servidor MCP no ha respost a la petició {}", method));
        }
        Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null))
    }

    /// Handshake complet: `initialize` + `notifications/initialized`.
    /// Sense això, `tools/list` del servidor contesta amb error.
    pub async fn connect(&self) -> Result<serde_json::Value> {
        let info = self
            .request(
                "initialize",
                serde_json::json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "noorbit", "version": env!("CARGO_PKG_VERSION") },
                }),
            )
            .await?;
        // Notificació (sense id, sense resposta esperada).
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {},
        });
        let mut req = self
            .http
            .post(&self.base)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("User-Agent", "NoOrbit");
        if let Some(sid) = self.session.lock().unwrap_or_else(|g| g.into_inner()).clone() {
            req = req.header("mcp-session-id", sid);
        }
        // La resposta a una notificació pot ser 202 o buida: es descarta.
        let _ = req.json(&body).send().await;
        Ok(info)
    }

    /// Catàleg d'eines que exposa l'editor (crear actors, editar BPs…).
    pub async fn tools_list(&self) -> Result<Vec<McpTool>> {
        let res = self.request("tools/list", serde_json::json!({})).await?;
        let tools = res
            .get("tools")
            .and_then(|t| t.as_array())
            .ok_or_else(|| anyhow!("tools/list no ha retornat una llista d'eines"))?;
        serde_json::from_value(serde_json::Value::Array(tools.clone()))
            .map_err(|e| anyhow!("Eines MCP amb format inesperat: {}", e))
    }

    /// Executa una eina MCP. El resultat arriba com `content[]` de text
    /// (o `structuredContent`) i pot portar `isError`.
    pub async fn tools_call(&self, name: &str, arguments: serde_json::Value) -> Result<McpCallResult> {
        let res = self
            .request(
                "tools/call",
                serde_json::json!({ "name": name, "arguments": arguments }),
            )
            .await?;
        let is_error = res.get("isError").and_then(|b| b.as_bool()).unwrap_or(false);
        let text = if let Some(content) = res.get("content").and_then(|c| c.as_array()) {
            content
                .iter()
                .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        } else if let Some(sc) = res.get("structuredContent") {
            sc.to_string()
        } else {
            String::new()
        };
        if is_error && text.is_empty() {
            return Err(anyhow!("L'eina «{}» ha fallat dins de l'editor d'Unreal", name));
        }
        Ok(McpCallResult { text, is_error })
    }

    /// Port pel qual aquest client parla (per als missatges d'error).
    fn port(&self) -> u16 {
        self.base
            .rsplit(':')
            .next()
            .and_then(|s| s.trim_end_matches("/mcp").parse().ok())
            .unwrap_or(MCP_DEFAULT_PORT)
    }
}

/// Client connectat i catàleg d'eines ja descarregat (per l'agent i la UI).
pub struct McpSession {
    pub client: McpClient,
    pub tools: Vec<McpTool>,
    pub server_info: serde_json::Value,
}

/// Connecta i baixa el catàleg — el punt d'entrada únic per a tots els
/// fluxos. Cap cau: la connexió i el catàleg són barats (uns pocs HTTP),
/// i cap flux fa polling sobre això.
pub async fn open_session() -> Result<McpSession> {
    let client = McpClient::new(MCP_DEFAULT_PORT);
    let server_info = client.connect().await?;
    let tools = client.tools_list().await?;
    Ok(McpSession { client, tools, server_info })
}

#[cfg(test)]
mod proves {
    use super::*;

    #[test]
    fn parse_sse_troba_el_missatge_entre_pings() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n\n\
                    : comentari\n\
                    data: {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n";
        let v = parse_sse_json(body).expect("ha de trobar el primer JSON");
        assert_eq!(v["id"], 1);
        assert_eq!(v["result"]["ok"], true);
    }

    #[test]
    fn sse_recull_tots_els_missatges_pel_id() {
        let body = "data: {\"jsonrpc\":\"2.0\",\"id\":null,\"method\":\"ping\"}\n\
                    data: {\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"tools\":[]}}\n";
        let msgs = sse_messages(body);
        assert_eq!(msgs.len(), 2);
        let resp = msgs.iter().find(|m| m.get("id").and_then(|i| i.as_u64()) == Some(3));
        assert!(resp.is_some());
        assert!(resp.unwrap().get("result").is_some());
    }

    #[test]
    fn parse_sse_ignora_dades_no_json() {
        let body = "data: [DONE]\n\ndata: not-json\n\n";
        assert!(parse_sse_json(body).is_none());
    }

    #[test]
    fn parse_sse_accepta_espais_despres_dels_dos_punts() {
        let body = "data:    {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":null}\n";
        let v = parse_sse_json(body).expect("ha de parir amb espais extra");
        assert_eq!(v["id"], 7);
    }

    #[test]
    fn parse_sse_buit_retorna_res() {
        assert!(parse_sse_json("").is_none());
        assert!(parse_sse_json("event: message\n").is_none());
    }

    #[test]
    fn parse_sse_retorna_la_darrera_resposta_que_es_troba() {
        let body = "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"primera\"}\n\
                    data: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":\"segona\"}\n";
        // El client MCP només envia UNA petició per resposta HTTP: la primera
        // línia `data:` vàlida és la resposta (els pings no porten result).
        let v = parse_sse_json(body).expect("ha de trobar la primera");
        assert_eq!(v["result"], "primera");
    }
}
