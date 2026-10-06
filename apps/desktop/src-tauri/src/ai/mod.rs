//! Gestió de proveïdors d'IA local (Ollama) per a NoOrbit.

use crate::api::{self, ApiManager};
use crate::config::AppConfig;
use crate::external_orchestrator::OrchestratorAgent;
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

/// Detecta un **bucle desbocat**: un model petit (o sense suficient RAM) pot
/// entrar a repetir la mateixa frase una i una altra sense acabar mai —la UI
/// queda a «Treballant…» indefinidament. Comprova si el tram final del text
/// generat és la repetició consecutiva d'un mateix bloc. Si és així, el
/// streaming es talla i es conserva el text obtingut fins al moment.
pub fn runaway_repetition(full: &str) -> bool {
    const MIN: usize = 24; // bloc mínim que considerem un «cicle» possible
    const MAX: usize = 240; // bloc màxim
    const REPEATS: usize = 4; // repeticions seguides que disparen el tall

    let b = full.as_bytes();
    if b.len() < MIN * REPEATS {
        return false;
    }
    // Provem mides de cicle de gran a petit; si els últims `len` bytes es
    // repeteixen `REPEATS` vegades seguides cap arrere, és un bucle.
    let max = MAX.min(b.len() / REPEATS);
    let mut len = max;
    while len >= MIN {
        if b.len() >= len * REPEATS {
            let tail = &b[b.len() - len..];
            let mut reps = 1usize;
            let mut i = b.len() - len;
            while i >= len && &b[i - len..i] == tail {
                reps += 1;
                if reps >= REPEATS {
                    return true;
                }
                i -= len;
            }
        }
        len = len.saturating_sub(1);
    }
    false
}

/// Xat per defecte: el que fan servir l'agent, les tasques autònomes i
/// qualsevol crida que no s'haja iniciat dins d'un xat múltiple.
pub const DEFAULT_SESSION: &str = "main";

/// Extrau el RAONAMENT d'un fragment NDJSON d'Ollama. El camp oficial és
/// `message.thinking`, però els servidors compatibles amb OpenAI (Ollama Cloud,
/// DeepSeek, OpenRouter…) l'anomenen `reasoning` o `reasoning_content`: els
/// tres es reconeixen perquè QUALSEVOL model puga mostrar el seu pensament.
fn chunk_thinking(msg: &serde_json::Value) -> String {
    ["thinking", "reasoning", "reasoning_content"]
        .iter()
        .filter_map(|k| msg.get(*k).and_then(|v| v.as_str()))
        .find(|s| !s.is_empty())
        .unwrap_or_default()
        .to_string()
}

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
}

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
        }
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

/// Model disponible al núvol d'Ollama (ollama.com), per navegar-lo i baixar-lo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudModel {
    pub name: String,
    pub description: String,
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
    loop {
        tokio::select! {
            r = &mut fut => return r,
            _ = &mut watch => {
                // La petició pendent es descarta en deixar-la caure.
                drop(fut);
                return Err(anyhow!(CANCELLED_MSG));
            }
        }
    }
}

/// Etiquetes que alguns models empren per a escriure el raonament DINS del seu
/// propi text (formats antics de DeepSeek, models a què es demana «pensament en
/// veu alta»…). La primera component OBRI el pensament i la segona el TANCA.
/// S'escriuen per parts amb `concat!` perquè aquest fitxer no continga mai la
/// marca literal sencera (confongria cerques i resums).
const REASON_TAGS: [(&str, &str); 2] = [
    (concat!("<", "think", ">"), concat!("<", "/think", ">")),
    (
        concat!("<", "reasoning", ">"),
        concat!("<", "/reasoning", ">"),
    ),
];

/// Paraules que obren/tanquen el pensament quan el model no usa etiquetes.
const REASON_OPEN_WORDS: [&str; 5] = ["reason:", "thinking:", "think:", "pensament:", "pensa:"];
const REASON_CLOSE_WORDS: [&str; 3] = ["answer:", "respon:", "respuesta:"];

