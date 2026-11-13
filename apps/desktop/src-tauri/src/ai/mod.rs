//! Gestió de proveïdors d'IA local (Ollama) per a NoOrbit.

use crate::api::{self, ApiManager};
use crate::config::AppConfig;
use crate::external_orchestrator::OrchestratorAgent;

/// Compactació Densa del context (resum que conserva l'esperit del programa).
pub mod compact;

/// Hugging Bay (huggingbay.xyz): cerca i descàrrega de models GGUF.
pub mod hf_bay;

/// Motor local sense Ollama: llama.cpp («llama-server»).
pub mod llamacpp;

mod cloud;
mod reasoning;
mod repetition;

pub use repetition::runaway_repetition;
use cloud::parse_cloud_models;
use reasoning::{chunk_thinking, split_inline_reasoning, ThoughtSplitter};

use anyhow::{anyhow, Result};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{Notify, RwLock};
#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Error compartit: la generació s'ha aturat a petició de l'usuari.
pub const CANCELLED_MSG: &str = "NoOrbit: cancel·lat";

/// Xat per defecte: el que fan servir l'agent, les tasques autònomes i
/// qualsevol crida que no s'haja iniciat dins d'un xat múltiple.
pub const DEFAULT_SESSION: &str = "main";

/// Què està fent REALMENT Ollama mentre encara no ha mostrat text. Abans
/// NoOrbit ho deia tot endevinant («està CARREGANT el model»…) i repetia la
/// mateixa línia cada 15 s; ara ho pregunta a Ollama amb «/api/ps».
#[derive(Debug, Clone, Copy, PartialEq)]
enum WaitPhase {
    /// Ollama no respon a l'estat dels models: no ho endevinem, ho diem.
    Unknown,
    /// El model encara no és a la memòria: es carrega des del disc.
    Loading,
    /// Ja és a la memòria: llegeix el context abans de la primera paraula.
    Prefill,
    /// Ja estan arribant fragments de text, però triga.
    Generating,
}

/// Bytes en «GB» amb una xifra decimal (els fabricants de models fan servir
/// unitats decimals, com «/api/tags»).
fn fmt_gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

/// Temps d'espera llegible: «45 s» o «2 min 15 s».
fn fmt_wait(ms: u64) -> String {
    let s = ms / 1000;
    if s < 60 {
        format!("{} s", s)
    } else {
        format!("{} min {} s", s / 60, s % 60)
    }
}

/// Context d'una esperada: què sap NoOrbit del model i de l'equip, i quina
/// nota va donar per última vegada. Serveix per dir VERITATS (mida del model,
/// RAM disponible, si ja és carregat) i per NO repetir la mateixa frase.
struct WaitCtx {
    model: String,
    /// Mida del model al disc (bytes), si Ollama la dona.
    size: Option<u64>,
    /// RAM total de l'equip (bytes).
    ram: Option<u64>,
    /// Clau de l'última nota emesa i instant en què es va emetre.
    last: Option<(String, u64)>,
}

impl WaitCtx {
    fn new(model: String, size: Option<u64>, ram: Option<u64>) -> Self {
        Self { model, size, ram, last: None }
    }

    /// El model no cap còmodament a la RAM (més del 60 %: el mateix criteri
    /// que usa `best_fit_model` per triar-lo). Aleshores carregar-lo vol dir
    /// anar lent i intercanviar dades amb el disc.
    fn tight(&self) -> bool {
        match (self.size, self.ram) {
            (Some(s), Some(r)) if r > 0 => s * 5 > r * 3,
            _ => false,
        }
    }

    /// Mida i RAM entre parèntesis, si les sabem.
    fn sizes(&self) -> String {
        match (self.size, self.ram) {
            (Some(s), Some(r)) => format!(" ({} al disc, {} de RAM)", fmt_gb(s), fmt_gb(r)),
            (Some(s), None) => format!(" ({} al disc)", fmt_gb(s)),
            (None, Some(r)) => format!(" (l'equip en té {} de RAM)", fmt_gb(r)),
            (None, None) => String::new(),
        }
    }

    /// Consell pràctic: QUÈ fer, no només esperar. L'avís de la RAM ja es va
    /// dir abans de començar; ací es RECORDA quan l'esperada es fa llarga.
    fn advice(&self, ms: u64) -> String {
        let mut s = String::new();
        if self.tight() && ms >= 60_000 {
            s.push_str(&format!(
                "\n⚠️ «{}» ocupa {} i l'equip en té {} de RAM: anirà molt a poc a poc i pot \
                 quedar-se sense memòria.",
                self.model,
                fmt_gb(self.size.unwrap_or_default()),
                fmt_gb(self.ram.unwrap_or_default())
            ));
        }
        if ms >= 300_000 {
            s.push_str(
                "\nPorta més de 5 minuts: sol ser perquè el model no hi cap. Prem «Atura» i, al \
                 «Gestor de models», tria'n un de més lleuger o connecta'n un de gratuït al \
                 núvol (aquest NO cal baixar-lo).",
            );
        }
        s
    }

    /// Redacta la nota per a una fase i un temps donats.
    fn note(&self, phase: WaitPhase, ms: u64) -> String {
        let head = match phase {
            WaitPhase::Generating => format!(
                "NoOrbit espera paraules del model ({}): «{}» ja és a la memòria i genera, poc a poc \
                 si el model és gran per a aquesta RAM.",
                fmt_wait(ms), self.model
            ),
            WaitPhase::Prefill => format!(
                "NoOrbit espera Ollama ({}): «{}» JA és a la memòria; ara llegeix tot el context \
                 abans d'escriure la primera paraula.",
                fmt_wait(ms), self.model
            ),
            WaitPhase::Loading => format!(
                "NoOrbit espera Ollama ({}): està CARREGANT «{}»{} des del disc.",
                fmt_wait(ms), self.model, self.sizes()
            ),
            WaitPhase::Unknown => format!(
                "NoOrbit espera Ollama ({}): Ollama no respon a l'estat dels models, així que NoOrbit \
                 no pot saber si està carregant «{}» o bloquejat.",
                fmt_wait(ms), self.model
            ),
        };
        format!("{}{}\n", head, self.advice(ms))
    }

    /// Diu quina nota cal emetre ara mateix, o `None` si NO cal dir res de nou:
    /// només es parla quan CANVIA l'estat real o cada 60 s. Així el visor ja no
    /// s'omple de nou vegades la mateixa línia.
    fn take(&mut self, phase: WaitPhase, ms: u64) -> Option<String> {
        let key = format!("{:?}|{}|{}", phase, self.tight(), ms >= 300_000);
        let repeat = self
            .last
            .as_ref()
            .map(|(_, t)| ms.saturating_sub(*t) >= 60_000)
            .unwrap_or(true);
        let changed = self.last.as_ref().map(|(k, _)| *k != key).unwrap_or(true);
        if !changed && !repeat {
            return None;
        }
        let note = self.note(phase, ms);
        self.last = Some((key, ms));
        Some(note)
    }
}

tokio::task_local! {
    /// Xat ACTUAL. Cada comanda de xat l'obre amb `in_session` abans de cridar
    /// la IA: així els events, la cancel·lació i el model triat pertanyen al
    /// xat que ha iniciat la tasca, encara que n'hi hagi diversos generant.
    static SESSION_SCOPE: String;
}

/// Identificador del xat en curs (`DEFAULT_SESSION` si no n'hi ha cap d'obert).
pub fn current_session() -> String {
    SESSION_SCOPE
        .try_with(|s| s.clone())
        .unwrap_or_else(|_| DEFAULT_SESSION.to_string())
}

/// Executa `fut` dins de l'àmbit del xat `session`.
pub async fn in_session<F>(session: &str, fut: F) -> F::Output
where
    F: std::future::Future,
{
    SESSION_SCOPE.scope(session.to_string(), fut).await
}

/// Un torn previ de la conversa: el missatge d'un usuari o la resposta de la IA.
/// S'usa per CONTINUAR un xat i pels «forks»: el model veu què s'ha dit abans.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryTurn {
    pub role: String,
    pub content: String,
}

/// Opcions d'una generació de xat: proveïdor i model propis d'aquell xat (per
/// treballar en paral·lel amb la IA local en un xat i una remota en un altre)
/// i la conversa precedent per no perdre el fil.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatOpts {
    /// `None` = el proveïdor global (el que estigui actiu a la configuració).
    #[serde(default)]
    pub provider: Option<String>,
    /// `None` = el model global triat per l'usuari.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub history: Vec<HistoryTurn>,
    /// Rutes d'imatges adjuntes al missatge (enganxades al xat). Només les
    /// reben els models d'Ollama amb visió; els proveïdors remots continuen
    /// veient el text (i la ruta, per si la IA vol descriure-la després).
    #[serde(default)]
    pub images: Vec<String>,
}

/// Estat propi de cada xat que pot treballar en paral·lel: bandera d'aturada,
/// despertador, raonament i proveïdor/model d'aquell xat.
pub struct SessionCtl {
    pub id: String,
    cancel: Arc<AtomicBool>,
    notify: Arc<Notify>,
    thinking: Arc<RwLock<Option<String>>>,
    provider: Arc<RwLock<Option<String>>>,
    model: Arc<RwLock<Option<String>>>,
    /// Certifica que AQUEST xat ja ha llançat un «error» explicat al visor. Així
    /// la comanda de xat no repeteix el mateix faliment com un segon bàner.
    err_shown: Arc<AtomicBool>,
    /// Última vegada que s'ha fet servir (per poder esborrar els xats vells).
    last_used: Arc<std::sync::Mutex<std::time::Instant>>,
}

/// Nombre màxim de xats que es conserven en memòria (els més recents).
const MAX_SESSIONS: usize = 64;

impl SessionCtl {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            cancel: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
            thinking: Arc::new(RwLock::new(None)),
            provider: Arc::new(RwLock::new(None)),
            model: Arc::new(RwLock::new(None)),
            err_shown: Arc::new(AtomicBool::new(false)),
            last_used: Arc::new(std::sync::Mutex::new(std::time::Instant::now())),
        }
    }

    /// Marca el xat com a acabat de fer-se servir i retorna l'instant.
    pub fn last_used(&self) -> std::time::Instant {
        if let Ok(mut g) = self.last_used.lock() {
            *g = std::time::Instant::now();
            return *g;
        }
        std::time::Instant::now()
    }

    /// Comença un torn nou: sense bandera d'aturada i sense raonament anterior.
    pub async fn begin(&self) {
        self.cancel.store(false, Ordering::Relaxed);
        self.err_shown.store(false, Ordering::Relaxed);
        *self.thinking.write().await = None;
    }

    /// Demana aturar aquest xat (no els altres, que poden estar generant).
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        // Desperta el bucle de streaming perquè isca de seguida, sense esperar
        // que arribe un fragment nou de dades.
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    pub fn notify(&self) -> Arc<Notify> {
        self.notify.clone()
    }

    pub async fn set_provider(&self, provider: Option<&str>) {
        *self.provider.write().await =
            provider.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
    }

    pub async fn provider(&self) -> Option<String> {
        self.provider.read().await.clone()
    }

    pub async fn set_model(&self, model: Option<&str>) {
        *self.model.write().await =
            model.map(|m| m.trim().to_string()).filter(|m| !m.is_empty());
    }

    pub async fn model(&self) -> Option<String> {
        self.model.read().await.clone()
    }

    pub async fn set_thinking(&self, thinking: Option<String>) {
        *self.thinking.write().await = thinking;
    }

    pub async fn thinking(&self) -> Option<String> {
        self.thinking.read().await.clone()
    }

    /// Fragment de text que arriba en directe a aquest xat.
    pub fn chunk(&self, app: &tauri::AppHandle, text: &str) {
        use tauri::Emitter;
        let _ = app.emit(
            "ai://chunk",
            serde_json::json!({ "text": text, "session": self.id }),
        );
    }

    /// Emet el procés d'aquest xat (fase + proveïdor + model + temps).
    pub fn process(&self, app: &tauri::AppHandle, provider: &str, model: &str, phase: &str, chunk: &str, elapsed_ms: u64) {
        emit_process(app, &self.id, provider, model, phase, chunk, elapsed_ms);
        // Un sol «error» per torn: el primer és el que conté l'explicació.
        if phase == "error" {
            self.err_shown.store(true, Ordering::Relaxed);
        }
    }

    /// Diu si aquest xat ja ha mostrat un error en el torn en curs.
    pub fn error_shown(&self) -> bool {
        self.err_shown.load(Ordering::Relaxed)
    }
}

/// Event unificat per mostrar el procés de QUALSEVOL IA en temps real. Tots
/// els backends (Ollama i els proveïdors remots) l'emeten amb el mateix
/// format, perquè la UI (`AIProcessViewer`) puga seguir token a token la
/// generació sense importar quin model hi haja darrere.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessEvent {
    /// "ollama" | nom del proveïdor remot (OpenAI, Claude, Venice…).
    pub provider: String,
    pub model: String,
    /// "thinking" | "streaming" | "done" | "error".
    pub phase: String,
    /// Text incremental (buit en les fases done/error).
    pub chunk: String,
    pub elapsed_ms: u64,
    /// Xat al qual pertany aquesta generació (per no mesclar dos xats).
    pub session: String,
}

/// Emet l'event «ai://process» (helper compartit pels backends d'IA i remot).
#[allow(clippy::too_many_arguments)]
pub fn emit_process(
    app: &tauri::AppHandle,
    session: &str,
    provider: &str,
    model: &str,
    phase: &str,
    chunk: &str,
    elapsed_ms: u64,
) {
    use tauri::Emitter;
    let _ = app.emit(
        "ai://process",
        ProcessEvent {
            provider: provider.to_string(),
            model: model.to_string(),
            phase: phase.to_string(),
            chunk: chunk.to_string(),
            elapsed_ms,
            session: session.to_string(),
        },
    );
}

/// Error compartit: l'ordinador no té prou memòria per carregar el model.
pub const OOM_MSG: &str = "NoOrbit: memòria insuficient — el model és massa gran per a aquest ordinador. Prova un model més lleuger (p. ex. :3b o quantitzat q4).";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub size: u64,
    #[serde(default)]
    pub modified_at: String,
}

/// Un model que Ollama té carregat ARA MATEIX a la memòria (RAM/VRAM).
/// Es llegeix amb «/api/ps» i es pot alliberar amb `unload_models`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedModel {
    pub name: String,
    /// Pes del model a la memòria (bytes).
    pub size: u64,
    /// Part que ocupa VRAM (0 en un Mac sense GPU dedicada).
    pub size_vram: u64,
}

/// Model disponible al núvol (ollama.com, Hugging Face, CivitAI…), per
/// navegar-lo i baixar-lo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudModel {
    pub name: String,
    pub description: String,
    /// Mida de la descàrrega en BYTES, perquè l'usuari vegi quant pesa el
    /// model ABANS de descarregar-lo (al Gestor de models i a Hugging Face).
    /// `None` quan la font no la diu: la UI no mostra res.
    #[serde(default)]
    pub size: Option<u64>,
    /// URL de descàrrega DIRECTA del fitxer (CivitAI i altres fonts no
    /// natives d'Ollama): la UI la passa a «image_cpu_download_model» per
    /// desar-la a la carpeta del rodet de CPU. `None` per a fonts natives.
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub kind: String, // "text" | "image" | "3d"
    pub active: bool,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
    /// Raonament que alguns models (deepseek-r1, qwen3…) retornen a part.
    /// Els servidors compatibles amb OpenAI fan servir altres noms per al
    /// mateix canal: es reconeixen tots perquè cap IA es quede sense que se
    /// n'mostre el pensament.
    #[serde(default, alias = "reasoning", alias = "reasoning_content")]
    thinking: Option<String>,
    /// Imatges en base64 (format Ollama) per a models amb visió.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
}

