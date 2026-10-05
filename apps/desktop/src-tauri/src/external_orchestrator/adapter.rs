//! Adaptació de la comunicació a cada tipus d'IA local descoberta.
//! En lloc de cridar `curl` des del codi (com feia el pla original), s'usa
//! el mateix client HTTP async de NoOrbit: Ollama amb la seva API nativa
//! `/api/chat` i la resta amb la interfície compatible amb OpenAI
//! `/v1/chat/completions`. Totes dues en mode sense streaming: l'orquestrador
//! vol la resposta completa per reemetre-la després al xat.

use crate::external_orchestrator::discovery::ExternalIa;
use anyhow::{anyhow, Result};
use std::time::Duration;

/// Envia una petició a la IA indicada i retorna (text de la resposta,
/// model que l'ha generada). El model s'exposa perquè la UI puga etiquetar
/// l'origen REAL de cada resposta externa.
pub async fn send_request(
    http: &reqwest::Client,
    ia: &ExternalIa,
    prompt: &str,
    system: Option<&str>,
) -> Result<(String, String)> {
    match ia.api.as_str() {
        "ollama" => chat_ollama(http, ia, prompt, system).await,
        _ => chat_openai(http, ia, prompt, system).await,
    }
}

/// Construïu els missatges: la instrucció de sistema (si n'hi ha) i el prompt.
fn messages(prompt: &str, system: Option<&str>) -> Vec<serde_json::Value> {
    let mut msgs = Vec::new();
    if let Some(s) = system.filter(|s| !s.trim().is_empty()) {
        msgs.push(serde_json::json!({ "role": "system", "content": s }));
    }
    msgs.push(serde_json::json!({ "role": "user", "content": prompt }));
    msgs
}

/// Consulta la llista de models de la IA (format Ollama) per triar-ne un.
async fn ollama_first_model(http: &reqwest::Client, base: &str) -> Result<String> {
    let resp = http
        .get(format!("{}/api/tags", base))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| anyhow!("Ollama no respon — {}", e))?;
    let v: serde_json::Value = resp.json().await?;
    v.get("models")
        .and_then(|m| m.get(0))
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str())
        .map(|n| n.to_string())
        .ok_or_else(|| anyhow!("Aquesta Ollama externa no té cap model descarregat"))
}

/// Consulta la llista de models (format OpenAI) per triar-ne un.
async fn openai_first_model(http: &reqwest::Client, base: &str) -> Result<String> {
    let resp = http
        .get(format!("{}/v1/models", base))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| anyhow!("L'IA externa no respon — {}", e))?;
    let v: serde_json::Value = resp.json().await?;
    v.get("data")
        .and_then(|d| d.get(0))
        .and_then(|m| m.get("id"))
        .and_then(|n| n.as_str())
        .map(|n| n.to_string())
        .ok_or_else(|| anyhow!("Aquesta IA externa no té cap model carregat"))
}

/// Comunicació amb Ollama a través de la seva API nativa.
async fn chat_ollama(
    http: &reqwest::Client,
    ia: &ExternalIa,
    prompt: &str,
    system: Option<&str>,
) -> Result<(String, String)> {
    let model = ollama_first_model(http, &ia.base_url).await?;
    let body = serde_json::json!({
        "model": model,
        "messages": messages(prompt, system),
        "stream": false,
    });
    let resp = http
        .post(format!("{}/api/chat", ia.base_url))
        .json(&body)
        // Una generació en un model local pot tardar força: mateix límit
        // generós que el xat principal de NoOrbit.
        .timeout(Duration::from_secs(240))
        .send()
        .await
        .map_err(|e| anyhow!("Error comunicant amb {}: {}", ia.name, e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("{} error {}: {}", ia.name, status, text));
    }
    let v: serde_json::Value = resp.json().await?;
    let content = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| anyhow!("Sense resposta de {}", ia.name))?
        .to_string();
    Ok((content, model))
}

/// Comunicació amb una IA compatible amb OpenAI (LM Studio, Jan, vLLM…).
async fn chat_openai(
    http: &reqwest::Client,
    ia: &ExternalIa,
    prompt: &str,
    system: Option<&str>,
) -> Result<(String, String)> {
    let model = openai_first_model(http, &ia.base_url).await?;
    let body = serde_json::json!({
        "model": model,
        "messages": messages(prompt, system),
        "stream": false,
    });
    let resp = http
        .post(format!("{}/v1/chat/completions", ia.base_url))
        .json(&body)
        .timeout(Duration::from_secs(240))
        .send()
        .await
        .map_err(|e| anyhow!("Error comunicant amb {}: {}", ia.name, e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("{} error {}: {}", ia.name, status, text));
    }
    let v: serde_json::Value = resp.json().await?;
    let content = v
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| anyhow!("Sense resposta de {}", ia.name))?
        .to_string();
    Ok((content, model))
}
