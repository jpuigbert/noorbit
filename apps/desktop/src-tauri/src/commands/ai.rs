//! Comandes Tauri per a IA (Ollama).

use crate::ai::{AiManager, ChatOpts, HistoryTurn, OllamaModel};
use crate::computer::ComputerController;
use crate::AppState;
use std::sync::Arc;
use tauri::{command, AppHandle, State};

/// Xat on s'executa una crida sense `session` explícita.
fn sid_of(session: Option<String>) -> String {
    session.unwrap_or_else(|| crate::ai::DEFAULT_SESSION.to_string())
}

/// L'usuari dema accions reals (executar, empaquetar, llançar…) o bé la
/// RESPOSTA del model n'anuncia sense haver-les fet: «has d'executar…», «ara
/// cal compilar…», o comandes en `backticks` perquè les corre l'usuari. En
/// eixos casos, si el model NO ha emés cap directriu, li'n fem un recordatori
/// UNA sola vegada (EXEC_NUDGE): els models menuts s'obliden de les eines si
/// no se'ls empeny, i l'usuari es queda «amb la resposta i prou».
fn expects_execution(prompt: &str, answer: &str) -> bool {
    use regex::Regex;
    let user_re = match Regex::new(
        r"(?i)(execut|llan[cç]|empaquet|compil|instal|arrenca|funcionar|en marxa|prova\s|testej|comprova|fes\s+que)\w*",
    ) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let answer_re = match Regex::new(
        r"(?i)(heu d'?\s*executar|has d'?\s*executar|cal executar|per\s+a?\s+executar|executa(?:'s)?\s+(?:el|la|primer|ara)|executeu|obri\s+i\s+executa|`[^`]*\b(?:python3?|npm|pip|cargo|make|bash|node|open)\b[^`]*`)",
    ) {
        Ok(r) => r,
        Err(_) => return false,
    };
    user_re.is_match(prompt) || answer_re.is_match(answer)
}

/// Recordatori que es torna al model quan ha parlat d'executar sense haver
/// emés cap directriu. Curto i imperatiu: fins i tot un model 1.5b el obeïx.
const EXEC_NUDGE: &str = "NoOrbit: NO has emés cap directriu «RUN|», per tant NO \
s'ha executat res i l'usuari esperava accions reals. Si la tasca requereix \
executar, compilar o llançar alguna cosa: 1) si falten fitxers, reemet-los \
sencers ara mateix amb línies «@file: ruta» + contingut; 2) després escriu UNA \
línia «RUN|<comanda>» («RUN|BG|<comanda>» per a servidors/apps que han de \
quedar oberts) i ATURA la resposta eixe torn. No demanes a l'usuari que \
execute ell: executes tu. Si de veritat no cal cap execució, respon amb \
normalitat.";

