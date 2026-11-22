//! Millora CONTÍNUA i AUTOPILOTADA de QUALSEVOL projecte.
//!
//! L'usuari tria un temps de treball (X minuts/hores/dies) i la IA OMPLI eixe
//! temps millorant el projecte: busca bugs, rendiment, accessibilitat, SEO,
//! seguretat i noves funcionalitats, i IMPLEMENTA tot el que se li acudeix.
//!
//! Serveix per a temes de WordPress (`wp_theme_work`, perfil WordPress) i per
//! a QUALSEVOL altre projecte que l'usuari demani (`project_work`, perfil
//! genèric amb autodetecció): jocs, web, backend, eines… en general.
//!
//! AUTOPILOTATGE TOTAL (l'usuari no toca el PC per a res):
//! - La IA cerca informació a internet SOLA (`research_headless`: HTTP, mai
//!   obri finestres ni demana sessió/captcha/permís).
//! - Es fa servir `chat_opts` (xat CRU, sense eines): cap directriu «RUN|» ni
//!   «NB|» s'executa, cap diàleg de confirmació del control de l'ordinador.
//! - Els fitxers s'escriuen directament (ruta relativa segura dins el projecte).
//! - El bucle continua fins que s'OMPLI el temps triat o l'usuari prem «Atura».
//!
//! A diferència de la Jam (que PARA en completar el pla), ací el temps és un
//! MÍNIM de treball: cada passada re-llegeix el projecte (les millores
//! s'acumulen) i ataca un focus diferent, de manera que sempre hi ha feina útil.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

use crate::agent::AgentOrchestrator;
use crate::ai::{AiManager, ChatOpts};
use crate::AppState;

/// Temps MÀXIM de treball continu (7 dies, com la Jam). Mínim 60 s.
pub const MAX_WORK_SECS: u64 = 7 * 24 * 3600;

/// Estat de la millora en curs (per a la UI).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorkState {
    pub running: bool,
    /// Passada actual (cada passada ataca un focus diferent).
    pub pass: u32,
    pub elapsed_ms: u64,
    pub budget_ms: u64,
    /// Fitxers escrits/millorats acumulats entre totes les passades.
    pub files_changed: u32,
    pub detail: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub project_path: String,
    pub error: Option<String>,
}

impl Default for ProjectWorkState {
    fn default() -> Self {
        Self {
            running: false,
            pass: 0,
            elapsed_ms: 0,
            budget_ms: 0,
            files_changed: 0,
            detail: String::new(),
            started_at: 0,
            finished_at: None,
            project_path: String::new(),
            error: None,
        }
    }
}

/// Handle global (no hi ha dues millores alhora). Mateix patró que la Jam.
pub struct ProjectWorkHandle {
    pub cancel: std::sync::Mutex<CancellationToken>,
    pub state: std::sync::Mutex<ProjectWorkState>,
    pub running: AtomicBool,
}

impl Default for ProjectWorkHandle {
    fn default() -> Self {
        Self {
            cancel: std::sync::Mutex::new(CancellationToken::new()),
            state: std::sync::Mutex::new(ProjectWorkState::default()),
            running: AtomicBool::new(false),
        }
    }
}

impl ProjectWorkHandle {
    fn snapshot(&self) -> ProjectWorkState {
        self.state.lock().unwrap().clone()
    }
    fn set(&self, f: impl FnOnce(&mut ProjectWorkState)) {
        if let Ok(mut s) = self.state.lock() {
            f(&mut s);
        }
    }
}

fn emit_state(app: &AppHandle, handle: &ProjectWorkHandle) {
    let _ = app.emit("project://work", handle.snapshot());
}

/// Ruta relativa segura dins el projecte (mai «..», mai absoluta, mai «:»).
fn safe_rel(p: &str) -> Option<String> {
    let t = p.trim().replace('\\', "/");
    if t.is_empty() || t.starts_with('/') || t.split('/').any(|s| s == "..") || t.contains(':') {
        return None;
    }
    Some(t)
}

/// Perfil de treball: system prompt + focusos + prefix de cerca + extensions.
/// Tot és estàtic (`'static`) perquè el bucle el puga moure al spawn sense
/// problemes de vida. WordPress i «qualsevol projecte» comparteixen el motor.
struct WorkProfile {
    system: &'static str,
    focuses: &'static [&'static str],
    query_prefix: &'static str,
    allowed_ext: &'static [&'static str],
}

// ── Perfil WordPress ──────────────────────────────────────────────────────────

