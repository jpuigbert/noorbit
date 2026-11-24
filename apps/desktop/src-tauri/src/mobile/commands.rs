//! Comandes Tauri per a mòbil natiu (Android + iOS).
//!
//! Tot es fa dins `spawn_blocking` perquè les crides són síncrones (Command).

use super::android;
use std::path::Path;
use super::{ios, MobileStatus};
use tauri::command;

#[derive(Debug, Clone, serde::Serialize)]
pub struct AndroidRunInfo {
    pub name: String,
    pub running: bool,
}

#[command]
pub async fn mobile_status() -> Result<MobileStatus, String> {
    tauri::async_runtime::spawn_blocking(|| MobileStatus::current())
        .await
        .map_err(|e| e.to_string())
}

// ── Android ────────────────────────────────────────────────────────────────

#[command]
pub async fn mobile_android_list_avds() -> Result<Vec<android::AndroidAvd>, String> {
    tauri::async_runtime::spawn_blocking(|| android::list_avds().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[command]
pub async fn mobile_android_start(name: String) -> Result<AndroidRunInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        android::start_avd(&name).map_err(|e| e.to_string())?;
        Ok(AndroidRunInfo {
            name,
            running: true,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[command]
pub async fn mobile_android_stop(name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || android::stop_avd(&name).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// Captura en base64 PNG la pantalla del primer AVD en marxa.
#[command]
pub async fn mobile_android_screenshot() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| android::screenshot().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

// ── Instal·lació sota demanda de l'SDK d'Android ─────────────────────────

/// Carpeta on caurà l'SDK si l'usuari no n'ha triat cap: la ruta estàndard del
/// sistema operatiu. NO és mai un volum concret: qui vulga un disc extern, que
/// el pose al camp de destinació.
fn default_sdk_dir() -> String {
    if cfg!(target_os = "macos") {
        dirs::home_dir()
            .map(|h| h.join("Library/Android/sdk").to_string_lossy().into_owned())
            .unwrap_or_default()
    } else if cfg!(target_os = "windows") {
        std::env::var("LOCALAPPDATA")
            .map(|l| Path::new(&l).join("Android\\Sdk").to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        dirs::home_dir()
            .map(|h| h.join("Android/Sdk").to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// Escapa una cadena dins d'un literal AppleScript entre cometes dobles.
fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Embolica una ruta amb cometes simples per a shell (aguanta espais).
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Aplica la destinació de veres: guarda la configuració i, a macOS, exporta
/// ANDROID_HOME via `launchctl` perquè qualsevol subprocés (Gradle, adb, un
/// terminal obert des del Finder) la troba — mateix tracte que OLLAMA_MODELS.
fn apply_sdk_dir(dir: &str) -> Result<String, String> {
    let dir = dir.trim().to_string();
    let mut msg = String::new();
    if !dir.is_empty() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("No puc crear «{}» ({}. Volum desendollat o sense permisos?).", dir, e))?;
        #[cfg(target_os = "macos")]
        for key in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
            let _ = std::process::Command::new("launchctl")
                .args(["setenv", key, &dir])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        #[cfg(target_os = "macos")]
        {
            msg.push_str(" S'ha exportat ANDROID_HOME per a tots els programs.");
        }
    }
    Ok(msg)
}

/// Carpeta d'SDK configurada (buit = ruta estàndard del sistema).
#[command]
pub async fn android_get_sdk_dir() -> Result<String, String> {
    Ok(android::configured_sdk_dir())
}

/// Defineix on viurà l'SDK (pot ser un disc extern) SENSE baixar res. Si la
/// carpeta ja té un SDK, a partir d'ara NoOrbit el detectarà des d'allà.
#[command]
pub async fn android_set_sdk_dir(path: Option<String>) -> Result<String, String> {
    let dir = path.unwrap_or_default();
    let nota = apply_sdk_dir(&dir)?;
    let mut cfg = crate::config::AppConfig::load().map_err(|e| e.to_string())?;
    cfg.android.sdk_dir = dir.trim().to_string();
    cfg.save().map_err(|e| e.to_string())?;
    let dest = if cfg.android.sdk_dir.is_empty() {
        default_sdk_dir()
    } else {
        cfg.android.sdk_dir.clone()
    };
    Ok(format!("Destinació d'SDK guardada: {}{}", dest, nota))
}

/// Baixa l'SDK de COMPILACIÓ (sense emulator ni system images, llevat que
/// l'usuari ho demane) en UN SOL CLIC: obri el Terminal amb el script embotit
/// perquè l'usuari veja i autoritze el que s'executa — mateix patró transparent
/// que Blender o ComfyUI.
#[command]
pub async fn android_install_sdk(dest: Option<String>) -> Result<String, String> {
    let mut cfg = crate::config::AppConfig::load().map_err(|e| e.to_string())?;
    let dir = match dest.as_deref().map(str::trim) {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => {
            let c = android::configured_sdk_dir();
            if c.is_empty() {
                default_sdk_dir()
            } else {
                c
            }
        }
    };
    if dir.is_empty() {
        return Err("No s'ha pogut resoldre cap carpeta de destinació.".into());
    }
    apply_sdk_dir(&dir)?;
    cfg.android.sdk_dir = dir.clone();
    cfg.save().map_err(|e| e.to_string())?;
    let api = cfg.android.api;
    let amb_emulador = cfg.android.with_emulator;

    if !cfg!(unix) {
        // Windows: el script necessita bash; obrim la descàrrega oficial i ho
        // diem ben clar en lloc de fingir un clic que funcionaria a mitges.
        crate::commands::system::open_externally(
            "https://developer.android.com/studio#windows-cmdline".to_string(),
        )
        .await?;
        return Ok(format!(
            "A Windows no hi ha bash per fer el clic automàtic: s'ha obert la descàrrega oficial. El mateix script (scripts/install-android-sdk.sh) funciona en Git Bash amb --dest \"{dir}\"."
        ));
    }

    let script = tokio::task::spawn_blocking(android::installer_script_path)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let mut cmd = format!(
        "bash {} --dest {} --api {}",
        shell_quote(&script.to_string_lossy()),
        shell_quote(&dir),
        api
    );
    if amb_emulador {
        cmd.push_str(" --with-emulator");
    }

    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "tell application \"Terminal\" to do script \"{}\"",
            applescript_escape(&cmd)
        ))
        .spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let spawned = std::process::Command::new("x-terminal-emulator")
        .args(["-e", "bash", "-lc", &cmd])
        .spawn();

    spawned.map_err(|e| format!("No s'ha pogut obrir el Terminal: {}", e))?;
    let pes = if amb_emulador {
        "varis GB (emulator + system image)"
    } else {
        "uns 500 MB (sense emulator)"
    };
    Ok(format!(
        "S'ha obert el Terminal per baixar l'SDK de compilació a «{dir}» ({pes}). Quan acabi, prem «Revisa» i NoOrbit el detectarà sol."
    ))
}

/// Cert si el projecte indicat és d'Android: serveix per a OFERIR la baixada
/// només quan el projecte la demana (la idea «segons el projecte», no global).
#[command]
pub async fn android_project_marker(root: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || android::android_project_marker(Path::new(&root)))
        .await
        .map_err(|e| e.to_string())
}

// ── iOS ────────────────────────────────────────────────────────────────────

#[command]
pub async fn mobile_ios_list_devices() -> Result<Vec<ios::IosRuntimeGroup>, String> {
    tauri::async_runtime::spawn_blocking(|| ios::list_devices().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[command]
pub async fn mobile_ios_boot(udid: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || ios::boot(&udid).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[command]
pub async fn mobile_ios_shutdown(udid: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || ios::shutdown(&udid).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// Obri Simulator.app mostrant el dispositiu indicat (perquè l'usuari el veja
/// a la pantalla — iOS no permet «incrustar» la ventana en una altra app).
#[command]
pub async fn mobile_ios_open(udid: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || ios::open_simulator(&udid).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// Si no s'indica UDID, agafa el primer «Booted».
#[command]
pub async fn mobile_ios_screenshot(udid: Option<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let id = match udid {
            Some(s) => s,
            None => ios::booted_udid().ok_or_else(|| "Cap simulador iOS en marxa".to_string())?,
        };
        ios::screenshot(&id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
