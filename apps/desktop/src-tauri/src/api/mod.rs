//! Proveïdors d'IA en línia alternatius (APIs basades en token).
//!
//! Cada integració és un punt final compatible amb l'API d'OpenAI
//! (`POST {base_url}/chat/completions`), de manera que funciona amb proveïdors
//! d'avui (OpenAI, Groq, Together, Mistral, OpenRouter, LM Studio, llama.cpp
//! servidor…) i amb els que apareguin demà, sense canviar codi: només cal
//! registrar-ne la URL, el model i el token.
//!
//! Els tokens es desen a la carpeta de dades de l'aplicació i NO surten mai al
//! frontend en text pla: la llista els emmascara.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Com s'adjunta el token a les peticions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthScheme {
    /// Capçalera  Authorization: Bearer <token>  (estàndard OpenAI i compatibles)
    #[default]
    Bearer,
    /// Capçalera amb nom custom (p.ex. "x-api-key")
    #[serde(rename = "header")]
    ApiKeyHeader,
    /// Sense autenticació
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProvider {
    pub id: String,
    pub name: String,
    /// Per exemple https://api.openai.com/v1
    pub base_url: String,
    /// Model a usar (p.ex. gpt-4o-mini, llama-3.3-70b-versatile…)
    #[serde(default)]
    pub model: String,
    /// Format de l'API: "openai" (compatible OpenAI, per defecte) o
    /// "anthropic" (Claude). Determina com es construeix la petició.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub auth: AuthScheme,
    /// Nom de la capçalera quan auth = api_key_header
    #[serde(default)]
    pub header_name: String,
    #[serde(default)]
    pub token: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Versió sense el secret, per enviar a la UI (token emmascarat).
#[derive(Debug, Clone, Serialize)]
pub struct AiProviderPublic {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub kind: String,
    pub auth: AuthScheme,
    pub header_name: String,
    pub has_token: bool,
    pub token_masked: String,
    pub enabled: bool,
}

impl AiProvider {
    fn to_public(&self) -> AiProviderPublic {
        AiProviderPublic {
            id: self.id.clone(),
            name: self.name.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            kind: self.kind.clone(),
            auth: self.auth,
            header_name: self.header_name.clone(),
            has_token: !self.token.is_empty(),
            token_masked: mask(&self.token),
            enabled: self.enabled,
        }
    }

    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.auth {
            AuthScheme::Bearer => req.header("Authorization", format!("Bearer {}", self.token)),
            AuthScheme::ApiKeyHeader => {
                let name = if self.header_name.is_empty() {
                    "x-api-key".to_string()
                } else {
                    self.header_name.clone()
                };
                req.header(name, self.token.clone())
            }
            AuthScheme::None => req,
        }
    }

    /// Cert si aquest proveïdor usa l'API d'Anthropic (Claude) en comptes
    /// de la compatible amb OpenAI.
    pub fn is_anthropic(&self) -> bool {
        self.kind.eq_ignore_ascii_case("anthropic")
    }
}

