//! Comandes Tauri per a plugins.

use crate::plugins::Plugin;
use crate::AppState;
use tauri::{command, State};

#[command]
pub async fn list_plugins(state: State<'_, AppState>) -> Result<Vec<Plugin>, String> {
    let mgr = state.plugin_manager.clone();
    let guard = mgr.lock().map_err(|e| e.to_string())?;
    Ok(guard.list())
}

#[command]
pub async fn set_plugin_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let mgr = state.plugin_manager.clone();
    let mut guard = mgr.lock().map_err(|e| e.to_string())?;
    guard.set_enabled(&id, enabled).map_err(|e| e.to_string())
}

#[command]
pub async fn detect_blender() -> Result<Option<String>, String> {
    Ok(crate::blender::detect_blender_binary())
}

#[command]
pub async fn run_blender_script(code: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || crate::blender::run_blender_headless(&code))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}