/// Separa el raonament que un model escriu MESCLAT amb la seua resposta.
/// Retorna (pensament, resposta); si no hi ha cap senyal, el text sencer és la
/// resposta i el pensament queda buit.
fn split_inline_reasoning(text: &str) -> (String, String) {
    let lower = text.to_lowercase();
    // 1) hi ha una ETIQUETA DE TANCAMENT? tot el que precedeix és pensament.
    let closes: Vec<&str> = REASON_TAGS
        .iter()
        .map(|(_, c)| *c)
        .chain(REASON_CLOSE_WORDS.iter().copied())
        .collect();
    let mut best: Option<(usize, usize)> = None; // (índex, longitud de l'etiqueta)
    for c in closes {
        if let Some(i) = lower.find(c) {
            if best.map(|(bi, _)| i < bi).unwrap_or(true) {
                best = Some((i, c.len()));
            }
        }
    }
    if let Some((i, len)) = best {
        let head = &text[..i];
        // Si l'etiqueta d'obertura era una paraula, el pensament comença després.
        let opens: Vec<&str> = REASON_TAGS
            .iter()
            .map(|(o, _)| *o)
            .chain(REASON_OPEN_WORDS.iter().copied())
            .collect();
        let mut thought = head.to_string();
        for o in opens {
            if let Some(s) = head.to_lowercase().find(o) {
                thought = head[s + o.len()..].to_string();
                break;
            }
        }
        let answer = text[i + len..].trim_start().to_string();
        return (thought.trim().to_string(), answer);
    }
    // 2) encara no hi ha tancament: si el fragment COMENÇA per una obertura, tot
    //    el que porta fins ara és raonament i el model no ha contestat.
    let t = text.trim_start();
    let low = t.to_lowercase();
    for (o, _) in REASON_TAGS {
        if low.starts_with(o) {
            return (t[o.len()..].to_string(), String::new());
        }
    }
    for p in REASON_OPEN_WORDS {
        if low.starts_with(p) {
            return (t[p.len()..].trim_start().to_string(), String::new());
        }
    }
    (String::new(), text.to_string())
}

/// Tallem un tram del text acumulat, respectant els límits de caràcter (els
/// fragments d'un model poden partir un accent per la meitat).
fn take(s: &str, a: usize, b: usize) -> String {
    let (a, b) = (a.min(s.len()), b.min(s.len()));
    let mut i = a;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    let mut j = b;
    while j < s.len() && !s.is_char_boundary(j) {
        j += 1;
    }
    s[i..j].to_string()
}

/// Cerca la TANCAMENT del pensament dins d'un text ja baixat a minúscules.
/// Retorna (índex, longitud de l'etiqueta).
fn reasoning_close_at(low: &str) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for c in REASON_TAGS
        .iter()
        .map(|(_, c)| *c)
        .chain(REASON_CLOSE_WORDS.iter().copied())
    {
        if let Some(i) = low.find(c) {
            if best.map(|(bi, _)| i < bi).unwrap_or(true) {
                best = Some((i, c.len()));
            }
        }
    }
    best
}

/// Separa EN DIRECTE el pensament de la resposta quan un model escriu les dues
/// coses pel mateix canal. No es pot mirar fragment per fragment: Ollama
/// parteix les etiquetes entre tokens (d'««» en feia «<th» + «ink»»), així que
/// es guarda el text brut acumulat i es busca sempre damunt del conjunt.
struct ThoughtSplitter {
    /// Tot el «content» rebut fins ara.
    raw: String,
    /// Bytes de `raw` ja repartits al visor.
    sent: usize,
    /// None: encara no sabem si aquest model pensa en veu alta.
    /// Some(true): estem dins del pensament. Some(false): ja respon.
    mode: Option<bool>,
}

impl ThoughtSplitter {
    fn new() -> Self {
        Self {
            raw: String::new(),
            sent: 0,
            mode: None,
        }
    }

