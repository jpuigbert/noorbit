//! Comandes Tauri per a proveïdors d'IA en línia (tokens).

use crate::api::{self, AiProvider, AiProviderPublic, AuthScheme};
use crate::AppState;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use tauri::{command, State};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewProvider {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    /// Format d'API: "openai" (per defecte) o "anthropic" (Claude).
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub auth: Option<AuthScheme>,
    #[serde(default)]
    pub header_name: Option<String>,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPatch {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub auth: Option<AuthScheme>,
    #[serde(default)]
    pub header_name: Option<String>,
    /// Si és None, conserva el token existent.
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[command]
pub fn ai_provider_list(state: State<'_, AppState>) -> Result<Vec<AiProviderPublic>, String> {
    Ok(state.api_manager.lock().unwrap().list())
}

#[command]
pub fn ai_provider_add(
    state: State<'_, AppState>,
    provider: NewProvider,
) -> Result<AiProviderPublic, String> {
    let item = AiProvider {
        id: provider.id,
        name: provider.name,
        base_url: provider.base_url,
        model: provider.model,
        kind: provider.kind,
        auth: provider.auth.unwrap_or(AuthScheme::Bearer),
        header_name: provider.header_name.unwrap_or_default(),
        token: provider.token,
        enabled: provider.enabled.unwrap_or(true),
    };
    state
        .api_manager
        .lock()
        .unwrap()
        .add(item)
        .map_err(|e| e.to_string())
}

#[command]
pub fn ai_provider_update(
    state: State<'_, AppState>,
    patch: ProviderPatch,
) -> Result<AiProviderPublic, String> {
    state
        .api_manager
        .lock()
        .unwrap()
        .update(
            &patch.id,
            patch.name,
            patch.base_url,
            patch.model,
            patch.kind,
            patch.auth,
            patch.header_name,
            patch.token,
            patch.enabled,
        )
        .map_err(|e| e.to_string())
}

#[command]
pub fn ai_provider_remove(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .api_manager
        .lock()
        .unwrap()
        .remove(&id)
        .map_err(|e| e.to_string())
}

/// Prova un proveïdor enviant-hi una petició de xat senzilla.
#[command]
pub async fn ai_provider_test(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let mgr: Arc<Mutex<api::ApiManager>> = state.api_manager.clone();
    let (provider, client) = {
        let guard = mgr.lock().unwrap();
        guard.prepare(&id).map_err(|e| e.to_string())?
    };
    api::chat(
        &client,
        &provider,
        &[("user".to_string(), "Respon només amb la paraula: OK".to_string())],
        Some("Ets una prova de connexió. Respon molt breu."),
    )
    .await
    .map(|(content, _)| content)
    .map_err(|e| e.to_string())
}

/// Xat directe contra un proveïdor (independent del proveïdor actiu).
#[command]
pub async fn ai_provider_chat(
    state: State<'_, AppState>,
    id: String,
    prompt: String,
    system: Option<String>,
) -> Result<String, String> {
    let mgr: Arc<Mutex<api::ApiManager>> = state.api_manager.clone();
    let (provider, client) = {
        let guard = mgr.lock().unwrap();
        guard.prepare(&id).map_err(|e| e.to_string())?
    };
    api::chat(
        &client,
        &provider,
        &[("user".to_string(), prompt.clone())],
        system.as_deref(),
    )
        .await
        .map(|(content, _)| content)
        .map_err(|e| e.to_string())
}

/// Activa un proveïdor (id "ollama" o el d'una integració remota) per a tota l'IA.
#[command]
pub async fn ai_select_provider(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let ai = state.ai_manager.clone();
    ai.set_active_provider(&id).await;
    Ok(())
}

#[command]
pub async fn ai_current_provider(state: State<'_, AppState>) -> Result<String, String> {
    let ai = state.ai_manager.clone();
    Ok(ai.active_provider().await)
}
