//! Assistent de **Bevy**: les preguntes que no es poden endevinar.
//!
//! Treballar en Bevy vol dir dues coses que l'usuari ha de decidir:
//!
//!  1. **On viu el Rust** (el «cargo»): la instal·lació per defecte és
//!     `~/.cargo`, però l'usuari pot triar una altra carpeta (un disc
//!     extern, per exemple). NoOrbit li fa la pregunta, obre l'instal·lador
//!     oficial (rustup) en un Terminal TRANSPARENT — l'usuari veu exactament
//!     què s'executa — i després DESA la carpeta triada perquè la detecció
//!     la trobi sempre, encara que el PATH de l'app no la tingui.
//!  2. **On treballar-hi**: la carpeta i el nom del projecte Bevy nou. El
//!     projecte es crea amb el `Cargo.toml`, el `src/main.rs` i el `.gitignore`
//!     correctes, i tot seguit es VERIFICA amb el mateix detector que fa
//!     servir l'IA (el `Cargo.toml` ha de dir que depèn de `bevy`).

use std::path::{Path, PathBuf};

use tauri::{command, State};

use crate::AppState;

/// Projecte Bevy detectat (o creat) i estat del «cargo».
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BevyProject {
    /// Carpeta del projecte Bevy.
    pub dir: String,
    /// Ruta del `Cargo.toml`.
    pub manifest: String,
    /// S'ha detectat com a projecte Bevy (el manifest depèn de `bevy`).
    pub detected: bool,
    /// Versió de Bevy declarada al manifest (buida si no n'hi ha).
    pub bevy_version: String,
    /// Hi ha `cargo` instal·lat?
    pub cargo_installed: bool,
    /// Ruta del `cargo` detectat.
    pub cargo_path: String,
}

/// Tot el que l'assistent ha de saber per fer les preguntes.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BevySetupInfo {
    /// Estat del Rust/cargo.
    pub status: crate::bevy::BevyStatus,
    /// Carpeta on es creen els projectes per defecte (la de l'usuari).
    pub default_projects_dir: String,
    /// Projecte Bevy detectat dins del workspace (si n'hi ha).
    pub workspace_project: Option<BevyProject>,
}

/// Detecta un projecte Bevy dins d'una carpeta.
fn detect_in(root: &Path) -> BevyProject {
    let manifest = crate::bevy::find_project(root);
    let (detected, bevy_version) = match &manifest {
        Some(m) => {
            let text = std::fs::read_to_string(m).unwrap_or_default();
            (
                true,
                crate::bevy::manifest_bevy_version(&text).unwrap_or_default(),
            )
        }
        None => (false, String::new()),
    };
    let cargo = crate::bevy::detect_cargo();
    let dir = manifest
        .as_deref()
        .map(crate::bevy::project_dir)
        .unwrap_or_else(|| root.to_path_buf());
    BevyProject {
        dir: dir.display().to_string(),
        manifest: manifest
            .map(|m| m.display().to_string())
            .unwrap_or_default(),
        detected,
        bevy_version,
        cargo_installed: cargo.is_some(),
        cargo_path: cargo.unwrap_or_default(),
    }
}

/// Detecta el projecte Bevy dins d'una carpeta i diu si el «cargo» hi ha.
///
/// Serveix per VERIFICAR (a la interfície i als tests) que la detecció és
/// real: no es mira només el nom del fitxer, sinó que el manifest depengui
/// de debò de `bevy`.
#[command]
pub async fn bevy_detect_project(path: String) -> Result<BevyProject, String> {
    let root = PathBuf::from(path.trim());
    if !root.is_dir() {
        return Err(format!("no trobo la carpeta: {}", root.display()));
    }
    Ok(detect_in(&root))
}

/// Estat complet per a l'assistent: Rust, carpeta de projectes per defecte i
/// projecte Bevy del workspace actual.
#[command]
pub async fn bevy_setup_info(workspace: String) -> Result<BevySetupInfo, String> {
    let status = crate::bevy::status().await;
    let default_projects_dir = std::env::var("HOME").unwrap_or_default();
    let workspace_project = {
        let root = workspace.trim();
        if root.is_empty() {
            None
        } else {
            let p = PathBuf::from(root);
            if p.is_dir() {
                let found = detect_in(&p);
                if found.detected {
                    Some(found)
                } else {
                    None
                }
            } else {
                None
            }
        }
    };
    Ok(BevySetupInfo {
        status,
        default_projects_dir,
        workspace_project,
    })
}

