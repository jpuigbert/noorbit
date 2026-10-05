//! Gestor de skills: fitxers markdown amb frontmatter YAML simple.
//!
//! Cada skill és un fitxer `<skills_dir>/<nom>.md` amb:
//!   ---
//!   name: ...
//!   description: ...
//!   ---
//!   <cos de les instruccions>
//!
//! Skills preinstal·lades incrustades al binari (generades per build.rs
//! des de `skills-builtin/`). Format estàndard Agent Skills (SKILL.md),
//! compatible amb GitHub Copilot (`.github/skills/`) i OpenCode
//! (`~/.config/opencode/skills/`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/builtin_skills.rs"));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    /// true si prové de la carpeta `builtin/` (preinstal·lada amb l'app).
    #[serde(default)]
    pub builtin: bool,
}

pub struct SkillsManager {
    skills_dir: PathBuf,
    skills: Vec<Skill>,
}

impl SkillsManager {
    pub fn new(config: &crate::config::AppConfig) -> Self {
        let dir = if !config.skills_dir.is_empty() {
            PathBuf::from(&config.skills_dir)
        } else {
            dirs::data_dir()
                .unwrap_or_else(|| std::env::temp_dir())
                .join("no-orbit")
                .join("skills")
        };
        let _ = std::fs::create_dir_all(&dir);
        seed_builtin_skills(&dir);
        let mut manager = Self { skills_dir: dir, skills: vec![] };
        manager.reload();
        manager
    }

    pub fn skills_dir(&self) -> &Path {
        &self.skills_dir
    }

    pub fn reload(&mut self) {
        self.skills = scan_skills(&self.skills_dir);
    }

    pub fn list(&self) -> Vec<Skill> {
        self.skills.clone()
    }
}

/// Copia les skills incrustades a `<dir>/builtin/<nom>/SKILL.md` si encara
/// no existeixen. Les edits de l'usuari es conserven: no se sobreescriu
/// res existent.
fn seed_builtin_skills(dir: &Path) {
    for (rel, content) in BUILTIN_SKILL_FILES {
        let target = dir.join("builtin").join(rel);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&target, content);
    }
}

/// Restaura les skills preinstal·lades a la versió de fàbrica (sobreescriu
/// la carpeta `builtin/`). Retorna el nombre de fitxers restaurats.
pub fn restore_builtin_skills(dir: &Path) -> usize {
    let mut n = 0;
    for (rel, content) in BUILTIN_SKILL_FILES {
        let target = dir.join("builtin").join(rel);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&target, content).is_ok() {
            n += 1;
        }
    }
    n
}

fn scan_skills(root: &Path) -> Vec<Skill> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return out;
    }
    for entry in walkdir::WalkDir::new(root)
        .max_depth(3)
        .into_iter()
        .flatten()
    {
        let path = entry.path();
        if path.file_name().map(|n| n == "SKILL.md").unwrap_or(false)
            || (entry.file_type().is_file()
                && path.extension().map(|e| e == "md").unwrap_or(false)
                && path.parent().map(|p| p == root).unwrap_or(false))
        {
            if let Some(skill) = parse_skill(path) {
                out.push(skill);
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn parse_skill(path: &Path) -> Option<Skill> {
    let raw = std::fs::read_to_string(path).ok()?;
    let mut name = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if name.is_empty() || name == "skills" {
        name = path.file_stem()?.to_string_lossy().to_string();
    }
    let mut description = String::new();
    let mut in_front = false;
    let mut count = 0;
    for line in raw.lines() {
        count += 1;
        if count == 1 && line.trim() == "---" {
            in_front = true;
            continue;
        }
        if in_front {
            if line.trim() == "---" {
                break;
            }
            if let Some(rest) = line.strip_prefix("name:") {
                let v = rest.trim().trim_matches('"');
                if !v.is_empty() {
                    name = v.to_string();
                }
            } else if let Some(rest) = line.strip_prefix("description:") {
                description = rest.trim().trim_matches('"').to_string();
            }
        } else {
            break;
        }
    }
    Some(Skill {
        id: name.to_lowercase().replace([' ', '.'], "-"),
        name,
        description,
        path: path.to_string_lossy().to_string(),
        builtin: path
            .components()
            .any(|c| c.as_os_str() == "builtin"),
    })
}
