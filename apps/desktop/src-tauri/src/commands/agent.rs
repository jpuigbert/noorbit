//! Comandes Tauri per a l'agent IA multimodal.

use crate::agent::{AgentRequest, AgentResult, ApplyRequest, BackendInfo, TaskBudget};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{command, AppHandle, State};

pub struct AgentState {
    pub orchestrator: Arc<crate::agent::AgentOrchestrator>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunInput {
    pub prompt: String,
    pub modality: crate::agent::Modality,
    pub workspace: Option<String>,
    /// Xat que ha enviat l'ordre: els seus events i la seua aturada.
    #[serde(default)]
    pub session: Option<String>,
}

#[command]
pub async fn agent_check_backends(
    state: State<'_, AgentState>,
) -> Result<Vec<BackendInfo>, String> {
    let orch = state.orchestrator.clone();
    Ok(orch.check_backends().await)
}

#[command]
pub async fn agent_run(
    state: State<'_, AgentState>,
    app: State<'_, AppState>,
    handle: AppHandle,
    input: AgentRunInput,
) -> Result<AgentResult, String> {
    let orch = state.orchestrator.clone();
    let ai = app.ai_manager.clone();
    let request = AgentRequest {
        prompt: input.prompt,
        modality: input.modality,
        workspace: input.workspace,
    };
    // Objectiu 4: executa amb un pressupost de temps. Si la feina supera el
    // llindar de primer pla, es degrada sola a segon pla (menos CPU/memòria)
    // i la UI en rep l'estat amb l'event «task://state».
    let (fg_s, total_s) = {
        let c = app.config.read().await;
        (c.tasks.foreground_limit_s, c.tasks.total_limit_s)
    };
    let budget = TaskBudget::from_limits(fg_s, total_s);
    let sid = input
        .session
        .clone()
        .unwrap_or_else(|| crate::ai::DEFAULT_SESSION.to_string());
    crate::ai::in_session(
        &sid,
        orch.run_with_budget(&handle, &ai, &request, budget),
    )
    .await
    .map_err(|e| e.to_string())
}

/// Porta una tasca degradada a segon pla de nou a primer pla (botó «Mostra»
/// de l'indicador de la barra d'estat). `run_with_budget` recull la petició
/// al següent poll i restaura la prioritat i el mode complet de la IA.
#[command]
pub async fn task_bring_to_foreground(task_id: String) -> Result<(), String> {
    crate::agent::request_foreground(&task_id);
    Ok(())
}

#[command]
pub async fn agent_plan(
    state: State<'_, AgentState>,
    app: State<'_, AppState>,
    input: AgentRunInput,
) -> Result<AgentResult, String> {
    let orch = state.orchestrator.clone();
    let ai = app.ai_manager.clone();
    let request = AgentRequest {
        prompt: input.prompt,
        modality: input.modality,
        workspace: input.workspace,
    };
    crate::ai::in_session(
        &input
            .session
            .clone()
            .unwrap_or_else(|| crate::ai::DEFAULT_SESSION.to_string()),
        orch.plan(&ai, &request),
    )
    .await
    .map_err(|e| e.to_string())
}

#[command]
pub async fn agent_quick(
    state: State<'_, AgentState>,
    app: State<'_, AppState>,
    prompt: String,
    session: Option<String>,
) -> Result<String, String> {
    let orch = state.orchestrator.clone();
    let ai = app.ai_manager.clone();
    let sid = session.unwrap_or_else(|| crate::ai::DEFAULT_SESSION.to_string());
    crate::ai::in_session(&sid, orch.quick(&ai, &prompt))
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn agent_status(state: State<'_, AgentState>) -> Result<serde_json::Value, String> {
    let orch = state.orchestrator.clone();
    Ok(orch.status())
}

/// Atura la generació de la IA en curs (botó «Atura» del xat). Amb `session`
/// només talla AQUELL xat; els altres poden seguir treballant en paral·lel.
#[command]
pub async fn agent_stop(
    app: State<'_, AppState>,
    session: Option<String>,
) -> Result<(), String> {
    app.ai_manager.request_stop(session.as_deref());
    Ok(())
}

/// Aplica el text de l'usuari a Blender: la IA genera l'script bpy i
/// s'executa al pont. Retorna un missatge de resultat per a la UI.
#[command]
pub async fn agent_apply_blender(
    state: State<'_, AgentState>,
    app: State<'_, AppState>,
    handle: AppHandle,
    input: ApplyRequest,
) -> Result<String, String> {
    let orch = state.orchestrator.clone();
    let ai = app.ai_manager.clone();
    let sid = input
        .session
        .clone()
        .unwrap_or_else(|| crate::ai::DEFAULT_SESSION.to_string());
    let (msg, _ok) = crate::ai::in_session(&sid, orch.apply_blender(&ai, &handle, &input.prompt))
        .await
        .map_err(|e| e.to_string())?;
    Ok(msg)
}

/// Aplica el text de l'usuari a Unreal Engine (py via Remote Control).
#[command]
pub async fn agent_apply_unreal(
    state: State<'_, AgentState>,
    app: State<'_, AppState>,
    handle: AppHandle,
    input: ApplyRequest,
    config: crate::unreal::commands::UnrealConfig,
) -> Result<String, String> {
    let orch = state.orchestrator.clone();
    let ai = app.ai_manager.clone();
    let sid = input
        .session
        .clone()
        .unwrap_or_else(|| crate::ai::DEFAULT_SESSION.to_string());
    let (msg, _ok) =
        crate::ai::in_session(&sid, orch.apply_unreal(&ai, &handle, &input.prompt, &config))
            .await
            .map_err(|e| e.to_string())?;
    Ok(msg)
}

/// Mode «pensament profund»: cerca a internet (general + StackOverflow +
/// GitHub), consulta les IAs en línia configurades i sintetitza amb el model
/// actiu. Cada fase arriba a la UI com a progrés (agent://progress).
#[command]
pub async fn agent_deep_think(
    state: State<'_, AgentState>,
    app: State<'_, AppState>,
    handle: AppHandle,
    input: ApplyRequest,
) -> Result<String, String> {
    let orch = state.orchestrator.clone();
    let ai = app.ai_manager.clone();
    let sid = input
        .session
        .clone()
        .unwrap_or_else(|| crate::ai::DEFAULT_SESSION.to_string());
    crate::ai::in_session(&sid, orch.deep_think(&ai, &handle, &input.prompt))
        .await
        .map_err(|e| e.to_string())
}

/// Forja un especialista autònom per a un objectiu que la IA local no resol
/// sola: torna la feina creada (per seguir-ne el fil des del xat).
#[command]
pub async fn agent_forge(
    state: State<'_, AgentState>,
    app: State<'_, AppState>,
    handle: AppHandle,
    input: ApplyRequest,
) -> Result<serde_json::Value, String> {
    let orch = state.orchestrator.clone();
    let ai = app.ai_manager.clone();
    let sid = input
        .session
        .clone()
        .unwrap_or_else(|| crate::ai::DEFAULT_SESSION.to_string());
    let result = crate::ai::in_session(
        &sid,
        orch.forge(&ai, &handle, &app.experts, &app.autonomous, &input.prompt),
    )
    .await
    .map_err(|e| e.to_string())?;
    // Enllesta la feina acabada de néixer: el worker la recull ara mateix.
    if let Some(job) = result.get("job").cloned().and_then(|j| {
        serde_json::from_value::<crate::autonomous::Job>(j).ok()
    }) {
        crate::autonomous::commands::spawn_job(&handle, &app, job);
    }
    Ok(result)
}