const WP_FOCUSES: [&str; 7] = [
    "bugs i errors de codi (PHP notices, warnings, funcions mal cridades, escaping)",
    "rendiment (consultes, enqueue de scripts/estils, caché, imatges, lazy load)",
    "accessibilitat (ARIA, contrast, focus, teclat, etiquetes, semàntica HTML)",
    "SEO (meta etiquetes, dades estructurades schema.org, Open Graph, canonical)",
    "seguretat (nonces, sanitització, escaping, permisos, validació d'entrades)",
    "noves funcionalitats útils per al tema (gutenberg, customizer, widgets, menús)",
    "qualitat de codi i bones pràctiques WordPress (coding standards, i18n, documentació)",
];

const WP_SYSTEM: &str = "Ets un expert sènior en desenvolupament de temes de WordPress \
i treballes en mode AUTOPILOTA: millores el tema sol, sense demanar res a l'usuari. \
Cada passada rep el codi actual del tema i un focus concret. Has de:\n\
1. Analitzar el codi i buscar bugs, problemes i oportunitats de millora del focus.\n\
2. IMPLEMENTAR les millores (no només descriure-les): escriu el codi real i executable.\n\
3. Retornar els fitxers modificats amb blocs «@file: ruta/relativa» seguits del fitxer COMPLET \
(sense «...» ni «(implementa aquí)»). Només rutes RELATIVES dins del tema (mai «..», mai absolutes).\n\
4. NO eliminar funcionalitats existents ni trencar el tema; millora'l de manera acumulativa.\n\
5. Si pots, afegeix millores extra que se t'acudisquen (documentació, readme, noves opcions).\n\
6. Al final, escriu UNA línia «RESUM: …» (en català) amb què has millorat en aquesta passada.\n\
No faces preguntes ni demanes confirmació: treballa amb el que tens i continua.";

const WP_EXT: [&str; 8] = ["php", "css", "js", "json", "txt", "md", "html", "htm"];

fn wp_profile() -> WorkProfile {
    WorkProfile {
        system: WP_SYSTEM,
        focuses: &WP_FOCUSES,
        query_prefix: "WordPress theme",
        allowed_ext: &WP_EXT,
    }
}

// ── Perfil genèric (qualsevol projecte) ───────────────────────────────────────

const GEN_FOCUSES: [&str; 7] = [
    "bugs i errors de codi (excepcions, funcions mal cridades, casos límit, escaping)",
    "rendiment (algorismes, consultes, caché, càrrega de recursos, imatges, lazy load)",
    "accessibilitat i UX (ARIA, contrast, focus, teclat, semàntica, responsivitat)",
    "SEO i documentació (meta etiquetes, schema.org, Open Graph, readme, comentaris útils)",
    "seguretat (validació d'entrades, sanitització, escaping, permisos, gestió de secrets)",
    "noves funcionalitats útils per al projecte (segons l'objectiu que ha demanat l'usuari)",
    "qualitat de codi i proves (coding standards, i18n, tests, refactor net, documentació)",
];

const GEN_SYSTEM: &str = "Ets un expert sènior en desenvolupament de programari i treballes \
en mode AUTOPILOTA: millores el projecte sol, sense demanar res a l'usuari. Cada passada rep \
el codi actual del projecte i un focus concret. Has de:\n\
1. Analitzar el codi i buscar bugs, problemes i oportunitats de millora del focus.\n\
2. IMPLEMENTAR les millores (no només descriure-les): escriu el codi real i executable.\n\
3. Retornar els fitxers modificats amb blocs «@file: ruta/relativa» seguits del fitxer COMPLET \
(sense «...» ni «(implementa aquí)»). Només rutes RELATIVES dins del projecte (mai «..», mai absolutes).\n\
4. NO eliminar funcionalitats existents ni trencar el projecte; millora'l de manera acumulativa.\n\
5. Si pots, afegeix millores extra que se t'acudisquen (documentació, readme, noves opcions, tests).\n\
6. Al final, escriu UNA línia «RESUM: …» (en català) amb què has millorat en aquesta passada.\n\
No faces preguntes ni demanes confirmació: treballa amb el que tens i continua.";

/// Extensions llegibles per a un projecte genèric (web, backend, jocs, eines…).
const GEN_EXT: [&str; 33] = [
    "php", "css", "js", "ts", "tsx", "jsx", "json", "txt", "md", "html", "htm", "py", "rs", "go",
    "java", "c", "cpp", "h", "hpp", "cs", "gd", "vue", "svelte", "scss", "sass", "less", "yml",
    "yaml", "toml", "xml", "sql", "sh", "rb",
];

fn generic_profile() -> WorkProfile {
    WorkProfile {
        system: GEN_SYSTEM,
        focuses: &GEN_FOCUSES,
        query_prefix: "software development",
        allowed_ext: &GEN_EXT,
    }
}

