//! DIRECTOR de vídeo autopilotat: la IA munta el vídeo SENCERA.
//!
//! Aquest mòdul és el que converteix «tinc plats de Blender» en «tinc un vídeo
//! acabat». Funciona com el motor de `project_work` (pressupost de temps,
//! cancel·lable, esdeveniments d'estat) però en lloc d'editar codi edita
//! muntatge. Les seves fases:
//!
//! 1. **Inventari** — descobreix i sondeja tots els orígens (ffprobe).
//! 2. **Mirada** — mesura coses reals: lluentor, saturació, moviment, plans
//!    morts, canvis de pla, silencis, LUFS. I construeix GRAELLES de
//!    fotogrames perquè un model amb visió VEURI els clips sencers.
//! 3. **Guió** — la IA escriu l'EDL en JSON (ordre, entrades/iXudes,
//!    transicions, ritme, textos, sonoritat) seguint un contracte estricte.
//! 4. **Previsualització autocrítica** — renderitza una previsualització
//!    ràpida, en fa una graella, i la mateixa IA la critica amb una rúbrica
//!    professional; torna a l'EDL i repeteix. Aquí és on es ganha el que
//!    abans eren «els 40 minuts» de criteri humà.
//! 5. **Render** — mesura LUFS (pas 1) i fa el render final (pas 2 lineal),
//!    amb progrés i cancel·lació.
//! 6. **Comprovació** — verifica durada, sonoritat, pics de negre i plans
//!    congelats; exporta `.kdenlive`+`.srt` i l'EDL desat.
//!
//! Si la IA falla (model absent, JSON inservible), el director NO mor: cau a
//! `auto_edl` (muntatge determinista) i l'usuari acaba amb un vídeo igualment.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::ai::{AiManager, ChatOpts};
use crate::video::{detect, edl::{self, Clip, Edl, TextItem, Transition, Grade}, ff, mlt, render};

/// Pressupost màxim (7 dies, com la Jam) i mínim (60 s).
pub const MAX_BUDGET_SECS: u64 = 7 * 24 * 3600;
/// Crítiques per defecte (cada una costa una previsualització).
pub const DEFAULT_CRITIQUES: u32 = 2;
/// Màxim de crítiques que acceptem (el pressupost manà).
pub const MAX_CRITIQUES: u32 = 8;
/// Màxim d'orígens que es mostren a la IA amb imatges.
pub const MAX_VISION_SOURCES: usize = 6;
/// Màxim d'orígens que es descobreixen en un projecte.
pub const MAX_FOUND_SOURCES: usize = 60;

/// Estat visible per a la UI.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorState {
    pub running: bool,
    /// Fase actual (`inventari`, `mirada`, `guió`, `previsualització`,
    /// `crítica`, `render`, `comprovació`).
    pub stage: String,
    pub pass: u32,
    pub critique: u32,
    pub elapsed_ms: u64,
    pub budget_ms: u64,
    /// Progrés del render en curs (0…1).
    pub progress: f64,
    pub detail: String,
    pub sources_found: u32,
    pub clips: u32,
    pub target_s: f64,
    pub out_path: Option<String>,
    pub project_path: Option<String>,
    pub edl: Option<Edl>,
    pub report: Option<serde_json::Value>,
    /// Línies de registre curtes (les últimes 40) per veure què ha fet.
    pub log: Vec<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
}

impl Default for DirectorState {
    fn default() -> Self {
        Self {
            running: false,
            stage: "aturat".into(),
            pass: 0,
            critique: 0,
            elapsed_ms: 0,
            budget_ms: 0,
            progress: 0.0,
            detail: String::new(),
            sources_found: 0,
            clips: 0,
            target_s: 0.0,
            out_path: None,
            project_path: None,
            edl: None,
            report: None,
            log: Vec::new(),
            started_at: 0,
            finished_at: None,
            error: None,
        }
    }
}

/// Handle global: només un director alhora (mateix patró que la Jam).
pub struct DirectorHandle {
    pub cancel: std::sync::Mutex<CancellationToken>,
    pub state: std::sync::Mutex<DirectorState>,
    pub running: AtomicBool,
    /// Bandera que mira el bucle de render per matar `ffmpeg` de seguida.
    pub stop_render: Arc<AtomicBool>,
}

