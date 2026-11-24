//! Comandes de l'Autonomous Jam.
//!
//! Flux d'una Jam:
//! 1. `jam_plan` → el planner converteix el brief en un pla (mostrat a la UI).
//! 2. `jam_start` → arrenca el bucle: per cada tasca, el model escriu codi
//!    (blocs `@file:`), el materialitzem al projecte, verifiquem i commitem.
//! 3. `jam_stop` → atura netament (cancel·lació del token).
//! 4. `jam_state` → estat actual (per pintar la UI).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

use noorbit_planner::Engine;

use crate::jam::{
    emit, store_plan, derive_jam_id, jam_ai_path, jam_data_dir, jam_workdir, write_jam_ai,
    JamRequest, JamRunner, JamState,
};
use crate::AppState;

/// Sostre de temps per fer el PLA amb la IA.
///
/// Ha de ser MES GRAN que el pressupost TOTAL del planner (3 intents de
/// 20 min = ~1 h): si el nostre timeout es crusca abans que el planner
/// pugui fer el SEGON intent, es perden els reintents (que són els que
/// salven la Jam quan el model local triga o torna JSON invàlid) i la Jam
/// mor on abans s'hauria recuperat. El que atura la Jam de debò és
/// l'usuari: el pla és CANCEL·LABLE i el token es mira en cada instant.
pub(super) const PLAN_TIMEOUT: Duration = Duration::from_secs(65 * 60);


/// Estat global de la Jam (n'hi ha pas de dues alhora).
pub struct JamHandle {
    /// Token de cancel·lació ACTUAL (es substitueix a cada Jam nova).
    pub cancel: std::sync::Mutex<CancellationToken>,
    pub state: std::sync::Mutex<JamState>,
    pub running: AtomicBool,
    /// Fitxer on es desa la tria d'IA de la Jam en curs (el fill el llegeix
    /// per poder canviar d'IA EN MARXA). `None` si no hi ha cap Jam.
    pub ai_path: std::sync::Mutex<Option<std::path::PathBuf>>,
}

impl Default for JamHandle {
    fn default() -> Self {
        Self {
            cancel: std::sync::Mutex::new(CancellationToken::new()),
            state: std::sync::Mutex::new(JamState::default()),
            running: AtomicBool::new(false),
            ai_path: std::sync::Mutex::new(None),
        }
    }
}

impl JamHandle {
    pub(crate) fn snapshot(&self) -> JamState {
        // Sense `unwrap`: si el mutex s'havia envenenat (un panell va fallar
        // dins del lock), la Jam encara ha de poder pintar l'estat en comptes
        // de fer caure el panell sencer.
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }
    fn set(&self, f: impl FnOnce(&mut JamState)) {
        if let Ok(mut s) = self.state.lock() {
            f(&mut s);
        }
    }
    /// On és el fitxer d'IA de la Jam en curs (si n'hi ha).
    fn ai_path(&self) -> Option<std::path::PathBuf> {
        self.ai_path
            .lock()
            .ok()
            .and_then(|p| p.clone())
    }
    /// Recorda on és el fitxer d'IA (per poder-hi escriure després).
    fn set_ai_path(&self, path: Option<std::path::PathBuf>) {
        if let Ok(mut p) = self.ai_path.lock() {
            *p = path;
        }
    }
}

