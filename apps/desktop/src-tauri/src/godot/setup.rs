//! Projecte de **Godot 4**: creació automàtica i fitxers canònics.
//!
//! Quan NoOrbit ha de treballar en Godot i el projecte encara no existeix,
//! el crea sol: `project.godot`, `main.tscn`, `main.gd` i el `.gitignore`.
//!
//! Els tres fitxers són els MATEIXOS que genera la IA nativa (`ai::codegen`):
//! la font de la veritat són aquestes constant, de manera que un projecte
//! creat a mà i un projecte generat per l'IA són idèntics i no divergeixen mai.
//!
//! IMPORTANT: aquest mòdul té efectes secundaris (escriu fitxers), per això
//! la DETECCIÓ (`EngineContext::detect`) no el crida mai: només ho fan les
//! rutes d'acció (obrir l'editor, executar una directive, importar un asset).

use std::path::{Path, PathBuf};

/// Contingut canònic del `project.godot` (marcador `{Nom}`).
pub const GODOT_PROJECT_GODOT: &str = r#"; {Nom} — projecte Godot 4 generat per la IA nativa de NoOrbit.
; Renderer «Compatibility» (OpenGL 3.3): ideal per a Mac Intel amb GPU integrada.
; Obre'l amb l'editor de Godot 4 i prem Play.

config_version=5

[application]

config/name="{Nom}"
run/main_scene="res://main.tscn"

[rendering]

renderer/rendering_method="gl_compatibility"
renderer/rendering_method.mobile="gl_compatibility"
"#;

/// Contingut canònic de l'escena principal (`main.tscn`).
pub const GODOT_MAIN_TSCN: &str = r#"[gd_scene load_steps=2 format=3]

[ext_resource type="Script" path="res://main.gd" id="1"]

[node name="Main" type="Node2D"]
script = ExtResource("1")
"#;

/// Contingut canònic del script principal (`main.gd`).
pub const GODOT_MAIN_GD: &str = r#"# {Nom} — escena principal generada per la IA nativa de NoOrbit.
# Una pilota que orbita dibuixada amb `_draw`: sense assets, sense xarxa.
extends Node2D

var temps := 0.0

func _process(delta: float) -> void:
    temps += delta
    queue_redraw()

func _draw() -> void:
    var centre := Vector2(sin(temps) * 120.0, cos(temps) * 60.0)
    draw_circle(centre, 20.0, Color(0.2, 0.5, 1.0))
"#;

/// Els tres fitxers d'un projecte de Godot 4, amb el nom ja resolt.
#[derive(Debug, Clone)]
pub struct GodotFiles {
    /// Contingut del `project.godot`.
    pub project_godot: String,
    /// Contingut de l'escena principal.
    pub main_tscn: String,
    /// Contingut del script principal.
    pub main_gd: String,
}

/// Projecte de Godot garantit: on és i si l'hem creat nosaltres.
#[derive(Debug, Clone)]
pub struct GodotProject {
    /// Ruta del `project.godot`.
    pub project_godot: PathBuf,
    /// `true` si el projecte l'acaba de crear NoOrbit.
    pub created: bool,
}

/// Substitueix els marcadors `{Nom}` i `{nom}` pel nom del projecte.
fn omple(text: &str, name: &str) -> String {
    let slug = slugify(name);
    text.replace("{Nom}", name).replace("{nom}", &slug)
}

/// Els tres fitxers canònics amb el nom donat.
pub fn project_files(name: &str) -> GodotFiles {
    GodotFiles {
        project_godot: omple(GODOT_PROJECT_GODOT, name),
        main_tscn: omple(GODOT_MAIN_TSCN, name),
        main_gd: omple(GODOT_MAIN_GD, name),
    }
}

/// Nom en minúscules amb guions, per a rutes i noms de paquet.
fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut guio = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            guio = false;
        } else if !out.is_empty() && !guio {
            out.push('-');
            guio = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "joc".to_string()
    } else {
        out
    }
}

/// Nom llegible del projecte, derivat de la carpeta: «meu-joc» → «Meu Joc».
///
/// Les comilles i els caràcters de control es treuen: el nom s'escriu dins
/// `config/name="…"` i no pod trencar el fitxer de configuració.
pub fn display_name(dir: &Path) -> String {
    let raw = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let net: String = raw
        .chars()
        .map(|c| {
            if c == '-' || c == '_' {
                ' '
            } else if c == '"' || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let net = net.trim();
    if net.is_empty() {
        return "Projecte Godot".to_string();
    }
    // Title case: la primera lletra de cada paraula en majúscula
    // («meu-joc» → «Meu Joc»).
    let mut out = String::new();
    let mut inici = true;
    for c in net.chars() {
        if c.is_whitespace() {
            inici = true;
            out.push(c);
        } else if inici && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            inici = false;
        } else {
            out.push(c);
            inici = false;
        }
    }
    out
}

/// Crea el projecte de Godot 4 dins de `dir` i el retorna.
///
/// És **idempotent**: si el `project.godot` ja hi és, no es sobreescriu ni
/// s'esborra res (una segona crida torna el projecte existent).
pub fn create_project(dir: &Path, name: &str) -> Result<GodotProject, String> {
    let manifest = dir.join(crate::godot::PROJECT_FILE);
    if manifest.is_file() {
        // Ja hi ha un projecte: no es toca (mai es pisa el treball de l'usuari).
        return Ok(GodotProject { project_godot: manifest, created: false });
    }
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("no s'ha pogut crear «{}»: {e}", dir.display()))?;

    let files = project_files(name);
    std::fs::write(&manifest, files.project_godot.as_bytes())
        .map_err(|e| format!("no s'ha pogut escriure el project.godot: {e}"))?;
    std::fs::write(dir.join("main.tscn"), files.main_tscn.as_bytes())
        .map_err(|e| format!("no s'ha pogut escriure el main.tscn: {e}"))?;
    std::fs::write(dir.join("main.gd"), files.main_gd.as_bytes())
        .map_err(|e| format!("no s'ha pogut escriure el main.gd: {e}"))?;
    // La carpeta `.godot/` és la memòria cau del motor: mai al Git.
    crate::fsutil::append_gitignore(dir, &[".godot/"])?;

    Ok(GodotProject { project_godot: manifest, created: true })
}

