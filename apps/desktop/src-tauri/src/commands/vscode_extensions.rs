use crate::AppState;
use serde::Serialize;
use tauri::{command, State};

/// Informació completa d'una extensió de VS Code instal·lada.
#[derive(Debug, Serialize, Clone)]
pub struct VsCodeExtensionInfo {
    /// Identificador (publisher.name).
    pub id: String,
    /// Nom visible.
    pub display: String,
    /// Versió.
    pub version: String,
    /// Descripció.
    pub description: String,
    /// Carpeta d'origen (VS Code, Cursor, etc.).
    pub source: String,
    /// Comandes que aporta.
    pub commands: Vec<String>,
    /// Llenguatges que aporta.
    pub languages: Vec<String>,
    /// Fragments (snippets) que aporta.
    pub snippets: Vec<(String, String)>,
    /// Temes de color que aporta.
    pub themes: Vec<String>,
    /// Ja importada com a skill?
    pub imported: bool,
}

#[command]
pub async fn vscode_list_extensions(state: State<'_, AppState>) -> Result<Vec<VsCodeExtensionInfo>, String> {
    let mgr = state.skills_manager.clone();
    let dir_root = {
        let guard = mgr.lock().map_err(|e| e.to_string())?;
        guard.skills_dir().to_path_buf()
    };
    tokio::task::spawn_blocking(move || {
        let home = dirs::home_dir()
            .ok_or_else(|| "No s'ha trobat la carpeta d'usuari".to_string())?;
        let roots = [
            (".vscode/extensions", "VS Code"),
            (".vscode-server/extensions", "VS Code Server"),
            (".cursor/extensions", "Cursor"),
            (".qoder/extensions", "Qoder"),
            (".trae/extensions", "Trae"),
            (".windsurf/extensions", "Windsurf"),
        ];
        let mut out = Vec::new();
        for (rel, source) in roots {
            let p = home.join(rel);
            if !p.is_dir() { continue; }
            let Ok(entries) = std::fs::read_dir(&p) else { continue };
            for e in entries.flatten() {
                if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
                let Some(info) = read_vscode_ext_info(&e.path(), source, &dir_root) else { continue };
                out.push(info);
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn read_vscode_ext_info(dir: &std::path::Path, source: &str, skills_dir: &std::path::Path) -> Option<VsCodeExtensionInfo> {
    let manifest = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&manifest).ok()?;
    let publisher = v.get("publisher").and_then(|x| x.as_str()).unwrap_or("vscode");
    let name = v.get("name").and_then(|x| x.as_str())?;
    let display = v.get("displayName").and_then(|x| x.as_str()).unwrap_or(name).to_string();
    let version = v.get("version").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let description = v.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let id = format!("{publisher}.{name}");
    let dir_name = crate::commands::skills::sanitize_id(&id);

    let mut commands = Vec::new();
    if let Some(arr) = v.get("contributes").and_then(|c| c.get("commands")).and_then(|c| c.as_array()) {
        for cmd in arr {
            let title = cmd.get("title").and_then(|x| x.as_str()).unwrap_or_default();
            if !title.is_empty() { commands.push(title.to_string()); }
        }
    }

    let mut languages = Vec::new();
    if let Some(arr) = v.get("contributes").and_then(|c| c.get("languages")).and_then(|c| c.as_array()) {
        for lang in arr {
            let id = lang.get("id").and_then(|x| x.as_str()).unwrap_or_default();
            if !id.is_empty() { languages.push(id.to_string()); }
        }
    }

    let mut snippets = Vec::new();
    if let Some(map) = v.get("contributes").and_then(|c| c.get("snippets")).and_then(|c| c.as_object()) {
        for (_lang, paths_v) in map {
            let Some(paths) = paths_v.as_array() else { continue };
            for pv in paths {
                let Some(p) = pv.as_str() else { continue };
                let Ok(raw) = std::fs::read_to_string(dir.join(p)) else { continue };
                let Some(obj) = crate::commands::skills::parse_snippets_json(&raw) else { continue };
                for (snip_name, snip_v) in obj {
                    let prefix = snip_v.get("prefix").map(|x| match x {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    }).unwrap_or_else(|| snip_name.clone());
                    snippets.push((prefix, snip_name));
                }
            }
        }
    }

    let mut themes = Vec::new();
    if let Some(arr) = v.get("contributes").and_then(|c| c.get("themes")).and_then(|c| c.as_array()) {
        for th in arr {
            let label = th.get("label").and_then(|x| x.as_str())
                .or_else(|| th.get("id").and_then(|x| x.as_str()))
                .unwrap_or_default();
            if !label.is_empty() { themes.push(label.to_string()); }
        }
    }

    let imported = skills_dir.join("vscode").join(&dir_name).join("SKILL.md").is_file();

    Some(VsCodeExtensionInfo {
        id,
        display,
        version,
        description,
        source: source.to_string(),
        commands,
        languages,
        snippets,
        themes,
        imported,
    })
}