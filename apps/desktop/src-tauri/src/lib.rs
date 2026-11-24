//! NoOrbit — Nucli de l'aplicació (v0.5 amb Unreal Engine 5).

pub mod agent;
pub mod ai;
pub mod api;
pub mod acp;
pub mod autonomous;
pub mod bevy;
pub mod blender;
pub mod browser;
pub mod commands;
pub mod computer;
pub mod config;
pub mod crash;
pub mod engine_bridge;
pub mod engine_open;
pub mod engine_tools;
pub mod experts;
pub mod external_orchestrator;
pub mod fsutil;
pub mod git_ops;
pub mod godot;
pub mod grid;
pub mod imgen;
pub mod jam;
pub mod keep_awake;
pub mod learning;
pub mod lsp;
pub mod mobile;
pub mod plugins;
pub mod ports;
pub mod skills;
pub mod themes;
pub mod unreal;
pub mod project_work;
pub mod video;
pub mod voice;

use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

pub struct AppState {
    pub config: Arc<RwLock<config::AppConfig>>,
    pub ai_manager: Arc<ai::AiManager>,
    pub api_manager: Arc<Mutex<api::ApiManager>>,
    pub skills_manager: Arc<Mutex<skills::SkillsManager>>,
    pub plugin_manager: Arc<Mutex<plugins::PluginManager>>,
    pub computer: Arc<computer::ComputerController>,
    pub experts: Arc<experts::ExpertManager>,
    /// Orquestrador de l'agent: compartit entre les comandes d'agent i les
    /// feines autònomes (que necessiten `deep_think` per avançar).
    pub agent_orchestrator: Arc<agent::AgentOrchestrator>,
    /// Feines autònomes persistents (especialistes forjats en segon pla).
    pub autonomous: Arc<autonomous::AutonomousStore>,
    /// Control del treball en equip (evita dos equips alhora i permet aturar-lo).
    pub team: Arc<experts::TeamControl>,
    /// Aprenentatge local: registre de converses + lliçons apreses (tot local).
    pub learning: Arc<learning::LearningStore>,
    /// Autonomous Jam: estat i cancel·lació de la Jam en curs.
    pub jam: Arc<jam::commands::JamHandle>,
    /// Millora contínua autopilotada de qualsevol projecte (estat + cancel·lació).
    pub project_work: Arc<project_work::ProjectWorkHandle>,
    /// Muntatge de vídeo: render «a mà» (el botó del panell) en curs.
    pub video_render: Arc<video::commands::RenderHandle>,
    /// Muntatge de vídeo: el director autopilotat (mirada, guió, autocrítica).
    pub video_director: Arc<video::director::DirectorHandle>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Registra els panics en un fitxer dins de les dades de NoOrbit. Si l'app
    // mor a l'arrencada (p. ex. executada des d'un USB), queda una pista en
    // lloc d'un tancament sense cap informació. El hook viu al mòdul `crash`
    // (que també construeix l'informe que l'usuari pot enviar).
    let crash_dir = config::AppConfig::data_dir();
    crash::install_hook(crash_dir.clone());
    crash::mark_session_start(crash_dir.as_deref());

    let config = config::AppConfig::load_or_default();
    let skills_manager = skills::SkillsManager::new(&config);

    // Carpeta de dades: respecta el mode portàtil (USB) perquè configuració,
    // proveïdors, connectors i especialistes viatgen amb l'aplicació.
    let data_dir = config::AppConfig::data_dir()
        .unwrap_or_else(|| std::env::temp_dir().join("no-orbit"));
    let _ = std::fs::create_dir_all(&data_dir);
    let plugin_manager = plugins::PluginManager::new(data_dir.clone());

    // Proveïdors d'IA en línia (tokens) — creats abans que el gestor d'IA
    // perquè aquest els pugui usar com a alternatives a Ollama.
    let api_manager = Arc::new(Mutex::new(api::ApiManager::new(data_dir.clone())));
    let ai_manager = ai::AiManager::with_remote(&config, api_manager.clone());

    // Control de l'ordinador per la IA (permisos + confirmacions).
    let computer = Arc::new(computer::ComputerController::new(data_dir.clone()));

