//! Agent Unreal d'UN SOL CLIC: prepara l'entorn (Ollama, plugins MCP,
//! auto-arrencada del servidor) i executa tasques contra l'editor via el
//! protocol MCP amb un bucle d'eines (tool calling) del model local.
//!
//! La selecció d'eines es fa amb «capability search»: en lloc d'exposar el
//! catàleg sencer al model (que el fa encallar-se), escollim un grapat
//! d'eines rellevants per a la tasca a partir de les paraules del prompt.

use super::mcp::{self, McpTool};
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Emitter};

pub const MCP_PORT: u16 = mcp::MCP_DEFAULT_PORT;
/// Eines exposades al model per ronda (catàlegs grans fan que Qwen es perdi).
const MAX_TOOLS: usize = 5;
/// Rondes màximes del bucle d'eines abans de rendir-se.
const MAX_TURNS: usize = 8;
/// Text d'eina retallat per no inflar el context del model.
const MAX_TOOL_TEXT: usize = 6000;

#[derive(Debug, Clone, Serialize)]
pub struct SetupStep {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetupReport {
    pub steps: Vec<SetupStep>,
    pub ok: bool,
    pub mcp_connected: bool,
    pub tools: usize,
    pub message: String,
}

fn emit_step(app: &AppHandle, steps: &mut Vec<SetupStep>, name: &str, ok: bool, message: String) {
    let step = SetupStep { name: name.into(), ok, message: message.clone() };
    let _ = app.emit("unreal://agent-setup", &step);
    steps.push(step);
}

/// Esdeveniment del bucle d'eines que consuma la UI (fase 3 del flux:
/// xat lliure amb l'agent). `tool` i `turn` són opcionals: permeten pintar
/// l'eina que s'està executant sense haver de llegir el text.
fn emit_task(app: &AppHandle, phase: &str, text: &str) {
    emit_task_full(app, phase, text, None, None);
}

fn emit_task_full(
    app: &AppHandle,
    phase: &str,
    text: &str,
    tool: Option<&str>,
    turn: Option<usize>,
) {
    let mut payload = serde_json::json!({ "phase": phase, "text": text });
    if let Some(t) = tool {
        payload["tool"] = serde_json::json!(t);
    }
    if let Some(n) = turn {
        payload["turn"] = serde_json::json!(n);
    }
    let _ = app.emit("unreal://agent-task", payload);
}

/// ORQUESTRADOR «ACTIVAR AGENT UNREAL»: tot el que abans era un manual
/// (Ollama, plugins, preferències) ho fa aquesta funció en segon pla,
/// pas a pas i de forma IDEMPOTENT (es pot prémer tantes vegades com es
/// vulgui: allò que ja està bé es verifica i es continua).
pub async fn run_agent_setup(
    app: &AppHandle,
    project: &Path,
    launch: bool,
) -> Result<SetupReport> {
    let mut steps: Vec<SetupStep> = Vec::new();

    // 1) OLLAMA: en marxa? si no, arrenca'l (o avisa que cal instal·lar-lo).
    if crate::ai::ollama_running() {
        emit_step(app, &mut steps, "ollama", true, "Ollama escolta a 127.0.0.1:11434.".into());
    } else if crate::ai::ollama_installed() {
        match tokio::task::spawn_blocking(crate::ai::start_ollama_server).await {
            Ok(Ok(())) => emit_step(
                app, &mut steps, "ollama", true,
                "Ollama arrencat en segon pla (port 11434).".into(),
            ),
            Ok(Err(e)) => emit_step(
                app, &mut steps, "ollama", false,
                format!("No s'ha pogut arrencar Ollama: {}", e),
            ),
            Err(e) => emit_step(
                app, &mut steps, "ollama", false,
                format!("No s'ha pogut arrencar Ollama: {}", e),
            ),
        }
    } else {
        emit_step(
            app, &mut steps, "ollama", false,
            "Ollama no està instal·lat: instal·la'l amb un sol clic des de Configuració ▸ IA.".into(),
        );
    }

    // 2) PLUGINS MCP al .uproject (ModelContextProtocol + AllToolsets).
    match super::launcher::ensure_agent_plugins(project) {
        Ok(true) => emit_step(
            app, &mut steps, "plugins", true,
            "Plugins «ModelContextProtocol» + «AllToolsets» ACTIVATS al .uproject. \
             Si l'editor ja estava obert, reinicia'l perquè es carreguin.".into(),
        ),
        Ok(false) => emit_step(
            app, &mut steps, "plugins", true,
            "Plugins «ModelContextProtocol» + «AllToolsets» ja estaven actius.".into(),
        ),
        Err(e) => emit_step(app, &mut steps, "plugins", false, e.to_string()),
    }

    // 3) AUTO-ARRENCADA del servidor MCP per a AQUEST projecte (ini).
    match super::launcher::write_mcp_autostart(project) {
        Ok(ini) => emit_step(
            app, &mut steps, "autostart", true,
            format!("El servidor MCP s'aixecarà SOL en obrir el projecte ({})", ini.display()),
        ),
        Err(e) => emit_step(app, &mut steps, "autostart", false, e.to_string()),
    }

    // 4) SERVIDOR MCP: escolta ara mateix? si no, es pot arrencar l'editor.
    if mcp::McpClient::port_open(MCP_PORT).await {
        emit_step(
            app, &mut steps, "server", true,
            "Servidor MCP escoltant a 127.0.0.1:8000/mcp.".into(),
        );
    } else if launch {
        let installs = super::launcher::detect_engine_installations();
        match installs.first().and_then(|r| super::launcher::editor_binary(r)) {
            Some(editor) => {
                // L'argument força el servidor MCP EN AQUESTA sessió encara
                // que l'auto-arrencada de l'ini encara no s'hagi aplicat.
                let r = super::launcher::launch_editor(
                    &editor,
                    project,
                    &["-ModelContextProtocolStartServer".into()],
                )
                .await;
                match r {
                    Ok(()) => {
                        emit_step(
                            app, &mut steps, "server", true,
                            "Editor d'Unreal arrencat amb el servidor MCP forçat. \
                             Esperant el port 8000 (pot trigar: compilació de shaders)…".into(),
                        );
                        match super::launcher::wait_for_mcp(MCP_PORT, 300).await {
                            Ok(true) => emit_step(
                                app, &mut steps, "server", true,
                                "Servidor MCP responent al port 8000.".into(),
                            ),
                            Ok(false) => emit_step(
                                app, &mut steps, "server", false,
                                "L'editor no ha aixecat el servidor MCP en 5 minuts. \
                                 Comprova que els plugins estiguin actius i reinicia Unreal.".into(),
                            ),
                            Err(e) => emit_step(app, &mut steps, "server", false, e.to_string()),
                        }
                    }
                    Err(e) => emit_step(app, &mut steps, "server", false, e.to_string()),
                }
            }
            None => emit_step(
                app, &mut steps, "server", false,
                "No s'ha trobat cap instal·lació d'Unreal Engine 5 al sistema.".into(),
            ),
        }
    } else {
        emit_step(
            app, &mut steps, "server", false,
            "El servidor MCP encara no està actiu: obre el projecte amb Unreal (o reinicia'l) \
             i el servirà a 127.0.0.1:8000 sol.".into(),
        );
    }

    // 5) SONDA: handshake MCP + catàleg d'eines.
    let mut mcp_connected = false;
    let mut tools = 0;
    if mcp::McpClient::port_open(MCP_PORT).await {
        match mcp::open_session().await {
            Ok(sess) => {
                mcp_connected = true;
                tools = sess.tools.len();
                emit_step(
                    app, &mut steps, "probe", true,
                    format!("Connectat al servidor MCP d'Unreal: {} eines disponibles.", tools),
                );
            }
            Err(e) => emit_step(app, &mut steps, "probe", false, e.to_string()),
        }
    } else {
        emit_step(
            app, &mut steps, "probe", false,
            "Sense servidor MCP no es pot descarregar el catàleg d'eines.".into(),
        );
    }

    let ok = steps.iter().all(|s| s.ok);
    let message = if ok {
        "Entorn preparat: l'agent local ja pot treballar amb Unreal.".to_string()
    } else {
        "Falten passos: mira el registre de l'activació.".to_string()
    };
    Ok(SetupReport { steps, ok, mcp_connected, tools, message })
}

/// Paraules del prompt amb sentit per cercar eines (sense articles ni
/// verbs de farciment catalans/anglesos, i sense duplicats).
fn keywords_of(prompt: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "crea", "una", "que", "els", "les", "amb", "per", "dels", "del", "vull", "posa",
        "afegeix", "the", "and", "for", "fer", "has", "mou", "moure", "més", "tot", "tots",
        "dins", "sobre", "after", "into", "cada", "com", "then", "each", "with", "from",
    ];
    let mut out: Vec<String> = Vec::new();
    for w in prompt.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_') {
        let w = w.trim().to_lowercase();
        if w.chars().count() >= 4 && !STOP.contains(&w.as_str()) && !out.contains(&w) {
            out.push(w);
        }
    }
    out
}

