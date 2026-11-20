//! Integració amb **Bevy** (motor de joc en Rust): detecció de `cargo`,
//! arrencada del projecte i estat d'instal·lació.
//!
//! Bevy no té «editor» com Unreal o Godot: el projecte és un programa Rust
//! (`Cargo.toml` + `src/main.rs`) i s'EDITA amb `cargo run` (l'IA compila i
//! executa amb les directrius `@bevy:` del `engine_tools`). Aquí es resol el
//! `cargo` de l'usuari i s'obre el projecte en un Terminal perquè l'usuari
//! hi treballi igual que ho faria amb qualsevol projecte Rust.

use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Fitxer de manifest d'un projecte Rust (Bevy hi viu).
pub const MANIFEST_FILE: &str = "Cargo.toml";

/// Binari de `cargo`: el del PATH o el de l'usuari (`~/.cargo/bin/cargo`).
///
/// Les apps llançades des del Finder (com NoOrbit) NO tenen el PATH de la
/// shell, i allà és on viu el cargo d'un Rust instal·lat amb rustup.
pub fn detect_cargo() -> Option<String> {
    if let Ok(p) = which::which("cargo") {
        return Some(p.to_string_lossy().to_string());
    }
    if let Ok(home) = std::env::var("HOME") {
        let p = Path::new(&home).join(".cargo").join("bin").join("cargo");
        if p.exists() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    None
}

/// Projecte Bevy dins d'una carpeta: un `Cargo.toml` que depèn de `bevy`.
pub fn find_project(root: &Path) -> Option<PathBuf> {
    let mut dirs = vec![root.to_path_buf()];
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                dirs.push(e.path());
            }
        }
    }
    for dir in dirs {
        let p = dir.join(MANIFEST_FILE);
        if !p.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let is_bevy = text.lines().any(|l| {
            let l = l.trim();
            // `bevy = "0.14"`, `bevy = { … }` o `[dependencies.bevy]`.
            (l.starts_with("bevy") && l.contains('=')) || l.starts_with("[dependencies.bevy")
        });
        if is_bevy {
            return Some(p);
        }
    }
    None
}

/// Carpeta del projecte (la del `Cargo.toml`).
pub fn project_dir(cargo_toml: &Path) -> PathBuf {
    cargo_toml
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Obre el projecte en un Terminal amb `cargo run` (a macOS i Linux amb
/// Terminal; a la resta, arrenca el procés directament).
pub fn launch_project(project_dir: &Path) -> Result<(), String> {
    let Some(cargo) = detect_cargo() else {
        return Err("Rust (cargo) no està instal·lat: cal per compilar Bevy.".into());
    };
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"Terminal\" to do script \"cd '{}' && '{}' run\"",
            project_dir.display(),
            cargo
        );
        return std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("no s'ha pogut obrir el Terminal: {e}"));
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::process::Command::new(cargo)
            .arg("run")
            .current_dir(project_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("no s'ha pogut arrencar el projecte Bevy: {e}"))
    }
}

/// Resultat d'importar un asset al projecte Bevy.
#[derive(Debug, Clone)]
pub struct BevyImport {
    /// Nom del fitxer dins de `assets/`.
    pub file_name: String,
    /// Fitxer copiat dins el projecte.
    pub copied: PathBuf,
}

/// Importa un asset al projecte Bevy: el copia a `assets/`, on el codi Rust
/// el llegeix amb `asset_server.load("nom.glb")`.
///
/// Bevy no té un «importador» com Unreal o Godot: el joc carrega els fitxers
/// de `assets/` en temps d'execució, així que copiar-los és tot el que cal.
pub fn import_asset(cargo_toml: &Path, src: &Path) -> Result<BevyImport, String> {
    if !src.is_file() {
        return Err(format!("no trobo l'asset: {}", src.display()));
    }
    let dir = project_dir(cargo_toml);
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| "l'asset no té nom de fitxer".to_string())?;
    let assets_dir = dir.join("assets");
    std::fs::create_dir_all(&assets_dir)
        .map_err(|e| format!("no s'ha pogut crear «{}»: {e}", assets_dir.display()))?;
    let dest = assets_dir.join(&name);
    std::fs::copy(src, &dest)
        .map_err(|e| format!("no s'ha pogut copiar l'asset a «{}»: {e}", dest.display()))?;
    Ok(BevyImport { file_name: name, copied: dest })
}

/// Estat d'instal·lació de Rust/cargo (per a l'assistent i la UI).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BevyStatus {
    /// Hi ha `cargo` instal·lat (requisit per compilar Bevy).
    pub installed: bool,
    /// Versió de Rust detectada (buida si no s'ha pogut llegir).
    pub version: String,
    /// Ruta del binari (buida si no està instal·lat).
    pub path: String,
}

/// Detecta `cargo` i en llegeix la versió (`cargo --version`).
pub async fn status() -> BevyStatus {
    match detect_cargo() {
        None => BevyStatus { installed: false, version: String::new(), path: String::new() },
        Some(path) => {
            let version = tokio::process::Command::new(&path)
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .output()
                .await
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            BevyStatus { installed: true, version, path }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn troba_el_projecte_bevy() {
        let dir = std::env::temp_dir().join(format!("noorbit-bevy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MANIFEST_FILE), "[dependencies]\nbevy = \"0.14\"\n").unwrap();
        let p = find_project(&dir).expect("hauria de trobar el projecte");
        assert_eq!(p, dir.join(MANIFEST_FILE));
        assert_eq!(project_dir(&p), dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_cargo_toml_sense_bevy_no_es_un_projecte_bevy() {
        let dir = std::env::temp_dir().join(format!("noorbit-bevy-buit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MANIFEST_FILE), "[dependencies]\nserde = \"1\"\n").unwrap();
        assert!(find_project(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// L'asset arriba a `assets/`, on Bevy el llegeix amb
    /// `asset_server.load("…")`.
    #[test]
    fn copia_l_asset_a_assets() {
        let dir = std::env::temp_dir().join(format!("noorbit-bevy-imp-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = dir.join(MANIFEST_FILE);
        std::fs::write(&manifest, "[dependencies]\nbevy = \"0.14\"\n").unwrap();
        let src = dir.join("model.glb");
        std::fs::write(&src, b"glb").unwrap();

        let r = import_asset(&manifest, &src).unwrap();
        assert_eq!(r.file_name, "model.glb");
        assert!(dir.join("assets").join("model.glb").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_asset_inexistent_dona_error() {
        let dir = std::env::temp_dir().join(format!("noorbit-bevy-imp2-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = dir.join(MANIFEST_FILE);
        std::fs::write(&manifest, "[dependencies]\nbevy = \"0.14\"\n").unwrap();
        let err = import_asset(&manifest, &dir.join("no-hi-es.glb")).unwrap_err();
        assert!(err.contains("no trobo"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}