/// Prepara el pla i el retorna (per preview a la UI, sense executar).
#[tauri::command]
pub async fn jam_plan(
    state: State<'_, AppState>,
    request: JamRequest,
) -> Result<JamPlanPreview, String> {
    let plan = JamRunner::make_plan(state.ai_manager.clone(), &request).await?;
    Ok(JamPlanPreview {
        title: plan.title.clone(),
        tasks: plan
            .tasks
            .iter()
            .map(|t| JamTaskPreview {
                id: t.id.clone(),
                title: t.title.clone(),
                description: t.description.clone(),
                acceptance: t.acceptance.clone(),
            })
            .collect(),
        duration_secs: plan.duration.as_secs(),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JamPlanPreview {
    pub title: String,
    pub tasks: Vec<JamTaskPreview>,
    pub duration_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JamTaskPreview {
    pub id: String,
    pub title: String,
    pub description: String,
    pub acceptance: Vec<String>,
}

/// Arrenca la Jam en segon pla.
///
/// L'agent corre en un **SUBPROCÉS** (`no-orbit --jam-agent …`) supervisat
/// pel `guardian`:
/// - `RLIMIT_AS` al fill: si s'embala de memòria, MOR el fill (el Mac no
///   es congela).
/// - Process group: el supervisor mata el fill I els seus fills (Godot,
///   Blender) amb un sol `killpg`.
/// - El supervisor medeix swap i RAM disponible: si la màquina es queda sense
///   aire, atura el fill netament i el reinicia (amb límit de reinicis).
///
/// El pare només fa de supervisor i de «traductor» d'esdeveniments cap a la UI.
#[tauri::command]
pub async fn jam_start(
    app: AppHandle,
    state: State<'_, AppState>,
    jam: State<'_, Arc<JamHandle>>,
    request: JamRequest,
) -> Result<(), String> {
    // Comprovació ràpida: si ja hi ha una Jam en marxa, ni tan cal validar
    // res. El control que REALMENT decideix és el compare-and-swap de més
    // avall, que és atòmic.
    if jam.running.load(Ordering::SeqCst) {
        return Err("ja hi ha una Jam en marxa".into());
    }
    let brief = request.brief.trim().to_string();
    if brief.is_empty() {
        return Err("el brief és buit".into());
    }

    // Motor triat: decideix amb què s'escriu el joc i on viu.
    let engine = Engine::parse(&request.engine).map_err(|e| e.to_string())?;

    // Arrel del projecte: la que l'usuari ha demanat o la que té oberta.
    let base = match &request.repo_path {
        Some(p) if !p.trim().is_empty() => std::path::PathBuf::from(p),
        _ => crate::commands::workspace::current_root()
            .ok_or_else(|| "no hi ha cap projecte obert".to_string())?,
    };
    if !base.is_dir() {
        return Err(format!("la carpeta no existeix: {}", base.display()));
    }

    // ── CARPETA DEL JOC ──────────────────────────────────────────────────
    // La Jam ha de construir EL JOC que s'ha demanat. Si el workspace obert no
    // és un projecte del motor triat, el joc es crea a la SEVA carpeta
    // (`prototypes/<nom>/`): el resultat és un projecte del joc, i el projecte
    // que l'usuari té obert no queda ple de fitxers solts.
    let explicit = request
        .repo_path
        .as_deref()
        .map(|p| !p.trim().is_empty())
        .unwrap_or(false);
    let mut root = if explicit {
        base.clone()
    } else {
        crate::jam::game::carpeta_del_joc(&base, engine, &brief)
    };

    // Estructura del projecte de joc (manifest, escena principal, entrada):
    // sense això la Jam escriu fitxers solts i no hi ha res a obrir ni a
    // executar — és a dir, el resultat NO és un joc.
    match crate::jam::game::prepara_projecte(&root, engine) {
        Ok(dir) => root = dir,
        Err(e) => {
            eprintln!("jam: no s'ha pogut preparar el projecte de joc: {e}");
        }
    }

    // La Jam desa cada tasca com un commit per poder tornar enrere, però git
    // NO és una exigència per a l'usuari: si el projecte no és un repo amb un
    // commit base, NoOrbit el crea/arregla sol (com fa «Fitxer ▸ Nou
    // projecte»). El commit base és IMPRESCINDIBLE: el bucle arrenca amb
    // `git rev-parse HEAD` i, si no hi és, se salta totes les tasques en
    // silenci (la Jam sembla viva però no escriu res). Si ni tan sols això és
    // possible (git sense instal·lar, permisos), la Jam corre IGUALMENT sense
    // checkpoints: el fill ho detecta, ho diu al registre i treballa
    // generant i verificant tasques sense desfer-les.
    //
    // I si la carpeta del joc és una SUBCARPETA d'un altre repo (el workspace
    // obert en pot ser un), la Jam ha de fer el seu PROPI repo: amb el del
    // pare, el «git add -A» i el «git clean -fd» del checkpointer tocaven el
    // projecte de l'usuari sencer.
    let repo_propi = crate::commands::workspace::te_repo_propi(&root);
    if !repo_propi
        || noorbit_git::GitCheckpointer::new(root.clone()).is_err()
        || !crate::commands::workspace::te_base_commit(&root)
    {
        if let Err(e) = crate::commands::workspace::init_git_repo(&root).await {
            eprintln!(
                "jam: git no disponible per a {} ({e}); la Jam correrà sense commits",
                root.display()
            );
        }
    }

    // Rellotge de la Jam: el fill rep el `--duration` i s'atura sol.
    let duration = request.duration_secs.clamp(60, crate::jam::MAX_JAM_DURATION_SECS);

    // Identitat ESTABLE de la Jam (brief + motor + durada + repo). Abans es
    // derivava del pid+epoch del fill: cada reinici pel guardian en tenia una
    // de diferent i la represa mai trobava l'estat desat.
    let jam_id = derive_jam_id(&root, &brief, &request.engine, duration);

    // ── RESERVA IMMEDIATA DE LA JAM ────────────────────────────────────
    // Dos «Comença» (o dues finestres) no poden passar els dos el control.
    // El flag s'ha de posar ABANS de qualsevol espera: abans es marcava
    // després de dos `await` (el repo i el sondeig de la IA) i hi havia una
    // finestra de SEGONS en què una segona Jam es colava: pisava l'estat i
    // el token de la primera i les dues escrivien al mateix repo.
    if jam
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("ja hi ha una Jam en marxa".into());
    }

    // EL TOKEN S'INSTAL·LA ABANS DE QUALSEVOL ESPERA: «Atura» ha de poder
    // aturar la Jam des del primer instant (i no el token vell d'una Jam
    // anterior).
    let token = CancellationToken::new();
    if let Ok(mut c) = jam.cancel.lock() {
        *c = token.clone();
    }

    // LA JAM ARRENCA ABANS DEL PLA. Fins ara `jam_start` esperava la crida a
    // la IA per fer el pla (que amb un model local pot durar minuts) ABANS de
    // marcar-se com a en marxa: la UI no veia res, el panell semblava penjat
    // i «Comença» es podia polsar altra vegada. Ara l'estat `running` + el
    // cronòmetre s'emeten a l'instant i el pla es fa en segon pla; si falla,
    // la Jam es marca com a fallida amb explicació.
    let now = chrono::Local::now().timestamp();
    let report_path = root.join("REPORT.md");
    jam.set(|s| {
        *s = JamState {
            running: true,
            title: String::new(),
            completed: 0,
            blocked: 0,
            skipped: 0,
            total: 0,
            current_task: String::new(),
            current_detail: "preparant el pla…".into(),
            started_at: now,
            finished_at: None,
            duration_secs: duration,
            report_path: report_path.to_string_lossy().to_string(),
            project_path: root.to_string_lossy().to_string(),
            error: None,
            // El proveïdor i el model s'hi afegeixen quan es sapen (després
            // del sondeig de l'IA): no han de delaying l'arribada a pantalla.
            provider: String::new(),
            model: String::new(),
            stopping: false,
        };
    });
    let _ = app.emit("jam://state", jam.snapshot());
    // L'usuari ha de saber ON s'està construint el joc (pot ser una carpeta
    // nova dins del workspace obert).
    emit(
        &app,
        "note",
        "",
        &format!("el joc es construeix a: {}", root.display()),
        true,
    );

    // Fitxers interns FORA del repo del projecte: escrits a l'arrel, el
    // `git add -A` del checkpointer els incloïa als commits i el
    // `git clean -fd` els esborrava abans de cada tasca (perdia el brief i,
    // pitjor, l'estat de represa).
    let workdir = jam_workdir(&jam_data_dir(), &jam_id);
    if let Err(e) = std::fs::create_dir_all(&workdir) {
        // La reserva es lliura (i la UI ho sap): si no, la Jam quedaria
        // «en marxa» per sempre amb cap agent darrere.
        let msg = format!("no s'ha pogut crear la carpeta de treball: {e}");
        fail(&app, jam.inner(), msg.clone());
        return Err(msg);
    }
    // El `git clean -fd` del checkpointer només respeuta els fitxers IGNORED;
    // als directoris de dades que versions anteriors escrivien DINS del repo
    // (`.noorbit/`) cal afegir-los a `.git/info/exclude`: si no, una Jam
    // antiga encallada allà dins s'esborrava (o acabava als commits).
    // `target/` és el directori de compilació de Rust (Bevy): sense excloure-lo
    // el `git add -A` el commiteja (centenes de MB) i el `git clean -fd`
    // esborraria la compilació sencera entre tasques.
    if let Err(e) = noorbit_persistence::ensure_git_excluded(
        &root,
        &[".noorbit/", ".noorbit-jam/", "target/", "._*"],
    )
        .await
    {
        eprintln!("jam: no s'ha pogut actualitzar .git/info/exclude: {e}");
    }

    let brief_path = workdir.join("brief.json");
    let plan_path = workdir.join("plan.json");
    let summary_path = workdir.join("summary.json");
    let brief_json = serde_json::json!({
        "text": brief,
        "engine": request.engine,
        // Durada JA RETALLADA pels límits: el fill la reutilitza tal com està,
        // i si no rep `--jam-id` ha de poder derivar la mateixa id.
        "durationSecs": duration,
    });
    if let Err(e) = std::fs::write(&brief_path, serde_json::to_vec_pretty(&brief_json).unwrap_or_default()) {
        let msg = format!("no s'ha pogut escriure el brief: {e}");
        fail(&app, jam.inner(), msg.clone());
        return Err(msg);
    }

    // Proveïdor i model de la Jam. Si l'usuari n'ha triat un al panell, es
    // respecta la seva tria; si no, es fa servir el proveïdor ACTIU de l'app
    // (amb la caiguda del xat: Ollama aturat → remot → IA integrada).
    //
    // El fill construeix el seu propi `AiManager` i aquests dos valors NO es
    // persistitzen: sense passar-los, la Jam sempre parlava amb Ollama encara
    // que l'usuari tingui un proveïdor remot seleccionat.
    let (provider, model) = crate::jam::resolve_jam_ai(&state.ai_manager, &request).await;
    // La tria d'IA es desa al fitxer que el fill va llegint: l'usuari la
    // pot canviar EN MARXA (el fill el rellegeix i la tasca següent ja va amb
    // la IA nova) sense haver d'aturar la Jam.
    let ai_file = jam_ai_path(&workdir);
    if let Err(e) = write_jam_ai(&ai_file, &provider, &model) {
        eprintln!("jam: no s'ha pogut desar la tria d'IA: {e}");
    }
    jam.set_ai_path(Some(ai_file.clone()));
    let ai_manager = state.ai_manager.clone();
    // Ara sí que es sap quina IA hi treballa: la capçal del panell ho diu.
    jam.set(|s| {
        s.provider = provider.clone();
        s.model = model.clone();
    });
    let _ = app.emit("jam://state", jam.snapshot());

    let jam_handle = jam.inner().clone();
    let app2 = app.clone();

    tauri::async_runtime::spawn(async move {
        // El Mac/PC NO s'ha de suspendre mentre la Jam treballa en segon pla:
        // el guard dura el que dura el supervisor (i els seus reinicis); en
        // acabar o fallar, allibera i el SO torna al seu comportament normal.
        let _awake = crate::keep_awake::activar("Jam");
        // ── Pla (amb la IA connectada), ARA en segon pla ──────────────────
        // El pla el fa la IA: amb un model local pot trigar MINUTS i, si el
        // model no respon de cap manera, es queda penjada. Per això la fase
        // és CANCEL·LABLE (l'atura de l'usuari la talla de debò) i té un
        // sostre de temps.
        let plan = match plan_with_future(JamRunner::make_plan(ai_manager.clone(), &request), &token)
            .await
        {
            PlanOutcome::Planned(p) => p,
            PlanOutcome::Cancelled => {
                let _ = std::fs::remove_dir_all(&workdir);
                stopped(&app2, &jam_handle);
                return;
            }
            PlanOutcome::Failed(msg) => {
                let _ = std::fs::remove_dir_all(&workdir);
                fail(&app2, &jam_handle, msg);
                return;
            }
        };
        // El pla JA fet s'envia al fill: si no, el fill el torna a demanar a la IA
        // (cost doble) i executa un pla DISTINT del que veu l'usuari.
        if let Err(e) = std::fs::write(
            &plan_path,
            serde_json::to_vec_pretty(&store_plan(&plan)).unwrap_or_default(),
        ) {
            let _ = std::fs::remove_dir_all(&workdir);
            fail(&app2, &jam_handle, format!("no s'ha pogut escriure el pla: {e}"));
            return;
        }
        // El pla existeix: la UI rep el títol real i el denominador de la
        // barra de progrés abans que el fill movi un sol fitxer.
        jam_handle.set(|s| {
            s.title = plan.title.clone();
            s.total = plan.tasks.len() as u32;
            s.current_detail = format!("pla fet ({} tasques) · executant…", plan.tasks.len());
        });
        emit(
            &app2,
            "note",
            "",
            &format!("pla fet: «{}» amb {} tasques", plan.title, plan.tasks.len()),
            true,
        );
        let _ = app2.emit("jam://state", jam_handle.snapshot());
        // Si l'usuari ha demanat aturar DURANT el pla, ni tan sols no s'ha de
        // llançar l'agent: es neteja i la Jam s'acaba.
        if token.is_cancelled() {
            let _ = std::fs::remove_dir_all(&workdir);
            stopped(&app2, &jam_handle);
            return;
        }
        // ── El binari que es relanza com a fill ────────────────────────────
        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&workdir);
                fail(&app2, &jam_handle, format!("no s'ha pogut trobar el binari: {e}"));
                return;
            }
        };
        let mut args: Vec<String> = vec![
            "--jam-agent".into(),
            "--brief".into(),
            brief_path.to_string_lossy().to_string(),
            "--repo".into(),
            root.to_string_lossy().to_string(),
            "--engine".into(),
            request.engine.clone(),
            "--duration".into(),
            duration.to_string(),
        ];
        if let Some(cmd) = &request.build_cmd {
            if !cmd.trim().is_empty() {
                args.push("--build-cmd".into());
                args.push(cmd.clone());
            }
        }
        // Identitat + pla + IA del pare, perquè el fill no les torni a inventar.
        args.push("--jam-id".into());
        args.push(jam_id.clone());
        args.push("--plan".into());
        args.push(plan_path.to_string_lossy().to_string());
        args.push("--provider".into());
        args.push(provider.clone());
        if !model.trim().is_empty() {
            args.push("--model".into());
            args.push(model.clone());
        }
        let exe_path = exe.to_string_lossy().to_string();
        let factory: Arc<dyn Fn() -> noorbit_guardian::SpawnSpec + Send + Sync> =
            Arc::new(move || noorbit_guardian::SpawnSpec {
                program: exe_path.clone(),
                args: args.clone(),
                env: vec![],
                cwd: None,
            });

        // ── Supervisor del guardian ────────────────────────────────────────
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<noorbit_guardian::GuardianEvent>();
        let mut cfg = noorbit_guardian::GuardianConfig::for_ram(
            total_system_ram().unwrap_or(8 * 1024 * 1024 * 1024),
        );
        // El fill és l'agent: li donem el sostre que toca segons la RAM.
        cfg.nice = 10;
        let (supervisor, cancel) = match noorbit_guardian::Supervisor::new(cfg, factory, tx) {
            Ok(s) => s,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&workdir);
                fail(&app2, &jam_handle, format!("supervisor no vàlid: {e}"));
                return;
            }
        };

        // Cancel·lació des de la UI. El token es CLONA perquè el pares també
        // el vol consultar després (per saber si l'aturada va ser de l'usuari).
        let cancel_bg = cancel.clone();
        let token_watch = token.clone();
        tokio::spawn(async move {
            token_watch.cancelled().await;
            cancel_bg.cancel();
        });

        // Esdeveniments del guardian → UI.
        let app3 = app2.clone();
        let jam3 = jam_handle.clone();
        let summary_path3 = summary_path.clone();
        let mut reader = tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                match ev {
                    noorbit_guardian::GuardianEvent::ChildLine(line) => {
                        // Línies de progrés del fill (prefixe «JAM|»).
                        if let Some(json) = line.strip_prefix("JAM|") {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                                let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or("");
                                let task = v.get("task").and_then(|k| k.as_str()).unwrap_or("");
                                let detail = v.get("detail").and_then(|k| k.as_str()).unwrap_or("");
                                let ok = v.get("ok").and_then(|k| k.as_bool()).unwrap_or(true);
                                if kind == "stats" {
                                    // Comptadors REALS del bucle (els envia el
                                    // fill des del `save_fn`, després de cada
                                    // tasca): abans la UI només veia
                                    // «tasca actual» i la barra de progrés no
                                    // es mogia mai. NO s'afegeix al registre:
                                    // són dades, no un missatge per a l'usuari.
                                    let num = |k: &str| {
                                        v.get(k).and_then(|x| x.as_u64()).unwrap_or(0) as u32
                                    };
                                    jam3.set(|s| {
                                        s.completed = num("completed");
                                        s.blocked = num("blocked");
                                        s.skipped = num("skipped");
                                        let total = num("total");
                                        if total > 0 {
                                            s.total = total;
                                        }
                                        let quality = v
                                            .get("quality")
                                            .and_then(|x| x.as_str())
                                            .unwrap_or("");
                                        s.current_detail = if quality.is_empty() {
                                            detail.to_string()
                                        } else {
                                            format!("{detail} · qualitat: {quality}")
                                        };
                                    });
                                    let _ = app3.emit("jam://state", jam3.snapshot());
                                    continue;
                                }
                                emit(&app3, kind, task, detail, ok);
                                if kind == "task_start" {
                                    jam3.set(|s| {
                                        s.current_task = task.to_string();
                                        s.current_detail = detail.to_string();
                                    });
                                }
                                if kind == "note" {
                                    // «Què fa la IA ara mateix»: és una línia
                                    // curta que alimenta el detall viu (pantalla
                                    // de feina i panell) sense esborrar la tasca.
                                    jam3.set(|s| s.current_detail = detail.to_string());
                                    let _ = app3.emit("jam://state", jam3.snapshot());
                                }
                                if kind == "stream" {
                                    // TEXT EN VIU: el que la IA està escrivint
                                    // ara mateix (streaming del model local).
                                    // `current_detail` en mostra un retall (la
                                    // línia és d'una sola fila); el text sencer
                                    // arriba a la UI pel registre i pel requadre
                                    // «en viu» del panell.
                                    let curt: String = detail
                                        .chars()
                                        .rev()
                                        .take(140)
                                        .collect::<Vec<_>>()
                                        .into_iter()
                                        .rev()
                                        .collect();
                                    jam3.set(|s| {
                                        s.current_detail = format!("la IA està escrivint: {curt}")
                                    });
                                    let _ = app3.emit("jam://state", jam3.snapshot());
                                }
                                if kind == "task_done" || kind == "task_error" {
                                    // El text del `detail` és el que dona
                                    // context a l'usuari (fitxers tocats, error).
                                    jam3.set(|s| s.current_detail = detail.to_string());
                                }
                            }
                        }
                    }
                    noorbit_guardian::GuardianEvent::AgentStopping { reason } => {
                        emit(
                            &app3,
                            "memory",
                            "",
                            &format!("l'agent s'atura: {reason:?}"),
                            false,
                        );
                    }
                    noorbit_guardian::GuardianEvent::AgentRestarted { new_pid, restarts_this_hour } => {
                        emit(
                            &app3,
                            "restart",
                            "",
                            &format!("agent reiniciat (pid {new_pid}, {restarts_this_hour} reinicis)"),
                            false,
                        );
                    }
                    noorbit_guardian::GuardianEvent::GivingUp { restarts_this_hour } => {
                        emit(
                            &app3,
                            "error",
                            "",
                            &format!("s'ha superat el límit de reinicis ({restarts_this_hour})"),
                            false,
                        );
                    }
                    noorbit_guardian::GuardianEvent::Summary(summary) => {
                        // Es deixa el resum en un fitxer compartit (el pare és
                        // l'únic que té les mesures de memòria) i s'afegirà al
                        // REPORT.md al final. FORA del repo.
                        let _ = std::fs::write(
                            &summary_path3,
                            serde_json::to_vec_pretty(&summary).unwrap_or_default(),
                        );
                        emit(
                            &app3,
                            "summary",
                            "",
                            &format!(
                                "resum: pic swap {} MB · pic RSS {} MB · {} mostres",
                                summary.peak_swap_bytes / 1024 / 1024,
                                summary.peak_rss_bytes / 1024 / 1024,
                                summary.samples
                            ),
                            true,
                        );
                    }
                    _ => {}
                }
                let _ = app3.emit("jam://state", jam3.snapshot());
            }
        });

        // El supervisor corre fins que el fill acaba o l'aturen.
        if let Err(e) = supervisor.run().await {
            // ABANS s'empassolava (`let _ = …`): l'usuari veia la Jam «aturada»
            // sense cap explicació del perquè (spawn del fill, protocol, E/S).
            fail(&app2, &jam_handle, format!("supervisor: {e}"));
        }
        // Drenem el canal: `abort()` deixaria esdeveniments pel camí (la
        // darrera tasca acabada no s'hauria vist mai a la UI). Quan el
        // supervisor acaba, tots els senders moren i el bucle del reader
        // retorna sol; el timeout és només per si de cas.
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            &mut reader,
        )
        .await;
        if !reader.is_finished() {
            reader.abort();
        }

        // Neteja: els fitxers interns del brief/pla/resum no han de quedar
        // a la carpeta de dades de l'usuari.
        let _ = std::fs::remove_file(&brief_path);
        let _ = std::fs::remove_file(&plan_path);

        // El resum de memòria s'escriu DESPRÉS (el pare és l'únic que el
        // té) i s'hi afegeix al REPORT.md que va escriure el fill.
        append_memory_summary(&summary_path, &report_path);
        // Si el fill NO ha pogut deixar cap informe (l'ha mort el guardian, la
        // màquina s'ha apagat, el supervisor ha fallat…), es deixa un
        // REPORT.md MÍNIM que explica què ha passat. Abans el projecte
        // quedava sense informe i l'usuari només veia un «No such file or
        // directory» sense cap pista. Si l'ha aturat el mateix usuari, no:
        // una aturada ordenada no és un fracàs i no ha de deixar res escrit.
        if !token.is_cancelled() {
            let reason = jam_handle.snapshot().error.unwrap_or_else(|| {
                "l'agent s'ha aturat sense deixar l'informe complet (el procés pot haver estat aturat pel supervisor o pel sistema)".to_string()
            });
            write_fallback_report(&report_path, &reason);
        }
        let _ = std::fs::remove_dir_all(&workdir);

        jam_handle.set_ai_path(None);
        jam_handle.set(|s| {
            s.running = false;
            s.stopping = false;
            s.finished_at = Some(chrono::Local::now().timestamp());
            // Si l'aturada la va demanar l'usuari, el detall ho diu (no és un
            // error: és una ordre, no un fracàs).
            if token.is_cancelled() {
                s.current_detail = "aturada per l'usuari".into();
            }
        });
        jam_handle.running.store(false, Ordering::SeqCst);
        let _ = app2.emit("jam://state", jam_handle.snapshot());
    });

    Ok(())
}

