//! Ports de les integracions locals — FUENT ÚNICA.
//!
//! Aquests ports apareixen al Rust, al Python (scripts de Blender i Unreal)
//! i al shell (instal·lador). Si un canvia i els altres no, la integració
//! es trenca en silenci (el client connecta al port vell i no rep res).
//!
//! Els scripts Python els llegeixen de les variables d'entorn
//! (`NOORBIT_BRIDGE_PORT`, `NOORBIT_UNREAL_RC_PORT`), que NoOrbit injecta en
//! l'engegada: així el port CONFIGURABLE de l'usuari (Configuració ▸
//! Blender/Unreal) es respecta de debò (abans el Python sempre obria el port
//! per defecte i la configuració era ignorada).
//!
//! Hi ha un test (`ports_coincideixen_amb_els_scripts`) que falla si els
//! scripts i aquest fitxer deixen de coincidir.

/// Port del pont de Blender (TCP a 127.0.0.1).
pub const BLENDER_BRIDGE_PORT: u16 = 9876;

/// Port del servidor HTTP de Remote Control d'Unreal.
pub const UNREAL_RC_PORT: u16 = 30010;

/// Port del servidor WebSocket de Remote Control d'Unreal.
pub const UNREAL_RC_WS_PORT: u16 = 30020;

/// Variable d'entorn per indicar el port del pont a l'script de Blender.
pub const ENV_BLENDER_PORT: &str = "NOORBIT_BRIDGE_PORT";

/// Variable d'entorn per indicar el port de Remote Control al bootstrap d'Unreal.
pub const ENV_UNREAL_PORT: &str = "NOORBIT_UNREAL_RC_PORT";

#[cfg(test)]
mod proves {
    use super::*;

    /// Els ports han de coincidir amb els dels scripts Python i del shell.
    /// Si no, la integració es trenca en silenci.
    #[test]
    fn ports_coincideixen_amb_els_scripts() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("no s'ha trobat l'arrel del repo");
        let bridge = root.join("blender-scripts/noorbit_bridge.py");
        let bootstrap = root.join("unreal-scripts/bootstrap.py");
        let installer = root.join("scripts/install-unreal.sh");

        let bridge_txt = std::fs::read_to_string(&bridge).unwrap_or_default();
        assert!(
            bridge_txt.contains(&format!("\"{}\"", BLENDER_BRIDGE_PORT))
                || bridge_txt.contains(&format!("= {}", BLENDER_BRIDGE_PORT)),
            "noorbit_bridge.py no usa el port {} (font única: ports.rs)",
            BLENDER_BRIDGE_PORT
        );

        let boot_txt = std::fs::read_to_string(&bootstrap).unwrap_or_default();
        assert!(
            boot_txt.contains(&format!("\"{}\"", UNREAL_RC_PORT)),
            "bootstrap.py no usa el port {} (font única: ports.rs)",
            UNREAL_RC_PORT
        );

        let inst_txt = std::fs::read_to_string(&installer).unwrap_or_default();
        assert!(
            inst_txt.contains(&format!("RC_PORT={}", UNREAL_RC_PORT)),
            "install-unreal.sh no usa el port {} (font única: ports.rs)",
            UNREAL_RC_PORT
        );
    }
}
