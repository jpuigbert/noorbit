//! Instal·lació de skills des de GitHub, URL, zip o carpeta local.

use crate::skills::Skill;
use crate::AppState;
use std::path::{Path, PathBuf};
use tauri::{command, State};

fn skills_dir(state: &State<'_, AppState>) -> Result<PathBuf, String> {
    let mgr = state.skills_manager.clone();
    let guard = mgr.lock().map_err(|e| e.to_string())?;
    Ok(guard.skills_dir().to_path_buf())
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        let dest = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Llista les skills instal·lades (per defecte + usuari).
#[command]
pub async fn skills_store_list(state: State<'_, AppState>) -> Result<Vec<Skill>, String> {
    let mgr = state.skills_manager.clone();
    let guard = mgr.lock().map_err(|e| e.to_string())?;
    Ok(guard.list())
}

/// Instal·la una skill des d'un repo GitHub: "usuari/repo[/subcarpeta]@branca".
#[command]
pub async fn skills_install_github(
    state: State<'_, AppState>,
    repo: String,
) -> Result<Skill, String> {
    let dir = skills_dir(&state)?;
    let (repo_part, branch) = match repo.split_once('@') {
        Some((r, b)) => (r.to_string(), Some(b.to_string())),
        None => (repo.clone(), None),
    };
    let (owner, name_sub) = match repo_part.split_once('/') {
        Some((o, rest)) => (o.to_string(), rest.to_string()),
        None => return Err("Format esperat: usuari/repo[/carpeta]".into()),
    };
    if !owner.chars().all(|c| c.is_alphanumeric() || "-_".contains(c)) {
        return Err("Nom d'usuari no vàlid".into());
    }

    let url = format!("https://github.com/{}.git", owner.trim_matches('/'));
    let tmp = std::env::temp_dir().join(format!("noorbit-skill-{}", uuid::Uuid::new_v4()));
    let mut cmd = std::process::Command::new("git");
    cmd.args(["clone", "--depth", "1"]);
    if let Some(b) = &branch {
        cmd.args(["--branch", b]);
    }
    let status = cmd
        .arg(&url)
        .arg(&tmp)
        .status()
        .map_err(|e| format!("No s'ha pogut executar git: {}", e))?;
    if !status.success() {
        return Err(format!("git clone ha fallat per {}", url));
    }

    let src = if name_sub.is_empty() { tmp.clone() } else { tmp.join(&name_sub) };
    if !src.is_dir() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("La carpeta {} no existeix al repositori", name_sub));
    }

    let skill_name = name_sub
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(&owner)
        .to_string();
    let dest = dir.join(&skill_name);
    let _ = std::fs::remove_dir_all(&dest);
    copy_dir_recursive(&src, &dest)?;
    let _ = std::fs::remove_dir_all(&tmp);

    let mgr = state.skills_manager.clone();
    {
        let mut guard = mgr.lock().map_err(|e| e.to_string())?;
        guard.reload();
        guard
            .list()
            .into_iter()
            .find(|s| s.path.contains(&skill_name))
            .map(Ok)
            .unwrap_or_else(|| {
                Err(format!("Skill '{}' instal·lada però no detectada", skill_name))
            })
    }
}

/// Instal·la una skill des d una URL directa a un fitxer .md.
#[command]
pub async fn skills_install_url(
    state: State<'_, AppState>,
    url: String,
) -> Result<Skill, String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("La URL ha de començar per http(s)://".into());
    }
    let dir = skills_dir(&state)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Error HTTP {}", resp.status()));
    }
    let body = resp.text().await.map_err(|e| e.to_string())?;

    let name = url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("skill.md")
        .trim_end_matches(".md")
        .to_string();
    let dest_dir = dir.join(&name);
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    std::fs::write(dest_dir.join("SKILL.md"), body).map_err(|e| e.to_string())?;

    let mgr = state.skills_manager.clone();
    let mut guard = mgr.lock().map_err(|e| e.to_string())?;
    guard.reload();
    guard
        .list()
        .into_iter()
        .find(|s| s.path.contains(&name))
        .map(Ok)
        .unwrap_or_else(|| Err(format!("No s'ha pogut carregar la skill '{}' des de la URL", name)))
}

/// Instal·la una skill des d'un zip (URL o ruta local).
#[command]
pub async fn skills_install_zip(
    state: State<'_, AppState>,
    source: String,
) -> Result<Skill, String> {
    use std::io::Read;
    let dir = skills_dir(&state)?;

    let bytes: Vec<u8> = if source.starts_with("http://") || source.starts_with("https://") {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client.get(&source).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("Error HTTP {}", resp.status()));
        }
        resp.bytes().await.map_err(|e| e.to_string())?.to_vec()
    } else {
        std::fs::read(&source).map_err(|e| format!("No trobat {}: {}", source, e))?
    };

    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;
    let root_name = source
        .rsplit('/')
        .next()
        .unwrap_or("skill")
        .trim_end_matches(".zip")
        .to_string();
    let dest = dir.join(&root_name);
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let rel = match file.enclosed_name() {
            Some(p) => p,
            None => continue,
        };
        let out = dest.join(rel);
        if file.is_dir() {
            std::fs::create_dir_all(&out).ok();
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            std::fs::write(&out, buf).map_err(|e| e.to_string())?;
        }
    }

    let mgr = state.skills_manager.clone();
    let mut guard = mgr.lock().map_err(|e| e.to_string())?;
    guard.reload();
    guard
        .list()
        .into_iter()
        .find(|s| s.path.contains(&root_name))
        .map(Ok)
        .unwrap_or_else(|| Err(format!("El zip no conté cap skill vàlida ({})", root_name)))
}