/// Detecta fitxers que la IA ha ANNUNCIAT amb «@file: ruta» però ha deixat 
/// BUIT (cap línia de codi entre l'anunci i el següent @file:/@dir:/final de 
/// la resposta, o només prosa/promeses). Els models menuts fan sovint això: 
/// creen el fitxer i diuen «ara escric el codi» i s'aturen. Retornar les 
/// rutes buits permet donar-los un recordatori (FILE_NUDGE) perquè reemetent 
/// el contingut COMPLET en el mateix torn.
fn empty_file_paths(text: &str) -> Vec<String> {
    use regex::Regex;
    let Ok(re) = Regex::new(r"(?im)^[ \t>*-]*@?file:\s*(.+?)\s*$") else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    // Marques de cada línia que inicia un bloc (@file:, @dir:, @delete:…).
    let is_marker = |l: &str| -> bool {
        Regex::new(r"(?im)^\s*[\t >*-]*@?(file|dir|directori|carpeta|folder|delete|esborra|borra|elimina|remove|rm)\w*:\s*(.*)$")
            .map(|r| r.is_match(l))
            .unwrap_or(false)
    };
    let mut out: Vec<String> = Vec::new();
    for c in re.captures_iter(text) {
        let path = c.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        // Troba la línia de l'anunci i suma el contingut fins al següent marcador.
        let Some(idx) = lines.iter().position(|l| {
            re.is_match(l) && l.to_lowercase().replace('"', "").contains(&path.to_lowercase())
        }) else {
            continue;
        };
        let mut content = String::new();
        for l in &lines[idx + 1..] {
            if is_marker(l) {
                break;
            }
            if l.trim().starts_with("```") {
                continue; // tanques de codi: no són contingut
            }
            content.push_str(l.trim());
        }
        // Buit, o només una promesa («a continuació», «ara escriuré», TODO…).
        let low = content.to_lowercase();
        let flops = content.chars().count() < 40
            || low.contains("pendent d'implementar")
            || low.contains("TODO")
            || (low.contains("a continuaci") && !low.contains('\n'));
        if flops && !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// Recordatori per quan la IA anuncia fitxers i els deixa buits: que els 
/// reemetja amb el codi real. Una sola vegada per torn de xat.
const FILE_NUDGE: &str = "NoOrbit: has annunciat fitxers amb «@file:» però han \
quedats BUITS (cap línia de codi després de l'anunci). Un @file: sense \
contingut crea un fitxer buit: NO serveix. Reemet ara CADA fitxer que falta \
amb la seua línia «@file: ruta» i, JUST a la línia de seguida, el codi COMPLET \
(funcions, imports, lògica real; res de «…» ni promeses). Pot ser llarg: \
escriu'l sencer. Si de veritat no cal cap fitxer, respon amb normalitat.";

/// Un torn de xat amb les EINES DE NOORBIT connectades: qualsevol resposta
/// del model que continga directrius «NB|…» (navegador) o «RUN|…» / «IMG|…»
/// (terminal del projecte i generador d'imatges) s'executa de veritat, se'n
/// torna el resultat al model i el model acaba la feina amb ell. Així TOTES
/// les IAs (local sense token, remota, experts) poden navegar, compilar,
/// empaquetar, llançar i crear imatges sense cap botó ni endollable: només
/// cal que sàpiguen escriure la directriu.
async fn chat_with_tools(
    app: AppHandle,
    ai: &Arc<AiManager>,
    computer: &Arc<ComputerController>,
    prompt: &str,
    system: Option<&str>,
    mut opts: ChatOpts,
    sid: &str,
    streaming: bool,
) -> Result<String, String> {
    use crate::agent::runtools;
    use crate::browser::tools;
    // Límit de torns eina→resultat→model: prou per a cicles
    // escriure→compilar→corregir→tornar a compilar dins d'UNA tasca.
    const MAX_ROUNDS: usize = 6;
    // El manual de directrius s'afegeix AL SYSTEM PROMPT DE TOTES les IAs.
    // S'hi afegeix l'ESTAT REAL de les eines ara mateix (Blender connectat?
    // Unreal? projecte obert? imatges en local?) perquè el model no propose
    // el que aquest ordinador no pot fer.
    let status = runtools::tools_status_line(computer.permissions().enabled).await;
    let manual = format!("{}\n\n{}{}", tools::DIRECTIVE_PROMPT, runtools::RUN_PROMPT, status);
    let sys = match system {
        Some(s) => format!("{}\n\n{}", s, manual),
        None => manual,
    };
    let mut current = prompt.to_string();
    let mut text = String::new();
    let mut nudged = false;
    let mut files_nudged = false;
    let t0 = std::time::Instant::now();
    for round in 0..=MAX_ROUNDS {
        let res = if streaming {
            crate::ai::in_session(sid, ai.chat_stream(&app, &current, Some(&sys), &opts)).await
        } else {
            crate::ai::in_session(sid, ai.chat_opts(&current, Some(&sys), &opts)).await
        };
        text = match res {
            Ok(t) => t,
            Err(e) => {
                // El visor de procés MAI ha de quedar amb un «ERROR» mut i amb el
                // temps a zero: ací es conta QUÈ ha fallat i quant s'ha esperat.
                let msg = e.to_string();
                if msg != crate::ai::CANCELLED_MSG && !ai.session_error_shown(sid) {
                    let (provider, model) = ai.session_labels(sid).await;
                    crate::ai::emit_process(
                        &app,
                        sid,
                        &provider,
                        &model,
                        "error",
                        &msg,
                        t0.elapsed().as_millis() as u64,
                    );
                }
                return Err(msg);
            }
        };
        let nb = tools::parse(&text);
        let runs = runtools::parse(&text);
        if (nb.is_empty() && runs.is_empty()) || round == MAX_ROUNDS {
            // Sense directrius… però la tasca demanava accions reals? Si encara
            // no hem fet cap recordatori, donem-li UNA oportunitat més: el
            // torn que ve hauria d'anar ple de «RUN|» (o «@file:» si falten
            // fitxers) i NoOrbit l'executarà de veritat.
            if round < MAX_ROUNDS && !nudged && expects_execution(prompt, &text) {
                nudged = true;
                opts.history.push(HistoryTurn {
                    role: "assistant".into(),
                    content: text.clone(),
                });
                current = EXEC_NUDGE.to_string();
                continue;
            }
            // I si ha annunciat fitxers amb «@file:» i els ha deixat BUITS
            // (el model menut crea l'arxiu i promet el codi «després»)? Un
            // recordatori, UNA vegada: que reemetja cada fitxer COMPLET.
            if round < MAX_ROUNDS && !files_nudged {
                let empties = empty_file_paths(&text);
                if !empties.is_empty() {
                    files_nudged = true;
                    opts.history.push(HistoryTurn {
                        role: "assistant".into(),
                        content: text.clone(),
                    });
                    current =
                        format!("{}\n\nFitxers que han quedat buits: {}", FILE_NUDGE, empties.join(", "));
                    continue;
                }
            }
            break;
        }
        // El torn del model i els resultats de les eines entren a la conversa
        // com un torn nou: el model veu dades reals i respon sobre segur.
        let mut report = String::new();
        if !runs.is_empty() {
            // PRIMER els fitxers, DESPRÉS les ordres: si el model anuncia
            // «@file: x.py» i «RUN|python3 x.py» en el mateix torn, el fitxer
            // ha d'existir JA al disc quan s'executa (si no: «No such file»).
            // La materialització del frontend reescriurà el definitiu després.
            let pre = runtools::write_announced_files(&app, &text);
            if !pre.is_empty() {
                report.push_str(&format!(
                    "RUN|RESULTAT| fitxers escrits al projecte abans d'executar: {}\n",
                    pre.join(", ")
                ));
            }
            report.push_str(&runtools::execute(&app, computer, ai, &runs).await);
            report.push('\n');
        }
        if !nb.is_empty() {
            report.push_str(&tools::execute(&app, &nb).await);
        }
        opts.history.push(HistoryTurn {
            role: "assistant".into(),
            content: text.clone(),
        });
        current = report;
    }
    Ok(text)
}

#[command]
pub async fn send_prompt(
    app: AppHandle,
    state: State<'_, AppState>,
    prompt: String,
    system: Option<String>,
    session: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    history: Option<Vec<HistoryTurn>>,
    images: Option<Vec<String>>,
) -> Result<String, String> {
    let ai: Arc<AiManager> = state.ai_manager.clone();
    let opts = ChatOpts {
        provider,
        model,
        history: history.unwrap_or_default(),
        images: images.unwrap_or_default(),
    };
    // Dins del xat: els events i la cancel·lació són d'aquell xat, no globals.
    chat_with_tools(app, &ai, &state.computer, &prompt, system.as_deref(), opts, &sid_of(session), false).await
}

/// Envia un prompt i va emetent fragments amb l'event «ai://chunk» mentre el
/// model genera, per que la interficie mostre el text en directe. Retorna el
/// text complet al final (igual que `send_prompt`).
/// `session`/`provider`/`model`/`history` permeten que cada xat treballe en
/// paral·lel amb la seua IA (local o remota) i continue una conversa existent
/// o un «fork» d'una altra.
#[command]
pub async fn send_prompt_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    prompt: String,
    system: Option<String>,
    session: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    history: Option<Vec<HistoryTurn>>,
    images: Option<Vec<String>>,
) -> Result<String, String> {
    let ai: Arc<AiManager> = state.ai_manager.clone();
    let opts = ChatOpts {
        provider,
        model,
        history: history.unwrap_or_default(),
        images: images.unwrap_or_default(),
    };
    // També en streaming: si el model escriu directrius «NB|…» o «RUN|…»,
    // s'executen i la segona resposta continua al mateix xat (els chunks
    // arriben igual).
    chat_with_tools(
        app,
        &ai,
        &state.computer,
        &prompt,
        system.as_deref(),
        opts,
        &sid_of(session),
        true,
    )
    .await
}

#[command]
pub async fn list_providers(state: State<'_, AppState>) -> Result<Vec<crate::ai::ProviderInfo>, String> {
    let ai = state.ai_manager.clone();
    Ok(ai.list_providers().await)
}

#[command]
pub async fn set_active_provider(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let ai = state.ai_manager.clone();
    ai.set_active_provider(&id).await;
    Ok(())
}

#[command]
pub async fn ollama_is_running(state: State<'_, AppState>) -> Result<bool, String> {
    let ai = state.ai_manager.clone();
    Ok(ai.is_running().await)
}

#[command]
pub async fn ollama_list_models(state: State<'_, AppState>) -> Result<Vec<OllamaModel>, String> {
    let ai = state.ai_manager.clone();
    ai.list_models().await.map_err(|e| e.to_string())
}

#[command]
pub async fn ollama_pull_model(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<String, String> {
    use tauri::Emitter;
    // Abans de descarregar: si hi ha un disc extern configurat, assegurem que el
    // servidor l'utilitzi (si cal, l'arrenca o el reinicia amb OLLAMA_MODELS).
    // Sense això, un Ollama en marxa iniciat des del Finder baixaria els
    // models al disc intern malgrat la configuració.
    let nota = tokio::task::spawn_blocking(crate::ai::ensure_models_dir_applied)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    if let Some(msg) = nota {
        let _ = app.emit(
            "ai://pull",
            serde_json::json!({ "model": name, "message": msg }),
        );
    }
    let ai = state.ai_manager.clone();
    ai.pull_model(&app, &name).await.map_err(|e| e.to_string())
}

#[command]
pub async fn ollama_delete_model(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let ai = state.ai_manager.clone();
    ai.delete_model(&name).await.map_err(|e| e.to_string())
}

#[command]
pub async fn ollama_suggested_models() -> Result<Vec<serde_json::Value>, String> {
    Ok(AiManager::suggested_models())
}

/// Llista/cerca models al NÚVOL d'ollama.com. No requereix Ollama en marxa
/// ni genera tokens: només descarrega la pàgina de la biblioteca.
#[command]
pub async fn ollama_search_cloud(
    state: State<'_, AppState>,
    query: Option<String>,
) -> Result<Vec<crate::ai::CloudModel>, String> {
    let ai = state.ai_manager.clone();
    ai.search_cloud(query.as_deref().unwrap_or(""))
        .await
        .map_err(|e| e.to_string())
}

/// Cerca models a INTERNET (Hugging Face) per si NO són a Ollama. Retorna noms
/// «hf.co/…» que es poden baixar en natiu amb `ollama_pull_model`.
#[command]
pub async fn ollama_search_huggingface(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<crate::ai::CloudModel>, String> {
    let ai = state.ai_manager.clone();
    ai.search_huggingface(&query)
        .await
        .map_err(|e| e.to_string())
}

/// Diu si el binari/app d'Ollama és al sistema (per oferir instal·lar-lo).
#[command]
pub async fn ollama_installed() -> Result<bool, String> {
    Ok(crate::ai::ollama_installed())
}

/// Arrenca el servei `ollama serve` en segon pla (si està instal·lat),
/// aplicant la carpeta de models configurada (disc extern). Pot reiniciar
/// un Ollama ja en marxa si no la feia servir; per això es fa en blocking.
#[command]
pub async fn start_ollama() -> Result<String, String> {
    let msg = tokio::task::spawn_blocking(|| {
        crate::ai::start_ollama_server()?;
        // Si Ollama ja rodava sense la carpeta, start_ollama_server l'ha
        // reiniciat; ho confirmem amb el missatge de l'aplicació.
        anyhow::Ok(crate::ai::apply_models_dir(false).ok().flatten())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    Ok(msg.unwrap_or_else(|| "Ollama és en marxa.".to_string()))
}

/// Instal·la Ollama. Obre el mètode adequat a cada sistema:
/// - macOS: Terminal amb `brew install ollama` si hi ha Homebrew; si no, la
///   pàgina de descàrrega oficial.
/// - Linux: Terminal amb l'instal·lador oficial (curl … | sh).
/// - Windows: pàgina de descàrrega oficial.
/// Sempre és transparent: l'usuari veu i autoritza el que s'executa.
#[command]
pub async fn install_ollama() -> Result<String, String> {
    let download_url = "https://ollama.com/download";

    #[cfg(target_os = "macos")]
    {
        if which::which("brew").is_ok() {
            let script = "tell application \"Terminal\" to do script \"brew install ollama && ollama serve\"";
            std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
                .spawn()
                .map_err(|e| format!("No s'ha pogut obrir el Terminal: {}", e))?;
            return Ok("S'ha obert el Terminal per instal·lar Ollama amb Homebrew.".into());
        }
        std::process::Command::new("open")
            .arg(download_url)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega d'Ollama.".into());
    }

    #[cfg(target_os = "linux")]
    {
        let cmd = "curl -fsSL https://ollama.com/install.sh | sh";
        let term = which::which("x-terminal-emulator")
            .or_else(|_| which::which("gnome-terminal"))
            .or_else(|_| which::which("konsole"))
            .or_else(|_| which::which("xterm"))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if term.is_empty() {
            std::process::Command::new("xdg-open")
                .arg(download_url)
                .spawn()
                .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
            return Ok("S'ha obert la pàgina de descàrrega d'Ollama.".into());
        }
        std::process::Command::new(term)
            .arg("-e")
            .arg(format!("sh -c '{}; exec sh'", cmd))
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el Terminal: {}", e))?;
        return Ok("S'ha obert el Terminal per instal·lar Ollama.".into());
    }

    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg(download_url)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega d'Ollama.".into());
    }

    #[allow(unreachable_code)]
    Err("Instal·lació no suportada en aquest sistema".into())
}

// ── DeepSeek Harness (dsh): instal·lació autogestionada, sense admin ───────

/// Diu si DeepSeek Harness (dsh) està disponible: la instal·lació del
/// sistema (npm global, brew…) o la LOCAL que NoOrbit gestiona dins de les
/// seves dades (amb el seu propi Node.js, si al sistema no n'hi ha).
#[command]
pub async fn dsh_installed() -> Result<bool, String> {
    Ok(crate::external_orchestrator::dsh::dsh_installed())
}

/// Instal·la DeepSeek Harness automaticament: baixa el Node.js LTS oficial
/// (si no n'hi ha cap) i el paquet @deepseek-ai/dsh, tot DINS de les dades
/// de NoOrbit (o de NoOrbitData/ en mode portàtil/USB). No demanà
/// administració, no modifica el sistema i es pot emportar en un USB.
#[command]
pub async fn install_dsh(app: AppHandle) -> Result<String, String> {
    crate::external_orchestrator::dsh::install(app)
        .await
        .map_err(|e| e.to_string())
}

/// Arrenca la interfície web de dsh («dsh web», port 3080) en segon pla,
/// perquè l'usuari la configuri (models, claus) i la puga obrir al
/// navegador intern amb «NB|PREGUNTA_IA|dsh|…» o des d'esta mateixa secció.
#[command]
pub async fn start_dsh_web() -> Result<String, String> {
    crate::external_orchestrator::dsh::start_web().map_err(|e| e.to_string())?;
    Ok(
        "He arrencat «dsh web»: la seva interfície estarà a http://127.0.0.1:3080 \
         (si triga, torna a prémer en uns segons)."
            .into(),
    )
}

/// Diu si la UI web de dsh respon ja al port 3080 (per a l'etiqueta
/// «en marxa» de la secció de harnessos del Gestor de proveïdors).
#[command]
pub async fn dsh_web_running() -> Result<bool, String> {
    Ok(std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], 3080)),
        std::time::Duration::from_millis(400),
    )
    .is_ok())
}