    /// Afegeix un fragment i retorna (pensament nou, resposta nova) per emetre.
    fn push(&mut self, delta: &str) -> (String, String) {
        self.raw.push_str(delta);
        loop {
            // Encara sense decidir: es mira l'inici del text, no el fragment.
            if self.mode.is_none() {
                let skip = self.raw.len() - self.raw.trim_start().len();
                let head = take(&self.raw, skip, self.raw.len());
                let low = head.to_lowercase();
                // Quina etiqueta d'obertura (si n'hi ha) comença el text?
                let open_len = REASON_TAGS
                    .iter()
                    .map(|(o, _)| *o)
                    .chain(REASON_OPEN_WORDS)
                    .find(|tag| low.starts_with(tag))
                    .map(|t| t.len())
                    .unwrap_or(0);
                if let Some((i, len)) = reasoning_close_at(&low) {
                    // Pensament tancat abans de decidir-ne res: tot el que hi
                    // ha abans de l'etiqueta de tancament ho és.
                    let thought = take(&head, open_len, i).trim().to_string();
                    self.sent = skip + i + len;
                    self.mode = Some(false);
                    let answer = take(&self.raw, self.sent, self.raw.len());
                    self.sent = self.raw.len();
                    return (thought, answer);
                }
                if open_len > 0 {
                    self.mode = Some(true);
                    self.sent = skip + open_len;
                    continue;
                }
                // Massa text sense cap senyal: era la resposta normal i corrent.
                if head.chars().count() >= 24 || self.raw.len() >= 96 {
                    self.mode = Some(false);
                    self.sent = 0;
                    continue;
                }
                // Poquet encara: esperem el següent fragment abans de mostrar.
                return (String::new(), String::new());
            }
            if self.mode == Some(true) {
                // Dins del pensament: tot és raonament fins que aparega el
                // tancament (que pot haver arribat retallat en dues parts).
                let rest = take(&self.raw, self.sent, self.raw.len());
                if let Some((i, len)) = reasoning_close_at(&rest.to_lowercase()) {
                    let thought = take(&self.raw, self.sent, self.sent + i);
                    self.sent += i + len;
                    self.mode = Some(false);
                    let answer = take(&self.raw, self.sent, self.raw.len());
                    self.sent = self.raw.len();
                    return (thought, answer);
                }
                self.sent = self.raw.len();
                return (rest, String::new());
            }
            let answer = take(&self.raw, self.sent, self.raw.len());
            self.sent = self.raw.len();
            return (String::new(), answer);
        }
    }

