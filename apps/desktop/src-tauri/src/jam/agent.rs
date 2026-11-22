//! Mode `--jam-agent`: l'agent de la Jam executat com a FILL.
//!
//! El pare (l'app Tauri) llança aquest binari amb:
//!
//! ```text
//! no-orbit --jam-agent --brief <fitxer.json> --repo <carpeta> \
//!          --engine godot4 --duration <segs> --build-cmd "<ordre>"
//! ```
//!
//! I el supervisa amb el `guardian` (RLIMIT_AS, process group, killpg).
//!
//! PROTOCOL
//! --------
//! - stdin: el pare hi envia `PING`, `COMPACT` i `STOP`.
//! - stdout: el fill hi respon `PONG`, `OK`, `STOPPED` i, amb el prefix
//!   `JAM|`, les línies de progrés en JSON (que el pare tradueix a
//!   esdeveniments per a la UI).
//! - stderr: logs (heretats pel pare, surt al log de l'app).

use std::path::PathBuf;
use std::sync::Arc;

use noorbit_jam::{
    Cache, Checkpointer, JamDeps, Plan, Progress, ProgressSink, QualityLevel, Reporter, SaveInfo,
    Task, Verifier, Worker,
};
use noorbit_planner::{Brief, Engine, LlmClient, Planner};

use crate::jam::{
    materialize_plan, NoOrbitCache, NoOrbitLlm, NoOrbitReporter, NoOrbitVerifier,
    NoOrbitWorker, StoredPlan,
};