/// Diu si ComfyUI és al sistema (per oferir instal·lar-lo o arrencar-lo).
#[command]
pub async fn comfyui_installed() -> Result<bool, String> {
    Ok(crate::ai::comfyui_installed())
}

/// Diu si el servidor ComfyUI respon ja al port 8188.
#[command]
pub async fn comfyui_running() -> Result<bool, String> {
    Ok(crate::ai::comfyui_running())
}

/// Arrenca el servei ComfyUI en segon pla (si està instal·lat).
#[command]
pub async fn start_comfyui() -> Result<(), String> {
    crate::ai::start_comfyui().map_err(|e| e.to_string())
}

/// Instal·la ComfyUI. mateix procés transparent que amb Ollama:
/// - macOS: Terminal amb `brew install --cask comfyui` si hi ha Homebrew; si
///   no, la pàgina de descàrrega oficial.
/// - Linux/Windows: pàgina de descàrrega oficial (instal·ladors i portàtil).
/// L'usuari sempre veu i autoritza el que s'executa.
#[command]
pub async fn install_comfyui() -> Result<String, String> {
    let download_url = "https://www.comfy.org/download";

    #[cfg(target_os = "macos")]
    {
        // ComfyUI oficial només funciona en Apple Silicon. En un Mac Intel
        // (x86_64) no cal intentar instal·lar-lo: recomanem un proveïdor
        // d'imatges en línia (que NoOrbit usa per a «IMG|» i el xat).
        #[cfg(target_arch = "x86_64")]
        {
            let _ = download_url;
            return Err(
                "ComfyUI no està disponible per a Mac Intel (només Apple Silicon). \
                 Per generar imatges en aquest ordinador, registra un proveïdor en \
                 línia amb token al menú «Proveïdors d'IA» (p. ex. OpenAI amb el \
                 model «dall-e-3») o Venice; després el xat i «IMG|» funcionaran."
                    .into(),
            );
        }
        #[cfg(target_arch = "aarch64")]
        {
            if which::which("brew").is_ok() {
                let script = "tell application \"Terminal\" to do script \"brew install --cask comfyui && open -a ComfyUI\"";
                std::process::Command::new("osascript")
                    .arg("-e")
                    .arg(script)
                    .spawn()
                    .map_err(|e| format!("No s'ha pogut obrir el Terminal: {}", e))?;
                return Ok("S'ha obert el Terminal per instal·lar ComfyUI amb Homebrew.".into());
            }
            std::process::Command::new("open")
                .arg(download_url)
                .spawn()
                .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
            return Ok("S'ha obert la pàgina de descàrrega de ComfyUI.".into());
        }
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(download_url)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega de ComfyUI.".into());
    }

    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg(download_url)
            .spawn()
            .map_err(|e| format!("No s'ha pogut obrir el navegador: {}", e))?;
        return Ok("S'ha obert la pàgina de descàrrega de ComfyUI.".into());
    }

    #[allow(unreachable_code)]
    Err("Instal·lació no suportada en aquest sistema".into())
}