/// Contingut del `Cargo.toml` i del `src/main.rs` d'un projecte Bevy nou.
///
/// La plantilla és la mateixa que usa la IA quan crea un joc, i està escrita
/// amb l'API real de **Bevy 0.14** (verificada compilant el projecte):
/// `Assets<Mesh>` i `Assets<StandardMaterial>` (a la 0.14 `Material` és un
/// *trait*, no el tipus de l'actiu) i `Cuboid` (a la 0.14 no hi ha `Cube`).
pub fn project_template(name: &str) -> (String, String) {
    let manifest = format!(
        "# {name} — projecte Bevy creat per NoOrbit.\n\n[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nbevy = \"{version}\"\n",
        name = name,
        version = crate::bevy::DEFAULT_BEVY_VERSION
    );
    let main_rs = BEVY_MAIN_RS.replace("{name}", name);
    (manifest, main_rs)
}

/// Plantilla del `src/main.rs` d'un projecte Bevy (API de Bevy 0.14).
///
/// Es basa en l'exemple oficial `3d_scene` de Bevy 0.14: el cub és `Cuboid`
/// (no `Cube`), el material és `StandardMaterial` (no `Material`, que és un
/// trait) i el color es crea amb `Color::srgb` (no `Color::rgb`, que està
/// obsolet a la 0.14).
const BEVY_MAIN_RS: &str = r#"//! {name} — joc Bevy creat per NoOrbit.
//!
//! Compila'l i executa'l amb `cargo run` (o des de NoOrbit: el botó
//! «Obre al motor»).

use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // El cub blau.
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        material: materials.add(Color::srgb(0.3, 0.6, 0.9)),
        transform: Transform::from_xyz(0.0, 0.5, 0.0),
        ..default()
    });
    // La llum.
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 1500.0,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });
    // La càmera, mirant al cub.
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });
}
"#;

/// Crea un projecte Bevy nou i el VERIFICA amb el detector.
#[command]
pub async fn bevy_create_project(parent: String, name: String) -> Result<BevyProject, String> {
    let parent = PathBuf::from(parent.trim());
    if parent.as_os_str().is_empty() {
        return Err("cal indicar la carpeta on crear el projecte.".into());
    }
    let name = name.trim().to_string();
    // El nom es valida ABANS de tocar el disc: una petició amb un nom
    // invàlid no ha de deixar cap carpeta al sistema de fitxers.
    if !crate::bevy::valid_project_name(&name) {
        return Err(
            "el nom del projecte ha de ser com «meu-joc» (lletres, números, «-» i «_»)."
                .into(),
        );
    }
    std::fs::create_dir_all(&parent)
        .map_err(|e| format!("no s'ha pogut crear «{}»: {e}", parent.display()))?;
    let dir = parent.join(&name);
    if dir.exists() {
        return Err(format!("ja existeix la carpeta «{}».", dir.display()));
    }
    std::fs::create_dir_all(dir.join("src"))
        .map_err(|e| format!("no s'ha pogut crear «{}»: {e}", dir.display()))?;

    let (manifest, main_rs) = project_template(&name);
    std::fs::write(dir.join(crate::bevy::MANIFEST_FILE), manifest.as_bytes())
        .map_err(|e| format!("no s'ha pogut escriure el Cargo.toml: {e}"))?;
    std::fs::write(dir.join("src").join("main.rs"), main_rs.as_bytes())
        .map_err(|e| format!("no s'ha pogut escriure el main.rs: {e}"))?;

    // El «target/» de Rust pot/dec de tenir gigues: mai al Git.
    std::fs::write(dir.join(".gitignore"), b"/target\n")
        .map_err(|e| format!("no s'ha pogut escriure el .gitignore: {e}"))?;

    let found = detect_in(&dir);
    if !found.detected {
        return Err("el projecte s'ha creat però la detecció no el reconeix.".into());
    }
    Ok(found)
}

