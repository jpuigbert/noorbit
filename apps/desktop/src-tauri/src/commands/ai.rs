//! Comandes Tauri per a IA (Ollama).

use crate::ai::{AiManager, ChatOpts, HistoryTurn, OllamaModel};
use crate::AppState;
use std::sync::Arc;
use tauri::{command, AppHandle, State};

/// Xat on s'executa una crida sense `session` explícita.
fn sid_of(session: Option<String>) -> String {
    session.unwrap_or_else(|| crate::ai::DEFAULT_SESSION.to_string())
}

#[command]
pub async fn send_prompt(
    state: State<'_, AppState>,
    prompt: String,
    system: Option<String>,
    session: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    history: Option<Vec<HistoryTurn>>,
) -> Result<String, String> {
    let ai: Arc<AiManager> = state.ai_manager.clone();
    let opts = ChatOpts {
        provider,
        model,
        history: history.unwrap_or_default(),
    };
    // Dins del xat: els events i la cancel·lació són d'aquell xat, no globals.
    crate::ai::in_session(&sid_of(session), ai.chat_opts(&prompt, system.as_deref(), &opts))
        .await
        .map_err(|e| e.to_string())
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
) -> Result<String, String> {
    let ai: Arc<AiManager> = state.ai_manager.clone();
    let opts = ChatOpts {
        provider,
        model,
        history: history.unwrap_or_default(),
    };
    crate::ai::in_session(&sid_of(session), ai.chat_stream(&app, &prompt, system.as_deref(), &opts))
        .await
        .map_err(|e| e.to_string())
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
