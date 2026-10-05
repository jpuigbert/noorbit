//! Comandes de sistema de fitxers.

use serde::Serialize;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

const IGNORED: &[&str] = &[
    "node_modules", ".git", "target", "__pycache__", ".venv", "dist", "build", ".next",
];

#[derive(Debug, Clone, Serialize)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    let display = path.clone();
    // Resol rutes relatives a l'arrel del projecte.
    let resolved = super::workspace::resolve_in_root(&path)
        .to_string_lossy()
        .to_string();
    tauri::async_runtime::spawn_blocking(move || std::fs::read_to_string(&resolved))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("No s'ha pogut llegir {}: {}", display, e))
}

#[tauri::command]
pub async fn write_file(path: String, content: String) -> Result<(), String> {
    let display = path.clone();
    // Resol rutes relatives a l'arrel del projecte i rebutja salts de nivel
    // amb «..». NO exigeix «dins de l'arrel» perquè «Desa com a…» pot escriure
    // fora del projecte legitimament (diàleg del sistema).
    let p = super::workspace::resolve_in_root(&path);
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("Ruta no vàlida.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&p, content)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("No s'ha pogut escriure {}: {}", display, e))
}

#[tauri::command]
pub async fn list_dir(path: String) -> Result<Vec<FileNode>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut nodes: Vec<FileNode> = Vec::new();
        let entries = std::fs::read_dir(&path).map_err(|e| e.to_string())?;
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if IGNORED.contains(&name.as_str()) {
                continue;
            }
            let is_dir = e.path().is_dir();
            nodes.push(FileNode {
                name,
                path: e.path().to_string_lossy().to_string(),
                is_dir,
            });
        }
        nodes.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
        Ok(nodes)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// retorna la ruta absoluta resolta relativa a l'arrel del workspace si cal.
pub fn resolve_path(path: &str) -> PathBuf {
    super::workspace::resolve_in_root(path)
}

/// Seguretat: resol `path` i verifica que queda DINS de l'arrel del projecte
/// obert. Segueix SYMLINKS (canonicalitzant l'ancestre existent més proper)
/// perquè un enllaç dins el projecte no pugui redirigir l'escriptura/esborrat
/// cap a fora (p. ex. `link -> ~/.ssh`).
fn guard_in_root(path: &str) -> Result<PathBuf, String> {
    let root = super::workspace::current_root()
        .ok_or_else(|| "No hi ha cap projecte obert.".to_string())?;
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("Arrel no resoluble: {}", e))?;
    let p = super::workspace::resolve_in_root(path);
    // Rebutja qualsevol «..» explícit a la ruta sol·licitada.
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("Ruta no vàlida.".into());
    }
    let resolved = nearest_existing_canonical(&p)?;
    if resolved.starts_with(&canonical_root) {
        Ok(resolved)
    } else {
        Err("La ruta és fora del projecte obert.".into())
    }
}

/// Canonicalitza la part EXISTENT més propera d'una ruta (el component final pot
/// no existir encara) i hi reafegeix el sufix restant. Això resol symlinks als
/// ancestants sense exigir que el destí ja existeixi.
fn nearest_existing_canonical(p: &Path) -> Result<PathBuf, String> {
    let mut suffix: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(canon) = cur.canonicalize() {
            let mut base = canon;
            for name in suffix.into_iter().rev() {
                base.push(name);
            }
            return Ok(base);
        }
        match cur.file_name() {
            Some(name) => {
                suffix.push(name.to_owned());
                if !cur.pop() {
                    return Err("Ruta no vàlida.".into());
                }
            }
            None => return Err("Ruta no vàlida.".into()),
        }
    }
}

/// Crea una carpeta nova (i els pares que calguin) dins del projecte.
#[tauri::command]
pub async fn create_dir(path: String) -> Result<(), String> {
    let p = guard_in_root(&path)?;
    std::fs::create_dir_all(&p)
        .map_err(|e| format!("No s'ha pogut crear la carpeta {}: {}", p.display(), e))
}