/// Triat el model d'Ollama actiu per a tots els xats i agents.
#[command]
pub async fn ollama_set_model(
    state: State<'_, AppState>,
    name: Option<String>,
) -> Result<(), String> {
    let ai = state.ai_manager.clone();
    ai.set_selected_model(name).await;
    Ok(())
}

/// Model que s'està fent servir ara mateix (triat o per defecte).
#[command]
pub async fn ollama_active_model(state: State<'_, AppState>) -> Result<String, String> {
    let ai = state.ai_manager.clone();
    Ok(ai.effective_model().await)
}

/// Últim pensament (raonament) emès per un xat, per mostrar-lo a la UI.
/// Es buida a cada generació nova; retorna None si el model no raona.
#[command]
pub async fn ai_last_thinking(
    state: State<'_, AppState>,
    session: Option<String>,
) -> Result<Option<String>, String> {
    let ai = state.ai_manager.clone();
    Ok(ai.last_thinking(&sid_of(session)).await)
}

/// Descobreix les IAs locals EXTERNALS presents al sistema (Ollama, LM
/// Studio, Jan, llama.cpp, vLLM…): en marxa o només instal·lades.
#[command]
pub async fn external_ias_discover(
    state: State<'_, AppState>,
) -> Result<Vec<crate::external_orchestrator::ExternalIa>, String> {
    let ai = state.ai_manager.clone();
    Ok(ai.external.discover().await)
}

