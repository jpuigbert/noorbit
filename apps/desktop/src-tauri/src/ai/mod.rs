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
        }
    }

    /// Comença un torn nou: sense bandera d'aturada i sense raonament anterior.
    pub async fn begin(&self) {
        self.cancel.store(false, Ordering::Relaxed);
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
    #[serde(default)]
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

/// Arrenca `ollama serve` en segon pla si el binari existeix (perquè el
/// port 11434 responda quan l'usuari no té l'app oberta).
pub fn start_ollama_server() -> Result<()> {
    if !ollama_installed() {
        return Err(anyhow!("Ollama no està instal·lat"));
    }
    // Carpeta de models configurada (p. ex. un USB extern); buida = per defecte.
    let models_dir = crate::config::AppConfig::load()
        .map(|c| c.ai.models_dir.trim().to_string())
        .unwrap_or_default();
    // Si ja escolta, no cal fer res.
    if std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], 11434)),
        std::time::Duration::from_millis(400),
    )
    .is_ok()
    {
        return Ok(());
    }
    #[cfg(unix)]
    {
        let mut cmd = std::process::Command::new("nohup");
        cmd.arg("ollama")
            .arg("serve")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if !models_dir.is_empty() {
            cmd.env("OLLAMA_MODELS", &models_dir);
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
            cmd.env("OLLAMA_MODELS", &models_dir);
        }
        cmd.spawn()
            .map(|_| ())
            .map_err(|e| anyhow!("No s'ha pogut arrencar Ollama: {}", e))?;
    }
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
            ollama_url: config.ai.ollama_url.clone(),
            default_model: config.ai.default_model.clone(),
            active_provider: Arc::new(RwLock::new("ollama".into())),
            selected_model: Arc::new(RwLock::new(None)),
            remote: None,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            background: Arc::new(AtomicBool::new(false)),
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
                    ctl.process(
                        app,
                        "ollama",
                        "noorbit",
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
        let req = ChatRequest {
            model,
            messages,
            stream: true,
            system: system.map(|s| s.to_string()),
            options,
        };
        ctl.process(app, "ollama", &model_label, "thinking", "", 0);
        let chat_url = self.url("/api/chat");
        let resp = match Self::send_chat(&self.http, &chat_url, &req).await {
            Ok(r) => r,
            Err(e) => {
                ctl.process(app, "ollama", &model_label, "error", &e.to_string(), 0);
                return Err(e);
            }
        };
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            let err = anyhow!("Ollama error {}: {}", status, text);
            ctl.process(app, "ollama", &model_label, "error", &err.to_string(), 0);
            return Err(err);
        }
        let mut stream = resp.bytes_stream();
        use futures::StreamExt;
        let notify = ctl.notify();
        let mut buf: Vec<u8> = Vec::new();
        let mut full = String::new();
        let mut thinking_acc = String::new();
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
                    if let Some(msg) = v.get("message") {
                        if let Some(c) = msg.get("content").and_then(|c| c.as_str()) {
                            if !c.is_empty() {
                                full.push_str(c);
                                let ms = start.elapsed().as_millis() as u64;
                                ctl.chunk(app, c);
                                ctl.process(
                                    app,
                                    "ollama",
                                    &model_label,
                                    "streaming",
                                    c,
                                    ms,
                                );
                                if runaway_repetition(&full) {
                                    ctl.process(
                                        app,
                                        "ollama",
                                        &model_label,
                                        "thinking",
                                        "S'ha detectat un bucle de repetició: la generació s'atura per a no bloquejar-se.",
                                        ms,
                                    );
                                    break 'stream;
                                }
                            }
                        }
                        if let Some(th) = msg.get("thinking").and_then(|c| c.as_str()) {
                            if !th.is_empty() {
                                thinking_acc.push_str(th);
                                let ms = start.elapsed().as_millis() as u64;
                                ctl.process(
                                    app,
                                    "ollama",
                                    &model_label,
                                    "thinking",
                                    th,
                                    ms,
                                );
                            }
                        }
                    }
                }
            }
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
        let req = ChatRequest {
            model,
            messages,
            stream: false,
            system: system.map(|s| s.to_string()),
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
        ctl.set_thinking(parsed.message.thinking.filter(|t| !t.trim().is_empty()))
            .await;
        Ok(parsed.message.content)
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
    /// Es manté entre 2048 i 4096 tokens: una finestra gran desborda la RAM
    /// d'equips lleugers (8 GB) i fa que la resposta trigui minuts.
    /// En mode segon pla (eco) es redueix a 2048 fixos i es limiten els fils.
    fn ctx_options(prompt_chars: usize, system_chars: usize) -> Option<serde_json::Value> {
        let approx_tokens = (prompt_chars + system_chars) / 3 + 512;
        let mut n = 2048usize;
        while n < 4096 && n < approx_tokens {
            n *= 2;
        }
        Some(serde_json::json!({ "num_ctx": n }))
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