/// Resultat de la fase de pla.
#[derive(Debug)]
pub(super) enum PlanOutcome {
    Planned(noorbit_jam::Plan),
    /// L'usuari ha demanat aturar la Jam mentre la IA feia el pla.
    Cancelled,
    Failed(String),
}

/// Fa el pla amb la IA connectada: CANCEL·LABLE (l'atura de l'usuari la
/// talla de debò) i amb un sostre de temps (un model que no respon no ha de
/// deixar la Jam penjada per sempre sense que l'usuari ho noti).
///
/// El futur del pla s'injecta per poder-ho provar amb una IA falsa que no
/// acaba mai (vegeu `jam::tests`).
pub(super) async fn plan_with_future<Fut>(plan: Fut, token: &CancellationToken) -> PlanOutcome
where
    Fut: std::future::Future<Output = Result<noorbit_jam::Plan, String>>,
{
    tokio::select! {
        biased;
        _ = token.cancelled() => PlanOutcome::Cancelled,
        r = tokio::time::timeout(PLAN_TIMEOUT, plan) => match r {
            Ok(Ok(p)) => PlanOutcome::Planned(p),
            Ok(Err(e)) => PlanOutcome::Failed(format!("la IA no ha pogut fer el pla: {e}")),
            Err(_) => PlanOutcome::Failed("la IA no ha respost a temps per fer el pla".into()),
        },
    }
}