/// Ajusta les imatges adjuntades (rutes de fitxer) a l'últim missatge d'usuari,
/// codificades en base64 —el format que Ollama fa servir per als models amb
/// visió (llava, llama3.2-vision, gemma3, minicpm-v…). Si el model triat no
/// té visió, Ollama ho retorna com a error i la UI el mostra tal qual.
fn attach_images(messages: &mut Vec<ChatMessage>, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    let mut b64s: Vec<String> = Vec::new();
    for p in paths {
        if let Ok(bytes) = std::fs::read(p) {
            b64s.push(base64::engine::general_purpose::STANDARD.encode(&bytes));
        }
    }
    if b64s.is_empty() {
        return;
    }
    if let Some(msg) = messages.iter_mut().rev().find(|m| m.role == "user") {
        msg.images = Some(b64s);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    /// Demana el RAONAMENT SEPARAT al model (la bandera «think» d'Ollama).
    /// Només s'envia quan el model declara l'habilitat «thinking»: si no,
    /// Ollama retorna un error 400 i el torn moriria sense resposta.
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
    /// Quant de temps Ollama manté el model a la memòria després de respondre.
    /// «0» l'allibera de seguida (útil en equips amb poca RAM).
    #[serde(skip_serializing_if = "Option::is_none")]
    keep_alive: Option<String>,
    /// Opcions d'Ollama (p. ex. «num_ctx»). Absent = valors per defecte.
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatResponse {
    message: ChatMessage,
}

pub struct AiManager {
    http: reqwest::Client,
    /// Client per a streaming, SENSE temps màxim total: un model local en CPU
    /// pot trigar més de quatre minuts a generar, i tallar-ho era un error
    /// falsò. La seguretat la dona el «watchdog» d'inactivitat del bucle.
    http_stream: reqwest::Client,
    ollama_url: String,
    default_model: String,
    active_provider: Arc<RwLock<String>>,
    /// Model d'Ollama triat per l'usuari (si és None, s'usa default_model).
    selected_model: Arc<RwLock<Option<String>>>,
    /// Temps que Ollama manté el model carregat a la RAM («5m», «1m», «0»…).
    /// Buit = no s'envia i s'usa el valor per defecte d'Ollama. Un valor
    /// curt allibera RAM quan l'usuari no està treballant.
    keep_alive: Arc<RwLock<Option<String>>>,
    /// Proveïdors d'IA en línia alternatius (composables mitjançant active_provider).
    remote: Option<Arc<Mutex<ApiManager>>>,
    /// Un `SessionCtl` per xat obert: cadascú amb la seua bandera d'aturada,
    /// el seu raonament i el seu proveïdor/model. Així dos xats poden
    /// treballar alhora sense tallar-se ni mesclar respostes.
    sessions: Arc<Mutex<HashMap<String, Arc<SessionCtl>>>>,
    /// Mode eco (Objectiu 4): quan és true, les crides a Ollama usen menys
    /// fils i una finestra de context més xicoteta per no saturar la màquina
    /// mentre la tasca s'ha degradat a segon pla.
    background: Arc<AtomicBool>,
    /// Habilitats declarades per cada model («thinking», «vision», «tools»…),
    /// llegides una sola vegada amb «/api/show» i recordades.
    caps_cache: Arc<Mutex<HashMap<String, Vec<String>>>>,
    /// Orquestrador d'IAs externes: si la IA principal no resol la
    /// tasca, s'intenta amb una altra IA local i, si no n'hi ha cap de
    /// disponible, amb els proveïdors en línia oficials de l'usuari.
    /// L'origen real de cada resposta delegada va etiquetat.
    pub external: Arc<OrchestratorAgent>,
}

/// Enrotlla una futura de xat amb la cancel·lació del seu xat: si l'usuari
/// prem «Atura» en aquell xat, la petició s'avorta de seguida i el guard
/// es allibera (els altres xats continuen treballant).
async fn race_cancel<F, T>(ctl: &Arc<SessionCtl>, fut: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    let c = ctl.cancel_flag();
    let notify = ctl.notify();
    let watch = async move {
        loop {
            if c.load(Ordering::Relaxed) {
                return;
            }
            // `notified()` desperta quan l'usuari prem «Atura», sense haver
            // d'esperar el poll de 150 ms.
            tokio::select! {
                _ = notify.notified() => return,
                _ = tokio::time::sleep(std::time::Duration::from_millis(150)) => {}
            }
        }
    };
    tokio::pin!(fut);
    tokio::pin!(watch);
    // Sense `loop`: les dues branques del select fan `return`, així que un
    // bucle aquí és codi mort (clippy::never_loop) i no aporta res.
    tokio::select! {
        r = &mut fut => return r,
        _ = &mut watch => {
            // La petició pendent es descarta en deixar-la caure.
            drop(fut);
            return Err(anyhow!(CANCELLED_MSG));
        }
    }
}

/// Tradueix un codi d'error d'Ollama a una explicació accionable: un simple
/// «ERROR» no diu res, cal dir quin és el problema real i com eixir-ne.
fn ollama_error_note(model: &str, status: reqwest::StatusCode, body: &str) -> String {
    let low = body.to_lowercase();
    let q = format!("«{}»", model);
    let causa =
        if status.as_u16() == 404 || low.contains("not found") || low.contains("no such model") {
            format!("El model {} no està instal·lat a Ollama (404). Baixa'l o tries-ne un altre.", q)
        } else if low.contains("does not support thinking") {
            format!("{} no admet raonament separat: NoOrbit no el tornarà a demanar.", q)
        } else if low.contains("memory") || low.contains("oom") || low.contains("allocate") {
            format!(
                "La memòria d'aquest ordinador no abasta {}. Prova un model més lleuger (:3b, :1.5b o q4).",
                q
            )
        } else if status.is_server_error() {
            format!(
                "Ollama no ha sabut carregar {} (error {}). Pot ser el model malmès o la RAM insuficient.",
                q,
                status.as_u16()
            )
        } else {
            format!("Ollama ha rebutjat la petició amb l'error {}.", status.as_u16())
        };
    format!("{}\nDetall tècnic: {}", causa, body)
}

/// Finestra de context (en tokens) d'un model de NÚVOL: serveix per a saber
/// quin percentatge del context s'ha ocupat en cada resposta. Taula orienta-
/// tiva pels models més habituals; si no es coneix, 8k (valor conservador).
pub fn context_window_for(model: &str) -> u64 {
    let m = model.to_lowercase();
    if m.contains("claude") {
        200_000
    } else if m.contains("gpt-4o") || m.contains("gpt-4.1") || m.contains("gpt-5") {
        128_000
    } else if m.contains("gpt-4") || m.contains("gpt-3.5") {
        8_192
    } else if m.contains("deepseek") {
        65_536
    } else if m.contains("qwen3") {
        40_960
    } else if m.contains("qwen") {
        32_768
    } else if m.contains("llama3") || m.contains("llama-3") {
        128_000
    } else if m.contains("mistral") || m.contains("magistral") {
        32_768
    } else if m.contains("gemma") {
        8_192
    } else if m.contains("phi") {
        4_096
    } else if m.contains("glm") {
        128_000
    } else if m.contains("kimi") || m.contains("moonshot") {
        128_000
    } else {
        8_192
    }
}

/// Emet la COMPTABILITZACIÓ DE TOKENS d'un torn amb l'event «ai://usage»:
/// la interfície la mostra sota la resposta (quants tokens d'entrada i de
/// sortida, i quin percentatge del context s'ha ocupat). És el que permet
/// saber quant costa de veritat una resposta d'una API de núvol.
pub fn emit_usage(
    app: &tauri::AppHandle,
    session: &str,
    provider: &str,
    model: &str,
    input: u64,
    output: u64,
    context_window: Option<u64>,
) {
    use tauri::Emitter;
    let context_window = context_window
        .filter(|w| *w > 0)
        .unwrap_or_else(|| context_window_for(model));
    let total = input.saturating_add(output);
    let _ = app.emit(
        "ai://usage",
        serde_json::json!({
            "session": session,
            "provider": provider,
            "model": model,
            "input": input,
            "output": output,
            "total": total,
            "contextWindow": context_window,
            "contextUsedPct": if context_window > 0 {
                ((input as f64 / context_window as f64) * 100.0).min(100.0)
            } else {
                0.0
            },
        }),
    );
}

/// Tradueix errors opacs d'Ollama (memòria, connexió…) en missatges útils.
pub fn friendly_error(e: &str) -> String {
    let low = e.to_lowercase();
    if low.contains("out of memory")
        || low.contains("oom")
        || low.contains("could not allocate")
        || low.contains("failed to allocate")
        || low.contains("no buffer space")
    {
        OOM_MSG.into()
    } else if is_body_cut(e) {
        "NoOrbit: la memòria no ha abastat tot el context i la resposta s'ha tallat. \
         NoOrbit l'ha comprimida i ho ha tornat a provar; si es repeteix, desactiva \
         «Inclou el codi» o usa un model més lleuger.".into()
    } else {
        e.into()
    }
}

/// Detecta els talls de connexió a mitja resposta: reqwest los informa com a
/// «error decoding response body» (Ollama tanca el flux o expira el temps
/// màxim mentre es llegeix el cos) i els proveïdors remots com a «Stream
/// tallat». La causa habitual és la manca de memòria amb un context gran.
pub fn is_body_cut(e: &str) -> bool {
    let low = e.to_lowercase();
    low.contains("error decoding response body")
        || low.contains("stream tallat")
        || low.contains("payload was not fully received")
        || low.contains("connection reset")
        || low.contains("connection closed")
}

/// Extreu host i port d'una URL (p. ex. «http://localhost:11434/api/chat»)
/// per poder provar si Ollama està escoltant ARA mateix.
fn url_host_port(url: &str) -> (String, u16) {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let hp = rest.split('/').next().unwrap_or(rest);
    match hp.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(11434)),
        None => (hp.to_string(), 11434),
    }
}

/// Explica PER QUÈ Ollama no ha respost, en lloc del missatge opac de reqwest.
/// Distingeix tres casos reals i cadascun té una solució diferent:
/// 1. Temps màxim exhaurit (240 s): el model no cap a la RAM o el context és
///    enorme — la cua no s'arriba a processar mai.
/// 2. Res no escolta al port: Ollama no està obert (o ha caigut).
/// 3. Connexió refusada/tallada amb el port viu: Ollama saturat.
/// S'emprà des de `send_chat`; el text tècnic original s'adjunta al final per
/// no perdre informació (i perquè `is_body_cut` segueixis reconeixent-lo).
async fn explain_send_failure(e: &reqwest::Error, url: &str, auto_install: bool) -> String {
    use std::error::Error as _;
    use std::net::ToSocketAddrs;
    let (host, port) = url_host_port(url);
    // Prova ràpid: escolta algú al port ARA mateix?
    let listening = (host.as_str(), port)
        .to_socket_addrs()
        .map(|addrs| {
            addrs.into_iter().any(|a| {
                std::net::TcpStream::connect_timeout(&a, std::time::Duration::from_millis(600))
                    .is_ok()
            })
        })
        .unwrap_or(false);
    let detail = e
        .source()
        .map(|s| s.to_string())
        .unwrap_or_else(|| e.to_string());
    let msg = if e.is_timeout() {
        "HE EXHAURIT EL TEMPS MÀXIM (4 min) esperant Ollama. Això NO és un error \
         d'Ollama: el model triga més del que pot perquè (a) és massa gran per a la \
         RAM del teu equip i l'està carregant intercanviant amb el disc, o (b) el \
         missatge porta massa context. Què fer: tries un model més lleuger (p. ex. \
         qwen2.5-coder:1.5b), desactiva «Inclou el codi del projecte» o torna-ho a \
         intentar — si Ollama ja té el model carregat a la segona vegada, anirà molt \
         més ràpid."
            .to_string()
    } else if !listening {
        // El servei no hi és: intentem arrencar-lo nosaltres (si està instal·lat);
        // si NO ho està, l'hem de BAIXAR i INSTAL·LAR: ho llenç en un fil de fons
        // Ollama NO instal·lat: NoOrbit ha de poder funcionar igual. Ara:
        //  - Si l'usuari ja té un proveïdor EN LÍNIA (amb token), el xat ja ha
        //   uria d'haver caigut a ell; aquí l'error és del remot.
        //  - Si no en té cap, l'instal·lació d'Ollama en segon pla és la via
        //    per tenir IA local (gratis, sense comptes).
        // Arrencar Ollama és LENT (fins a ~40 s: espera del port + espera
        // del model). Ho fem en un fil a part perquè el runtime async
        // (streaming, altres peticions) no quedi congelat mentre esperem.
        let tried = if ollama_installed() {
            tokio::task::spawn_blocking(|| start_ollama_server().is_ok())
                .await
                .unwrap_or(false)
        } else if auto_install {
            std::thread::spawn(|| {
                let _ = ensure_ollama_installed();
                let _ = start_ollama_server();
            });
            false
        } else {
            // L'usuari ja té una IA EN LÍNIA: no li baixem Ollama a la força.
            false
        };
        format!(
            "OLLAMA NO ESTÀ EN MARXA: res no escolta a {}:{}. {}",
            host,
            port,
            if tried {
                "Ho he intentat arrencar jo: espera uns segons i repeteix el missatge."
            } else if !auto_install {
                "El motor local (Ollama) no està instal·lat, però tu ja tens una IA \
                 en línia configurada: el problema és el proveïdor remot (token, \
                 quota o connexió). Revisa'l a «Proveïdors». NoOrbit funciona igual \
                 sense Ollama: editor, terminal, fitxers, Blender, Unreal i \
                 imatges són iguals."
            } else {
                "No el tens instal·lat: estic BAIXANT i INSTAL·LANT Ollama \
                 automàticament en segon pla (triga uns minuts la descàrrega). \
                 Quan acabi torna a enviar el missatge — o prem «Inicia Ollama» \
                 al panell IA. \
                 \
                 MENTRE TANT: NoOrbit funciona igual (editor, terminal, fitxers, \
                 Blender, Unreal, imatges, extensions i skills). Per tenir IA al \
                 xat ara mateix, connecta un proveïdor amb token a «Proveïdors» \
                 (p. ex. Venice AI o OpenAI) i el xat hi caurà automàticament."
            }
        )
    } else if e.is_connect() {
        format!(
            "NO ME HE POGUT CONECTAR amb Ollama tot i que ara mateix el port {} SÍ \
             que respon: estava saturat en aquell instant (normalment, carregant un \
             model gran sense RAM suficient). Torna-ho a intentar una vegada.",
            port
        )
    } else {
        format!(
            "LA CONNEXIÓ S'HA TALLAT enviant la petició a Ollama (port {} viu). \
             Sol passar quan la RAM no arriba i Ollama tanca la connexió. Redueix \
             el context (desactiva «Inclou el codi») o usa un model més petit.",
            port
        )
    };
    format!("{} (detall tècnic: {})", msg, detail)
}

/// Comprimeix un text massa gran perquè la IA el puga processar quan la
/// memòria no hi dóna: conserva INTACTA la tasca (el tram final, darrere de
/// «PREGUNTA / TASCA:») i retalla el context del projecte, marcant el buit.
/// L'objectiu no és tornar una resposta perfecta sinó DEIXAR TREBALLAR
/// l'agent: amb ~7.000 caràcters (≈2.300 tokens) qualsevol num_ctx cab.
pub fn compact_prompt(text: &str) -> String {
    const KEEP: usize = 7_000;
    const TASK_KEEP: usize = 5_500;
    let total = text.chars().count();
    if total <= KEEP {
        return text.to_string();
    }
    match text.find("PREGUNTA / TASCA:") {
        // Format estàndard del xat: context del projecte + tasca al final.
        Some(pos) => {
            let ctx = &text[..pos];
            let task: String = text[pos..].chars().take(TASK_KEEP).collect();
            let head_budget = KEEP.saturating_sub(task.chars().count()).max(300);
            let ctx_head: String = ctx.chars().take(head_budget).collect();
            format!(
                "{}\n… [context comprimit per NoOrbit: la memòria no abastava \
                 tot el projecte; la tasca es conserva sencera] …\n\n{}",
                ctx_head, task
            )
        }
        // Text lliure: conserva el cap (encapçalaments/rutes) i la cua.
        None => {
            let head: String = text.chars().take(1_200).collect();
            let tail: String = text.chars().skip(total - (KEEP - 1_200)).collect();
            format!(
                "{}\n\n… [context comprimit per NoOrbit] …\n\n{}",
                head, tail
            )
        }
    }
}

/// Retalla la CONVERSA precedent (historial d'un xat o d'un «fork») quan la
/// memòria no abasta tot: conserva només els últims torns, cada un curt. Millor
/// poc context que cap context: el model no perd del tot el fil del treball.
pub fn compact_history(history: &[HistoryTurn]) -> Vec<HistoryTurn> {
    const KEEP_TURNS: usize = 4;
    const KEEP_CHARS: usize = 1_200;
    history
        .iter()
        .rev()
        .take(KEEP_TURNS)
        .rev()
        .map(|t| HistoryTurn {
            role: t.role.clone(),
            content: t.content.chars().take(KEEP_CHARS).collect(),
        })
        .collect()
}

