//! On es construeix el JOC de la Jam.
//!
//! La Jam ha de fer una cosa ben clara: construir **EL JOC** que l'usuari ha
//! demanat. Això comporta dues coses que abans no passaven i que eren la
//! causa de la queixa «li dic que faci un joc i no fa un joc»:
//!
//! 1. **El projecte ha de ser un projecte de joc**: un `project.godot` amb la
//!    seva escena principal, un `Cargo.toml` de Bevy, les carpetes d'assets…
//!    Sense això la Jam escriu fitxers solts i el resultat no és res que es
//!    pugui obrir ni executar.
//! 2. **El joc ha de viure al seu lloc**: si el workspace obert no és un
//!    projecte del motor triat, el joc es crea dins d'una carpeta pròpia
//!    (`prototypes/<nom>/`) i la Jam hi treballa amb el seu PROPI repo git.
//!    Amb el repo del pare, el `git add -A` i el `git clean -fd` del
//!    checkpointer tocaven el projecte de l'usuari sencer.
//!
//! La creació del projecte és una RUTA D'ACCIÓ (l'usuari ha demanat un joc),
//! mai una detecció: això és exactament el mateix criteri que segueixen
//! `godot::setup::ensure_project` i `bevy::ensure_project`.

use std::path::{Path, PathBuf};

use noorbit_planner::Engine;

/// Carpeta (dins del workspace) on viuen els jocs creats per la Jam.
const CARPETA_JOCS: &str = "prototypes";

/// La carpeta base ja és un projecte del motor triat?
///
/// Si ho és, la Jam hi treballa DIRECTAMENT: és el projecte de l'usuari (un
/// joc de Godot, un de Bevy…) i la Jam hi afegeix el que li han demanat.
pub fn es_projecte_del_motor(base: &Path, engine: Engine) -> bool {
    match engine {
        Engine::Godot4 => crate::godot::find_project(base).is_some(),
        Engine::Bevy => crate::bevy::find_project(base).is_some(),
        Engine::Unreal5 => te_uproject(base),
        Engine::Unity => base.join("Assets").is_dir(),
        // Un joc web es reconeix per la seva pàgina d'inici: només `index.html`.
        // (Un `package.json` sol no és prou: la mateixa arrel pot ser un
        // monorepo o l'aplicació de l'usuari, i escriure-hi el joc a dins
        // barrejaria les coses.)
        Engine::Custom => base.join("index.html").is_file(),
        Engine::Blender => false,
    }
}

/// Hi ha algun projecte d'Unreal (`.uproject`) a l'arrel?
fn te_uproject(base: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(base) else {
        return false;
    };
    rd.flatten().any(|e| {
        e.path().is_file()
            && e.path()
                .extension()
                .map(|x| x.eq_ignore_ascii_case("uproject"))
                .unwrap_or(false)
    })
}

/// Nom de la carpeta del joc, derivat del brief.
///
/// S'eviten els verbs d'ordre habituals («fes», «crea»…) i es queden les
/// primeres paraules significatives: «Fes un joc de nauxes perilloses» →
/// `joc-de-nauxes-perilloses`.
pub fn nom_del_joc(brief: &str) -> String {
    const ORDERS: &[&str] = &[
        "fes", "fes-me", "fem", "fer", "crea", "crea-me", "creem", "crear", "munta",
        "muntar", "vull", "vulgria", "em", "necessito", "necessit", "dona", "dona-me",
        "build", "make", "create", "generate", "write", "develop",
    ];
    /// Articles i determinants inicials: no porten informació al nom.
    const ARTICLES: &[&str] = &["un", "una", "uns", "unes", "el", "la", "els", "les"];
    let paraules: Vec<String> = brief
        .split_whitespace()
        .map(|p| {
            p.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|p| !p.is_empty())
        .take(16)
        .collect();
    let mut out: Vec<String> = Vec::new();
    for p in paraules {
        if out.is_empty() && (ORDERS.contains(&p.as_str()) || ARTICLES.contains(&p.as_str())) {
            continue;
        }
        out.push(p);
        if out.len() >= 4 {
            break;
        }
    }
    let slug: String = out.join("-").chars().take(48).collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "joc".to_string()
    } else {
        slug
    }
}