/// Hi ha ja fitxers d'un projecte de Godot a la carpeta?
fn hi_ha_fitxers_del_projecte(dir: &Path) -> bool {
    ["project.godot", "main.tscn", "main.gd"]
        .iter()
        .any(|f| dir.join(f).exists())
}

/// Projecte de Godot del workspace: el busca i, si no hi ha, el CREA.
///
/// Aquesta és la porta d'entrada de les rutes d'acció (obrir l'editor,
/// executar `@godot:`, importar un asset): l'usuari demana treballar en
/// Godot i NoOrbit deixa el projecte a punt en lloc de tornar un error.
pub fn ensure_project(root: &Path) -> Result<GodotProject, String> {
    if let Some(p) = crate::godot::find_project(root) {
        return Ok(GodotProject { project_godot: p, created: false });
    }
    let name = display_name(root);
    // No hi ha `project.godot`, però potser hi ha altres fitxers del projecte:
    // no es pisen mai. El projecte nou va a una subcarpeta.
    if hi_ha_fitxers_del_projecte(root) {
        let sub = root.join(slugify(&name));
        return create_project(&sub, &name);
    }
    create_project(root, &name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El projecte creat es detecta com a projecte de Godot i es pot obrir.
    #[test]
    fn crea_un_projecte_godot_detectable() {
        let dir = std::env::temp_dir().join(format!("noorbit-godot-new-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = create_project(&dir, "Meu Joc").unwrap();
        assert!(p.created);
        assert!(p.project_godot.is_file());
        assert!(dir.join("main.tscn").is_file());
        assert!(dir.join("main.gd").is_file());
        // Detecció real: el detector del motor ha de trobar-lo.
        assert_eq!(
            crate::godot::find_project(&dir),
            Some(dir.join(crate::godot::PROJECT_FILE))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// El projecte creat és coherent: el manifest apunta a l'escena i l'escena
    /// al script (sinó l'editor obre un projecte trencat).
    #[test]
    fn el_projecte_creat_es_coherent() {
        let dir = std::env::temp_dir().join(format!("noorbit-godot-ok-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        create_project(&dir, "Meu Joc").unwrap();
        let manifest = std::fs::read_to_string(dir.join("project.godot")).unwrap();
        assert!(manifest.contains("config_version=5"), "{manifest}");
        assert!(manifest.contains("run/main_scene=\"res://main.tscn\""), "{manifest}");
        assert!(manifest.contains("gl_compatibility"), "{manifest}");
        assert!(manifest.contains("config/name=\"Meu Joc\""), "{manifest}");
        let tscn = std::fs::read_to_string(dir.join("main.tscn")).unwrap();
        assert!(tscn.contains("res://main.gd"), "{tscn}");
        let gd = std::fs::read_to_string(dir.join("main.gd")).unwrap();
        assert!(gd.contains("extends Node2D"), "{gd}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Crear dues vegades no esborra res: la segona crida no crea res nou.
    #[test]
    fn crear_twice_no_pisa_res() {
        let dir = std::env::temp_dir().join(format!("noorbit-godot-idem-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        let p1 = create_project(&dir, "Meu Joc").unwrap();
        assert!(p1.created);
        // L'usuari hi toca l'escena…
        std::fs::write(dir.join("main.tscn"), "# tocada per l'usuari\n").unwrap();
        let p2 = create_project(&dir, "Meu Joc").unwrap();
        assert!(!p2.created, "no ha de tornar a crear res");
        let tscn = std::fs::read_to_string(dir.join("main.tscn")).unwrap();
        assert!(tscn.contains("tocada per l'usuari"), "{tscn}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `ensure_project` crea el projecte si no hi ha, i el torna si hi ha.
    #[test]
    fn assureix_el_projecte_del_workspace() {
        let dir = std::env::temp_dir().join(format!("noorbit-godot-ens-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p1 = ensure_project(&dir).unwrap();
        assert!(p1.created, "hauria de crear el projecte");
        let p2 = ensure_project(&dir).unwrap();
        assert!(!p2.created, "el segona cop no crea res");
        assert_eq!(p1.project_godot, p2.project_godot);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// El `.gitignore` del workspace no s'esborra: només s'hi afegeix la línia.
    #[test]
    fn no_pisa_el_gitignore_del_workspace() {
        let dir = std::env::temp_dir().join(format!("noorbit-godot-ign-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".gitignore"), "node_modules\n").unwrap();
        create_project(&dir, "Meu Joc").unwrap();
        let ign = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
        assert!(ign.contains("node_modules"), "{ign}");
        assert!(ign.contains(".godot/"), "{ign}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// El nom del projecte ven del nom de la carpeta, sense comilles ni salts.
    #[test]
    fn el_nom_ve_de_la_carpeta() {
        let dir = std::env::temp_dir().join("meu-joc");
        assert_eq!(display_name(&dir), "Meu Joc");
        let dir = std::env::temp_dir().join("joc\"molt");
        assert_eq!(display_name(&dir), "Joc Molt");
    }
}
