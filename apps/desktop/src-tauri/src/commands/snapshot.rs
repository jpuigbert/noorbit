//! Snapshots git shadow per pas d'agent (Fase 1d).
//!
//! Contracte EXACTE per al frontend:
//!   invoke("snapshot_count", { projectPath }) → usize (mida de la pila)
//!   invoke("snapshot_undo",  { projectPath }) → Vec<String> (rutes
//!       relatives restaurades/esborrades) o Err amb nota en català.
//!
//! `snapshot_take` NO s'exposa: és automàtic abans de cada lot
//! d'escriptures de l'agent (runtools) i a l'inici de cada torn de xat.

use tauri::command;

/// Mida de la pila de snapshots del projecte (0 si no n'hi ha cap o el
/// projecte no existeix). El frontend mostra «Desfés el pas» només si > 0.
#[command]
pub async fn snapshot_count(project_path: String) -> usize {
    tauri::async_runtime::spawn_blocking(move || {
        crate::agent::snapshot::count(std::path::Path::new(project_path.trim()))
    })
    .await
    .unwrap_or(0)
}

/// Desfés el darrer pas de l'agent: restaura el work-tree al tree-hash
/// guardat (read-tree + checkout-index) i esborra els fitxers que l'agent
/// havia creat. NO toca l'historial git de l'usuari. Pila buida → Err clar.
#[command]
pub async fn snapshot_undo(project_path: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::agent::snapshot::undo(std::path::Path::new(project_path.trim()))
    })
    .await
    .map_err(|e| format!("el desfer ha fallat: {e}"))?
}