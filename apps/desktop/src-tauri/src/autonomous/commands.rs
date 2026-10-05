//! Comandes per a les feines autònomes (especialistes forjats en segon pla).
//!
//! El cicle de cada feina és acotat: màxim `MAX_STEPS` passos, pausa llarga
//! en error i repòs automàtic («dorment») si s'encadenen els errors. L'estat
//! és persistent, de manera que la feina sobreviu reinicis i arrenca sola
//! quan l'aplicació es obre.

use crate::agent::AgentOrchestrator;
use crate::ai::AiManager;
use crate::autonomous::{
    Job, JournalNote, AutonomousStore, ERROR_BACKOFF_MS, MAX_CONSECUTIVE_ERRORS, MAX_STEPS,
    STEP_INTERVAL_MS,
};
use crate::AppState;
use tauri::{command, AppHandle, Emitter, Manager, State};

/// Notifica a la UI l'estat complet de les feines (per al panell xat).
pub fn emit_status(app: &AppHandle, store: &AutonomousStore) {
    let _ = app.emit("autonomous://status", store.list());
}

/// Un pas de la feina: pensar amb els recursos (web + altres IAs + local),
/// anotar el descobriment al diari i decidir si l'objectiu ja és resolt.
async fn run_job(
    app: AppHandle,
    id: String,
    ai: std::sync::Arc<AiManager>,
    orch: std::sync::Arc<AgentOrchestrator>,
    store: std::sync::Arc<AutonomousStore>,
) {
    loop {
        let Some(job) = store.get(&id) else { return };
        if !job.active || job.solved || job.steps >= MAX_STEPS {
            emit_status(&app, &store);
            return;
        }

        // 1) Investigar el focus actual amb tots els recursos permesos.
        let research = orch.deep_think(&ai, &app, &job.focus).await;
        let step = job.steps + 1;
        let (ok, text) = match research {
            Ok(t) => (true, t),
            Err(e) => (false, e.to_string()),
        };

        // 2) Demanar al model si l'objectiu ja està resolt (avaluació barata).
        let verdict = if ok {
            let sys = "Avalua si l'objectiu queda RESOLT amb el treball adjunt. \
                       Respon NOMÉS un objecte JSON {\"solved\": true o false, \
                       \"next\": \"passa concret que cal fer ara, o buit si ja és resolt\"}.";
            let user = format!(
                "Objectiu: {}\n\nTreball d'aquest pas:\n{}",
                job.objective,
                text.chars().take(3000).collect::<String>()
            );
            ai.chat(&user, Some(sys)).await.unwrap_or_default()
        } else {
            String::new()
        };
        let json_bit = verdict
            .find('{')
            .and_then(|a| verdict.rfind('}').map(|b| &verdict[a..=b]));
        let parsed: serde_json::Value = json_bit
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::Value::Null);
        let solved = parsed.get("solved").and_then(|v| v.as_bool()).unwrap_or(false);
        let next = parsed
            .get("next")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        // 3) Escriure el pas al diari (persistit: així «sobreviu»).
        let note = JournalNote {
            step,
            at: chrono::Utc::now().timestamp_millis(),
            ok,
            text: text.chars().take(4000).collect(),
        };
        let focus_now = job.focus.clone();
        let updated = store.mutate(&id, |j| {
            j.steps = step;
            if ok {
                j.consecutive_errors = 0;
            } else {
                j.errors += 1;
                j.consecutive_errors += 1;
            }
            j.solved = solved;
            j.journal.push(note);
            // La fila de treball incorpora l'últim aprenentatge.
            let learned: String = if ok {
                text.chars().take(1200).collect()
            } else {
                format!("L'intent va fallar: {}", text)
            };
            let tail = focus_now.chars().rev().take(800).collect::<String>();
            let tail: String = tail.chars().rev().collect();
            j.focus = if solved {
                format!("Objectiu aixo: {}\nRESOLT. Redacta la resposta final.", j.objective)
            } else if !next.is_empty() {
                format!("Objectiu: {}\nAprenentatges: …{}\nAra mateix: {}", j.objective, tail, next)
            } else {
                format!("Objectiu: {}\nAprenentatge recent: {}\nContinua ampliant la solució.", j.objective, learned)
            };
            if j.consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                // Massa errors seguits: es torna a adormir per no cremar res.
                j.active = false;
            }
        });
        emit_status(&app, &store);

        let Some(job) = updated else { return };
        if job.solved || job.steps >= MAX_STEPS || !job.active {
            return;
        }

        // 4) Pausa llarga: no superar mai la capacitat de la màquina.
        let wait = if !ok { ERROR_BACKOFF_MS } else { STEP_INTERVAL_MS };
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait);
        while std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(2_000)).await;
            // Si l'usuari l'ha pausada o eliminada, marxem de seguida.
            let Some(j) = store.get(&id) else { return };
            if !j.active || j.solved {
                emit_status(&app, &store);
                return;
            }
        }
    }
}

/// Llista de feines autònomes (estat viu per a la UI).
#[command]
pub async fn autonomous_list(state: State<'_, AppState>) -> Result<Vec<Job>, String> {
    Ok(state.autonomous.list())
}

/// Pausa o reactua una feina. En activar-la, engega el fil en segon pla.
#[command]
pub async fn autonomous_set_active(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
    active: bool,
) -> Result<(), String> {
    let store = state.autonomous.clone();
    let ai = state.ai_manager.clone();
    let orch = state.agent_orchestrator.clone();
    let job = store
        .set_active(&id, active)
        .ok_or_else(|| "La feina no existeix".to_string())?;
    emit_status(&app, &store);
    if active && !job.solved && job.steps < MAX_STEPS {
        tauri::async_runtime::spawn(run_job(
            app.clone(),
            job.id.clone(),
            ai,
            orch,
            store.clone(),
        ));
    }
    Ok(())
}

/// Elimina una feina (i el seu diari) de manera definitiva.
#[command]
pub async fn autonomous_remove(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
) -> Result<(), String> {
    state.autonomous.remove(&id);
    emit_status(&app, &state.autonomous);
    Ok(())
}

/// Engega el fil en segon pla d'una feina acabada de forjar (cridat des de
/// `agent_forge`). No fa res si la feina ja naix resolta, esgotada o pausada.
pub fn spawn_job(app: &AppHandle, state: &AppState, job: Job) {
    let store = state.autonomous.clone();
    let ai = state.ai_manager.clone();
    let orch = state.agent_orchestrator.clone();
    emit_status(app, &store);
    if job.active && !job.solved && job.steps < MAX_STEPS {
        let app = app.clone();
        let id = job.id.clone();
        tauri::async_runtime::spawn(run_job(app, id, ai, orch, store));
    }
}

/// Reempren les feines actives pendents: anomenat des de la posada en marxa
/// de l'aplicació (és el punt on «arrenquen soles» en obrir NoOrbit).
pub fn resume_all(app: &AppHandle) {
    let state = app.state::<AppState>();
    let store = state.autonomous.clone();
    let ai = state.ai_manager.clone();
    let orch = state.agent_orchestrator.clone();
    for job in store.resumable() {
        let (app, id, ai, orch, store) = (app.clone(), job.id.clone(), ai.clone(), orch.clone(), store.clone());
        tauri::async_runtime::spawn(async move {
            // Deixem arrencar la IA local (Ollama) abans del primer pas.
            tokio::time::sleep(std::time::Duration::from_secs(45)).await;
            run_job(app, id, ai, orch, store).await;
        });
    }
}
