//! VE DE L'USUARI — donar ordres al xat parlant, pel botó de micròfon.
//!
//! El flux és senzill a propòsit: la vista web grava (MediaRecorder), envia
//! l'àudio ací i ací es passa a text. El text torna al composer com si
//! l'usuari l'haguera teclejat: pot corregir una paraula abans d'enviar-lo.
//!
//! Motors, en este ordre (guanya el primer que està llest):
//!  1. **macOS Speech** (`SFSpeechRecognizer`): gratuït, sense clau; amb el
//!     model de la llengua instal·lat, el so NO ix de l'ordinador.
//!  2. **whisper.cpp local** (`whisper-cli`): si l'usuari l'ha instal·lat i té
//!     un model `ggml-*.bin`, es reconeix totalment fora de línia.
//!  3. **Proveïdor compatible OpenAI**: `POST {base}/audio/transcriptions`
//!     amb la clau que l'usuari JA té configurada (OpenAI, Groq, etc.).
//!
//! Per què no es fa tot al navegador? Perquè el reconeixement del WebKit no
//! està garantit dins de l'app; en canvi, el sistema i els motors de dalt sí
//! que hi són sempre o quasi sempre. I si cap està llest, es diu QUÈ falta.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::api::AiProvider;
use crate::AppState;

#[cfg(target_os = "macos")]
mod macos;

/// Idioma per defecte del dictat: la interfície de NoOrbit és en català, però
/// la UI pot enviar el que convinga (p. ex. «es-ES» si l'usuari canvia d'idioma).
pub const DEFAULT_LOCALE: &str = "ca-ES";

/// Un motor de veu, tal com el panell el mostra (i el tooltip del botó).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceEngine {
    pub id: String,
    pub label: String,
    pub ready: bool,
    /// Què falta (o què cal saber) per fer-lo servir.
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatus {
    pub engines: Vec<VoiceEngine>,
    pub locale: String,
    /// El primer motor llest (o `None`): el botó el pot mostrar al tooltip.
    pub chosen: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub text: String,
    /// Motor que l'ha reconegut («macos», «whisper», «openai:…»).
    pub engine: String,
}

// ───────────────────────────────────────────────────────────────────────────
// Estat dels motors
// ───────────────────────────────────────────────────────────────────────────

pub fn status(state: &AppState, locale: Option<String>) -> VoiceStatus {
    let locale = locale.unwrap_or_else(|| DEFAULT_LOCALE.to_string());
    let mut engines = Vec::new();

    #[cfg(target_os = "macos")]
    {
        let (ready, detail) = macos::status();
        engines.push(VoiceEngine {
            id: "macos".into(),
            label: "Veu del sistema (macOS)".into(),
            ready,
            detail,
        });
    }

    let (w_ready, w_detail) = whisper_status();
    engines.push(VoiceEngine {
        id: "whisper".into(),
        label: "whisper.cpp local".into(),
        ready: w_ready,
        detail: w_detail,
    });

    let stt = stt_providers(state);
    engines.push(VoiceEngine {
        id: "openai".into(),
        label: stt
            .first()
            .map(|p| format!("{} (àudio)", p.name))
            .unwrap_or_else(|| "Proveïdor compatible OpenAI".into()),
        ready: !stt.is_empty(),
        detail: if stt.is_empty() {
            "Cap proveïdor amb clau configurat. Afig-ne un (OpenAI, Groq…) per \
             reconèixer la veu al núvol."
                .into()
        } else {
            format!(
                "S'enviarà l'àudio a {} amb la clau que ja tens configurada.",
                stt[0].name
            )
        },
    });

    let chosen = engines.iter().find(|e| e.ready).map(|e| e.id.clone());
    VoiceStatus {
        engines,
        locale,
        chosen,
    }
}

/// Dimensió mínima d'una gravació perquè valga la pena transcribir-la:
/// per davall d'això és un clic sense paraules i els motors donen errors rars.
const MIN_BYTES: usize = 1500;
const MAX_BYTES: usize = 60 * 1024 * 1024;

