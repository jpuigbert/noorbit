//! ROUTING del motor (Fase 2): el local de tres nivells és el camí base;
//! el NÚVOL és una AMPLIACIÓ, no un substitut. Sense cap clau API, NoOrbit
//! funciona exactament igual que abans (Ollama → nativa neuronal → reserva
//! simbòlica). Amb una clau configurada, les tasques de CODI MULTI-FITXER
//! poden anar al proveïdor en línia de l'usuari; les preguntes curtes i les
//! feines ràpides es queden al local. La selecció EXPLÍCITA de proveïdor de
//! l'usuari (per xat o global) es respecta SEMPRE: mai es rerouteja.
//!
//! Tot el criteri de decisió viu en funcions PURES (testables sense xarxa,
//! sense Ollama i sense Tauri).

use super::native;

/// Mode per defecte: decideix NoOrbit segons la tasca i les claus disponibles.
pub const MODE_AUTO: &str = "auto";
/// Força el camí local (Ollama si és en marxa; si no, la nativa integrada).
pub const MODE_LOCAL: &str = "local";
/// Preferix el núvol de l'usuari; sense clau, cau al local (mai es queda mut).
pub const MODE_CLOUD: &str = "cloud";

/// Normalitza el mode rebut de la UI: qualsevol valor desconegut és «auto».
pub fn normalize_mode(mode: &str) -> &'static str {
    match mode.trim().to_ascii_lowercase().as_str() {
        MODE_LOCAL => MODE_LOCAL,
        MODE_CLOUD => MODE_CLOUD,
        _ => MODE_AUTO,
    }
}

/// La PREGUNTA real dins d'un prompt que pot portar el context del projecte
/// davant (marcador «PREGUNTA / TASCA:»). El routing es decideix sobre la
/// pregunta, no sobre els milers de caràcters de codi adjunt.
pub fn question_of(prompt: &str) -> String {
    let tail = match prompt.rfind("PREGUNTA / TASCA:") {
        Some(pos) => &prompt[pos + "PREGUNTA / TASCA:".len()..],
        None => prompt,
    };
    tail.trim().chars().take(2_000).collect()
}

/// Una tasca de CODI MULTI-FITXER? Heurística barata i determinista:
/// · dos o més fitxers de codi/config anomenats explícitament;
/// · paraules clau de projecte sencer/multi-fitxer (català, castellà, anglès);
/// · «fitxers»/«archivos»/«files» en plural + un verb de crear/editar codi.
/// Les preguntes curtes («què és X?», «calcula…») no hi cauen mai.
pub fn is_multi_file_code_task(question: &str) -> bool {
    let low = super::codegen::pla(question);
    const MULTIFITXER: &[&str] = &[
        "multi-fitxer",
        "multifitxer",
        "multi-file",
        "multiple files",
        "several files",
        "varios archivos",
        "multiples archivos",
        "diversos fitxers",
        "varis fitxers",
        "multiples fitxers",
        "tots els fitxers",
        "tot el projecte",
        "projecte sencer",
        "proyecto completo",
        "todo el proyecto",
        "whole project",
        "entire project",
    ];
    if MULTIFITXER.iter().any(|k| low.contains(k)) {
        return true;
    }
    // Fitxers de codi/config anomenats: dues rutes DISTINTES ja són una feina
    // multi-fitxer (el mateix fitxer repetit no compta).
    if let Ok(re) = regex::Regex::new(
        r"(?i)[a-z0-9_][a-z0-9_./-]*\.(?:rs|tsx|ts|jsx|js|py|php|css|scss|html|gd|tscn|godot|json|toml|lua|cs|cpp|cc|h|hpp|java|go|rb|sh|yml|yaml|vue|svelte|sql|uproject)\b",
    ) {
        let mut vistes: Vec<String> = Vec::new();
        for m in re.find_iter(&low) {
            let s = m.as_str().to_string();
            if !vistes.contains(&s) {
                vistes.push(s);
            }
            if vistes.len() >= 2 {
                return true;
            }
        }
    }
    const PLURAL: &[&str] = &["fitxers", "arxius", "archivos", "files"];
    const VERBS: &[&str] = &[
        "crea", "genera", "escriu", "implementa", "edita", "modifica", "refactor",
        "create", "generate", "write", "implement", "edit", "modify", "build",
        "crear", "generar", "escribir", "implementar", "editar", "modificar",
    ];
    PLURAL.iter().any(|p| low.contains(p)) && VERBS.iter().any(|v| low.contains(v))
}

