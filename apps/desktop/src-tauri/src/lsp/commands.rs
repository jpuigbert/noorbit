//! Comandaments Tauri de la capa LSP (Fase 3c). Contracte EXACTE amb la UI:
//!
//!   lsp_completion(projectPath, path, line, col) → [{label, insertText, detail, kind}]
//!   lsp_hover(projectPath, path, line, col)      → {contents: string} | null
//!   lsp_references(projectPath, path, line, col) → [{path, line, col}]
//!   lsp_definition(projectPath, path, line, col) → {path, line, column, preview} | null
//!   lsp_rename(projectPath, path, line, col, newName) → {files, edits, paths} | null
//!   lsp_diagnostics(projectPath, path)           → [{line, col, severity, message, source}]
//!   lsp_status(projectPath)                      → {server, language, ready}
//!
//! Tots són asíncronics (spawn_blocking: el client és bloquejant amb temps
//! límit propi de 8 s) i DEGRADEN amb honestedat: llista buida o null, MAI un
//! error que trenque l'editor. Línia/columna d'entrada i eixida: 1-based.

use super::session::{self, CompletionEntry, Diag, RefLoc, RenameEdit, QUERY_TIMEOUT};
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

/// Resultat d'un renombrament aplicat: quants fitxers s'han tocat, quantes
/// edicions, i les rutes (relatives a l'arrel) perquè la UI rellegesqui els
/// fitxers oberts.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameResp {
    pub files: usize,
    pub edits: usize,
    pub paths: Vec<String>,
}

/// Converteix una columna UTF-16 (LSP) a desplaçament de BYTES dins d'una
/// línia (els offsets de Rust són bytes; fora del BMP, un caràcter = 2).
fn utf16_col_to_byte(line: &str, col: usize) -> usize {
    let mut utf16 = 0usize;
    for (i, ch) in line.char_indices() {
        if utf16 >= col {
            return i;
        }
        utf16 += ch.len_utf16();
    }
    line.len()
}

/// (línia, columna UTF-16) 0-based → desplaçament de bytes dins del text.
fn byte_pos(text: &str, line: usize, col: usize) -> Option<usize> {
    let mut off = 0usize;
    for _ in 0..line {
        off = text[off..].find('\n')? + off + 1;
    }
    let end = text[off..].find('\n').map(|i| off + i).unwrap_or(text.len());
    Some(off + utf16_col_to_byte(&text[off..end], col))
}

/// Aplica les edicions d'un renombrament al DISC: les ordena de la darrera a
/// la primera (així els desplaçaments anteriors no es mouen) i reescriu cada
/// fitxer tocat. Torna la llista de fitxers que han quedat ben escrits.
fn apply_rename_edits(
    root: &Path,
    files: &[(PathBuf, Vec<RenameEdit>)],
) -> Result<RenameResp, String> {
    let mut total_edits = 0usize;
    let mut paths: Vec<String> = Vec::new();
    for (file, edits) in files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue; // fitxer desaparegut entre la resposta i l'aplicació
        };
        let mut ed: Vec<RenameEdit> = edits.clone();
        ed.sort_by(|a, b| {
            (b.start_line, b.start_char, b.end_line, b.end_char)
                .cmp(&(a.start_line, a.start_char, a.end_line, a.end_char))
        });
        let mut out = text;
        let mut applied = 0usize;
        for e in &ed {
            let Some(start) = byte_pos(&out, e.start_line, e.start_char) else {
                continue;
            };
            let Some(end) = byte_pos(&out, e.end_line, e.end_char) else {
                continue;
            };
            if start > end || !out.is_char_boundary(start) || !out.is_char_boundary(end) {
                continue;
            }
            out.replace_range(start..end, &e.new_text);
            applied += 1;
        }
        if applied > 0 && std::fs::write(file, &out).is_ok() {
            let rel = file
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| file.to_string_lossy().to_string());
            paths.push(rel);
            total_edits += applied;
        }
    }
    Ok(RenameResp { files: paths.len(), edits: total_edits, paths })
}