/// CAPABILITY SEARCH: puntua el catàleg d'eines contra les paraules del
/// prompt (el nom pesa més que la descripció) i en retorna les millors
/// (màxim `max`, ordre original en cas d'empat).
pub fn capability_search(tools: &[McpTool], prompt: &str, max: usize) -> Vec<McpTool> {
    let keywords = keywords_of(prompt);
    if keywords.is_empty() || tools.len() <= max {
        return tools.iter().take(max).cloned().collect();
    }
    let mut scored: Vec<(usize, usize)> = tools
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let name = t.name.to_lowercase();
            let desc = t.description.as_deref().unwrap_or("").to_lowercase();
            let mut s = 0usize;
            for kw in &keywords {
                if name.contains(kw.as_str()) {
                    s += 3;
                }
                if desc.contains(kw.as_str()) {
                    s += 2;
                }
            }
            (i, s)
        })
        .collect();
    scored.sort_by_key(|(i, s)| (std::cmp::Reverse(*s), *i));
    scored.into_iter().take(max).map(|(i, _)| tools[i].clone()).collect()
}

fn tool_schema(t: &McpTool) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": t.name,
            "description": t.description.clone().unwrap_or_default(),
            "parameters": t.input_schema.clone().unwrap_or_else(|| {
                serde_json::json!({ "type": "object", "properties": {} })
            }),
        }
    })
}