/// Passa l'àudio a text. `bytes` és el que ha gravat la vista web.
pub async fn transcribe(
    state: &AppState,
    bytes: Vec<u8>,
    mime: &str,
    locale: &str,
) -> Result<Transcript, String> {
    if bytes.len() < MIN_BYTES {
        return Err("La gravació és massa curta: mantín el micròfon i parla clar.".into());
    }
    if bytes.len() > MAX_BYTES {
        return Err("La gravació és massa llarga (màxim 60 MB).".into());
    }

    let dir = crate::config::AppConfig::data_dir()
        .ok_or_else(|| "No s'ha pogut trobar la carpeta de dades de NoOrbit.".to_string())?
        .join("voice");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("No s'ha pogut crear la carpeta de veu ({e})."))?;
    let path = dir.join(format!("veu-{}.{}", uuid::Uuid::new_v4(), extension(mime)));
    std::fs::write(&path, &bytes)
        .map_err(|e| format!("No s'ha pogut desar la gravació temporal ({e})."))?;

    let result = run_engines(state, &path, mime, locale).await;
    // Brossa zero: la gravació no s'ha de quedar al disc ni un minut més.
    let _ = std::fs::remove_file(&path);
    result
}

async fn run_engines(
    state: &AppState,
    path: &Path,
    mime: &str,
    locale: &str,
) -> Result<Transcript, String> {
    let mut errors: Vec<String> = Vec::new();

    // 1. Sistema (macOS): el primer i el més còmode.
    #[cfg(target_os = "macos")]
    {
        let p = path.to_path_buf();
        let loc = locale.to_string();
        match tokio::task::spawn_blocking(move || macos::transcribe(&p, &loc)).await {
            Ok(Ok(text)) if !text.trim().is_empty() => {
                return Ok(Transcript {
                    text: text.trim().to_string(),
                    engine: "macos".into(),
                });
            }
            Ok(Ok(_)) => errors.push("Veu del sistema: no s'ha entès cap paraula".into()),
            Ok(Err(e)) => errors.push(format!("Veu del sistema: {e}")),
            Err(e) => errors.push(format!("Veu del sistema: {e}")),
        }
    }

    // 2. whisper.cpp local.
    if let (Some(bin), Some(model)) = (whisper_bin(), whisper_model()) {
        let p = path.to_path_buf();
        let loc = locale.to_string();
        match tokio::task::spawn_blocking(move || whisper_run(&bin, &model, &p, &loc)).await {
            Ok(Ok(text)) if !text.trim().is_empty() => {
                return Ok(Transcript {
                    text: text.trim().to_string(),
                    engine: "whisper".into(),
                });
            }
            Ok(Ok(_)) => errors.push("whisper.cpp: no s'ha entès cap paraula".into()),
            Ok(Err(e)) => errors.push(format!("whisper.cpp: {e}")),
            Err(e) => errors.push(format!("whisper.cpp: {e}")),
        }
    }

    // 3. Qualsevol proveïdor compatible OpenAI amb clau.
    for provider in stt_providers(state) {
        match openai_transcribe(state, &provider, path, mime, locale).await {
            Ok(text) if !text.trim().is_empty() => {
                return Ok(Transcript {
                    text: text.trim().to_string(),
                    engine: format!("openai:{}", provider.name),
                });
            }
            Ok(_) => errors.push(format!("{}: no s'ha entès cap paraula", provider.name)),
            Err(e) => errors.push(format!("{}: {e}", provider.name)),
        }
    }

    if errors.is_empty() {
        return Err(
            "No hi ha cap motor de veu disponible. Al macOS, dona permís a \
             «Reconeixement de veu» (Ajustos del Sistema ▸ Privadesa i seguretat); \
             o instal·la `whisper-cli` amb un model `ggml-*.bin`; o configura un \
             proveïdor d'IA amb clau."
                .into(),
        );
    }
    Err(format!("No s'ha pogut reconèixer la veu. {}", errors.join(" · ")))
}

// ───────────────────────────────────────────────────────────────────────────
// whisper.cpp local
// ───────────────────────────────────────────────────────────────────────────

fn whisper_bin() -> Option<PathBuf> {
    ["whisper-cli", "whisper-cpp", "whisper"]
        .iter()
        .find_map(|name| which::which(name).ok())
}

fn whisper_model_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data) = crate::config::AppConfig::data_dir() {
        dirs.push(data.join("whisper"));
    }
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/whisper"));
        dirs.push(home.join(".cache/whisper"));
        dirs.push(home.join(".local/share/whisper.cpp/models"));
        dirs.push(home.join("whisper.cpp/models"));
        dirs.push(home.join("Library/Application Support/whisper.cpp/models"));
    }
    dirs.push(PathBuf::from("/opt/homebrew/share/whisper-cpp/models"));
    dirs.push(PathBuf::from("/usr/local/share/whisper-cpp/models"));
    dirs.push(PathBuf::from("/usr/share/whisper.cpp/models"));
    dirs
}

