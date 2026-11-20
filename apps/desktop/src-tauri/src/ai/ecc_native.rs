//! Consulta NATIVA de les skills d'ECC (i de la resta de skills instal·lades).
//!
//! ECC ve de sèrie a NoOrbit (`ecc-builtin/`, sembrat a `<skills_dir>/ecc/`).
//! Aquest mòdul permet que la IA nativa —el motor simbòlic, sense Ollama ni
//! cap model— també aprofite aquest coneixement: si la petició anomena una
//! skill (p. ex. «com faig tdd workflow»), la nativa respon amb el contingut
//! del seu SKILL.md. És disclosure simbòlic: només es llegeix el fitxer de
//! les skills que encaixen, mai s'injecten 329 skills al prompt.
//!
//! També respon «quines skills ecc tens» amb un índex compacte. No executa
//! res: els hooks i MCP d'ECC no s'executen (limitació honesta documentada).

use std::path::{Path, PathBuf};

/// Resol el directori de skills igual que el gestor (config o dades locals).
fn skills_dir() -> Option<PathBuf> {
    let cfg = crate::config::AppConfig::load_or_default();
    Some(crate::skills::resolve_skills_dir(&cfg))
}

/// Una skill candidata: nom de carpeta, tokens del nom i ruta del SKILL.md.
struct Candidata {
    slug: String,
    tokens: Vec<String>,
    path: PathBuf,
}

/// Llista les skills instal·lades (carpetes amb SKILL.md) a `ecc/`,
/// `ecc-rules/` i `builtin/`. Només llegeix noms de carpeta: barat.
fn llista_skills(dir: &Path) -> Vec<Candidata> {
    let mut out = Vec::new();
    for sub in ["ecc", "ecc-rules", "builtin"] {
        let Ok(entries) = std::fs::read_dir(dir.join(sub)) else { continue };
        for e in entries.flatten() {
            if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let skill = e.path().join("SKILL.md");
            if !skill.is_file() {
                continue;
            }
            let slug = e.file_name().to_string_lossy().to_string();
            let tokens: Vec<String> = crate::learning::tokens(&slug.replace('-', " "))
                .into_iter()
                .filter(|w| w.len() >= 3)
                .collect();
            if tokens.is_empty() {
                continue;
            }
            out.push(Candidata { slug, tokens, path: skill });
        }
    }
    out
}

/// Índex compacte de skills (resposta a «quines skills ecc tens»).
fn index(skills: &[Candidata], dir: &Path) -> String {
    let ecc: Vec<&str> = skills
        .iter()
        .filter(|s| s.path.starts_with(dir.join("ecc")))
        .map(|s| s.slug.as_str())
        .collect();
    let mut llista: Vec<&str> = ecc.iter().copied().take(80).collect();
    llista.sort_unstable();
    let resta = ecc.len().saturating_sub(llista.len());
    format!(
        "Tinc {} skills d'ECC de sèrie (més les builtin i les que instal·les). \
         Digues el nom d'una i te la explique sencera; les primeres: {}{}. \
         Amb una IA amb eines, l'índex complet arriba al prompt i les skills \
         es llegeixen soles quan calen.",
        ecc.len(),
        llista.join(", "),
        if resta > 0 { format!("… i {resta} més") } else { String::new() },
    )
}

/// Cos d'una skill sense frontmatter, retallat si és molt llarg.
fn cos_skill(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let (name, _, body) = crate::commands::ecc::parse_ecc_md(&raw, "");
    if body.is_empty() {
        return None;
    }
    let total = body.chars().count();
    let cos: String = body.chars().take(6000).collect();
    let cua = if total > 6000 {
        format!("\n\n… (retallat: {total} caràcters; el fitxer sencer és a {})", path.display())
    } else {
        format!("\n\n(Font: {})", path.display())
    };
    let titol = if name.is_empty() { "Skill".to_string() } else { name };
    Some(format!("**{titol}** — skill de sèrie a NoOrbit.\n\n{cos}{cua}"))
}

