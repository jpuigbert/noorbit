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

/// Valida un nom de skill derivat d'entrada externa (repo, URL, zip): ha de ser
/// un únic component de ruta. Sense això, un nom com `..` fa que
/// `remove_dir_all(dir.join(name))` esborri la carpeta de dades de NoOrbit.
fn safe_name(name: &str, fallback: &str) -> String {
    let n = name.trim();
    if !n.is_empty()
        && n != "."
        && n != ".."
        && !n.contains('/')
        && !n.contains('\\')
        && !n.contains('\0')
    {
        n.to_string()
    } else {
        fallback.to_string()
    }
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        let dest = to.join(entry.file_name());
        // file_type NO segueix symlinks: un enllaç a un directori es copia com
        // a enllaç, no com a contingut (evita bucles infinits amb cicles).
        let is_real_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        // Els SYMLINKS es salten: el seu contingut podseria venir de fora del
        // repositori (i copiant-lo es duria el que altres enllocs no tenen).
        if entry.file_type().map(|t| t.is_symlink()).unwrap_or(false) {
            continue;
        }
        if is_real_dir {
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
    // El clonatge pot trigar MINUTS (xarxa): es fa en un fil a part perquè
    // el runtime async no quedi congelat (i la interfície, bloquejada).
    let url_clone = url.clone();
    let tmp_clone = tmp.clone();
    let cloned = tauri::async_runtime::spawn_blocking(move || {
        let mut cmd = std::process::Command::new("git");
        cmd.args(["clone", "--depth", "1"]);
        if let Some(b) = &branch {
            cmd.args(["--branch", b]);
        }
        cmd.arg(&url_clone)
            .arg(&tmp_clone)
            .status()
            .map_err(|e| format!("No s'ha pogut executar git: {}", e))
    })
    .await
    .map_err(|e| e.to_string())?;
    let status = cloned.map_err(|e| e)?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("git clone ha fallat per {}", url));
    }

    let src = if name_sub.is_empty() { tmp.clone() } else { tmp.join(&name_sub) };
    if !src.is_dir() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("La carpeta {} no existeix al repositori", name_sub));
    }

    let skill_name = safe_name(
        name_sub
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(&owner),
        &owner,
    );
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

/// Mida màxima d'un SKILL.md descarregat (2 MB és amplíssim per a un
/// document de skill): evita que una URL mal-intencionada omplui el disc.
const MAX_SKILL_BYTES: u64 = 2 * 1024 * 1024;

/// Límits per als zips de skills: 20 MB comprimits i 50 MB descomprimits
/// (evita que un zip enorme — o una «bomba zip» — ompli el disc).
const MAX_SKILL_ZIP_BYTES: u64 = 20 * 1024 * 1024;
const MAX_SKILL_EXTRACT_BYTES: u64 = 50 * 1024 * 1024;

/// Instal·la una skill des d una URL directa a un fitxer .md.
#[command]
pub async fn skills_install_url(
    state: State<'_, AppState>,
    url: String,
) -> Result<Skill, String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("La URL ha de començar per http(s)://".into());
    }
    // Fora de localhost, només HTTPS: el contingut d'una skill és codi que
    // l'agent pot arribar a executar.
    if url.starts_with("http://")
        && !url.contains("127.0.0.1")
        && !url.contains("localhost")
    {
        return Err("Cal una URL https:// (o http:// a localhost).".into());
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
    // Límit de mida: es comprova el Content-Length i, si no hi ha, es llegeix
    // com a molt fins al límit.
    if let Some(len) = resp.content_length() {
        if len > MAX_SKILL_BYTES {
            return Err(format!(
                "El fitxer és massa gran ({} MB; el màxim és {} MB)",
                len / 1_000_000,
                MAX_SKILL_BYTES / 1_000_000
            ));
        }
    }
    let mut body = Vec::new();
    let mut stream = resp.bytes_stream();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        if body.len() as u64 + chunk.len() as u64 > MAX_SKILL_BYTES {
            return Err("El fitxer és massa gran (màxim 2 MB).".into());
        }
        body.extend_from_slice(&chunk);
    }
    let body = String::from_utf8_lossy(&body).to_string();

    let name = safe_name(
        url.rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("skill.md"),
        "skill.md",
    );
    let name = name.trim_end_matches(".md").to_string();
    let name = safe_name(&name, "skill");
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
        // Mateixa regla que skills_install_url: fora de localhost, només
        // HTTPS (el contingut d'una skill és codi que l'agent pot executar).
        if source.starts_with("http://")
            && !source.contains("127.0.0.1")
            && !source.contains("localhost")
        {
            return Err("Cal una URL https:// (o http:// a localhost).".into());
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client.get(&source).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("Error HTTP {}", resp.status()));
        }
        if let Some(len) = resp.content_length() {
            if len > MAX_SKILL_ZIP_BYTES {
                return Err(format!(
                    "El zip és massa gran ({} MB; el màxim és {} MB)",
                    len / 1_000_000,
                    MAX_SKILL_ZIP_BYTES / 1_000_000
                ));
            }
        }
        // Es llegeix com a molt fins al límit (sense Content-Length també).
        let mut body = Vec::new();
        let mut stream = resp.bytes_stream();
        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| e.to_string())?;
            if body.len() as u64 + chunk.len() as u64 > MAX_SKILL_ZIP_BYTES {
                return Err("El zip és massa gran (màxim 20 MB).".into());
            }
            body.extend_from_slice(&chunk);
        }
        body
    } else {
        let meta = std::fs::metadata(&source)
            .map_err(|e| format!("No trobat {}: {}", source, e))?;
        if meta.len() > MAX_SKILL_ZIP_BYTES {
            return Err("El zip és massa gran (màxim 20 MB).".into());
        }
        std::fs::read(&source).map_err(|e| format!("No trobat {}: {}", source, e))?
    };

    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;
    let root_name = source.rsplit('/').next().unwrap_or("skill");
    let root_name = root_name.trim_end_matches(".zip");
    let root_name = safe_name(root_name, "skill");
    let dest = dir.join(&root_name);
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    let mut total_extracted: u64 = 0;
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
            total_extracted += buf.len() as u64;
            if total_extracted > MAX_SKILL_EXTRACT_BYTES {
                let _ = std::fs::remove_dir_all(&dest);
                return Err("El zip descomprimit és massa gran (màxim 50 MB).".into());
            }
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
            // Tant si és un SKILL.md com un fitxer .md pla, la skill és la
            // carpeta que el conté.
            let skill_root = p.parent().unwrap_or(p).to_path_buf();
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

#[cfg(test)]
mod proves {
    use super::safe_name;

    /// Un nom de skill derivat de l'exterior ha de ser un únic component de
    /// ruta: si no, `remove_dir_all(dir.join(name))` esborria la carpeta de
    /// dades de NoOrbit (o qualsevol cosa del sistema).
    #[test]
    fn els_noms_de_skill_invalids_es_descarten() {
        assert_eq!(safe_name("python", "fallback"), "python");
        assert_eq!(safe_name("  meu-skill  ", "fallback"), "meu-skill");
        // Els perillosos cauen al nom segur.
        assert_eq!(safe_name("..", "fallback"), "fallback");
        assert_eq!(safe_name(".", "fallback"), "fallback");
        assert_eq!(safe_name("", "fallback"), "fallback");
        assert_eq!(safe_name("../../etc", "fallback"), "fallback");
        assert_eq!(safe_name("a/b", "fallback"), "fallback");
        assert_eq!(safe_name("a\\b", "fallback"), "fallback");
    }
}
