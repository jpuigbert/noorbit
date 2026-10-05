//! Comandes Tauri per als especialistes (agents d'IA creats per l'usuari)
//! i per al treball múltiple: diversos experts executant subtasques en
//! paral·lel sobre un mateix objectiu.

use crate::ai::AiManager;
use crate::experts::{Expert, ExpertInput, ExpertManager};
use crate::AppState;
use serde::Serialize;
use std::sync::Arc;
use tauri::{command, AppHandle, Emitter, State};

/// Progrés del treball en equip (es mostra al fil de l'agent).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    stage: String, // planing | running | done
    expert: Option<String>,
    detail: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtaskResult {
    pub expert_id: String,
    pub expert_name: String,
    pub task: String,
    pub result: String,
    pub ok: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamReport {
    pub subtasks: Vec<SubtaskResult>,
    pub answer: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanItem {
    expert_id: String,
    task: String,
}

const PLANNER_SYSTEM: &str = "Ets el planificador d'un equip d'agents d'IA. \
Donat un objectiu i la llista d'especialistes disponibles, reparteix el treball: \
respon NOMÉS un array JSON de l'estil \
[{\"expertId\":\"...\",\"task\":\"subtasca concreta per a aquest especialista\"}]. \
Usa cada especialista necessari com a molt una vegada i escriu les tasques en català.";

const MERGER_SYSTEM: &str = "Redacta en català una resposta final i coherent a partir \
 dels resultats de l'equip. No inventis res que no sigui als resultats; integra'ls \
 de forma ordenada i assenyala què aporta cada especialista.";

#[command]
pub async fn experts_list(state: State<'_, AppState>) -> Result<Vec<Expert>, String> {
    Ok(state.experts.list())
}

#[command]
pub async fn expert_create(
    state: State<'_, AppState>,
    input: ExpertInput,
) -> Result<Expert, String> {
    state.experts.add(input).map_err(|e| e.to_string())
}

#[command]
pub async fn expert_update(
    state: State<'_, AppState>,
    id: String,
    input: ExpertInput,
) -> Result<Expert, String> {
    state.experts.update(&id, input).map_err(|e| e.to_string())
}

#[command]
pub async fn expert_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.experts.remove(&id).map_err(|e| e.to_string())
}

/// Executa una tasca concreta amb un especialista (el seu rol, instruccions i
/// model). `session`/`history` fan que la resposta pertanya al xat que l'ha
/// cridat — i que l'especialista conega el fil del treball (o d'un «fork»).
#[command]
pub async fn expert_run(
    state: State<'_, AppState>,
    expert_id: String,
    prompt: String,
    session: Option<String>,
    history: Option<Vec<crate::ai::HistoryTurn>>,
) -> Result<String, String> {
    let ai: Arc<AiManager> = state.ai_manager.clone();
    let expert = state
        .experts
        .get(&expert_id)
        .ok_or_else(|| "L'especialista no existeix".to_string())?;
    let opts = crate::ai::ChatOpts {
        provider: None,
        model: expert.model.clone(),
        history: history.unwrap_or_default(),
        images: Vec::new(),
    };
    let sid = session.unwrap_or_else(|| crate::ai::DEFAULT_SESSION.to_string());
    crate::ai::in_session(
        &sid,
        ai.chat_opts(&prompt, Some(&expert.system_prompt), &opts),
    )
    .await
    .map_err(|e| e.to_string())
}

/// Treball múltiple en equip: planifica, executa les subtasques en paral·lel i
/// consolida una resposta final.
#[command]
pub async fn experts_team_run(
    app: AppHandle,
    state: State<'_, AppState>,
    goal: String,
    expert_ids: Option<Vec<String>>,
) -> Result<TeamReport, String> {
    let ai: Arc<AiManager> = state.ai_manager.clone();
    let mgr: Arc<ExpertManager> = state.experts.clone();

    let all = mgr.list();
    if all.is_empty() {
        return Err("Primer cal crear algun especialista.".into());
    }
    let team: Vec<Expert> = match &expert_ids {
        Some(ids) if !ids.is_empty() => all.into_iter().filter(|e| ids.contains(&e.id)).collect(),
        _ => all,
    };
    if team.is_empty() {
        return Err("Cap especialista seleccionat.".into());
    }

    let emit = |stage: &str, expert: Option<String>, detail: String| {
        let _ = app.emit(
            "experts://progress",
            Progress {
                stage: stage.into(),
                expert,
                detail,
            },
        );
    };

    // 1) Planificació: el model reparteix l'objectiu entre els especialistes.
    emit("planing", None, "Repartint l'objectiu entre l'equip…".into());
    let roster: String = team
        .iter()
        .map(|e| format!("- id={} · {} ({})", e.id, e.name, e.role))
        .collect::<Vec<_>>()
        .join("\n");
    let plan_prompt = format!(
        "Especialistes disponibles:\n{}\n\nObjectiu de l'usuari: {}\n\nReparteix el treball en subtasques.",
        roster, goal
    );
    let plan = match ai.chat_with(&plan_prompt, Some(PLANNER_SYSTEM), None).await {
        Ok(text) => parse_plan(&text, &team),
        Err(_) => None,
    }
    // Si la planificació falla, cada especialista rep l'objectiu sencer.
    .unwrap_or_else(|| {
        team.iter()
            .map(|e| PlanItem {
                expert_id: e.id.clone(),
                task: goal.clone(),
            })
            .collect()
    });

    // 2) Execució en paral·lel de totes les subtasques.
    let futures = plan.into_iter().map(|item| {
        let ai = ai.clone();
        let expert = team.iter().find(|e| e.id == item.expert_id).cloned();
        let emit_clone = app.clone();
        async move {
            let expert = match expert {
                Some(e) => e,
                None => {
                    return SubtaskResult {
                        expert_id: item.expert_id,
                        expert_name: "(desconegut)".into(),
                        task: item.task,
                        result: "Especialista no trobat".into(),
                        ok: false,
                    }
                }
            };
            let _ = emit_clone.emit(
                "experts://progress",
                Progress {
                    stage: "running".into(),
                    expert: Some(expert.name.clone()),
                    detail: format!("Treballant: {}", truncate(&item.task)),
                },
            );
            let outcome = ai
                .chat_with(&item.task, Some(&expert.system_prompt), expert.model.as_deref())
                .await;
            let (result, ok) = match outcome {
                Ok(text) => (text, true),
                Err(e) => (e.to_string(), false),
            };
            let _ = emit_clone.emit(
                "experts://progress",
                Progress {
                    stage: "running".into(),
                    expert: Some(expert.name.clone()),
                    detail: if ok { "Ha acabat".into() } else { "Ha fallat".into() },
                },
            );
            SubtaskResult {
                expert_id: expert.id,
                expert_name: expert.name,
                task: item.task,
                result,
                ok,
            }
        }
    });
    let subtasks = futures::future::join_all(futures).await;

    // 3) Consolidar la resposta final.
    emit("running", None, "Integrant els resultats de l'equip…".into());
    let digest: String = subtasks
        .iter()
        .map(|s| format!("## {} — {}\n{}", s.expert_name, truncate(&s.task), s.result))
        .collect::<Vec<_>>()
        .join("\n\n");
    let merge_prompt = format!(
        "Objectiu original: {}\n\nResultats de l'equip:\n\n{}",
        goal, digest
    );
    let answer = match ai.chat_with(&merge_prompt, Some(MERGER_SYSTEM), None).await {
        Ok(text) => text,
        Err(_) => digest, // si no pot consolidar, mostra els resultats en brut
    };
    emit("done", None, "Treball en equip acabat".into());

    Ok(TeamReport { subtasks, answer })
}

/// Converteix la sortida del planificador en subtasques vàlides; filtra ids
/// inexistents i descarta soroll no-JSON.
fn parse_plan(text: &str, team: &[Expert]) -> Option<Vec<PlanItem>> {
    let cleaned = strip_fences(text);
    let start = cleaned.find('[')?;
    let end = cleaned.rfind(']')?;
    let items: Vec<PlanItem> = serde_json::from_str(&cleaned[start..=end]).ok()?;
    let valid: Vec<PlanItem> = items
        .into_iter()
        .filter(|i| team.iter().any(|e| e.id == i.expert_id) && !i.task.trim().is_empty())
        .collect();
    if valid.is_empty() {
        None
    } else {
        Some(valid)
    }
}

fn strip_fences(s: &str) -> String {
    s.replace("```json", "").replace("```", "").trim().to_string()
}

fn truncate(s: &str) -> String {
    let t = s.trim();
    t.chars().take(120).collect::<String>()
}