/// Renombra el símbol sota el cursor amb el servidor LSP real i aplica les
/// edicions al disc. Null = sense servidor, sense resposta o res a canviar:
/// degradació honesta (la UI ho explica).
#[tauri::command]
pub async fn lsp_rename(
    project_path: String,
    path: String,
    line: usize,
    col: usize,
    new_name: String,
) -> Result<Option<RenameResp>, String> {
    let out = tauri::async_runtime::spawn_blocking(move || -> Result<Option<RenameResp>, String> {
        let (root, abs) = resolve(&project_path, &path);
        let sess = session::ensure_session(&root, &abs)?;
        let edits = sess.rename(&abs, line, col, &new_name)?;
        match edits {
            None => Ok(None),
            Some(files) => Ok(Some(apply_rename_edits(&root, &files)?)),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columna_utf16_a_bytes_respecta_bmp_i_fora() {
        // «aà𐐀»: a = 1 byte, à = 2 bytes, 𐐀 = 4 bytes (2 unitats UTF-16).
        let linia = "aà𐐀";
        assert_eq!(utf16_col_to_byte(linia, 0), 0); // la «a»
        assert_eq!(utf16_col_to_byte(linia, 1), 1); // la «à»
        assert_eq!(utf16_col_to_byte(linia, 2), 3); // el «𐐀» (col 2 → byte 3)
        assert_eq!(utf16_col_to_byte(linia, 4), 7); // final de línia
    }

    #[test]
    fn byte_pos_localitza_linia_i_columna() {
        let text = "abc\ndef\nghi";
        assert_eq!(byte_pos(text, 0, 2), Some(2));
        assert_eq!(byte_pos(text, 1, 1), Some(5));
        assert_eq!(byte_pos(text, 2, 0), Some(8));
        assert_eq!(byte_pos(text, 9, 0), None); // línia inexistent
    }

    #[test]
    fn aplica_edicions_desordenades_i_reescriu_el_disc() {
        let dir = std::env::temp_dir().join(format!("noorbit-lsp-rename-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("prova.txt");
        std::fs::write(&f, "hola mon\nadeu mon\n").unwrap();
        // Arriben desordenades (com pot fer un servidor): l'aplicació les
        // ordena i les dues queden bé.
        let edits = vec![
            RenameEdit {
                start_line: 1,
                start_char: 5,
                end_line: 1,
                end_char: 8,
                new_text: "MÓN".into(),
            },
            RenameEdit {
                start_line: 0,
                start_char: 0,
                end_line: 0,
                end_char: 4,
                new_text: "ADEU".into(),
            },
        ];
        let r = apply_rename_edits(&dir, &[(f.clone(), edits)]).unwrap();
        assert_eq!(r.files, 1);
        assert_eq!(r.edits, 2);
        assert_eq!(r.paths, vec!["prova.txt".to_string()]);
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "ADEU mon\nadeu MÓN\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edicio_fora_del_text_es_descarta_sense_trencar_les_altres() {
        let dir = std::env::temp_dir().join(format!("noorbit-lsp-rename-b-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("b.txt");
        std::fs::write(&f, "una línia").unwrap();
        let edits = vec![
            RenameEdit {
                start_line: 99, // fora del fitxer: es descarta
                start_char: 0,
                end_line: 99,
                end_char: 3,
                new_text: "NO".into(),
            },
            RenameEdit {
                start_line: 0,
                start_char: 0,
                end_line: 0,
                end_char: 3,
                new_text: "DUES".into(),
            },
        ];
        let r = apply_rename_edits(&dir, &[(f.clone(), edits)]).unwrap();
        assert_eq!(r.edits, 1);
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "DUES línia");
        let _ = std::fs::remove_dir_all(&dir);
    }
}