/// Recupera el resum del guardian (el pare el coneix) i l'afegeix al
/// REPORT.md que va escriure el fill.
fn append_memory_summary(summary_path: &std::path::Path, report_path: &std::path::Path) {
    // El guardian emet el resum cap al final; el pare el rep pel canal i
    // l'acaba d'escriure al fitxer de la carpeta de dades.
    let Ok(bytes) = std::fs::read(summary_path) else {
        return;
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return;
    };
    let num = |k: &str| v.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
    let mb = |b: u64| format!("{:.0} MB", b as f64 / 1024.0 / 1024.0);
    let mut extra = String::new();
    extra.push_str("\n## Memòria del guardian\n\n| Mètrica | Valor |\n|---|---|\n");
    extra.push_str(&format!("| Mostres | {} |\n", num("samples")));
    extra.push_str(&format!("| Pic swap | {} |\n", mb(num("peak_swap_bytes"))));
    extra.push_str(&format!("| Pic RSS de l'agent | {} |\n", mb(num("peak_rss_bytes"))));
    extra.push_str(&format!("| Durada vigilada | {} s |\n", num("duration_ms") / 1000));
    if let Ok(mut md) = std::fs::read_to_string(report_path) {
        md.push_str(&extra);
        let _ = std::fs::write(report_path, md);
    }
}

