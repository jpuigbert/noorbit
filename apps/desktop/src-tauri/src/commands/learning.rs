//! Comandes Tauri de l'APRENENATGE LOCAL.
//!
//! Tot aquí és LOCAL: el registre, les lliçons i el dataset viuen a la
//! carpeta de dades de NoOrbit; la destil·lació la fa el model d'Ollama de
//! la màquina (i, si no n'hi ha, una heurística que no necessita cap IA).
//! Res surt de l'ordinador tret que l'usuari exporti el dataset ell mateix.

use crate::ai::AiManager;
use crate::config::LearningConfig;
use crate::learning::{
    build_distill_prompt, heuristic_lessons, parse_lessons, Exchange, Lesson, LearningStats,
    LearningStore, AUTO_DISTILL_MIN, DISTILL_SYSTEM, DISTILL_TIMEOUT_S,
};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::{command, AppHandle, Emitter, State};
use tokio::sync::RwLock;

/// Xat intern de la destil·lació: aïllat del xat de l'usuari (ni la
/// cancel·lació ni el raonament se'n barrejen).
const DISTILL_SESSION: &str = "learning";

/// Estat complet de l'aprenentatge (per a la interfície).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningStatus {
    pub enabled: bool,
    pub auto_distill: bool,
    pub inject: bool,
    pub model: String,
    pub max_lessons: usize,
    pub stats: LearningStats,
    pub dataset_path: String,
    /// Hi ha Ollama en marxa? (sinó la destil·lació serà heurística).
    pub local_model: bool,
}

/// Lliçons + intercanvis recents (per a la interfície).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningSnapshot {
    pub lessons: Vec<Lesson>,
    pub exchanges: Vec<Exchange>,
}

/// Opcions que l'interfície pot canviar. Venen en camelCase (com la resta de
/// les comandes) i es barregen amb la configuració persistent (que és
/// snake_case, com la resta de `config.toml`): així un canvi parcial no
/// perdre la resta d'opcions.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningOptions {
    pub enabled: Option<bool>,
    pub auto_distill: Option<bool>,
    pub inject: Option<bool>,
    pub model: Option<String>,
    pub max_lessons: Option<usize>,
}

async fn status(state: &AppState) -> LearningStatus {
    let cfg = state.config.read().await;
    LearningStatus {
        enabled: cfg.learning.enabled,
        auto_distill: cfg.learning.auto_distill,
        inject: cfg.learning.inject,
        model: cfg.learning.model.clone(),
        max_lessons: cfg.learning.max_lessons,
        stats: state.learning.stats(),
        dataset_path: state.learning.dataset_path().to_string_lossy().to_string(),
        local_model: crate::ai::ollama_running(),
    }
}

/// Avisa la interfície que l'aprenentatge ha canviat (lliçons noves, etc.).
fn notify(app: &AppHandle) {
    let _ = app.emit("learning://changed", ());
}

#[command]
pub async fn learning_status(state: State<'_, AppState>) -> Result<LearningStatus, String> {
    Ok(status(&state).await)
}

/// Activa/desactiva l'aprenentatge i n'ajusta les opcions (es desa a
/// `config.toml`, igual que la resta de la configuració).
#[command]
pub async fn learning_set_config(
    state: State<'_, AppState>,
    config: LearningOptions,
) -> Result<LearningStatus, String> {
    {
        let mut c = state.config.write().await;
        let l = &mut c.learning;
        if let Some(v) = config.enabled {
            l.enabled = v;
        }
        if let Some(v) = config.auto_distill {
            l.auto_distill = v;
        }
        if let Some(v) = config.inject {
            l.inject = v;
        }
        if let Some(v) = config.model {
            l.model = v;
        }
        if let Some(v) = config.max_lessons {
            if v > 0 {
                l.max_lessons = v;
            }
        }
    }
    let snapshot = state.config.read().await.clone();
    snapshot.save().map_err(|e| e.to_string())?;
    Ok(status(&state).await)
}