impl Default for DirectorHandle {
    fn default() -> Self {
        Self {
            cancel: std::sync::Mutex::new(CancellationToken::new()),
            state: std::sync::Mutex::new(DirectorState::default()),
            running: AtomicBool::new(false),
            stop_render: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl DirectorHandle {
    pub fn snapshot(&self) -> DirectorState {
        self.state.lock().unwrap().clone()
    }

    fn set(&self, f: impl FnOnce(&mut DirectorState)) {
        if let Ok(mut s) = self.state.lock() {
            f(&mut s);
        }
    }

    fn log(&self, line: &str) {
        self.set(|s| {
            s.log.push(line.to_string());
            if s.log.len() > 40 {
                let extra = s.log.len() - 40;
                s.log.drain(..extra);
            }
        });
    }
}

fn emit(app: &AppHandle, handle: &DirectorHandle) {
    let _ = app.emit("video://director", handle.snapshot());
}

/// Carpeta de treball del muntatge dins el projecte.
pub fn workdir(root: &Path) -> PathBuf {
    root.join("VideoStudio")
}

/// Directriu del xat/Jam: `@video:<acció>[ <arguments>]`.
pub fn directive_help() -> &'static str {
    "\n\
## Eines de vídeo (muntatge Blender/Kdenlive)\n\
Pots controlar el muntatge amb directrius, una per línia:\n\
- `@video:status` — quines eines hi ha (ffmpeg/melt) i si el projecte ja té muntatge desat.\n\
- `@video:scan` — inventari mesurat de tots els vídeos del projecte (durada, mida, fps, àudio).\n\
- `@video:mount <json>` — guarda un muntatge sense renderitzar. El JSON és:\n\
  `{\"name\":\"…\",\"profile\":{\"width\":1920,\"height\":1080,\"fps\":25,\"crf\":20,\"preset\":\"medium\"},`\n\
  `\"clips\":[{\"source\":\"ruta.mp4\",\"label\":\"què s'hi veu\",\"inS\":0.0,\"outS\":4.2,\"mute\":false,`\n\
  `\"transition\":{\"kind\":\"fade\",\"durS\":0.5},\"grade\":{\"contrast\":1.0,\"saturation\":1.05}}],`\n\
  `\"texts\":[{\"text\":\"…\",\"atS\":0.6,\"durS\":2.4,\"size\":56,\"pos\":\"center\"}],`\n\
  `\"targetLufs\":-16.0,\"normalizeAudio\":true}`. Les rutes van EXACTES com a `@video:scan`;\n\
  l'últim clip no porta `transition`.\n\
- `@video:auto <segons>` — muntatge determinista (sense criteri) a partir del que hi ha al projecte.\n\
- `@video:preview` — codifica una previsualització ràpida i barata per revisar el pla.\n\
- `@video:render` — render final (pot portar `container=webm`).\n\
- `@video:export` — escriu el projecte `.kdenlive` + `.srt` per polir-ho a la GUI.\n\
- `@video:verify` — comprovació de l'últim render (durada, sonoritat, plans morts).\n\
- `@video:open <ruta>` — afegeix un origen concret a la llista d'orígens.\n\
- `@video:director {\"budgetS\":2400,\"targetS\":60,\"critiques\":2,\"brief\":\"…\"}` — munta TOT SOL:\n\
  mira els clips amb visió, escriu el guió, es critica amb rúbrica professional i renderitza.\n\
  És l'opció quan demanis un vídeo acabat amb criteri (pausa, continuïtat, sonoritat).\n\
Les accions lentes es fan en segon terme: consulta `@video:status` per saber per on van.\n"
}

/// Origen dels fitxers de vídeo del projecte (per a la directriu `@video:open`).
pub fn remember_source(root: &Path, source: &str) -> Result<PathBuf, String> {
    let p = render::resolve(root, source);
    if !p.exists() {
        return Err(format!("no existeix: {}", p.display()));
    }
    let s = ff::probe(&p.to_string_lossy()).map_err(|e| e.to_string())?;
    let ed = workdir(root).join("sources.txt");
    let mut lines: Vec<String> = std::fs::read_to_string(&ed)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let rel = p.to_string_lossy().to_string();
    if !lines.contains(&rel) {
        lines.push(rel);
    }
    let _ = s;
    std::fs::write(&ed, lines.join("\n")).map_err(|e| e.to_string())?;
    Ok(p)
}

/// Cerca vídeos dins el projecte (sense entrar en brossa de build).
pub fn discover(root: &Path) -> Vec<String> {
    const SKIP: &[&str] = &[
        "node_modules", ".git", "target", "build", "dist", ".venv", "venv", "__pycache__",
        ".cache", "Library", "DerivedData", ".gradle", ".cargo", "Pods", "Intermediate",
        "Saved", ".noorbit",
    ];
    let mut out: Vec<String> = Vec::new();
    for e in walkdir::WalkDir::new(root).max_depth(6).into_iter().filter_map(|x| x.ok()) {
        if !e.file_type().is_file() {
            continue;
        }
        let path = e.path();
        if path
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.') && c.as_os_str() != ".")
            && !path.to_string_lossy().contains("VideoStudio")
        {
            // Ignora carpetes puntejades, excepte la nostra àrea de treball.
        }
        if SKIP.iter().any(|s| path.to_string_lossy().contains(&format!("/{s}/"))) {
            continue;
        }
        let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if !ff::VIDEO_EXT.contains(&ext.as_str()) {
            continue;
        }
        // El resultat d'un render anterior no és material brut.
        if path.to_string_lossy().contains("VideoStudio/renders") {
            continue;
        }
        let sz = path.metadata().map(|m| m.len()).unwrap_or(0);
        if sz < 4096 {
            continue;
        }
        out.push(path.to_string_lossy().to_string());
        if out.len() >= MAX_FOUND_SOURCES {
            break;
        }
    }
    out.sort();
    out
}

/// Llista d'orígens guardats a `VideoStudio/sources.txt` (el que l'usuari ha anat afegint).
pub fn saved_sources(root: &Path) -> Vec<String> {
    let f = workdir(root).join("sources.txt");
    std::fs::read_to_string(&f)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && Path::new(l).exists())
        .collect()
}

pub fn save_sources(root: &Path, list: &[String]) -> Result<(), String> {
    std::fs::create_dir_all(workdir(root)).map_err(|e| e.to_string())?;
    std::fs::write(workdir(root).join("sources.txt"), list.join("\n")).map_err(|e| e.to_string())
}

/// Dossier de mesures en text, per al prompt.
fn dossier(items: &[(ff::Source, Vec<ff::FrameStat>, Vec<f64>, Vec<ff::Silence>)]) -> String {
    let mut s = String::new();
    for (src, st, sc, sil) in items {
        s.push_str(&ff::describe(src, st, sc, sil));
    }
    s
}

/// Contract de sortida de la IA (JSON d'EDL) explicat al model.
const EDL_CONTRACT: &str = "\n\
FORMAT DE RESPOSTA OBLIGATORI (no n'afegis res fora d'aquest marc):\n\
NOORBIT_EDL\n\
{\n\
  \"name\": \"títol curt del muntatge\",\n\
  \"profile\": { \"width\": 1920, \"height\": 1080, \"fps\": 25, \"crf\": 20, \"preset\": \"medium\" },\n\
  \"clips\": [\n\
    { \"source\": \"ruta/relativa/o/absoluta.mp4\", \"label\": \"què s'hi veu\", \"inS\": 0.0, \"outS\": 4.2,\n\
      \"speed\": 1.0, \"gain\": 1.0, \"fadeInS\": 0.0, \"fadeOutS\": 0.0, \"mute\": false,\n\
      \"transition\": { \"kind\": \"fade\", \"durS\": 0.5 },\n\
      \"grade\": { \"brightness\": 0.0, \"contrast\": 1.0, \"saturation\": 1.05, \"warmth\": 0.0, \"stabilize\": false } }\n\
  ],\n\
  \"texts\": [ { \"text\": \"...\", \"atS\": 0.6, \"durS\": 2.4, \"size\": 56, \"pos\": \"center\", \"kind\": \"title\" } ],\n\
  \"targetLufs\": -16.0, \"normalizeAudio\": true,\n\
  \"note\": \"per què aquest muntatge és el correcte (2-4 frases, en català)\"\n\
}\n\
FIN_NOORBIT_EDL\n\
RESUM: una frase dient què has muntat i per què.\n\
\n\
REGLES DEL JSON:\n\
- `source` EXACTAMENT com apareix a la llista d'orígens (respecta majúscules i rutes).\n\
- `inS`/`outS` en segons dins l'origen, mai negatius, mai més llargs que l'origen.\n\
- `transition.kind` només d'aquesta llista: fade, wipeleft, wiperight, wipeup, wipedown,\n\
  slideleft, slideright, slideup, slidedown, circleopen, circleclose, radial, smoothleft,\n\
  smoothright, dissolve, pixelize, hblur, fadegrays, distance, zoomin.\n\
- L'últim clip NO porta `transition`.\n\
- Escriu el JSON complet, sense cometes ni comes trencades, i sense cap altre text entre\n\
  les marques NOORBIT_EDL / FIN_NOORBIT_EDL.\n";

/// Rúbrica professional que la IA ha d'aplicar a la seva autocrítica.
const RUBRIC: &str = "\n\
RÚBRICA DE MUNTATGE PROFESSIONAL (avalua punt per punt, amb notes 0-10):\n\
1. RITME: les durades dels plans són intencionades? cap planxat de més de 6 s sense\n\
   motiu? els tallos cauen sobre acció (no a mitja transició natural del pla)?\n\
2. CONTINUÏTAT: hi ha salts d'eix, de mida o de llum bruts entre plans següents?\n\
3. HARMONIA: els plans tenen temperatura i saturació coherents (les mesures ho diuen)?\n\
4. ÀUDIO: nivells homogenis, sense pujades sobtades, sense silencis morts al final?\n\
5. CAPÇALERA I TANCAMENT: el primer pla enganxa en 2 s? l'últim tanca amb calma?\n\
6. TEXTOS: els títols es llegeixen en la seva durada? no tapen acció important?\n\
7. FIDELITAT AL BRIEF: compleix l'objectiu, l'audiència i la durada demanades?\n\
8. QUALITAT VISUAL: cap mostra gairebé negra o congelada? cap escalat visible?\n";

/// Cerca el primer objecte JSON balanced que es pugui llegir com a EDL.
fn extract_edl(text: &str) -> Option<Edl> {
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == '{' {
            if let Some(end) = match_brace(&bytes, i) {
                let cand: String = bytes[i..=end].iter().collect();
                if let Ok(mut e) = serde_json::from_str::<Edl>(&cand) {
                    if e.has_clips() {
                        edl::sanitize(&mut e);
                        if e.has_clips() {
                            return Some(e);
                        }
                    }
                }
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    None
}

fn match_brace(s: &[char], from: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (k, c) in s.iter().enumerate().skip(from) {
        if in_str {
            if esc {
                esc = false;
            } else if *c == '\\' {
                esc = true;
            } else if *c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

/// Accepta text lliure (la resposta d'una IA, un fitxer JSON o un bloc
/// amb les marques `NOORBIT_EDL`) i en treu el primer EDL vàlid.
pub fn parse_edl_text(text: &str) -> Option<Edl> {
    extract_edl(text)
}

/// `RESUM: …` de la resposta del model (per a la UI).
fn extract_resum(text: &str) -> Option<String> {
    for line in text.lines() {
        let l = line.trim_start_matches('#').trim();
        for key in ["RESUM:", "Resum:", "resum:"] {
            if let Some(rest) = l.strip_prefix(key) {
                let t = rest.trim();
                if !t.is_empty() {
                    return Some(t.chars().take(400).collect());
                }
            }
        }
    }
    None
}

/// `VEREDICTE: OK|POLIR` de l'autocrítica.
fn extract_verdict(text: &str) -> Option<bool> {
    for line in text.lines() {
        let l = line.to_ascii_uppercase();
        if let Some(rest) = l.strip_prefix("VEREDICTE:") {
            let r = rest.trim();
            if r.starts_with("OK") || r.starts_with("APROV") || r.starts_with("PASS") {
                return Some(true);
            }
            if r.starts_with("POLIR") || r.starts_with("MILLOR") || r.starts_with("FAIL") {
                return Some(false);
            }
        }
    }
    None
}

/// Demana la IA amb imatges i, si el model no té visió, ho torna a intentar
/// sense (els models locals petits fan error dur).
async fn ask(
    ai: &Arc<AiManager>,
    prompt: &str,
    system: &str,
    images: &[String],
    provider: &Option<String>,
    model: &Option<String>,
    vision_ok: &mut bool,
    token: &CancellationToken,
    per_call: Duration,
) -> Result<String, String> {
    let use_images = if *vision_ok { images.to_vec() } else { Vec::new() };
    let opts = ChatOpts {
        provider: provider.clone(),
        model: model.clone(),
        images: use_images,
        long_run: true,
        ..Default::default()
    };
    let r = tokio::select! {
        _ = token.cancelled() => return Err("cancel·lat per l'usuari".into()),
        r = tokio::time::timeout(per_call, ai.chat_opts(prompt, Some(system), &opts)) => r,
    };
    match r {
        Ok(Ok(t)) => Ok(t),
        Ok(Err(e)) => {
            let low = e.to_string().to_lowercase();
            let no_vision = low.contains("image") || low.contains("vision") || low.contains("does not support");
            if *vision_ok && !opts.images.is_empty() && no_vision {
                *vision_ok = false;
                // Reintent sense imatges: la feina surt igual, només perdem la
                // capa estètica de visió (les mesures numèriques segueixen ahí).
                let opts2 = ChatOpts {
                    provider: provider.clone(),
                    model: model.clone(),
                    long_run: true,
                    ..Default::default()
                };
                let r2 = tokio::select! {
                    _ = token.cancelled() => return Err("cancel·lat per l'usuari".into()),
                    r = tokio::time::timeout(per_call, ai.chat_opts(prompt, Some(system), &opts2)) => r,
                };
                return match r2 {
                    Ok(Ok(t)) => Ok(t),
                    Ok(Err(e)) => Err(e.to_string()),
                    Err(_) => Err("la IA ha tardat massa (temps esgotat)".into()),
                };
            }
            Err(e.to_string())
        }
        Err(_) => Err("la IA ha tardat massa (temps esgotat)".into()),
    }
}

#[allow(clippy::too_many_arguments)]
pub struct Request {
    pub root: PathBuf,
    pub name: String,
    pub brief: String,
    pub sources: Vec<String>,
    pub budget: Duration,
    pub target_s: f64,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub critiques: u32,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    /// Qualitat i velocitat de codificació triades a la UI (van al perfil de l'EDL).
    pub crf: Option<u32>,
    pub preset: Option<String>,
    pub container: String,
    pub silent: bool,
}

/// Arrenca el director (ja validat). No bloqueja.
pub fn start(
    app: AppHandle,
    ai: Arc<AiManager>,
    handle: Arc<DirectorHandle>,
    req: Request,
) -> Result<(), String> {
    if handle.running.load(Ordering::SeqCst) {
        return Err("ja hi ha un director de vídeo treballant".into());
    }
    if !req.root.is_dir() {
        return Err(format!("la carpeta del projecte no existeix: {}", req.root.display()));
    }
    let token = CancellationToken::new();
    if let Ok(mut c) = handle.cancel.lock() {
        *c = token.clone();
    }
    handle.stop_render.store(false, Ordering::SeqCst);
    handle.running.store(true, Ordering::SeqCst);
    let now = chrono::Local::now().timestamp();
    handle.set(|s| {
        *s = DirectorState {
            running: true,
            stage: "inventari".into(),
            budget_ms: req.budget.as_millis() as u64,
            detail: "Buscant orígens de vídeo…".into(),
            started_at: now,
            project_path: Some(req.root.to_string_lossy().to_string()),
            target_s: req.target_s,
            ..Default::default()
        };
    });
    emit(&app, &handle);

    tauri::async_runtime::spawn(async move {
        run(app, handle, ai, req, token).await;
    });
    Ok(())
}

/// Ajustos del perfil des de la UI (resolució/fps triats per l'usuari).
fn apply_overrides(e: &mut Edl, req: &Request) {
    if let (Some(w), Some(h)) = (req.width, req.height) {
        if w >= 160 && h >= 120 {
            e.profile.width = w;
            e.profile.height = h;
        }
    }
    if let Some(f) = req.fps {
        if f > 1.0 && f < 120.0 {
            e.profile.fps = f;
        }
    }
    if let Some(c) = req.crf {
        e.profile.crf = c.clamp(12, 32);
    }
    if let Some(p) = req.preset.as_ref() {
        const PRESETS: [&str; 5] = ["ultrafast", "superfast", "veryfast", "medium", "slow"];
        if PRESETS.contains(&p.trim()) {
            e.profile.preset = p.trim().to_string();
        }
    }
    if e.profile.preset.trim().is_empty() {
        e.profile.preset = "medium".into();
    }
    if req.silent {
        for c in e.clips.iter_mut() {
            c.mute = true;
        }
    }
}

/// Resol l'arrel dels orígens: les rutes relatives de la IA poden ser
/// relatives al projecte o absolutes.
fn fix_sources(e: &mut Edl, root: &Path, known: &[String]) -> Vec<String> {
    let mut notes = Vec::new();
    for c in e.clips.iter_mut() {
        let raw = c.source.trim().to_string();
        let direct = render::resolve(root, &raw);
        if direct.exists() {
            c.source = direct.to_string_lossy().to_string();
            continue;
        }
        // El model de vegades inventa el nom d'una carpeta: cerquem per nom.
        let base = Path::new(&raw)
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if let Some(hit) = known.iter().find(|k| {
            k.to_lowercase() == raw.to_lowercase()
                || k.to_lowercase().ends_with(&format!("/{base}"))
                || Path::new(k).file_name().map(|n| n.to_string_lossy().to_lowercase()).as_deref() == Some(base.as_str())
        }) {
            c.source = hit.clone();
            notes.push(format!("l'origen «{raw}» s'ha resolt com a «{hit}»"));
        } else {
            notes.push(format!("l'origen «{raw}» no existeix; el clip s'ha descartat"));
            c.source = String::new();
        }
    }
    e.clips.retain(|c| !c.source.trim().is_empty());
    notes
}

#[allow(clippy::too_many_arguments)]
async fn run(
    app: AppHandle,
    handle: Arc<DirectorHandle>,
    ai: Arc<AiManager>,
    req: Request,
    token: CancellationToken,
) {
    let _awake = crate::keep_awake::activar("director de vídeo");
    let t0 = Instant::now();
    let deadline = t0 + req.budget;
    let root = req.root.clone();
    let work = workdir(&root);
    let renders = work.join("renders");
    let sheets = work.join("analysis");
    let _ = std::fs::create_dir_all(&renders);
    let _ = std::fs::create_dir_all(&sheets);
    let mut vision_ok = true;
    let per_call = Duration::from_secs(20 * 60);
    let container = render::Container::parse(&req.container);

    macro_rules! beat {
        ($stage:expr, $detail:expr) => {{
            let h = handle.clone();
            let appc = app.clone();
            let st = $stage.to_string();
            let dt = $detail.to_string();
            h.set(|s| {
                s.stage = st;
                s.detail = dt;
                s.elapsed_ms = t0.elapsed().as_millis() as u64;
            });
            emit(&appc, &h);
        }};
    }

    let mut final_err: Option<String> = None;
    let mut out_path: Option<PathBuf> = None;
    let mut report: Option<serde_json::Value> = None;
    let mut chosen: Option<Edl> = None;

    // ── 1) INVENTARI ────────────────────────────────────────────────────────
    let mut sources: Vec<String> = Vec::new();
    for s in &req.sources {
        let p = render::resolve(&root, s);
        if p.exists() && !sources.contains(&p.to_string_lossy().to_string()) {
            sources.push(p.to_string_lossy().to_string());
        }
    }
    if sources.is_empty() {
        let mut disc = discover(&root);
        let saved = saved_sources(&root);
        for s in saved {
            if !disc.contains(&s) {
                disc.insert(0, s);
            }
        }
        sources = disc;
    }
    if sources.is_empty() {
        final_err = Some(
            "no he trobat cap vídeo al projecte. Genera'l primer amb Blender \
             (@blender-video:) o afegeix orígens al panell."
                .into(),
        );
    } else {
        handle.set(|s| s.sources_found = sources.len() as u32);
        beat!(
            "inventari",
            format!("{} orígens trobats; els prenc la mesura", sources.len())
        );
    }

    // ── 2) MIRADA (mesurar + veure) ─────────────────────────────────────────
    let mut measured: Vec<(ff::Source, Vec<ff::FrameStat>, Vec<f64>, Vec<ff::Silence>)> = Vec::new();
    if final_err.is_none() {
        for (i, path) in sources.iter().enumerate() {
            if token.is_cancelled() || Instant::now() >= deadline {
                break;
            }
            beat!(
                "mirada",
                format!("mesurant origen {} de {}: {}", i + 1, sources.len(), Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
            );
            let src = match ff::probe(path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let st = ff::stats(path, 8).unwrap_or_default();
            let sc = ff::scenes(path, 0.35, 24).unwrap_or_default();
            let sil = ff::silences(path, -45.0, 0.45).unwrap_or_default();
            measured.push((src, st, sc, sil));
            // Graella per a la visió (només els primers orígens: cada imatge pesa).
            if measured.len() <= MAX_VISION_SOURCES {
                let png = sheets.join(format!("origen-{:02}.png", measured.len()));
                let _ = ff::contact_sheet(path, &png, 4, 3, 320);
            }
        }
        handle.log(&format!("mesurats {} orígens", measured.len()));
    }

    let images: Vec<String> = std::fs::read_dir(&sheets)
        .map(|rd| {
            let mut v: Vec<String> = rd
                .flatten()
                .filter(|e| e.path().to_string_lossy().ends_with(".png"))
                .map(|e| e.path().to_string_lossy().to_string())
                .collect();
            v.sort();
            v
        })
        .unwrap_or_default();

    // ── 3) GUIÓ (EDL de la IA) ──────────────────────────────────────────────
    let mut edl_draft: Option<Edl> = None;
    if final_err.is_none() {
        beat!("guió", "la IA dissenya el muntatge");
        let mut p = String::new();
        p.push_str(&format!(
            "Ets un MUNTADOR de vídeo professional. Projecte: «{}».\n\
             BRIEF DE L'USUARI:\n{}\n\n",
            req.name, req.brief
        ));
        if req.target_s > 0.5 {
            p.push_str(&format!(
                "DURADA OBJECTIU: uns {:.0} s (accepta'n un ±15%).\n\n",
                req.target_s
            ));
        }
        p.push_str("MESURES REALS DELS ORÍGENS (ffprobe + estadística de fotogrames):\n");
        p.push_str(&dossier(&measured));
        p.push_str("\nLLISTA D'ORÍGENS (usa exactament aquestes rutes a \"source\"):\n");
        for m in &measured {
            p.push_str(&format!("- {}\n", m.0.path));
        }
        if !images.is_empty() {
            p.push_str(
                "\nT'adjunto una GRAELLA de fotogrames de cada origen (ordre de la llista). \
                 Mira-les: el que veus decideix l'ordre i els punts de tall.\n",
            );
        }
        p.push_str(&format!(
            "\n{}{}{}Si un origen té parts mortes (silencis o plans congelats) DEIXA'LES \
             FORA amb inS/outS. Talla on passi algo. Explica't a \"note\".\n",
            EDL_CONTRACT,
            "\n",
            RUBRIC
        ));
        let system = "Ets un muntador i colorista de gran experiència. Responns amb \
            precisió tècnica i criteri estètic. Mai inventes rutes: nomsols els orígens \
            que se t'han donat.";
        match ask(&ai, &p, system, &images, &req.provider, &req.model, &mut vision_ok, &token, per_call).await {
            Ok(text) => {
                if let Some(mut e) = extract_edl(&text) {
                    let mut notes = fix_sources(&mut e, &root, &sources);
                    apply_overrides(&mut e, &req);
                    let (_d, fixes) = edl::sanitize(&mut e);
                    notes.extend(fixes);
                    for n in &notes {
                        handle.log(n);
                    }
                    if e.has_clips() {
                        if let Some(r) = extract_resum(&text) {
                            handle.log(&r);
                        }
                        edl_draft = Some(e);
                    } else {
                        final_err = Some("la IA ha respost un EDL sense clips vàlids".into());
                    }
                } else {
                    handle.log("el JSON de la IA no s'ha entès; caic al muntatge automàtic");
                }
            }
            Err(e) => {
                handle.log(&format!("la IA no ha respost ({e}); caic al muntatge automàtic"));
            }
        }
    }

    // Fallback determinista perquè l'usuari ACABI SEMPRE amb un vídeo.
    let mut current = match edl_draft {
        Some(e) => e,
        None => {
            let list: Vec<ff::Source> = measured.iter().map(|m| m.0.clone()).collect();
            let mut e = edl::auto_edl(&list, &req.name, req.target_s);
            apply_overrides(&mut e, &req);
            let (_d, fixes) = edl::sanitize(&mut e);
            for f in fixes {
                handle.log(&f);
            }
            if e.has_clips() {
                handle.log("muntatge automàtic de reserva (sense criteri de IA)");
                e
            } else {
                final_err = Some(final_err.take().unwrap_or_else(|| "cap origen usable".into()));
                Edl::default()
            }
        }
    };

    // ── 4) PREVISUALITZACIÓ + AUTOCRÍTICA (els «40 minuts» de criteri) ──────
    if final_err.is_none() && req.critiques > 0 {
        for round in 1..=req.critiques.min(MAX_CRITIQUES) {
            if token.is_cancelled() || Instant::now() >= deadline {
                break;
            }
            handle.set(|s| s.critique = round);
            beat!("previsualització", format!("render de prova per a l'autocrítica {round}"));
            let prev_plan = match render::plan(&current, &root, &work, &sheets, true, container.clone(), None) {
                Ok(p) => p,
                Err(e) => {
                    handle.log(&format!("no puc previsualitzar: {e}"));
                    break;
                }
            };
            let prev_out = prev_plan.out.clone();
            let h = handle.clone();
            let appc = app.clone();
            let stop = handle.stop_render.clone();
            let tok = token.clone();
            let rc = root.clone();
            let done = tokio::task::spawn_blocking(move || {
                let res = render::execute(
                    &prev_plan,
                    &rc,
                    &mut |f, txt| {
                        h.set(|s| {
                            s.progress = f;
                            s.detail = format!("previsualització {round}: {txt}");
                        });
                        emit(&appc, &h);
                    },
                    &|| stop.load(Ordering::SeqCst) || tok.is_cancelled(),
                );
                render::cleanup(&prev_plan);
                res
            })
            .await;
            if !matches!(done, Ok(Ok(()))) {
                let msg = match done {
                    Ok(Err(e)) => format!("la previsualització ha fallat ({e}); passo al render final"),
                    _ => "la previsualització s'ha tallat; passo al render final".to_string(),
                };
                handle.log(&msg);
                break;
            }
            // Graella del resultat, perquè el model VEURI el seu muntatge.
            let png = sheets.join("previsualitzacio.png");
            let _ = ff::contact_sheet(&prev_out.to_string_lossy(), &png, 5, 4, 384);
            let mut imgs = vec![png.to_string_lossy().to_string()];
            imgs.extend(preview_frames(&prev_out, &sheets, 6));

            beat!("crítica", format!("la IA critica la previsualització {round}"));
            let st = ff::stats(&prev_out.to_string_lossy(), 10).unwrap_or_default();
            let p = format!(
                "Aquesta és la PREVISUALITZACIÓ del teu muntatge (graella de fotogrames en \
                 ordre cronològic, i després {} fotogrames individuals). \
                 El muntatge actual és:
{}

MESURES DEL RESULTAT:
{}
\
                {}{}\n\
                CRITICA'l amb la rúbrica. Si cal millorar-lo, torna el EDL COMPLET corregit \
                (totes les marques obligatòries). Si ja és bones, escriu exactament una línia \
                «VEREDICTE: OK» i res més.",
                imgs.len() - 1,
                serde_json::to_string_pretty(&current).unwrap_or_default(),
                st.iter()
                    .map(|f| format!("{:.1}s llum {:.2} sat {:.2} mov {:.3} negre {:.2}", f.t, f.luma, f.saturation, f.motion, f.black_ratio))
                    .collect::<Vec<_>>()
                    .join("\n"),
                RUBRIC,
                EDL_CONTRACT
            );
            let system = "Ets el MUNTADOR que es revisa a si mateix. Sigues sever amb el ritme \
                i la continuïtat; no acceptes plans morts ni bucles sense motiu.";
            match ask(&ai, &p, system, &imgs, &req.provider, &req.model, &mut vision_ok, &token, per_call).await {
                Ok(text) => {
                    if let Some(r) = extract_resum(&text) {
                        handle.log(&format!("autocrítica {round}: {r}"));
                    }
                    match extract_verdict(&text) {
                        Some(true) => {
                            handle.log("autocrítica aprovada: cap a la versió final");
                            break;
                        }
                        _ => {
                            if let Some(mut e) = extract_edl(&text) {
                                let notes = fix_sources(&mut e, &root, &sources);
                                apply_overrides(&mut e, &req);
                                let (_d, fixes) = edl::sanitize(&mut e);
                                for n in notes.into_iter().chain(fixes) {
                                    handle.log(&n);
                                }
                                if e.has_clips() {
                                    current = e;
                                    handle.set(|s| s.clips = current.clips.len() as u32);
                                    continue;
                                }
                            }
                            handle.log("l'autocrítica no ha tornat un EDL vàlid; em quedo amb aquest");
                            break;
                        }
                    }
                }
                Err(e) => {
                    handle.log(&format!("autocrítica impossible ({e})"));
                    break;
                }
            }
        }
    }

    // ── 5) RENDER FINAL ─────────────────────────────────────────────────────
    if final_err.is_none() {
        handle.set(|s| s.clips = current.clips.len() as u32);
        save_edl(&work, &current);
        beat!("render", "mesurant sonoritat (pas 1 de 2)");
        let measured_lufs = if current.normalize_audio {
            let r = root.clone();
            let w = work.clone();
            let e = current.clone();
            tokio::task::spawn_blocking(move || render::measure_audio(&e, &r, &w))
                .await
                .unwrap_or(None)
        } else {
            None
        };
        if let Some(m) = &measured_lufs {
            handle.log(&format!(
                "sonoritat mesurada {:.1} LUFS (objectiu {:.1})",
                m.input_i, current.target_lufs
            ));
        }
        beat!("render", "codificant la versió final");
        match render::plan(&current, &root, &work, &renders, false, container.clone(), measured_lufs.as_ref()) {
            Ok(pl) => {
                handle.log(&format!("graf: {}", pl.graph_note));
                let h = handle.clone();
                let appc = app.clone();
                let stop = handle.stop_render.clone();
                let tok = token.clone();
                let plan_out = pl.out.clone();
                let rc = root.clone();
                let done = tokio::task::spawn_blocking(move || {
                    let res = render::execute(
                        &pl,
                        &rc,
                        &mut |f, txt| {
                            h.set(|s| {
                                s.progress = f;
                                s.detail = format!("render final: {txt}");
                            });
                            emit(&appc, &h);
                        },
                        &|| stop.load(Ordering::SeqCst) || tok.is_cancelled(),
                    );
                    render::cleanup(&pl);
                    res.map(|_| plan_out)
                })
                .await;
                match done {
                    Ok(Ok(path)) => {
                        out_path = Some(path);
                    }
                    Ok(Err(e)) => final_err = Some(e.to_string()),
                    Err(e) => final_err = Some(format!("el render s'ha tallat: {e}")),
                }
            }
            Err(e) => final_err = Some(format!("no s'ha pogut construir el render: {e}")),
        }
    }

    // ── 6) COMPROVACIÓ + EXPORT ─────────────────────────────────────────────
    if let Some(path) = &out_path {
        beat!("comprovació", "verificant el resultat");
        if let Ok(rep) = render::verify(path, &current) {
            report = Some(rep.clone());
            let notes = rep.get("notes").and_then(|n| n.as_array()).cloned().unwrap_or_default();
            if notes.is_empty() {
                handle.log("comprovació: durada, sonoritat i imatge dins l'objectiu");
            } else {
                for n in notes.iter().filter_map(|v| v.as_str()) {
                    handle.log(&format!("avís: {n}"));
                }
            }
        }
        let mlt_path = renders.join(format!("{}.kdenlive", current.slug()));
        let ver = detect::detect().melt.version.clone().unwrap_or_else(|| "7.0.0".into());
        match mlt::write_project(&current, &root, &mlt_path, &ver) {
            Ok(written) => {
                for w in written {
                    handle.log(&format!("exportat: {w}"));
                }
                if let Some(r) = report.as_mut() {
                    r["project"] = serde_json::Value::String(mlt_path.to_string_lossy().to_string());
                }
            }
            Err(e) => handle.log(&format!("no s'ha pogut exportar el .kdenlive: {e}")),
        }
        chosen = Some(current.clone());
    }

    let secs = t0.elapsed().as_secs();
    handle.set(|s| {
        s.running = false;
        s.progress = if out_path.is_some() { 1.0 } else { s.progress };
        s.stage = if out_path.is_some() { "fet".into() } else { "error".into() };
        s.finished_at = Some(chrono::Local::now().timestamp());
        s.elapsed_ms = secs * 1000;
        s.out_path = out_path.as_ref().map(|p| p.to_string_lossy().to_string());
        s.edl = chosen.clone();
        s.report = report.clone();
        s.error = final_err.clone();
        s.detail = match (&out_path, &final_err) {
            (Some(p), _) => format!("muntatge acabat en {secs} s → {}", p.display()),
            (None, Some(e)) => e.clone(),
            (None, None) => "no s'ha arribat a renderitzar".into(),
        };
    });
    emit(&app, &handle);
}

/// Desa l'EDL corrent perquè el puga llegir la UI i el xat.
fn save_edl(work: &Path, e: &Edl) {
    let _ = std::fs::create_dir_all(work);
    if let Ok(s) = serde_json::to_string_pretty(e) {
        let _ = std::fs::write(work.join("edl.json"), s);
    }
}

/// Fotogrames individuals de la previsualització (per a l'autocrítica amb visió).
fn preview_frames(path: &Path, dir: &Path, n: usize) -> Vec<String> {
    let dur = ff::probe(&path.to_string_lossy()).map(|s| s.duration).unwrap_or(0.0);
    if dur <= 0.0 || n == 0 {
        return Vec::new();
    }
    let times: Vec<f64> = (0..n)
        .map(|i| dur * (i as f64 + 0.5) / n as f64)
        .map(|t| t.clamp(0.0, (dur - 0.04).max(0.0)))
        .collect();
    let sub = dir.join("frames");
    ff::frames_at(&path.to_string_lossy(), &sub, &times, 640)
        .unwrap_or_default()
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

/// Llegeix l'EDL desat (si hi és).
pub fn load_edl(root: &Path) -> Option<Edl> {
    let s = std::fs::read_to_string(workdir(root).join("edl.json")).ok()?;
    serde_json::from_str::<Edl>(&s).ok()
}

/// Reuneix tots els orígens possibles (per a la directriu `@video:auto`).
pub fn sources_from_dir(root: &Path, extra: &[String]) -> Vec<String> {
    let mut v: Vec<String> = extra.to_vec();
    for d in discover(root) {
        if !v.contains(&d) {
            v.push(d);
        }
    }
    v
}

/// Abre el vídeo a l'aplicació de sistema (reproductor per defecte).
pub fn open_with_default(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut c = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut c = Command::new("cmd");
    #[cfg(target_os = "windows")]
    c.args(["/C", "start", ""]);
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut c = Command::new("xdg-open");
    let status = c.arg(path).status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("el reproduceor del sistema ha fallat".into())
    }
}

/// Per si la IA demana afegir una transició concreta a mà (validació).
pub fn known_transitions() -> Vec<&'static str> {
    edl::TRANSITIONS.to_vec()
}

/// Construeix un EDL senzill d'un sol origen (per a proves ràpides).
pub fn single(path: &str, label: &str, in_s: f64, out_s: f64) -> Edl {
    Edl {
        name: label.into(),
        clips: vec![Clip {
            source: path.into(),
            label: label.into(),
            in_s,
            out_s,
            speed: 1.0,
            gain: 1.0,
            fade_in_s: 0.4,
            fade_out_s: 0.6,
            mute: false,
            transition: None,
            grade: None,
        }],
        texts: vec![TextItem {
            text: label.into(),
            at_s: 0.3,
            dur_s: 2.0,
            size: 54.0,
            pos: "bottom".into(),
            kind: "title".into(),
        }],
        target_lufs: -16.0,
        normalize_audio: true,
        note: "muntatge d'un sol origen".into(),
        profile: Default::default(),
    }
}

/// Transició de mostra per a la UI.
pub fn default_transition() -> Transition {
    Transition { kind: "fade".into(), dur_s: 0.5 }
}

/// Gradació neutra (per a la UI).
pub fn neutral_grade() -> Grade {
    Grade { brightness: 0.0, contrast: 1.0, saturation: 1.0, warmth: 0.0, stabilize: false }
}