/// Llegeix el REPORT.md de la Jam amb un missatge HONEST quan encara no hi és.
///
/// Abans el frontend llegia el fitxer directament (`read_file`) i, si el
/// REPORT.md no existia (Jam aturada, fallida, o projecte en un volum lent on
/// la Jam no arriba a acabar), l'usuari veia l'error cru de Rust: «No s'ha
/// pogut llegir /Volumes/…/REPORT.md: No such file or directory (os error 2)».
/// Ara el backend diu què està passant de veritat i per què.
#[tauri::command]
pub async fn jam_read_report(jam: State<'_, Arc<JamHandle>>) -> Result<String, String> {
    let st = jam.snapshot();
    if st.report_path.trim().is_empty() {
        return Err("Encara no s'ha fet cap Jam: no hi ha cap REPORT.md.".into());
    }
    let path = std::path::PathBuf::from(&st.report_path);
    match tokio::fs::read_to_string(&path).await {
        Ok(text) => Ok(text),
        Err(e) => {
            // El motiu importa: si la Jam va fallar, el diu; si encara corre,
            // diu que és a punt; si el problema és el disc, ho diu.
            let mut msg = format!("Encara no hi ha cap REPORT.md a {}.", st.report_path);
            if let Some(err) = st.error.as_deref() {
                msg.push_str(&format!(" La Jam va fallar: {err}"));
            } else if st.finished_at.is_some() {
                msg.push_str(
                    " La Jam va acabar sense deixar l'informe (si el projecte és en un \
                     disc extern lent, és sovint que el git no arribi a temps).",
                );
            } else {
                msg.push_str(" La Jam encara no ha acabat: l'informe s'escriu quan acaba.");
            }
            if e.kind() == std::io::ErrorKind::NotFound {
                msg.push_str(" (El fitxer no existeix.)");
            }
            Err(msg)
        }
    }
}