/// Configuració del fill, llegida dels arguments.
#[derive(Debug, Clone)]
pub struct AgentArgs {
    pub brief_path: PathBuf,
    pub repo_path: PathBuf,
    pub engine: String,
    pub duration_secs: u64,
    pub build_cmd: Option<String>,
    /// Id ESTABLE de la Jam (calculada pel pare). Si no ve, se'n deriva una
    /// de determinista a partir del brief + repo: mai del `pid`, perquè un
    /// reinici del guardian canviaria el pid i la represa no trobaria l'estat.
    pub jam_id: Option<String>,
    /// Pla JA fet pel pare (JSON). Sense això el fill torna a demanar el pla
    /// a la IA: cost doble i un pla diferent del que veu l'usuari.
    pub plan_path: Option<PathBuf>,
    /// Proveïdor i model ACTIUS a l'app. El `active_provider` i el
    /// `selected_model` viuen en memòria del procés pare i NO es persistixen:
    /// sense passar-los, el fill sempre parlaria amb Ollama.
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// Parseja els arguments del mode fill. `Err` amb missatge per a l'usuari.
pub fn parse_args(args: &[String]) -> Result<AgentArgs, String> {
    let get = |name: &str| -> Option<String> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let brief_path = get("--brief").ok_or("falta --brief <fitxer>")?;
    let repo_path = get("--repo").ok_or("falta --repo <carpeta>")?;
    let brief_path = PathBuf::from(brief_path);
    let repo_path = PathBuf::from(repo_path);
    if !brief_path.exists() {
        return Err(format!("el brief no existeix: {}", brief_path.display()));
    }
    if !repo_path.exists() {
        return Err(format!("el repo no existeix: {}", repo_path.display()));
    }
    Ok(AgentArgs {
        brief_path,
        repo_path,
        engine: get("--engine").unwrap_or_else(|| "godot4".into()),
        duration_secs: get("--duration")
            .and_then(|s| s.parse().ok())
            .unwrap_or(4 * 3600),
        build_cmd: get("--build-cmd").filter(|s| !s.trim().is_empty()),
        jam_id: get("--jam-id").filter(|s| !s.trim().is_empty()),
        plan_path: get("--plan").map(PathBuf::from).filter(|p| p.exists()),
        provider: get("--provider").filter(|s| !s.trim().is_empty()),
        model: get("--model").filter(|s| !s.trim().is_empty()),
    })
}

/// Contingut del fitxer de brief (JSON).
#[derive(serde::Deserialize)]
struct RawBrief {
    #[serde(default)]
    text: String,
    #[serde(default)]
    engine: Option<String>,
    #[serde(default)]
    duration_secs: Option<u64>,
}

/// Receptor de progrés que escriu línies `JAM|<json>` a stdout.
struct StdoutProgress;

#[async_trait::async_trait]
impl ProgressSink for StdoutProgress {
    async fn on_progress(&self, ev: Progress) {
        let payload = match &ev {
            Progress::TaskStart { id, title } => serde_json::json!({
                "kind": "task_start", "task": id, "detail": title, "ok": true
            }),
            Progress::TaskDone { id, files } => serde_json::json!({
                "kind": "task_done", "task": id,
                "detail": format!("{files} fitxers tocats"), "ok": true
            }),
            Progress::TaskError { id, error } => serde_json::json!({
                "kind": "task_error", "task": id, "detail": error, "ok": false
            }),
            Progress::TaskBlocked { id, error } => serde_json::json!({
                "kind": "task_error", "task": id, "detail": error, "ok": false
            }),
            Progress::Note { id, text } => serde_json::json!({
                "kind": "note", "task": id, "detail": text, "ok": true
            }),
            Progress::Thinking { id, text } => serde_json::json!({
                "kind": "thinking", "task": id, "detail": text, "ok": true
            }),
            Progress::MemoryPressure { id, peak_bytes } => serde_json::json!({
                "kind": "memory", "task": id,
                "detail": format!(
                    "pressió de memòria (pic {} MB): la tasca s'ha aturat i es reintenta amb menys qualitat",
                    peak_bytes / 1024 / 1024
                ),
                "ok": false
            }),
            Progress::Compacted => serde_json::json!({
                "kind": "compact", "task": "", "detail": "caches alliberats", "ok": true
            }),
            Progress::Finished => serde_json::json!({
                "kind": "finished", "task": "", "detail": "jam finalitzada", "ok": true
            }),
        };
        println!("JAM|{payload}");
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }
}

/// Checkpointer BUIT per quan no hi ha git usable (sense binari, sense
/// permisos): la Jam corre igualment —genera i verifica cada tasca— però no
/// hi ha commits ni manera de tornar enrere. Cap mètode falla mai, perquè el
/// bucle de la Jam atura la tasca si el checkpointer retorna error, i aquí
/// l'objectiu precisament és continuar sense ell.
struct NoGitCheckpointer;

#[async_trait::async_trait]
impl Checkpointer for NoGitCheckpointer {
    async fn head(&self) -> Result<String, String> {
        Ok("sense-git".into())
    }
    async fn reset_hard(&self, _sha: &str) -> Result<(), String> {
        Ok(())
    }
    async fn commit(&self, task: &Task) -> Result<String, String> {
        Ok(format!("sense-git:{}", task.id))
    }
    async fn tag(&self, _name: &str) -> Result<(), String> {
        Ok(())
    }
}

/// Point d'entrada del mode fill. Retorna el codi de sortida (0 = ok).
pub fn run_agent_mode(args: &[String]) -> i32 {
    let parsed = match parse_args(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("noorbit --jam-agent: {e}");
            return 1;
        }
    };

    // Runtime asíncron propi: el fill és un procés independent.
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("noorbit --jam-agent: no s'ha pogut crear el runtime: {e}");
            return 1;
        }
    };

    rt.block_on(async move { run_agent(parsed).await })
}

