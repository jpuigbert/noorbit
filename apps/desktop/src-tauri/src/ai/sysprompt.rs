//! SYSTEM PROMPT EN CAPES (patró OpenCode, llicència MIT — atribució al
//! NOTICE). En lloc d'un prompt monolític, cada torn es muntatge de capes
//! independents i IDEMPOTENTS:
//!
//! 1. capa del PROVEÏDOR: instruccions específiques del format/model;
//! 2. capa d'ENTORN: cwd del projecte, sistema operatiu i eines disponibles
//!    detectades de manera BARATA (existència d'executables al PATH, sense
//!    spawnar processos);
//! 3. capa de PROJECTE: instruccions llegides de `AGENTS.md` o `NOORBIT.md`
//!    a l'arrel del workspace (truncades a 4.000 caràcters).
//!
//! Cada capa porta una MARCA: les capes d'entorn i de projecte són
//! idempotents (si el system prompt ja les conté, NO es dupliquen). La capa
//! de PROVEÏDOR, en canvi, és SUBSTITUÏBLE: el routing Auto→cloud ha de
//! reemplaçar la consigna LOCAL pre-muntada per la del proveïdor en línia
//! (abans la marca idempotent feia que la capa local «Ets el motor LOCAL…
//! fes-ne un tros» viatjara al núvol). Així el camí de xat amb eines i el
//! routing cloud poden aplicar les capes sense barallar-se.

use std::path::Path;

/// Marca de la capa d'entorn (detecció de duplicitats).
pub const ENV_MARK: &str = "── ENTORN DE TREBALL (NoOrbit) ──";
/// Marca de la capa d'instruccions del projecte.
pub const PROJECT_MARK: &str = "── INSTRUCCIONS DEL PROJECTE";
/// Marca de la capa específica del proveïdor.
pub const PROVIDER_MARK: &str = "── INSTRUCCIONS DEL PROVEÏDOR";

/// Sostre de caràcters de les instruccions del projecte (AGENTS.md/NOORBIT.md):
/// prou per a convencions reals, sense ofegar el context del model.
const PROJECT_MAX: usize = 4_000;

/// Eines que es detecten al PATH (barat: només existència del fitxer).
const TOOLS: &[&str] = &["php", "node", "cargo", "godot", "python3", "git"];

/// Capa específica del proveïdor: el que aquell format/model necessita sentir
/// per escriure codi COMPLET i en el format que NoOrbit materialitza.
pub fn provider_layer(kind: &str, model: &str) -> String {
    let low = model.to_lowercase();
    let consigna = if kind == "anthropic" || low.contains("claude") {
        "Ets Claude (API d'Anthropic). Raona abans d'escriure codi, però la \
         resposta final ha de portar els fitxers COMPLETS (cap bloc a mitja ni \
         «// resta igual»). Quan modifiquis un fitxer existent, usa edicions \
         CERCA/NOU exactes."
    } else if low.contains("deepseek") {
        "Ets DeepSeek. Escriu sempre els fitxers sencers de principi a fi: el \
         teu límit de sortida és ample i NoOrbit materialitza el que escriguis. \
         No deixis funcions a mitja ni comentaris «// TODO: implementar»."
    } else if kind == "ollama" || kind == "native" {
        "Ets el motor LOCAL de NoOrbit. Respon en català, clar i directe, amb \
         el codi complet dels fitxers que demani la tasca. Si la feina és massa \
         gran per a una resposta, fes-ne un tros i digues què queda. Per \
         defecte, un joc genèric («fes un joc», «make a game») es fa en \
         GODOT 4 amb els 3 fitxers canònics (project.godot, main.tscn, \
         main.gd); Python/Pygame o qualsevol altre motor NOMÉS si l'usuari \
         ho demana explícitament."
    } else {
        "Ets un model de xat via API compatible OpenAI. Escriu els fitxers \
         COMPLETS (cap truncament ni «// resta igual») i segueix les \
         convencions de directives que se t'indiquen."
    };
    format!("{} ({} · {})\n{}", PROVIDER_MARK, kind, model, consigna)
}

/// Detecció BARATA d'eines: recorre els directoris del PATH i comprova que
/// l'executable existeix (cap procés, cap `which` llançat). `path_var` és el
/// valor de la variable PATH (injectable per als tests).
pub fn detect_tools(path_var: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for tool in TOOLS {
        let name = if cfg!(windows) {
            format!("{}.exe", tool)
        } else {
            tool.to_string()
        };
        let trobat = std::env::split_paths(path_var)
            .any(|dir| dir.join(&name).is_file());
        if trobat {
            out.push(tool);
        }
    }
    out
}

