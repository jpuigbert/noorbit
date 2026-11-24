//! PONT ENTRE MOTORS: el que crea una eina (Blender sobretot) entra al
//! motor del joc —Unreal, Godot o Bevy— sense que l'usuari faci res a mà.
//!
//! El flux és EL MATEIX que el de les altres eines de motor: la IA escriu
//! una directriu i NoOrbit l'executa DE VERITAT. Concretament:
//!
//! 1. `@blender-headless:` crea el model i l'exporta a `EngineWork/Exports/`
//!    (dins del projecte, perquè el repositori se'l quedi).
//! 2. `@import: nom-del-model.glb` l'importa al motor detectat: a Unreal
//!    amb la Remote Control API, a Godot copiant-lo a `assets/` i reimportant
//!    en headless, a Bevy copiant-lo a `assets/` (on Bevy els llegeix).
//!
//! La IA decideix quan importar: sense la directriu `@import:` no es mou res.

use std::path::{Path, PathBuf};

use crate::engine_tools::EngineContext;

/// Carpeta (relativa a l'arrel del projecte) on es deixen els assets
/// exportats per Blender i companyia.
pub const EXPORTS_DIR: &str = "EngineWork/Exports";

/// Destí on s'importa un asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Unreal,
    Godot,
    Bevy,
}

impl Target {
    /// Nom curt (per a missatges i registres).
    pub fn name(self) -> &'static str {
        match self {
            Target::Unreal => "unreal",
            Target::Godot => "godot",
            Target::Bevy => "bevy",
        }
    }

    /// Reconeix el nom d'un motor: «godot4», «unreal5», «bevy», «rust»…
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "unreal" | "unreal5" | "ue5" | "ue" => Some(Target::Unreal),
            "godot" | "godot4" => Some(Target::Godot),
            "bevy" | "rust" => Some(Target::Bevy),
            _ => None,
        }
    }
}

/// Resultat d'importar un asset a UN motor.
#[derive(Debug, Clone)]
pub struct ImportReport {
    pub target: Target,
    pub ok: bool,
    pub summary: String,
    /// Fitxers tocats dins del projecte (perquè el Diff de la Jam els vegi).
    pub artifacts: Vec<PathBuf>,
}

/// Separa l'especificació «[motor] <ruta>»: si la primera paraula és un
/// motor, es queda com a destí i la resta és la ruta de l'asset.
pub fn split_spec(payload: &str) -> (String, Option<Target>) {
    let t = payload.trim();
    if let Some((first, rest)) = t.split_once(char::is_whitespace) {
        if let Some(target) = Target::parse(first) {
            return (rest.trim().to_string(), Some(target));
        }
    }
    (t.to_string(), None)
}

/// Carpeta d'exportació d'assets del projecte.
pub fn export_dir(root: &Path) -> PathBuf {
    root.join("EngineWork").join("Exports")
}

