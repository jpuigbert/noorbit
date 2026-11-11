//! Comandes Tauri per als especialistes (agents d'IA creats per l'usuari)
//! i per al treball múltiple: diversos experts executant subtasques en
//! paral·lel sobre un mateix objectiu.

use crate::ai::{AiManager, ChatOpts};
use crate::experts::{Expert, ExpertInput, ExpertManager};
use crate::AppState;
use serde::Serialize;
use std::sync::Arc;
use tauri::{command, AppHandle, Emitter, State};

/// Xat intern (aïllat de la conversa de l'usuari) on treballa l'equip.
const TEAM_SESSION: &str = "__team__";

/// Progrés del treball en equip (es mostra al fil de l'agent).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    stage: String, // planing | running | done | cancelled
    expert: Option<String>,
    detail: String,
    /// Mil·lisegons des de l'inici de l'equip (per mostrar el temps).
    elapsed_ms: u64,
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

#[derive(Clone, serde::Deserialize)]
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

/// Protocol de fitxers comú a TOTS els especialistes: si la seua feina genera
/// codi o proves, ha d'emetre «@file: ruta» (NoOrbit l'escriu al projecte).
/// Sense això, un especialista podia respondre amb prosa i la feina no
/// existia al disc. S'afegeix DESPRÉS de les seues instruccions.
const FILE_PROTOCOL: &str = "\n\n— FITXERS REALS (NoOrbit) —\n\
Quan generes o modifiques codi o proves (tests), emet cada fitxer amb una \
línia «@file: ruta/real» seguida del contingut COMPLET (imports i lògica \
real, mai «…» ni TODO). Les proves són fitxers de prova reals, no una \
explicació. No diguis que ho has fet si no has emès cap «@file:».";

/// Executa una tasca concreta amb un especialista (el seu rol, instruccions i
/// model). `session`/`history` fan que la resposta pertanya al xat que l'ha
/// cridat — i que l'especialista conega el fil del treball (o d'un «fork»).
#[command]
pub async fn expert_run(
    app: AppHandle,
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
    let system = format!("{}{}", expert.system_prompt, FILE_PROTOCOL);
    let out = crate::ai::in_session(
        &sid,
        ai.chat_opts(&prompt, Some(&system), &opts),
    )
    .await
    .map_err(|e| e.to_string())?;
    // Aprenentatge local: la resposta de l'especialista també s'aprèn (amb
    // l'origen ben marcat: un especialista no és la IA principal).
    crate::commands::learning::record_turn(
        &state,
        &prompt,
        &out,
        &format!("especialista:{}", expert.name),
        expert.model.as_deref().unwrap_or(""),
    )
    .await;
    crate::commands::learning::maybe_auto_distill(
        app,
        ai.clone(),
        state.learning.clone(),
        state.config.clone(),
    );
    Ok(out)
}

/// La sessió d'un especialista DINS el treball en equip: cada un té el seu
/// propi fil, així el frontend (ai://chunk amb «session») pot mostrar EN
/// DIRECTE el que escriu cada especialista, igual que amb el xat principal.
fn team_sid(expert_id: &str) -> String {
    format!("{}:{}", TEAM_SESSION, expert_id)
}

/// Atura el treball en equip en curs (botó «Atura» de la interfície).
#[command]
pub async fn experts_team_stop(state: State<'_, AppState>) -> Result<(), String> {
    state.team.cancel();
    // A més de la bandera pròpia, atura la generació del xat intern de l'equip:
    // la del planificador/fusió i les pròpies de cada especialista.
    state.ai_manager.request_stop(Some(TEAM_SESSION));
    for e in state.experts.list() {
        state.ai_manager.request_stop(Some(team_sid(&e.id).as_str()));
    }
    Ok(())
}