/// El model que es farà servir: la variable `NOORBIT_WHISPER_MODEL` mana; si
/// no, es busca un `ggml-*.bin` en els llocs habituals, preferint els models
/// mitjans (bona relació qualitat/velocitat en un portàtil).
fn whisper_model() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("NOORBIT_WHISPER_MODEL") {
        let p = PathBuf::from(raw);
        if p.is_file() {
            return Some(p);
        }
    }
    let rank = |name: &str| -> u8 {
        let n = name.to_lowercase();
        if n.contains("base") {
            0
        } else if n.contains("small") {
            1
        } else if n.contains("tiny") {
            2
        } else if n.contains("medium") {
            3
        } else {
            4
        }
    };
    let mut best: Option<(u8, PathBuf)> = None;
    for dir in whisper_model_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.extension().and_then(|e| e.to_str()) != Some("bin") || !name.contains("ggml") {
                continue;
            }
            // Els models «-en» són només anglès: no serveixen per al català.
            if name.contains("-en") {
                continue;
            }
            let r = rank(name);
            if best.as_ref().map(|(br, _)| r < *br).unwrap_or(true) {
                best = Some((r, path));
            }
        }
    }
    best.map(|(_, p)| p)
}

fn whisper_status() -> (bool, String) {
    match (whisper_bin(), whisper_model()) {
        (Some(bin), Some(model)) => (
            true,
            format!(
                "Reconeixement fora de línia amb {} i {}.",
                bin.display(),
                model
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("el model")
            ),
        ),
        (None, _) => (
            false,
            "No s'ha trobat `whisper-cli` (whisper.cpp). Instal·la'l i posa un \
             model `ggml-*.bin` a ~/.local/share/whisper."
                .into(),
        ),
        (_, None) => (
            false,
            "Hi ha `whisper-cli`, però falta el model `ggml-*.bin` a \
             ~/.local/share/whisper (o a NOORBIT_WHISPER_MODEL)."
                .into(),
        ),
    }
}

fn whisper_run(bin: &Path, model: &Path, audio: &Path, locale: &str) -> Result<String, String> {
    let wav = to_wav_16k(audio)?;
    let lang = language_code(locale);
    let out = Command::new(bin)
        .arg("-m")
        .arg(model)
        .arg("-f")
        .arg(&wav)
        .arg("-l")
        .arg(&lang)
        // Sense marques de temps: volem el text, no un subtítol.
        .arg("-nt")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("no s'ha pogut executar {}: {e}", bin.display()))?;
    if wav != audio {
        let _ = std::fs::remove_file(&wav);
    }
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err.lines().rev().take(3).collect::<Vec<_>>().join(" · ");
        return Err(if tail.trim().is_empty() {
            format!("whisper-cli ha fallat ({})", out.status)
        } else {
            tail
        });
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // Amb els models multilingües poden eixir etiquetes «[BLANK_AUDIO]».
    let clean = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('['))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(clean)
}

/// whisper.cpp només llig WAV; la gravació del navegador és m4a/webm/ogg.
/// Es convertix a 16 kHz mono amb ferramentes del sistema (sense dependències).
fn to_wav_16k(src: &Path) -> Result<PathBuf, String> {
    if src.extension().and_then(|e| e.to_str()) == Some("wav") {
        return Ok(src.to_path_buf());
    }
    let out = src.with_extension("16k.wav");

    #[cfg(target_os = "macos")]
    {
        if which::which("afconvert").is_ok() {
            let st = Command::new("afconvert")
                .args(["-f", "WAVE", "-d", "LEI16@16000", "-c", "1"])
                .arg(src)
                .arg(&out)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if matches!(st, Ok(s) if s.success()) && out.is_file() {
                return Ok(out);
            }
        }
    }

    if which::which("ffmpeg").is_ok() {
        let st = Command::new("ffmpeg")
            .args(["-y", "-i"])
            .arg(src)
            .args(["-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le"])
            .arg(&out)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if matches!(st, Ok(s) if s.success()) && out.is_file() {
            return Ok(out);
        }
    }
    Err(
        "per a whisper.cpp cal convertir l'àudio a WAV i no s'ha trobat ni \
         `afconvert` ni `ffmpeg`"
            .into(),
    )
}

// ───────────────────────────────────────────────────────────────────────────
// Proveïdor compatible OpenAI (POST /audio/transcriptions)
// ───────────────────────────────────────────────────────────────────────────