/// Esborra un fitxer o carpeta (recursivament) dins del projecte.
#[tauri::command]
pub async fn delete_path(path: String) -> Result<(), String> {
    let p = guard_in_root(&path)?;
    // Mai esborrar l'arrel sencera del projecte.
    if let Some(root) = super::workspace::current_root() {
        if let Ok(canon_root) = root.canonicalize() {
            if p == canon_root {
                return Err("No es pot esborrar l'arrel del projecte.".into());
            }
        }
    }
    let res = if p.is_dir() {
        std::fs::remove_dir_all(&p)
    } else {
        std::fs::remove_file(&p)
    };
    res.map_err(|e| format!("No s'ha pogut esborrar {}: {}", p.display(), e))
}

/// Reanomena/mou un fitxer o carpeta dins del projecte.
#[tauri::command]
pub async fn rename_path(from: String, to: String) -> Result<(), String> {
    let f = guard_in_root(&from)?;
    let t = guard_in_root(&to)?;
    if let Some(parent) = t.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::rename(&f, &t).map_err(|e| format!("No s'ha pogut reanomenar: {}", e))
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub preview: String,
    pub match_len: usize,
}

/// Extensions clarament BINÀRIES o de grans volums que MAI oferim com a codi
/// ni cerquem. Estratègia «llista negra»: en lloc de limitar a un conjunt
/// d'extensions de codi conegudes (que excloia llenguatges nous o fitxers
/// sense extensió com `Makefile`, `Dockerfile`, `CMakeLists.txt`), acceptem
/// QUALSEVOL fitxer que sembli text. `read_to_string` ja descarta els binaris
/// realment no-UTF-8, així que només cal evitar imatges, paquets, models…
const BINARY_EXT: &[&str] = &[
    // imatges / vídeo / àudio
    "png","jpg","jpeg","gif","bmp","tif","tiff","webp","ico","icns","heic","raw","cr2","nef",
    "mp4","m4v","mov","avi","mkv","webm","mp3","m4a","wav","flac","aac","ogg","opus","aiff",
    // paquets / binaris / libraries
    "zip","tar","gz","tgz","bz2","xz","7z","rar","jar","war","dmg","iso","pkg",
    "exe","dll","dylib","so","a","o","obj","bin","class","pyc","pyo","wasm","rlib","rmeta",
    // fonts
    "ttf","otf","woff","woff2","eot",
    // docs binaris / dades
    "pdf","doc","docx","xls","xlsx","ppt","pptx","ods","odt","rtf","epub",
    "db","sqlite","sqlite3","parquet","arrow","feather","npy","npz","pkl","pickle",
    // 3D / motor de joc
    "blend","blend1","fbx","glb","gltf","uasset","umap","unity","unitypackage",
];

/// Di si un fitxer (pel seu nom i extensió) és candidat a ser codi/text que la IA
/// pot veure i buscar. Accepta qualsevol extensió no binària, incloent-hi
/// fitxers sense extensió (Makefile, LICENSE, README, Dockerfile…).
fn looks_like_text(name: &str, ext: &str) -> bool {
    let e = ext.to_lowercase();
    if BINARY_EXT.contains(&e.as_str()) {
        return false;
    }
    // Sense extensió: només si el nom és «conegut» de text o no conté punts
    // (evita `archive.1024` etc.). Amb extensió no-binària: sempre candidate.
    if e.is_empty() {
        !name.contains('.')
    } else {
        true
    }
}