    // Feines autònomes: estat persistent (jobs.json) dels especialistes forjats.
    let autonomous = Arc::new(autonomous::AutonomousStore::new(data_dir.clone()));

    // Especialistes: agents d'IA creats per l'usuari (individuals i en equip).
    let experts = Arc::new(experts::ExpertManager::new(data_dir.clone()));

    // Aprenentatge local: el registre de converses i les lliçons apreses.
    let learning = Arc::new(learning::LearningStore::new(data_dir));
    // La porta d'avaluació del cervell necessita les proves (els 👍) des d'un
    // fil de fons, on no arriba l'estat de Tauri.
    learning::set_shared(learning.clone());

    let agent_config = agent::AgentConfig::default();
    let orchestrator = Arc::new(agent::AgentOrchestrator::new(agent_config));
    let agent_state = commands::agent::AgentState {
        orchestrator: orchestrator.clone(),
    };

    let blender_state = blender::commands::BlenderState::new();

    // Connexió automàtica a Blender (Objectiu 1): es llegeix ara, abans de
    // moure `config` dins de l'estat, perquè `.setup()` és síncron.
    let auto_connect_blender = config.blender.auto_connect;

    let state = AppState {
        config: Arc::new(RwLock::new(config)),
        ai_manager: Arc::new(ai_manager),
        api_manager,
        skills_manager: Arc::new(Mutex::new(skills_manager)),
        plugin_manager: Arc::new(Mutex::new(plugin_manager)),
        computer,
        experts,
        agent_orchestrator: orchestrator,
        autonomous,
        team: Arc::new(experts::TeamControl::new()),
        learning,
        jam: Arc::new(jam::commands::JamHandle::default()),
        project_work: Arc::new(project_work::ProjectWorkHandle::default()),
        video_render: Arc::new(video::commands::RenderHandle::default()),
        video_director: Arc::new(video::director::DirectorHandle::default()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        // El handle de la Jam es registra EXPLÍCITAMENT: les comandes
        // jam_start/jam_stop/jam_state demanen `State<Arc<JamHandle>>` i, sense
        // aquest manage, Tauri no el trobaria (petaria en runtime).
        .manage(state.jam.clone())
        // Handle de la millora de projectes (mateix patró que la Jam).
        .manage(state.project_work.clone())
        // El muntatge de vídeo té dos handles: el render a mà i el director.
        .manage(state.video_render.clone())
        .manage(state.video_director.clone())
        .manage(state)
        .manage(agent_state)
        .manage(blender_state)
        .setup(move |app| {
            use tauri::Manager;
            // «Arrenquen soles»: en obrir NoOrbit es reempren les feines
            // autònomes actives (amb espera per deixar pujar Ollama).
            let handle = app.handle().clone();
            // Enganxa l'aplicació al gestor d'IA: els camins que no la reben
            // com a paràmetre (el xat sense streaming, que usen els agents i
            // els planificadors) necessiten poder emetre la comptabilització
            // de tokens del torn.
            app.state::<AppState>()
                .ai_manager
                .set_app(handle.clone());
            autonomous::commands::resume_all(&handle);
            // Registres de servidors locals orfes (l'app anterior va morir
            // de cop): es netejen abans d'arrencar res, sinó el port del
            // motor local apareix ocupat per un servei que no existeix.
            crate::ai::llamacpp::cleanup_stale();
            // IA NATIVA neuronal: es resol on viu el model GGUF empaquetat (i
            // el motor, si n'hi ha) perquè la nativa tinga un LLM real de
            // ple dret sense dependre de la ruta de l'instal·lador.
            crate::ai::native_llm::init(&handle);

            // SNAPSHOTS GIT SHADOW: el git-dir ocult viu a les dades de
            // l'app (app_data_dir), MAI dins del projecte ni del seu .git.
            // Si Tauri no pot resoldre app_data_dir, es degrada al
            // data_dir de config (mode portàtil inclòs).
            let snap_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| {
                    config::AppConfig::data_dir()
                        .unwrap_or_else(|| std::env::temp_dir().join("no-orbit"))
                });
            crate::agent::snapshot::init(snap_dir);

            // MUNTATGE DE VÍDEO: els bucles d'IA (Jam, treball autopilotat, xat
            // amb eines de motor) no reben `State<'_, …>`, així que els handles
            // del render i del director es publiquen aquí una vegada. Sense
            // aquest pont, `@video:render` i `@video:director` quedarien morts.
            {
                use tauri::Manager as _;
                let st = app.state::<AppState>();
                video::commands::set_runtime(video::commands::Runtime {
                    app: handle.clone(),
                    ai: st.ai_manager.clone(),
                    render: st.video_render.clone(),
                    director: st.video_director.clone(),
                });
            }

            // XARXA ALTRUISTA P2P (grid): el runtime es publica un cop, com
            // els motors — els fils de la Jam no reben `State<'_, …>`. Els
            // clients voluntaris arrenquen sols segons la configuració.
            {
                use tauri::Manager as _;
                let st = app.state::<AppState>();
                let grid_dir = config::AppConfig::data_dir()
                    .unwrap_or_else(|| std::env::temp_dir().join("no-orbit"))
                    .join("grid");
                let grid_cfg = st.config.blocking_read().grid.clone();
                let rt = grid::Runtime {
                    app: handle.clone(),
                    ai: st.ai_manager.clone(),
                    cfg: st.config.clone(),
                    jam: st.jam.clone(),
                    state: grid::GridState::new(&grid_cfg, grid_dir),
                };
                grid::set_runtime(rt);
                tauri::async_runtime::spawn(grid::arranca_segons_config(grid_cfg));
            }

            // PROVEÏDORS LOCALS MORTS: un «llamacpp_11435» desat quan el
            // servidor ja no hi és (l'app es va tancar i el `llama-server`
            // va morir amb ella) es treu de la llista. Sinó, el xat el
            // ofereix al selector i no hi ha res al darrere: l'usuari tria
            // el model i «no deixa fer-lo servir».
            {
                // El gestor de proveïdors es clona (Arc) per no tenir el
                // prestat del `State` viu més enllà del bloc.
                let api_manager = {
                    use tauri::Manager as _;
                    app.state::<AppState>().api_manager.clone()
                };
                if let Ok(mut api) = api_manager.lock() {
                    let ids: Vec<String> = crate::ai::llamacpp::servers()
                        .into_iter()
                        .filter(|s| crate::ai::llamacpp::is_alive(s.port))
                        .map(|s| format!("llamacpp_{}", s.port))
                        .collect();
                    let morts: Vec<String> = api
                        .list()
                        .iter()
                        .filter(|p| p.id.starts_with("llamacpp_") && !ids.contains(&p.id))
                        .map(|p| p.id.clone())
                        .collect();
                    for id in morts {
                        let _ = api.remove(&id);
                    }
                };
            }

            // Descàrregues de models tallades (l'app es va tancar mentre una
            // IA baixava un model): el «.part» a mitges s'esborra en arrencar
            // perquè no ocupe disc ni es done per bo. Només toca «*.part».
            crate::ai::hf_bay::cleanup_partial_downloads();

            // ECC DE SÈRIE: el coneixement d'ECC (skills, agents i regles)
            // viatja amb l'instal·lador (`ecc-builtin/`) i es sembra al
            // directori de skills el PRIMER cop, sense xarxa. En un fil a
            // part: són ~7 MB i 400 carpetes, l'arrencada no s'ha d'alentir.
            {
                let ecc_handle = handle.clone();
                std::thread::spawn(move || {
                    use tauri::Manager as _;
                    let state = ecc_handle.state::<AppState>();
                    let dir = match state.skills_manager.lock() {
                        Ok(g) => g.skills_dir().to_path_buf(),
                        Err(_) => return,
                    };
                    if let Some(report) = crate::commands::ecc::seed_ecc_builtin(
                        Some(&ecc_handle),
                        &dir,
                        &state.experts,
                    ) {
                        if let Ok(mut g) = state.skills_manager.lock() {
                            g.reload();
                        }
                        eprintln!(
                            "NoOrbit/ECC: de sèrie sembrat ({} skills, {} agents, {} regles)",
                            report.skills, report.agents, report.rules
                        );
                    }
                });
            }

            // Connexió automàtica a Blender (Objectiu 1): si és habilitada a
            // configuració, arrenca el vigilant que connecta sol quan detecta
            // el port 9876 obert (l'usuari només ha d'obrir Blender).
            if auto_connect_blender {
                crate::blender::auto_connect::spawn_auto_connect(handle.clone());
            }

            // IA local sense trencacols: quan NoOrbit s'obre, si el port
            // 11434 no escolta, arrenca `ollama serve` en segon pla; i si
            // Ollama no hi és a l'ordinador, el BAIXA i l'INSTAL·LA sol
            // (descàrrega oficial d'ollama.com) i després l'engega.
            let ollama_handle = handle.clone();
            std::thread::spawn(move || {
                // NOMÉS si no escolta res encara: si Ollama ja corria no cal
                // instal·lar ni arrencar res — però el model inicial es
                // comprova SEMPRE (més avall), perquè un Ollama ja en marxa
                // pot no tenir CAP model i tots els xats donarien 404.
                if !crate::ai::ollama_running() {
                    if !crate::ai::ollama_installed() {
                        match crate::ai::ensure_ollama_installed() {
                            Ok(msg) => eprintln!("NoOrbit/Ollama: {}", msg),
                            Err(e) => eprintln!("NoOrbit/Ollama: no s'ha pogut instal·lar: {e}"),
                        }
                    }
                    if let Err(e) = crate::ai::start_ollama_server() {
                        eprintln!("NoOrbit/Ollama: no s'ha pogut arrencar el servei: {e}");
                    }
                }
                // USUARI NOU: un Ollama acabat d'instal·lar (o JA en marxa però
                // sense cap model baixat) fa que TOTS els xats fallin amb 404.
                // Si no n'hi ha cap, baixa'n un d'inicial (mida segons la RAM)
                // amb progrés — sense que l'usuari hagi de fer res ni entendre
                // res d'Ollama. ABANS només es feia quan Nosaltres havíem
                // arrencat el servei: qui ja tenia Ollama obert mai rebia cap
                // model i veia «404 model not found» amb Ollama «disponible».
                let ai = {
                    use tauri::Manager as _;
                    let state = ollama_handle.state::<AppState>();
                    state.ai_manager.clone()
                };
                tauri::async_runtime::spawn(async move {
                    ai.ensure_default_model(&ollama_handle).await;
                });
            });
            // APRENENTATGE AUTÒNOM EN REPÒS: la IA nativa navega sola i aprén
            // de contingut lliure (StackOverflow → Viquipèdia → web → profunda)
            // només quan ningú no toca el programa, no hi ha cap generació en
            // curs i la RAM és lliure. Es pot desactivar a Configuració.
            crate::ai::wander::spawn(handle.clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                use tauri::{Emitter as _, Manager as _};
                let label = window.label();
                if label == "main" {
                    // En sortir de l'app, els servidors locals que NoOrbit ha
                    // engegat (llama.cpp) es tanquen: si no, quedaven com a
                    // processos orfes ocupant el port. Només ho fem en tancar
                    // la finestra PRINCIPAL: si ho féssim a qualsevol finestra,
                    // tancar un panell separat (xat o terminal) mataria la IA
                    // mentre l'app segueix oberta.
                    crate::ai::llamacpp::stop_all();
                    // Sessions PTY del terminal: cap shell òrfena en sortir.
                    crate::commands::term::kill_all();
                    // Sessió LSP persistent (Fase 3c): cap servidor orfe.
                    crate::lsp::session::kill_all();
                } else if label == "noorbit-chat" || label == "noorbit-terminal" {
                    // Xarxa de seguretat: si una finestra separada mor sense
                    // passar pel seu tancament JS (crash del webview, tancament
                    // extern), el programa principal es quedaria amb el panell
                    // «destacat» per sempre. El listener de windowing.ts és
                    // idempotent: si el flag ja és net, no fa res.
                    let _ = window
                        .app_handle()
                        .emit("panel://closed", serde_json::json!({ "label": label }));
                } else if label == "noorbit-screensaver" {
                    // L'estalvi s'ha tancat: el voluntari deixa de tenir la
                    // porta triple oberta (els clics van a la seva finestra,
                    // no a la principal).
                    crate::grid::nota_estalvi(false);
                }
            }
            let _ = window;
        })
        .invoke_handler(tauri::generate_handler![
            // Fitxers
            commands::fs::read_file,
            commands::fs::write_file,
            commands::fs::list_dir,
            commands::fs::create_dir,
            commands::fs::delete_path,
            commands::fs::rename_path,
            commands::fs::collect_project_files,
            commands::fs::search_files,
            commands::fs::find_definition,
            commands::git::git_remote_get,
            commands::git::git_remote_set,
            commands::git::git_sync_status,
            commands::git::git_sync,
            commands::git::git_status,
            commands::git::git_diff_file,
            commands::git::git_commit,
            commands::git::git_push,
            commands::git::git_pull,
            commands::git::git_init,
            commands::git::git_stash,
            commands::git::git_stash_list,
            commands::git::git_stash_pop,
            commands::git::git_log,
            // Branques i worktrees: llistar, crear, canviar i esborrar.
            commands::git::git_branches,
            commands::git::git_branch_create,
            commands::git::git_branch_checkout,
            commands::git::git_branch_delete,
            commands::git::git_worktrees,
            commands::git::git_worktree_prune,
            // Snapshots git shadow per pas d'agent (desfer fiable, Fase 1d).
            commands::snapshot::snapshot_count,
            commands::snapshot::snapshot_undo,
            commands::term::run_command,
            commands::term::kill_command,
            // Terminal PTY real multi-sessió (Fase 3b).
            commands::term::term_start,
            commands::term::term_write,
            commands::term::term_resize,
            commands::term::term_kill,
            commands::term::term_list,
            commands::term::term_scrollback,
            commands::workspace::open_workspace,
            commands::workspace::create_project,
            jam::commands::jam_plan,
            jam::commands::jam_start,
            jam::commands::jam_stop,
            jam::commands::jam_state,
            // Llegir el REPORT.md amb un missatge HONEST quan encara no hi és
            // (Jam fallida, aturada o projecte en un volum lent).
            jam::commands::jam_read_report,
            // Canviar la IA de la Jam EN MARXA (sense aturar-la).
            jam::commands::jam_set_ai,
            // Xarxa altruista P2P (grid): estat, compartició i còpies.
            grid::commands::grid_status,
            grid::commands::grid_set_enabled,
            grid::commands::grid_share_new,
            grid::commands::grid_share_stop,
            grid::commands::grid_peer_add,
            grid::commands::grid_peer_remove,
            grid::commands::grid_note_saver,
            grid::commands::grid_distribute_start,
            grid::commands::grid_distribute_stop,
            grid::commands::grid_copy_status,
            grid::commands::grid_copy_set_dir,
            engine_open::open_project_in_engine,
            engine_open::godot_status,
            // Arregla Godot a macOS: li treu la marca de quarantina (evita el
            // SIGBUS «backing vnode was force unmounted»).
            engine_open::godot_fix_quarantine,
            engine_open::bevy_status,
            // Assistent de Bevy: preguntes (on instal·lar Rust, on treballar),
            // instal·lació i creació/verificació del projecte.
            bevy::setup::bevy_setup_info,
            bevy::setup::bevy_detect_project,
            bevy::setup::bevy_create_project,
            bevy::setup::bevy_install_rust,
            commands::workspace::clone_game_studios,
            // Sistema operatiu
            commands::system::open_terminal,
            commands::system::reveal_in_finder,
            commands::system::open_externally,
            commands::system::open_tool_download,
            commands::system::install_blender,
            commands::system::exit_app,
            commands::system::system_total_memory,
            commands::system::system_memory_status,
            // Mode portàtil (USB / disc extern)
            commands::portable::portable_status,
            commands::portable::portable_enable,
            commands::portable::portable_create_bundle,
            commands::portable::portable_copy_app_to,
            commands::portable::portable_disable,
            // Actualitzacions (GitHub Releases)
            commands::update::check_update,
            // IA
            commands::ai::send_prompt,
            commands::ai::ai_chat_plain,
            commands::ai::send_prompt_stream,
            // Mode del motor (auto/local/cloud) — Fase 2.
            commands::ai::ai_engine_mode_get,
            commands::ai::ai_engine_mode_set,
            // Verificació de sintaxi barata (php -l / node --check) per a la UI.
            commands::ai::lint_paths,
            commands::ai::list_providers,
            commands::ai::set_active_provider,
            commands::ai::ollama_is_running,
            commands::ai::ollama_starter_model,
            commands::ai::ollama_pull_starter,
            commands::ai::ollama_list_models,
            commands::ai::ollama_pull_model,
            commands::ai::ollama_delete_model,
            commands::ai::ollama_loaded_models,
            commands::ai::ollama_unload_models,
            // Allibera la memòria que NoOrbit no necessita (models + motors).
            commands::ai::ai_free_ram,
            // Memòria que està usant la IA local ara mateix (model + sistema).
            commands::ai::ai_local_memory,
            commands::ai::ollama_get_keep_alive,
            commands::ai::ollama_set_keep_alive,
            commands::ai::ollama_suggested_models,
            commands::ai::ai_fit_check,
            commands::ai::ollama_search_cloud,
            commands::ai::ollama_search_huggingface,
            commands::ai::hf_bay_search,
            commands::ai::hf_bay_page,
            commands::ai::llamacpp_status,
            commands::ai::llamacpp_set_dirs,
            commands::ai::llamacpp_install,
            commands::ai::llamacpp_start,
            commands::ai::llamacpp_stop,
            commands::ai::llamacpp_remove_model,
            commands::ai::hf_bay_details,
            commands::ai::hf_bay_download,
            commands::ai::hf_bay_install_native,
            commands::ai::hf_bay_cancel,
            commands::ai::hf_bay_disk_space,
            commands::ai::ollama_installed,
            commands::ai::start_ollama,
            commands::ai::install_ollama,
            commands::ai::comfyui_installed,
            commands::ai::comfyui_running,
            commands::ai::start_comfyui,
            commands::ai::install_comfyui,
            commands::ai::install_image_cpu,
            commands::ai::image_cpu_info,
            commands::ai::image_cpu_setup,
            commands::ai::image_cpu_download_model,
            commands::ai::install_video_editor,
            commands::ai::civitai_search,
            commands::ai::install_local_ai,
            commands::ai::dsh_installed,
            commands::ai::install_dsh,
            commands::ai::start_dsh_web,
            commands::ai::dsh_web_running,
            commands::ai::copilot_installed,
            commands::ai::copilot_version,
            commands::ai::install_copilot,
            commands::ai::acp_permission_reply,
            commands::ai::ollama_get_models_dir,
            commands::ai::ollama_set_models_dir,
            commands::ai::ollama_migrate_models,
            commands::ai::list_external_volumes,
            commands::ai::ollama_set_model,
            commands::ai::ollama_active_model,
            commands::ai::ai_last_thinking,
            // APRENENTATGE AUTÒNOM EN REPÒS: estat, commutador, sessió ara,
            // adreces de la Internet profunda i cor d'activitat de la interfície.
            commands::ai::wander_status,
            commands::ai::wander_set,
            commands::ai::wander_run,
            commands::ai::wander_profunda_set,
            commands::ai::native_note_activity,
            // Catàleg d'IAs: censura, visió, generació d'imatges/vídeo i mida
            // aproximada, ABANS de descarregar res
            commands::ai::ia_catalog,
            commands::image::image_preview_base64,
            // Orquestració d'IAs externes (rescat amb origen etiquetat)
            commands::ai::external_ias_discover,
            commands::ai::delegate_external_ia,
            // Generació i manipulació d'imatges (local si hi ha recursos,
            // online només amb els proveïdors i tokens registrats per l'usuari)
            commands::image::image_generate,
            commands::image::image_edit,
            commands::image::image_transform,
            commands::image::image_import,
                        commands::image::file_import,
                        commands::image::file_context_read,
            // Navegador web intern (quan la IA ho necessiti: llegir pàgines
            // renderitzades; les accions humanes es mostren, mai se supla ningú)
            commands::browser::browser_open,
            commands::browser::browser_navigate,
            commands::browser::browser_status,
            commands::browser::browser_set_visible,
            commands::browser::browser_close,
            commands::browser::browser_extract,
            commands::browser::browser_eval,
            // Consulta MANUAL a IAs web (sense token: l'usuari logueja el seu
            // compte al navegador intern i la IA llegeix el que hi ha en pantalla)
            commands::browser::web_ia_list,
            commands::browser::web_ia_open,
            commands::browser::web_ia_status,
            commands::browser::web_ia_ask,
            commands::browser::web_ia_grab_code,
            // Menú «Pujar»: temes WordPress al marketplace
            commands::themes::theme_inspect,
            commands::themes::theme_build_zip,
            commands::themes::theme_upload,
            commands::themes::theme_open_login,
            commands::themes::theme_upload_session,
            commands::themes::theme_check_auth,
            commands::themes::theme_open_sales_page,
            // DICTAT PER VEU al xat (botó de micròfon del composer)
            commands::voice::voice_status,
            commands::voice::voice_transcribe,
            // Millora contínua AUTOPILOTADA de qualsevol projecte (temps de treball)
            project_work::wp_theme_work,
            project_work::project_work,
            project_work::project_work_stop,
            project_work::project_work_state,
            // MUNTATGE DE VÍDEO (menú «Vídeo»: Blender → Kdenlive/ffmpeg amb IA)
            video::commands::video_tools,
            video::commands::video_scan,
            video::commands::video_sheet,
            video::commands::video_edl_get,
            video::commands::video_edl_set,
            video::commands::video_edl_parse,
            video::commands::video_auto_edl,
            video::commands::video_source_add,
            video::commands::video_sources_set,
            video::commands::video_plan,
            video::commands::video_render,
            video::commands::video_render_state,
            video::commands::video_render_stop,
            video::commands::video_export,
            video::commands::video_verify,
            video::commands::video_open,
            video::commands::video_open_kdenlive,
            video::commands::video_director_start,
            video::commands::video_director_stop,
            video::commands::video_director_state,
            video::commands::video_transitions,
            video::commands::video_directive,
            // Proveïdors d'IA en línia (tokens)
            commands::api::ai_provider_list,
            commands::api::ai_provider_add,
            commands::api::ai_provider_update,
            commands::api::ai_provider_remove,
            commands::api::ai_provider_test,
            commands::api::ai_inline_complete,
            commands::api::ai_inline_complete_cancel,
            commands::api::ai_endpoint_check,
            commands::api::ai_remote_deploy,
            commands::api::ai_remote_verify,
            commands::api::ai_provider_chat,
            commands::api::ai_select_provider,
            commands::api::ai_current_provider,
            // Skills
            commands::skills::list_skills,
            commands::skills::reload_skills,
            commands::skills::vscode_import_skills,
            commands::ecc::ecc_install,
            commands::soup::soup_export_recipe,
            commands::vscode_extensions::vscode_list_extensions,
            commands::skills_store::skills_store_list,
            commands::skills_store::skills_install_github,
            commands::skills_store::skills_install_url,
            commands::skills_store::skills_install_zip,
            commands::skills_store::skills_install_local,
            commands::skills_store::skills_preview,
            commands::skills_store::skills_uninstall,
            commands::skills_store::skills_restore_builtin,
            commands::skills_store::skills_export_external,
            // Config
            commands::config::get_config,
            commands::config::update_config,
            commands::config::list_lsp_servers,
            // LSP headless persistent (Fase 3c): diagnòstics i IntelliSense
            // reals amb degradació honesta si no hi ha servidor instal·lat.
            lsp::commands::lsp_completion,
            lsp::commands::lsp_hover,
            lsp::commands::lsp_references,
            lsp::commands::lsp_definition,
            lsp::commands::lsp_rename,
            lsp::commands::lsp_diagnostics,
            lsp::commands::lsp_status,
            // Informe de fallides («crash»): es pot copiar i enviar.
            commands::crash::crash_report,
            commands::crash::crash_log_path,
            // Preview
            commands::preview::start_preview_server,
            commands::preview::stop_preview_server,
            // Plugins
            commands::plugins::list_plugins,
            commands::plugins::set_plugin_enabled,
            commands::plugins::detect_blender,
            commands::plugins::run_blender_script,
            // Agent IA
            commands::agent::agent_check_backends,
            commands::agent::agent_run,
            commands::agent::agent_plan,
            commands::agent::agent_quick,
            commands::agent::agent_status,
            commands::agent::agent_stop,
            commands::agent::agent_apply_blender,
            commands::agent::agent_compact_context,
            commands::agent::agent_apply_unreal,
            commands::agent::agent_deep_think,
            commands::agent::agent_forge,
            // Tasques amb pressupost de temps (degradació a segon pla).
            commands::agent::task_bring_to_foreground,
            // Feines autònomes (especialistes forjats en segon pla).
            autonomous::commands::autonomous_list,
            autonomous::commands::autonomous_set_active,
            autonomous::commands::autonomous_remove,
            // Control de l'ordinador (amb permisos)
            commands::computer::computer_permissions,
            commands::computer::computer_set_permissions,
            commands::computer::computer_remove_pattern,
            commands::computer::computer_history,
            commands::computer::computer_clear_history,
            commands::computer::computer_execute,
            commands::computer::computer_confirm,
            commands::computer::computer_agent_run,
            // Especialistes (agents d'IA creats per l'usuari) i treball en equip
            commands::experts::experts_list,
            commands::experts::expert_create,
            commands::experts::expert_update,
            commands::experts::expert_delete,
            commands::experts::expert_run,
            commands::experts::experts_team_run,
            commands::experts::experts_team_stop,
            // Aprenentatge local (la IA que aprèn de les altres IAs)
            commands::learning::learning_status,
            commands::learning::learning_set_config,
            commands::learning::learning_list,
            commands::learning::learning_delete_lesson,
            commands::learning::learning_clear,
            commands::learning::learning_distill,
            commands::learning::learning_export,
            commands::learning::learning_record,
            // Bucle de dades: els vots del xat i el paquet que entén Soup
            commands::learning_soup::learning_vote,
            commands::learning_soup::learning_soup_export,
            // Veredicte de la porta d'avaluació del cervell natiu
            commands::learning_soup::learning_brain_verdict,
            // Blender
            blender::commands::blender_ping,
            blender::commands::blender_info,
            blender::commands::blender_execute,
            blender::commands::blender_live_start,
            blender::commands::blender_live_stop,
            blender::commands::blender_watch_workspace,
            blender::commands::blender_run_code,
            blender::commands::blender_detect_path,
            blender::commands::blender_port_open,
            blender::commands::blender_auto_connect,
            blender::commands::blender_auto_connect_start,
            blender::commands::blender_auto_connect_stop,
            blender::commands::blender_auto_connect_status,
            blender::commands::blender_status,
            blender::commands::blender_launch,
            blender::commands::blender_install_addon,
            blender::commands::blender_install_mcp_addon,
            // Unreal Engine 5
            unreal::commands::unreal_ping,
            unreal::commands::unreal_info,
            unreal::commands::unreal_detect_installations,
            unreal::commands::unreal_detect_pending_installations,
            unreal::commands::unreal_find_uproject,
            unreal::commands::unreal_get_rc_port,
            unreal::commands::unreal_set_rc_port,
            unreal::commands::unreal_auto_connect,
            unreal::commands::unreal_run_python,
            unreal::commands::unreal_create_blueprint,
            unreal::commands::unreal_spawn_actor,
            unreal::commands::unreal_import_asset,
            unreal::commands::unreal_build_lighting,
            unreal::commands::unreal_package_game,
            unreal::commands::unreal_agent_setup,
            unreal::commands::unreal_agent_task,
            unreal::commands::unreal_mcp_tools,
            unreal::commands::unreal_mcp_call,
            unreal::commands::unreal_search_snippets,
            // Mòbil natiu (Android SDK + Xcode/iOS Simulator)
            mobile::commands::mobile_status,
            mobile::commands::mobile_android_list_avds,
            mobile::commands::mobile_android_start,
            mobile::commands::mobile_android_stop,
            mobile::commands::mobile_android_screenshot,
            mobile::commands::android_get_sdk_dir,
            mobile::commands::android_set_sdk_dir,
            mobile::commands::android_install_sdk,
            mobile::commands::android_project_marker,
            mobile::commands::mobile_ios_list_devices,
            mobile::commands::mobile_ios_boot,
            mobile::commands::mobile_ios_shutdown,
            mobile::commands::mobile_ios_open,
            mobile::commands::mobile_ios_screenshot,
        ])
        .run(tauri::generate_context!())
        .expect("Error en executar NoOrbit");
}
