//! Mòdul «Autonomous Jam» — el bucle autònom integrat a NoOrbit.
//!
//! Reutilitza els crates de `crates/` (guardian, jam, planner, git, worktree,
//! reporter) però amb les PECES REALS de NoOrbit:
//!
//! - `NoOrbitLlm`: implementa `LlmClient` amb l'`AiManager` de l'app, de
//!   manera que la Jam usa **exactament la IA que l'usuari té connectada**
//!   (Ollama/llama.cpp locals, OpenAI, Claude, Gemini, DeepSeek, GLM…).
//! - `NoOrbitWorker`: demana codi al model, n'extreu els blocs `@file:` i
//!   els **materialitza** al working tree (mateix mecanisme que el xat).
//! - `NoOrbitVerifier`: comprova els criteris d'acceptació (fitxers creats,
//!   no buits) i, si el projecte té un script de build, l'executa.
//!
//! L'agent corre en un SUBPROCÉS (mode `--jam-agent`) supervisat pel
//! `guardian`: si s'embala de memòria, mor el fill i el supervisor el
//! reinicia sense tocar la UI.

pub mod agent;
pub mod commands;
#[cfg(test)]
mod agent_tests;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use noorbit_jam::{
    Cache, Diff, Plan, Progress, ProgressSink, Reporter, Task, TaskContext, Verifier, Worker,
};
use noorbit_planner::{Brief, Engine, LlmClient, Planner};

use crate::ai::{AiManager, ChatOpts};

/// Durada MÀXIMA d'una Jam: 7 dies. Les feines multi-dia (un joc complex que
/// es construeix al llarg de dies) necessiten un sostre molt per damunt de les
/// 24 h antigues. El planner, el pare i el fill fan servir EL MATEIX límit
/// perquè la durada no es retalle de manera diferent en cada extrem.
pub const MAX_JAM_DURATION_SECS: u64 = 7 * 24 * 3600;

/// Estat d'una Jam en curs (per a la UI).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JamState {
    pub running: bool,
    pub title: String,
    pub completed: u32,
    pub blocked: u32,
    pub skipped: u32,
    /// Total de tasques del pla que s'està EXECUTANT. La UI no el pot deduir
    /// del pla que va mostrar (es buida en aturar), i sense això la barra de
    /// progrés es recalculava amb un denominador incorrecte.
    pub total: u32,
    pub current_task: String,
    pub current_detail: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    /// Durada màxima triada per l'usuari (en segons). La UI la fa servir per
    /// calcular el SOSTRE de l'ETA i l'hora de fin estimada (la Jam s'atura sol
    /// en arribar a `started_at + duration_secs`).
    pub duration_secs: u64,
    pub report_path: String,
    /// Projecte (arrel del repo) on la Jam està treballant. Sense això, quan
    /// la Jam acabava l'usuari no podia saber QUIN projecte havia tocat —
    /// sobretot si té diverses finestres de NoOrbit obertes.
    pub project_path: String,
    pub error: Option<String>,
    /// Proveïdor i model d'IA que fa servir la Jam. La UI els mostra: sense
    /// això l'usuari no sabia QUI li estava construint el joc.
    pub provider: String,
    pub model: String,
    /// L'usuari ha demanat aturar la Jam i encara no s'ha aturat del tot
    /// (l'agent pot estar enmig d'una tasca). La UI hi posa «aturant…».
    pub stopping: bool,
}

impl Default for JamState {
    fn default() -> Self {
        Self {
            running: false,
            title: String::new(),
            completed: 0,
            blocked: 0,
            skipped: 0,
            total: 0,
            current_task: String::new(),
            current_detail: String::new(),
            started_at: 0,
            finished_at: None,
            duration_secs: 0,
            report_path: String::new(),
            project_path: String::new(),
            error: None,
            provider: String::new(),
            model: String::new(),
            stopping: false,
        }
    }
}

/// Esdeveniments que s'emeten a la UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JamEvent {
    pub kind: String,
    pub task: String,
    pub detail: String,
    pub ok: bool,
}