/// Carpeta on es construirà el joc.
///
/// - Si `base` ja és un projecte del motor → `base` (el projecte de l'usuari).
/// - Altrament → `base/prototypes/<nom del joc>/`: el joc té la seva carpeta,
///   el projecte de l'usuari no s'hi barreja i el resultat és un joc que es
///   pot obrir, executar i compartir.
pub fn carpeta_del_joc(base: &Path, engine: Engine, brief: &str) -> PathBuf {
    if es_projecte_del_motor(base, engine) {
        return base.to_path_buf();
    }
    base.join(CARPETA_JOCS).join(nom_del_joc(brief))
}

/// Prepara l'estructura del projecte de joc dins de `root`.
///
/// Retorna la CARPETA on viu el joc: pot ser `root` mateix o una subcarpeta
/// (el projecte de Bevy va a dins si `root` ja té un `Cargo.toml` que no és
/// de Bevy; el de Godot, si `root` ja té fitxers del projecte).
pub fn prepara_projecte(root: &Path, engine: Engine) -> Result<PathBuf, String> {
    match engine {
        Engine::Godot4 => {
            let p = crate::godot::setup::ensure_project(root)?;
            let dir = p
                .project_godot
                .parent()
                .map(|d| d.to_path_buf())
                .unwrap_or_else(|| root.to_path_buf());
            Ok(dir)
        }
        Engine::Bevy => {
            let manifest = crate::bevy::ensure_project(root)?;
            let dir = manifest
                .parent()
                .map(|d| d.to_path_buf())
                .unwrap_or_else(|| root.to_path_buf());
            Ok(dir)
        }
        Engine::Unity => {
            for sub in ["Assets/Scripts", "Assets/Scenes", "Assets/Prefabs"] {
                std::fs::create_dir_all(root.join(sub))
                    .map_err(|e| format!("no s'ha pogut crear «{sub}»: {e}"))?;
            }
            Ok(root.to_path_buf())
        }
        Engine::Custom => {
            std::fs::create_dir_all(root)
                .map_err(|e| format!("no s'ha pogut crear la carpeta del joc: {e}"))?;
            Ok(root.to_path_buf())
        }
        Engine::Unreal5 | Engine::Blender => {
            std::fs::create_dir_all(root)
                .map_err(|e| format!("no s'ha pogut crear la carpeta del joc: {e}"))?;
            Ok(root.to_path_buf())
        }
    }
}

