//! Comandes Tauri per a skills.

use crate::skills::Skill;
use crate::AppState;
use tauri::{command, State};

#[command]
pub async fn list_skills(state: State<'_, AppState>) -> Result<Vec<Skill>, String> {
    let mgr = state.skills_manager.clone();
    let guard = mgr.lock().map_err(|e| e.to_string())?;
    Ok(guard.list())
}

#[command]
pub async fn reload_skills(state: State<'_, AppState>) -> Result<Vec<Skill>, String> {
    let mgr = state.skills_manager.clone();
    let mut guard = mgr.lock().map_err(|e| e.to_string())?;
    guard.reload();
    Ok(guard.list())
}
