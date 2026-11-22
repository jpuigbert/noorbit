//! Comandes Tauri per a generació i manipulació d'imatges.
//!
//! `image_generate`/`image_edit` moren opcions de `crate::imgen`: proven el
//! backend local (ComfyUI) només si la màquina té prou RAM lliure, i si no,
//! els proveïdors en línia oficials que l'usuari haja registrat amb token.
//! Sense credencials explícites, el camí online NO s'intenta.
//! `image_transform` fa operacions de píxel deterministes (sense IA).

use crate::imgen::{self, GenOpts, GeneratedImage};
use crate::AppState;
use tauri::{command, State};

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageGenArgs {
    pub prompt: String,
    #[serde(default)]
    pub negative: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub steps: Option<u32>,
    /// 0 = llavor aleatòria.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Model concret (checkpoint de ComfyUI o model d'imatges remot).
    #[serde(default)]
    pub model: Option<String>,
}

fn into_opts(a: ImageGenArgs) -> GenOpts {
    let mut o = GenOpts::default();
    o.prompt = a.prompt;
    if let Some(n) = a.negative.filter(|s| !s.trim().is_empty()) {
        o.negative = n;
    }
    if let Some(w) = a.width.filter(|w| *w > 0) {
        o.width = w;
    }
    if let Some(h) = a.height.filter(|h| *h > 0) {
        o.height = h;
    }
    if let Some(s) = a.steps.filter(|s| *s > 0) {
        o.steps = s;
    }
    if let Some(s) = a.seed {
        o.seed = s;
    }
    o.model = a.model.filter(|m| !m.trim().is_empty());
    o
}

/// Genera una imatge a partir de text (local si la màquina aguanta, si no
/// online amb les credencials que l'usuari haja donat).
#[command]
pub async fn image_generate(
    state: State<'_, AppState>,
    args: ImageGenArgs,
) -> Result<GeneratedImage, String> {
    let opts = into_opts(args);
    let remote = state.ai_manager.remote_manager();
    imgen::generate(remote, &opts)
        .await
        .map_err(|e| e.to_string())
}

/// Edita una imatge existent amb una instrucció de text. `strength` (0..1)
/// indica com de fort es reinterpreta la imatge original.
#[command]
pub async fn image_edit(
    state: State<'_, AppState>,
    path: String,
    args: ImageGenArgs,
    strength: Option<f64>,
) -> Result<GeneratedImage, String> {
    let opts = into_opts(args);
    let remote = state.ai_manager.remote_manager();
    imgen::edit(remote, std::path::Path::new(&path), &opts, strength.unwrap_or(0.6))
        .await
        .map_err(|e| e.to_string())
}

