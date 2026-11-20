//! Integració amb **Godot 4**: detecció del binari, arrencada de l'editor i
//! del projecte, i estat d'instal·lació.
//!
//! És el germà de `crate::unreal` (llançador + editor) i `crate::blender`
//! (pont + scripts): el projecte es pot OBRIR a l'editor de Godot perquè
//! l'usuari hi treballi, mentre la IA pot executar el motor en headless
//! (importar assets, llançar l'escena, veure els errors de GDScript) amb les
//! directrius `@godot:` del `engine_tools`.
//!
//! El detector és la font de veritat ÚNICA: el fa servir l'editor, el
//! llançador, l'assistent d'instal·lació i les eines de l'IA.

use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Fitxer que defineix un projecte de Godot.
pub const PROJECT_FILE: &str = "project.godot";

/// Binari de Godot: l'app de macOS, el que està al PATH o `godot4`.
///
/// Les apps llançades des del Finder tenen un PATH mínim, així que es proven
/// també les rutes habituals (Homebrew, `~/Applications`).
pub fn detect_binary() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        for base in ["/Applications", "/Applications/Godot"] {
            let dir = Path::new(base);
            if !dir.is_dir() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with("Godot") && name.ends_with(".app") {
                        let p = e.path().join("Contents").join("MacOS").join("Godot");
                        if p.exists() {
                            return Some(p.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
        for p in [
            "/opt/homebrew/bin/godot",
            "/usr/local/bin/godot",
            "/opt/homebrew/bin/godot4",
            "/usr/local/bin/godot4",
        ] {
            if Path::new(p).exists() {
                return Some(p.to_string());
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            let bin = Path::new(&home)
                .join("Applications")
                .join("Godot.app")
                .join("Contents")
                .join("MacOS")
                .join("Godot");
            if bin.exists() {
                return Some(bin.to_string_lossy().to_string());
            }
        }
    }
    for name in ["godot", "godot4", "Godot"] {
        if let Ok(p) = which::which(name) {
            return Some(p.to_string_lossy().to_string());
        }
    }
    None
}

/// Projecte de Godot dins d'una carpeta: el `project.godot` de l'arrel o
/// d'un nivell de subcarpetes.
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
        let p = dir.join(PROJECT_FILE);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Carpeta del projecte (la del `project.godot`).
pub fn project_dir(project_godot: &Path) -> PathBuf {
    project_godot
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Obre l'EDITOR de Godot sobre el projecte (l'usuari hi treballa a mà).
pub fn launch_editor(project_dir: &Path) -> Result<(), String> {
    let bin = detect_binary()
        .ok_or_else(|| "Godot no està instal·lat (no s'ha trobat el binari «godot»)".to_string())?;
    std::process::Command::new(&bin)
        .arg("--path")
        .arg(project_dir)
        .arg("--editor")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("no s'ha pogut obrir l'editor de Godot: {e}"))
}

/// Executa el projecte (el joc) amb Godot, sense editor.
pub fn launch_project(project_dir: &Path) -> Result<(), String> {
    let bin = detect_binary()
        .ok_or_else(|| "Godot no està instal·lat (no s'ha trobat el binari «godot»)".to_string())?;
    std::process::Command::new(&bin)
        .arg("--path")
        .arg(project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("no s'ha pogut executar el projecte de Godot: {e}"))
}

/// Resultat d'importar un asset al projecte de Godot.
#[derive(Debug, Clone)]
pub struct GodotImport {
    /// Ruta `res://` de l'asset dins el projecte.
    pub res_path: String,
    /// Ruta `res://` de l'escena `.tscn` creada (només per a models 3D).
    pub scene_path: Option<String>,
    /// Fitxer de l'escena `.tscn` dins el projecte (per al Diff de la Jam).
    pub scene_file: Option<PathBuf>,
    /// Fitxer copiat dins el projecte.
    pub copied: PathBuf,
}

/// Importa un asset al projecte de Godot.
///
/// 1. Copia el fitxer a `assets/` del projecte.
/// 2. Si és un `.glb`, crea l'escena `scenes/<nom>.tscn` que l'instancia
///    (el mateix patró que el crate `noorbit-blender-import`).
///
/// La reimportació en headless és un pas apart (`reimport`): així aquesta
/// funció és pura feina de fitxers i es pot provar sense el binari de Godot.
pub async fn import_asset(project_godot: &Path, src: &Path) -> Result<GodotImport, String> {
    if !src.is_file() {
        return Err(format!("no trobo l'asset: {}", src.display()));
    }
    let dir = project_dir(project_godot);
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

    let res_path = format!("res://assets/{name}");
    // Escena instanciable per als models 3D: el codi del joc la pot obrir
    // amb `load("res://scenes/…")` i posar-la a l'escena.
    let scene_path = if name.to_ascii_lowercase().ends_with(".glb") {
        let stem = Path::new(&name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| name.clone());
        let scenes_dir = dir.join("scenes");
        std::fs::create_dir_all(&scenes_dir)
            .map_err(|e| format!("no s'ha pogut crear «{}»: {e}", scenes_dir.display()))?;
        let tscn = format!(
            "[gd_scene load_steps=2 format=3]\n\n[ext_resource type=\"PackedScene\" path=\"{res_path}\" id=\"1_glb\"]\n\n[node name=\"{stem}\" type=\"Node3D\"]\n\n[node name=\"Model\" parent=\".\" instance=ExtResource(\"1_glb\")]\n"
        );
        let tscn_path = scenes_dir.join(format!("{stem}.tscn"));
        std::fs::write(&tscn_path, tscn.as_bytes())
            .map_err(|e| format!("no s'ha pogut escriure l'escena «{}»: {e}", tscn_path.display()))?;
        Some((format!("res://scenes/{stem}.tscn"), tscn_path))
    } else {
        None
    };

    Ok(GodotImport {
        res_path,
        scene_path: scene_path.as_ref().map(|(p, _)| p.clone()),
        scene_file: scene_path.map(|(_, f)| f),
        copied: dest,
    })
}

/// Reimporta els assets del projecte en headless (`--import`): Godot crea
/// els fitxers `.import` i les textures. Sense aquest pas els models
/// acabats d'importar no es poden fer servir des del codi.
///
/// Si el binari de Godot no està instal·lat no és un error: l'editor ho farà
/// quan l'usuari obri el projecte.
pub async fn reimport(project_godot: &Path) -> Result<(), String> {
    let Some(bin) = detect_binary() else {
        return Ok(());
    };
    let dir = project_dir(project_godot);
    let out = tokio::process::Command::new(bin)
        .arg("--headless")
        .arg("--path")
        .arg(&dir)
        .arg("--import")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("no s'ha pogut executar Godot en headless: {e}"))?;
    if out.status.code() != Some(0) {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = crate::engine_tools::clip(err.trim(), 300);
        return Err(format!("Godot ha fallat en reimportar els assets: {err}"));
    }
    Ok(())
}

/// Estat d'instal·lació de Godot (per a l'assistent i la UI).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GodotStatus {
    pub installed: bool,
    /// Versió detectada (buida si no s'ha pogut llegir).
    pub version: String,
    /// Ruta del binari (buida si no està instal·lat).
    pub path: String,
}

