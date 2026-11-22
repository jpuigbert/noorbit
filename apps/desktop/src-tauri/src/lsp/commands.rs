//! Comandaments Tauri de la capa LSP (Fase 3c). Contracte EXACTE amb la UI:
//!
//!   lsp_completion(projectPath, path, line, col) → [{label, insertText, detail, kind}]
//!   lsp_hover(projectPath, path, line, col)      → {contents: string} | null
//!   lsp_references(projectPath, path, line, col) → [{path, line, col}]
//!   lsp_definition(projectPath, path, line, col) → {path, line, column, preview} | null
//!   lsp_diagnostics(projectPath, path)           → [{line, col, severity, message, source}]
//!   lsp_status(projectPath)                      → {server, language, ready}
//!
//! Tots són asíncronics (spawn_blocking: el client és bloquejant amb temps
//! límit propi de 8 s) i DEGRADEN amb honestedat: llista buida o null, MAI un
//! error que trenque l'editor. Línia/columna d'entrada i eixida: 1-based.

use super::session::{self, CompletionEntry, Diag, RefLoc, QUERY_TIMEOUT};
use super::DefLoc;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Resol (arrel, ruta absoluta) a partir del contracte de la UI: `path` pot
/// ser relatiu a l'arrel del projecte o ja absolut.
fn resolve(project_path: &str, path: &str) -> (PathBuf, PathBuf) {
    let root = PathBuf::from(project_path);
    let p = Path::new(path);
    let abs = if p.is_absolute() { p.to_path_buf() } else { root.join(p) };
    (root, abs)
}

#[derive(Debug, Clone, Serialize)]
pub struct HoverResp {
    pub contents: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusResp {
    pub server: Option<String>,
    pub language: Option<String>,
    pub ready: bool,
}

#[tauri::command]
pub async fn lsp_completion(
    project_path: String,
    path: String,
    line: usize,
    col: usize,
) -> Result<Vec<CompletionEntry>, String> {
    let out = tauri::async_runtime::spawn_blocking(move || {
        let (root, abs) = resolve(&project_path, &path);
        let sess = session::ensure_session(&root, &abs)?;
        sess.completion(&abs, line, col)
    })
    .await;
    Ok(out.ok().and_then(|r| r.ok()).unwrap_or_default())
}

#[tauri::command]
pub async fn lsp_hover(
    project_path: String,
    path: String,
    line: usize,
    col: usize,
) -> Result<Option<HoverResp>, String> {
    let out = tauri::async_runtime::spawn_blocking(move || {
        let (root, abs) = resolve(&project_path, &path);
        let sess = session::ensure_session(&root, &abs)?;
        sess.hover(&abs, line, col)
    })
    .await;
    Ok(out
        .ok()
        .and_then(|r| r.ok())
        .flatten()
        .map(|contents| HoverResp { contents }))
}

#[tauri::command]
pub async fn lsp_references(
    project_path: String,
    path: String,
    line: usize,
    col: usize,
) -> Result<Vec<RefLoc>, String> {
    let out = tauri::async_runtime::spawn_blocking(move || {
        let (root, abs) = resolve(&project_path, &path);
        let sess = session::ensure_session(&root, &abs)?;
        sess.references(&abs, line, col)
    })
    .await;
    Ok(out.ok().and_then(|r| r.ok()).unwrap_or_default())
}

/// Mateix format que el `find_definition` antic (DefLoc relatiu + preview),
/// però sense respall heurístic: si no hi ha servidor, null honest (la UI
/// continua tenint el F12 heurístic a `find_definition`).
#[tauri::command]
pub async fn lsp_definition(
    project_path: String,
    path: String,
    line: usize,
    col: usize,
) -> Result<Option<DefLoc>, String> {
    let out = tauri::async_runtime::spawn_blocking(move || {
        let (root, abs) = resolve(&project_path, &path);
        let sess = session::ensure_session(&root, &abs)?;
        sess.definition(&abs, line, col, QUERY_TIMEOUT)
    })
    .await;
    Ok(out.ok().and_then(|r| r.ok()).flatten())
}

#[tauri::command]
pub async fn lsp_diagnostics(project_path: String, path: String) -> Result<Vec<Diag>, String> {
    let out = tauri::async_runtime::spawn_blocking(move || {
        let (root, abs) = resolve(&project_path, &path);
        let sess = session::ensure_session(&root, &abs)?;
        sess.diagnostics(&abs, QUERY_TIMEOUT)
    })
    .await;
    Ok(out.ok().and_then(|r| r.ok()).unwrap_or_default())
}

/// Estat honest de la sessió per a l'indicador de la UI. MAI arrenca un
/// servidor: només mira el gestor (barat, es pot pol·lejar cada pocs segons).
#[tauri::command]
pub async fn lsp_status(project_path: String) -> Result<StatusResp, String> {
    let out = tauri::async_runtime::spawn_blocking(move || {
        let (server, language, ready) = session::status(Path::new(&project_path));
        StatusResp { server, language, ready }
    })
    .await;
    Ok(out.unwrap_or(StatusResp { server: None, language: None, ready: false }))
}