/// Comanda SHELL escapada entre cometes simples.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Obre el Terminal del sistema amb una ordre i en comprova el resultat.
///
/// A macOS l'«osascript» s'espera: si el permís d'automatització està
/// denegat, l'error torna a l'usuari (no un botó que no fa res).
fn open_terminal(command_line: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let script = format!("tell application \"Terminal\" to do script \"{command_line}\"");
        let out = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| format!("no s'ha pogut obrir el Terminal: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(if err.is_empty() {
                "no s'ha pogut obrir el Terminal: macOS pot demanar permís \
                 d'automatització a NoOrbit (Ajustos del Sistema ▸ Privacitat ▸ Automatització)."
                    .to_string()
            } else {
                format!("no s'ha pogut obrir el Terminal: {err}")
            });
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        for term in ["gnome-terminal", "konsole", "xfce4-terminal", "x-terminal-emulator"] {
            if which::which(term).is_ok() {
                let _ = std::process::Command::new(term).arg("-e").arg(command_line).spawn();
                return Ok(());
            }
        }
        Err(format!(
            "no s'ha trobat cap Terminal. Executa a mà:\n{command_line}"
        ))
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", "cmd"])
            .spawn();
        Err(format!(
            "executa a mà l'instal·lador de Rust:\n{command_line}"
        ))
    }
}

/// Instal·la Rust (rustup) a la carpeta que l'usuari ha triat.
///
/// La instal·lació és la oficial de rustup, en un Terminal transparent: es veu
/// què s'executa i l'usuari pot aturar-ho. Les carpetes triades es desen a la
/// configuració perquè la detecció del «cargo» les trobi sempre.
#[command]
pub async fn bevy_install_rust(
    state: State<'_, AppState>,
    cargo_home: String,
    rustup_home: String,
) -> Result<String, String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let cargo_home = expand_home(cargo_home.trim(), &home);
    let rustup_home = expand_home(rustup_home.trim(), &home);
    let cargo_home = if cargo_home.is_empty() {
        PathBuf::from(&home).join(".cargo")
    } else {
        PathBuf::from(cargo_home)
    };
    let rustup_home = if rustup_home.is_empty() {
        PathBuf::from(&home).join(".rustup")
    } else {
        PathBuf::from(rustup_home)
    };
    if !cargo_home.is_absolute() || !rustup_home.is_absolute() {
        return Err("cal una ruta completa (per exemple: /Users/…/.cargo).".into());
    }
    std::fs::create_dir_all(&cargo_home)
        .map_err(|e| format!("no s'ha pogut crear «{}»: {e}", cargo_home.display()))?;
    std::fs::create_dir_all(&rustup_home)
        .map_err(|e| format!("no s'ha pogut crear «{}»: {e}", rustup_home.display()))?;

    // Desa la tria: la detecció la buscarà primer, encara que el PATH de
    // l'aplicació no la tingui (una app llançada des del Finder no el té).
    {
        let mut cfg = state.config.write().await;
        cfg.bevy.cargo_home = cargo_home.display().to_string();
        cfg.bevy.rustup_home = rustup_home.display().to_string();
        let saved = cfg.save();
        if let Err(e) = saved {
            return Err(format!("no s'ha pogut desar la configuració: {e}"));
        }
    }

    let line = format!(
        "export CARGO_HOME={}; export RUSTUP_HOME={}; curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable",
        shell_quote(&cargo_home.display().to_string()),
        shell_quote(&rustup_home.display().to_string())
    );
    #[cfg(target_os = "windows")]
    {
        let _ = crate::commands::system::open_externally("https://rustup.rs/".into()).await;
        return Err(format!(
            "a Windows l'instal·lació s'ha d'obrir a mà:\n{line}"
        ));
    }
    #[cfg(not(target_os = "windows"))]
    {
        open_terminal(&line)?;
        Ok(format!(
            "S'ha obert el Terminal per instal·lar Rust a «{}». Quan acabi, \
             prem «Revisa» i NoOrbit detectarà el «cargo» sol.",
            cargo_home.display()
        ))
    }
}

