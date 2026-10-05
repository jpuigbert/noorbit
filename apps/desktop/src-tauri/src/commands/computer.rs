//! Comandes Tauri per al control de l'ordinador (amb permisos).

use crate::computer::{ExecReport, HistoryEntry, Permissions};
use crate::AppState;
use tauri::{command, AppHandle, Emitter, State};

#[command]
pub async fn computer_permissions(state: State<'_, AppState>) -> Result<Permissions, String> {
    Ok(state.computer.permissions())
}

#[command]
pub async fn computer_set_permissions(
    state: State<'_, AppState>,
    enabled: Option<bool>,
    confirm_each: Option<bool>,
) -> Result<Permissions, String> {
    state.computer.set_permissions(enabled, confirm_each);
    Ok(state.computer.permissions())
}

#[command]
pub async fn computer_remove_pattern(
    state: State<'_, AppState>,
    pattern: String,
) -> Result<(), String> {
    state.computer.remove_pattern(&pattern);
    Ok(())
}

#[command]
pub async fn computer_history(state: State<'_, AppState>) -> Result<Vec<HistoryEntry>, String> {
    Ok(state.computer.history())
}

#[command]
pub async fn computer_clear_history(state: State<'_, AppState>) -> Result<(), String> {
    state.computer.clear_history();
    Ok(())
}

/// Executa una comanda directa (l'usuari des del panell). Passa pel cascade
/// de permisos: si cal, obre el diàleg de confirmació.
#[command]
pub async fn computer_execute(
    app: AppHandle,
    state: State<'_, AppState>,
    command: String,
) -> Result<ExecReport, String> {
    let ctrl = state.computer.clone();
    ctrl.execute(&app, &command)
        .await
        .map_err(|e| e.to_string())
}

/// Resposta de la UI al diàleg "computer://confirm".
#[command]
pub async fn computer_confirm(
    state: State<'_, AppState>,
    id: String,
    approved: bool,
    remember: bool,
) -> Result<(), String> {
    state
        .computer
        .respond(&id, approved, remember)
        .map_err(|e| e.to_string())
}

/// Un pas del bucle de l'agent: transforma una ordren en llenguatge natural
/// en (a) una comanda a executar o (b) una resposta final. Torna JSON.
/// La IA NO executa res directament aquí: només proposa la comanda; qui
/// l'executa és `computer_agent_run` cridant a `computer.execute`.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentStep {
    /// "run" | "done"
    action: String,
    #[serde(default)]
    command: String,
    #[serde(default)]
    answer: String,
    #[serde(default)]
    reason: String,
}

/// Pla: convertir llenguatge natural (català) en UNA comanda de shell real.
/// Un model local petit (qwen 1.5b/3b) NO sap per si sol que «obre Blender»
/// significa `open -a Blender`. Per això donem un diccionari EXPLÍCIT de
/// verbs catalans → comandes macOS i exemples concrets (few-shot): els models
/// dèbils obeyen molt millor patrons copiables que instruccions abstractes.
const PLANNER_SYSTEM: &str = r#"Ets l'agent de control d'ordinador de NoOrbit (macOS). Converteix l'objectiu de l'usuari en UNA sola comanda de shell REAL i executable cada torn, o bé respon quan la tasca sigui feta.

Respon NOMÉS amb JSON estricte, sense text al voltant, amb aquest esquema:
{"action":"run","command":"...","reason":"..."} per executar una comanda, o {"action":"done","answer":"..."} quan hagueu acabat.