/// Proveïdors que poden transcriure: compatibles OpenAI, actius i amb clau
/// (o servidors locals, que no en demanen). Els que «semblen» d'àudio van
/// primer; si cap ho és, igualment s'intenten un a un.
fn stt_providers(state: &AppState) -> Vec<AiProvider> {
    let Ok(mgr) = state.api_manager.lock() else {
        return Vec::new();
    };
    let mut list: Vec<AiProvider> = mgr
        .providers
        .iter()
        .filter(|p| p.enabled)
        .filter(|p| p.kind != "anthropic" && p.kind != "native")
        .filter(|p| !p.base_url.trim().is_empty())
        .filter(|p| p.has_token() || crate::api::is_local_url(&p.base_url))
        .cloned()
        .collect();
    // Estable: primer els que tenen pinta de saber àudio, després la resta.
    list.sort_by_key(|p| !looks_like_audio(p));
    list
}

fn looks_like_audio(p: &AiProvider) -> bool {
    let hay = format!("{} {} {}", p.name, p.base_url, p.model).to_lowercase();
    hay.contains("openai.com")
        || hay.contains("whisper")
        || hay.contains("transcri")
        || hay.contains("audio")
        || hay.contains("groq")
}

fn stt_model(p: &AiProvider) -> String {
    let m = p.model.to_lowercase();
    if m.contains("whisper") || m.contains("transcri") {
        p.model.clone()
    } else {
        // El nom estàndard del servei d'àudio d'OpenAI i dels seus clons.
        "whisper-1".into()
    }
}

async fn openai_transcribe(
    state: &AppState,
    provider: &AiProvider,
    audio: &Path,
    mime: &str,
    locale: &str,
) -> Result<String, String> {
    let bytes = std::fs::read(audio).map_err(|e| format!("no s'ha pogut llegir l'àudio: {e}"))?;
    let filename = audio
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("veu.m4a")
        .to_string();

    let url = format!(
        "{}/audio/transcriptions",
        provider.base().trim_end_matches('/')
    );
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(filename)
        .mime_str(mime)
        .map_err(|e| format!("format d'àudio no vàlid: {e}"))?;
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", stt_model(provider))
        .text("language", language_code(locale))
        .text("response_format", "json");

    let req = state
        .ai_manager
        .http_client()
        .post(&url)
        .timeout(std::time::Duration::from_secs(120))
        .multipart(form);
    let req = provider.apply_auth(req);

    let resp = req.send().await.map_err(|e| {
        if e.is_timeout() {
            "el servei no ha respost (temps esgotat)".to_string()
        } else {
            format!("no s'ha pogut connectar: {e}")
        }
    })?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        let hint = match status.as_u16() {
            401 | 403 => " (la clau no s'ha acceptat)",
            404 => " (este servei no oferix /audio/transcriptions)",
            413 => " (l'àudio és massa gran)",
            429 => " (massa peticions o sense saldo)",
            _ => "",
        };
        return Err(format!("error {status}{hint}: {}", body.trim()));
    }
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("resposta no vàlida: {e}"))?;
    value
        .get("text")
        .and_then(|t| t.as_str())
        .map(|t| t.trim().to_string())
        .ok_or_else(|| "la resposta no portava cap text".to_string())
}

// ───────────────────────────────────────────────────────────────────────────
// Utilitats
// ───────────────────────────────────────────────────────────────────────────

/// Extensió coherent amb el MIME: els motors (i els serveis) la miren.
fn extension(mime: &str) -> &'static str {
    let m = mime.to_lowercase();
    if m.contains("mp4") || m.contains("m4a") || m.contains("aac") {
        "m4a"
    } else if m.contains("webm") {
        "webm"
    } else if m.contains("ogg") {
        "ogg"
    } else if m.contains("wav") {
        "wav"
    } else if m.contains("mpeg") || m.contains("mp3") {
        "mp3"
    } else {
        "m4a"
    }
}

/// «ca-ES» → «ca»: és el que demanen whisper i les APIs de transcripció.
fn language_code(locale: &str) -> String {
    locale
        .split(['-', '_'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("ca")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensio_segons_mime() {
        assert_eq!(extension("audio/mp4"), "m4a");
        assert_eq!(extension("audio/webm;codecs=opus"), "webm");
        assert_eq!(extension("audio/ogg"), "ogg");
        assert_eq!(extension("audio/x-wav"), "wav");
        assert_eq!(extension(""), "m4a");
    }

    #[test]
    fn codi_d_idioma() {
        assert_eq!(language_code("ca-ES"), "ca");
        assert_eq!(language_code("es_ES"), "es");
        assert_eq!(language_code(""), "ca");
    }

    #[test]
    fn el_model_d_audio_es_troba() {
        let p = AiProvider {
            id: "x".into(),
            name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            kind: "openai".into(),
            auth: crate::api::AuthScheme::Bearer,
            header_name: String::new(),
            token: "sk-x".into(),
            enabled: true,
        };
        assert_eq!(stt_model(&p), "whisper-1");
        assert!(looks_like_audio(&p));
    }
}