/// Autodetecció: si el projecte és un tema de WordPress (style.css amb
/// «Theme Name:» o l'usuari ho demana explícitament), usa el perfil WordPress;
/// si no, el perfil genèric (que també serveix per a WordPress).
fn detect_profile(root: &Path, idea: &str) -> WorkProfile {
    let lower = idea.to_ascii_lowercase();
    if lower.contains("wordpress") || lower.contains("tema wp") {
        return wp_profile();
    }
    if let Ok(css) = std::fs::read_to_string(root.join("style.css")) {
        if css.contains("Theme Name:") {
            return wp_profile();
        }
    }
    generic_profile()
}

/// Llegeix els fitxers del projecte (amb límits perquè el prompt no es dispare).
/// Torna (ruta relativa, contingut retallat). Ignora dependències i build.
fn collect_project_files(root: &Path, allowed_ext: &[&str]) -> Vec<(String, String)> {
    const MAX_FILES: usize = 30;
    const MAX_PER_FILE: usize = 4000;
    const MAX_TOTAL: usize = 50_000;
    const SKIP_DIRS: [&str; 6] = ["node_modules", "vendor", ".git", "build", "dist", ".cache"];

    let mut out: Vec<(String, String)> = Vec::new();
    let mut total = 0usize;

    fn walk(
        dir: &Path,
        root: &Path,
        depth: usize,
        allowed_ext: &[&str],
        out: &mut Vec<(String, String)>,
        total: &mut usize,
    ) {
        if depth > 4 || out.len() >= MAX_FILES || *total >= MAX_TOTAL {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = rd.flatten().collect();
        // Ordre estable: primer els fitxers clau del projecte.
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if out.len() >= MAX_FILES || *total >= MAX_TOTAL {
                return;
            }
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                    continue;
                }
                walk(&p, root, depth + 1, allowed_ext, out, total);
            } else if p.is_file() {
                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
                if !allowed_ext.contains(&ext.as_str()) {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(&p) else { continue };
                let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                let clipped: String = content.chars().take(MAX_PER_FILE).collect();
                *total += clipped.chars().count();
                out.push((rel, clipped));
            }
        }
    }
    walk(root, root, 0, allowed_ext, &mut out, &mut total);
    out
}

/// Prompt d'una passada: focus + codi actual + informació d'internet.
fn build_pass_prompt(
    name: &str,
    idea: &str,
    focus: &str,
    pass: u32,
    files: &[(String, String)],
    research: &str,
) -> String {
    let mut p = String::new();
    p.push_str(&format!(
        "Projecte: «{name}». Què ha de fer: {idea}\n\
         PASSADA {pass} — FOCUS D'AQUESTA PASSADA: {focus}\n\n\
         CODI ACTUAL DEL PROJECTE (fitxers i contingut):\n"
    ));
    if files.is_empty() {
        p.push_str("(el projecte encara no té fitxers llegibles: crea'ls segons el focus)\n");
    }
    for (rel, content) in files {
        p.push_str(&format!("\n----- @file: {rel} -----\n{content}\n"));
    }
    if !research.trim().is_empty() {
        p.push_str("\n\n");
        p.push_str(research);
    }
    p.push_str(
        "\n\nARA MILLORA EL PROJECTE segons el focus d'aquesta passada. Retorna els fitxers \n\
         modificats COMPLETS amb blocs «@file: ruta/relativa». Implementa millores reals \n\
         (bugs, rendiment, accessibilitat, SEO, seguretat, noves funcionalitats i tot el que \n\
         se t'acudeixi). Si un fitxer és molt llarg i sortir-lo sencer et faria tallar, \n\
         MODIFICA'l amb etiquetes d'edició: una línia amb @edit: i la ruta; després, per \n\
         cada canvi, una línia «<<<<<<< CERCA», el codi antic exacte, una línia «=======», \n\
         el codi nou (buit = esborrar) i una línia «>>>>>>>». Acaba amb una línia «RESUM: …».",
    );
    p
}