async fn run_agent(args: AgentArgs) -> i32 {
    // 1) Brief.
    let bytes = match tokio::fs::read(&args.brief_path).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("noorbit --jam-agent: no s'ha pogut llegir el brief: {e}");
            return 1;
        }
    };
    let raw: RawBrief = match serde_json::from_slice(&bytes) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("noorbit --jam-agent: brief invàlid: {e}");
            return 1;
        }
    };
    let engine_name = raw.engine.clone().unwrap_or_else(|| args.engine.clone());
    let engine = match Engine::parse(&engine_name) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("noorbit --jam-agent: {e}");
            return 1;
        }
    };
    let brief = Brief {
        text: raw.text,
        engine,
        duration: std::time::Duration::from_secs(
            raw.duration_secs.unwrap_or(args.duration_secs).clamp(60, crate::jam::MAX_JAM_DURATION_SECS),
        ),
        budget: None,
    };

    // 2) IA: la mateixa configuració de l'app (proveïdors i tokens).
    let config = crate::config::AppConfig::load_or_default();
    let data_dir = crate::jam::jam_data_dir();
    let api_manager = Arc::new(std::sync::Mutex::new(crate::api::ApiManager::new(
        data_dir.clone(),
    )));
    let ai_manager = Arc::new(crate::ai::AiManager::with_remote(&config, api_manager));

    // El fill té el SEU `AiManager`: si no se li diu res, arrenca amb
    // «ollama» i el model per defecte, encara que l'usuari estigui fent servir
    // OpenAI/Claude/Gemini al xat. El pare passa la selecció real.
    if let Some(provider) = &args.provider {
        ai_manager.set_active_provider(provider).await;
        eprintln!("noorbit --jam-agent: proveïdor d'IA = {provider}");
    }
    if let Some(model) = &args.model {
        // El model triat només té sentit per a Ollama: en un proveïdor en
        // línia el model és el que té configurat el proveïdor, i posar-lo com a
        // model d'Ollama faria que la CAIGUDA a Ollama intentés baixar un
        // model que no existeix.
        let es_ollama = args.provider.as_deref().map(|p| p == "ollama").unwrap_or(true);
        if es_ollama {
            ai_manager.set_selected_model(Some(model.clone())).await;
        }
    }
    let llm: Arc<dyn LlmClient> = Arc::new(NoOrbitLlm::new(ai_manager.clone()));

    // LA IA ES POT CANVIAR EN MARXA: el pare escriu la tria al fitxer
    // «ai.json» del directori de la Jam i el fill la va llegint. Sense això,
    // canviar d'IA obligava a aturar la Jam i tornar-la a començar.
    let ai_file = args.brief_path.parent().map(|d| crate::jam::jam_ai_path(d));
    if let Some(ai_file) = ai_file {
        let watch_manager = ai_manager.clone();
        let inicial = args
            .provider
            .clone()
            .map(|p| (p, args.model.clone().unwrap_or_default()));
        tokio::spawn(async move {
            let mut ultima: Option<(String, String)> = inicial;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let Some((provider, model)) = crate::jam::read_jam_ai(&ai_file) else {
                    continue;
                };
                if ultima.as_ref() == Some(&(provider.clone(), model.clone())) {
                    continue;
                }
                ultima = Some((provider.clone(), model.clone()));
                watch_manager.set_active_provider(&provider).await;
                if provider == "ollama" && !model.is_empty() {
                    watch_manager.set_selected_model(Some(model.clone())).await;
                }
                eprintln!("noorbit --jam-agent: IA canviada a {provider} · {model}");
                // L'usuari ha de VEURE el canvi al registre de la Jam.
                println!(
                    "JAM|{}",
                    serde_json::json!({
                        "kind": "note", "task": "",
                        "detail": format!("IA canviada: {provider} · {model}"),
                        "ok": true
                    })
                );
                use std::io::Write;
                let _ = std::io::stdout().flush();
            }
        });
    }

    // 3) Pla: el del pare si ve en un fitxer (mateix pla que ve l'usuari i
    //    cap cost extra); si no, el fa el planner ara.
    let plan: Plan = match load_plan_file(args.plan_path.as_deref()).await {
        Some(p) => {
            eprintln!("noorbit --jam-agent: reutilitzant el pla del pare ({} tasques)", p.tasks.len());
            p
        }
        None => {
            let planner = Planner::new(llm.clone());
            match planner.plan(&brief).await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("noorbit --jam-agent: no s'ha pogut fer el pla: {e}");
                    return 1;
                }
            }
        }
    };

    // 4) Dependències del bucle. El git és DESEJABLE (cada tasca es desa com
    //    un commit i es pot tornar enrere) però NO obligatori: si el projecte
    //    no és un repo amb un commit base, s'inicialitza/arrecla sol (com
    //    «Fitxer ▸ Nou projecte»); si tampoc és possible, la Jam corre amb el
    //    checkpointer buit i ho diu al registre perquè l'usuari sàpiga que no
    //    podrà desfer. El commit base compta tant com el repo: sense HEAD el
    //    bucle se saltaria totes les tasques en silenci.
    let mut ckpt_estat = "git";
    let ckpt: Arc<dyn Checkpointer> = match noorbit_git::GitCheckpointer::new(&args.repo_path)
        .ok()
        .filter(|_| crate::commands::workspace::te_base_commit(&args.repo_path))
    {
        Some(g) => Arc::new(g),
        None => match crate::commands::workspace::init_git_repo(&args.repo_path).await {
            Ok(()) => match noorbit_git::GitCheckpointer::new(&args.repo_path) {
                Ok(g) => {
                    ckpt_estat = "auto";
                    Arc::new(g)
                }
                Err(e) => {
                    eprintln!("noorbit --jam-agent: git no funcional després d'init: {e}");
                    ckpt_estat = "cap";
                    Arc::new(NoGitCheckpointer)
                }
            },
            Err(e) => {
                eprintln!("noorbit --jam-agent: repo git no utilitzable ({e}); Jam sense commits");
                ckpt_estat = "cap";
                Arc::new(NoGitCheckpointer)
            }
        },
    };
    // Les lliçons (intents fallits) van al directori de dades, MAI dins el
    // repo: a l'arrel les esborrava el `git clean -fd` de cada intent.
    let lessons = Arc::new(noorbit_lessons::LessonsStore::new(crate::jam::jam_lessons_path(
        &data_dir,
        &args.repo_path,
    )));
    // Receptor de progrés: ES CREA ABANS del worker perquè aquest pugui
    // emetre «què fa la IA» i el seu pensament (línies `JAM|` per stdout).
    let progress: Arc<dyn ProgressSink> = Arc::new(StdoutProgress);
    // L'usuari veu a la UI quina modalitat de checkpoints té la Jam: perdre
    // la capacitat de desfer feina NO pot ser silenciós.
    match ckpt_estat {
        "auto" => {
            progress
                .on_progress(Progress::Note {
                    id: String::new(),
                    text: "he inicialitzat git al projecte automàticament: cada tasca \
                           es desa com un commit i es pot tornar enrere".into(),
                })
                .await;
        }
        "cap" => {
            progress
                .on_progress(Progress::Note {
                    id: String::new(),
                    text: "la Jam corre SENSE git (no instal·lat o sense permisos): les \
                           tasques es generen i verifiquen, però no es poden desfer. \
                           Instala git perquè els checkpoints s'activin sols.".into(),
                })
                .await;
        }
        _ => {}
    }
    let worker: Arc<dyn Worker> = Arc::new(
        NoOrbitWorker::new(llm, args.repo_path.clone())
            .with_lessons(lessons.clone())
            // AUTOPILOTATGE: el worker cerca informació a internet sol (headless,
            // sense finestres ni permisos) per fonamentar el codi de cada tasca.
            .with_research(Arc::new(crate::agent::AgentOrchestrator::new(
                crate::agent::AgentConfig::default(),
            )))
            .with_progress(progress.clone()),
    );
    let verifier: Arc<dyn Verifier> =
        Arc::new(NoOrbitVerifier::new(args.repo_path.clone(), args.build_cmd.clone()));
    let report_path = args.repo_path.join("REPORT.md");
    let reporter: Arc<dyn Reporter> = Arc::new(NoOrbitReporter::new(report_path));
    let cache: Arc<dyn Cache> = Arc::new(NoOrbitCache::new(args.repo_path.clone()));

    // ── Persistència (reprendre després d'un crash/reinici) ───────────────
    // Id ESTABLE: la del pare o, si no ve, derivada del brief + repo. Mai del
    // `pid`: el guardian reinicia el fill amb un pid nou i llavors `load()`
    // rebutjava l'estat («jam_id diferent») i la Jam refacia tota la feina.
    let jam_id = args.jam_id.clone().unwrap_or_else(|| {
        crate::jam::derive_jam_id(
            &args.repo_path,
            &brief.text,
            &args.engine,
            brief.duration.as_secs(),
        )
    });
    let brief_fp = noorbit_persistence::brief_fingerprint(&brief.text, &args.engine, brief.duration.as_secs());
    // L'estat es deu FORA del repo (si no, `git clean -fd` l'esborra abans de
    // cada tasca i la represa/els reinicis per memòria no funcionen).
    let state_path = crate::jam::jam_state_path(&data_dir, &args.repo_path);
    let persist = noorbit_persistence::ProgressStore::new(state_path.clone());

    // Intentar reprendre.
    let (resume_idx, resume_stats) = match persist.load(&jam_id, &brief_fp, &args.repo_path).await {
        Some(s) => {
            eprintln!(
                "noorbit --jam-agent: reprenent la Jam des de la tasca {} ({} fetes)",
                s.next_task_idx + 1,
                s.stats.completed
            );
            (s.next_task_idx, Some(s.stats.into()))
        }
        None => (0, None),
    };

    let plan_clone = plan.clone();
    let jam_state = state_path;
    let save_fn: Arc<dyn Fn(&SaveInfo) + Send + Sync> = Arc::new(move |info: &SaveInfo| {
        let plan = plan_clone.clone();
        // El SHA i la qualitat els DONA el bucle (`SaveInfo`): abans el `save_fn`
        // feia el seu `git rev-parse HEAD` i fixava `QualityLevel::Full`, de
        // manera que una represa tornava a un commitequivocat i amb un
        // pressupost de qualitat fals.
        let state = noorbit_persistence::make_state(
            &jam_id,
            &brief_fp,
            &plan,
            &info.last_good_sha,
            info.next_idx,
            info.quality,
            &info.stats,
            &noorbit_jam::PhaseTracker::new(plan.budget.clone()),
        );
        let stats_line = serde_json::json!({
            "kind": "stats",
            "task": info.task_id,
            "detail": format!(
                "{} de {} tasques",
                info.stats.completed,
                plan.tasks.len()
            ),
            "ok": true,
            "completed": info.stats.completed,
            "blocked": info.stats.blocked,
            "skipped": info.stats.skipped,
            "total": plan.tasks.len(),
            "quality": match info.quality {
                QualityLevel::Full => "completa",
                QualityLevel::Reduced => "reduïda",
                QualityLevel::Minimal => "mínima",
            },
        });
        // Tot i que el bucle crida `save_fn` en un context sincrònic, un
        // `println!` ací és segur (stdout del fill és un pipe de 64 kb i
        // el pare el drena contínuament).
        println!("JAM|{stats_line}");
        use std::io::Write;
        let _ = std::io::stdout().flush();

        // DESADA ATÒMICA i sense bloquejar: aquest tancament corre dins el
        // bucle asincrònic, així que la versió sincrònica és intencional.
        if let Some(parent) = jam_state.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_vec_pretty(&state) {
            let tmp = jam_state.with_extension(format!("json.tmp-{}", std::process::id()));
            if std::fs::write(&tmp, &json).is_ok() {
                let _ = std::fs::rename(&tmp, &jam_state);
            }
        }
    });

    let deps = JamDeps {
        worker,
        verifier,
        ckpt,
        reporter,
        cache,
        repo_path: args.repo_path.clone(),
        progress: Some(progress),
        save_fn: Some(save_fn),
    };

    // ── Signal handler: SIGTERM/SIGINT → cancel·lació ordenada ─────────────
    // El guardian envia STOP via stdin; però si el SO envia SIGTERM (kill,
    // docker stop, systemctl stop) o l'usuari fa Ctrl+C, el fill també ha
    // de netejar (worktree, report) abans de morir.
    let signal = noorbit_signal::SignalHandler::spawn();
    let signal_token = signal.token();

    // 5) Bucle amb el runtime del fill (recomp STOP/COMPACT del guardian).
    let runtime = noorbit_guardian::ChildRuntime::new();
    let rx = runtime.subscribe();
    let cfg = noorbit_jam::JamConfig::default();

    runtime
        .run(async move {
            let rt = noorbit_jam::JamRuntime::new(rx);
            rt.attach_external_cancel(signal_token);
            noorbit_jam::run_autonomous_loop_resumable(rt, plan, deps, cfg, resume_idx, resume_stats).await;
            // Neteja: esborra l'estat (Jam completada o aturada neta).
            let _ = persist.clear().await;
        })
        .await;

    // `ChildRuntime::run` fa exit(0) quan l'agent acaba.
    0
}

/// Llegeix el pla que ha escrit el pare. `None` si no hi és o no és vàlid
/// (llavors el fill el fa amb la IA).
async fn load_plan_file(path: Option<&std::path::Path>) -> Option<Plan> {
    let path = path?;
    let bytes = tokio::fs::read(path).await.ok()?;
    let stored: StoredPlan = serde_json::from_slice(&bytes).ok()?;
    materialize_plan(stored)
}