/// Treball múltiple en equip: planifica, executa les subtasques i consolida
/// una resposta final.
///
/// Pensat per a equips amb poca RAM: si el proveïdor és local (`ollama`), les
/// subtasques s'executen D'UNA EN UNA (no totes alhora); amb proveïdors
/// remots se'n fan fins a 3 en paral·lel. Així s'evita carregar diversos
/// models a la memòria al mateix temps i que l'equip es quede penjat.
#[command]
pub async fn experts_team_run(
    app: AppHandle,
    state: State<'_, AppState>,
    goal: String,
    expert_ids: Option<Vec<String>>,
    provider: Option<String>,
    session: Option<String>,
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

    // Només un equip alhora: dos equips competirien per la mateixa RAM i
    // l'usuari no sabria quin està responent.
    if !state.team.begin() {
        return Err("Ja hi ha un equip treballant. Atura'l abans de llançar-ne un altre.".into());
    }

    // Proveïdor: el del xat que ha llançat l'equip o, si no n'hi ha, el global.
    let provider = match provider.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()) {
        Some(p) => p,
        None => {
            let sid = session.unwrap_or_else(|| crate::ai::DEFAULT_SESSION.to_string());
            ai.session_labels(&sid).await.0
        }
    };
    let local = provider.eq_ignore_ascii_case("ollama");

    let out = run_team(&app, &ai, team, goal, provider, local, &state.team).await;
    state.team.finish();
    out
}

