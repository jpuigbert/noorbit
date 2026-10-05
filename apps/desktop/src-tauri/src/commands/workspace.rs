//! Gestió de l'arrel del workspace obert.

use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::RwLock;
use tauri::{command, AppHandle, Emitter, Manager};

static WORKSPACE_ROOT: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));

/// retorna l'arrel del workspace actual (si n'hi ha).
pub fn current_root() -> Option<PathBuf> {
    WORKSPACE_ROOT.read().ok().and_then(|g| g.clone())
}

/// resol un path relatiu a l'arrel del workspace.
pub fn resolve_in_root(path: &str) -> PathBuf {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        current_root().map(|r| r.join(p)).unwrap_or_else(|| p.to_path_buf())
    }
}

#[command]
pub async fn open_workspace(app: AppHandle, path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    if !p.is_dir() {
        return Err(format!("La carpeta {} no existeix", path));
    }
    *WORKSPACE_ROOT.write().unwrap() = Some(p.clone());

    let title = format!("NoOrbit — {}", p.file_name().unwrap_or_default().to_string_lossy());
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title(&title);
    }

    let display = p.to_string_lossy().to_string();
    let _ = app.emit("workspace://opened", display.clone());
    Ok(display)
}

/// Crea un projecte nou: carpeta amb un README inicial, dins de `parent`
/// (per defecte ~/Projects). Retornarà el camí creat per obrir-lo com a workspace.
#[command]
pub async fn create_project(name: String, parent: Option<String>) -> Result<String, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("El nom del projecte no pot ser buit".into());
    }
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err("El nom no pot contenir bars".into());
    }

    let base = match parent {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => dirs::home_dir()
            .ok_or_else(|| "Carpeta personal no disponible".to_string())?
            .join("Projects"),
    };
    let dir = base.join(&name);
    if dir.exists() {
        return Err(format!("Ja existeix la carpeta {}", dir.display()));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("No s'ha pogut crear: {}", e))?;

    let readme = format!(
        "# {}\n\nProjecte creat amb NoOrbit.\n",
        name
    );
    let _ = std::fs::write(dir.join("README.md"), readme);
    Ok(dir.to_string_lossy().to_string())
}
