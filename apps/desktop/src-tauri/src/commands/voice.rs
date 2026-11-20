//! Comandes Tauri del dictat per veu (el botó de micròfon del xat).
//!
//! La vista web grava i envia l'àudio en base64; ací només es valida, es
//! passa al motor (`voice::transcribe`) i torna el text. El fitxer temporal
//! es crea i s'esborra dins del mateix motor: res no queda al disc.

use base64::Engine as _;
use tauri::State;

use crate::voice::{self, Transcript, VoiceStatus};
use crate::AppState;

/// Quins motors de veu hi ha i quin es farà servir (per al tooltip del botó).
#[tauri::command]
pub fn voice_status(state: State<'_, AppState>, locale: Option<String>) -> VoiceStatus {
    voice::status(state.inner(), locale)
}

/// Passa a text la gravació feta al composer.
#[tauri::command]
pub async fn voice_transcribe(
    state: State<'_, AppState>,
    audio_base64: String,
    mime: Option<String>,
    locale: Option<String>,
) -> Result<Transcript, String> {
    let mime = mime.unwrap_or_else(|| "audio/mp4".to_string());
    let locale = locale.unwrap_or_else(|| voice::DEFAULT_LOCALE.to_string());
    // El prefix «data:audio/…;base64,» no ha d'arribar al descodificador.
    let payload = audio_base64
        .split_once("base64,")
        .map(|(_, rest)| rest)
        .unwrap_or(&audio_base64);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|e| format!("l'àudio ha arribat malmès ({e})"))?;
    voice::transcribe(state.inner(), bytes, &mime, &locale).await
}
