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
    imgen::import_base64(&data_base64, name.as_deref()).map_err(|e| e.to_string())
}

/// Llegeix una imatge local i la retorna com a data-URL, per que la UI la
/// mostri sense activar el protocol d'assets (menys superfície d'atac).
/// Només imatges conegudes i fins a 25 MB.
#[command]
pub async fn image_preview_base64(path: String) -> Result<String, String> {
    use base64::Engine as _;
    let p = std::path::Path::new(&path);
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
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
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
}