/// Escript un REPORT.md MÍNIM quan el fill no ha pogut deixar-ne cap.
///
/// Sense això, una Jam que mor (procés killed, màquina apagada, error del
/// supervisor) deixa el projecte sense informe i l'usuari es queda amb un
/// «no such file or directory» sense cap pista del que va passar.
fn write_fallback_report(report_path: &std::path::Path, reason: &str) {
    if report_path.exists() {
        return;
    }
    let md = format!(
        "# Jam Report\n\n\
         - **Generat:** {}\n\
         - **Estat:** la Jam no va poder acabar\n\n\
         ## Per què\n\n\
         {}\n\n\
         _Aquest fitxer l'escriu NoOrbit quan la Jam s'atura sense haver pogut \
         deixar l'informe complet._\n",
        chrono::Local::now().to_rfc3339(),
        reason.trim()
    );
    if let Some(parent) = report_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(report_path, md);
}

/// Marca la Jam com a fallada i l'atura.
fn fail(app: &AppHandle, jam: &Arc<JamHandle>, msg: String) {
    emit(app, "error", "", &msg, false);
    jam.set(|s| {
        s.running = false;
        s.stopping = false;
        s.error = Some(msg);
        s.finished_at = Some(chrono::Local::now().timestamp());
    });
    jam.running.store(false, Ordering::SeqCst);
    let _ = app.emit("jam://state", jam.snapshot());
}

