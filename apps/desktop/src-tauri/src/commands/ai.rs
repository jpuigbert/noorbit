//! Comandes Tauri per a IA (Ollama).

use crate::ai::{AiManager, ChatOpts, HistoryTurn, OllamaModel};
use crate::AppState;
use std::sync::Arc;
use tauri::{command, AppHandle, State};

/// Xat on s'executa una crida sense `session` explícita.
fn sid_of(session: Option<String>) -> String {
    session.unwrap_or_else(|| crate::ai::DEFAULT_SESSION.to_string())
}

/// Un torn de xat amb el NAVEGADOR INTERN connectat: qualsevol resposta del
/// model que continga directrius «NB|…» s'executa de veritat per NoOrbit, en
/// torna el resultat al model i el model acaba la resposta amb ell. Així
/// TOTES les IAs (local sense token, remota, experts) naveguen sense cap
/// botó ni endollable: només cal que sàpiguen escriure la directriu.
async fn chat_with_browser(
    app: AppHandle,
    ai: &Arc<AiManager>,
    prompt: &str,
    system: Option<&str>,
    mut opts: ChatOpts,
    sid: &str,
    streaming: bool,
) -> Result<String, String> {
    use crate::browser::tools;
    // El manual de directrius s'afegeix AL SYSTEM PROMPT DE TOTES les IAs.
    let sys = match system {
        Some(s) => format!("{}\n\n{}", s, tools::DIRECTIVE_PROMPT),
        None => tools::DIRECTIVE_PROMPT.to_string(),
    };
    let mut current = prompt.to_string();
    let mut text = String::new();
    for round in 0..=tools::MAX_ROUNDS {
        text = if streaming {
            crate::ai::in_session(sid, ai.chat_stream(&app, &current, Some(&sys), &opts))
                .await
                .map_err(|e| e.to_string())?
        } else {
            crate::ai::in_session(sid, ai.chat_opts(&current, Some(&sys), &opts))
                .await
                .map_err(|e| e.to_string())?
        };
        let actions = tools::parse(&text);
        // Sense directrius (o límit de torns assolit): resposta definitiva.
        if actions.is_empty() || round == tools::MAX_ROUNDS {
            break;
        }
        // El torn del model i el resultat del navegador entren a la conversa
        // com un torn nou: el model veu les dades reals i respon sobre segur.
        let report = tools::execute(&app, &actions).await;
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
    chat_with_browser(app, &ai, &prompt, system.as_deref(), opts, &sid_of(session), false).await
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
    // També en streaming: si el model escriu directrius «NB|…», s'executen i
    // la segona resposta continua al mateix xat (els chunks arriben igual).
    chat_with_browser(
        app,
        &ai,
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

/// Arrenca el servei `ollama serve` en segon pla (si està instal·lat).
#[command]
pub async fn start_ollama() -> Result<(), String> {
    crate::ai::start_ollama_server().map_err(|e| e.to_string())
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

/// Defineix la carpeta de models (p. ex. un USB extern) i la desa.
#[command]
pub async fn ollama_set_models_dir(path: Option<String>) -> Result<(), String> {
    let mut cfg = crate::config::AppConfig::load().map_err(|e| e.to_string())?;
    cfg.ai.models_dir = path.unwrap_or_default().trim().to_string();
    cfg.save().map_err(|e| e.to_string())
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