/// Decisió de routing PURA. `explicit` és el proveïdor triat explícitament
/// per l'usuari (per xat o global): si n'hi ha, es respecta TAL QUAL. Si no,
/// s'aplica el mode del motor sobre l'estat real (Ollama en marxa? clau?).
/// Retorna l'identificador del proveïdor que ha de respondre.
pub fn decide_engine(
    explicit: Option<&str>,
    mode: &str,
    ollama_running: bool,
    remote: Option<&str>,
    heavy_code: bool,
) -> String {
    // Selecció explícita: MAI es rerouteja (ni tan sols «local»/«cloud»).
    if let Some(p) = explicit {
        return p.to_string();
    }
    let local = || {
        if ollama_running {
            "ollama".to_string()
        } else {
            native::PROVIDER_ID.to_string()
        }
    };
    match normalize_mode(mode) {
        MODE_LOCAL => local(),
        MODE_CLOUD => remote.map(|r| r.to_string()).unwrap_or_else(local),
        // AUTO: amb clau, el codi multi-fitxer va al núvol (millor qualitat);
        // la resta es queda al local. Sense Ollama en marxa, la caiguda al
        // remot disponible es conserva (NoOrbit funciona sense motor local).
        _ => {
            if heavy_code {
                if let Some(r) = remote {
                    return r.to_string();
                }
            }
            if ollama_running {
                "ollama".to_string()
            } else if let Some(r) = remote {
                r.to_string()
            } else {
                native::PROVIDER_ID.to_string()
            }
        }
    }
}

#[cfg(test)]
mod proves {
    use super::*;

    const REMOT: Option<&str> = Some("prov_deepseek");

    /// Sense cap clau API, el pipeline local de tres nivells és l'únic camí:
    /// ni el mode «cloud» pot inventar-se un proveïdor.
    #[test]
    fn sense_clau_tot_es_local() {
        assert_eq!(
            decide_engine(None, MODE_AUTO, true, None, true),
            "ollama"
        );
        assert_eq!(
            decide_engine(None, MODE_AUTO, false, None, true),
            native::PROVIDER_ID
        );
        assert_eq!(
            decide_engine(None, MODE_CLOUD, false, None, true),
            native::PROVIDER_ID
        );
        assert_eq!(decide_engine(None, MODE_LOCAL, true, None, false), "ollama");
    }

    /// Amb clau + Auto + tasca multi-fitxer → núvol; pregunta curta → local.
    #[test]
    fn amb_clau_i_auto_el_codi_multifitxer_va_al_nuvol() {
        assert_eq!(
            decide_engine(None, MODE_AUTO, true, REMOT, true),
            "prov_deepseek"
        );
        // Pregunta curta amb Ollama en marxa: es queda al local.
        assert_eq!(
            decide_engine(None, MODE_AUTO, true, REMOT, false),
            "ollama"
        );
        // Sense Ollama en marxa, la caiguda al remot es conserva (curta o no).
        assert_eq!(
            decide_engine(None, MODE_AUTO, false, REMOT, false),
            "prov_deepseek"
        );
    }

    /// La selecció explícita de l'usuari es respecta SEMPRE, en tots els modes.
    #[test]
    fn la_seleccio_explicita_mai_es_rerouteja() {
        for mode in [MODE_AUTO, MODE_LOCAL, MODE_CLOUD] {
            assert_eq!(
                decide_engine(Some("prov_anthropic"), mode, true, REMOT, true),
                "prov_anthropic"
            );
            assert_eq!(
                decide_engine(Some("ollama"), mode, false, REMOT, true),
                "ollama"
            );
            assert_eq!(
                decide_engine(Some(native::PROVIDER_ID), mode, true, REMOT, true),
                native::PROVIDER_ID
            );
        }
    }

    /// Modes forçats: «local» ignora la clau; «cloud» la fa servir si existeix.
    #[test]
    fn els_modes_forcats_s_apliquen_sobre_l_estat_real() {
        assert_eq!(decide_engine(None, MODE_LOCAL, true, REMOT, true), "ollama");
        assert_eq!(
            decide_engine(None, MODE_LOCAL, false, REMOT, true),
            native::PROVIDER_ID
        );
        assert_eq!(
            decide_engine(None, MODE_CLOUD, true, REMOT, false),
            "prov_deepseek"
        );
        // Mode desconegut = auto.
        assert_eq!(normalize_mode("qualsevol-cosa"), MODE_AUTO);
        assert_eq!(normalize_mode(" CLOUD "), MODE_CLOUD);
    }

    #[test]
    fn detectar_tasca_multifitxer_per_rutes_i_paraules() {
        assert!(is_multi_file_code_task(
            "crea src/App.tsx i src/styles.css per al joc"
        ));
        assert!(is_multi_file_code_task(
            "implementa el login multi-fitxer amb backend i frontend"
        ));
        assert!(is_multi_file_code_task(
            "genera els fitxers del projecte sencer"
        ));
        assert!(is_multi_file_code_task(
            "edita els arxius main.gd i player.gd"
        ));
        // El mateix fitxer repetit NO és multi-fitxer.
        assert!(!is_multi_file_code_task("edita main.rs i després torna a main.rs"));
        // Preguntes curtes i generals: mai al núvol per aquesta heurística.
        assert!(!is_multi_file_code_task("què és una closure?"));
        assert!(!is_multi_file_code_task("calcula 2+2"));
        assert!(!is_multi_file_code_task("explica'm aquest error"));
    }

    #[test]
    fn la_pregunta_es_treu_del_marcador_del_prompt() {
        let prompt = format!(
            "{}\n\n---\nPREGUNTA / TASCA:\n{}",
            "x".repeat(5_000),
            "crea src/a.ts i src/b.ts"
        );
        assert_eq!(question_of(&prompt), "crea src/a.ts i src/b.ts");
        assert_eq!(question_of("hola"), "hola");
    }
}