/// Expandeix «~» al camí de l'usuari.
fn expand_home(path: &str, home: &str) -> String {
    if path == "~" {
        return home.to_string();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if home.is_empty() {
            return path.to_string();
        }
        return PathBuf::from(home).join(rest).display().to_string();
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El projecte creat es detecta com a projecte Bevy (el manifest depèn
    /// de `bevy` de debò, no pas pel nom del fitxer).
    #[tokio::test]
    async fn crea_i_detecta_un_projecte_bevy() {
        let dir = std::env::temp_dir().join(format!("noorbit-bevy-new-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = bevy_create_project(dir.display().to_string(), "meu-joc".into())
            .await
            .unwrap();
        assert!(p.detected, "hauria de detectar el projecte");
        assert_eq!(p.bevy_version, crate::bevy::DEFAULT_BEVY_VERSION);
        assert!(Path::new(&p.manifest).is_file());
        assert!(dir.join("meu-joc").join("src").join("main.rs").is_file());
        assert!(dir.join("meu-joc").join(".gitignore").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Un nom invàlid es rebutja abans de tocar el disc.
    #[tokio::test]
    async fn rebutja_un_nom_invalide() {
        let dir = std::env::temp_dir().join(format!("noorbit-bevy-bad-{}", uuid::Uuid::new_v4()));
        let err = bevy_create_project(dir.display().to_string(), "meu joc!".into())
            .await
            .unwrap_err();
        assert!(err.contains("nom del projecte"), "{err}");
        assert!(!dir.exists());
    }

    /// Un projecte que NO depèn de `bevy` no es detecta com a projecte Bevy.
    #[tokio::test]
    async fn no_detecta_un_cargo_toml_sense_bevy() {
        let dir = std::env::temp_dir().join(format!("noorbit-bevy-nobevy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[dependencies]\nserde = \"1\"\n").unwrap();
        let p = bevy_detect_project(dir.display().to_string()).await.unwrap();
        assert!(!p.detected);
        assert_eq!(p.bevy_version, "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// «bevy_ecs» NO fa pensar que és un projecte de Bevy.
    #[test]
    fn bevy_ecs_no_es_un_projecte_bevy() {
        assert!(!crate::bevy::manifest_has_bevy(
            "[dependencies]\nbevy_ecs = \"0.14\"\n"
        ));
        assert!(crate::bevy::manifest_has_bevy("[dependencies]\nbevy = \"0.14\"\n"));
        assert!(crate::bevy::manifest_has_bevy("[dependencies.bevy]\nversion = \"0.14\"\n"));
        assert!(!crate::bevy::manifest_has_bevy("[package]\nname = \"bevy\"\n"));
    }

    #[test]
    fn expandeix_la_teulada() {
        assert_eq!(expand_home("~/x", "/Users/puigbert"), "/Users/puigbert/x");
        assert_eq!(expand_home("/abs", "/Users/puigbert"), "/abs");
        assert_eq!(expand_home("", "/Users/puigbert"), "");
    }

    /// La plantilla ha d'utilitzar l'API real de Bevy 0.14 (verificada
    /// compilant el projecte generat): a la 0.14 `Material` és un *trait*
    /// (l'actiu és `StandardMaterial`), el cub és `Cuboid` (no hi ha `Cube`) i
    /// el color es fa amb `Color::srgb` (`Color::rgb` està obsolet). Les API
    /// antigues NO compilen, i el projecte que crea NoOrbit ha de compilar.
    #[test]
    fn la_plantilla_usa_l_api_de_bevy_014() {
        let (_toml, main_rs) = project_template("meu-joc");
        assert!(main_rs.contains("Assets<Mesh>"), "{main_rs}");
        assert!(main_rs.contains("Assets<StandardMaterial>"), "{main_rs}");
        assert!(main_rs.contains("Cuboid::new("), "{main_rs}");
        assert!(main_rs.contains("Color::srgb("), "{main_rs}");
        // Les API obsoletes no hi han de ser.
        assert!(!main_rs.contains("Assets<Material>"), "{main_rs}");
        assert!(!main_rs.contains("Cube::default()"), "{main_rs}");
        assert!(!main_rs.contains("Color::rgb("), "{main_rs}");
    }
}