/// Detecta Godot i en llegeix la versió (`godot --version`).
pub async fn status() -> GodotStatus {
    match detect_binary() {
        None => GodotStatus { installed: false, version: String::new(), path: String::new() },
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
            GodotStatus { installed: true, version, path }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn troba_el_projecte_a_l_arrel() {
        let dir = std::env::temp_dir().join(format!("noorbit-godot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(PROJECT_FILE), "").unwrap();
        let p = find_project(&dir).expect("hauria de trobar el projecte");
        assert_eq!(p, dir.join(PROJECT_FILE));
        assert_eq!(project_dir(&p), dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sense_project_godot_no_hi_ha_projecte() {
        let dir = std::env::temp_dir().join(format!("noorbit-godot-buit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(find_project(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// El flux Blender → Godot: el model arriba a `assets/` i hi ha una
    /// escena instanciable a `scenes/` que el codi del joc pot obrir.
    #[tokio::test]
    async fn importa_un_glb_i_crea_l_escena() {
        let dir = std::env::temp_dir().join(format!("noorbit-godot-imp-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let project = dir.join(PROJECT_FILE);
        std::fs::write(&project, "[application]\n").unwrap();
        let src = dir.join("model.glb");
        std::fs::write(&src, b"glb").unwrap();

        let r = import_asset(&project, &src).await.unwrap();
        assert_eq!(r.res_path, "res://assets/model.glb");
        assert_eq!(r.scene_path.as_deref(), Some("res://scenes/model.tscn"));
        assert!(dir.join("assets").join("model.glb").is_file());
        let tscn = dir.join("scenes").join("model.tscn");
        assert!(tscn.is_file());
        let text = std::fs::read_to_string(&tscn).unwrap();
        assert!(text.contains("res://assets/model.glb"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Un asset que no és model 3D no crea escena: només es copia.
    #[tokio::test]
    async fn un_asset_plat_no_crea_escena() {
        let dir = std::env::temp_dir().join(format!("noorbit-godot-imp2-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let project = dir.join(PROJECT_FILE);
        std::fs::write(&project, "[application]\n").unwrap();
        let src = dir.join("musica.ogg");
        std::fs::write(&src, b"ogg").unwrap();

        let r = import_asset(&project, &src).await.unwrap();
        assert_eq!(r.res_path, "res://assets/musica.ogg");
        assert!(r.scene_path.is_none());
        assert!(!dir.join("scenes").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