/// Arrenca la millora contínua autopilotada (motor compartit pels dos perfils).
fn start_work(
    app: AppHandle,
    ai: Arc<AiManager>,
    orch: Arc<AgentOrchestrator>,
    handle: Arc<ProjectWorkHandle>,
    root: PathBuf,
    name: String,
    idea: String,
    budget_secs: u64,
    provider: Option<String>,
    model: Option<String>,
    profile: WorkProfile,
) -> Result<(), String> {
    if handle.running.load(Ordering::SeqCst) {
        return Err("ja hi ha una millora de projecte en marxa".into());
    }
    if !root.is_dir() {
        return Err(format!("la carpeta del projecte no existeix: {}", root.display()));
    }
    let budget = Duration::from_secs(budget_secs.clamp(60, MAX_WORK_SECS));

    let token = CancellationToken::new();
    if let Ok(mut c) = handle.cancel.lock() {
        *c = token.clone();
    }
    handle.running.store(true, Ordering::SeqCst);
    let now = chrono::Local::now().timestamp();
    handle.set(|s| {
        *s = ProjectWorkState {
            running: true,
            pass: 0,
            elapsed_ms: 0,
            budget_ms: budget.as_millis() as u64,
            files_changed: 0,
            detail: "Començant la millora autopilotada…".into(),
            started_at: now,
            finished_at: None,
            project_path: root.to_string_lossy().to_string(),
            error: None,
        };
    });
    emit_state(&app, &handle);

    let name = name.trim().to_string();
    let idea = idea.trim().to_string();
    tauri::async_runtime::spawn(async move {
        run_loop(app, handle, ai, orch, root, name, idea, budget, provider, model, token, profile)
            .await;
    });
    Ok(())
}

/// Millora autopilotada d'un tema de WordPress (perfil WordPress).
#[tauri::command]
pub async fn wp_theme_work(
    app: AppHandle,
    state: State<'_, AppState>,
    handle: State<'_, Arc<ProjectWorkHandle>>,
    theme_path: String,
    name: String,
    idea: String,
    budget_secs: u64,
    provider: Option<String>,
    model: Option<String>,
) -> Result<(), String> {
    start_work(
        app,
        state.ai_manager.clone(),
        state.agent_orchestrator.clone(),
        handle.inner().clone(),
        PathBuf::from(theme_path.trim()),
        name,
        idea,
        budget_secs,
        provider,
        model,
        wp_profile(),
    )
}

/// Millora autopilotada de QUALSEVOL projecte (perfil genèric + autodetecció).
#[tauri::command]
pub async fn project_work(
    app: AppHandle,
    state: State<'_, AppState>,
    handle: State<'_, Arc<ProjectWorkHandle>>,
    project_path: String,
    name: String,
    idea: String,
    budget_secs: u64,
    provider: Option<String>,
    model: Option<String>,
) -> Result<(), String> {
    let root = PathBuf::from(project_path.trim());
    let profile = detect_profile(&root, &idea);
    start_work(
        app,
        state.ai_manager.clone(),
        state.agent_orchestrator.clone(),
        handle.inner().clone(),
        root,
        name,
        idea,
        budget_secs,
        provider,
        model,
        profile,
    )
}

/// Atura la millora netament.
#[tauri::command]
pub fn project_work_stop(handle: State<'_, Arc<ProjectWorkHandle>>) {
    if let Ok(c) = handle.cancel.lock() {
        c.cancel();
    }
}

/// Estat actual de la millora.
#[tauri::command]
pub fn project_work_state(handle: State<'_, Arc<ProjectWorkHandle>>) -> ProjectWorkState {
    handle.snapshot()
}