DICCONARI (tradueix el que demana l'usuari a la comanda de la dreta):
- «obre / arrenca / engega / obre'm <App>»            -> open -a "<App>"    (p. ex. «obre blender» -> open -a "Blender")
- «obre <url>»                                        -> open "<url>"
- «obre <arxiu>»                                      -> open "<arxiu>"
- «llista / ensenya / què hi ha a <carpeta>»          -> ls -la "<carpeta>"
- «mira / ensenya el contingut de <arxiu>»            -> cat "<arxiu>"
- «crear / fes una carpeta <nom>»                     -> mkdir -p "<nom>"
- «copia <a> a <b>»                                   -> cp -R "<a>" "<b>"
- «mou / raciona <a> cap a <b>»                       -> mv "<a>" "<b>"
- «quins programes / llista aplicacions»              -> ls /Applications
- «cerca <text> a <carpeta>»                          -> grep -r "<text>" "<carpeta>"
- «tanca <App>»                                       -> osascript -e 'quit app "<App>"'

EXEMPLES:
Usuari: "obre blender"
{"action":"run","command":"open -a \"Blender\"","reason":"Llençar Blender"}
Usuari: "obre el navegador i després llista l'escriptori"
{"action":"run","command":"open -a \"Safari\"","reason":"Primer pas: obrir navegador"}

REGLES:
- Escriu NUNCA el verb natural («obre») com a comanda: és un error «command not found». Tradueix-lo sempre a una comanda Unix real.
- Si no entens quina comanda cal o no existeix equivalent segur, respon {"action":"done","answer":"..."} explicant què faries.
- Prefereix comandes no destructives (ls, cat, open, mkdir, cp, grep). No usis sudo ni esborris dades. Les comandes van a /bin/zsh."#;

#[command]
pub async fn computer_agent_run(
    app: AppHandle,
    state: State<'_, AppState>,
    goal: String,
) -> Result<String, String> {
    let ai = state.ai_manager.clone();
    let ctrl = state.computer.clone();

    let mut transcript = String::new();
    let max_steps = 6usize;

    for step in 0..max_steps {
        let prompt = format!(
            "Objectiu: {}\n\nFins ara:\n{}\n\nQuin és el següent pas? (torn {}/{})",
            goal,
            if transcript.is_empty() { "(cap encara)" } else { &transcript },
            step + 1,
            max_steps
        );

        let raw = ai
            .chat(&prompt, Some(PLANNER_SYSTEM))
            .await
            .map_err(|e| e.to_string())?;

        let parsed = parse_step(&raw);
        let parsed = match parsed {
            Some(p) => p,
            None => {
                transcript.push_str(&format!("assistant (no-JSON): {}\n", raw.trim()));
                continue;
            }
        };

        if parsed.action == "done" {
            let _ = app.emit(
                "computer://agent",
                serde_json::json!({ "action": "done", "answer": parsed.answer }),
            );
            return Ok(if parsed.answer.is_empty() {
                "Tasca finalitzada".into()
            } else {
                parsed.answer
            });
        }

        // action == "run": executa via el cascade de permisos (demostra UI).
        let _ = app.emit(
            "computer://agent",
            serde_json::json!({
                "action": "run",
                "command": parsed.command,
                "reason": parsed.reason,
            }),
        );

        match ctrl.execute(&app, &parsed.command).await {
            Ok(rep) => {
                let out = truncate_for_llm(&rep.stdout, &rep.stderr);
                transcript.push_str(&format!(
                    "$ {}\n(exit {}) {}\n",
                    parsed.command, rep.exit_code, out
                ));
            }
            Err(e) => {
                transcript.push_str(&format!("$ {}  [BLOQUEJAT/ERROR: {}]\n", parsed.command, e));
            }
        }
    }

    let _ = app.emit(
        "computer://agent",
        serde_json::json!({ "action": "done", "answer": "He arribat al límit de passos." }),
    );
    Ok("He arribat al límit de passos sense completar la tasca.".into())
}

fn parse_step(raw: &str) -> Option<AgentStep> {
    let trimmed = raw.trim();
    // Retalla blocs ```json … ``` si n'hi ha.
    let cleaned = strip_fences(trimmed);
    // Busca el primer '{' i l'últim '}'.
    let start = cleaned.find('{')?;
    let end = cleaned.rfind('}')?;
    if end <= start {
        return None;
    }
    let json = &cleaned[start..=end];
    serde_json::from_str::<AgentStep>(json).ok()
}

fn strip_fences(s: &str) -> String {
    let mut out = s.to_string();
    if let Some(pos) = out.find("```") {
        let rest = &out[pos + 3..];
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        if let Some(end) = rest.find("```") {
            out = rest[..end].to_string();
        } else {
            out = rest.to_string();
        }
    }
    out
}

fn truncate_for_llm(stdout: &str, stderr: &str) -> String {
    let mut s = String::new();
    if !stdout.trim().is_empty() {
        s.push_str(stdout.trim());
    }
    if !stderr.trim().is_empty() {
        if !s.is_empty() {
            s.push('\n');
        }
        s.push_str(stderr.trim());
    }
    let mut chars: String = s.chars().take(1500).collect();
    if s.chars().count() > 1500 {
        chars.push_str("…[tallat]");
    }
    chars
}