/// Capa d'ENTORN: on treballem, amb quin SO i quines eines hi ha instal·lades.
/// El model no proposa ordres que aquest ordinador no pot executar.
pub fn env_layer(cwd: &str) -> String {
    let path = std::env::var("PATH").unwrap_or_default();
    let tools = detect_tools(&path);
    let eines = if tools.is_empty() {
        "cap de les habituals (php/node/cargo/godot/python3/git)".to_string()
    } else {
        tools.join(", ")
    };
    format!(
        "{}\nProjecte (cwd): {}\nSistema operatiu: {}\nEines disponibles al PATH: {}",
        ENV_MARK,
        cwd,
        std::env::consts::OS,
        eines
    )
}

/// Capa de PROJECTE: llegeix `AGENTS.md` (o, si no hi és, `NOORBIT.md`) de
/// l'arrel del workspace i el trunca raonablement. `None` si no existeix cap
/// dels dos fitxers (o són buits).
pub fn project_layer(root: &Path) -> Option<String> {
    for nom in ["AGENTS.md", "NOORBIT.md"] {
        let path = root.join(nom);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let mut cos: String = text.chars().take(PROJECT_MAX).collect();
        if text.chars().count() > PROJECT_MAX {
            cos.push_str("\n… [instruccions truncades a 4.000 caràcters per NoOrbit]");
        }
        return Some(format!("{} ({}):\n{}", PROJECT_MARK, nom, cos));
    }
    None
}

/// Munta el system prompt EN CAPES sobre una base existent. Les capes
/// d'entorn i projecte són idempotents (les marques ho delaten); la capa de
/// proveïdor es SUBSTITUEIX in-place (es reemplaça el bloc PROVIDER_MARK
/// existent, conservant l'ordre de la resta). `root` és l'arrel del projecte
/// obert (si n'hi ha): d'ella surten el cwd i les instruccions.
pub fn layered(base: Option<&str>, kind: &str, model: &str, root: Option<&Path>) -> String {
    let mut out = base.unwrap_or("").trim().to_string();
    // Capa de proveïdor SUBSTITUÏBLE: si ja n'hi ha una (p. ex. la LOCAL
    // pre-muntada per chat_with_tools), es reemplaça pel bloc nou sense
    // duplicar-la ni conservar la consigna antiga. El bloc va des de la
    // marca fins al proper separador de capes («\n\n») o el final del text.
    let nova = provider_layer(kind, model);
    if let Some(start) = out.find(PROVIDER_MARK) {
        let block_end = out[start..]
            .find("\n\n")
            .map(|i| start + i)
            .unwrap_or(out.len());
        out.replace_range(start..block_end, &nova);
    } else {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&nova);
    }
    let mut afegir = |capa: String, marca: &str| {
        if out.contains(marca) {
            return;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&capa);
    };
    let cwd = root
        .map(|r| r.display().to_string())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .map(|d| d.display().to_string())
                .unwrap_or_else(|_| "(desconegut)".to_string())
        });
    afegir(env_layer(&cwd), ENV_MARK);
    if let Some(r) = root {
        if let Some(capa) = project_layer(r) {
            afegir(capa, PROJECT_MARK);
        }
    }
    out
}

#[cfg(test)]
mod proves {
    use super::*;