/// Descripció OPERATIVA del motor triat: què ha d'escriure la IA i què ja
/// existeix al projecte.
///
/// El worker només detectava el motor PEL DISCO (`.uproject`, `project.godot`,
/// `Cargo.toml`…). En un projecte nou —el cas normal d'una Jam— no hi ha res
/// a detectar i la IA es queda sense saber si ha d'escriure GDScript, Rust o
/// C#: aleshores fa «una altra cosa». Aquesta funció ho resol sense
/// dependre de què hi hagi a l'arrel.
pub fn engine_spec(engine: Engine) -> String {
    match engine {
        Engine::Godot4 => "MOTOR: Godot 4 (GDScript). Escriu scripts «.gd» i escenes «.tscn». \
            El projecte ja té «project.godot», «main.tscn» i «main.gd»: NO els tornis a \
            escriure, OBRE-los amb «@edit:» si cal modificar-los. L'escena principal és \
            «res://main.tscn» i el script que l'ha d'utilitzar és «res://main.gd»."
            .into(),
        Engine::Bevy => "MOTOR: Bevy 0.14 (Rust amb ECS). Escriu mòduls Rust a «src/*.rs» i \
            registra'ls al «Cargo.toml» i al «src/main.rs» («mod» + «use»). El projecte ja \
            té «Cargo.toml» i «src/main.rs»: NO els tornis a escriure, modifica'ls amb \
            «@edit:». La finestra i el bucle del joc es construeixen amb els plugins de Bevy."
            .into(),
        Engine::Unreal5 => "MOTOR: Unreal Engine 5 (C++/Blueprints). Escriu codi C++ (».cpp»/».h») \
            i, si cal, assets dins de «Content/». El projecte ja té el «.uproject»: NO el tornis \
            a escriure."
            .into(),
        Engine::Unity => "MOTOR: Unity (C#). Escriu scripts «.cs» dins de «Assets/Scripts/» i \
            escenes «.unity» dins de «Assets/Scenes/». Cada script ha de tenir el seu component \
            (MonoBehaviour) i la escena ha de tenir la referència al script."
            .into(),
        Engine::Custom => "MOTOR: joc de navegador (HTML + JavaScript). Tria UNA pila i \
            fes-la servir de manera consistent: «index.html» + «src/main.js» (sense \
            dependències) o amb Vite («package.json» + «index.html» + «src/main.ts»). Sense \
            gràfics externs: dibuixa amb «canvas» 2D. El joc ha de tenir entrada (teclat/ratolí) \
            i un bucle de joc («requestAnimationFrame»)."
            .into(),
        Engine::Blender => "MOTOR: Blender (Python amb «bpy»). Escriu scripts «.py» i, si cal, \
            escenes «.blend»; usa les directrius «@blender:» per executar-los."
            .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_nom_del_joc_ignora_el_verb_d_ordre() {
        assert_eq!(nom_del_joc("Fes un joc de nauxes"), "joc-de-nauxes");
        assert_eq!(nom_del_joc("crea un joc de cotxes"), "joc-de-cotxes");
    }

    #[test]
    fn el_nom_del_joc_torna_a_joc_si_no_hi_ha_res() {
        assert_eq!(nom_del_joc("   "), "joc");
        assert_eq!(nom_del_joc("!!!"), "joc");
    }

    #[test]
    fn el_nom_del_joc_te_guions_i_no_espais() {
        let n = nom_del_joc("Fes un joc de vaixells perillosos");
        assert!(!n.contains(' '), "{n}");
        assert!(n.contains('-'), "{n}");
    }

    /// La carpeta del joc és la del propi projecte quan el workspace ja és un
    /// projecte del motor: la Jam hi treballa directament (no crea cap
    /// carpeta nova).
    #[test]
    fn si_el_base_ja_es_un_projecte_del_motor_s_usa_el_base() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("project.godot"), "config_version=5\n").unwrap();
        let r = carpeta_del_joc(dir.path(), Engine::Godot4, "qualsevol joc");
        assert_eq!(r, dir.path());
    }

    /// Si el workspace NO és un projecte del motor, el joc va a la seva carpeta
    /// (i el projecte de l'usuari no s'hi barreja).
    #[test]
    fn si_el_base_no_es_un_projecte_el_joc_va_a_la_seva_carpeta() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
        let r = carpeta_del_joc(dir.path(), Engine::Godot4, "Fes un joc de nauxes");
        assert!(r.starts_with(dir.path()), "{r:?}");
        assert!(r.ends_with("prototypes/joc-de-nauxes"), "{r:?}");
    }

    /// El projecte de Godot creat és el dels tres fitxers canònics: sense això
    /// la Jam escriu fitxers solts i no hi ha res a obrir ni a executar.
    #[test]
    fn prepara_el_projecte_de_godot_amb_escena_principal() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("prototypes").join("joc");
        std::fs::create_dir_all(&root).unwrap();
        let projecte = prepara_projecte(&root, Engine::Godot4).unwrap();
        assert!(projecte.join("project.godot").is_file(), "{projecte:?}");
        assert!(projecte.join("main.tscn").is_file(), "{projecte:?}");
        assert!(projecte.join("main.gd").is_file(), "{projecte:?}");
    }

    /// El projecte de Bevy crea el `Cargo.toml` i el `main.rs` d'entrada.
    #[test]
    fn prepara_el_projecte_de_bevy() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("prototypes").join("joc");
        std::fs::create_dir_all(&root).unwrap();
        let projecte = prepara_projecte(&root, Engine::Bevy).unwrap();
        assert!(projecte.join("Cargo.toml").is_file(), "{projecte:?}");
        assert!(projecte.join("src").join("main.rs").is_file(), "{projecte:?}");
    }

    /// L'especificació del motor ha de dir el LLENGUATGE: sense això la IA
    /// escriu en el llenguatge que li sembla i el joc no és del motor triat.
    #[test]
    fn l_engine_spec_diga_el_llenguatge() {
        assert!(engine_spec(Engine::Godot4).contains("GDScript"));
        assert!(engine_spec(Engine::Bevy).contains("Rust"));
        assert!(engine_spec(Engine::Unity).contains("C#"));
        assert!(engine_spec(Engine::Custom).contains("index.html"));
    }
}