/// Marca la Jam com a ATURADA per l'usuari (sense error: no ha estat un
/// fracàs, sinó una ordre de l'usuari) i neteja l'estat.
fn stopped(app: &AppHandle, jam: &Arc<JamHandle>) {
    jam.set(|s| {
        s.running = false;
        s.stopping = false;
        s.current_detail = "aturada per l'usuari".into();
        s.finished_at = Some(chrono::Local::now().timestamp());
    });
    jam.running.store(false, Ordering::SeqCst);
    let _ = app.emit("jam://state", jam.snapshot());
}

/// RAM total del sistema (macOS/Linux), si es pot llegir.
fn total_system_ram() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        // sysctl hw.memsize
        let out = std::process::Command::new("sysctl").arg("-n").arg("hw.memsize").output().ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        return s.parse::<u64>().ok();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        meminfo
            .lines()
            .find_map(|l| l.strip_prefix("MemTotal:"))
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse::<u64>().ok())
            .map(|kb| kb * 1024)
    }
}

/// Atura la Jam.
///
/// Cancel·la el token —que respecten TOTS els actors: la fase de pla, el
/// guardian i el fill— i avisa la UI DE SEGUIDA. Abans no s'emetia cap estat
/// en aturar, així que prémer «Atura» no canvivia res a pantalla i l'usuari
/// pensava que el botó no funcionava.
#[tauri::command]
pub fn jam_stop(app: AppHandle, jam: State<'_, Arc<JamHandle>>) {
    request_stop(&jam);
    let _ = app.emit("jam://state", jam.snapshot());
    emit(&app, "note", "", "aturada demanada per l'usuari", true);
}