/// Delega una tasca directament a una IA local externa o a un proveïdor
/// oficial amb token (prova les IAs en marxa i retorna la primera resposta
/// vàlida, amb el seu origen etiquetat). El xat normal ja fa este bot de
/// rescat quan la IA principal no resol la tasca; esta comanda serveix per a
/// provar-lo o usar-lo de forma explícita.
#[command]
pub async fn delegate_external_ia(
    state: State<'_, AppState>,
    prompt: String,
    system: Option<String>,
) -> Result<String, String> {
    let ai = state.ai_manager.clone();
    ai.external
        .delegate(&prompt, system.as_deref())
        .await
        .map(|resp| resp.text)
        .map_err(|e| e.to_string())
}

/// Retorna la carpeta on Ollama desa els models (buide = per defecte).
#[command]
pub async fn ollama_get_models_dir() -> Result<String, String> {
    Ok(crate::config::AppConfig::load()
        .map(|c| c.ai.models_dir)
        .unwrap_or_default())
}

/// Defineix la carpeta de models (p. ex. un USB extern), la desa i L'APLICA
/// de seguida: reinicia Ollama amb OLLAMA_MODELS perquè les descàrregues
/// següents vagin al disc extern (i no al dur, com passava abans).
#[command]
pub async fn ollama_set_models_dir(path: Option<String>) -> Result<String, String> {
    let mut cfg = crate::config::AppConfig::load().map_err(|e| e.to_string())?;
    cfg.ai.models_dir = path.unwrap_or_default().trim().to_string();
    cfg.save().map_err(|e| e.to_string())?;
    let dir = cfg.ai.models_dir.clone();
    let msg = tokio::task::spawn_blocking(|| crate::ai::apply_models_dir(true))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    Ok(msg.unwrap_or_else(|| {
        if dir.is_empty() {
            "Els models tornaran a la carpeta per defecte del sistema.".to_string()
        } else {
            format!("Els models es baixaran i s'executaran des de {}.", dir)
        }
    }))
}

