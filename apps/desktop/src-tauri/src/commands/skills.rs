//! Comandes Tauri per a skills.

use crate::skills::Skill;
use crate::AppState;
use serde::Serialize;
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

/// ── COMPATIBILITAT VS CODE ──────────────────────────────────────────────────
/// Importa les extensions instal·lades de VS Code (i variants Cursor / server)
/// com a skills de NoOrbit: d'una extensió s'extreuen les COMANDES (títols i
/// IDs) i els SNIPPETS (prefix + fragment), i es genera un `SKILL.md` per
/// extensió dins `<skills_dir>/vscode/`. Així allò que l'usuari ja té a VS
/// Code (comandes ràpides, plantilles de text) queda disponible per a la IA
/// de NoOrbit sense repetir-ho cap repar.
#[derive(Debug, Serialize)]
pub struct VsCodeReport {
    /// Extensions analitzades amb manifest vàlid.
    pub scanned: usize,
    /// Carpettes `vscode/<ext>/SKILL.md` creades.
    pub imported: usize,
    /// Institutions realment importades (nom + motiu si fou saltada).
    pub details: Vec<String>,
}

struct VsCodeExt {
    dir_name: String,
    name: String,
    display: String,
    description: String,
    commands: Vec<String>,
    snippets: Vec<(String, String)>, // (prefix, fragment)
}

fn sanitize_id(raw: &str) -> String {
    raw.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Llegeix un fitxer de snippets (pot tenir comentaries JSON, típic de VS Code):
/// tolera `//` i `/* */` eliminant-los prèviament.
fn parse_snippets_json(raw: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut out = String::with_capacity(raw.len());
    let mut in_block = false;
    let mut in_line = false;
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_line {
            if c == '\n' {
                in_line = false;
                out.push(c);
            }
            i += 1;
            continue;
        }
        if in_block {
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                in_block = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            in_line = true;
            i += 2;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            in_block = true;
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    serde_json::from_str::<serde_json::Value>(&out)
        .ok()?
        .as_object()
        .cloned()
}

fn read_vscode_ext(dir: &std::path::Path) -> Option<VsCodeExt> {
    let manifest = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&manifest).ok()?;
    let publisher = v
        .get("publisher")
        .and_then(|x| x.as_str())
        .unwrap_or(" vscode");
    let name = v.get("name").and_then(|x| x.as_str())?;
    let display = v
        .get("displayName")
        .and_then(|x| x.as_str())
        .unwrap_or(name);
    let description = v
        .get("description")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let mut commands = Vec::new();
    if let Some(arr) = v
        .get("contributes")
        .and_then(|c| c.get("commands"))
        .and_then(|c| c.as_array())
    {
        for cmd in arr {
            let title = cmd
                .get("title")
                .and_then(|x| x.as_str())
                .unwrap_or_default();
            let id = cmd
                .get("command")
                .and_then(|x| x.as_str())
                .unwrap_or_default();
            if !title.is_empty() {
                commands.push(format!(
                    "- **{title}** · `{id}`"
                ));
            }
        }
    }
    let mut snippets = Vec::new();
    if let Some(map) = v
        .get("contributes")
        .and_then(|c| c.get("snippets"))
        .and_then(|c| c.as_object())
    {
        for (_lang, paths_v) in map {
            let Some(paths) = paths_v.as_array() else { continue };
            for pv in paths {
                let Some(p) = pv.as_str() else { continue };
                let Ok(raw) = std::fs::read_to_string(dir.join(p)) else {
                    continue;
                };
                let Some(obj) = parse_snippets_json(&raw) else { continue };
                for (snip_name, snip_v) in obj {
                    let prefix = snip_v
                        .get("prefix")
                        .map(|x| match x {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .unwrap_or_else(|| snip_name.clone());
                    let body = snip_v
                        .get("body")
                        .and_then(|b| match b {
                            serde_json::Value::Array(a) => Some(
                                a.iter()
                                    .filter_map(|l| l.as_str())
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            ),
                            serde_json::Value::String(s) => Some(s.clone()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    snippets.push((prefix, body));
                }
            }
        }
    }
    Some(VsCodeExt {
        dir_name: sanitize_id(&format!("{publisher}.{name}")),
        name: format!("{publisher}.{name}"),
        display: display.to_string(),
        description,
        commands,
        snippets,
    })
}

#[command]
pub async fn vscode_import_skills(state: State<'_, AppState>) -> Result<VsCodeReport, String> {
    let mgr = state.skills_manager.clone();
    let dir_root = {
        let guard = mgr.lock().map_err(|e| e.to_string())?;
        guard.skills_dir().to_path_buf()
    };
    tokio::task::spawn_blocking(move || {
        let mut report = VsCodeReport { scanned: 0, imported: 0, details: vec![] };
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("No s'ha trobat la carpeta d'usuari"))?;
        let mut roots = Vec::new();
        // VS Code i totes les seves variants d'IDE (Qoder, Cursor, Trae…):
        // totes tenen extensions amb `package.json` amb menys versions de
        // VS Code — mateix format, una única instalació serveix.
        for r in [
            ".vscode/extensions",
            ".vscode-server/extensions",
            ".cursor/extensions",
            ".qoder/extensions",
            ".trae/extensions",
            ".windsurf/extensions",
        ] {
            let p = home.join(r);
            if p.is_dir() {
                roots.push(p);
            }
        }
        if roots.is_empty() {
            report
                .details
                .push("No hi ha VS Code (ni Qoder, Cursor, Trae o Windsurf) instal·lat: cap carpeta ~/.vscode/extensions ni equivalents".into());
            return Ok(report);
        }
        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else { continue };
            for e in entries.flatten() {
                if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let Some(ext) = read_vscode_ext(&e.path()) else { continue };
                report.scanned += 1;
                if ext.commands.is_empty() && ext.snippets.is_empty() {
                    report.details.push(format!("{} — sense comandes ni fragments, saltada", ext.name));
                    continue;
                }
                let target_dir = dir_root.join("vscode").join(&ext.dir_name);
                let _ = std::fs::create_dir_all(&target_dir);
                let mut md = String::new();
                md.push_str(&format!(
                    "name: VS Code · {}\ndescription: Comandes i fragments de l'extensió {} de VS Code.\n---\n\n",
                    ext.display, ext.display
                ));
                if !ext.description.is_empty() {
                    md.push_str(&ext.description);
                    md.push_str("\n\n");
                }
                if !ext.commands.is_empty() {
                    md.push_str("## Comandes disponibles\n");
                    for c in ext.commands.iter().take(60) {
                        md.push_str(&c);
                        md.push('\n');
                    }
                    md.push('\n');
                }
                if !ext.snippets.is_empty() {
                    md.push_str("## Fragments (prefix → fragment)\n");
                    for (prefix, body) in ext.snippets.iter().take(60) {
                        md.push_str(&format!("### {prefix}\n```text\n{body}\n```\n"));
                    }
                    md.push('\n');
                }
                let target = target_dir.join("SKILL.md");
                match std::fs::write(&target, &md) {
                    Ok(()) => {
                        report.imported += 1;
                        report.details.push(format!(
                            "{} — importada ({} comandes, {} fragments)",
                            ext.name,
                            ext.commands.len().min(60),
                            ext.snippets.len().min(60)
                        ));
                    }
                    Err(e) => report.details.push(format!("{} — error: {e}", ext.name)),
                }
            }
        }
        // Recarrega la llista perquè la UI les vegi ja.
        if let Ok(mut guard2) = mgr.lock() {
            guard2.reload();
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e: anyhow::Error| e.to_string())
}