/// Crea (si cal) la carpeta d'exportació i en torna la ruta ABSOLUTA.
///
/// La crea abans que la IA exporti res: Blender no crea carpetes i un
/// export a una ruta inexistent falla. Aquesta és la ruta que se li dona al
/// model perquè l'export acabi dins del projecte (i el repositori se'l quedi).
pub fn ensure_exports_dir(root: &Path) -> PathBuf {
    let dir = export_dir(root);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Assets exportats al projecte, els més recents primer (és el que la IA
/// vol importar: el darrer que ha creat).
pub fn list_exports(root: &Path) -> Vec<PathBuf> {
    let dir = export_dir(root);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<(std::time::SystemTime, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if !p.is_file() {
                return None;
            }
            let m = p
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            Some((m, p))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().map(|(_, p)| p).collect()
}

/// Resol una referència a un asset: ruta absoluta, ruta relativa al projecte,
/// ruta dins de `EngineWork/Exports/` o només el nom del fitxer exportat.
pub fn resolve_asset(root: &Path, spec: &str) -> Result<PathBuf, String> {
    let s = spec.trim().trim_matches('"').trim();
    if s.is_empty() {
        return Err(
            "cal indicar quin asset s'ha d'importar (p. ex. «@import: EngineWork/Exports/model.glb»)"
                .into(),
        );
    }
    let p = Path::new(s);
    if p.is_absolute() {
        return if p.is_file() {
            Ok(p.to_path_buf())
        } else {
            Err(format!("no trobo el fitxer: {}", p.display()))
        };
    }
    let direct = root.join(s);
    if direct.is_file() {
        return Ok(direct);
    }
    let in_exports = export_dir(root).join(s);
    if in_exports.is_file() {
        return Ok(in_exports);
    }
    // Només el nom del fitxer: es busca entre les exportacions recents.
    if !s.contains('/') && !s.contains('\\') {
        if let Some(found) = list_exports(root)
            .into_iter()
            .find(|f| f.file_name().map(|n| n == s).unwrap_or(false))
        {
            return Ok(found);
        }
    }
    Err(format!(
        "no trobo l'asset «{s}» ni al projecte ni a «{EXPORTS_DIR}/»"
    ))
}

/// Motors detectats dins el projecte (en ordre: Unreal, Godot, Bevy).
pub fn detected_targets(ctx: &EngineContext) -> Vec<Target> {
    let mut out = Vec::new();
    if ctx.uproject.is_some() {
        out.push(Target::Unreal);
    }
    if ctx.godot.is_some() {
        out.push(Target::Godot);
    }
    if ctx.bevy.is_some() {
        out.push(Target::Bevy);
    }
    out
}

/// Importa un asset al motor indicat.
pub async fn import_into(ctx: &EngineContext, target: Target, src: &Path) -> ImportReport {
    match target {
        Target::Godot => import_godot(ctx, src).await,
        Target::Bevy => import_bevy(ctx, src),
        Target::Unreal => import_unreal(src).await,
    }
}

/// A Godot: copia l'asset a `assets/`, crea l'escena `.tscn` (si és un `.glb`)
/// i reimporta els assets en headless (`.godot/imported`).
async fn import_godot(ctx: &EngineContext, src: &Path) -> ImportReport {
    let fail = |msg: String| ImportReport {
        target: Target::Godot,
        ok: false,
        summary: msg,
        artifacts: Vec::new(),
    };
    // Sense projecte, es CREA: la IA ha demanat importar a Godot, així que el
    // projecte ha d'existir i el detector l'ha de confirmar.
    let project = match crate::godot::setup::ensure_project(&ctx.root) {
        Ok(p) => p.project_godot,
        Err(e) => return fail(format!("Godot: no s'ha pogut crear el projecte ({e})")),
    };
    match crate::godot::import_asset(&project, src).await {
        Ok(r) => {
            let mut summary = format!("Godot: asset a «{}»", r.res_path);
            if let Some(scene) = &r.scene_path {
                summary.push_str(&format!(" (escena «{scene}» per instanciar-la)"));
            }
            // Reimport en headless: sense això Godot no crea els `.import`.
            match crate::godot::reimport(&project).await {
                Ok(()) => summary.push_str("; assets reimportats"),
                Err(e) => summary.push_str(&format!("; AVÍS: no s'han reimportat els assets ({e})")),
            }
            let mut artifacts = vec![r.copied];
            if let Some(f) = r.scene_file {
                artifacts.push(f);
            }
            ImportReport {
                target: Target::Godot,
                ok: true,
                summary,
                artifacts,
            }
        }
        Err(e) => fail(format!("Godot: no s'ha pogut importar l'asset ({e})")),
    }
}

/// A Bevy: copia l'asset a `assets/` (Bevy hi llegeix des del codi amb
/// `asset_server.load(...)`).
fn import_bevy(ctx: &EngineContext, src: &Path) -> ImportReport {
    let fail = |msg: String| ImportReport {
        target: Target::Bevy,
        ok: false,
        summary: msg,
        artifacts: Vec::new(),
    };
    // Sense projecte, es CREA: la IA ha demanat importar a Bevy, així que el
    // projecte ha d'existir i el detector l'ha de confirmar.
    let cargo = match crate::bevy::ensure_project(&ctx.root) {
        Ok(p) => p,
        Err(e) => return fail(format!("Bevy: no s'ha pogut crear el projecte ({e})")),
    };
    match crate::bevy::import_asset(&cargo, src) {
        Ok(r) => ImportReport {
            target: Target::Bevy,
            ok: true,
            summary: format!(
                "Bevy: asset a «assets/{}» (carrega'l amb asset_server.load(\"{}\"))",
                r.file_name, r.file_name
            ),
            artifacts: vec![r.copied],
        },
        Err(e) => fail(format!("Bevy: no s'ha pogut importar l'asset ({e})")),
    }
}

/// A Unreal: import real via la Remote Control API (l'editor ha d'estar
/// obert). `script_import_asset` verifica que l'asset s'ha creat DE VERITAT.
async fn import_unreal(src: &Path) -> ImportReport {
    let fail = |msg: String| ImportReport {
        target: Target::Unreal,
        ok: false,
        summary: msg,
        artifacts: Vec::new(),
    };
    let client = crate::unreal::client::UnrealClient::new();
    let code = crate::unreal::python::script_import_asset(
        &src.to_string_lossy(),
        "/Game/Imports",
    );
    match crate::unreal::python::run_script(&client, &code).await {
        Ok(r) if r.success => ImportReport {
            target: Target::Unreal,
            ok: true,
            summary: format!(
                "Unreal: {}", crate::engine_tools::clip(&r.output.join(" / "), 400)
            ),
            artifacts: Vec::new(),
        },
        Ok(r) => fail(format!(
            "Unreal: la importació ha fallat ({})",
            crate::engine_tools::clip(&r.errors.join(" / "), 400)
        )),
        Err(e) => fail(format!(
            "Unreal: no s'ha pogut connectar amb l'editor (Remote Control): {e}. \
             Obre l'editor del projecte i torna-ho a provar."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separa_el_motor_de_la_ruta() {
        let (spec, target) = split_spec("godot EngineWork/Exports/model.glb");
        assert_eq!(target, Some(Target::Godot));
        assert_eq!(spec, "EngineWork/Exports/model.glb");
    }

    #[test]
    fn sense_motor_la_ruta_es_queda_entera() {
        let (spec, target) = split_spec("EngineWork/Exports/model.glb");
        assert_eq!(target, None);
        assert_eq!(spec, "EngineWork/Exports/model.glb");
    }

    #[test]
    fn reconeix_els_motors() {
        assert_eq!(Target::parse("godot4"), Some(Target::Godot));
        assert_eq!(Target::parse("unreal5"), Some(Target::Unreal));
        assert_eq!(Target::parse("rust"), Some(Target::Bevy));
        assert_eq!(Target::parse("blender"), None);
    }

    #[test]
    fn resol_l_asset_exportat_per_nom() {
        let root = std::env::temp_dir().join(format!("noorbit-bridge-{}", uuid::Uuid::new_v4()));
        let dir = export_dir(&root);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("model.glb");
        std::fs::write(&file, b"glb").unwrap();
        let found = resolve_asset(&root, "model.glb").unwrap();
        assert_eq!(found, file);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn un_asset_inexistent_dona_error_clar() {
        let root = std::env::temp_dir().join(format!("noorbit-bridge-buit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let err = resolve_asset(&root, "no-hi-es.glb").unwrap_err();
        assert!(err.contains("no trobo"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn les_exportacions_tenen_el_mes_recent_davant() {
        let root = std::env::temp_dir().join(format!("noorbit-bridge-ord-{}", uuid::Uuid::new_v4()));
        let dir = export_dir(&root);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("vell.glb"), b"a").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("nou.glb"), b"b").unwrap();
        let list = list_exports(&root);
        assert_eq!(list.len(), 2);
        assert!(list[0].ends_with("nou.glb"), "{list:?}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