/// Recull ELS FITXERS REALS del projecte obert perquè la IA els puga veure
/// (context). Recorre recursivament, omet directoris ignorats (node_modules,
/// .git, target…), ignora binaris (no són UTF-8) i limita el total de
/// caràcters per no desbordar models petits.
#[tauri::command]
pub async fn collect_project_files(limit: usize) -> Result<Vec<ProjectFile>, String> {
    let root = super::workspace::current_root()
        .ok_or_else(|| "No hi ha cap projecte obert.".to_string())?;
    let limit = limit.max(1000);
    tauri::async_runtime::spawn_blocking(move || {
        let mut out: Vec<ProjectFile> = Vec::new();
        let mut total = 0usize;
        let mut stack: Vec<PathBuf> = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            if total >= limit {
                break;
            }
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for e in entries.flatten() {
                if total >= limit {
                    break;
                }
                let name = e.file_name().to_string_lossy().to_string();
                if IGNORED.contains(&name.as_str()) {
                    continue;
                }
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let ext = p
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if !looks_like_text(&name, &ext) {
                    continue;
                }
                // read_to_string falla en binaris → els saltem de manera natural.
                if let Ok(content) = std::fs::read_to_string(&p) {
                    let rel = p
                        .strip_prefix(&root)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/");
                    let clipped: String = if content.chars().count() > 1500 {
                        content.chars().take(1500).collect::<String>() + "\n… (contingut tallat)"
                    } else {
                        content
                    };
                    total += clipped.chars().count();
                    out.push(ProjectFile {
                        path: rel,
                        content: clipped,
                    });
                }
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Cerca de TEXT COMPLET al projecte obert (com el ⇧⌘F de VS Code):
/// ignora directoris pesats i fitxers binaris/grans, i retorna cada
/// coincidència amb ruta relativa, línia, columna i vista prèvia.
#[tauri::command]
pub async fn search_files(query: String, limit: usize) -> Result<Vec<SearchHit>, String> {
    let root = super::workspace::current_root()
        .ok_or_else(|| "No hi ha cap projecte obert.".to_string())?;
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(10, 300);
    tauri::async_runtime::spawn_blocking(move || {
        let mut hits: Vec<SearchHit> = Vec::new();
        let mut stack: Vec<PathBuf> = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            if hits.len() >= limit {
                break;
            }
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for e in entries.flatten() {
                if hits.len() >= limit {
                    break;
                }
                let name = e.file_name().to_string_lossy().to_string();
                if IGNORED.contains(&name.as_str()) {
                    continue;
                }
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let ext = p
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if !looks_like_text(&name, &ext) {
                    continue;
                }
                // Saltem fitxers enormes (>300 KB) per rapidesa.
                if e.metadata().map(|m| m.len() > 300_000).unwrap_or(true) {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(&p) else {
                    continue; // binaris: sense problema
                };
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                for (idx, raw_line) in content.lines().enumerate() {
                    if hits.len() >= limit {
                        break;
                    }
                    let lowered = raw_line.to_lowercase();
                    if let Some(byte_pos) = lowered.find(&needle) {
                        let column = lowered[..byte_pos].chars().count() + 1;
                        let preview: String = {
                            let t = raw_line.trim();
                            if t.chars().count() > 160 {
                                t.chars().take(160).collect::<String>() + "…"
                            } else {
                                t.to_string()
                            }
                        };
                        hits.push(SearchHit {
                            path: rel.clone(),
                            line: idx + 1,
                            column,
                            preview,
                            match_len: needle.chars().count(),
                        });
                    }
                }
            }
        }
        hits.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
        Ok(hits)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Clone, Serialize)]
pub struct Definition {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub preview: String,
}

/// «Ves a la definició» (F12) sense servidor LSP: busca al projecte patrons
/// de definició reals (def/function/class/let/const/fn/struct…) del símbol.
/// Heurística honesta — cobreix els casos habituals en Python/JS/TS/Rust —
/// i prioritza el fitxer on és el caller, que és on més probablement ho és.
#[tauri::command]
pub async fn find_definition(
    path: String,
    line: usize,
    column: usize,
    symbol: Option<String>,
) -> Result<Definition, String> {
    let root = super::workspace::current_root()
        .ok_or_else(|| "No hi ha cap projecte obert.".to_string())?;

    // 1) Primer ho preguntem a un SERVIDOR LSP REAL si n'hi ha un instal·lat
    //    per a aquest tipus de fitxer (rust-analyzer, clangd, pylsp…). ÉS la
    //    mateixa resolució que fa VS Code. Si no n'hi ha o falla, caurem en
    //    l'heurística de text pla (més avall). Tot bloquejant va a spawn_blocking.
    {
        let root_lsp = root.clone();
        let path_lsp = path.clone();
        let lsp = tokio::task::spawn_blocking(move || {
            let abs = super::workspace::resolve_in_root(&path_lsp);
            crate::lsp::definition(&root_lsp, &abs, line, column)
        })
        .await;
        if let Ok(Ok(loc)) = lsp {
            return Ok(Definition {
                path: loc.path,
                line: loc.line,
                column: loc.column,
                preview: loc.preview,
            });
        }
    }

    // 2) Respall heurístic: cerca patrons de definició per text pla.
    tokio::task::spawn_blocking(move || {
        // Determine el símbol: el que arriba de la UI o, si ve buit, la
        // paraula que hi ha just al cursor del fitxer indicat.
        let caller = super::workspace::resolve_in_root(&path);
        let content = std::fs::read_to_string(&caller).map_err(|e| e.to_string())?;
        let sym = match symbol {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => {
                let l = content.lines().nth(line.saturating_sub(1)).unwrap_or("");
                let ci = l.chars().count().min(column.saturating_sub(1));
                let chars: Vec<char> = l.chars().collect();
                let mut a = ci;
                let mut b = ci;
                while a > 0 && (chars[a - 1].is_alphanumeric() || chars[a - 1] == '_') {
                    a -= 1;
                }
                while b < chars.len() && (chars[b].is_alphanumeric() || chars[b] == '_') {
                    b += 1;
                }
                chars[a..b].iter().collect()
            }
        };
        if sym.len() < 2 || sym.chars().any(|c| !(c.is_alphanumeric() || c == '_')) {
            return Err("No hi ha cap símbol sota el cursor.".into());
        }

        static DEFPAT: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(r"(?x)
                ^\s*(?:
                      (?:async\s+)?def\s+SYM\b                     # Python
                    | (?:export\s+)?(?:const|let|var)\s+SYM\b       # JS/TS
                    | (?:export\s+)?(?:function|class)\s+SYM\b
                    | fn\s+SYM\b                                     # Rust
                    | (?:pub\s+)?(?:struct|enum|trait|type|mod)\s+SYM\b
                )
            ")
            .unwrap()
        });
        let pattern = DEFPAT.as_str().replace("SYM", &regex::escape(&sym));
        let re = regex::Regex::new(&pattern).map_err(|e| e.to_string())?;

        let mut candidates: Vec<Definition> = Vec::new();
        let caller_rel = caller
            .strip_prefix(&root)
            .map(|x| x.to_string_lossy().trim_start_matches('/').to_string())
            .unwrap_or_default();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                if p.is_dir() {
                    if name == ".git" || IGNORED.contains(&name.as_str()) {
                        continue;
                    }
                    stack.push(p);
                    continue;
                }
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
                if !looks_like_text(&name, &ext) {
                    continue;
                }
                let Ok(txt) = std::fs::read_to_string(&p) else { continue };
                for (i, l) in txt.lines().enumerate() {
                    if re.is_match(l) {
                        let rel = p
                            .strip_prefix(&root)
                            .map(|x| x.to_string_lossy().trim_start_matches('/').to_string())
                            .unwrap_or_else(|_| p.display().to_string());
                        candidates.push(Definition {
                            path: rel,
                            line: i + 1,
                            column: 1,
                            preview: l.trim().to_string(),
                        });
                        if candidates.len() > 40 {
                            break;
                        }
                    }
                }
            }
        }
        if candidates.is_empty() {
            return Err(format!("No he trobat la definició de «{}» al projecte.", sym));
        }
        // Prioritat: definició al mateix fitxer del caller; després, per
        // nom de fitxer i línia (resultat estable i previsible).
        candidates.sort_by(|a, b| {
            let pa = if a.path == caller_rel { 0 } else { 1 };
            let pb = if b.path == caller_rel { 0 } else { 1 };
            pa.cmp(&pb).then(a.path.cmp(&b.path)).then(a.line.cmp(&b.line))
        });
        Ok(candidates.into_iter().next().unwrap())
    })
    .await
    .map_err(|e| e.to_string())?
}