/// Consulta pura (testeable): `dir` és el directori de skills.
pub fn consulta(prompt: &str, dir: &Path) -> Option<String> {
    let tokens = crate::learning::tokens(prompt);
    if tokens.is_empty() {
        return None;
    }
    let parla_de_skills = tokens.iter().any(|w| w == "ecc" || w == "skill" || w == "skills");
    let skills = llista_skills(dir);
    if skills.is_empty() {
        return None;
    }
    // Índex explícit: «quines skills ecc tens», «llista les skills».
    if parla_de_skills
        && tokens.iter().any(|w| {
            matches!(w.as_str(), "quines" | "quin" | "llista" | "llistat" | "tens" | "hi" | "catalog" | "cataleg")
        })
    {
        return Some(index(&skills, dir));
    }
    // Coincidència pel NOM de la skill: tots els tokens del nom presents a
    // la petició. Amb un sol token, cal que siga prou específic (4+ lletres)
    // o que la petició parle explícitament d'skills/ECC (evita segrestar
    // converses normals com «fes code»).
    let mut millor: Option<(usize, &Candidata)> = None;
    for s in &skills {
        let tots = s.tokens.iter().all(|t| tokens.iter().any(|w| w == t));
        if !tots {
            continue;
        }
        let especific = s.tokens.len() >= 2
            || s.tokens.iter().any(|t| t.len() >= 4)
            || parla_de_skills;
        if !especific {
            continue;
        }
        let punts = s.tokens.len();
        if millor.map(|(p, _)| punts > p).unwrap_or(true) {
            millor = Some((punts, s));
        }
    }
    if let Some((_, s)) = millor {
        return cos_skill(&s.path);
    }
    // Sense coincidència de nom però parlant d'ECC/skills: índex orientatiu.
    if parla_de_skills {
        return Some(index(&skills, dir));
    }
    None
}

/// Resposta de la nativa sobre skills instal·lades, o None si la petició no
/// en parla. Es crida després de les plantilles de codi (codegen) i abans del
/// cervell: una skill concreta és més específica que la FAQ genèrica.
pub fn answer(prompt: &str) -> Option<String> {
    let dir = skills_dir()?;
    // Només la PREGUNTA de l'usuari: el context del projecte que el xat
    // anteposa conté milers de paraules i segrestaria la cerca de skills.
    consulta(&super::project::question_of(prompt), &dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arbre_fake(root: &Path) {
        for (rel, name, body) in [
            ("ecc/tdd-workflow", "tdd-workflow", "Escriu els tests ABANS que el codi."),
            ("ecc/api-design", "api-design", "Dissenya l'API REST primer."),
            ("ecc-rules/testing", "ECC · regla testing", "Sempre fes tests."),
            ("builtin/deepinfer", "deepinfer", "Guia del motor Deep Infer."),
        ] {
            let d = root.join(rel);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(
                d.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: prova\n---\n\n{body}\n"),
            )
            .unwrap();
        }
        // Carpeta sense SKILL.md: no ha de comptar.
        std::fs::create_dir_all(root.join("ecc/buida")).unwrap();
    }

    #[test]
    fn respon_la_skill_anomenada() {
        let tmp = std::env::temp_dir().join(format!("noorbit-eccnat-{}", uuid::Uuid::new_v4()));
        arbre_fake(&tmp);
        let r = consulta("com faig tdd workflow", &tmp).unwrap();
        assert!(r.contains("Escriu els tests ABANS"), "{}", r);
        assert!(r.contains("tdd-workflow"), "{}", r);
        // Un sol token específic també val.
        let r = consulta("explica deepinfer", &tmp).unwrap();
        assert!(r.contains("Deep Infer"), "{}", r);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn no_segresta_converses() {
        let tmp = std::env::temp_dir().join(format!("noorbit-eccnat-{}", uuid::Uuid::new_v4()));
        arbre_fake(&tmp);
        assert!(consulta("hola com estàs", &tmp).is_none());
        assert!(consulta("calcula 2+2", &tmp).is_none());
        // Token curt i genèric sense menció d'skills: no dispara.
        assert!(consulta("fes code", &tmp).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn index_de_skills_ecc() {
        let tmp = std::env::temp_dir().join(format!("noorbit-eccnat-{}", uuid::Uuid::new_v4()));
        arbre_fake(&tmp);
        let r = consulta("quines skills ecc tens", &tmp).unwrap();
        assert!(r.contains("tdd-workflow"), "{}", r);
        assert!(r.contains("api-design"), "{}", r);
        // Les regles i builtin no compten com a ECC a l'índex.
        assert!(!r.contains("deepinfer"), "{}", r);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}