    /// El que quedava per repartir quan el model ha acabat: si encara no
    /// s'havia decidit res, era una resposta curta i no cal perdre-la.
    fn flush(&mut self) -> (String, String) {
        let rest = take(&self.raw, self.sent, self.raw.len());
        self.sent = self.raw.len();
        match self.mode {
            Some(true) => (rest, String::new()),
            _ => (String::new(), rest),
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
fn explain_send_failure(e: &reqwest::Error, url: &str) -> String {
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
        // El servei no hi és: intentem arrencar-lo nosaltres (si està instal·lat).
        let tried = if ollama_installed() {
            start_ollama_server().is_ok()
        } else {
            false
        };
        format!(
            "OLLAMA NO ESTÀ EN MARXA: res no escolta a {}:{}. {}",
            host,
            port,
            if tried {
                "Ho he intentat arrencar jo: espera uns segons i repeteix el missatge."
            } else {
                "Obre l'app d'Ollama o executa `ollama serve` en un terminal i \
                 repeteix el missatge. Si no el tens instal·lat, instal·la'l o \
                 connecta un proveïdor remot amb token (p. ex. Venice AI)."
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

/// Extreu els models de la pàgina HTML d'ollama.com (biblioteca o cerca).
/// Ambdues pàgines comparteixen l'estructura: enllaç `href="/library/NOM"`
/// seguit d'un paràgraf de descripció.
fn parse_cloud_models(html: &str) -> Vec<CloudModel> {
    let mut out: Vec<CloudModel> = Vec::new();
    let mut seen: std::collections::HashSet<String> = Default::default();
    for chunk in html.split("href=\"/library/").skip(1) {
        // El nom acaba a la següent cometes que tanca l'atribut href.
        let name: String = chunk.chars().take_while(|&c| c != '"').collect();
        let name = name.trim().to_string();
        if name.is_empty() || name.contains('/') || !seen.insert(name.clone()) {
            continue;
        }
        let description = extract_desc(chunk);
        out.push(CloudModel { name, description });
        if out.len() >= 80 {
            break;
        }
    }
    out
}

/// Dins d'un fragment d'un model, retorna el text del primer <p> descriptiu.
fn extract_desc(chunk: &str) -> String {
    // El primer <p ...>text...</p> del fragment és la descripció.
    let Some(p_idx) = chunk.find("<p ") else {
        return String::new();
    };
    let after_open = &chunk[p_idx..];
    let Some(gt) = after_open.find('>') else {
        return String::new();
    };
    let body = &after_open[gt + 1..];
    let desc = match body.find("</p>") {
        Some(end) => &body[..end],
        None => body,
    };
    decode_entities(&strip_tags(desc)).trim().to_string()
}

/// Elimina qualsevol etiqueta HTML restant.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Decodifica les entitats HTML més habituals que apareixen a les descripcions.
fn decode_entities(s: &str) -> String {
    s.replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

/// Detecta si el binari/servei d'Ollama és al sistema (CLI a PATH o l'app).
pub fn ollama_installed() -> bool {
    if which::which("ollama").is_ok() {
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
        cmd.arg("ollama")
            .arg("serve")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if !models_dir.is_empty() {
            cmd.env("OLLAMA_MODELS", models_dir);
        }
        cmd.spawn()
            .map(|_| ())
            .map_err(|e| anyhow!("No s'ha pogut arrencar Ollama: {}", e))?;
    }
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("ollama");
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
        return Err(anyhow!("Ollama no està instal·lat"));
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

    /// descarrega un model amb streaming de progrés (event "ai://pull").
    pub async fn pull_model(&self, app: &tauri::AppHandle, name: &str) -> Result<String> {
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
                        let _ = app.emit("ai://pull", serde_json::json!({
                            "model": name, "message": msg
                        }));
                    }
                }
            }
        }
        Ok(format!("Model '{}' descarregat", name))
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
        [
            ("qwen3:8b", "Qwen3 8B — raonament i codi; pesos oberts (Apache 2.0), modificables/afinables (~5 GB)"),
            ("qwen2.5-coder:7b", "Codi — equilibri qualitat/velocitat"),
            ("qwen2.5-coder:3b", "Codi — lleuger, per a màquines modestes"),
            ("llama3.2:3b", "General — ràpid i multilingüe"),
            ("mistral:7b", "General — bo seguint instruccions"),
            ("codellama:13b", "Codi — més qualitat, més lent"),
            ("stable-diffusion-v2:latest", "Imàtges — requereix GPU potent"),
            // famílies «sense filtres»: públiques a Ollama, NOMÉS opcions
            // instal·lables (no es descarreguen sols). Cal prou RAM: un 7B
            // necessita ~5 GB, un 8x7b ~24 GB (inviable en 8 GB).
            ("dolphin-mistral:7b", "Sense filtres — codi i raonament (7B, ~5 GB)"),
            ("dolphin-mixtral:8x7b", "Sense filtres — MoE 8x7b, codi/raonament (~24 GB)"),
            ("goekdenizguelmez/JOSIEFIED-Qwen3", "Sense filtres — Qwen3-8B abliterated, codi i raonament (~5 GB)"),
            ("richardyoung/qwen3-8b-abliterated", "Sense filtres — Qwen3-8B abliterated per modificar i ús lliure (~5 GB)"),
            // Xiaomi MiMo: raonament fort en matemàtiques i codi, amb
            // «thinking» visible que NoOrbit mostra al xat de l'agent.
            ("alibayram/mimo-7b-rl", "Xiaomi MiMo-7B-RL — raonament (mat/codi), thinking visible (~5 GB)"),
            ("maternion/mimo-v2.6", "Xiaomi MiMo-V2.6 — agent 9B, crides d'eines i codi (~6 GB)"),
            // OpenAI de pesos oberts (únicos GPT descarregables localment):
            // MoE molt eficients; el 120b demana ~60 GB de RAM/VRAM.
            ("gpt-oss:20b", "OpenAI de pesos oberts — MoE 20B, qualitat/velocitat (~13 GB)"),
            ("gpt-oss:120b", "OpenAI de pesos oberts — MoE 120B, màxima qualitat local (~61 GB)"),
        ]
        .iter()
        .map(|(name, desc)| serde_json::json!({ "name": name, "description": desc }))
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
        let url = format!(
            "https://huggingface.co/api/models?search={}&filter=gguf&sort=downloads&direction=-1&limit=40",
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
                        Some(CloudModel { name, description })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(models)
    }

    /// Retorna el control d'un xat concret, creant-lo si és nou.
    fn ctl(&self, session: &str) -> Arc<SessionCtl> {
        let mut map = self.sessions.lock().unwrap();
        let entry = map
            .entry(session.to_string())
            .or_insert_with(|| Arc::new(SessionCtl::new(session)));
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
        let provider = self.effective_provider(&ctl).await;
        let model = self.model_for(&ctl).await;
        (provider, model)
    }

    /// La comanda de xat només ha de contar un faliment si cap backend (Ollama
    /// o remot) no n'ha llançat ja un amb explicació i temps real.
    pub fn session_error_shown(&self, session: &str) -> bool {
        self.ctl(session).error_shown()
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
        // Memòria exhaurida a mitja resposta: comprimim el context i tornem-hi
        // UNA vegada, per a poder continuar treballant en lloc de fallar.
        if let Err(e) = &result {
            if e.to_string() != CANCELLED_MSG && is_body_cut(&e.to_string()) {
                let compacted = compact_prompt(prompt);
                if compacted != prompt {
                    let light = ChatOpts {
                        history: compact_history(&opts.history),
                        ..opts.clone()
                    };
                    result = self.chat_inner(&compacted, system, &ctl, &provider, &light).await;
                }
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

        let active = self.effective_provider(&ctl).await;
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
        // talla per manca de memòria, segon intent amb el context COMPRIMIT,
        // perquè l'agent puga seguir treballant en lloc de rendir-se.
        let mut res = self.ollama_stream(app, prompt, system, &ctl, opts, &start).await;
        if let Err(e) = &res {
            if e.to_string() != CANCELLED_MSG && is_body_cut(&e.to_string()) {
                let compacted = compact_prompt(prompt);
                if compacted != prompt {
                    // El visor ha de continuar mostrant el model REAL: posar-hi
                    // «noorbit» com a etiqueta confonia l'usuari.
                    let (_, m) = self.session_labels(&ctl.id).await;
                    ctl.process(
                        app,
                        "ollama",
                        &m,
                        "thinking",
                        "La memòria no abastava tot el context: el comprimeix i ho torna a provar.",
                        start.elapsed().as_millis() as u64,
                    );
                    let light = ChatOpts {
                        history: compact_history(&opts.history),
                        ..opts.clone()
                    };
                    res = self
                        .ollama_stream(app, &compacted, system, &ctl, &light, &start)
                        .await;
                }
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
        let req = ChatRequest {
            model,
            messages,
            stream: true,
            system: system.map(|s| s.to_string()),
            think,
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
        let send = Self::send_chat(&self.http_stream, &chat_url, &req);
        tokio::pin!(send);
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
        let mut thinking_acc = String::new();
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
        let req = ChatRequest {
            model,
            messages,
            stream: false,
            system: system.map(|s| s.to_string()),
            think,
            options: Self::ctx_options(prompt_chars, system.map_or(0, |s| s.len())),
        };
        let chat_url = self.url("/api/chat");
        let parsed: ChatResponse = race_cancel(ctl, async {
            let resp = Self::send_chat(&self.http, &chat_url, &req).await?;
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
    ) -> Result<reqwest::Response> {
        http.post(url)
            .json(req)
            .send()
            .await
            // No propaguem l'error opac de reqwest: expliquem la CAUSA real
            // (temps exhaurit, Ollama aturat, port satur…) amb què fer.
            .map_err(|e| anyhow!("Ollama no respon — {}", explain_send_failure(&e, url)))
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
                    available: p.enabled && !p.token.trim().is_empty(),
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