/// Mou els models ja baixats al disc intern cap al disc extern configurat i
/// reinicia Ollama apuntant-hi. Emet progrés a «ai://migrate».
#[command]
pub async fn ollama_migrate_models(app: AppHandle) -> Result<String, String> {
    tokio::task::spawn_blocking(move || crate::ai::migrate_models(&app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Llista els volums muntats (USB/discos externs) per suggerir-los a la UI.
/// A Windows no s'exposa res: l'usuari tria la unitat des del diàleg natiu.
#[command]
pub async fn list_external_volumes() -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        Ok(vec![])
    }
    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        let base = "/Volumes";
        #[cfg(target_os = "linux")]
        let base = "/media";
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(base) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let p = e.path().to_string_lossy().to_string();
                if std::fs::metadata(&p).map(|m| m.is_dir()).unwrap_or(false) {
                    out.push(p);
                }
            }
        }
        Ok(out)
    }
}

// ── Catàleg d'IAs: característiques ABANS de descarregar-les ────────────────

/// Entrada del catàleg que mostra el menú IA: mida aproximada, si és sense
/// censura, si ACCEPTA imatges (visió), si EN genera (imatges/vídeo) i si ja
/// és instal·lada. Mida 0 = model al núvol, no cal baixar res.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IaCatalogEntry {
    pub name: String,
    /// «ollama» per als models locals; nom del proveïdor per als en línia.
    pub source: String,
    pub approx_size_gb: f64,
    pub uncensored: bool,
    pub vision: bool,
    pub image_gen: bool,
    pub video_gen: bool,
    pub installed: bool,
    pub note: String,
}