/// Lliçons apreses + intercanvis recents.
#[command]
pub async fn learning_list(state: State<'_, AppState>) -> Result<LearningSnapshot, String> {
    Ok(LearningSnapshot {
        lessons: state.learning.lessons(),
        exchanges: state.learning.exchanges(40),
    })
}

/// Esborra una lliçó concreta.
#[command]
pub async fn learning_delete_lesson(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<LearningStatus, String> {
    state.learning.delete_lesson(&id);
    notify(&app);
    Ok(status(&state).await)
}

/// Esborra tot (lliçons + registre).
#[command]
pub async fn learning_clear(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<LearningStatus, String> {
    state.learning.clear();
    notify(&app);
    Ok(status(&state).await)
}

/// Destil·la ara les converses pendents (primer pla: l'usuari ho demana
/// explícitament). Retorna quantes lliçons noves s'han creat.
#[command]
pub async fn learning_distill(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<LearningStatus, String> {
    let ai = state.ai_manager.clone();
    let store = state.learning.clone();
    let cfg = { state.config.read().await.learning.clone() };
    if store.distilling.swap(true, Ordering::SeqCst) {
        // Ja n'hi ha una en marxa: no se'n encadena cap altra.
        return Ok(status(&state).await);
    }
    // Avisa que la destil·lació ha començat: la interfície pot mostrar que
    // la IA està aprenent (en segon pla).
    notify(&app);
    let res = distill_now(&ai, &store, &cfg).await;
    store.distilling.store(false, Ordering::SeqCst);
    res?;
    notify(&app);
    Ok(status(&state).await)
}

/// Exporta el registre en JSONL (per entrenar un model local amb LoRA:
/// LLaMA-Factory, Unsloth, Axolotl…). Retorna la ruta del fitxer.
#[command]
pub async fn learning_export(state: State<'_, AppState>) -> Result<String, String> {
    let (path, n) = state.learning.export_jsonl()?;
    if n == 0 {
        return Err(format!(
            "Encara no hi ha cap conversa al registre (a {}).",
            path.display()
        ));
    }
    Ok(path.to_string_lossy().to_string())
}

/// Registra un torn a mà (per a camins que no passen pel xat normal).
#[command]
pub async fn learning_record(
    app: AppHandle,
    state: State<'_, AppState>,
    question: String,
    answer: String,
    provider: Option<String>,
    model: Option<String>,
) -> Result<LearningStatus, String> {
    let cfg = { state.config.read().await.learning.clone() };
    if !cfg.enabled {
        return Ok(status(&state).await);
    }
    state.learning.record(
        &question,
        &answer,
        provider.as_deref().unwrap_or("desconegut"),
        model.as_deref().unwrap_or(""),
    );
    notify(&app);
    Ok(status(&state).await)
}

/// Registra un torn al registre d'aprenentatge (si l'usuari ho ha activat).
/// S'usa pels camins que no passen pel xat normal (especialistes, IAs web).
pub async fn record_turn(
    state: &AppState,
    question: &str,
    answer: &str,
    provider: &str,
    model: &str,
) {
    let enabled = { state.config.read().await.learning.enabled };
    if !enabled {
        return;
    }
    if question.trim().is_empty() || answer.trim().is_empty() {
        return;
    }
    state.learning.record(question, answer, provider, model);
}

/// Destil·lació: llegeix els intercanvis pendents i en treu lliçons.
/// Primer prova el model LOCAL; si no hi ha model (o no respon), aplica la
/// heurística de reserva, que no necessita cap IA.
async fn distill_now(
    ai: &Arc<AiManager>,
    store: &Arc<LearningStore>,
    cfg: &LearningConfig,
) -> Result<usize, String> {
    let pending = store.pending();
    if pending.is_empty() {
        return Ok(0);
    }
    // Els 12 més recents (el model petit no pot processar-ne més de cop).
    let batch: Vec<Exchange> = pending
        .iter()
        .rev()
        .take(12)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let upto = batch.last().map(|e| e.at).unwrap_or_else(|| chrono::Local::now().timestamp());
    let prompt = build_distill_prompt(&batch);
    let model = if cfg.model.trim().is_empty() {
        None
    } else {
        Some(cfg.model.as_str())
    };
    let raw = match tokio::time::timeout(
        Duration::from_secs(DISTILL_TIMEOUT_S),
        // Sessió pròpia («learning»): la destil·lació en segon pla NO pot
        // tocar el raonament ni la cancel·lació del xat de l'usuari.
        crate::ai::in_session(
            DISTILL_SESSION,
            ai.chat_local_only(&prompt, Some(DISTILL_SYSTEM), model),
        ),
    )
    .await
    {
        Ok(Ok(text)) => Some(text),
        Ok(Err(e)) => {
            eprintln!("NoOrbit/aprenentatge: el model local no ha pogut destil·lar ({e}).");
            None
        }
        Err(_) => {
            eprintln!("NoOrbit/aprenentatge: la destil·lació ha passat del temps.");
            None
        }
    };
    let mut lessons: Vec<Lesson> = match raw {
        Some(text) => parse_lessons(&text, "model local"),
        None => Vec::new(),
    };
    // El model no ha donat res útil (o no hi ha model): la heurística salva
    // el dia — l'aprenentatge funciona fins i tot sense cap IA instal·lada.
    if lessons.is_empty() {
        lessons = heuristic_lessons(&batch);
    }
    let n = store.add_lessons(lessons, cfg.max_lessons);
    store.mark_distilled_upto(upto);
    Ok(n)
}

/// Destil·lació automàtica EN SEGON PLA: es dispara quan ja hi ha prou
/// converses pendents. No bloqueja el xat ni el consumeix de primer pla.
pub fn maybe_auto_distill(
    app: AppHandle,
    ai: Arc<AiManager>,
    store: Arc<LearningStore>,
    cfg: Arc<RwLock<crate::config::AppConfig>>,
) {
    tauri::async_runtime::spawn(async move {
        let learning_cfg = { cfg.read().await.learning.clone() };
        if !learning_cfg.enabled || !learning_cfg.auto_distill {
            return;
        }
        if store.pending().len() < AUTO_DISTILL_MIN {
            return;
        }
        // RAM: la destil·lació fa servir el model LOCAL. Si l'usuari treballa
        // amb un proveïdor en línia, carregar un model local a la RAM «de
        //Cop» seria una sorpresa: en aquest cas la destil·lació automàtica
        // s'espera (l'usuari pot fer-la amb «Aprendre ara»).
        let provider = ai.active_provider().await.to_lowercase();
        if !provider.is_empty() && provider != "ollama" {
            return;
        }
        // Una destil·lació alhora: si ja n'hi ha una en marxa, aquesta espera.
        if store.distilling.swap(true, Ordering::SeqCst) {
            return;
        }
        // La interfície ha de saber que la IA està aprenent (en segon pla).
        let _ = app.emit("learning://changed", ());
        let res = distill_now(&ai, &store, &learning_cfg).await;
        store.distilling.store(false, Ordering::SeqCst);
        if let Ok(n) = res {
            if n > 0 {
                let _ = app.emit("learning://changed", ());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// L'interfície envia les opcions en camelCase (com la resta de comandes)
    /// i la configuració persistent és snake_case (com la resta de
    /// `config.toml`). Aquest test ho fixa: si algú canvia una de les dues
    /// convencions, l'opció es perdria en silenci.
    #[test]
    fn accepts_camel_case_options_from_the_ui() {
        let opts: LearningOptions = serde_json::from_str(
            r#"{"enabled":true,"autoDistill":false,"inject":true,"model":"qwen3:4b","maxLessons":50}"#,
        )
        .unwrap();
        assert_eq!(opts.enabled, Some(true));
        assert_eq!(opts.auto_distill, Some(false));
        assert_eq!(opts.inject, Some(true));
        assert_eq!(opts.model.as_deref(), Some("qwen3:4b"));
        assert_eq!(opts.max_lessons, Some(50));
    }

    /// Un canvi parcial no pot esborrar la resta d'opcions.
    #[test]
    fn partial_options_leave_the_rest_untouched() {
        let opts: LearningOptions = serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert_eq!(opts.enabled, Some(true));
        assert_eq!(opts.auto_distill, None);
        assert_eq!(opts.inject, None);
    }
}