#[allow(clippy::too_many_arguments)]
async fn run_loop(
    app: AppHandle,
    handle: Arc<ProjectWorkHandle>,
    ai: Arc<AiManager>,
    orch: Arc<AgentOrchestrator>,
    root: PathBuf,
    name: String,
    idea: String,
    budget: Duration,
    provider: Option<String>,
    model: Option<String>,
    token: CancellationToken,
    profile: WorkProfile,
) {
    let start = Instant::now();
    let deadline = start.checked_add(budget).unwrap_or(start);
    let mut pass = 0u32;
    let mut files_changed = 0u32;
    // Bucle de verificació: errors de `php -l` de la passada anterior que es
    // realimenten a la següent perquè la IA els corregeixi ABANS de millorar.
    let mut lint_feedback = String::new();
    // El treball autopilotat pot durar dies sense que l'usuari toqui res:
    // bloquem el son del SO mentre el guard visqui (s'allibera en acabar).
    let _awake = crate::keep_awake::activar("treball autopilotat");

    loop {
        if token.is_cancelled() || Instant::now() >= deadline {
            break;
        }
        pass += 1;
        let focus = profile.focuses[(pass as usize - 1) % profile.focuses.len()];
        handle.set(|s| {
            s.pass = pass;
            s.elapsed_ms = start.elapsed().as_millis() as u64;
            s.detail = format!("Passada {pass}: cercant millores de {focus}");
        });
        emit_state(&app, &handle);

        // 1) Re-llegeix el projecte: les millores de passades anteriors s'acumulen.
        let files = collect_project_files(&root, profile.allowed_ext);

        // 2) AUTOPILOTATGE: recerca web headless (mai finestra ni permís).
        let query = format!("{} {} {}", profile.query_prefix, focus, name);
        let research = tokio::time::timeout(Duration::from_secs(60), orch.research_headless(&query, 2))
            .await
            .unwrap_or_default();

        // 3) Demana les millores a la IA (xat CRU: sense eines ni confirmacions).
        // `long_run`: models locals en CPU poden trigar mitja hora llarga per
        // resposta; el sostre real és el pressupost total (`deadline`).
        // Eines de motor: si el projecte és d'Unreal (o té un .uproject a dins)
        // o Blender és a l'abast, la IA pot dirigir-los amb directrius. La
        // secció s'afegeix al prompt perquè el model ho sàpiga i decideixi.
        let engine = crate::engine_tools::EngineContext::detect(&root);
        let mut prompt = build_pass_prompt(&name, &idea, focus, pass, &files, &research);
        if engine.any() {
            prompt.push_str(&crate::engine_tools::directives_help(&engine));
        }
        // Errors de lint de la passada anterior: entren al prompt perquè la
        // IA els corregeixi en aquesta passada (verificació obligatòria).
        prompt.push_str(&lint_feedback);
        let opts = ChatOpts {
            provider: provider.clone(),
            model: model.clone(),
            long_run: true,
            ..Default::default()
        };
        let resp = tokio::select! {
            _ = token.cancelled() => break,
            r = tokio::time::timeout(Duration::from_secs(45 * 60), ai.chat_opts(&prompt, Some(profile.system), &opts)) => r,
        };
        let mut text = match resp {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => {
                // Un error de passada NO atura el bucle: es continua autopilotat
                // fins omplir el temps (resiliència sense intervenció).
                handle.set(|s| {
                    s.detail = format!("Passada {pass}: la IA ha fallat ({e}); continue amb la següent.");
                });
                emit_state(&app, &handle);
                continue;
            }
            Err(_) => {
                handle.set(|s| s.detail = format!("Passada {pass}: temps esgotat; continue."));
                emit_state(&app, &handle);
                continue;
            }
        };
        if token.is_cancelled() {
            break;
        }

        // 3bis) Eines de motor: les directrius (@unreal:, @blender:…) de la
        // resposta s'executen DE VERITAT contra Unreal/Blender i els
        // resultats tornen al model, que pot demanar-ne més o acabar amb
        // els fitxers. Les operacions queden a EngineWork/ del projecte.
        let mut tool_ok = 0u32;
        if engine.any() {
            let mut tool_context = String::new();
            for _round in 0..crate::engine_tools::MAX_TOOL_ROUNDS {
                let directives = crate::engine_tools::parse_directives(&text);
                if directives.is_empty() {
                    break;
                }
                let mut outcomes = Vec::new();
                for d in &directives {
                    handle.set(|s| {
                        s.detail = format!(
                            "Passada {pass}: executant l'eina de motor «{}»…",
                            d.kind.name()
                        );
                    });
                    emit_state(&app, &handle);
                    let o = crate::engine_tools::execute(&engine, d).await;
                    if o.ok {
                        tool_ok += 1;
                    }
                    handle.set(|s| {
                        s.detail = format!(
                            "Passada {pass}: eina {} — {}",
                            o.kind,
                            crate::engine_tools::clip(&o.summary, 160)
                        );
                    });
                    emit_state(&app, &handle);
                    outcomes.push(o);
                }
                tool_context.push_str(&crate::engine_tools::format_results(&outcomes));
                let follow = format!(
                    "{prompt}\n{tool_context}\nLes directrius d'eines ja s'han executat (els resultats són a sobre). \
Ara COMPLETA la passada: escriu els fitxers modificats amb «@file:» o «@edit:» i acaba amb «RESUM: …». \
Si et calen més eines de motor, emet més directrius."
                );
                let resp2 = tokio::select! {
                    _ = token.cancelled() => break,
                    r = tokio::time::timeout(Duration::from_secs(45 * 60), ai.chat_opts(&follow, Some(profile.system), &opts)) => r,
                };
                match resp2 {
                    Ok(Ok(t)) => text = t,
                    Ok(Err(e)) => {
                        handle.set(|s| {
                            s.detail = format!(
                                "Passada {pass}: la IA ha fallat en el seguiment d'eines ({e})."
                            );
                        });
                        emit_state(&app, &handle);
                        break;
                    }
                    Err(_) => break,
                }
            }
        }
        if token.is_cancelled() {
            break;
        }

        // 4) Materialitza els fitxers millorats (escriptura directa segura).
        let blocks = crate::agent::runtools::extract_file_blocks(&text);
        let mut n = 0u32;
        let mut touched: Vec<String> = Vec::new();
        for (rel, content) in blocks {
            let Some(rel) = safe_rel(&rel) else { continue };
            if content.trim().is_empty() {
                continue;
            }
            let path = root.join(&rel);
            if let Some(par) = path.parent() {
                let _ = std::fs::create_dir_all(par);
            }
            if std::fs::write(&path, content).is_ok() {
                n += 1;
                touched.push(rel);
            }
        }
        // SEGONA VIA: si el model no ha reeixit a tornar fitxers sencers (molt
        // normal en models petits que tallen per longitud), acceptem les
        // edicions quirúrgiques CERCA/NOU. Sense això la passada feia «0
        // fitxers millorats» i el projecte no canviava mai res.
        if n == 0 {
            let edits = crate::agent::runtools::extract_edit_blocks(&text);
            if !edits.is_empty() {
                let report = crate::agent::runtools::apply_edit_blocks_at(&root, &text);
                // Només compten les edicions REALMENT aplicades (no els errors).
                n = report.iter().filter(|r| r.contains("aplicades")).count() as u32;
                for b in &edits {
                    if report.iter().any(|r| r.contains("aplicades") && r.starts_with(b.path.as_str())) {
                        touched.push(b.path.clone());
                    }
                }
            }
        }
        // 4bis) VERIFICACIÓ OBLIGATÒRIA: `php -l` sobre els .php i
        // `node --check` sobre els .js/.mjs/.cjs tocats pel lot. Si falla,
        // l'error es realimenta a la passada següent i NO es reporta
        // «millora aplicada» (l'informe/progrés mostra l'error).
        let lint_touched: Vec<String> = touched
            .iter()
            .filter(|p| {
                let low = p.to_ascii_lowercase();
                low.ends_with(".php")
                    || low.ends_with(".js")
                    || low.ends_with(".mjs")
                    || low.ends_with(".cjs")
            })
            .cloned()
            .collect();
        let mut lint_errors: Vec<crate::agent::runtools::LintResult> = Vec::new();
        if !lint_touched.is_empty() {
            handle.set(|s| {
                s.detail = format!("Passada {pass}: verificant la sintaxi (php -l / node --check)…");
            });
            emit_state(&app, &handle);
            let root_lint = root.clone();
            let results = tokio::task::spawn_blocking(move || {
                crate::agent::runtools::lint_paths_blocking(&root_lint, &lint_touched)
            })
            .await
            .unwrap_or_default();
            lint_errors = results.into_iter().filter(|r| !r.ok).collect();
        }
        // Decisió PURA del gate de verificació (compartida amb els tests):
        // amb lint fallant el lot NO es compta com a millora i l'error real
        // es realimenta a la passada següent.
        let gate = verification_gate(pass, n, tool_ok, &lint_errors);
        files_changed += gate.counted;
        match gate.feedback {
            Some(fb) => lint_feedback = fb,
            None => lint_feedback.clear(),
        }
        let resum = text
            .lines()
            .find(|l| l.trim_start().to_ascii_uppercase().starts_with("RESUM:"))
            .unwrap_or("")
            .trim()
            .to_string();
        let lint_detail = gate.detail;
        handle.set(|s| {
            s.pass = pass;
            s.files_changed = files_changed;
            s.elapsed_ms = start.elapsed().as_millis() as u64;
            s.detail = if !lint_detail.is_empty() {
                lint_detail.clone()
            } else if resum.is_empty() {
                format!("Passada {pass} ({focus}): {n} fitxers millorats")
            } else {
                resum.clone()
            };
        });
        emit_state(&app, &handle);
    }

    // Final (temps omplert o aturat): l'estat es conserva perquè la UI el veja.
    let now = chrono::Local::now().timestamp();
    handle.running.store(false, Ordering::SeqCst);
    handle.set(|s| {
        s.running = false;
        s.finished_at = Some(now);
        s.elapsed_ms = start.elapsed().as_millis() as u64;
        s.detail = if token.is_cancelled() {
            "Millora aturada per l'usuari.".into()
        } else {
            "Temps de treball completat: projecte millorat.".into()
        };
    });
    emit_state(&app, &handle);
}