pub fn emit(app: &AppHandle, kind: &str, task: &str, detail: &str, ok: bool) {
    let _ = app.emit(
        "jam://event",
        JamEvent {
            kind: kind.to_string(),
            task: task.to_string(),
            detail: detail.to_string(),
            ok,
        },
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//  LLM: parla amb l'IA que l'usuari té connectada
// ─────────────────────────────────────────────────────────────────────────────

/// Client que usa l'`AiManager` de NoOrbit (proveïdor actiu de l'app).
pub struct NoOrbitLlm {
    ai: Arc<AiManager>,
}

impl NoOrbitLlm {
    pub fn new(ai: Arc<AiManager>) -> Self {
        Self { ai }
    }
}

#[async_trait::async_trait]
impl LlmClient for NoOrbitLlm {
    async fn complete(&self, prompt: &str) -> Result<String, String> {
        // `long_run`: som en segon pla, ningú mira la pantalla. Els clients
        // d'120–240 s del xat humà tallaven la generació d'un fitxer sencer
        // amb un model local i la Jam mai arribava a escriure res.
        self.ai
            .chat_opts(
                prompt,
                None,
                &ChatOpts {
                    long_run: true,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| e.to_string())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  WORKER: demana codi i materialitza els fitxers
// ─────────────────────────────────────────────────────────────────────────────

/// Prompt del worker: es demana codi amb blocs `@file:` (el mateix format
/// que usa el xat de NoOrbit), i el model ha de llistar els criteris que
/// pot comprovar.
fn worker_prompt(task: &Task, root: &Path, lessons: &str, quality: noorbit_jam::QualityLevel, engine_help: &str) -> String {
    let acceptance = task
        .acceptance
        .iter()
        .map(|a| format!("- {a}"))
        .collect::<Vec<_>>()
        .join("\n");
    let listing = std::fs::read_dir(root)
        .ok()
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_file())
                .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                .take(60)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();

    format!(
        "Ets un programador expert. Implementa aquesta tasca en el projecte obert.\n\n\
         TASCA: {title}\n\
         DESCRIPCIÓ: {description}\n\n\
         CRITERIS D'ACCEPTACIÓ (ha de complir):\n{acceptance}\n\n\
         FITXERS A L'ARREL DEL PROJECTE: {listing}\n\n\
         {quality_hint}{engine_help}\n\n\
         REGLES ESTRICTES:\n\
         1. Retorna el codi amb blocs «@file: ruta/relativa» seguits del codi.\n\
         2. Només paths RELATIUS dins del projecte (mai «..», mai absoluts).\n\
         3. Toca els mínims fitxers possibles; no reescriguis tot el projecte.\n\
         4. Per MODIFICAR un fitxer que ja existeix, si surt més curt que\n\
            reescriure'l sencer: una línia «@edit: ruta», i per cada canvi\n\
            una línia «<<<<<<< CERCA», el codi antic exacte, una línia\n\
            «=======», el codi nou (buit = esborrar) i una línia «>>>>>>>».\n\
         5. Si la tasca no es pot fer, explica-ho en 3 línies i no escriguis fitxers.\n\
         6. Al final, escriu una línia «RESUM: …» amb què has fet.\n\
         {lessons}",
        title = task.title,
        description = task.description,
        acceptance = acceptance,
        listing = listing,
        quality_hint = match quality {
            noorbit_jam::QualityLevel::Full => "Qualitat: COMPLETA. Pots escriure fins a 300 línies per fitxer.",
            noorbit_jam::QualityLevel::Reduced => "Qualitat: REDUÏDA. Escriu només el mínim necessari (≤150 línies). Evita comentaris llargs.",
            noorbit_jam::QualityLevel::Minimal => "Qualitat: MÍNIMA. Escriu el codi més simple possible (≤80 línies). Només el que fa la tasca.",
        },
        engine_help = engine_help,
        lessons = lessons,
    )
}

/// Worker que materialitza els fitxers que el model anuncia.
pub struct NoOrbitWorker {
    llm: Arc<dyn LlmClient>,
    root: PathBuf,
    lessons: Option<Arc<noorbit_lessons::LessonsStore>>,
    /// Recerca web AUTOPILOTADA (headless): si hi és, el worker s'informa sol
    /// a internet abans d'escriure codi, sense que l'usuari toque res.
    research: Option<Arc<crate::agent::AgentOrchestrator>>,
    /// Emissor de «què fa la IA» i del seu pensament cap a la UI. El worker
    /// corre al SUBPROCÉS, així que l'únic canal és el `ProgressSink` (que
    /// `StdoutProgress` tradueix a línies `JAM|`).
    progress: Option<Arc<dyn ProgressSink>>,
}

impl NoOrbitWorker {
    pub fn new(llm: Arc<dyn LlmClient>, root: PathBuf) -> Self {
        Self {
            llm,
            root,
            lessons: None,
            research: None,
            progress: None,
        }
    }

    pub fn with_lessons(mut self, lessons: Arc<noorbit_lessons::LessonsStore>) -> Self {
        self.lessons = Some(lessons);
        self
    }

    /// Habilita la recerca web autònoma (autopilotatge) per a cada tasca.
    pub fn with_research(mut self, orch: Arc<crate::agent::AgentOrchestrator>) -> Self {
        self.research = Some(orch);
        self
    }

    /// Habilita l'emissió de notes i pensament a la UI (què està fent la IA).
    pub fn with_progress(mut self, progress: Arc<dyn ProgressSink>) -> Self {
        self.progress = Some(progress);
        self
    }

    /// Envia un esdeveniment de progrés si hi ha emissor (si no, no fa res).
    async fn emit(&self, ev: Progress) {
        if let Some(sink) = &self.progress {
            sink.on_progress(ev).await;
        }
    }
}

/// Ruta relativa segura dins el projecte (mai «..», mai absoluta).
fn safe_rel(p: &str) -> Option<String> {
    let t = p.trim().replace('\\', "/");
    if t.is_empty() || t.starts_with('/') || t.split('/').any(|s| s == "..") || t.contains(':') {
        return None;
    }
    Some(t)
}

/// Extrau el «pensament» del model: la prosa i la línia RESUM, SENS ELS
/// blocs de codi (` ``` `) ni els marcadors `@file:`/`@edit:` i de bloc
/// d'edició. Es retalla a ~1200 caràcters perquè la UI no s'ompli de codi.
fn thinking_excerpt(response: &str) -> String {
    const MAX: usize = 1200;
    let mut out = String::new();
    let mut in_code = false;
    for line in response.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        // Marcadors del protocol d'escriptura: no són pensament.
        let end_marker = ">".repeat(7);
        let is_marker = t.starts_with("@file:")
            || t.starts_with("@edit:")
            || t.starts_with("@unreal:")
            || t.starts_with("@unreal-py:")
            || t.starts_with("@blender:")
            || t.starts_with("@blender-headless:")
            || t.starts_with("@blender-video:")
            || t.starts_with("@godot:")
            || t.starts_with("@bevy:")
            || t.starts_with("@import:")
            || t.starts_with("<<<<<<<")
            || t.starts_with(&end_marker)
            || t == "=======";
        let is_resum = t.starts_with("RESUM:") || t.starts_with("RESUM ");
        if is_marker || (t.is_empty() && !is_resum) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        if out.len() >= MAX {
            break;
        }
    }
    let mut out = out.trim().to_string();
    if out.chars().count() > MAX {
        let cut: String = out.chars().take(MAX).collect();
        out = format!("{}…", cut.trim_end());
    }
    out
}

#[async_trait::async_trait]
impl Worker for NoOrbitWorker {
    async fn run(&self, task: &Task, ctx: &TaskContext) -> Result<Diff, String> {
        // Lliçons: injecta intents fallits anteriors (si n'hi ha).
        let lessons = if let Some(store) = &self.lessons {
            let ls = store
                .recent(Some(noorbit_jam::Phase::General), 5, 4000)
                .await;
            noorbit_lessons::LessonsStore::format_for_prompt(&ls)
        } else {
            String::new()
        };

        // Eines de motor: si la carpeta del projecte és (o conté) un projecte
        // d'Unreal o hi ha Blender a l'abast, el model pot dirigir-los amb
        // directrius (@unreal:, @blender:…). Es detecta a cada tasca perquè
        // l'usuari pot obrir Blender/Unreal a mitja Jam.
        let engine = crate::engine_tools::EngineContext::detect(&self.root);
        let engine_help = if engine.any() {
            crate::engine_tools::directives_help(&engine)
        } else {
            String::new()
        };
        let prompt = worker_prompt(task, &self.root, &lessons, ctx.quality, &engine_help);
        // AUTOPILOTATGE: la IA cerca informació fresca a internet (documentació,
        // APIs, solucions) abans d'escriure codi. Headless: mai obri finestres
        // ni demana res a l'usuari. Amb timeout perquè una cerca lenta no
        // encalle la tasca; si falla, es continua sense el context web.
        let mut prompt = prompt;
        if let Some(orch) = &self.research {
            self.emit(Progress::Note {
                id: task.id.clone(),
                text: "cercant informació a internet per fonamentar la tasca…".into(),
            })
            .await;
            let q = format!("{} {}", task.title, task.description);
            if let Ok(info) = tokio::time::timeout(
                Duration::from_secs(60),
                orch.research_headless(&q, 2),
            )
            .await
            {
                if !info.trim().is_empty() {
                    prompt.push_str("\n\n");
                    prompt.push_str(&info);
                }
            }
        }
        self.emit(Progress::Note {
            id: task.id.clone(),
            text: format!("generant codi per «{}»…", task.title),
        })
        .await;
        let mut response = self.llm.complete(&prompt).await.map_err(|e| e.to_string())?;

        // PROCÉS DE PENSAMENT: es mostra a la UI el raonament del model
        // (prosa + RESUM, sense blocs de codi) perquè l'usuari vegi QUÈ
        // està fent la IA, no només el títol de la tasca.
        let thought = thinking_excerpt(&response);
        if !thought.is_empty() {
            self.emit(Progress::Thinking {
                id: task.id.clone(),
                text: thought,
            })
            .await;
        }

        // ── Eines de motor: el model pot dirigir Unreal/Blender ─────────────
        // Cada directriu s'executa DE VERITAT contra el motor i el resultat
        // torna al model, que pot demanar-ne més o escriure els fitxers
        // finals. Les operacions queden registrades a EngineWork/ del projecte.
        let mut tool_files: Vec<String> = Vec::new();
        if engine.any() {
            let mut tool_context = String::new();
            for _round in 0..crate::engine_tools::MAX_TOOL_ROUNDS {
                let directives = crate::engine_tools::parse_directives(&response);
                if directives.is_empty() {
                    break;
                }
                let mut outcomes = Vec::new();
                for d in &directives {
                    self.emit(Progress::Note {
                        id: task.id.clone(),
                        text: format!("executant l'eina de motor «{}»…", d.kind.name()),
                    })
                    .await;
                    let o = crate::engine_tools::execute(&engine, d).await;
                    tool_files.extend(o.artifacts.iter().filter_map(|p| {
                        p.strip_prefix(&self.root)
                            .ok()
                            .map(|r| r.to_string_lossy().replace('\\', "/"))
                    }));
                    self.emit(Progress::Note {
                        id: task.id.clone(),
                        text: format!(
                            "eina {}: {}",
                            o.kind,
                            crate::engine_tools::clip(&o.summary, 200)
                        ),
                    })
                    .await;
                    outcomes.push(o);
                }
                tool_context.push_str(&crate::engine_tools::format_results(&outcomes));
                let follow = format!(
                    "{prompt}\n{tool_context}\nLes directrius d'eines ja s'han executat (els resultats són a sobre). \
Ara COMPLETA la tasca: escriu els fitxers que calgui amb «@file:» o «@edit:» i acaba amb «RESUM: …». \
Si et calen més eines de motor, emet més directrius."
                );
                response = match self.llm.complete(&follow).await {
                    Ok(r) => r,
                    Err(e) => {
                        self.emit(Progress::Note {
                            id: task.id.clone(),
                            text: format!(
                                "la IA ha fallat en el seguiment d'eines ({e}); es conserva la feina de motor feta"
                            ),
                        })
                        .await;
                        break;
                    }
                };
            }
            tool_files.sort();
            tool_files.dedup();
        }

        // Extracció dels blocs «@file:» (mateix motor que el xat).
        let blocks = crate::agent::runtools::extract_file_blocks(&response);
        if blocks.is_empty() {
            // SEGONA VIA: els models petits tallen fitxers sencers o els
            // inventen; amb «@edit:» (CERCA/NOU quirúrgic) sí que saben
            // MODIFICAR codi existent. Si n'hi ha, la tasca COMPTA com a
            // escrita — abans es declarava «el model no ha retornat cap
            // fitxer» i la Jam moria sense haver canviat res.
            let edits = crate::agent::runtools::extract_edit_blocks(&response);
            if !edits.is_empty() {
                let report = crate::agent::runtools::apply_edit_blocks_at(&self.root, &response);
                if !report.is_empty() {
                    let touched: Vec<String> = edits
                        .iter()
                        .filter_map(|b| safe_rel(&b.path))
                        .collect();
                    return Ok(Diff::materialized_files(touched, 0));
                }
            }
            // NOVA SORTIDA: la tasca pot ser de MOTOR (actors, escenes,
            // vídeos renderitzats) sense cap fitxer de codi; els artefactes
            // d'eines (EngineWork/) són la feina feta i compten com a tal.
            if !tool_files.is_empty() {
                return Ok(Diff::materialized_files(tool_files, 0));
            }
            if let Some(store) = &self.lessons {
                let _ = store
                    .append(noorbit_jam::Phase::General, &task.id, "el model no ha retornat cap fitxer (@file:)", "")
                    .await;
            }
            return Err("el model no ha retornat cap fitxer (@file:)".into());
        }

        let mut written = Vec::new();
        for (rel, content) in blocks {
            let Some(rel) = safe_rel(&rel) else { continue };
            if ctx.cancel.is_cancelled() {
                return Err("memory: cancel·lat".into());
            }
            let path = self.root.join(&rel);
            if let Some(par) = path.parent() {
                let _ = std::fs::create_dir_all(par);
            }
            if let Err(e) = std::fs::write(&path, content) {
                if let Some(store) = &self.lessons {
                    let _ = store
                        .append(
                            noorbit_jam::Phase::General,
                            &task.id,
                            &format!("escriptura {rel}: {e}"),
                            &response,
                        )
                        .await;
                }
                return Err(format!("escriptura {rel}: {e}"));
            }
            written.push(rel);
        }
        // Els artefactes de les eines de motor també formen part del patch:
        // el verificador i la UI han de veure que la tasca ha fet feina real.
        written.extend(tool_files.clone());
        if written.is_empty() {
            return Err("cap fitxer vàlid al patch".into());
        }

        // Patch = llista de fitxers tocats (ja materialitzats). Es declaren
        // explícitament: si no, el verifier i la UI no sabien què s'havia
        // escrit realment.
        Ok(Diff::materialized_files(written, 0))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  VERIFIER: comprova els criteris d'acceptació
// ─────────────────────────────────────────────────────────────────────────────

pub struct NoOrbitVerifier {
    root: PathBuf,
    /// Ordre de build opcional (p. ex. «npm run build»). Si és buit, no es
    /// comprova res i només es validen els fitxers.
    build_cmd: Option<String>,
}

impl NoOrbitVerifier {
    pub fn new(root: PathBuf, build_cmd: Option<String>) -> Self {
        Self { root, build_cmd }
    }
}

/// Hi ha ALMENYS un fitxer al projecte (cercant fins a 3 nivells, perquè
/// un projecte nou pot tenir tot el codi dins de `src/`).
fn project_has_files(root: &Path) -> bool {
    fn walk(dir: &Path, depth: usize) -> bool {
        if depth > 3 {
            return false;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return false;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() {
                // Ignora els fitxers «de sorra» (build, cache).
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.starts_with('.') || name == "README.md" {
                    continue;
                }
                return true;
            }
            if p.is_dir() && walk(&p, depth + 1) {
                return true;
            }
        }
        false
    }
    walk(root, 0)
}

/// D'un criteri d'acceptació n'extreiem els fragments que tenen pinta de
/// CAMÍ de fitxer (contenen un punt i cap espai): són els únics comprovables
/// de manera mecànica sense invocar una altra vegada la IA.
fn path_tokens(criteri: &str) -> Vec<String> {
    criteri
        .split(|c| c == ' ' || c == ',' || c == ';' || c == '(' || c == ')' || c == '`' || c == '"')
        .filter_map(|raw| {
            let t = raw.trim_matches(|c: char| ".:!?»«’'".contains(c)).to_string();
            if t.contains('/') && t.contains('.') && !t.contains("..") && !t.starts_with('/') {
                Some(t)
            } else {
                None
            }
        })
        .collect()
}

#[async_trait::async_trait]
impl Verifier for NoOrbitVerifier {
    /// Versió amb els fitxers REALMENT tocats: és el que faltava per
    /// comprovar alguna cosa més que «el projecte no és buit».
    async fn run_with_files(
        &self,
        task: &Task,
        ctx: &TaskContext,
        files: &[String],
    ) -> Result<(), String> {
        // Cap fitxer tocat = la tasca no ha fet res. Abans, tot i així, es
        // commitejava com a «completada» i el REPORT.md ho deia.
        if files.is_empty() {
            return Err("cap fitxer modificat: la tasca no ha fet res".into());
        }
        // Criteris d'acceptació que mencionen un camí. EL PLA SOLEIX barrejar
        // fitxers nous amb altres que JA EXISTEIXEN al projecte (i que la
        // tasca no ha de tocar): exigir que TOTS hi fossin bloquejava tasques
        // perfectament fetes — la Jam les perdia totes amb el `reset_hard` i
        // acabava sense haver escrit codi. Amb prou fenia n'hem de trobar UN.
        let mut trobats = 0usize;
        let mut demanats = 0usize;
        for crit in &task.acceptance {
            for tok in path_tokens(crit) {
                demanats += 1;
                let escrit = files.iter().any(|f| f == &tok || f.ends_with(&tok) || tok.ends_with(f.as_str()));
                let existeix = self.root.join(&tok).exists();
                if escrit || existeix {
                    trobats += 1;
                }
            }
        }
        if demanats > 0 && trobats == 0 {
            return Err(format!(
                "cap criteri d'acceptació amb fitxer trobat ({demanats} camins mencionats)"
            ));
        }
        self.run(task, ctx).await
    }

    async fn run(&self, task: &Task, _ctx: &TaskContext) -> Result<(), String> {
        // 1) El projecte ha de tenir ALMENYS un fitxer (a qualsevol nivell).
        if !project_has_files(&self.root) {
            return Err("el projecte és buit".into());
        }

        // 2) Build opcional.
        if let Some(cmd) = &self.build_cmd {
            if !cmd.trim().is_empty() {
                let out = tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .current_dir(&self.root)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .kill_on_drop(true)
                    .output()
                    .await
                    .map_err(|e| format!("build: {e}"))?;
                if !out.status.success() {
                    return Err(format!(
                        "build fallà: {}",
                        String::from_utf8_lossy(&out.stderr).trim()
                    ));
                }
            }
        }
        let _ = task;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  CACHE: neteja la brossa (.import de Godot, __pycache__…)
pub struct NoOrbitCache {
    root: PathBuf,
}
impl NoOrbitCache {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}
#[async_trait::async_trait]
impl Cache for NoOrbitCache {
    async fn clear(&self) {
        let _ = std::fs::remove_dir_all(self.root.join("__pycache__"));
        let _ = std::fs::remove_dir_all(self.root.join(".godot"));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  REPORTER: escript el REPORT.md al final
pub struct NoOrbitReporter {
    path: PathBuf,
}
impl NoOrbitReporter {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}
#[async_trait::async_trait]
impl Reporter for NoOrbitReporter {
    async fn report(&self, plan: &Plan, stats: &noorbit_jam::JamStats) -> Result<(), String> {
        let mut md = String::new();
        md.push_str(&format!("# Jam Report — {}\n\n", plan.title));
        md.push_str(&format!("- Completades: {}\n", stats.completed));
        md.push_str(&format!("- Bloquejades: {}\n", stats.blocked));
        md.push_str(&format!("- Saltades: {}\n", stats.skipped));
        md.push_str(&format!("- Intents: {}\n", stats.total_attempts));
        md.push_str(&format!("- Durada: {} ms\n\n", stats.duration_ms));
        md.push_str("## Tasques\n\n| ID | Títol | Criteris |\n|---|---|---|\n");
        for t in &plan.tasks {
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                t.id,
                t.title.replace('|', "\\|"),
                t.acceptance.join("; ")
            ));
        }
        if let Some(par) = self.path.parent() {
            let _ = std::fs::create_dir_all(par);
        }
        let tmp = self.path.with_extension("md.tmp");
        std::fs::write(&tmp, md.as_bytes()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())?;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  RUNNER: engega la Jam (in-process, amb cancel·lació i esdeveniments)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JamRequest {
    /// Text del brief (el que ha escrit l'usuari).
    pub brief: String,
    /// Motor: godot4 | bevy | blender | unreal5 | unity | custom.
    pub engine: String,
    /// Durada màxima en segons.
    pub duration_secs: u64,
    /// Carpeta del projecte (per defecte, l'arrel oberta).
    pub repo_path: Option<String>,
    /// Ordre de build opcional per validar.
    pub build_cmd: Option<String>,
    /// Què fer si el model no pot (o no vol) escriure fitxers.
    #[serde(default)]
    pub dry_run: bool,
}

pub struct JamRunner {
    pub cancel: CancellationToken,
}

// ─────────────────────────────────────────────────────────────────────────────
//  IDENTITAT I DIRECTORIS DE TREBALL
// ─────────────────────────────────────────────────────────────────────────────

/// Clau curta i estable del camí d'un repo (per a noms de fitxer).
fn repo_key(repo: &Path) -> String {
    use sha2::{Digest, Sha256};
    let canon = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    let mut h = Sha256::new();
    h.update(canon.to_string_lossy().as_bytes());
    let digest = h.finalize();
    let mut s = String::from("r");
    for b in &digest[..8] {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Carpeta de dades amb què es calculen els fitxers interns de la Jam.
///
/// El PARE i el FILL han d'arribar a LA MATEIXA ruta (per això el `fallback`
/// és compartit): si no, el fill no trobaria l'estat desat i la represa no
/// funcionaria mai.
pub fn jam_data_dir() -> PathBuf {
    crate::config::AppConfig::data_dir().unwrap_or_else(|| std::env::temp_dir().join("no-orbit"))
}

/// Id **estable** d'una Jam: el mateix brief + motor + durada + repo donen
/// sempre la mateixa id.
///
/// Amb una id derivada del `pid` + l'epoch (com abans) el fill reiniciat pel
/// guardian en tenia una de diferent a cada arrencada: `load()` rebutjava
/// l'estat («jam_id diferent») i la Jam recomençava tota la feina des de zero
/// just quan la memòria ja apretava.
pub fn derive_jam_id(repo: &Path, brief: &str, engine: &str, duration_secs: u64) -> String {
    let fp = noorbit_persistence::brief_fingerprint(brief, engine, duration_secs);
    format!("jam-{fp}-{}", repo_key(repo))
}

/// Carpeta de treball d'una Jam (brief, pla, resum de memòria).
///
/// Ha de ser FORA del repo: escrits a l'arrel, `git add -A` els incloïa en
/// els commits (brutícia a l'historial de l'usuari) i `git clean -fd` els
/// esborrava abans de cada tasca.
pub fn jam_workdir(data_dir: &Path, jam_id: &str) -> PathBuf {
    let key = jam_id.strip_prefix("jam-").unwrap_or(jam_id);
    data_dir.join("jam").join(key)
}

/// On es desa l'estat de represa: al directori de dades, MAI dins el repo
/// (dins el repo, el `git clean -fd` del checkpointer l'esborrava a cada
/// intent i la represa era impossible).
pub fn jam_state_path(data_dir: &Path, repo: &Path) -> PathBuf {
    data_dir
        .join("jam")
        .join("state")
        .join(format!("{}.json", repo_key(repo)))
}

/// Lliçons (intents fallits) d'un repo, també FORA del repo: són dades
/// internes de la Jam, no un artefacte del projecte de l'usuari.
pub fn jam_lessons_path(data_dir: &Path, repo: &Path) -> PathBuf {
    data_dir
        .join("jam")
        .join("lessons")
        .join(format!("{}.jsonl", repo_key(repo)))
}

/// Pla serialitzat per passar-lo del PARE al FILL.
///
/// El pare ja ha fet el pla per mostrar-lo a la UI. Si el fill en tornava a
/// fer un de nou (com abans) passaven dues coses: es pagava dues vegades la
/// mateixa invocació a la IA i la UI mostrava un pla que no era l'executat
/// (el model no és determinista).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredPlan {
    pub title: String,
    pub duration_secs: u64,
    pub tasks: Vec<StoredTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredTask {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub acceptance: Vec<String>,
    #[serde(default)]
    pub phase: Option<noorbit_jam::Phase>,
}

pub fn store_plan(plan: &Plan) -> StoredPlan {
    StoredPlan {
        title: plan.title.clone(),
        duration_secs: plan.duration.as_secs(),
        tasks: plan
            .tasks
            .iter()
            .map(|t| StoredTask {
                id: t.id.clone(),
                title: t.title.clone(),
                description: t.description.clone(),
                acceptance: t.acceptance.clone(),
                phase: t.phase,
            })
            .collect(),
    }
}

/// Torna `None` si el pla desat no és usable (buit o sense id): llavors el
/// fill fa el pla amb la IA com a mínim una vegada.
pub fn materialize_plan(sp: StoredPlan) -> Option<Plan> {
    if sp.tasks.is_empty() || sp.duration_secs == 0 {
        return None;
    }
    if sp.tasks.iter().any(|t| t.id.trim().is_empty()) {
        return None;
    }
    let duration = Duration::from_secs(sp.duration_secs.clamp(60, MAX_JAM_DURATION_SECS));
    Some(Plan {
        title: sp.title,
        tasks: sp
            .tasks
            .into_iter()
            .map(|t| Task {
                id: t.id,
                title: t.title,
                description: t.description,
                acceptance: t.acceptance,
                phase: t.phase,
            })
            .collect(),
        duration,
        budget: noorbit_jam::TimeBudget::from_total(duration),
    })
}

impl JamRunner {
    /// Prepara el pla (sense executar res) perquè la UI el pugui mostrar.
    pub async fn make_plan(
        ai: Arc<AiManager>,
        req: &JamRequest,
    ) -> Result<Plan, String> {
        let engine = Engine::parse(&req.engine).map_err(|e| e.to_string())?;
        let brief = Brief {
            text: req.brief.clone(),
            engine,
            duration: Duration::from_secs(req.duration_secs.clamp(60, MAX_JAM_DURATION_SECS)),
            budget: None,
        };
        let llm: Arc<dyn LlmClient> = Arc::new(NoOrbitLlm::new(ai));
        let planner = Planner::new(llm);
        planner.plan(&brief).await.map_err(|e| e.to_string())
    }
}