/// Cancel·lació de la Jam en curs: el token es cancel·la i l'estat passa a
/// «aturant…» fins que l'agent s'ha aturat del tot.
pub fn request_stop(jam: &JamHandle) {
    if let Ok(c) = jam.cancel.lock() {
        c.cancel();
    }
    jam.set(|s| {
        if s.running {
            s.stopping = true;
            s.current_detail = "aturant…".into();
        }
    });
}

/// CANVIA LA IA de la Jam en marxa (proveïdor + model).
///
/// L'usuari pot triar una altra IA des del panell SENSE aturar la Jam: el
/// pare ho escriu al fitxer d'IA i el fill el rellegeix (la tasca següent ja
/// va amb la IA nova). Si no hi ha cap Jam en marxa, avisa en comptes de
/// fer res en silenci.
#[tauri::command]
pub async fn jam_set_ai(
    app: AppHandle,
    state: State<'_, AppState>,
    jam: State<'_, Arc<JamHandle>>,
    provider: String,
    model: Option<String>,
) -> Result<(), String> {
    if !jam.running.load(Ordering::SeqCst) {
        return Err("no hi ha cap Jam en marxa".into());
    }
    // Es resol igual que a l'inici: si l'usuari tria Ollama però el motor
    // local és aturat, la Jam cau a la IA en línia o a la integrada (mai es
    // queda penjada esperant un servei que no hi és).
    let req = JamRequest {
        brief: String::new(),
        engine: String::new(),
        duration_secs: 60,
        repo_path: None,
        build_cmd: None,
        dry_run: false,
        provider: Some(provider),
        model,
    };
    let (provider, model) = crate::jam::resolve_jam_ai(&state.ai_manager, &req).await;
    let Some(path) = jam.ai_path() else {
        return Err("no s'ha pogut trobar la Jam en marxa".into());
    };
    write_jam_ai(&path, &provider, &model).map_err(|e| e.to_string())?;
    jam.set(|s| {
        s.provider = provider.clone();
        s.model = model.clone();
    });
    let _ = app.emit("jam://state", jam.snapshot());
    emit(
        &app,
        "note",
        "",
        &format!("IA canviada: {provider} · {model}"),
        true,
    );
    Ok(())
}

/// Estat actual.
#[tauri::command]
pub async fn jam_state(
    state: State<'_, AppState>,
    jam: State<'_, Arc<JamHandle>>,
) -> Result<JamState, String> {
    let mut st = jam.snapshot();
    if !st.running {
        // Quan no hi ha Jam en marxa, el panell mostra la IA que farà servir
        // la PRÒXIMA Jam (l'usuari pot haver canviat de proveïdor al xat).
        let (provider, model) = state.ai_manager.active_labels().await;
        st.provider = provider;
        st.model = model;
        st.stopping = false;
    }
    Ok(st)
}