/// Construeix la conversa completa que veu el model: els torns ja digitats del
/// xat (historial d'una conversa llarga o d'un «fork») seguits del missatge
/// actual. Fusiona els torns consecutius del mateix rol i garanteix que la
/// conversa comence i acabe en «user», com exigeix l'API d'Anthropic.
pub fn conversation(history: &[HistoryTurn], prompt: &str) -> Vec<(String, String)> {
    let mut turns: Vec<(String, String)> = Vec::new();
    for t in history {
        let role = if t.role.eq_ignore_ascii_case("assistant") {
            "assistant"
        } else {
            "user"
        };
        let content = t.content.trim();
        if content.is_empty() {
            continue;
        }
        // Abans del primer «user» no acceptem torns d'assistent: sense el
        // pregunta previa la resposta perd el sentit (i Anthropic la rebutja).
        if role == "assistant" && turns.is_empty() {
            continue;
        }
        match turns.last_mut() {
            Some(prev) if prev.0 == role => {
                prev.1.push_str("\n\n");
                prev.1.push_str(content);
            }
            _ => turns.push((role.to_string(), content.to_string())),
        }
    }
    // El missatge actual tanca sempre la conversa en «user».
    match turns.last_mut() {
        Some(prev) if prev.0 == "user" => {
            prev.1.push_str("\n\n");
            prev.1.push_str(prompt);
        }
        _ => turns.push(("user".to_string(), prompt.to_string())),
    }
    turns
}

/// Ruta REAL del binari d'Ollama al sistema. Crítica: quan NoOrbit corre
/// com a .app el seu PATH és mínim (sovint sense /usr/local/bin), i si
/// Ollama és l'App d'escriptori el CLI viu DINS del bundle
/// (Ollama.app/Contents/Resources/ollama) — per això un «ollama serve»
/// des de NoOrbit fallava amb ENOENT i el port 11434 quedava mort.
pub fn ollama_binary() -> Option<std::path::PathBuf> {
    if let Ok(p) = which::which("ollama") {
        return Some(p);
    }
    let mut cands: Vec<std::path::PathBuf> = Vec::new();
    if cfg!(target_os = "macos") {
        if let Some(home) = dirs::home_dir() {
            cands.push(home.join("Applications/Ollama.app/Contents/Resources/ollama"));
            cands.push(home.join(".noorbit/bin/ollama"));
        }
        cands.push(std::path::PathBuf::from(
            "/Applications/Ollama.app/Contents/Resources/ollama",
        ));
    }
    cands.push(std::path::PathBuf::from("/opt/homebrew/bin/ollama"));
    cands.push(std::path::PathBuf::from("/usr/local/bin/ollama"));
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        // Windows: l'instal·lador oficial deixa el CLI aquí (l'usuari normal).
        cands.push(std::path::PathBuf::from(local)
            .join("Programs")
            .join("Ollama")
            .join("ollama.exe"));
    }
    cands.into_iter().find(|p| p.is_file())
}

/// Detecta si el binari/servei d'Ollama és al sistema (CLI a PATH o l'app).
pub fn ollama_installed() -> bool {
    if ollama_binary().is_some() {
        return true;
    }
    // macOS: l'aplicació d'escriptori arrenca el servei en segon pla.
    if cfg!(target_os = "macos") {
        if std::path::Path::new("/Applications/Ollama.app").exists() {
            return true;
        }
        if let Some(home) = dirs::home_dir() {
            if home.join("Applications/Ollama.app").exists() {
                return true;
            }
        }
    }
    false
}

/// Compràpida: algun procés escolta al port 11434 (el servidor Ollama)?
fn ollama_port_open() -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], 11434)),
        std::time::Duration::from_millis(400),
    )
    .is_ok()
}

/// Diu si el servidor Ollama està escoltant ara mateix (versió pública).
pub fn ollama_running() -> bool {
    ollama_port_open()
}

/// Còpia recursiva d'una carpeta (per instal·lar la App d'Ollama a /Applications
/// quan el zip i el destí poden estar en volums diferents i `rename` falla).
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// BAIXA I INSTAL·LA Ollama automàticament quan no hi és al sistema (macOS):
/// 1) l'app oficial (zip universal d'ollama.com) a /Applications (o a
/// ~/Applications si /Applications no és escrivible), 2) si la descàrrega
/// falla, el binari CLI de GitHub Releases dins de ~/.noorbit/bin.
/// No demana res a l'usuari: el missatge resultant explica on ha quedat.
#[cfg(target_os = "macos")]
pub fn ensure_ollama_installed() -> Result<String> {
    if ollama_installed() {
        return Ok("Ollama ja està instal·lat".into());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .user_agent("NoOrbit")
        .build()
        .map_err(|e| anyhow!("client HTTP: {}", e))?;

    // 1) App oficial (universal): conté el CLI dins Resources/ollama.
    if let Ok(resp) = client.get("https://ollama.com/download/Ollama-darwin.zip").send() {
        if resp.status().is_success() {
            if let Ok(bytes) = resp.bytes() {
                let zip = std::env::temp_dir().join("noorbit_ollama.zip");
                let dir = std::env::temp_dir().join("noorbit_ollama_unzip");
                let _ = std::fs::remove_dir_all(&dir);
                if std::fs::write(&zip, &bytes).is_ok()
                    && std::fs::create_dir_all(&dir).is_ok()
                    && std::process::Command::new("unzip")
                        .args(["-q", "-o"])
                        .arg(&zip)
                        .arg("-d")
                        .arg(&dir)
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false)
                {
                    let app = dir.join("Ollama.app");
                    if app.exists() {
                        let writable = std::fs::metadata("/Applications")
                            .map(|m| {
                                use std::os::unix::fs::PermissionsExt as _;
                                m.permissions().mode() & 0o200 != 0
                            })
                            .unwrap_or(false);
                        let dest = if writable {
                            std::path::PathBuf::from("/Applications/Ollama.app")
                        } else if let Some(home) = dirs::home_dir() {
                            home.join("Applications/Ollama.app")
                        } else {
                            std::path::PathBuf::from("/Applications/Ollama.app")
                        };
                        if copy_dir_recursive(&app, &dest).is_ok() {
                            let _ = std::fs::remove_dir_all(&dir);
                            let _ = std::fs::remove_file(&zip);
                            return Ok(format!(
                                "Ollama instal·lat automàticament a {}",
                                dest.display()
                            ));
                        }
                    }
                }
            }
        }
    }

    // 2) Fallback: binari CLI de GitHub Releases a la carpeta local de NoOrbit.
    let home = dirs::home_dir().ok_or_else(|| anyhow!("no hi ha carpeta d'usuari"))?;
    let bindir = home.join(".noorbit/bin");
    std::fs::create_dir_all(&bindir)?;
    let dest = bindir.join("ollama");
    let resp = client
        .get("https://github.com/ollama/ollama/releases/latest/download/ollama-darwin")
        .send()
        .map_err(|e| anyhow!("descàrrega del CLI d'Ollama: {}", e))?;
    if !resp.status().is_success() {
        return Err(anyhow!(
            "No he pogut descarregar Ollama (HTTP {})",
            resp.status()
        ));
    }
    let bytes = resp.bytes().map_err(|e| anyhow!("cos: {}", e))?;
    std::fs::write(&dest, &bytes)?;
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
    }
    Ok(format!("Ollama (CLI) instal·lat a {}", dest.display()))
}

/// Sistemes no-macOS: mantenim el flux manual (Terminal / pàgina oficial).
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn ensure_ollama_installed() -> Result<String> {
    anyhow::bail!("La instal·lació automàtica d'Ollama no està disponible en aquest sistema")
}

/// Linux: executa l'INSTAL·LADOR OFICIAL (https://ollama.com/install.sh),
/// que resol el binari, el servei systemd i el grup segons la distro.
/// Si l'usuari no pot escriure a /usr/local (cal sudo i no hi ha terminal
/// darrere d'una app gràfica), ho diem amb el comandament manual exacte.
#[cfg(target_os = "linux")]
pub fn ensure_ollama_installed() -> Result<String> {
    if ollama_installed() {
        return Ok("Ollama ja està instal·lat".into());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent("NoOrbit")
        .build()
        .map_err(|e| anyhow!("client HTTP: {}", e))?;
    let resp = client
        .get("https://ollama.com/install.sh")
        .send()
        .map_err(|e| anyhow!("descàrrega de l'instal·lador oficial: {}", e))?;
    if !resp.status().is_success() {
        return Err(anyhow!(
            "No he pogut descarregar l'instal·lador oficial (HTTP {})",
            resp.status()
        ));
    }
    let body = resp.bytes().map_err(|e| anyhow!("cos: {}", e))?;
    let script = std::env::temp_dir().join("noorbit_ollama_install.sh");
    std::fs::write(&script, &body)?;
    let out = std::process::Command::new("sh")
        .arg(&script)
        .output()
        .map_err(|e| anyhow!("execució de l'instal·lador: {}", e))?;
    let _ = std::fs::remove_file(&script);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() && ollama_installed() {
        return Ok("Ollama instal·lat amb l'instal·lador oficial de ollama.com".into());
    }
    // Sense permisos d'administrador (sudo sense terminal no funciona):
    // donem l'ordre manual exacta en lloc d'un fracàs opac.
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    Err(anyhow!(
        "L'instal·lador oficial ha demanat permisos d'administrador i des \
d'una app gràfica no n'hi ha. Executa manualment en un terminal:\n\
  curl -fsSL https://ollama.com/install.sh | sh\n(detall: {})",
        stderr.lines().last().unwrap_or("sense sortida d'error")
    ))
}

/// Windows: descarrega l'INSTAL·LADOR OFICIAL (OllamaSetup.exe) i l'executa
/// en mode SILENCIÓS (Inno Setup: /VERYSILENT), sense obrir finestres.
#[cfg(target_os = "windows")]
pub fn ensure_ollama_installed() -> Result<String> {
    use std::os::windows::process::CommandExt as _;
    if ollama_installed() {
        return Ok("Ollama ja està instal·lat".into());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .user_agent("NoOrbit")
        .build()
        .map_err(|e| anyhow!("client HTTP: {}", e))?;
    let resp = client
        .get("https://ollama.com/download/OllamaSetup.exe")
        .send()
        .map_err(|e| anyhow!("descàrrega de l'instal·lador oficial: {}", e))?;
    if !resp.status().is_success() {
        return Err(anyhow!(
            "No he pogut descarregar OllamaSetup.exe (HTTP {})",
            resp.status()
        ));
    }
    let bytes = resp.bytes().map_err(|e| anyhow!("cos: {}", e))?;
    let exe = std::env::temp_dir().join("noorbit_ollama_setup.exe");
    std::fs::write(&exe, &bytes)?;
    let out = std::process::Command::new(&exe)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .status()
        .map_err(|e| anyhow!("execució de l'instal·lador: {}", e))?;
    let _ = std::fs::remove_file(&exe);
    if out.success() && ollama_installed() {
        return Ok("Ollama instal·lat amb l'instal·lador oficial per a Windows".into());
    }
    Err(anyhow!(
        "L'instal·lador oficial de Windows no ha acabat bé (exit {}). \
Prova d'executar-lo manualment des de https://ollama.com/download",
        out.code().unwrap_or(-1)
    ))
}

/// Espera que el port d'Ollama quede (obert | tancat); cert si ho aconsegueix.
fn wait_ollama_port(want_open: bool, timeout_ms: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if ollama_port_open() == want_open {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Atura el servidor Ollama en marxa. A macOS tanca també l'app d'escriptori
/// (que reengegar `serve` sola sense la nostra variable d'entorn).
fn stop_ollama_server() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("osascript")
            .args(["-e", "quit app \"Ollama\""])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("pkill")
            .args(["-f", "ollama serve"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(windows)]
    {
        for img in ["ollama.exe", "ollama app.exe"] {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/IM", img])
                .creation_flags(0x08000000)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
    // Dona temps perquè allibere el port abans de reengegar-lo.
    wait_ollama_port(false, 8_000);
}

/// Arrenca `ollama serve` en segon pla. Si `models_dir` no és buit, li passa
/// `OLLAMA_MODELS` perquè els models es baixin i s'executin des d'aquesta
/// carpeta (p. ex. un disc extern); Ollama en si pot seguir al disc intern.
/// A macOS, `launchctl setenv` fa que l'app d'Ollama també la faci servir
/// quan es torne a obrir des del Finder.
fn spawn_ollama_serve(models_dir: &str) -> Result<()> {
    // Binari resolt (mai el nom buit «ollama»: des d'una .app el PATH no
    // el conté i fallava amb «client error (Connect)» per sempre).
    let bin = ollama_binary().ok_or_else(|| anyhow!("No trobo el binari d'Ollama al sistema"))?;
    #[cfg(target_os = "macos")]
    {
        let mut cmd = std::process::Command::new("launchctl");
        if models_dir.is_empty() {
            cmd.args(["unsetenv", "OLLAMA_MODELS"]);
        } else {
            cmd.args(["setenv", "OLLAMA_MODELS", models_dir]);
        }
        let _ = cmd
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        let mut cmd = std::process::Command::new("nohup");
        cmd.arg(&bin)
            .arg("serve")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if !models_dir.is_empty() {
            cmd.env("OLLAMA_MODELS", models_dir);
        }
        // PATH amplat perquè qualsevol subprocés d'Ollama trobi eines del sistema.
        let path_env = std::env::var("PATH").unwrap_or_default();
        cmd.env(
            "PATH",
            format!("/usr/local/bin:/opt/homebrew/bin:{}", path_env),
        );
        cmd.spawn()
            .map(|_| ())
            .map_err(|e| anyhow!("No s'ha pogut arrencar Ollama ({}): {}", bin.display(), e))?;
    }
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new(&bin);
        cmd.arg("serve").creation_flags(0x00000008);
        if !models_dir.is_empty() {
            cmd.env("OLLAMA_MODELS", models_dir);
        }
        cmd.spawn()
            .map(|_| ())
            .map_err(|e| anyhow!("No s'ha pogut arrencar Ollama: {}", e))?;
    }
    Ok(())
}

/// Recorda quina carpeta de models fa servir REALMENT el servei en marxa,
/// perquè NoOrbit sàpiga si cal reiniciar-lo després d'un canvi.
fn set_applied_models_dir(dir: &str) {
    if let Ok(mut cfg) = crate::config::AppConfig::load() {
        cfg.ai.models_dir_applied = dir.trim().to_string();
        let _ = cfg.save();
    }
}

/// Assegura que el servidor Ollama en marxa utilitzi la carpeta de models
/// configurada (`ai.models_dir`). És el cas típic de l'error: l'app d'Ollama
/// arrencada des del Finder no la coneix, i per això els models es baixaven
/// al disc intern encara que hi hagués un USB triat. Si la carpeta no és la
/// aplicada, reinicia el servei amb `OLLAMA_MODELS`. Retorna un missatge per
/// a la UI quan ha calgut fer alguna cosa. `force = true` el reinicia sempre
/// (per exemple, just després que l'usuari triï una carpeta nova).
pub fn apply_models_dir(force: bool) -> Result<Option<String>> {
    let cfg = crate::config::AppConfig::load()?;
    let dir = cfg.ai.models_dir.trim().to_string();
    let applied = cfg.ai.models_dir_applied.trim().to_string();

    if dir.is_empty() {
        // Sense carpeta externa: si el servei ja fa servir la per defecte, res.
        if applied.is_empty() {
            return Ok(None);
        }
        if !ollama_port_open() {
            set_applied_models_dir("");
            return Ok(None);
        }
        // L'usuari ha tornat a la carpeta per defecte: reinicia sense OLLAMA_MODELS.
        stop_ollama_server();
        spawn_ollama_serve("")?;
        wait_ollama_port(true, 25_000);
        set_applied_models_dir("");
        return Ok(Some(
            "Ollama torna a fer servir la carpeta de models per defecte del sistema.".to_string(),
        ));
    }

    // Hi ha una carpeta externa configurada. Pot ser una subcarpeta nova del
    // disc extern (p. ex. «/Volumes/USB/ollama-models»): intentem crear-la;
    // si no podem, és que el volum no està muntat.
    if !std::path::Path::new(&dir).exists() && std::fs::create_dir_all(&dir).is_err() {
        // El disc extern no és disponible: no es pot aplicar; arrenca Ollama al disc intern.
        if !ollama_port_open() {
            spawn_ollama_serve("")?;
            wait_ollama_port(true, 20_000);
            set_applied_models_dir("");
        }
        return Ok(Some(format!(
            "El disc extern encara no està disponible ({}). Connecta'l i prem \
             «Arrenca Ollama»: aleshores els models es baixaran des d'allà.",
            dir
        )));
    }
    if ollama_port_open() && applied == dir && !force {
        return Ok(None); // el servei ja mira cap a aquesta carpeta
    }
    std::fs::create_dir_all(&dir)?;
    if ollama_port_open() {
        stop_ollama_server();
    }
    spawn_ollama_serve(&dir)?;
    let ok = wait_ollama_port(true, 25_000);
    set_applied_models_dir(&dir);
    Ok(Some(if ok {
        format!("Ollama reiniciat: els models es baixen i s'executen des de {}.", dir)
    } else {
        format!(
            "He arrencat Ollama amb la carpeta {}; pot trigar uns segons a estar llest.",
            dir
        )
    }))
}

/// Versió per cridar abans de cada descàrrega: si Ollama no roda, l'arrenca
/// amb la carpeta configurada; si roda sense aplicar-la, el reinicia.
pub fn ensure_models_dir_applied() -> Result<Option<String>> {
    if !ollama_port_open() {
        if ollama_installed() {
            start_ollama_server()?;
            wait_ollama_port(true, 20_000);
        }
        return Ok(None); // start_ollama_server ja usa la carpeta configurada
    }
    apply_models_dir(false)
}

/// Mida total (bytes) d'un arbre de directoris; 0 si no existeix.
fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for e in entries.flatten() {
            let p = e.path();
            if let Ok(md) = e.metadata() {
                if md.is_dir() {
                    total += dir_size(&p);
                } else {
                    total += md.len();
                }
            }
        }
    }
    total
}

/// Copia recursivament `from` dins `to` anant emetent progrés a «ai://migrate».
fn copy_tree_progress(
    app: &tauri::AppHandle,
    from: &std::path::Path,
    to: &std::path::Path,
    total: u64,
    done: &mut u64,
) -> Result<()> {
    use tauri::Emitter;
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let src = e.path();
        let dst = to.join(e.file_name());
        let ft = e.file_type()?;
        if ft.is_dir() {
            copy_tree_progress(app, &src, &dst, total, done)?;
        } else if ft.is_file() {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            // Els blobs són únics (adreça de contingut): si ja hi és amb la
            // mateixa mida, ja el tenim i estalviem la còpia.
            let ja_copiat = std::fs::metadata(&dst).map(|m| m.len() == size).unwrap_or(false);
            if !ja_copiat {
                std::fs::copy(&src, &dst)?;
            }
            *done += size;
            let pct = if total > 0 { (100.0 * *done as f64 / total as f64) as u32 } else { 100 };
            let _ = app.emit(
                "ai://migrate",
                serde_json::json!({
                    "message": format!("Mouent els models al disc extern… {}%", pct)
                }),
            );
        }
    }
    Ok(())
}

/// Mou els models ja baixats al disc intern (per defecte, `~/.ollama/models`)
/// cap a la carpeta externa configurada, i reinicia Ollama apuntant-hi. Els
/// blobs es copien un per un (amb progrés) i, només quan tot és copiat, 
/// s'esborra la còpia del disc intern per alliberar espai.
pub fn migrate_models(app: &tauri::AppHandle) -> Result<String> {
    let cfg = crate::config::AppConfig::load()?;
    let target = cfg.ai.models_dir.trim().to_string();
    if target.is_empty() {
        return Err(anyhow!("Primer tria una carpeta en un disc extern"));
    }
    let target_path = std::path::Path::new(&target);
    if !target_path.exists() {
        return Err(anyhow!(
            "El disc extern no està connectat ({})", 
            target
        ));
    }
    let source = dirs::home_dir()
        .map(|h| h.join(".ollama").join("models"))
        .filter(|p| p.is_dir())
        .ok_or_else(|| anyhow!("No hi ha models al disc intern (~/.ollama/models)"))?;
    if source == target_path {
        return Ok("La carpeta triada ja és la per defecte: no cal moure res.".to_string());
    }
    let total = dir_size(&source);
    if total == 0 {
        return Ok("No hi havia models a moure.".to_string());
    }
    let mut done = 0u64;
    for sub in ["blobs", "manifests"] {
        let from = source.join(sub);
        if from.is_dir() {
            copy_tree_progress(app, &from, &target_path.join(sub), total, &mut done)?;
        }
    }
    // Tot copiat: esborra només les dades de models del disc intern (mai les
    // claus d'usuari del veí ~/.ollama).
    for sub in ["blobs", "manifests"] {
        let _ = std::fs::remove_dir_all(source.join(sub));
    }
    // El servidor ha de mirar ara cap al disc extern.
    let _ = apply_models_dir(true);
    let gb = total as f64 / 1e9;
    Ok(format!(
        "He mogut {:.1} GB de models a {} i he alliberat el disc intern.", 
        gb, target
    ))
}

/// Arrenca `ollama serve` en segon pla si el binari existeix (perquè el
/// port 11434 responda quan l'usuari no té l'app oberta). Si hi ha una
/// carpeta de models configurada (p. ex. un USB extern), l'arrenca-hi o,
/// si ja roda sense aplicar-la, el reinicia perquè les descàrregues hi vagin.
pub fn start_ollama_server() -> Result<()> {
    if !ollama_installed() {
        // El programa s'ho fa sol: baixa i instal·la Ollama abans d'engegar-lo.
        ensure_ollama_installed()?;
    }
    // Si ja escolta, assegurem que utilitzi la carpeta configurada (si cal,
    // el reinicia amb OLLAMA_MODELS).
    if ollama_port_open() {
        let _ = apply_models_dir(false);
        return Ok(());
    }
    // Carpeta de models configurada (p. ex. un USB extern); buida = per defecte.
    let models_dir = crate::config::AppConfig::load()
        .map(|c| c.ai.models_dir.trim().to_string())
        .unwrap_or_default();
    // Si el disc extern no és muntat (o la carpeta no es pot crear), arrenca
    // amb la carpeta del sistema.
    let dir = if !models_dir.is_empty()
        && (std::path::Path::new(&models_dir).exists()
            || std::fs::create_dir_all(&models_dir).is_ok())
    {
        models_dir
    } else {
        String::new()
    };
    spawn_ollama_serve(&dir)?;
    set_applied_models_dir(&dir);
    Ok(())
}

/// Detecta si ComfyUI és al sistema (CLI `comfy`, l'app d'escriptori o la
/// carpeta d'instal·lació habitual), encara que no estiga en marxa.
pub fn comfyui_installed() -> bool {
    if which::which("comfy").is_ok() || which::which("comfyui").is_ok() {
        return true;
    }
    if cfg!(target_os = "macos") {
        if std::path::Path::new("/Applications/ComfyUI.app").exists() {
            return true;
        }
        if let Some(home) = dirs::home_dir() {
            if home.join("Applications/ComfyUI.app").exists() {
                return true;
            }
        }
    }
    if let Some(home) = dirs::home_dir() {
        if home.join("ComfyUI").is_dir() {
            return true;
        }
    }
    false
}

/// Compràpida: algun procés escolta al port 8188 (el servidor ComfyUI)?
pub fn comfyui_running() -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], 8188)),
        std::time::Duration::from_millis(400),
    )
    .is_ok()
}

