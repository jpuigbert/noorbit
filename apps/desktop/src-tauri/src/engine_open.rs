//! Obre el projecte del joc al MOTOR on l'usuari hi treballarà a mà.
//!
//! NoOrbit no substitueix l'editor: el prepara (codi, assets, escenes) i
//! després li passa la pila. Aquest mòdul és el «botó» que ho fa: el mateix
//! camí per a Godot, Bevy i Unreal, exactament com es fa amb Blender.

use std::path::{Path, PathBuf};

use tauri::command;

/// Motor on què s'obre el projecte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Unreal,
    Godot,
    Bevy,
}

impl Engine {
    /// Reconeix el nom d'un motor: «godot4», «unreal5», «bevy», «rust»…
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "unreal" | "unreal5" | "ue5" | "ue" => Some(Engine::Unreal),
            "godot" | "godot4" => Some(Engine::Godot),
            "bevy" | "rust" => Some(Engine::Bevy),
            _ => None,
        }
    }

    /// Nom curt (per a missatges i registres).
    pub fn name(self) -> &'static str {
        match self {
            Engine::Unreal => "unreal",
            Engine::Godot => "godot",
            Engine::Bevy => "bevy",
        }
    }
}

/// Detecta el motor d'un projecte: Godot i Bevy primer (són projectes
/// propis), després Unreal.
pub fn detect_engine(root: &Path) -> Option<Engine> {
    if crate::godot::find_project(root).is_some() {
        return Some(Engine::Godot);
    }
    if crate::bevy::find_project(root).is_some() {
        return Some(Engine::Bevy);
    }
    if crate::engine_tools::find_uproject(root).is_some() {
        return Some(Engine::Unreal);
    }
    None
}

/// Estat d'instal·lació de Godot (per a l'assistent i la UI).
#[command]
pub async fn godot_status() -> crate::godot::GodotStatus {
    crate::godot::status().await
}

/// Arregla la instal·lació de Godot a macOS: li treu la marca de quarantina.
///
/// Sense això, macOS executa l'app baixada des d'una *translocació* temporal
/// (`/private/var/folders/…/AppTranslocation/…`). Quan aquella còpia es
/// desmunta —cosa que fa macOS sense avisar— el motor es queda sense les
/// pàgines de codi i mor amb `SIGBUS` («backing vnode was force unmounted»).
///
/// Retorna l'estat ja corregit, perquè la UI ho mostri tot seguit.
#[command]
pub async fn godot_fix_quarantine() -> Result<crate::godot::GodotStatus, String> {
    let install = crate::godot::detect_install()
        .ok_or_else(|| "no trobo cap instal·lació de Godot al sistema".to_string())?;
    if install.app_path.is_empty() {
        return Err(
            "aquesta instal·lació de Godot no és una app de macOS: no cal treure-li la \
             quarantina."
                .to_string(),
        );
    }
    let app = PathBuf::from(&install.app_path);
    if !crate::godot::is_quarantined(&app) {
        // Ja estava neta: no hi ha res a fer.
        return Ok(crate::godot::status().await);
    }
    crate::godot::remove_quarantine(&app)?;
    Ok(crate::godot::status().await)
}

/// Estat d'instal·lació de Rust/cargo (requisit de Bevy).
#[command]
pub async fn bevy_status() -> crate::bevy::BevyStatus {
    crate::bevy::status().await
}