/// Desa una imatge enganxada al xat (base64 del porta-retalls) a la carpeta
/// de dades i retorna'n la ruta absoluta. La UI la mostra i els models amb
/// visió d'Ollama la poden rebre com a imatge adjunta.
#[command]
pub async fn image_import(
    data_base64: String,
    name: Option<String>,
) -> Result<String, String> {
    // Descodificar i escriure una imatge pot trigar (I/O + codec): en un
    // fil a part perquè el runtime async no es bloquegi.
    tauri::async_runtime::spawn_blocking(move || {
        imgen::import_base64(&data_base64, name.as_deref()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Desa un fitxer QUALSIVOL adjuntat al xat (codi, documents, dades…) a la
/// carpeta de dades «adjunts/» i en retorna la ruta absoluta. Serveix per al
/// que l'usuari enganxa o tria des del disc, que no arriba amb ruta real.
#[command]
pub async fn file_import(data_base64: String, name: Option<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        imgen::attach_base64(&data_base64, name.as_deref().unwrap_or("fitxer"))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, serde::Serialize)]
pub struct FileContext {
    pub name: String,
    pub path: String,
    pub text: String,
    pub truncated: bool,
    pub is_binary: bool,
}

/// Llegeix un fitxer adjunt al xat perquè la IA en veja el contingut REAL.
/// Els fitxers arrossegats des de fora del projecte no són accessibles amb
/// l'eina «read_file» (limitada a l'arrel); ací el contingut s'envia directament
/// dins del missatge. Màx. 2 MB / 60.000 caràcters; els binaris només fan
/// constar la ruta.
#[command]
pub async fn file_context_read(path: String) -> Result<FileContext, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = std::path::Path::new(&path);
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        if !p.exists() {
            return Err(format!("El fitxer no existeix: {}", path));
        }
        if p.is_dir() {
            return Ok(FileContext {
                name,
                path,
                text: "És una CARPETA, no un fitxer. Adjunta els fitxers individuals per veure'n el contingut.".into(),
                truncated: false,
                is_binary: false,
            });
        }
        let bytes = std::fs::read(p).map_err(|e| format!("No s'ha pogut llegir: {}", e))?;
        if bytes.len() > 2_000_000 {
            return Ok(FileContext {
                name,
                path,
                text: "(fitxer de més de 2 MB: no s'ha inclòs el contingut; la IA pot copiar-lo al projecte amb una comanda RUN| si cal)".into(),
                truncated: true,
                is_binary: false,
            });
        }
        let is_binary = bytes.iter().take(8192).any(|b| *b == 0);
        if is_binary {
            return Ok(FileContext {
                name,
                path,
                text: "(fitxer BINARI: contingut no llegível com a text; la ruta és disponible per a les eines)".into(),
                truncated: false,
                is_binary: true,
            });
        }
        let lossy = String::from_utf8_lossy(&bytes).to_string();
        let max = 60_000usize;
        let count = lossy.chars().count();
        let text: String = lossy.chars().take(max).collect();
        Ok(FileContext {
            name,
            path,
            text,
            truncated: count > max,
            is_binary: false,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Valida la SIGNATURA (magic bytes) del format declarat per l'extensió.
/// L'extensió sola no basta: la IA pot escriure text pla dins d'un .png i
/// el data-URL resultant seria indecodificable per la webview.
fn magic_ok(mime: &str, bytes: &[u8]) -> bool {
    match mime {
        "image/png" => bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]),
        "image/jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/gif" => bytes.starts_with(b"GIF8"),
        "image/webp" => {
            bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
        }
        "image/bmp" => bytes.starts_with(b"BM"),
        _ => false,
    }
}

/// Cos síncron de `image_preview_base64` (testeable sense runtime Tauri).
fn preview_data_url(p: &std::path::Path) -> Result<String, String> {
    use base64::Engine as _;
    if !p.is_file() {
        return Err("La imatge no existeix".into());
    }
    let mime = match p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()) {
        Some(s) if s == "png" => "image/png",
        Some(s) if s == "jpg" || s == "jpeg" => "image/jpeg",
        Some(s) if s == "webp" => "image/webp",
        Some(s) if s == "gif" => "image/gif",
        Some(s) if s == "bmp" => "image/bmp",
        _ => return Err("Format no suportat com a imatge".into()),
    };
    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if meta.len() > 25_000_000 {
        return Err("Imatge massa gran per mostrar-la al xat (màx. 25 MB)".into());
    }
    let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
    if !magic_ok(mime, &bytes) {
        return Err("El fitxer no és una imatge vàlida (contingut no reconegut)".into());
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

/// Llegeix una imatge local i la retorna com a data-URL, per que la UI la
/// mostri sense activar el protocol d'assets (menys superfície d'atac).
/// Només imatges conegudes i fins a 25 MB, amb la signatura del format
/// verificada (un .png amb text pla NO és una imatge).
#[command]
pub async fn image_preview_base64(path: String) -> Result<String, String> {
    preview_data_url(std::path::Path::new(&path))
}

/// Operació de píxel determinista: resize, crop, rotate, flip_h, flip_v o
/// grayscale. No cal IA ni recursos grans.
#[command]
pub async fn image_transform(
    path: String,
    op: String,
    width: Option<u32>,
    height: Option<u32>,
    x: Option<u32>,
    y: Option<u32>,
    angle: Option<u32>,
) -> Result<GeneratedImage, String> {
    // Decodificació + codificació d'una imatge pot trigar (codec, imatges
    // grans): en un fil a part perquè el runtime async no es bloquegi.
    tauri::async_runtime::spawn_blocking(move || {
        imgen::transform(
            std::path::Path::new(&path),
            &op,
            width.unwrap_or(0),
            height.unwrap_or(0),
            x.unwrap_or(0),
            y.unwrap_or(0),
            angle.unwrap_or(0),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::preview_data_url;
    use std::io::Write as _;

    /// Capçalera PNG real mínima: signatura de 8 bytes + chunk IHDR.
    const PNG_HEADER: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // signatura
        0x00, 0x00, 0x00, 0x0D, // llargària IHDR
        0x49, 0x48, 0x44, 0x52, // «IHDR»
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1×1
        0x08, 0x06, 0x00, 0x00, 0x00, // RGBA 8 bits
        0x1F, 0x15, 0xC4, 0x89, // CRC
    ];

    fn write_tmp(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(bytes).unwrap();
        p
    }

    #[test]
    fn png_valid_retorna_data_url() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_tmp(dir.path(), "cara.png", PNG_HEADER);
        let url = preview_data_url(&p).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn text_pla_dins_de_png_es_rebutja() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_tmp(
            dir.path(),
            "face_diffuse.png",
            "[Contingut de la textura de la cara]".as_bytes(),
        );
        let err = preview_data_url(&p).unwrap_err();
        assert!(err.contains("no és una imatge vàlida"), "err: {err}");
    }

    #[test]
    fn png_truncat_es_rebutja() {
        let dir = tempfile::tempdir().unwrap();
        // Tallat a mitja signatura: ni tan sols els 4 bytes màgics hi són.
        let p = write_tmp(dir.path(), "tallat.png", &PNG_HEADER[..3]);
        assert!(preview_data_url(&p).is_err());
    }

    #[test]
    fn png_corrupt_es_rebutja() {
        let dir = tempfile::tempdir().unwrap();
        let mut bytes = PNG_HEADER.to_vec();
        bytes[0] = 0x00; // signatura trencada
        let p = write_tmp(dir.path(), "corrupt.png", &bytes);
        assert!(preview_data_url(&p).is_err());
    }

    #[test]
    fn jpeg_valid() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_tmp(dir.path(), "foto.jpg", &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]);
        assert!(preview_data_url(&p).unwrap().starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn gif_valid() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_tmp(dir.path(), "anim.gif", b"GIF89a\x01\x00\x01\x00");
        assert!(preview_data_url(&p).unwrap().starts_with("data:image/gif;base64,"));
    }

    #[test]
    fn webp_valid() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_tmp(dir.path(), "foto.webp", b"RIFF\x24\x00\x00\x00WEBPVP8 ");
        assert!(preview_data_url(&p).unwrap().starts_with("data:image/webp;base64,"));
    }

    #[test]
    fn webp_amb_riff_d_altre_format_es_rebutja() {
        let dir = tempfile::tempdir().unwrap();
        // RIFF d'àudio (WAVE) dins d'un .webp: no és una imatge.
        let p = write_tmp(dir.path(), "so.webp", b"RIFF\x24\x00\x00\x00WAVEfmt ");
        assert!(preview_data_url(&p).is_err());
    }

    #[test]
    fn bmp_valid() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_tmp(dir.path(), "mapa.bmp", b"BM\x36\x00\x0C\x00\x00\x00");
        assert!(preview_data_url(&p).unwrap().starts_with("data:image/bmp;base64,"));
    }

    #[test]
    fn extensio_no_suportada() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_tmp(dir.path(), "notes.txt", b"hola");
        assert!(preview_data_url(&p).unwrap_err().contains("Format no suportat"));
    }

    #[test]
    fn fitxer_inexistent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(preview_data_url(&dir.path().join("no-hi-es.png")).is_err());
    }

    #[test]
    fn limit_de_mida_es_mante() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("gegant.png");
        let f = std::fs::File::create(&p).unwrap();
        // Fitxer espars: metadata.len > 25 MB sense escriure 25 MB reals.
        f.set_len(25_000_001).unwrap();
        assert!(preview_data_url(&p).unwrap_err().contains("massa gran"));
    }
}