/// Arrenca ComfyUI en segon pla: primer l'app d'escriptori (macOS), si no,
/// la CLI oficial `comfy launch` (comfy-cli de Comfy Org).
pub fn start_comfyui() -> Result<()> {
    if comfyui_running() {
        return Ok(());
    }
    if !comfyui_installed() {
        return Err(anyhow!("ComfyUI no està instal·lat"));
    }
    #[cfg(target_os = "macos")]
    {
        let mut apps = vec![std::path::PathBuf::from("/Applications/ComfyUI.app")];
        if let Some(home) = dirs::home_dir() {
            apps.push(home.join("Applications/ComfyUI.app"));
        }
        for app in &apps {
            if app.is_dir() {
                std::process::Command::new("open")
                    .arg(app)
                    .spawn()
                    .map_err(|e| anyhow!("No s'ha pogut arrencar ComfyUI: {}", e))?;
                return Ok(());
            }
        }
    }
    if which::which("comfy").is_ok() {
        #[cfg(unix)]
        std::process::Command::new("nohup")
            .args(["comfy", "launch", "--background"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| anyhow!("No s'ha pogut arrencar ComfyUI: {}", e))?;
        #[cfg(windows)]
        std::process::Command::new("comfy")
            .args(["launch", "--background"])
            .spawn()
            .map(|_| ())
            .map_err(|e| anyhow!("No s'ha pogut arrencar ComfyUI: {}", e))?;
        return Ok(());
    }
    Err(anyhow!(
        "S'ha trobat ComfyUI, però no l'app d'escriptori ni la CLI 'comfy' per arrencar-lo"
    ))
}

impl AiManager {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            http: reqwest::Client::builder()
                // Si Ollama no accepta la connexió (no està en marxa), falla
                // de seguida en lloc de penjar-se.
                .connect_timeout(std::time::Duration::from_secs(10))
                // Temps màxim d'una generació: abans eren 600 s i, si el
                // servidor es bloquejava (p. ex. carregant un model enorme),
                // la interfície es quedava «Treballant…» 10 minuts. El limitem
                // a 240 s, suficient per a models grans sense congelar l'app.
                .timeout(std::time::Duration::from_secs(240))
                .build()
                .unwrap_or_default(),
            // El mateix, però sense `timeout`: per al streaming, on el temps
            // el controla l'usuari (botó «Atura») i el watchdog d'inactivitat.
            http_stream: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            ollama_url: config.ai.ollama_url.clone(),
            default_model: config.ai.default_model.clone(),
            active_provider: Arc::new(RwLock::new("ollama".into())),
            selected_model: Arc::new(RwLock::new(None)),
            keep_alive: Arc::new(RwLock::new(
                Some(config.ai.keep_alive.trim().to_string()).filter(|v| !v.is_empty()),
            )),
            remote: None,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            background: Arc::new(AtomicBool::new(false)),
            caps_cache: Arc::new(Mutex::new(HashMap::new())),
            external: Arc::new(OrchestratorAgent::new()),
        }
    }

    /// Crea un gestor amb accés als proveïdors remots registrats.
    pub fn with_remote(config: &AppConfig, remote: Arc<Mutex<ApiManager>>) -> Self {
        let mut mgr = Self::new(config);
        mgr.remote = Some(remote.clone());
        // L'orquestrador extern també els rep: si cap IA local resol, el bot
        // de rescat en línia consulta els proveïdors oficials amb token.
        mgr.external = Arc::new(OrchestratorAgent::with_remote(remote));
        mgr
    }

    /// Accés als proveïdors remots registrats (token de l'usuari). El mòdul
    /// `imgen` els consulta per generar imatges online quan la màquina no té
    /// prou recursos o ComfyUI no està disponible.
    pub fn remote_manager(&self) -> Option<Arc<Mutex<ApiManager>>> {
        self.remote.clone()
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.ollama_url.trim_end_matches('/'), path)
    }

    /// Client HTTP (per a les ordres que parlen amb Hugging Bay, CivitAI…).
    pub fn http_client(&self) -> reqwest::Client {
        self.http.clone()
    }

    /// Adreça base del servei d'Ollama (per registrar models hi).
    pub fn base_url(&self) -> String {
        self.ollama_url.clone()
    }

    pub async fn is_running(&self) -> bool {
        self.http
            .get(self.url("/api/tags"))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    pub async fn list_models(&self) -> Result<Vec<OllamaModel>> {
        let resp = self
            .http
            .get(self.url("/api/tags"))
            .send()
            .await
            .map_err(|e| anyhow!("Ollama no respon: {}", e))?;
        let value: serde_json::Value = resp.json().await?;
        let models = value
            .get("models")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|m| OllamaModel {
                        name: m
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        size: m.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
                        modified_at: m
                            .get("modified_at")
                            .and_then(|s| s.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(models)
    }

    /// Normalitza un nom de model per comparar-lo amb els que Ollama reporta:
    /// «llama3.2» i «llama3.2:latest» són el mateix model.
    fn ps_key(name: &str) -> String {
        name.trim().trim_end_matches(":latest").to_lowercase()
    }

    /// Mida AL DISC d'un model (bytes) segons «/api/tags», o `None` si no la
    /// dona. Temps propi i curt: si Ollama està carregant alguna cosa, no volem
    /// afegir més esperada abans fins i tot de començar la generació.
    async fn model_disk_size(&self, model: &str) -> Option<u64> {
        let want = Self::ps_key(model);
        let value = self
            .http
            .get(self.url("/api/tags"))
            .timeout(std::time::Duration::from_secs(4))
            .send()
            .await
            .ok()?
            .json::<serde_json::Value>()
            .await
            .ok()?;
        value
            .get("models")?
            .as_array()?
            .iter()
            .find(|m| {
                Self::ps_key(m.get("name").and_then(|s| s.as_str()).unwrap_or_default()) == want
            })
            .and_then(|m| m.get("size").and_then(|s| s.as_u64()))
            .filter(|s| *s > 0)
    }

    /// Diu si Ollama té ARA MATEIX el model a la memòria («/api/ps»). Així es
    /// pot distingir «encara el carrega del disc» de «ja el té i llegeix el
    /// context». `None` = Ollama no respon i NO ho sabem: millor dir-ho que
    /// endevinar-ho (abans sempre deia «carregant», encara que no fos cert).
    async fn model_is_loaded(&self, model: &str) -> Option<bool> {
        let want = Self::ps_key(model);
        let value = self
            .http
            .get(self.url("/api/ps"))
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
            .ok()?
            .json::<serde_json::Value>()
            .await
            .ok()?;
        Some(
            value
                .get("models")?
                .as_array()?
                .iter()
                .any(|m| {
                    ["model", "name"]
                        .iter()
                        .filter_map(|k| m.get(*k).and_then(|s| s.as_str()))
                        .any(|n| Self::ps_key(n) == want)
                }),
        )
    }

    /// Models que Ollama té ARA carregats a la memòria. Permet ensenyar a
    /// l'usuari quanta RAM ocupen i alliberar-la d'un clic.
    pub async fn loaded_models(&self) -> Result<Vec<LoadedModel>> {
        let value = self
            .http
            .get(self.url("/api/ps"))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| anyhow!("Ollama no respon: {}", e))?
            .json::<serde_json::Value>()
            .await?;
        let out = value
            .get("models")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let name = ["model", "name"]
                            .iter()
                            .filter_map(|k| m.get(*k).and_then(|s| s.as_str()))
                            .next()?
                            .to_string();
                        Some(LoadedModel {
                            name,
                            size: m.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
                            size_vram: m.get("size_vram").and_then(|v| v.as_u64()).unwrap_or(0),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(out)
    }

    /// Allibera de la memòria un model (o TOTS els carregats) amb
    /// `keep_alive: 0`. Retorna quants models s'han descarregat.
    pub async fn unload_models(&self, model: Option<&str>) -> Result<usize> {
        let names: Vec<String> = match model.map(str::trim).filter(|m| !m.is_empty()) {
            Some(m) => vec![m.to_string()],
            None => self
                .loaded_models()
                .await?
                .into_iter()
                .map(|m| m.name)
                .collect(),
        };
        for name in &names {
            // /api/generate amb keep_alive 0 i sense prompt = descarrega.
            let _ = self
                .http
                .post(self.url("/api/generate"))
                .timeout(std::time::Duration::from_secs(20))
                .json(&serde_json::json!({ "model": name, "keep_alive": 0 }))
                .send()
                .await;
        }
        Ok(names.len())
    }

    /// ALLIBERA RAM quan la memòria no dona: descarrega els models que Ollama
    /// té a la RAM (i deixa el servei engegat, buit). Retorna quants n'ha
    /// alliberat. La IA torna a carregar el que li cal quan fa la petició
    /// següent: és més lent, però evita que l'agent es quedi aturat.
    pub async fn free_ram(&self) -> usize {
        match self.unload_models(None).await {
            Ok(n) => n,
            Err(_) => 0,
        }
    }

    /// Defineix quant de temps Ollama manté els models a la RAM.
    /// Buit = valor per defecte d'Ollama.
    pub async fn set_keep_alive(&self, value: Option<String>) {
        let v = value
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        *self.keep_alive.write().await = v;
    }

    pub async fn keep_alive(&self) -> Option<String> {
        self.keep_alive.read().await.clone()
    }

    /// descarrega un model amb streaming de progrés (event "ai://pull").
    pub async fn pull_model(&self, app: Option<&tauri::AppHandle>, name: &str) -> Result<String> {
        use tauri::Emitter;
        let body = serde_json::json!({ "name": name, "stream": true });
        let resp = self
            .http
            .post(self.url("/api/pull"))
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("Error descarregant: {}", e))?;
        if !resp.status().is_success() {
            return Err(anyhow!("Ollama error: {}", resp.status()));
        }
        let mut stream = resp.bytes_stream();
        let mut last = String::new();
        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            for line in chunk.split(|b| *b == b'\n') {
                if line.is_empty() {
                    continue;
                }
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(line) {
                    let status = v
                        .get("status")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    let pct = v.get("completed").and_then(|c| c.as_f64()).unwrap_or(0.0);
                    let total = v.get("total").and_then(|c| c.as_f64()).unwrap_or(1.0);
                    let msg = if total > 0.0 && pct > 0.0 {
                        format!("{} {:.0}%", status, pct / total * 100.0)
                    } else {
                        status
                    };
                    if msg != last {
                        last = msg.clone();
                        // Sense AppHandle (p. ex. recuperació 404 dins d'un xat)
                        // el progrés no s'emet: la baixada continua igual.
                        if let Some(app) = app {
                            let _ = app.emit("ai://pull", serde_json::json!({
                                "model": name, "message": msg
                            }));
                        }
                    }
                }
            }
        }
        Ok(format!("Model '{}' descarregat", name))
    }

    /// Nom del MODEL INICIAL per a usuaris nous, segons la RAM de la màquina
    /// (quantització Q4 d'ollama.com): prou per seguir instruccions i escriure
    /// codi sense quedar-se sense memòria. Un Mac Intel de 8 GB amb qwen3:4b
    /// va bé; màquines modestes reben el 1.7b o el 0.6b.
    pub fn starter_model_name() -> String {
        const GB: u64 = 1024 * 1024 * 1024;
        match Self::total_memory_bytes() {
            Some(ram) if ram >= 8 * GB => "qwen3:4b".to_string(),
            Some(ram) if ram >= 4 * GB => "qwen3:1.7b".to_string(),
            _ => "qwen3:0.6b".to_string(),
        }
    }

    /// El model demanat no està instal·lat (404) però Ollama TÉ d'altres:
    /// en tria un de JA INSTAL·LAT per continuar (preferint el model inicial)
    /// — el xat funciona DE SEGUIT sense baixar res i l'usuari veu l'avís.
    /// None si realment no hi ha cap model instal·lat (llavors toca l'inicial).
    fn pick_fallback_model(installed: &[OllamaModel]) -> Option<String> {
        if installed.is_empty() {
            return None;
        }
        let starter = Self::starter_model_name();
        installed
            .iter()
            .find(|m| m.name == starter)
            .or_else(|| installed.first())
            .map(|m| m.name.clone())
    }

    /// Baixa el MODEL INICIAL si Ollama no té CAP model instal·lat (estat
    /// d'un usuari que acabà d'instal·lar-ho: sense això TOTS els xats
    /// fallarien amb 404). Retorna el nom baixat, o None si ja n'hi havia /
    /// no s'ha pogut baixar. Emès amb els events «ai://pull» de progrés.
    pub async fn pull_starter_model(&self, app: &tauri::AppHandle) -> Option<String> {
        use tauri::Emitter;
        let empty = self
            .list_models()
            .await
            .map(|m| m.is_empty())
            .unwrap_or(true);
        if !empty {
            return None;
        }
        let name = Self::starter_model_name();
        let _ = app.emit(
            "ai://pull",
            serde_json::json!({
                "model": name,
                "message": format!(
                    "Primer arrencada: descarregant el model inicial «{}» (només passa un cop)…",
                    name
                )
            }),
        );
        match self.pull_model(Some(app), &name).await {
            Ok(_) => Some(name),
            Err(e) => {
                let _ = app.emit(
                    "ai://pull",
                    serde_json::json!({
                        "model": name,
                        "message": format!("No s'ha pogut baixar el model inicial: {}", e)
                    }),
                );
                None
            }
        }
    }

    /// Tasca d'ARRANCADA: espera que el servei Ollama escolti (pot tardar:
    /// pot estar descarregant-se i instal·lant-se ara mateix) i, si no té cap
    /// model, en baixa un d'inicial. No bloca la UI — s'invoca des d'un fil
    /// de fons en obrir NoOrbit.
    pub async fn ensure_default_model(&self, app: &tauri::AppHandle) {
        // Fins a 3 minuts: la primera instal·lació d'Ollama pot trigar.
        for _ in 0..90 {
            if self.is_running().await {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        if self.is_running().await {
            self.pull_starter_model(app).await;
        }
    }

    pub async fn delete_model(&self, name: &str) -> Result<()> {
        self.http
            .delete(self.url("/api/delete"))
            .json(&serde_json::json!({ "name": name }))
            .send()
            .await?;
        Ok(())
    }

    pub fn suggested_models() -> Vec<serde_json::Value> {
        // La mida és l'APROXIMACIÓ real del pes de descàrrega a ollama.com
        // (quantització Q4 per defecte): l'usuari la veu ABANS de prémer
        // «Baixa» — sense això no sabia si li cabria al disc ni a la RAM.
        [
            ("qwen3:8b", "Qwen3 8B — raonament i codi; pesos oberts (Apache 2.0), modificables/afinables", 5_200_000_000u64),
            ("qwen2.5-coder:7b", "Codi — equilibri qualitat/velocitat", 4_700_000_000),
            ("qwen2.5-coder:3b", "Codi — lleuger, per a màquines modestes", 1_900_000_000),
            ("llama3.2:3b", "General — ràpid i multilingüe", 2_000_000_000),
            ("mistral:7b", "General — bo seguint instruccions", 4_100_000_000),
            ("codellama:13b", "Codi — més qualitat, més lent", 7_400_000_000),
            ("stable-diffusion-v2:latest", "Imàtges — requereix GPU potent", 5_000_000_000),
            // famílies «sense filtres»: públiques a Ollama, NOMÉS opcions
            // instal·lables (no es descarreguen sols). Cal prou RAM: un 7B
            // necessita ~5 GB, un 8x7b ~24 GB (inviable en 8 GB).
            ("dolphin-mistral:7b", "Sense filtres — codi i raonament (7B)", 4_900_000_000),
            ("dolphin-mixtral:8x7b", "Sense filtres — MoE 8x7b, codi/raonament", 26_000_000_000),
            ("goekdenizguelmez/JOSIEFIED-Qwen3", "Sense filtres — Qwen3-8B abliterated, codi i raonament", 5_200_000_000),
            ("richardyoung/qwen3-8b-abliterated", "Sense filtres — Qwen3-8B abliterated per modificar i ús lliure", 5_200_000_000),
            // Xiaomi MiMo: raonament fort en matemàtiques i codi, amb
            // «thinking» visible que NoOrbit mostra al xat de l'agent.
            ("alibayram/mimo-7b-rl", "Xiaomi MiMo-7B-RL — raonament (mat/codi), thinking visible", 4_700_000_000),
            ("maternion/mimo-v2.6", "Xiaomi MiMo-V2.6 — agent 9B, crides d'eines i codi", 5_800_000_000),
            // OpenAI de pesos oberts (únicos GPT descarregables localment):
            // MoE molt eficients; el 120b demana ~60 GB de RAM/VRAM.
            ("gpt-oss:20b", "OpenAI de pesos oberts — MoE 20B, qualitat/velocitat", 13_000_000_000),
            ("gpt-oss:120b", "OpenAI de pesos oberts — MoE 120B, màxima qualitat local", 64_000_000_000),
            // GLM Z.ai natiu multimodal «flash» (està a ollama.com, descarregable
            // amb `ollama pull`): no donem mida perquè canvia per quantització.
            ("glm-5.3-flash", "Z.ai GLM-5.3 Flash — MoE multimodal 18B actius, codi/agents de primer nivell; veu imatges", 0),
            ("glm-ocr", "Z.ai GLM OCR — llegeix text de fitxes, captures i documents en imatge", 0),
        ]
        .iter()
        .map(|(name, desc, size)| {
            // Mida 0 = no la sabem: serialitzem null i la UI no mostra xip.
            let size = if *size == 0 {
                serde_json::Value::Null
            } else {
                serde_json::json!(*size)
            };
            serde_json::json!({ "name": name, "description": desc, "size": size })
        })
        .collect()
    }

    /// Cerca models a la biblioteca del NÚVOL d'Ollama (ollama.com).
    /// Amb consulta buida llista la biblioteca; amb text, cerca-hi.
    /// Retorna els models amb nom i descripció perquè l'usuari en triï un
    /// i el descarregui amb `pull_model` (que ja funciona per a qualsevol nom).
    pub async fn search_cloud(&self, query: &str) -> Result<Vec<CloudModel>> {
        let q = query.trim();
        let url = if q.is_empty() {
            "https://ollama.com/library".to_string()
        } else {
            format!("https://ollama.com/search?q={}&c=library", urlencoding::encode(q))
        };
        let resp = self
            .http
            .get(&url)
            .timeout(std::time::Duration::from_secs(20))
            .header("User-Agent", "NoOrbit")
            .send()
            .await
            .map_err(|e| anyhow!("No s'ha pogut contactar amb ollama.com: {}", e))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("ollama.com ha respost {}", status));
        }
        let html = resp.text().await?;
        Ok(parse_cloud_models(&html))
    }

    /// Cerca models GGUF a INTERNET (Hugging Face) per si NO són a la
    /// biblioteca d'Ollama. Retorna noms amb el format «hf.co/<autor>/<model>»
    /// perquè Ollama els baixe NATIVAMENT amb `pull_model` (que ja accepta
    /// qualsevol nom, inclòs hf.co/…). No requereix Ollama en marxa: només
    /// consulta l'API pública de huggingface.co.
    pub async fn search_huggingface(&self, query: &str) -> Result<Vec<CloudModel>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        // «filter=gguf» retorna repositoris amb fitxers GGUF (els que Ollama
        // pot carregar). «sort=downloads» prioritza els més descarregats.
        // «blobs=true» hi afegeix la MIDA de cada fitxer: amb això sumem els
        // .gguf del repo i l'usuari veu quant pesa ABANS de descarregar-lo.
        let url = format!(
            "https://huggingface.co/api/models?search={}&filter=gguf&sort=downloads&direction=-1&limit=40&blobs=true",
            urlencoding::encode(q)
        );
        let resp = self
            .http
            .get(&url)
            .timeout(std::time::Duration::from_secs(20))
            .header("User-Agent", "NoOrbit")
            .send()
            .await
            .map_err(|e| anyhow!("No s'ha pogut contactar amb huggingface.co: {}", e))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("huggingface.co ha respost {}", status));
        }
        let value: serde_json::Value = resp.json().await?;
        let models = value
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        // «id» o «modelId» és el camí autor/model del repositori.
                        let id = m
                            .get("id")
                            .or_else(|| m.get("modelId"))
                            .and_then(|i| i.as_str())
                            .map(|s| s.to_string())?;
                        if id.is_empty() {
                            return None;
                        }
                        // Baixa «hf.co/<id>»: Ollama l'enten i en carrega el GGUF.
                        let name = format!("hf.co/{}", id);
                        // Descripció breu: nº de descàrregues si ve al JSON.
                        let description = m
                            .get("downloads")
                            .and_then(|d| d.as_f64())
                            .map(|d| format!("Hugging Face · {:.0} descàrregues", d))
                            .unwrap_or_else(|| "Hugging Face (GGUF)".to_string());
                        // MIDA de la descàrrega: suma dels fitxers .gguf del
                        // repo (el que Ollama baixarà). Sense blobs no hi ha
                        // mides: es queda None i la UI no mostra res.
                        let size = m
                            .get("siblings")
                            .and_then(|s| s.as_array())
                            .map(|files| {
                                files
                                    .iter()
                                    .filter_map(|f| {
                                        let name = f.get("rfilename").and_then(|r| r.as_str())?;
                                        if !name.to_ascii_lowercase().ends_with(".gguf") {
                                            return None;
                                        }
                                        f.get("size").and_then(|s| s.as_u64())
                                    })
                                    .sum::<u64>()
                            })
                            .filter(|s| *s > 0);
                        Some(CloudModel { name, description, size, url: None })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(models)
    }

    /// Cerca models d'IMATGE a CivitAI (civitai.com), la tercera font del
    /// gestor juntament amb Hugging Face i Ollama: checkpoints de
    /// stable-diffusion, LoRAs… L'API pública no cal token. Retorna un model
    /// per fitxer descarregable amb la seva URL DIRECTA: la UI el passa a
    /// «image_cpu_download_model» i el fitxer va a parar a la carpeta
    /// «sd-models/» del rodet de CPU (funciona també en Mac Intel).
    pub async fn search_civitai(&self, query: &str) -> Result<Vec<CloudModel>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!(
            "https://civitai.com/api/v1/models?query={}&limit=30&types=Checkpoint",
            urlencoding::encode(q)
        );
        let resp = self
            .http
            .get(&url)
            .timeout(std::time::Duration::from_secs(20))
            .header("User-Agent", "NoOrbit")
            .send()
            .await
            .map_err(|e| anyhow!("No s'ha pogut contactar amb civitai.com: {}", e))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("civitai.com ha respost {}", status));
        }
        let value: serde_json::Value = resp.json().await?;
        let items = value
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(items
            .iter()
            .filter_map(|m| {
                let title = m.get("name").and_then(|n| n.as_str())?.trim();
                if title.is_empty() {
                    return None;
                }
                let mid = m.get("id").and_then(|i| i.as_f64()).unwrap_or(0.0);
                // Primera versió publicada + el seu fitxer principal (el que
                // es pot baixar i posar al rodet de CPU).
                let v0 = m
                    .get("modelVersions")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .cloned()
                    .unwrap_or_default();
                let file = v0
                    .get("files")
                    .and_then(|f| f.as_array())
                    .and_then(|a| a.first())
                    .cloned()
                    .unwrap_or_default();
                let fname = file
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let dl = file
                    .get("downloadUrl")
                    .and_then(|d| d.as_str())
                    .map(|d| d.to_string())
                    .map(|d| if d.is_empty() {
                        format!("https://civitai.com/api/download/models/{}",
                            v0.get("id").and_then(|i| i.as_f64()).unwrap_or(0.0))
                    } else {
                        d
                    });
                let size = file
                    .get("sizeKB")
                    .and_then(|s| s.as_f64())
                    .map(|kb| (kb * 1_000.0) as u64)
                    .filter(|s| *s > 0);
                // Descripció curta: tipus + model + base + subtítols sense HTML.
                let brief = {
                    let base = v0
                        .get("baseModel")
                        .and_then(|b| b.as_str())
                        .unwrap_or("")
                        .to_string();
                    let creator = m
                        .get("creator")
                        .and_then(|c| c.get("username"))
                        .and_then(|u| u.as_str())
                        .unwrap_or("CivitAI");
                    if base.is_empty() {
                        format!("CivitAI · {}", creator)
                    } else {
                        format!("CivitAI · base {} · {}", base, creator)
                    }
                };
                Some(CloudModel {
                    name: if fname.is_empty()
                        { format!("civitai-{}.safetensors", mid) }
                        else { fname },
                    description: format!("{} — {}", title, brief),
                    size,
                    url: dl,
                })
            })
            .collect())
    }

    /// Retorna el control d'un xat concret, creant-lo si és nou.
    fn ctl(&self, session: &str) -> Arc<SessionCtl> {
        let mut map = self.sessions.lock().unwrap();
        let entry = map
            .entry(session.to_string())
            .or_insert_with(|| Arc::new(SessionCtl::new(session)))
            .clone();
        // EVICCIÓ: el mapa de sessions creixia per sempre (cada xat nou
        // quedava per sempre al mapa). Netejem els xats vells que no estan
        // generant: en mantenim els N més recents.
        if map.len() > MAX_SESSIONS {
            let mut ids: Vec<(std::time::Instant, String)> = map
                .iter()
                .map(|(k, v)| (v.last_used(), k.clone()))
                .collect();
            ids.sort_by(|a, b| a.0.cmp(&b.0));
            let surplus = map.len() - MAX_SESSIONS;
            for (_, id) in ids.into_iter().take(surplus) {
                if id != session {
                    map.remove(&id);
                }
            }
        }
        entry.clone()
    }

    /// Control del xat que està generant ARA mateix (vegeu `in_session`).
    fn ctl_now(&self) -> Arc<SessionCtl> {
        self.ctl(&current_session())
    }

    /// Fixa el que un xat ha d'usar a partir d'ara: si el missatge porta
    /// proveïdor o model, es guarden al xat. Així les ordres següents d'aquell
    /// mateix xat (i accions com «aplica a Blender») segueixen la mateixa IA.
    async fn bind(&self, ctl: &Arc<SessionCtl>, opts: &ChatOpts) {
        if opts.provider.is_some() {
            ctl.set_provider(opts.provider.as_deref()).await;
        }
        if opts.model.is_some() {
            ctl.set_model(opts.model.as_deref()).await;
        }
    }

    /// Proveïdor d'un xat: el que ell hagi triat o, si no n'hi ha, el global.
    async fn effective_provider(&self, ctl: &Arc<SessionCtl>) -> String {
        match ctl.provider().await {
            Some(p) => p,
            None => self.active_provider.read().await.clone(),
        }
    }

    /// Primer proveïdor EN LÍNIA utilitzable (activat i amb token). Serveix
    /// perquè NoOrbit funcioni SENSE OLLAMA: si el motor local no hi és (o no
    /// està instal·lat), el xat continua amb la IA del núvol de l'usuari.
    pub fn first_working_remote(&self) -> Option<String> {
        let remote = self.remote.as_ref()?;
        let guard = remote.lock().unwrap();
        guard
            .providers
            .iter()
            .find(|p| p.enabled && p.has_token())
            .map(|p| p.id.clone())
    }

    /// Hi ha ALGUNA IA disponible (local en marxa o remota amb token)?
    pub async fn has_any_provider(&self) -> bool {
        if self.is_running().await {
            return true;
        }
        self.first_working_remote().is_some()
    }

    /// Model d'un xat: el seu propi o, si no, el triat globalment.
    async fn model_for(&self, ctl: &Arc<SessionCtl>) -> String {
        match ctl.model().await {
            Some(m) if !m.trim().is_empty() => m,
            _ => self.effective_model().await,
        }
    }

    /// Etiqueta (proveïdor, model) amb la qual treballa un xat ara mateix. El
    /// visor del procés la fa servir per a etiquetar també els errors.
    pub async fn session_labels(&self, session: &str) -> (String, String) {
        let ctl = self.ctl(session);
        let mut provider = self.effective_provider(&ctl).await;
        // SENSE OLLAMA: caiguda a la IA en línia de l'usuari (si n'hi ha).
        if provider == "ollama" && !self.is_running().await {
            if let Some(remot) = self.first_working_remote() {
                provider = remot;
            }
        }
        let model = self.model_for(&ctl).await;
        (provider, model)
    }

    /// La comanda de xat només ha de contar un faliment si cap backend (Ollama
    /// o remot) no n'ha llançat ja un amb explicació i temps real.
    pub fn session_error_shown(&self, session: &str) -> bool {
        self.ctl(session).error_shown()
    }

    /// Xat **NOMÉS local** (Ollama): mai no remeteix la conversa a cap
    /// proveïdor en línia, ni quan el model local falla (aquí no hi ha cap
    /// reserva a l'IA remota).
    ///
    /// Serveix per a l'APRENENATGE: les converses de l'usuari es destil·len
    /// amb el model de la màquina i no suren mai de l'ordinador.
    pub async fn chat_local_only(
        &self,
        prompt: &str,
        system: Option<&str>,
        model: Option<&str>,
    ) -> Result<String> {
        let ctl = self.ctl_now();
        if let Some(m) = model {
            if !m.trim().is_empty() {
                ctl.set_model(Some(m)).await;
            }
        }
        ctl.begin().await;
        // Proveïdor «ollama» = camí local SEMPRE (dins `chat_inner`).
        self.chat_inner(prompt, system, &ctl, "ollama", &ChatOpts::default())
            .await
    }

    /// Envia un prompt de xat i retorna la resposta completa.
    /// Si hi ha un proveïdor remot actiu, enruta cap a ell; si no, usa Ollama.
    pub async fn chat(&self, prompt: &str, system: Option<&str>) -> Result<String> {
        self.chat_opts(prompt, system, &ChatOpts::default()).await
    }

    /// Igual que `chat` però permet forçar un model concret (per als
    /// especialistes/agents que en tenen un d'assignat).
    pub async fn chat_with(
        &self,
        prompt: &str,
        system: Option<&str>,
        model_override: Option<&str>,
    ) -> Result<String> {
        let opts = ChatOpts {
            provider: None,
            model: model_override.map(|m| m.to_string()),
            history: Vec::new(),
            images: Vec::new(),
        };
        self.chat_opts(prompt, system, &opts).await
    }

    /// Xat d'un sol torn DINS DEL XAT ACTUAL: amb el seu proveïdor, el seu
    /// model i la conversa precedent (`opts.history`).
    pub async fn chat_opts(
        &self,
        prompt: &str,
        system: Option<&str>,
        opts: &ChatOpts,
    ) -> Result<String> {
        let ctl = self.ctl_now();
        self.bind(&ctl, opts).await;
        // Cada generació nova comença sense bandera d'aturada i sense raonament.
        ctl.begin().await;
        let provider = self.effective_provider(&ctl).await;
        let mut result = self.chat_inner(prompt, system, &ctl, &provider, opts).await;
        // Memòria exhaurida a mitja resposta: ATUREM, ALLIBEREM RAM i
        // CONDENSEM el context (no el retallem: així es conserva l'esperit del
        // programa); la IA CONTINUA la feina a trossos en lloc de començar de
        // zero o de quedar-se penjada.
        if let Err(e) = &result {
            if e.to_string() != CANCELLED_MSG && is_body_cut(&e.to_string()) {
                // RAM: alliberem els models que Ollama té carregats.
                self.free_ram().await;
                let task = compact::extract_task(prompt);
                let memoria = compact::dense_compact(&opts.history, &task);
                let light = ChatOpts {
                    // El fil ja és DINS de la memòria: no l'enviem dues vegades.
                    history: Vec::new(),
                    ..opts.clone()
                };
                result = self.chat_inner(&memoria, system, &ctl, &provider, &light).await;
            }
        }
        match result {
            // Els errors ja cancel·lats es retornen nets; la resta es tradueixen.
            Err(e) if e.to_string() == CANCELLED_MSG => Err(e),
            Err(e) => {
                // La IA principal no ha pogut: s'intenta amb una IA local
                // externa o amb els proveïdors oficials de l'usuari. La
                // resposta delegada conserva el seu origen (provider/model)
                // en el tipus; ací el xat sense streaming només retorna text.
                if let Ok(resp) = self.external.delegate(prompt, system).await {
                    return Ok(resp.text);
                }
                Err(anyhow!(friendly_error(&e.to_string())))
            }
            ok => ok,
        }
    }

    /// Xat amb **streaming**: emet cada fragment amb l'event «ai://chunk»
    /// mentre el model genera i retorna el text complet al final. A més, emet
    /// l'event unificat «ai://process» (fase + proveïdor + model + temps) per a
    /// QUALSEVOL backend: Ollama (NDJSON) i proveïdors remots (SSE d'OpenAI/
    /// Claude). Així la UI pot veure el text arribant token a token siga quin
    /// siga el model.
    pub async fn chat_stream(
        &self,
        app: &tauri::AppHandle,
        prompt: &str,
        system: Option<&str>,
        opts: &ChatOpts,
    ) -> Result<String> {
        let ctl = self.ctl_now();
        self.bind(&ctl, opts).await;
        ctl.begin().await;
        let start = std::time::Instant::now();

        let mut active = self.effective_provider(&ctl).await;
        // SENSE OLLAMA: si el motor local no està instal·lat (o no escolta) i
        // l'usuari té un proveïdor EN LÍNIA amb token, el xat continua amb
        // aquell — NoOrbit funciona igual (editor, terminal, imatges, Blender).
        if active == "ollama" && !self.is_running().await {
            if let Some(remot) = self.first_working_remote() {
                let (_, m) = self.session_labels(&ctl.id).await;
                ctl.process(
                    app,
                    &remot,
                    &m,
                    "thinking",
                    &format!(
                        "Ollama no està instal·lat o no està en marxa: faig servir                          «{}» (el teu proveïdor en línia). NoOrbit funciona igual                          sense el motor local.",
                        remot
                    ),
                    0,
                );
                active = remot;
            }
        }
        // Proveïdor remot: fa streaming SSE (emet «ai://process» token a token,
        // com Ollama). La cancel·lació és la del XAT que ha enviat el missatge.
        if active != "ollama" {
            if let Some(remote) = &self.remote {
                let (provider, client) = remote.lock().unwrap().prepare(&active)?;
                let mut res = api::chat_stream(
                    app,
                    &client,
                    &provider,
                    &conversation(&opts.history, prompt),
                    system,
                    &ctl,
                    start,
                )
                .await;
                // Memòria exhaurida: comprimeix el context i torna-hi una vegada.
                if let Err(e) = &res {
                    if e.to_string() != CANCELLED_MSG && is_body_cut(&e.to_string()) {
                        let compacted = compact_prompt(prompt);
                        if compacted != prompt {
                            ctl.process(
                                app,
                                &provider.name,
                                &provider.model,
                                "thinking",
                                "La memòria no abastava tot el context: el comprimeix i ho torna a provar.",
                                0,
                            );
                            res = api::chat_stream(
                                app,
                                &client,
                                &provider,
                                &conversation(&compact_history(&opts.history), &compacted),
                                system,
                                &ctl,
                                start,
                            )
                            .await;
                        }
                    }
                }
                match res {
                    Ok((content, thinking)) => {
                        ctl.set_thinking(thinking).await;
                        return Ok(content);
                    }
                    Err(e) if e.to_string() == CANCELLED_MSG => return Err(e),
                    Err(e) => {
                        // Fallida del proveïdor remot: última oportunitat abans
                        // de mostrar l'error — delegar a una IA local externa.
                        if let Some(resp) =
                            self.external_fallback_stream(app, prompt, system, &ctl, &start).await
                        {
                            return Ok(resp);
                        }
                        return Err(anyhow!(friendly_error(&e.to_string())));
                    }
                }
            }
        }

        // Ollama (local): intent normal amb el prompt sencer; si la connexió es
        // talla per manca de memòria, segon intent amb el context CONDENSAT,
        // perquè l'agent pugui SEGUIR PROGRAMANT en lloc de rendir-se.
        let mut res = self.ollama_stream(app, prompt, system, &ctl, opts, &start).await;
        if let Err(e) = &res {
            if e.to_string() != CANCELLED_MSG && is_body_cut(&e.to_string()) {
                // RAM: ATUREM, ALLIBEREM la memòria dels models d'Ollama i
                // després CONTINUEM amb el context condensat.
                let alliberats = self.free_ram().await;
                let task = compact::extract_task(prompt);
                let memoria = compact::dense_compact(&opts.history, &task);
                // El visor ha de continuar mostrant el model REAL: posar-hi
                // «noorbit» com a etiqueta confonia l'usuari.
                let (_, m) = self.session_labels(&ctl.id).await;
                ctl.process(
                    app,
                    "ollama",
                    &m,
                    "thinking",
                    &format!(
                        "La memòria no donava per tot el context: he ALLIBERAT RAM ({} \
                         model(s) descarregats de la memòria) i CONDENSO el context \
                         (conservo l'objectiu, el pla i l'esquelet del codi). La IA \
                         CONTINUA la feina a trossos.",
                        alliberats
                    ),
                    start.elapsed().as_millis() as u64,
                );
                let light = ChatOpts {
                    history: Vec::new(),
                    ..opts.clone()
                };
                res = self
                    .ollama_stream(app, &memoria, system, &ctl, &light, &start)
                    .await;
            }
        }
        match res {
            Err(e) if e.to_string() == CANCELLED_MSG => Err(e),
            Err(e) => {
                // Ni remot ni local principal han resolt: es delega a una IA
                // externa i la seua resposta arriba etiquetada amb el seu
                // origen real.
                if let Some(resp) =
                    self.external_fallback_stream(app, prompt, system, &ctl, &start).await
                {
                    return Ok(resp);
                }
                Err(anyhow!(friendly_error(&e.to_string())))
            }
            ok => ok,
        }
    }

    /// Bot de rescat extern: delega un torn fallit a una IA local externa o
    /// a un proveïdor oficial amb token, i reemet la resposta amb els mateixos
    /// events «ai://chunk» i «ai://process» que qualsevol generació, però
    /// etiquetant-ne l'ORIGEN REAL (proveïdor i model que han respost):
    /// la interfície mai indica que siga una generació interna de NoOrbit.
    async fn external_fallback_stream(
        &self,
        app: &tauri::AppHandle,
        prompt: &str,
        system: Option<&str>,
        ctl: &Arc<SessionCtl>,
        start: &std::time::Instant,
    ) -> Option<String> {
        if ctl.is_cancelled() {
            return None;
        }
        let resp = self.external.delegate(prompt, system).await.ok()?;
        if resp.text.trim().is_empty() {
            return None;
        }
        // Emet el text a trossets, amb el provider/model reals de l'origen.
        let mut rest: &str = &resp.text;
        while !rest.is_empty() {
            let mut n = 96usize.min(rest.len());
            while n > 0 && !rest.is_char_boundary(n) {
                n -= 1;
            }
            let (head, tail) = rest.split_at(n.max(1));
            ctl.chunk(app, head);
            ctl.process(
                app,
                &resp.provider,
                &resp.model,
                "streaming",
                head,
                start.elapsed().as_millis() as u64,
            );
            rest = tail;
            tokio::time::sleep(std::time::Duration::from_millis(8)).await;
            if ctl.is_cancelled() {
                break;
            }
        }
        ctl.process(
            app,
            &resp.provider,
            &resp.model,
            "done",
            "",
            start.elapsed().as_millis() as u64,
        );
        Some(resp.text)
    }

    /// Streaming NDJSON contra Ollama (cos de `chat_stream`, sense la capa de
    /// reintent amb context comprimit).
    async fn ollama_stream(
        &self,
        app: &tauri::AppHandle,
        prompt: &str,
        system: Option<&str>,
        ctl: &Arc<SessionCtl>,
        opts: &ChatOpts,
        start: &std::time::Instant,
    ) -> Result<String> {
        let bg = self.is_background();
        let model = match opts.model.as_deref().filter(|m| !m.trim().is_empty()) {
            Some(m) => m.to_string(),
            None => self.model_for(ctl).await,
        };
        let model = self.resolve_usable_model(model).await;
        // Conversa del xat: primer els torns ja digitats (si el missatge ve
        // d'un fork o d'una conversa llarga) i després el prompt actual.
        let mut messages: Vec<ChatMessage> = conversation(&opts.history, prompt)
            .into_iter()
            .map(|(role, content)| ChatMessage {
                role,
                content,
                thinking: None,
                images: None,
            })
            .collect();
        attach_images(&mut messages, &opts.images);
        // El context que realment veu el model és tota la conversa, no només
        // l'últim missatge: la finestra de context s'ha de dimensionar per això.
        let prompt_chars = messages.iter().map(|m| m.content.chars().count()).sum();
        let sys_chars = system.map_or(0, |s| s.len());
        let options = if bg {
            Self::ctx_options_bg(prompt_chars, sys_chars)
        } else {
            Self::ctx_options(prompt_chars, sys_chars)
        };
        // Finestra de context REAL d'aquest torn («num_ctx»): la interfície la
        // mostra com a «tant % del context s'ha ocupat».
        let ctx_window = options
            .as_ref()
            .and_then(|o| o.get("num_ctx"))
            .and_then(|v| v.as_u64());
        let model_label = model.clone();
        // Els models que declaren l'habilitat «thinking» (qwen3, deepseek-r1…)
        // poden emetre el raonament en un CANAL PROPI: se'ls demana de forma
        // expressa. Els que no la tenen rebutjarien la bandera amb un 400
        // («does not support thinking»), així que simplement no s'envia.
        let think = if self.supports_thinking(&model_label).await {
            Some(true)
        } else {
            None
        };
        let mut req = ChatRequest {
            model,
            messages,
            stream: true,
            system: system.map(|s| s.to_string()),
            think,
            keep_alive: self.keep_alive.read().await.clone(),
            options,
        };
        // Arrencada del procés: el visor explica QUÈ està fent NoOrbit en lloc
        // de quedar-se en blanc amb un simple «Treballant…». Abans de llançar
        // la generació es miren les DADES reals del model (mida al disc, si ja
        // és a la memòria i quanta RAM té l'equip): així les notes d'espera
        // diuen veritats en lloc d'endevinar-ho.
        let size = self.model_disk_size(&model_label).await;
        let already_loaded = self.model_is_loaded(&model_label).await == Some(true);
        let mut wc = WaitCtx::new(model_label.clone(), size, Self::total_memory_bytes());
        ctl.process(
            app,
            "ollama",
            &model_label,
            "thinking",
            &format!(
                "NoOrbit crida «{}» a Ollama…{}\n",
                model_label,
                if already_loaded {
                    " (ja el té a la memòria: anirà ràpid)"
                } else {
                    ""
                }
            ),
            0,
        );
        // Avís immediat: si el model no cap còmodament a la RAM, l'usuari ho
        // sap ABANS d'esperar-se minuts sense saber per què.
        if wc.tight() {
            ctl.process(
                app,
                "ollama",
                &model_label,
                "thinking",
                &format!(
                    "⚠️ Aquest model ocupa {} i l'equip en té {} de RAM: carregar-lo pot tardar \
                     minuts i anar molt lent. Al «Gestor de models» pots triar-ne un de més \
                     lleuger o un de gratuït al NÚVOL (aquest NO cal baixar-lo).\n",
                    fmt_gb(size.unwrap_or_default()),
                    fmt_gb(wc.ram.unwrap_or_default())
                ),
                0,
            );
        }
        if think.is_none() {
            ctl.process(
                app,
                "ollama",
                &model_label,
                "thinking",
                "Aquest model no emet el raonament en un canal separat: NoOrbit mostra el text que va generant, paraula a paraula.\n",
                0,
            );
        }
        let chat_url = self.url("/api/chat");
        use futures::StreamExt;
        let notify = ctl.notify();
        // Sense temps màxim total (http_stream): un model local en CPU pot
        // tardar més de quatre minuts i tallar-ho era un error falsò. Mentre
        // espera la capçalera, el visor ho pregunta a Ollama cada 15 s i només
        // en parla quan hi ha alguna cosa NOVA a dir (o cada 60 s).
        // «Atura» es mira des de ja: la futura d'avís es crea ABANS d'esperar
        // i es manté d'una iteració a l'altra (si es creara dins del `select!`,
        // un clic fet entre dues iteracions es podria perdre).
        let watch = async {
            loop {
                if ctl.is_cancelled() {
                    return;
                }
                tokio::select! {
                    _ = notify.notified() => return,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(150)) => {}
                }
            }
        };
        tokio::pin!(watch);
        // USUARI NOU SENSE CAP MODEL: el primer enviament a un Ollama acabat
        // d'instal·lar retorna 404 («model no trobat»). En lloc de mostrar un
        // error tècnic, baixem el model inicial (progrés a «ai://pull») i
        // REINTENTEM el mateix xat una vegada: l'usuari no ha de fer res.
        let mut starter_tried = false;
        // Model de RESERVA ja instal·lat si el triat no hi és (404): una sola
        // vegada — si el de reserva tampoc va (impossible en teoria), error.
        let mut fallback_used: Option<String> = None;
        // Podem instal·lar Ollama en segon pla? Només si l'usuari NO té cap
        // IA en línia configurada (respectem la seva elecció).
        let auto_install = self.first_working_remote().is_none();
        let resp = loop {
            // El futur «send» reté el préstec de «req»: el contenim en un
            // bloc propi perquè mori ABANS de poder substituir el model.
            let resp = {
                let send = Self::send_chat(&self.http_stream, &chat_url, &req, auto_install);
                tokio::pin!(send);
                let resp = loop {
                tokio::select! {
                    biased;
                    _ = &mut watch => return Err(anyhow!(CANCELLED_MSG)),
                    r = &mut send => break r,
                    _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {
                        let ms = start.elapsed().as_millis() as u64;
                        // Què fa Ollama DE VERITAT: si el model ja és a la memòria,
                        // el que triga és la lectura del context, no la càrrega.
                        let phase = match self.model_is_loaded(&model_label).await {
                            Some(true) => WaitPhase::Prefill,
                            Some(false) => WaitPhase::Loading,
                            None => WaitPhase::Unknown,
                        };
                        if let Some(note) = wc.take(phase, ms) {
                            ctl.process(app, "ollama", &model_label, "thinking", &note, ms);
                        }
                    }
                }
            };
            let resp = match resp {
                Ok(r) => r,
                Err(e) => {
                    ctl.process(
                        app,
                        "ollama",
                        &model_label,
                        "error",
                        &e.to_string(),
                        start.elapsed().as_millis() as u64,
                    );
                    return Err(e);
                }
            };
            resp // el futur «send» i el seu préstec de «req» moren aquí
            };
            if resp.status().as_u16() != 404 || starter_tried || fallback_used.is_some() {
                break resp;
            }
            starter_tried = true;
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let instalats = self.list_models().await.unwrap_or_default();
            let ja_hi_es = instalats.iter().any(|m| m.name == model_label);
            if ja_hi_es {
                // 404 amb un model que SÍ és a la llista (volum expulsat,
                // model esborrat mentre volava…): error normal explicat.
                let note = ollama_error_note(&model_label, status, &body);
                ctl.process(app, "ollama", &model_label, "error", &note, start.elapsed().as_millis() as u64);
                return Err(anyhow!("Ollama error {}: {}", status, body));
            }
            if !instalats.is_empty() {
                // EL MODEL DEMANAT NO ÉS però es pot BAIXAR: descarrega'l
                // («descarregant model» amb progrés en viu, un sol cop) i
                // torna a provar el MATEIX xat amb ell — l'usuari no fa res.
                ctl.process(
                    app,
                    "ollama",
                    &model_label,
                    "thinking",
                    &format!(
                        "El model «{}» no està instal·lat a Ollama: DESCARREGANT MODEL \
                         (progrés al gestor de models; pot trigar uns minuts)…",
                        model_label
                    ),
                    start.elapsed().as_millis() as u64,
                );
                match self.pull_model(Some(app), &model_label).await {
                    Ok(_) => continue, // ja baixat: reintent amb el mateix model
                    Err(e) => {
                        // No s'ha pogut baixar (nom inventat, xarxa, disc ple):
                        // RESERVA amb un model JA instal·lat per no tallar el xat.
                        let Some(fb) = Self::pick_fallback_model(&instalats) else {
                            let note = ollama_error_note(&model_label, status, &body);
                            ctl.process(app, "ollama", &model_label, "error", &note, start.elapsed().as_millis() as u64);
                            return Err(anyhow!("Ollama error {}: {} (i la descàrrega ha fallat: {})", status, body, e));
                        };
                        ctl.process(
                            app,
                            "ollama",
                            &model_label,
                            "thinking",
                            &format!(
                                "No he pogut descarregar «{}» ({}). Faig servir «{}», \
                                 que ja tens baixat — si vols el de primer, baixa'l \
                                 des del gestor de models (menú IA).",
                                model_label, e, fb
                            ),
                            start.elapsed().as_millis() as u64,
                        );
                        fallback_used = Some(fb.clone());
                        req.model = fb;
                        continue;
                    }
                }
            }
            ctl.process(
                app,
                "ollama",
                &model_label,
                "thinking",
                "Ollama encara no té cap model: baixo el model inicial (només passa el primer cop; pot trigar uns minuts)…",
                start.elapsed().as_millis() as u64,
            );
            match self.pull_starter_model(app).await {
                Some(nou) => {
                    // Tornem a provar el MATEIX xat amb el model acabat de baixar.
                    req.model = nou;
                }
                None => {
                    return Err(anyhow!(
                        "No he pogut baixar el model inicial d'Ollama (pot ser manca de xarxa \
                         o d'espai al disc). Torna-ho a provar més tard o baixa'n un des del \
                         gestor de models."
                    ));
                }
            }
        };
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            let note = ollama_error_note(&model_label, status, &text);
            ctl.process(
                app,
                "ollama",
                &model_label,
                "error",
                &note,
                start.elapsed().as_millis() as u64,
            );
            return Err(anyhow!("Ollama error {}: {}", status, text));
        }
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut full = String::new();
        // Comptador de comprovació del bucle de repetició: l'escombrada
        // Carter ja és O(n) per crida, però cridarla a CADA tros feia que el
        // cost total creiés amb el quadrat de la resposta. Ara es comprova
        // quan ha crescut prou text (8 KB) o quan el tros és gran.
        let mut last_repeat_check: usize = 0;
        let mut thinking_acc = String::new();
        // Comptabilització de tokens del torn (Ollama la publica al final).
        let mut usage_input: u64 = 0;
        let mut usage_output: u64 = 0;
        // Temps sense rebre cap fragment. Un model que no pot cabre en la RAM
        // no dona senyals: amb 3 minuts de silenci es talla i HO EXPLICA, en
        // lloc de penjar la interfície o mostrar un «ERROR» buit.
        let idle = std::time::Duration::from_secs(180);
        let mut tick = tokio::time::interval_at(
            tokio::time::Instant::now() + std::time::Duration::from_secs(15),
            std::time::Duration::from_secs(15),
        );
        // Reparteix el «content» entre pensament i resposta quan el model no els
        // separa en canals propis (amb «think» actiu ja vénen separats).
        let mut splitter = ThoughtSplitter::new();
        'stream: loop {
            // Aturada immediata entre chunks: es mira la bandera del PROPI xat
            // i el botó «Atura» respon tot seguit, sense esperar dades noves.
            if ctl.is_cancelled() {
                return Err(anyhow!(CANCELLED_MSG));
            }
            let next = tokio::select! {
                biased;
                _ = notify.notified() => {
                    return Err(anyhow!(CANCELLED_MSG));
                }
                // Sense cap fragment durant 3 minuts el model no avança: es
                // talla i s'explica PER QUÈ (normalment, la memòria).
                _ = tokio::time::sleep(idle) => {
                    let ms = start.elapsed().as_millis() as u64;
                    let msg = format!(
                        "Ollama porta {} s sense emetre ni un sol fragment: la generació no avança. \
                         Sol ser perquè el model {} no cap a la memòria d'aquest ordinador o perquè \
                         el context és enorme. Prova un model més lleuger (p. ex. qwen2.5-coder:1.5b) \
                         o desactiva «Inclou el codi del projecte».",
                        ms / 1000, model_label
                    );
                    if full.trim().is_empty() && thinking_acc.trim().is_empty() {
                        ctl.process(app, "ollama", &model_label, "error", &msg, ms);
                        return Err(anyhow!(msg));
                    }
                    // Ja hi ha alguna cosa escrita: es conserva i es tanca el torn.
                    ctl.process(app, "ollama", &model_label, "thinking", &format!("\n{}\n", msg), ms);
                    break;
                }
                // Cor del procés: mentre no arriba res, el visor no està mut.
                _ = tick.tick() => {
                    let ms = start.elapsed().as_millis() as u64;
                    if ms >= 15_000 {
                        // Si ja ha caigut algun fragment, el model està generant;
                        // si no, mirem si el té carregat o si encara carrega.
                        let phase = if !full.is_empty() {
                            WaitPhase::Generating
                        } else {
                            match self.model_is_loaded(&model_label).await {
                                Some(true) => WaitPhase::Prefill,
                                Some(false) => WaitPhase::Loading,
                                None => WaitPhase::Unknown,
                            }
                        };
                        if let Some(note) = wc.take(phase, ms) {
                            ctl.process(app, "ollama", &model_label, "thinking", &note, ms);
                        }
                    }
                    continue;
                }
                item = stream.next() => item,
            };
            let chunk = match next {
                Some(Ok(c)) => c,
                Some(Err(e)) => {
                    // Connexió tallada a mitja resposta (manca de memòria per a
                    // un context gran o temps màxim exhaurit): si ja tenim text,
                    // el conservem i tanquem el torn — millor una resposta
                    // parcial que cap. Si no en tenim, propaguem l'error perquè
                    // `chat_stream` comprimísca el context i ho torne a provar.
                    if full.trim().is_empty() {
                        let msg = e.to_string();
                        ctl.process(
                            app,
                            "ollama",
                            &model_label,
                            "error",
                            &msg,
                            start.elapsed().as_millis() as u64,
                        );
                        return Err(anyhow!(msg));
                    }
                    break;
                }
                None => break,
            };
            buf.extend_from_slice(&chunk);
            // NDJSON: cada línia completa és un objecte que Ollama retorna.
            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let mut line: Vec<u8> = buf.drain(..=pos).collect();
                if line.last() == Some(&b'\n') {
                    line.pop();
                }
                if line.is_empty() {
                    continue;
                }
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&line) {
                    // Comptabilització de tokens: Ollama envia el compte al
                    // final del flux («prompt_eval_count» = tokens llegits,
                    // «eval_count» = tokens generats).
                    if let Some(p) = v.get("prompt_eval_count").and_then(|x| x.as_u64()) {
                        usage_input = p;
                    }
                    if let Some(c) = v.get("eval_count").and_then(|x| x.as_u64()) {
                        usage_output = c;
                    }
                    let msg = match v.get("message") {
                        Some(m) => m,
                        None => continue,
                    };
                    // 1) Raonament en canal PROPI: «thinking» (Ollama amb «think»
                    //    actiu) o «reasoning»/«reasoning_content» en servidors
                    //    compatibles amb OpenAI.
                    let own = chunk_thinking(msg);
                    if !own.is_empty() {
                        thinking_acc.push_str(&own);
                        let ms = start.elapsed().as_millis() as u64;
                        ctl.process(app, "ollama", &model_label, "thinking", &own, ms);
                    }
                    // 2) Raonament MESCLAT amb la resposta: hi ha models (MiMo-RL
                    //    i semblants) que NO separen el pensament i l'escriuen
                    //    primer, amb etiquetes. El repartidor els separa EN DIRECTE
                    //    perquè el visor mai quede reduït a un «està treballant».
                    let raw = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
                    let (thought, answer) = if raw.is_empty() {
                        (String::new(), String::new())
                    } else if think.is_some() || !own.is_empty() {
                        (String::new(), raw.to_string())
                    } else {
                        splitter.push(raw)
                    };
                    if !thought.is_empty() {
                        thinking_acc.push_str(&thought);
                        let ms = start.elapsed().as_millis() as u64;
                        ctl.process(app, "ollama", &model_label, "thinking", &thought, ms);
                    }
                    if !answer.is_empty() {
                        full.push_str(&answer);
                        let ms = start.elapsed().as_millis() as u64;
                        ctl.chunk(app, &answer);
                        ctl.process(app, "ollama", &model_label, "streaming", &answer, ms);
                        // Bucle desbocat (model petit repetint la mateixa frase).
                        let grown = full.len() - last_repeat_check;
                        if grown >= 8 * 1024 || answer.len() >= 8 * 1024 {
                            last_repeat_check = full.len();
                            if runaway_repetition(&full) {
                                ctl.process(
                                    app,
                                    "ollama",
                                    &model_label,
                                    "thinking",
                                    "\nS'ha detectat un bucle de repetició: la generació s'atura per a no bloquejar-se.\n",
                                    ms,
                                );
                                break 'stream;
                            }
                        }
                    }
                }
            }
        }
        // El torn pot acabar amb el repartidor encara indecis (resposta de
        // poquíssimes paraules): el que retenia és la resposta i s'emet ara.
        let (thought, answer) = splitter.flush();
        if !thought.is_empty() {
            thinking_acc.push_str(&thought);
            let ms = start.elapsed().as_millis() as u64;
            ctl.process(app, "ollama", &model_label, "thinking", &thought, ms);
        }
        if !answer.is_empty() {
            full.push_str(&answer);
            let ms = start.elapsed().as_millis() as u64;
            ctl.chunk(app, &answer);
            ctl.process(app, "ollama", &model_label, "streaming", &answer, ms);
        }
        if !thinking_acc.trim().is_empty() {
            ctl.set_thinking(Some(thinking_acc)).await;
        }
        ctl.process(
            app,
            "ollama",
            &model_label,
            "done",
            "",
            start.elapsed().as_millis() as u64,
        );
        // Comptabilització de tokens (Ollama). Si el model no l'ha donada,
        // s'estima pel text (≈4 caràcters per token) perquè la interfície
        // sempre pugui dir quant ha ocupat el context.
        if usage_input > 0 || usage_output > 0 {
            emit_usage(
                app,
                &ctl.id,
                "ollama",
                &model_label,
                usage_input,
                usage_output,
                ctx_window,
            );
        } else {
            let est_in = ((prompt_chars + sys_chars) / 4) as u64;
            let est_out = (full.chars().count() / 4) as u64;
            if est_in > 0 || est_out > 0 {
                emit_usage(
                    app,
                    &ctl.id,
                    "ollama",
                    &model_label,
                    est_in,
                    est_out,
                    ctx_window,
                );
            }
        }
        Ok(full)
    }

    /// Nucli del xat sense streaming, DINS D'UN XAT concret: usa el seu
    /// proveïdor i el seu model, inclou la conversa precedent (`opts.history`)
    /// i enrotlla la petició amb la bandera d'aturada d'aquell xat, perquè els
    /// altres xats en paral·lel no s'aturen ni es mesuren entre ells.
    async fn chat_inner(
        &self,
        prompt: &str,
        system: Option<&str>,
        ctl: &Arc<SessionCtl>,
        provider: &str,
        opts: &ChatOpts,
    ) -> Result<String> {
        if provider != "ollama" {
            if let Some(remote) = &self.remote {
                // prepare() retorna dades propietàries; el guard allibera abans de l'await
                let (prov, client) = remote.lock().unwrap().prepare(provider)?;
                let conv = conversation(&opts.history, prompt);
                let system = system.map(|s| s.to_string());
                let (content, thinking) = race_cancel(ctl, async move {
                    api::chat(&client, &prov, &conv, system.as_deref()).await
                })
                .await?;
                ctl.set_thinking(thinking).await;
                return Ok(content);
            }
        }
        let model = self.model_for(ctl).await;
        // Respatller: si el model triat/per defecte no és instal·lat, agafa'n
        // un de disponible perquè l'agent local *sempre* puga respondre.
        let model = self.resolve_usable_model(model).await;
        // Conversa del xat: primer els torns ja digitats (fork o conversa
        // llarga) i després el prompt actual.
        let mut messages: Vec<ChatMessage> = conversation(&opts.history, prompt)
            .into_iter()
            .map(|(role, content)| ChatMessage {
                role,
                content,
                thinking: None,
                images: None,
            })
            .collect();
        attach_images(&mut messages, &opts.images);
        // El context que veu el model és tota la conversa, no només l'últim
        // missatge: la finestra de context s'ha de dimensionar per això.
        let prompt_chars = messages.iter().map(|m| m.content.chars().count()).sum();
        // Mateix criteri que el camí streaming: només demanem el raonament
        // separat als models que declaren l'habilitat «thinking»; els altres
        // rebutjarien la bandera amb un 400, així que no s'envia res.
        let model_label = model.clone();
        let think = if self.supports_thinking(&model_label).await {
            Some(true)
        } else {
            None
        };
        let chat_url = self.url("/api/chat");
        let keep_alive = self.keep_alive.read().await.clone();
        let system_owned = system.map(|s| s.to_string());
        let options = Self::ctx_options(prompt_chars, system.map_or(0, |s| s.len()));
        let parsed: ChatResponse = race_cancel(ctl, async move {
            let mut req = ChatRequest {
                model: model_label.clone(),
                messages,
                stream: false,
                system: system_owned,
                think,
                keep_alive,
                options,
            };
            // Podem instal·lar Ollama en segon pla? Només si l'usuari NO
            // té cap IA en línia configurada.
            let auto_install = self.first_working_remote().is_none();
            let mut resp = Self::send_chat(&self.http, &chat_url, &req, auto_install).await?;
            // USUARI NOU SENSE CAP MODEL: el primer enviament a un Ollama
            // acabat d'instal·lar (o JA en marxa però buit) retorna 404.
            // Mateixa recuperació que el camí streaming: si Ollama no té cap
            // model, baixem l'inicial i REINTENTEM una vegada — l'usuari no
            // ha de fer res. Sense això, experts/equip/agent fallaven amb 404
            // mentre el xat normal funcionava.
            if resp.status().as_u16() == 404 {
                let body = resp.text().await.unwrap_or_default();
                let instalats = self.list_models().await.unwrap_or_default();
                let ja_hi_es = instalats.iter().any(|m| m.name == model_label);
                if ja_hi_es {
                    // El model hi és però 404 igualment (esborrat mentre
                    // volava…): error normal.
                    return Err(anyhow!("Ollama error 404: {}", body));
                }
                if !instalats.is_empty() {
                    // El model DEMANAT no és: DESCARREGA'L (silenciós, sense
                    // AppHandle) i reintent una vegada amb ell.
                    if self.pull_model(None, &model_label).await.is_ok() {
                        resp = Self::send_chat(&self.http, &chat_url, &req, auto_install).await?;
                    } else if let Some(fb) = Self::pick_fallback_model(&instalats) {
                        // La descàrrega ha fallat: reserva amb un JA instal·lat.
                        req.model = fb;
                        resp = Self::send_chat(&self.http, &chat_url, &req, auto_install).await?;
                    } else {
                        // Res instal·lat útil: baixa l'inicial i reintent.
                        let nom = Self::starter_model_name();
                        self.pull_model(None, &nom).await?;
                        req.model = nom;
                        resp = Self::send_chat(&self.http, &chat_url, &req, auto_install).await?;
                    }
                } else {
                    // Ollama SENSE CAP model: baixem l'inicial i reintentem.
                    let nom = Self::starter_model_name();
                    self.pull_model(None, &nom).await?;
                    req.model = nom;
                    resp = Self::send_chat(&self.http, &chat_url, &req, auto_install).await?;
                }
            }
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(anyhow!("Ollama error {}: {}", status, text));
            }
            let parsed: ChatResponse = resp.json().await?;
            Ok(parsed)
        })
        .await?;
        // Sense canal propi de raonament, potser el model l'ha escrit dins del
        // text: se'n separa una part per a poder-lo mostrar al xat i al visor.
        let mut content = parsed.message.content;
        let mut thought = parsed.message.thinking.filter(|t| !t.trim().is_empty());
        if thought.is_none() && think.is_none() {
            let (t, a) = split_inline_reasoning(&content);
            if !t.trim().is_empty() {
                thought = Some(t);
                content = a;
            }
        }
        ctl.set_thinking(thought).await;
        Ok(content)
    }

    /// POST /api/chat separat per poder enrotllar-lo amb la cancel·lació.
    async fn send_chat(
        http: &reqwest::Client,
        url: &str,
        req: &ChatRequest,
        auto_install: bool,
    ) -> Result<reqwest::Response> {
        match http.post(url).json(req).send().await {
            Ok(resp) => Ok(resp),
            Err(e) => {
                // No propaguem l'error opac de reqwest: expliquem la CAUSA real
                // (temps exhaurit, Ollama aturat, port satur…) amb què fer.
                // L'explicació pot ARENCAR Ollama: això es fa en un fil a
                // part (spawn_blocking) perquè el runtime async no quedi
                // congelat fins a ~40 s.
                let url_owned = url.to_string();
                let explained = explain_send_failure(&e, &url_owned, auto_install).await;
                Err(anyhow!("Ollama no respon — {}", explained))
            }
        }
    }

    /// Demana aturar la generació d'un xat (botó «Atura» de la interfície).
    /// Amb `session` només atura aquell xat; sense, atura TODS els xats que
    /// estiguen generant ara mateix.
    pub fn request_stop(&self, session: Option<&str>) {
        let ctls: Vec<Arc<SessionCtl>> = {
            let guard = self.sessions.lock().unwrap();
            match session {
                Some(id) => guard.get(id).cloned().into_iter().collect(),
                None => guard.values().cloned().collect(),
            }
        };
        for ctl in ctls {
            ctl.stop();
        }
    }

    /// Defineix el mode eco de segon pla (menys recursos a Ollama).
    pub fn set_background(&self, on: bool) {
        self.background
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_background(&self) -> bool {
        self.background.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Dimensiona la finestra de context d'Ollama («num_ctx») segons la
    /// llargària del prompt. SENSE açò, Ollama usa la finestra xiqueta per
    /// defecte i TALLA el context del projecte abans que el model el veja.
    /// Creix fins a 16384 tokens: amb un sostre baix (4096) la resposta llarga
    /// (un fitxer sencer, per exemple) es tallava a mitges perquè el prompt
    /// menjava tota la finestra. `num_predict: -1` elimina el límit de tokens
    /// eixents: la IA acaba la resposta sencera, no mig JSON.
    /// En mode segon pla (eco) es redueix a 2048 fixos i es limiten els fils.
    fn ctx_options(prompt_chars: usize, system_chars: usize) -> Option<serde_json::Value> {
        let approx_tokens = (prompt_chars + system_chars) / 3 + 512;
        let mut n = 2048usize;
        while n < 16384 && n < approx_tokens {
            n *= 2;
        }
        // En equips amb poca RAM, limitem la finestra de context: el KV cache
        // ocupa memòria proporcional a «num_ctx» i, en un Mac de 8 GB, un
        // context de 16k pot provocar swap i frenar-ho tot.
        if let Some(ram) = Self::total_memory_bytes() {
            if ram <= 6_500_000_000 {
                n = n.min(4096);
            } else if ram <= 10_500_000_000 {
                n = n.min(8192);
            }
        }
        Some(serde_json::json!({ "num_ctx": n, "num_predict": -1 }))
    }

    /// Opcions d'Ollama en mode eco (tasca degradada a segon pla): menys fils,
    /// context reduït i menys tokens per torn, per no bloquejar l'equip.
    fn ctx_options_bg(prompt_chars: usize, system_chars: usize) -> Option<serde_json::Value> {
        let _ = (prompt_chars, system_chars);
        Some(serde_json::json!({
            "num_ctx": 2048,
            "num_thread": 2,
            "num_predict": 512,
        }))
    }

    /// Llegeix i recorda les HABILITATS que un model d'Ollama declara
    /// («thinking», «vision», «tools»…). Es consulta amb «/api/show», que és
    /// instantani i no carrega el model. Si Ollama no respon, la llista és buida
    /// i NoOrbit no demanarà raonament separat (millor cap pensament que un 400).
    pub async fn model_capabilities(&self, model: &str) -> Vec<String> {
        if let Some(c) = self.caps_cache.lock().unwrap().get(model) {
            return c.clone();
        }
        let caps = match self
            .http
            .post(self.url("/api/show"))
            .timeout(std::time::Duration::from_secs(15))
            .json(&serde_json::json!({ "model": model }))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => resp
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| {
                    v.get("capabilities")
                        .and_then(|c| c.as_array())
                        .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        self.caps_cache
            .lock()
            .unwrap()
            .insert(model.to_string(), caps.clone());
        caps
    }

    /// Diu si el model pot emetre raonament SEPARAT de la resposta. Alguns
    /// (qwen3, deepseek-r1…) sí; d'altres que també raonen (MiMo-RL) ho fan
    /// dins del mateix text i Ollama rebutja la bandera «think» amb un 400.
    async fn supports_thinking(&self, model: &str) -> bool {
        self.model_capabilities(model)
            .await
            .iter()
            .any(|c| c == "thinking")
    }

    /// Retorna el model si és instal·lat; si no, el primer model disponible.
    /// Evita l'error 404 d'«Ollama no fa res» quan el per defecte no és ací.
    async fn resolve_usable_model(&self, want: String) -> String {
        let models = match self.list_models().await {
            Ok(m) if !m.is_empty() => m,
            _ => return want,
        };
        // Coincidència exacta o per prefixi de família (p. ex. "qwen3").
        let ok = models
            .iter()
            .any(|m| m.name == want || m.name.starts_with(&want) || want.starts_with(&m.name));
        if ok {
            want
        } else {
            models[0].name.clone()
        }
    }

    pub async fn set_active_provider(&self, id: &str) {
        *self.active_provider.write().await = id.to_string();
    }

    /// Defineix el model d'Ollama actiu per a tots els xats (None = per defecte).
    pub async fn set_selected_model(&self, name: Option<String>) {
        *self.selected_model.write().await = name.filter(|n| !n.trim().is_empty());
    }

    pub async fn selected_model(&self) -> Option<String> {
        self.selected_model.read().await.clone()
    }

    /// Model real que s'usa: el triat per l'usuari o, si no n'hi ha, el per defecte.
    pub async fn effective_model(&self) -> String {
        self.selected_model
            .read()
            .await
            .clone()
            .unwrap_or_else(|| self.default_model.clone())
    }

    pub async fn active_provider(&self) -> String {
        self.active_provider.read().await.clone()
    }

    /// Últim pensament (raonament) emès per UN XAT. La UI el demana després de
    /// cada generació per mostrar el procés de pensament del model.
    pub async fn last_thinking(&self, session: &str) -> Option<String> {
        self.ctl(session).thinking().await
    }

    pub async fn list_providers(&self) -> Vec<ProviderInfo> {
        let active = self.active_provider.read().await.clone();
        let ollama_available = self.is_running().await;
        let mut out = vec![
            ProviderInfo {
                id: "ollama".into(),
                name: "Ollama (local · text/codi)".into(),
                kind: "text".into(),
                active: active == "ollama",
                available: ollama_available,
            },
            ProviderInfo {
                id: "comfyui".into(),
                name: "ComfyUI (imatge)".into(),
                kind: "image".into(),
                active: active == "comfyui",
                available: false, // gestionat pel mòdul agent
            },
        ];
        // Afegeix els proveïdors d'IA en línia registrats (tokens).
        if let Some(remote) = &self.remote {
            let infos: Vec<ProviderInfo> = remote
                .lock()
                .unwrap()
                .providers
                .iter()
                .map(|p| ProviderInfo {
                    id: p.id.clone(),
                    name: format!("{} · {}", p.name, p.model),
                    kind: "text".into(),
                    active: active == p.id,
                    // Els serveis LOCALS (llama.cpp, LM Studio…) no demanen
                    // clau: «has_token» ho sap (auth = None).
                    available: p.enabled && p.has_token(),
                })
                .collect();
            out.extend(infos);
        }
        out
    }

    /// Consulta altres IAs en línia (proveïdors amb token i activats: OpenAI,
    /// Claude, Ollama Cloud…). Retorna (nom · model, resposta) per a cada una
    /// que hagi funcionat. Les fallades individuals s'ignorenen.
    pub async fn ask_peers(&self, prompt: &str, max: usize) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let Some(remote) = &self.remote else { return out };
        // Fotografia els ids habilitats (allibera el guard abans dels awaits).
        let ids: Vec<(String, String, String)> = {
            let guard = remote.lock().unwrap();
            guard
                .providers
                .iter()
                .filter(|p| p.enabled && !p.token.trim().is_empty())
                .take(max)
                .map(|p| (p.id.clone(), p.name.clone(), p.model.clone()))
                .collect()
        };
        let sys = "Respon en català, de forma concisa i directa.";
        for (id, name, model) in ids {
            let prepared = remote.lock().unwrap().prepare(&id);
            let (provider, client) = match prepared {
                Ok(v) => v,
                Err(_) => continue,
            };
            let one_turn = [("user".to_string(), prompt.to_string())];
            if let Ok((content, _)) =
                api::chat(&client, &provider, &one_turn, Some(sys)).await
            {
                out.push((format!("{} · {}", name, model), content));
            }
        }
        out
    }

    /// RAM total del sistema (bytes), si el podem determinar.
    fn total_memory_bytes() -> Option<u64> {
        #[cfg(target_os = "macos")]
        {
            let out = std::process::Command::new("sysctl")
                .args(["-n", "hw.memsize"])
                .output()
                .ok()?;
            String::from_utf8_lossy(&out.stdout).trim().parse().ok()
        }
        #[cfg(target_os = "linux")]
        {
            let f = std::fs::read_to_string("/proc/meminfo").ok()?;
            let line = f.lines().find(|l| l.starts_with("MemTotal:"))?;
            let kb: u64 = line
                .trim_end()
                .split_whitespace()
                .nth(1)?
                .parse()
                .ok()?;
            Some(kb * 1024)
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            None
        }
    }

    /// El millor model LOCAL instal·lat que càpiga còmodament a la RAM
    /// (màxim 60 % del total): prou capacitat sense saturar la màquina.
    /// S'usa per lligar els especialistes autònums a la capacitat real.
    pub async fn best_fit_model(&self) -> Option<String> {
        let ram = Self::total_memory_bytes()?;
        let budget = (ram as f64 * 0.6) as u64;
        let models = self.list_models().await.ok()?;
        models
            .into_iter()
            .filter(|m| m.size > 0 && m.size <= budget)
            .max_by_key(|m| m.size)
            .map(|m| m.name)
    }
}

#[cfg(test)]
mod proves {
    use super::runaway_repetition;

    /// Un model que repeteix el mateix bloc 14 vegades (cas real amb un
    /// model d'1,5B) HA de detectar-se com a bucle, encara que la repetició
    /// no sigui a la cua del text.
    #[test]
    fn detecta_repeticio_al_mig_del_text() {
        let bloc = "```python\nimport unittest\nfrom my_module import MyModule\n\nclass TestMyModule(unittest.TestCase):\n    def test_x(self):\n        pass\n```\n";
        let text = format!(
            "Introducció del model.\n{}{}\nI aquí continua el text final que no es repeteix.",
            bloc.repeat(14),
            "Fi de la resposta amb una frase llarga i diferent del bloc repetit."
        );
        assert!(
            runaway_repetition(&text),
            "la repetició al mig del text hauria de detectar-se"
        );
    }

    /// Un text normal NO ha de ser considerat bucle.
    #[test]
    fn no_talla_un_text_normal() {
        let text = "Aquí tens el fitxer style.css:\n\
body { color: red; }\n\
I el fitxer functions.php:\n\
<?php\nfunction add_action() { return true; }\n\
Final del text.";
        assert!(!runaway_repetition(text));
    }
}

#[cfg(test)]
mod proves_cerca {
    use super::reasoning::find_ignore_ascii_case;

    /// La cerca ha de conservar els índexs del text ORIGINAL: amb `to_lowercase`
    /// un caràcter com «İ» canvia la longitud en bytes i els índexs trobats
    /// no tallaven correctament (pànic de char boundary).
    #[test]
    fn la_cerca_conserva_els_indexos() {
        let text = "<think>Hola, món!</think>Resposta";
        let i = find_ignore_ascii_case(text, "</think>").expect("ha de trobar la etiqueta");
        assert_eq!(&text[i..i + "</think>".len()], "</think>");
        // Majúscules/minúscules indifferents.
        let i2 = find_ignore_ascii_case("<think>", "<think>").expect("ha de trobar-ho");
        assert_eq!(i2, 0);
        // Caràcter multibyte abans de l'etiqueta: l'índex ha de ser vàlid.
        let text2 = "İstanbul <think>fi</think>";
        let i3 = find_ignore_ascii_case(text2, "</think>").expect("ha de trobar-ho");
        assert!(text2.is_char_boundary(i3));
    }
}