/// Obre el projecte al motor triat (o al detectat si `engine` és buit).
///
/// Retorna un missatge per a la UI: l'usuari ha de saber què ha passat
/// realment (editor obert, Terminal obert, eina no instal·lada…).
#[command]
pub async fn open_project_in_engine(project_path: String, engine: String) -> Result<String, String> {
    let root = PathBuf::from(project_path.trim());
    if !root.is_dir() {
        return Err(format!("no trobo la carpeta del projecte: {}", root.display()));
    }
    let chosen = if engine.trim().is_empty() || engine.trim().eq_ignore_ascii_case("auto") {
        detect_engine(&root).ok_or_else(|| {
            "no puc detectar el motor del projecte: tria un motor (Godot, Bevy o \
             Unreal) i NoOrbit hi crearà el projecte."
                .to_string()
        })?
    } else {
        Engine::parse(&engine).ok_or_else(|| {
            format!("motor desconegut: «{engine}». Fes servir: godot | bevy | unreal")
        })?
    };

    match chosen {
        Engine::Godot => {
            // Si el projecte encara no existeix, es CREA: l'usuari demana
            // treballar en Godot i NoOrbit deixa el projecte a punt en lloc de
            // tornar un error que l'hauria de fer repetir a mà.
            let ensured = crate::godot::setup::ensure_project(&root)?;
            let dir = crate::godot::project_dir(&ensured.project_godot);
            let crea = ensured.created;
            crate::godot::launch_editor(&dir).map_err(|e| {
                if crea {
                    format!(
                        "{e}. S'ha creat igualment el projecte de Godot a «{}».",
                        dir.display()
                    )
                } else {
                    e
                }
            })?;
            Ok(if crea {
                format!(
                    "S'ha obert l'editor de Godot sobre «{}» (projecte creat per NoOrbit).",
                    dir.display()
                )
            } else {
                format!("S'ha obert l'editor de Godot sobre «{}».", dir.display())
            })
        }
        Engine::Bevy => {
            // Mateixa idea que amb Godot: el projecte Bevy es crea si cal.
            let manifest = crate::bevy::ensure_project(&root)?;
            let dir = crate::bevy::project_dir(&manifest);
            crate::bevy::launch_project(&dir).map_err(|e| {
                format!(
                    "{e}. El projecte Bevy ja és a «{}»: només cal el Rust instal·lat.",
                    dir.display()
                )
            })?;
            Ok(format!(
                "S'ha obert el projecte Bevy («{}») amb «cargo run» en un Terminal.",
                dir.display()
            ))
        }
        Engine::Unreal => {
            let uproject = crate::engine_tools::find_uproject(&root).ok_or(
                "no hi ha cap projecte d'Unreal (.uproject) dins el projecte",
            )?;
            let installs = crate::unreal::launcher::detect_engine_installations();
            let engine_root = installs
                .first()
                .cloned()
                .ok_or_else(|| {
                    "Unreal Engine 5 no s'ha trobat instal·lat. Descarrega'l des de \
                     https://www.unrealengine.com/download"
                        .to_string()
                })?;
            let editor = crate::unreal::launcher::editor_binary(&engine_root).ok_or_else(|| {
                format!("no s'ha trobat el binari de l'editor a «{}»", engine_root.display())
            })?;
            // Els plugins del pont (Python + Remote Control) s'activen ABANS de
            // dur el projecte: sense ells, l'editor no respon a la IA.
            let _ = crate::unreal::launcher::ensure_project_plugins(&uproject);
            crate::unreal::launcher::launch_editor(&editor, &uproject, &[])
                .await
                .map_err(|e| format!("no s'ha pogut obrir l'editor d'Unreal: {e}"))?;
            Ok(format!(
                "S'ha obert l'editor d'Unreal sobre «{}».",
                uproject.display()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconeix_els_motors() {
        assert_eq!(Engine::parse("godot4"), Some(Engine::Godot));
        assert_eq!(Engine::parse("Unreal5"), Some(Engine::Unreal));
        assert_eq!(Engine::parse("rust"), Some(Engine::Bevy));
        assert_eq!(Engine::parse("blender"), None);
    }

    #[test]
    fn detecta_godot_al_projecte() {
        let dir = std::env::temp_dir().join(format!("noorbit-open-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("project.godot"), "[application]\n").unwrap();
        assert_eq!(detect_engine(&dir), Some(Engine::Godot));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detecta_bevy_al_projecte() {
        let dir = std::env::temp_dir().join(format!("noorbit-open2-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[dependencies]\nbevy = \"0.14\"\n").unwrap();
        assert_eq!(detect_engine(&dir), Some(Engine::Bevy));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sense_motor_no_detecta_res() {
        let dir = std::env::temp_dir().join(format!("noorbit-open3-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(detect_engine(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
