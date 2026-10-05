//! Comandes Tauri per a la configuració.

use crate::config::AppConfig;
use crate::lsp;
use crate::AppState;
use tauri::{command, State};

#[command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let cfg = { state.config.read().await.clone() };
    Ok(cfg)
}

#[command]
pub async fn update_config(
    state: State<'_, AppState>,
    config: AppConfig,
) -> Result<AppConfig, String> {
    config.save().map_err(|e| e.to_string())?;
    {
        *state.config.write().await = config.clone();
    }
    Ok(config)
}

#[command]
pub async fn list_lsp_servers() -> Result<Vec<lsp::LspServer>, String> {
    Ok(lsp::known_servers())
}