/// Consulta el catàleg: taula curated dels models locals més rellevants
/// (mida aproximada de l'etiqueta estàndard; les variants «:3b» o q4 pesen
/// molt menys) + els proveïdors en línia que l'usuari haja registrat.
#[command]
pub async fn ia_catalog(state: State<'_, AppState>) -> Result<Vec<IaCatalogEntry>, String> {
    // (nom, GB aproximats, sense censura, visió, nota)
    const LOCAL: [(&str, f64, bool, bool, &str); 24] = [
        ("qwen3:8b", 5.2, false, false, "Raonament i codi; pesos oberts (Apache 2.0)"),
        ("qwen2.5-coder:7b", 4.7, false, false, "Codi — equilibri qualitat/velocitat"),
        ("qwen2.5-coder:3b", 1.9, false, false, "Codi — per a màquines modestes"),
        ("llama3.2:3b", 2.0, false, false, "General — ràpid i multilingüe"),
        ("mistral:7b", 4.4, false, false, "General — bo seguint instruccions"),
        ("codellama:13b", 7.5, false, false, "Codi — més qualitat, més lent"),
        ("deepseek-r1:8b", 5.2, false, false, "Raonament pas a pas (thinking visible)"),
        ("gpt-oss:20b", 12.6, false, false, "OpenAI de pesos oberts — MoE eficient"),
        ("gpt-oss:120b", 61.0, false, false, "Màxima qualitat local — requereix ~60 GB de RAM"),
        ("dolphin-mistral:7b", 4.4, true, false, "Sense filtres — codi i raonament"),
        ("dolphin-mixtral:8x7b", 26.0, true, false, "Sense filtres — MoE 8x7b (cal molta RAM)"),
        ("goekdenizguelmez/JOSIEFIED-Qwen3", 5.0, true, false, "Sense filtres — Qwen3-8B abliterated"),
        ("richardyoung/qwen3-8b-abliterated", 5.0, true, false, "Sense filtres — Qwen3 per a ús lliure"),
        ("alibayram/mimo-7b-rl", 4.7, false, false, "Xiaomi MiMo — raonament mat/codi, thinking visible"),
        ("maternion/mimo-v2.6", 6.0, false, false, "Xiaomi MiMo-V2.6 — agent 9B amb eines"),
        ("llava:7b", 4.2, false, true, "Visió: descriu i respon sobre imatges"),
        ("llava:13b", 8.0, false, true, "Visió — més qualitat que el 7B"),
        ("llama3.2-vision:11b", 7.9, false, true, "Visió — Meta, bona lectura d'imatges"),
        ("gemma3:4b", 3.3, false, true, "Visió — lleuger, de Google"),
        ("gemma3:12b", 8.1, false, true, "Visió — equilibri"),
        ("gemma3:27b", 17.0, false, true, "Visió — el millor gemma3, cal RAM"),
        ("minicpm-v:8b", 6.6, false, true, "Visió — excel·lent en detalls i OCR"),
        ("moondream:latest", 1.7, false, true, "Visió — miniatura (1.7 GB), ràpid"),
        ("stable-diffusion-v2:latest", 4.9, false, false, "Genera imatges, però NoOrbit recomana ComfyUI (qualitat i control)"),
    ];
    let installed: Vec<String> = state
        .ai_manager
        .list_models()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.name)
        .collect();
    let mut out: Vec<IaCatalogEntry> = LOCAL
        .iter()
        .map(|(name, gb, unc, vis, note)| {
            let fam = name.split(':').next().unwrap_or(name);
            IaCatalogEntry {
                name: name.to_string(),
                source: "ollama".into(),
                approx_size_gb: *gb,
                uncensored: *unc,
                vision: *vis,
                image_gen: name.starts_with("stable-diffusion"),
                // Cap model d'aquesta llista genera vídeo avui.
                video_gen: false,
                installed: installed
                    .iter()
                    .any(|i| i == name || i.split(':').next().unwrap_or(i) == fam),
                note: note.to_string(),
            }
        })
        .collect();

    // Proveïdors en línia registrats per l'usuari: característiques conegudes
    // segons la URL base (cap no genera vídeo de moment).
    let guard = state.api_manager.lock().map_err(|e| e.to_string())?;
    for p in guard.providers.iter().filter(|p| p.enabled) {
        let base = p.base_url.to_lowercase();
        let model = p.model.to_lowercase();
        let (unc, vis, gen, note) = if base.contains("openai.com") {
            (
                false,
                model.contains("4o") || model.contains("4.1") || model.contains("o1") || model.contains("o3"),
                true,
                "Genera imatges amb «dall-e-3»/«gpt-image-1» via l'API d'imatges".to_string(),
            )
        } else if base.contains("anthropic") {
            (false, true, false, "Rep imatges com a entrada (visió); no en genera".to_string())
        } else if base.contains("ollama.com") {
            (false, false, false, "Models al núvol d'Ollama: no cal baixar res".to_string())
        } else if base.contains("venice") {
            (
                true,
                false,
                true,
                "Inferència sense filtres; model de xat «venice-uncensored», \
                 imatges amb «venice-sd15»; API pròpia".to_string(),
            )
        } else if base.contains("deepseek") {
            (false, false, false, "Només text (deepseek-chat / reasoner)".to_string())
        } else if base.contains("perplexity") {
            (false, false, false, "Cerca web en línia amb citacions; només text".to_string())
        } else {
            (false, false, false, "Sense metadades conegudes per a aquesta URL".to_string())
        };
        out.push(IaCatalogEntry {
            name: format!("{} · {}", p.name, p.model),
            source: p.name.clone(),
            approx_size_gb: 0.0,
            uncensored: unc,
            vision: vis,
            image_gen: gen,
            video_gen: false,
            installed: !p.token.trim().is_empty(),
            note,
        });
    }
    drop(guard);
    Ok(out)
}