fn mask(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() <= 8 {
        return "•".repeat(chars.len());
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{}…{}", head, tail)
}

pub struct ApiManager {
    path: PathBuf,
    pub providers: Vec<AiProvider>,
    http: reqwest::Client,
}

impl ApiManager {
    pub fn new(data_dir: PathBuf) -> Self {
        let path = data_dir.join("ai_providers.json");
        let mut providers = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<AiProvider>>(&s).ok())
            .unwrap_or_default();
        // Reserva un lloc fixe per a Venice AI si encara no n'hi ha cap
        // entrada: apareix a la llista com a «Venice AI · ⚠ sense token»
        // i només cal prémer l'editat (✏) i hi enganxar el token més tard.
        // Venice exposa una API compatible amb OpenAI, així que no cal cap
        // codi addicional: rebrà `POST {base}/chat/completions`.
        let has_venice = providers.iter().any(|p| p.id == "venice");
        let mut seeded = false;
        if !has_venice {
            providers.push(AiProvider {
                id: "venice".into(),
                name: "Venice AI".into(),
                base_url: "https://api.venice.ai/api/v1".into(),
                model: "qwen3-coder".into(),
                kind: "openai".into(),
                auth: AuthScheme::Bearer,
                header_name: String::new(),
                token: String::new(),
                enabled: true,
            });
            seeded = true;
        }
        let mgr = Self {
            path,
            providers,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
        };
        // només desem al disc si acabem de crear el lloc per a Venice.
        if seeded {
            let _ = mgr.save();
        }
        mgr
    }

    fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.providers)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<AiProviderPublic> {
        self.providers.iter().map(|p| p.to_public()).collect()
    }

    fn find(&mut self, id: &str) -> Result<&mut AiProvider> {
        self.providers
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow!("Proveïdor no trobat: {}", id))
    }

    pub fn add(&mut self, mut item: AiProvider) -> Result<AiProviderPublic> {
        if item.id.is_empty() {
            item.id = format!("prov_{}", std::time::UNIX_EPOCH.elapsed()?.as_millis());
        }
        if item.kind.trim().is_empty() {
            item.kind = "openai".into();
        }
        if matches!(item.auth, AuthScheme::None) && item.token.is_empty() {
            // per defecte,Bearer si no s'indica el contrari
            item.auth = AuthScheme::Bearer;
        }
        let pub_item = item.to_public();
        self.providers.push(item);
        self.save()?;
        Ok(pub_item)
    }

    /// Actualitza camps opcionals. Si `token` és None, conserva l'existent.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        id: &str,
        name: Option<String>,
        base_url: Option<String>,
        model: Option<String>,
        kind: Option<String>,
        auth: Option<AuthScheme>,
        header_name: Option<String>,
        token: Option<String>,
        enabled: Option<bool>,
    ) -> Result<AiProviderPublic> {
        let item = self.find(id)?;
        if let Some(v) = name {
            item.name = v;
        }
        if let Some(v) = base_url {
            item.base_url = v;
        }
        if let Some(v) = model {
            item.model = v;
        }
        if let Some(v) = kind {
            item.kind = v;
        }
        if let Some(v) = auth {
            item.auth = v;
        }
        if let Some(v) = header_name {
            item.header_name = v;
        }
        if let Some(v) = token {
            item.token = v;
        }
        if let Some(v) = enabled {
            item.enabled = v;
        }
        let pub_item = item.to_public();
        self.save()?;
        Ok(pub_item)
    }

    pub fn remove(&mut self, id: &str) -> Result<()> {
        self.providers.retain(|p| p.id != id);
        self.save()?;
        Ok(())
    }

    /// Prepara una crida: retorna una còpia del proveïdor i un client HTTP.
    /// Així no cal retenir el mutex a través de l'`await` (el guard no és `Send`).
    pub fn prepare(&self, id: &str) -> Result<(AiProvider, reqwest::Client)> {
        let item = self
            .providers
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| anyhow!("Proveïdor no trobat: {}", id))?;
        if !item.enabled {
            return Err(anyhow!("El proveïdor està desactivat"));
        }
        Ok((item, self.http.clone()))
    }
}

/// Xat contra un proveïdor remot. Suporta dos formats:
/// - compatible amb OpenAI (`POST {base}/chat/completions`)
/// - Anthropic / Claude (`POST {base}/messages`)
///
/// `conversation` és el FIL COMPLE del xat (historial + missatge actual, roles
/// alternats i acabat en «user»): si només hi ha un torn, és un xat nou.
/// Retorna el text de la resposta i, si el model n'emitix, el seu
/// *pensament* (raonament) per separat.
pub async fn chat(
    http: &reqwest::Client,
    provider: &AiProvider,
    conversation: &[(String, String)],
    system: Option<&str>,
) -> Result<(String, Option<String>)> {
    if provider.is_anthropic() {
        return chat_anthropic(http, provider, conversation, system).await;
    }
    chat_openai(http, provider, conversation, system).await
}

// ── Streaming (event unificat «ai://process») ─────────────────────────────
//
// Versió amb streaming token a token de `chat`. Fa `stream: true`, parseja el
// SSE del proveïdor i, per a cada fragment, emet «ai://chunk» (per al xat, com
// fa Ollama) i «ai://process» (amb la fase). Així qualsevol proveïdor remot
// (OpenAI, Groq, Claude…) mostra el procés en temps real igual que Ollama.

/// Com `chat` però amb streaming SSE. `conversation` porta tot el fil del xat;
/// `ctl` i `start` els passa la cap crida (`AiManager::chat_stream`) perquè la
/// cancel·lació, el cronòmetre i els events «ai://chunk»/«ai://process»
/// siguen del XAT que ha enviat el missatge (paral·lel entre xats).
pub async fn chat_stream(
    app: &tauri::AppHandle,
    http: &reqwest::Client,
    provider: &AiProvider,
    conversation: &[(String, String)],
    system: Option<&str>,
    ctl: &std::sync::Arc<crate::ai::SessionCtl>,
    start: std::time::Instant,
) -> Result<(String, Option<String>)> {
    if provider.is_anthropic() {
        stream_sse(app, http, provider, conversation, system, ctl, start, anthropic_delta).await
    } else {
        stream_sse(app, http, provider, conversation, system, ctl, start, openai_delta).await
    }
}