/// Instal·la una skill des d'una carpeta local.
#[command]
pub async fn skills_install_local(
    state: State<'_, AppState>,
    path: String,
) -> Result<Skill, String> {
    let dir = skills_dir(&state)?;
    let src = PathBuf::from(&path);
    if !src.is_dir() {
        return Err(format!("La carpeta {} no existeix", path));
    }
    let name = src
        .file_name()
        .ok_or_else(|| "Ruta sense nom de carpeta".to_string())?
        .to_string_lossy()
        .to_string();
    let dest = dir.join(&name);
    let _ = std::fs::remove_dir_all(&dest);
    copy_dir_recursive(&src, &dest)?;

    let mgr = state.skills_manager.clone();
    let mut guard = mgr.lock().map_err(|e| e.to_string())?;
    guard.reload();
    guard
        .list()
        .into_iter()
        .find(|s| s.path.contains(&name))
        .map(Ok)
        .unwrap_or_else(|| Err(format!("La carpeta {} no conté cap skill", name)))
}

/// Restaura les skills preinstal·lades a la versió de fàbrica.
#[command]
pub async fn skills_restore_builtin(
    state: State<'_, AppState>,
) -> Result<Vec<Skill>, String> {
    let dir = skills_dir(&state)?;
    crate::skills::restore_builtin_skills(&dir);
    let mgr = state.skills_manager.clone();
    let mut guard = mgr.lock().map_err(|e| e.to_string())?;
    guard.reload();
    Ok(guard.list())
}

/// Copia totes les skills instal·lades a un directori extern, en format
/// estàndard `<dir>/<nom>/SKILL.md`: compatible amb OpenCode
/// (`~/.config/opencode/skills`), Claude (`~/.claude/skills`), Agents
/// (`~/.agents/skills`) i GitHub Copilot (`<projecte>/.github/skills`).
/// Accepta `~/...` com a prefixi. Retorna el nombre d'exports.
#[command]
pub async fn skills_export_external(
    state: State<'_, AppState>,
    directory: String,
) -> Result<usize, String> {
    let dest_root = if let Some(rest) = directory.strip_prefix("~/") {
        dirs::home_dir()
            .ok_or_else(|| "No s'ha trobat el directori de l'usuari".to_string())?
            .join(rest)
    } else if directory == "~" {
        dirs::home_dir().ok_or_else(|| "No s'ha trobat el directori de l'usuari".to_string())?
    } else {
        PathBuf::from(&directory)
    };
    if !dest_root.is_absolute() {
        return Err("Cal una ruta absoluta o començada per ~/".into());
    }
    std::fs::create_dir_all(&dest_root).map_err(|e| e.to_string())?;

    let mgr = state.skills_manager.clone();
    let list = {
        let guard = mgr.lock().map_err(|e| e.to_string())?;
        guard.list()
    };
    let mut n = 0;
    for skill in &list {
        let src_file = Path::new(&skill.path);
        let is_skill_md =
            src_file.file_name().map(|c| c == "SKILL.md").unwrap_or(false);
        let dest = dest_root.join(&skill.id);
        let _ = std::fs::remove_dir_all(&dest);
        if is_skill_md {
            // carpeta sencera (pot tenir scripts/ o references/)
            let src_dir = src_file.parent().ok_or_else(|| "Ruta sense carpeta".to_string())?;
            copy_dir_recursive(src_dir, &dest)?;
        } else {
            // fitxer .md pla: normalitza a <id>/SKILL.md
            std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            let raw = std::fs::read(src_file).map_err(|e| e.to_string())?;
            std::fs::write(dest.join("SKILL.md"), raw).map_err(|e| e.to_string())?;
        }
        n += 1;
    }
    Ok(n)
}

/// Desinstal·la una skill pel seu id o nom de carpeta.
#[command]
pub async fn skills_uninstall(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let dir = skills_dir(&state)?;
    let mgr = state.skills_manager.clone();
    let target: Option<String> = {
        let guard = mgr.lock().map_err(|e| e.to_string())?;
        guard
            .list()
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.path.clone())
    };
    match target {
        Some(path) => {
            let p = Path::new(&path);
            let skill_root = if p.file_name().map(|n| n == "SKILL.md").unwrap_or(false) {
                p.parent().unwrap_or(p).to_path_buf()
            } else {
                p.parent().unwrap_or(p).to_path_buf()
            };
            // només dins del directori de skills de l'usuari
            if skill_root.starts_with(&dir) {
                let _ = std::fs::remove_dir_all(&skill_root);
            } else {
                return Err("La skill és fora del directori d'instal·lació".into());
            }
            let mut guard = mgr.lock().map_err(|e| e.to_string())?;
            guard.reload();
            Ok(())
        }
        None => Err(format!("Skill '{}' no trobada", id)),
    }
}