fn clip(text: &str, n: usize) -> String {
    let t: String = text.trim().chars().take(n).collect();
    if text.trim().chars().count() > n {
        format!("{}…", t)
    } else {
        t
    }
}

/// BUCLE DE L'AGENT: una ordre d'alt nivell («crea una fila de 5 llums»)
/// → el model local tria eines del catàleg seleccionat → NoOrbit les
/// executa contra Unreal via MCP → el model inspecciona el resultat i
/// decideix el següent pas → fins a la resposta final o el límit de rondes.
pub async fn run_agent_task(app: &AppHandle, prompt: &str, model: Option<String>) -> Result<String> {
    let sess = mcp::open_session().await?;
    let selected = capability_search(&sess.tools, prompt, MAX_TOOLS);
    let names = selected
        .iter()
        .map(|t| t.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    emit_task(app, "eines", &format!("{} eines seleccionades per a la tasca: {}", selected.len(), names));

    let model = model.unwrap_or_else(|| "qwen2.5:7b".to_string());
    emit_task(app, "model", &model);
    let system = "Ets l'agent d'Unreal Engine de NoOrbit. Tens un conjunt d'eines MCP per \
                  treballar DINS de l'editor (crear actors, editar blueprints, moure coses…). \
                  Respon en català, breu. Si l'eina falla, llegeix l'error i torna-ho a provar \
                  amb uns altres paràmetres abans de dir que no es pot. Quan la feina estigui \
                  feta, explica en una frase què has fet i marxa.";

    let mut messages: Vec<serde_json::Value> = vec![
        serde_json::json!({ "role": "system", "content": system }),
        serde_json::json!({ "role": "user", "content": prompt }),
    ];

    for turn in 0..MAX_TURNS {
        emit_task_full(app, "ronda", &format!("Ronda {}", turn + 1), None, Some(turn + 1));
        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "tools": selected.iter().map(tool_schema).collect::<Vec<_>>(),
            "stream": false,
            "options": { "temperature": 0.2 },
        });
        let http = reqwest::Client::new();
        let resp = tokio::time::timeout(
            std::time::Duration::from_secs(480),
            http.post("http://127.0.0.1:11434/api/chat").json(&body).send(),
        )
        .await
        .map_err(|_| anyhow!("Ollama ha trigat més de 8 minuts a respondre (model {} en CPU)", model))?
        .map_err(|e| anyhow!("No s'ha pogut parlar amb Ollama: {}", e))?;
        if resp.status().as_u16() == 404 {
            return Err(anyhow!(
                "El model {} no està instal·lat a Ollama. Baixa'l (o tria un altre) a Configuració ▸ IA.",
                model
            ));
        }
        if !resp.status().is_success() {
            return Err(anyhow!("Ollama ha respost {}", resp.status()));
        }
        let v: serde_json::Value = resp.json().await?;
        let msg = v
            .get("message")
            .cloned()
            .ok_or_else(|| anyhow!("Ollama no ha retornat cap missatge"))?;
        let tool_calls = msg
            .get("tool_calls")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");

        // Sense crides d'eines: resposta final (o el model ha acabat).
        if tool_calls.is_empty() {
            if content.trim().is_empty() {
                return Err(anyhow!("El model ha acabat sense resposta ni eines (potser no suporta tool calling: tria un model Qwen/Llama recent)"));
            }
            emit_task(app, "answer", content);
            return Ok(content.to_string());
        }

        messages.push(msg);
        for call in &tool_calls {
            let f = call.get("function").cloned().unwrap_or(serde_json::Value::Null);
            let name = f
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            // El model pot enviar els arguments com a objecte o com a JSON string.
            let mut args = f.get("arguments").cloned().unwrap_or(serde_json::json!({}));
            if let Some(s) = args.as_str() {
                args = serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!({}));
            }
            emit_task_full(
                app,
                "eina",
                &format!("Executant «{}»…", if name.is_empty() { "eina desconeguda" } else { &name }),
                if name.is_empty() { None } else { Some(name.as_str()) },
                Some(turn + 1),
            );
            let result = sess.client.tools_call(&name, args).await;
            match result {
                Ok(r) => {
                    let text = clip(&r.text, MAX_TOOL_TEXT);
                    emit_task_full(
                        app,
                        "eina_ok",
                        &format!("✓ {} — {}", name, clip(&r.text, 160)),
                        if name.is_empty() { None } else { Some(name.as_str()) },
                        Some(turn + 1),
                    );
                    messages.push(serde_json::json!({ "role": "tool", "content": text }));
                }
                Err(e) => {
                    emit_task_full(
                        app,
                        "eina_err",
                        &format!("✗ {} — {}", name, e),
                        if name.is_empty() { None } else { Some(name.as_str()) },
                        Some(turn + 1),
                    );
                    messages.push(serde_json::json!({
                        "role": "tool",
                        "content": format!("ERROR: {}", e),
                    }));
                }
            }
        }
    }

    Err(anyhow!(
        "L'agent no ha acabat la tasca després de {} rondes d'eines. Torna-ho a provar amb una ordre més concreta.",
        MAX_TURNS
    ))
}