/// Extreu (text, raonament) d'un fragment JSON SSE d'OpenAI-compatible.
fn openai_delta(v: &serde_json::Value) -> (String, String) {
    let delta = v.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("delta"));
    let text = delta
        .and_then(|d| d.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    let thinking = delta
        .and_then(|d| d.get("reasoning_content").or_else(|| d.get("reasoning")))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    (text, thinking)
}

/// Extreu (text, raonament) d'un fragment JSON SSE d'Anthropic. Només els
/// events `content_block_delta` porten text: `text_delta` → resposta,
/// `thinking_delta` → raonament.
fn anthropic_delta(v: &serde_json::Value) -> (String, String) {
    if v.get("type").and_then(|t| t.as_str()) != Some("content_block_delta") {
        return (String::new(), String::new());
    }
    let delta = v.get("delta");
    let kind = delta.and_then(|d| d.get("type")).and_then(|t| t.as_str()).unwrap_or("");
    match kind {
        "text_delta" => (
            delta.and_then(|d| d.get("text")).and_then(|t| t.as_str()).unwrap_or("").to_string(),
            String::new(),
        ),
        "thinking_delta" => (
            String::new(),
            delta
                .and_then(|d| d.get("thinking"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
        ),
        _ => (String::new(), String::new()),
    }
}

/// Cos comú de streaming: obre la petició (stream:true), llegeix el flux byte
/// a byte, part per línies i, per a cada `data: {json}`, aplica `parse_delta`
/// per obtenir (text, thinking) i emetre'ls cap al XAT `ctl`.
#[allow(clippy::too_many_arguments)]
async fn stream_sse(
    app: &tauri::AppHandle,
    http: &reqwest::Client,
    provider: &AiProvider,
    conversation: &[(String, String)],
    system: Option<&str>,
    ctl: &std::sync::Arc<crate::ai::SessionCtl>,
    start: std::time::Instant,
    parse_delta: fn(&serde_json::Value) -> (String, String),
) -> Result<(String, Option<String>)> {
    let msgs: Vec<serde_json::Value> = conversation
        .iter()
        .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
        .collect();

    let base = provider.base_url.trim_end_matches('/');
    let (_url, req) = if provider.is_anthropic() {
        let url = if base.ends_with("/v1") {
            format!("{}/messages", base)
        } else {
            format!("{}/v1/messages", base)
        };
        let body = serde_json::json!({
            "model": provider.model,
            "max_tokens": 8192,
            "stream": true,
            "thinking": { "type": "enabled", "budget_tokens": 4096 },
            "system": system.unwrap_or("Ets l'assistent de NoOrbit."),
            "messages": msgs,
        });
        let req = http
            .post(&url)
            .header("x-api-key", provider.token.clone())
            .header("anthropic-version", "2023-06-01")
            .header("Accept", "text/event-stream")
            .json(&body);
        (url, req)
    } else {
        let url = format!("{}/chat/completions", base);
        let mut messages = Vec::new();
        if let Some(s) = system {
            messages.push(serde_json::json!({ "role": "system", "content": s }));
        }
        messages.extend(msgs);
        let body = serde_json::json!({
            "model": provider.model,
            "messages": messages,
            "stream": true,
        });
        let req = provider
            .apply_auth(http.post(&url))
            .header("Accept", "text/event-stream")
            .json(&body);
        (url, req)
    };

    let resp = req
        .send()
        .await
        .map_err(|e| anyhow!("El proveïdor no respon: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Error {}: {}", status, text));
    }

    use futures::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    let mut full = String::new();
    let mut thinking = String::new();
    let label = provider.name.clone();
    let model = provider.model.clone();

    loop {
        if ctl.is_cancelled() {
            return Err(anyhow!(crate::ai::CANCELLED_MSG));
        }
        let chunk = match stream.next().await {
            Some(Ok(c)) => c,
            Some(Err(e)) => return Err(anyhow!("Stream tallat: {}", e)),
            None => break,
        };
        buf.extend_from_slice(&chunk);
        while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
            let mut line: Vec<u8> = buf.drain(..=pos).collect();
            while matches!(line.last(), Some(&b'\n') | Some(&b'\r')) {
                line.pop();
            }
            let text = String::from_utf8_lossy(&line);
            let text = text.trim();
            // SSE: només les línies «data: …» porten càrrega; la resta
            // («event:», comentaris «:», buit) s'ignoren.
            let Some(payload) = text.strip_prefix("data:") else {
                continue;
            };
            let payload = payload.trim();
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
                continue;
            };
            let (tok, th) = parse_delta(&v);
            let ms = start.elapsed().as_millis() as u64;
            if !th.is_empty() {
                thinking.push_str(&th);
                ctl.process(app, &label, &model, "thinking", &th, ms);
            }
            if !tok.is_empty() {
                full.push_str(&tok);
                ctl.chunk(app, &tok);
                ctl.process(app, &label, &model, "streaming", &tok, ms);
            }
        }
    }
    let ms = start.elapsed().as_millis() as u64;
    ctl.process(app, &label, &model, "done", "", ms);
    if full.trim().is_empty() {
        return Err(anyhow!("Resposta buida del proveïdor"));
    }
    Ok((full, (!thinking.trim().is_empty()).then_some(thinking)))
}