/// Cos del treball en equip (separat perquè el control sempre es tanca, tant
/// si acaba bé com si falla).
async fn run_team(
    app: &AppHandle,
    ai: &Arc<AiManager>,
    team: Vec<Expert>,
    goal: String,
    provider: String,
    local: bool,
    ctl: &crate::experts::TeamControl,
) -> Result<TeamReport, String> {
    let started = std::time::Instant::now();
    let emit = |stage: &str, expert: Option<String>, detail: String| {
        let _ = app.emit(
            "experts://progress",
            Progress {
                stage: stage.into(),
                expert,
                detail,
                elapsed_ms: started.elapsed().as_millis() as u64,
            },
        );
    };

    // 1) Planificació. Amb un sol especialista no cal: estalvia tokens i RAM.
    let plan: Vec<PlanItem> = if team.len() == 1 {
        vec![PlanItem {
            expert_id: team[0].id.clone(),
            task: goal.clone(),
        }]
    } else {
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
        let opts = ChatOpts {
            provider: Some(provider.clone()),
            ..Default::default()
        };
        match crate::ai::in_session("team:plan", ai.chat_opts(&plan_prompt, Some(PLANNER_SYSTEM), &opts))
        .await
        {
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
        })
    };

    // 2) Execució. Local: d'un en un (per torns, igual de prou RAM); remot:
    // fins a 3 alhora. Cada subtaska corre dins la SEUA sessió perquè el
    // xat en mostre el text en viu amb el stream per sessió.
    let batch = if local { 1 } else { 3 };
    let total_subs = plan.len();
    let mut subtasks: Vec<SubtaskResult> = Vec::new();
    let mut cancelled = false;

    for (chunk_idx, chunk) in plan.chunks(batch).enumerate() {
        if ctl.is_cancelled() {
            cancelled = true;
            break;
        }
        let futures = chunk.iter().map(|item| {
            let ai = ai.clone();
            let app2 = app.clone();
            let provider = provider.clone();
            let item = item.clone();
            let expert = team.iter().find(|e| e.id == item.expert_id).cloned();
            let turn_label = if batch == 1 {
                format!("Torn {}/{} · ", chunk_idx + 1, total_subs)
            } else {
                String::new()
            };
            async move {
                let Some(expert) = expert else {
                    return SubtaskResult {
                        expert_id: item.expert_id,
                        expert_name: "(desconegut)".into(),
                        task: item.task,
                        result: "Especialista no trobat".into(),
                        ok: false,
                    };
                };
                let _ = app2.emit(
                    "experts://progress",
                    Progress {
                        stage: "running".into(),
                        expert: Some(expert.name.clone()),
                        detail: format!("{}Treballant: {}", turn_label, truncate(&item.task)),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    },
                );
                let opts = ChatOpts {
                    provider: Some(provider),
                    model: expert.model.clone(),
                    ..Default::default()
                };
                // Temps màxim per especialista: si un model es penja, l'equip
                // continua amb els altres en lloc de quedar-se bloquejat.
                let limit = if local {
                    std::time::Duration::from_secs(600)
                } else {
                    std::time::Duration::from_secs(300)
                };
                // Sessió PRÒPIA de l'especialista: el seu fil/stream és visible
                // des del panell d'especialistes i des del xat («Obre al xat»).
                let sid = team_sid(&expert.id);
                let system = format!("{}{}", expert.system_prompt, FILE_PROTOCOL);
                let outcome = tokio::time::timeout(
                    limit,
                    crate::ai::in_session(
                        &sid,
                        ai.chat_opts(&item.task, Some(&system), &opts),
                    ),
                )
                .await;
                let (result, ok) = match outcome {
                    Ok(Ok(text)) => (text, true),
                    Ok(Err(e)) => (e.to_string(), false),
                    Err(_) => (
                        "Temps exhaurit sense resposta del model. Prova un model més lleuger o un proveïdor al núvol."
                            .to_string(),
                        false,
                    ),
                };
                let _ = app2.emit(
                    "experts://progress",
                    Progress {
                        stage: "running".into(),
                        expert: Some(expert.name.clone()),
                        detail: if ok {
                            "Ha acabat".into()
                        } else {
                            "Ha fallat".into()
                        },
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    },
                );
                // RAM: en proves locals, ALLIBERA el model just acabat d'usar
                // abans del torn següent. Així, amb prou RAM el següent
                // especialista del torn no troba la memòria plena del model
                // de l'anterior (que pot ser un de més gran o diferent).
                if local {
                    let target: Option<String> = match &expert.model {
                        Some(m) => Some(m.clone()),
                        None => ai.selected_model().await,
                    };
                    if let Some(m) = target {
                        let _ = ai.unload_models(Some(&m)).await;
                    }
                }
                SubtaskResult {
                    expert_id: expert.id,
                    expert_name: expert.name,
                    task: item.task,
                    result,
                    ok,
                }
            }
        });
        let mut got = futures::future::join_all(futures).await;
        subtasks.append(&mut got);
    }

    if cancelled || ctl.is_cancelled() {
        emit("cancelled", None, "Aturat per l'usuari.".into());
        return Ok(TeamReport {
            subtasks,
            answer: "Treball en equip aturat per l'usuari. Els resultats que ja havien acabat es mostren a dalt.".into(),
        });
    }

    // 3) Consolidar només els resultats bons (estalvia tokens i evita que els
    // errors es colen a la resposta final).
    let good: Vec<&SubtaskResult> = subtasks.iter().filter(|s| s.ok).collect();
    let answer = if good.is_empty() {
        "Cap especialista ha pogut completar la seua part. Revisa el proveïdor i el model actiu (o tria un model més lleuger) i torna-ho a provar.".to_string()
    } else if subtasks.len() == 1 {
        good[0].result.clone()
    } else {
        emit("running", None, "Integrant els resultats de l'equip…".into());
        let digest: String = good
            .iter()
            .map(|s| format!("## {} — {}\n{}", s.expert_name, truncate(&s.task), s.result))
            .collect::<Vec<_>>()
            .join("\n\n");
        let merge_prompt = format!(
            "Objectiu original: {}\n\nResultats de l'equip:\n\n{}",
            goal, digest
        );
        let opts = ChatOpts {
            provider: Some(provider),
            ..Default::default()
        };
        match crate::ai::in_session("team:final", ai.chat_opts(&merge_prompt, Some(MERGER_SYSTEM), &opts)).await {
            Ok(text) => text,
            Err(_) => digest, // si no pot consolidar, mostra els resultats en brut
        }
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