    /// Carpeta temporal única per als tests (sense dependències externes).
    fn temp_dir(nom: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "noorbit_sysprompt_{}_{}",
            nom,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn la_capa_de_projecte_llegeix_agents_md_i_es_trunca() {
        let d = temp_dir("project");
        // AGENTS.md mana sobre NOORBIT.md.
        std::fs::write(d.join("AGENTS.md"), "Fes servir tabs i català.").unwrap();
        std::fs::write(d.join("NOORBIT.md"), "Ignora'm").unwrap();
        let capa = project_layer(&d).unwrap();
        assert!(capa.contains(PROJECT_MARK));
        assert!(capa.contains("AGENTS.md"));
        assert!(capa.contains("Fes servir tabs i català."));
        assert!(!capa.contains("Ignora'm"));
        // Sense AGENTS.md, entra NOORBIT.md.
        std::fs::remove_file(d.join("AGENTS.md")).unwrap();
        assert!(project_layer(&d).unwrap().contains("Ignora'm"));
        // Truncament raonable: 10.000 caràcters → sostre de 4.000 + nota.
        std::fs::write(d.join("NOORBIT.md"), "x".repeat(10_000)).unwrap();
        let llarga = project_layer(&d).unwrap();
        assert!(llarga.contains("truncades"));
        assert!(llarga.chars().count() < PROJECT_MAX + 200);
        // Fitxer buit o inexistent: cap capa.
        std::fs::write(d.join("NOORBIT.md"), "   ").unwrap();
        assert!(project_layer(&d).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn la_capa_d_entorn_porta_cwd_so_i_eines() {
        let capa = env_layer("/projectes/joc");
        assert!(capa.contains(ENV_MARK));
        assert!(capa.contains("/projectes/joc"));
        assert!(capa.contains(std::env::consts::OS));
        assert!(capa.contains("Eines disponibles al PATH:"));
    }

    #[test]
    fn les_eines_es_detecten_sense_spawnar_processos() {
        let d = temp_dir("path");
        // Un «node» i un «cargo» falsos (fitxers buits): n'hi ha prou perquè
        // la detecció és per existència, no per execució.
        std::fs::write(d.join("node"), "").unwrap();
        std::fs::write(d.join("cargo"), "").unwrap();
        let path = d.display().to_string();
        let tools = detect_tools(&path);
        assert!(tools.contains(&"node"));
        assert!(tools.contains(&"cargo"));
        assert!(!tools.contains(&"php"));
        // PATH buit: cap eina (i cap pànic).
        assert!(detect_tools("").is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn les_capes_no_es_dupliquen_si_ja_hi_son() {
        let d = temp_dir("dup");
        std::fs::write(d.join("AGENTS.md"), "Convencions del projecte.").unwrap();
        let base = "Prompt base de l'agent.";
        let una = layered(Some(base), "anthropic", "claude-sonnet-5-5", Some(&d));
        assert!(una.contains(PROVIDER_MARK));
        assert!(una.contains(ENV_MARK));
        assert!(una.contains(PROJECT_MARK));
        assert!(una.starts_with(base));
        // Segona passada (p. ex. routing cloud després de chat_with_tools):
        // el resultat és IDÈNTIC, sense cap capa repetida.
        let dues = layered(Some(&una), "anthropic", "claude-sonnet-5-5", Some(&d));
        assert_eq!(una, dues);
        assert_eq!(una.matches(ENV_MARK).count(), 1);
        assert_eq!(una.matches(PROJECT_MARK).count(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cada_proveidor_te_la_seva_consigna() {
        let claude = provider_layer("anthropic", "claude-sonnet-5-5");
        assert!(claude.contains("Claude"));
        let deepseek = provider_layer("openai", "deepseek-chat");
        assert!(deepseek.contains("DeepSeek"));
        let local = provider_layer("ollama", "qwen3:4b");
        assert!(local.contains("LOCAL"));
        // Totes porten la marca comuna: és el que permet no duplicar-les.
        for c in [claude, deepseek, local] {
            assert!(c.contains(PROVIDER_MARK));
        }
    }

    /// El joc genèric té motor per defecte a la capa LOCAL: Godot 4 amb els
    /// 3 fitxers canònics; Python/Pygame només explícit. La consigna NO ha de
    /// viatjar al núvol (la capa cloud la substitueix in-place).
    #[test]
    fn la_consigna_local_fixa_godot_com_a_motor_per_defecte() {
        for kind in ["ollama", "native"] {
            let local = provider_layer(kind, "qwen3:4b");
            assert!(local.contains("GODOT 4"), "{kind}: {local}");
            assert!(local.contains("project.godot"), "{kind}: {local}");
            assert!(local.contains("main.tscn"), "{kind}: {local}");
            assert!(local.contains("main.gd"), "{kind}: {local}");
            assert!(local.contains("Pygame"), "{kind}: {local}");
            assert!(local.contains("explícitament"), "{kind}: {local}");
        }
        // La capa cloud no porta la consigna de motor local.
        let cloud = provider_layer("anthropic", "claude-sonnet-5-5");
        assert!(!cloud.contains("GODOT 4"));
    }

    /// Finding #7: la capa LOCAL pre-muntada (Auto) NO ha de viatjar al model
    /// CLOUD quan el routing decideix proveïdor en línia: la capa cloud la
    /// SUBSTITUEIX (cap consigna local residual, cap marca duplicada).
    #[test]
    fn la_capa_local_es_substitueix_per_la_cloud_en_routing() {
        let d = temp_dir("route");
        let base = "Prompt base de l'agent.";
        // Auto pre-munta la capa LOCAL (motor natiu/ollama) a chat_with_tools.
        let local = layered(Some(base), "ollama", "qwen3:4b", Some(&d));
        assert!(local.contains("motor LOCAL"), "capa local: {local}");
        assert!(local.contains("fes-ne un tros"), "capa local: {local}");
        // Routing Auto→cloud: la capa cloud REPLAÇA la local in-place.
        let cloud = layered(Some(&local), "anthropic", "claude-sonnet-5-5", Some(&d));
        assert!(cloud.contains("Claude"), "capa cloud: {cloud}");
        assert!(!cloud.contains("motor LOCAL"), "la capa local no ha de viatjar al cloud: {cloud}");
        assert!(!cloud.contains("fes-ne un tros"), "consigna local residual: {cloud}");
        assert_eq!(cloud.matches(PROVIDER_MARK).count(), 1, "marca duplicada: {cloud}");
        assert_eq!(cloud.matches(ENV_MARK).count(), 1);
        // Idempotent: reaplicar el mateix proveïdor cloud no canvia res.
        let cloud2 = layered(Some(&cloud), "anthropic", "claude-sonnet-5-5", Some(&d));
        assert_eq!(cloud, cloud2);
        let _ = std::fs::remove_dir_all(&d);
    }
}