/// Format compatible amb OpenAI.Models de raonament (o-series, DeepSeek-R1,
/// Ollama Cloud…) retornen el pensament a `message.reasoning_content`.
async fn chat_openai(
    http: &reqwest::Client,
    provider: &AiProvider,
    conversation: &[(String, String)],
    system: Option<&str>,
) -> Result<(String, Option<String>)> {
    let base = provider.base_url.trim_end_matches('/');
    let url = format!("{}/chat/completions", base);

    let mut messages = Vec::new();
    if let Some(s) = system {
        messages.push(serde_json::json!({ "role": "system", "content": s }));
    }
    // Tot el fil del xat: historial + missatge actual.
    messages.extend(
        conversation
            .iter()
            .map(|(role, content)| serde_json::json!({ "role": role, "content": content })),
    );

    let body = serde_json::json!({
        "model": provider.model,
        "messages": messages,
        "stream": false,
    });

    let req = provider
        .apply_auth(http.post(&url))
        .header("Accept", "application/json")
        .json(&body);

    let resp = req
        .send()
        .await
        .map_err(|e| anyhow!("El proveïdor no respon: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Error {}: {}", status, text));
    }
    let parsed: serde_json::Value = resp.json().await?;
    let message = parsed
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"));
    let content = message
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    // El raonament ve en `reasoning_content` (DeepSeek, OpenRouter, Ollama
    // Cloud) o en `reasoning` (algunes implementacions).
    let thinking = message
        .and_then(|m| m.get("reasoning_content").or_else(|| m.get("reasoning")))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty());
    if content.is_empty() {
        return Err(anyhow!("Resposta buida del proveïdor"));
    }
    Ok((content, thinking))
}

/// Anthropic (Claude): `POST {base}/messages` amb capçalera `x-api-key` i
/// `anthropic-version`. S'habilita el raonament extendit perquè la UI el puga
/// mostrar; el pensament ve en un bloc `{type:"thinking"}` del `content`.
async fn chat_anthropic(
    http: &reqwest::Client,
    provider: &AiProvider,
    conversation: &[(String, String)],
    system: Option<&str>,
) -> Result<(String, Option<String>)> {
    let base = provider.base_url.trim_end_matches('/');
    // Accepta tant la URL base (`https://api.anthropic.com`) com la versió
    // ja amb `/v1`; si no porta `/v1`, l'afegim.
    let url = if base.ends_with("/v1") {
        format!("{}/messages", base)
    } else {
        format!("{}/v1/messages", base)
    };

    let msgs: Vec<serde_json::Value> = conversation
        .iter()
        .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
        .collect();
    let body = serde_json::json!({
        "model": provider.model,
        "max_tokens": 8192,
        // Raonament extendit: Claude emet blocs de pensament.
        "thinking": { "type": "enabled", "budget_tokens": 4096 },
        "system": system.unwrap_or("Ets l'assistent de NoOrbit."),
        "messages": msgs,
    });

    let req = http
        .post(&url)
        .header("x-api-key", provider.token.clone())
        .header("anthropic-version", "2023-06-01")
        .header("Accept", "application/json")
        .json(&body);

    let resp = req
        .send()
        .await
        .map_err(|e| anyhow!("El proveïdor no respon: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Error {}: {}", status, text));
    }
    let parsed: serde_json::Value = resp.json().await?;
    let blocks = parsed.get("content").and_then(|c| c.as_array());
    let mut answer = String::new();
    let mut thinking = String::new();
    if let Some(arr) = blocks {
        for b in arr {
            match b.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                "text" => answer.push_str(b.get("text").and_then(|t| t.as_str()).unwrap_or("")),
                "thinking" => thinking
                    .push_str(b.get("thinking").and_then(|t| t.as_str()).unwrap_or("")),
                _ => {}
            }
        }
    }
    if answer.trim().is_empty() {
        return Err(anyhow!("Resposta buida del proveïdor"));
    }
    Ok((answer, (!thinking.trim().is_empty()).then_some(thinking)))
}