#[cfg(test)]
mod proves {
    use super::*;

    fn eina(name: &str, desc: &str) -> McpTool {
        McpTool {
            name: name.into(),
            description: Some(desc.into()),
            input_schema: None,
        }
    }

    #[test]
    fn keywords_descarta_farciment_i_duplicats() {
        let k = keywords_of("crea una fila de 5 llums lights llums amb actors");
        assert!(k.contains(&"llums".to_string()));
        assert!(k.contains(&"actors".to_string()));
        assert!(!k.contains(&"crea".to_string()));
        assert!(!k.contains(&"una".to_string()));
        assert!(!k.contains(&"amb".to_string()));
        // «llums» no pot sortir duplicada
        assert_eq!(k.iter().filter(|w| *w == "llums").count(), 1);
    }

    #[test]
    fn cerca_troba_les_eines_amb_millor_puntuacio() {
        let tools = vec![
            eina("spawn_actor", "Spawn a new actor in the level"),
            eina("spawn_lights", "Spawn a row of lights"),
            eina("create_blueprint", "Create a Blueprint class"),
            eina("build_lighting", "Build lighting for the level"),
        ];
        let sel = capability_search(&tools, "spawn lights along the x axis", 2);
        let noms: Vec<&str> = sel.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(noms, vec!["spawn_lights", "spawn_actor"]);
    }

    #[test]
    fn cerca_respecta_l_ordre_original_en_empat() {
        let tools = vec![
            eina("create_blueprint", "Create a Blueprint"),
            eina("blueprint_info", "Info about blueprints"),
            eina("save_asset", "Save an asset"),
        ];
        let sel = capability_search(&tools, "blueprint", 2);
        let noms: Vec<&str> = sel.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(noms, vec!["create_blueprint", "blueprint_info"]);
    }

    #[test]
    fn cerca_sense_paraules_retorna_les_primeres() {
        let tools = vec![
            eina("a", "d"),
            eina("b", "d"),
            eina("c", "d"),
        ];
        let sel = capability_search(&tools, "ho", 2);
        assert_eq!(sel.len(), 2);
        assert_eq!(sel[0].name, "a");
    }
}
