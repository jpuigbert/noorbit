//! NoOrbit — Nucli de l'aplicació (v0.5 amb Unreal Engine 5).

pub mod agent;
pub mod ai;
pub mod api;
pub mod autonomous;
pub mod blender;
pub mod commands;
pub mod computer;
pub mod config;
pub mod experts;
pub mod lsp;
pub mod mobile;
pub mod plugins;
pub mod skills;
pub mod unreal;

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
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = config::AppConfig::load().expect("Configuració NoOrbit");
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
    let experts = Arc::new(experts::ExpertManager::new(data_dir));

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
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .manage(agent_state)
        .manage(blender_state)
        .setup(move |app| {
            // «Arrenquen soles»: en obrir NoOrbit es reempren les feines
            // autònomes actives (amb espera per deixar pujar Ollama).
            let handle = app.handle().clone();
            autonomous::commands::resume_all(&handle);

            // Connexió automàtica a Blender (Objectiu 1): si és habilitada a
            // configuració, arrenca el vigilant que connecta sol quan detecta
            // el port 9876 obert (l'usuari només ha d'obrir Blender).
            if auto_connect_blender {
                crate::blender::auto_connect::spawn_auto_connect(handle.clone());
            }
            Ok(())
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
            commands::term::run_command,
            commands::workspace::open_workspace,
            commands::workspace::create_project,
            // Sistema operatiu
            commands::system::open_terminal,
            commands::system::reveal_in_finder,
            commands::system::system_total_memory,
            // IA
            commands::ai::send_prompt,
            commands::ai::send_prompt_stream,
            commands::ai::list_providers,
            commands::ai::set_active_provider,
            commands::ai::ollama_is_running,
            commands::ai::ollama_list_models,
            commands::ai::ollama_pull_model,
            commands::ai::ollama_delete_model,
            commands::ai::ollama_suggested_models,
            commands::ai::ollama_search_cloud,
            commands::ai::ollama_search_huggingface,
            commands::ai::ollama_installed,
            commands::ai::start_ollama,
            commands::ai::install_ollama,
            commands::ai::comfyui_installed,
            commands::ai::comfyui_running,
            commands::ai::start_comfyui,
            commands::ai::install_comfyui,
            commands::ai::ollama_get_models_dir,
            commands::ai::ollama_set_models_dir,
            commands::ai::list_external_volumes,
            commands::ai::ollama_set_model,
            commands::ai::ollama_active_model,
            commands::ai::ai_last_thinking,
            // Proveïdors d'IA en línia (tokens)
            commands::api::ai_provider_list,
            commands::api::ai_provider_add,
            commands::api::ai_provider_update,
            commands::api::ai_provider_remove,
            commands::api::ai_provider_test,
            commands::api::ai_provider_chat,
            commands::api::ai_select_provider,
            commands::api::ai_current_provider,
            // Skills
            commands::skills::list_skills,
            commands::skills::reload_skills,
            commands::skills_store::skills_store_list,
            commands::skills_store::skills_install_github,
            commands::skills_store::skills_install_url,
            commands::skills_store::skills_install_zip,
            commands::skills_store::skills_install_local,
            commands::skills_store::skills_uninstall,
            commands::skills_store::skills_restore_builtin,
            commands::skills_store::skills_export_external,
            // Config
            commands::config::get_config,
            commands::config::update_config,
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
            unreal::commands::unreal_auto_connect,
            unreal::commands::unreal_run_python,
            unreal::commands::unreal_create_blueprint,
            unreal::commands::unreal_spawn_actor,
            unreal::commands::unreal_import_asset,
            unreal::commands::unreal_build_lighting,
            unreal::commands::unreal_package_game,
            // Mòbil natiu (Android SDK + Xcode/iOS Simulator)
            mobile::commands::mobile_status,
            mobile::commands::mobile_android_list_avds,
            mobile::commands::mobile_android_start,
            mobile::commands::mobile_android_stop,
            mobile::commands::mobile_android_screenshot,
            mobile::commands::mobile_ios_list_devices,
            mobile::commands::mobile_ios_boot,
            mobile::commands::mobile_ios_shutdown,
            mobile::commands::mobile_ios_open,
            mobile::commands::mobile_ios_screenshot,
        ])
        .run(tauri::generate_context!())
        .expect("Error en executar NoOrbit");
}
