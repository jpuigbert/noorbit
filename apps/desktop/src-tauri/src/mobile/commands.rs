//! Comandes Tauri per a mòbil natiu (Android + iOS).
//!
//! Tot es fa dins `spawn_blocking` perquè les crides són síncrones (Command).

use super::{android, ios, MobileStatus};
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
