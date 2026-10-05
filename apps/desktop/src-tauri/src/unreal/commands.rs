//! Comandes Tauri per a Unreal Engine 5.

use super::builder::{self, PackageOptions};
use super::client::UnrealClient;
use super::launcher;
use super::python;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{command, AppHandle};

#[derive(Debug, Serialize, Deserialize)]
pub struct UnrealConfig {
    pub host: String,
    pub port: u16,
}

impl Default for UnrealConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 30010,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UnrealStatus {
    pub connected: bool,
    pub engine_version: Option<String>,
    pub project_name: Option<String>,
}

#[command]
pub async fn unreal_ping(config: UnrealConfig) -> Result<bool, String> {
    let client = UnrealClient::with_port(config.port);
    client.ping().await.map_err(|e| e.to_string())
}

#[command]
pub async fn unreal_info(config: UnrealConfig) -> Result<UnrealStatus, String> {
    let client = UnrealClient::with_port(config.port);
    let connected = client.ping().await.unwrap_or(false);
    if !connected {
        return Ok(UnrealStatus {
            connected: false,
            engine_version: None,
            project_name: None,
        });
    }
    let info = client.info().await.map_err(|e| e.to_string())?;
    Ok(UnrealStatus {
        connected: true,
        engine_version: info.engine_version,
        project_name: info.project_name,
    })
}

#[command]
pub async fn unreal_detect_installations() -> Result<Vec<String>, String> {
    Ok(launcher::detect_engine_installations()
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}

#[command]
pub async fn unreal_auto_connect(
    project: String,
    timeout: u64,
) -> Result<launcher::LaunchReport, String> {
    launcher::auto_connect(&PathBuf::from(&project), timeout)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn unreal_run_python(
    config: UnrealConfig,
    code: String,
) -> Result<python::PythonResult, String> {
    let client = UnrealClient::with_port(config.port);
    python::run_script(&client, &code)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn unreal_create_blueprint(
    config: UnrealConfig,
    name: String,
    parent_class: String,
) -> Result<python::PythonResult, String> {
    let code = python::script_create_blueprint(&name, &parent_class);
    let client = UnrealClient::with_port(config.port);
    python::run_script(&client, &code)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn unreal_spawn_actor(
    config: UnrealConfig,
    class_path: String,
    location: [f64; 3],
) -> Result<python::PythonResult, String> {
    let code = python::script_spawn_actor(&class_path, location);
    let client = UnrealClient::with_port(config.port);
    python::run_script(&client, &code)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn unreal_import_asset(
    config: UnrealConfig,
    file_path: String,
    dest_path: String,
) -> Result<python::PythonResult, String> {
    let code = python::script_import_asset(&file_path, &dest_path);
    let client = UnrealClient::with_port(config.port);
    python::run_script(&client, &code)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn unreal_build_lighting(
    config: UnrealConfig,
    quality: String,
) -> Result<python::PythonResult, String> {
    let code = python::script_build_lighting(&quality);
    let client = UnrealClient::with_port(config.port);
    python::run_script(&client, &code)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn unreal_package_game(
    app: AppHandle,
    engine_root: String,
    options: PackageOptions,
) -> Result<builder::BuildResult, String> {
    builder::package_game(&PathBuf::from(&engine_root), &options, &app)
        .await
        .map_err(|e| e.to_string())
}