/// Resultat del gate de VERIFICACIÓ OBLIGATÒRIA d'una passada d'autopilotatge.
struct VerifyGate {
    /// Fitxers comptats com a feina feta: amb lint fallant el lot NO compta.
    counted: u32,
    /// Realimentació per a la passada següent (Some només si el lint falla).
    feedback: Option<String>,
    /// Detall per al progrés/UI (buit si no hi ha errors).
    detail: String,
}

/// Decisió PURA del gate de verificació de `project_work`: si hi ha errors
/// de `php -l`, el lot escrit NO es compta com a millora aplicada (només
/// compten les operacions de motor) i l'error REAL es realimenta a la
/// passada següent. Mateix codi que el bucle async, testeable sense Tauri.
fn verification_gate(
    pass: u32,
    n: u32,
    tool_ok: u32,
    lint_errors: &[crate::agent::runtools::LintResult],
) -> VerifyGate {
    if lint_errors.is_empty() {
        // Les operacions de motor també compten com a feina feta.
        return VerifyGate { counted: n + tool_ok, feedback: None, detail: String::new() };
    }
    // El lot NO es compta com a millora aplicada: el lint falla.
    let mut fb = String::from(
        "\n\nVERIFICACIÓ FALLIDA DE LA PASSADA ANTERIOR (php -l). ABANS de qualsevol \
         millora nova, CORREGEIX aquests errors de sintaxi reemetent els fitxers \
         afectats amb «@file:» (codi COMPLET corregit):\n",
    );
    for e in lint_errors.iter().take(6) {
        fb.push_str(&format!(
            "— {}: {}\n",
            e.path,
            crate::agent::runtools::clip_head_tail(&e.output, 300, 500)
        ));
    }
    let detail = format!(
        "Passada {pass}: verificació PHP FALLIDA ({} error(s)) — no es compta com a millora; es corregeix a la passada següent. {}",
        lint_errors.len(),
        crate::engine_tools::clip(&lint_errors[0].output, 160)
    );
    VerifyGate { counted: tool_ok, feedback: Some(fb), detail }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_rel_rebutja_paths_perillosos() {
        assert_eq!(safe_rel("style.css").as_deref(), Some("style.css"));
        assert_eq!(safe_rel("inc/custom.php").as_deref(), Some("inc/custom.php"));
        assert!(safe_rel("../fora.php").is_none());
        assert!(safe_rel("/absolut.php").is_none());
        assert!(safe_rel("a:../b.php").is_none());
        assert!(safe_rel("   ").is_none());
    }

    #[test]
    fn safe_rel_normalitza_barres_invertides() {
        assert_eq!(safe_rel("inc\\custom.php").as_deref(), Some("inc/custom.php"));
    }

    #[test]
    fn budget_es_retalla_al_maxim() {
        assert_eq!(MAX_WORK_SECS, 7 * 24 * 3600);
        let huge = u64::MAX.clamp(60, MAX_WORK_SECS);
        assert_eq!(huge, MAX_WORK_SECS);
        let tiny = 1u64.clamp(60, MAX_WORK_SECS);
        assert_eq!(tiny, 60);
    }

    #[test]
    fn collect_project_files_llegeix_i_limita() {
        let dir = std::env::temp_dir().join(format!("noorbit-pw-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("inc"));
        let _ = std::fs::create_dir_all(dir.join("node_modules"));
        std::fs::write(dir.join("style.css"), "/* Theme Name: T */").unwrap();
        std::fs::write(dir.join("inc/a.php"), "<?php echo 1;").unwrap();
        std::fs::write(dir.join("node_modules/b.js"), "no s'ha de llegir").unwrap();
        std::fs::write(dir.join("binari.bin"), "ignorat").unwrap();
        let files = collect_project_files(&dir, &WP_EXT);
        let rels: Vec<&str> = files.iter().map(|(r, _)| r.as_str()).collect();
        assert!(rels.contains(&"style.css"));
        assert!(rels.contains(&"inc/a.php"));
        assert!(!rels.iter().any(|r| r.contains("node_modules")));
        assert!(!rels.iter().any(|r| r.ends_with(".bin")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_generic_llegeix_ts_i_py() {
        let dir = std::env::temp_dir().join(format!("noorbit-pw-gen-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("main.ts"), "export const x = 1;").unwrap();
        std::fs::write(dir.join("app.py"), "print(1)").unwrap();
        std::fs::write(dir.join("foto.png"), "binari").unwrap();
        let files = collect_project_files(&dir, &GEN_EXT);
        let rels: Vec<&str> = files.iter().map(|(r, _)| r.as_str()).collect();
        assert!(rels.contains(&"main.ts"));
        assert!(rels.contains(&"app.py"));
        assert!(!rels.iter().any(|r| r.ends_with(".png")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prompt_inclou_focus_i_fitxers() {
        let files = vec![("style.css".to_string(), "body{}".to_string())];
        let p = build_pass_prompt("T", "idea", "SEO", 2, &files, "INFO WEB");
        assert!(p.contains("PASSADA 2"));
        assert!(p.contains("SEO"));
        assert!(p.contains("@file: style.css"));
        assert!(p.contains("INFO WEB"));
        assert!(p.contains("RESUM"));
    }

    #[test]
    fn detect_profile_tria_wordpress_o_generic() {
        let dir = std::env::temp_dir().join(format!("noorbit-pw-det-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // Carpeta buida → genèric.
        assert!(!detect_profile(&dir, "un joc").system.contains("WordPress"));
        // style.css amb Theme Name → WordPress.
        std::fs::write(dir.join("style.css"), "/*\nTheme Name: El meu tema\n*/").unwrap();
        assert!(detect_profile(&dir, "un joc").system.contains("WordPress"));
        // Idea explícita → WordPress encara que no hi haja style.css.
        let _ = std::fs::remove_file(dir.join("style.css"));
        assert!(detect_profile(&dir, "un tema WordPress").system.contains("WordPress"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn perfils_tenen_set_focusos() {
        assert_eq!(wp_profile().focuses.len(), 7);
        assert_eq!(generic_profile().focuses.len(), 7);
        assert!(generic_profile().allowed_ext.contains(&"rs"));
        assert!(generic_profile().allowed_ext.contains(&"gd"));
    }

    // ── REGRESSIÓ: cap camí autònom reporta èxit sense verificació ──────

    /// `php -l` FALLANT sobre un fitxer real: el lot no compta com a millora
    /// (només les operacions de motor) i l'error real es realimenta.
    #[test]
    fn lint_php_fallant_no_compta_com_millora() {
        if crate::agent::runtools::php_binary().is_none() {
            eprintln!("php no disponible en aquesta màquina: test omès");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("trencat.php"), "<?php function( {\n").unwrap();
        let results = crate::agent::runtools::lint_paths_blocking(
            dir.path(),
            &["trencat.php".to_string()],
        );
        let errors: Vec<_> = results.into_iter().filter(|r| !r.ok).collect();
        assert_eq!(errors.len(), 1, "php -l ha de detectar l'error");
        // Passada amb 3 fitxers escrits + 1 operació de motor: amb lint
        // fallant NOMÉS compta l'operació de motor.
        let gate = verification_gate(2, 3, 1, &errors);
        assert_eq!(gate.counted, 1, "el lot amb lint fallant NO és una millora");
        let fb = gate.feedback.expect("feedback per a la passada següent");
        assert!(fb.contains("VERIFICACIÓ FALLIDA"), "feedback: {fb}");
        assert!(fb.contains("CORREGEIX"), "feedback: {fb}");
        assert!(fb.contains("trencat.php"), "feedback: {fb}");
        assert!(fb.to_lowercase().contains("error"), "feedback: {fb}");
        assert!(gate.detail.contains("FALLIDA"), "detail: {}", gate.detail);
        assert!(gate.detail.contains("no es compta com a millora"), "detail: {}", gate.detail);
    }

    /// `php -l` verd sobre un fitxer real: el lot SÍ compta i no hi ha
    /// feedback d'errors (la passada següent comença neta).
    #[test]
    fn lint_php_verd_compta_el_lot_i_netega_feedback() {
        if crate::agent::runtools::php_binary().is_none() {
            eprintln!("php no disponible en aquesta màquina: test omès");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bo.php"), "<?php echo 1;\n").unwrap();
        let results = crate::agent::runtools::lint_paths_blocking(
            dir.path(),
            &["bo.php".to_string()],
        );
        let errors: Vec<_> = results.into_iter().filter(|r| !r.ok).collect();
        assert!(errors.is_empty());
        let gate = verification_gate(3, 4, 2, &errors);
        assert_eq!(gate.counted, 6, "lot + operacions de motor");
        assert!(gate.feedback.is_none());
        assert!(gate.detail.is_empty());
    }

    /// Gate sintètic (sense PHP): sostre de 6 errors realimentats i detall
    /// amb el PRIMER error; sense errors de lint, tot compta.
    #[test]
    fn gate_sintetic_realimenta_fins_a_sis_errors() {
        let err = |i: usize| crate::agent::runtools::LintResult {
            path: format!("f{i}.php"),
            ok: false,
            output: format!("PHP Parse error: syntax error {i}"),
            checked: true,
        };
        let vuit: Vec<_> = (0..8).map(err).collect();
        let gate = verification_gate(1, 8, 0, &vuit);
        assert_eq!(gate.counted, 0, "cap millora amb lint fallant");
        let fb = gate.feedback.unwrap();
        assert_eq!(fb.matches("— f").count(), 6, "màxim 6 errors al feedback: {fb}");
        assert!(fb.contains("f0.php") && !fb.contains("f7.php"));
        assert!(gate.detail.contains("8 error(s)"), "detail: {}", gate.detail);
        assert!(gate.detail.contains("syntax error 0"), "detail: {}", gate.detail);
        // Sense errors: el lot compta sencer i no hi ha res a realimentar.
        let ok = verification_gate(1, 8, 3, &[]);
        assert_eq!(ok.counted, 11);
        assert!(ok.feedback.is_none() && ok.detail.is_empty());
    }
}