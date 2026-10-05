//! Detecció de Xcode / iOS Simulator i control via `xcrun simctl`.
//!
//! Les simulacions iOS només funcionen a macOS (requisit del propi Xcode).
//! En altres sistemes operatius, este mòdul retorna `installed=false` amb un
//! avís llegible. Les operacions que fem:
//!
//! - `xcrun simctl list -j devices` → llistat JSON de dispositius i runtimes
//! - `xcrun simctl boot <udid>`      → arranca (background) un dispositiu
//! - `xcrun simctl shutdown <udid>`  → apaga
//! - `open -a Simulator`             → mostra la finestra de Simulator.app
//! - `xcrun simctl io <udid> screenshot <file.png>` → captura PNG
//!
//! NoOrbit NO inclou cap binari propietari d'Apple. Només invoca el que
//! Xcode haja instal·lat.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct IosStatus {
    pub installed: bool,
    /// `xcrun --find simctl` (ruta del binari) si es troba.
    pub simctl: Option<String>,
    /// `xcode-select -p` (ruta del developer dir).
    pub developer_dir: Option<String>,
    /// Ruta de Simulator.app (si es troba).
    pub simulator_app: Option<String>,
    /// Cert només a macOS (iOS Simulator no existeix en altres SO).
    pub supported: bool,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IosDevice {
    pub udid: String,
    pub name: String,
    pub state: String,
    pub is_available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct IosRuntimeGroup {
    pub runtime: String,
    /// Nom llegible del runtime (p. ex. «iOS 17.5»).
    pub build: String,
    pub devices: Vec<IosDevice>,
}

fn which(bin: &str) -> Option<PathBuf> {
    which::which(bin).ok()
}

pub fn detect() -> IosStatus {
    let supported = cfg!(target_os = "macos");
    if !supported {
        return IosStatus {
            installed: false,
            simctl: None,
            developer_dir: None,
            simulator_app: None,
            supported: false,
            hint: Some(
                "iOS Simulator només està disponible a macOS (requereix Xcode).".to_string(),
            ),
        };
    }
    let simctl = Command::new("xcrun")
        .args(["--find", "simctl"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let developer_dir = Command::new("xcode-select")
        .arg("-p")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    // Simulator.app sol ser dins del developer dir.
    let simulator_app = developer_dir.as_ref().and_then(|d| {
        let p = PathBuf::from(d)
            .join("Applications")
            .join("Simulator.app");
        p.exists().then(|| p.to_string_lossy().into_owned())
    });
    let installed = simctl.is_some();
    let hint = if installed {
        None
    } else {
        Some(
            "Xcode no està instal·lat o `xcrun` no respon. Instal·la Xcode des del Mac App Store i executa `xcode-select --install`.".to_string(),
        )
    };
    IosStatus {
        installed,
        simctl,
        developer_dir,
        simulator_app,
        supported: true,
        hint,
    }
}

/// Llista els dispositius iOS disponibles agrupats per runtime.
pub fn list_devices() -> anyhow::Result<Vec<IosRuntimeGroup>> {
    let status = detect();
    if !status.installed {
        return Err(anyhow::anyhow!(
            "{}",
            status.hint.unwrap_or_else(|| "Xcode no disponible".into())
        ));
    }
    let out = Command::new("xcrun")
        .args(["simctl", "list", "-j", "devices"])
        .output()?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "xcrun simctl list ha fallat: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let parsed: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("JSON invàlid: {}", e))?;
    let obj = parsed
        .get("devices")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let mut groups: Vec<IosRuntimeGroup> = Vec::new();
    for (runtime, devs) in obj {
        // Filtrem els runtimes iOS (tvOS/watchOS no són telèfon).
        if !runtime.contains("iOS-") {
            continue;
        }
        let mut list: Vec<IosDevice> = Vec::new();
        if let Some(arr) = devs.as_array() {
            for d in arr {
                let is_avail = d
                    .get("isAvailable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                if !is_avail {
                    continue;
                }
                list.push(IosDevice {
                    udid: d.get("udid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    name: d.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    state: d.get("state").and_then(|v| v.as_str()).unwrap_or("Shutdown").to_string(),
                    is_available: true,
                });
            }
        }
        if list.is_empty() {
            continue;
        }
        // Ordre: els Booted primer.
        list.sort_by(|a, b| match (a.state.as_str(), b.state.as_str()) {
            ("Booted", "Booted") => a.name.cmp(&b.name),
            ("Booted", _) => std::cmp::Ordering::Less,
            (_, "Booted") => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });
        let build = runtime.rsplit('-').next().unwrap_or("").replace('_', ".");
        groups.push(IosRuntimeGroup {
            runtime: runtime.clone(),
            build,
            devices: list,
        });
    }
    groups.sort_by(|a, b| b.runtime.cmp(&a.runtime));
    Ok(groups)
}

pub fn boot(udid: &str) -> anyhow::Result<()> {
    let out = Command::new("xcrun")
        .args(["simctl", "boot", udid])
        .output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // «Unable to boot device in current state: Booted» és tolerable.
        if err.contains("current state: Booted") {
            return Ok(());
        }
        return Err(anyhow::anyhow!("simctl boot: {}", err));
    }
    Ok(())
}

pub fn shutdown(udid: &str) -> anyhow::Result<()> {
    let out = Command::new("xcrun")
        .args(["simctl", "shutdown", udid])
        .output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if err.contains("current state: Shutdown") {
            return Ok(());
        }
        return Err(anyhow::anyhow!("simctl shutdown: {}", err));
    }
    Ok(())
}

/// Obri Simulator.app i el porta al davant mostrant el dispositiu indicat.
pub fn open_simulator(udid: &str) -> anyhow::Result<()> {
    let status = detect();
    // Primer: assegurar-boot.
    let _ = boot(udid);
    // Després: obrim l'app amb el device concret. `simctl boot` sol deixar
    // la UI tancada si Simulator.app no s'ha obert mai: forçar.
    let app = status
        .simulator_app
        .clone()
        .unwrap_or_else(|| "/Applications/Xcode.app/Contents/Developer/Applications/Simulator.app".to_string());
    let out = Command::new("open")
        .args(["-a", &app, "--args", "-CurrentDeviceUDID", udid])
        .output()?;
    if !out.status.success() {
        // Fallback: obrir sense args.
        let _ = Command::new("open").arg("-a").arg("Simulator").output();
    }
    Ok(())
}

/// Retorna la ruta temporal on s'ha desat el PNG.
pub fn screenshot(udid: &str) -> anyhow::Result<String> {
    let dir = std::env::temp_dir().join(format!("noorbit-ios-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("shot-{}.png", uuid::Uuid::new_v4()));
    let out = Command::new("xcrun")
        .args(["simctl", "io", udid, "screenshot", path.to_str().unwrap()])
        .output()?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "simctl io screenshot: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let bytes = std::fs::read(&path)?;
    let _ = std::fs::remove_file(&path);
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    Ok(format!("data:image/png;base64,{}", B64.encode(bytes)))
}

/// Dispositiu «Booted» (primer, si n'hi ha més d'un).
pub fn booted_udid() -> Option<String> {
    list_devices()
        .ok()?
        .into_iter()
        .flat_map(|g| g.devices)
        .find(|d| d.state == "Booted")
        .map(|d| d.udid)
}

/// Utilitat: detecta si una eina externa (per exemple `adb`) està al PATH
/// fora de l'SDK. S'usa com a reserva quan l'usuari l'ha configurat a mà.
#[allow(dead_code)]
fn tool_in_path(name: &str) -> Option<PathBuf> {
    which(name)
}
