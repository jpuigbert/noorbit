//! Compatibilitat amb ECC (github.com/affaan-m/ECC, llicència MIT).
//!
//! ECC és un sistema d'optimització per a agents de codi: skills en format
//! estàndard Agent Skills (SKILL.md, el MATEIX que NoOrbit), agents
//! especialistes (`agents/*.md`) i regles (`rules/common/*.md`). Des de
//! 0.71.12 ve DE SÈRIE: una còpia del coneixement viu a `ecc-builtin/`
//! (recurs del bundle) i es sembra al directori de skills en arrencar, sense
//! xarxa. El comandament `ecc_install` reimporta des d'aquesta còpia local
//! (o clona la font oficial si el bundle no hi és, p. ex. en desenvolupament
//! sense recursos copiats):
//!
//! - skills (`skills/` i `.agents/skills/`) → `<skills_dir>/ecc/<nom>/`
//!   (carpeta sencera, amb references/ i scripts/ si en té).
//! - agents (`agents/*.md`) → Especialistes de NoOrbit (nom, rol i prompt).
//! - regles comunes (`rules/common/*.md`) → `<skills_dir>/ecc-rules/<nom>/`.
//!
//! LIMITACIÓ HONESTA: els hooks, servidors MCP i AgentShield d'ECC són
//! específics d'altres harnessos (Claude Code…) i NO s'executen ací; només
//! s'importa el coneixement (skills, agents i regles), mai codi que s'executi
//! sol. Les regles per llenguatge (`rules/<lang>/`) es deixen fora: són
//! específiques de cada projecte i l'usuari les pot instal·lar com a skill
//! local si les vol.

use crate::commands::skills_store::{copy_dir_recursive, safe_name};
use crate::experts::ExpertInput;
use crate::AppState;
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use tauri::{command, State};

/// Font oficial: cap altre fork ni mirror (el README d'ECC avisa de còpies
/// no oficials amb contingut modificat). Només es clona si la còpia DE SÈRIE
/// (`ecc-builtin/`, recurs del bundle) no és disponible.
const ECC_REPO: &str = "https://github.com/affaan-m/ECC.git";

/// Carpeta de la còpia d'ECC que viatja amb l'instal·lador. En producció és
/// un recurs del bundle; en desenvolupament (o als tests) pot caure a la
/// carpeta del codi font. Retorna None si no hi ha cap còpia usable.
pub fn bundled_ecc_dir(app: Option<&tauri::AppHandle>) -> Option<std::path::PathBuf> {
    if let Some(app) = app {
        use tauri::Manager as _;
        if let Ok(res) = app.path().resource_dir() {
            let dir = res.join("ecc-builtin");
            if dir.join("skills").is_dir() {
                return Some(dir);
            }
        }
    }
    // Desenvolupament sense recursos copiats: la còpia del repositori.
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ecc-builtin");
    if dir.join("skills").is_dir() {
        Some(dir)
    } else {
        None
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EccReport {
    /// Skills importades a `ecc/`.
    pub skills: usize,
    /// Agents d'ECC convertits en especialistes.
    pub agents: usize,
    /// Regles comunes importades a `ecc-rules/`.
    pub rules: usize,
    /// Elements saltats (duplicats o sense format vàlid).
    pub skipped: usize,
    /// Resum llegible per a la interfície.
    pub details: Vec<String>,
}

/// Extreu el frontmatter (name/description) i el cos d'un fitxer .md d'ECC
/// (agent o regla). Sense frontmatter, el nom és el del fitxer i tot el
/// contingut és el cos.
pub fn parse_ecc_md(raw: &str, fallback_name: &str) -> (String, String, String) {
    let mut name = fallback_name.to_string();
    let mut description = String::new();
    let mut body_start = 0;
    let mut in_front = false;
    let mut offset = 0;
    for line in raw.split_inclusive('\n') {
        let trimmed = line.trim();
        if offset == 0 && trimmed == "---" {
            in_front = true;
            offset += line.len();
            continue;
        }
        if in_front {
            offset += line.len();
            if trimmed == "---" {
                body_start = offset;
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("name:") {
                let v = rest.trim().trim_matches('"');
                if !v.is_empty() {
                    name = v.to_string();
                }
            } else if let Some(rest) = trimmed.strip_prefix("description:") {
                description = rest.trim().trim_matches('"').to_string();
            }
            continue;
        }
        // Sense frontmatter: tot el fitxer és el cos.
        break;
    }
    (name, description, raw[body_start..].trim().to_string())
}

/// Importa skills i regles d'un clonatge d'ECC al directori de skills de
/// NoOrbit. Funció pura (sense estat global) per poder-la provar amb tests.
/// Retorna (skills, regles, saltats, detalls).
pub fn import_ecc_tree(repo: &Path, skills_dir: &Path) -> (usize, usize, usize, Vec<String>) {
    let mut skills = 0;
    let mut rules = 0;
    let mut skipped = 0;
    let mut details = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Skills: `skills/` mana; `.agents/skills/` només aporta les que no
    // existeixen ja (la majoria són duplicats).
    for root in [repo.join("skills"), repo.join(".agents").join("skills")] {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for e in entries.flatten() {
            if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let dir_name = e.file_name().to_string_lossy().to_string();
            let name = safe_name(&dir_name, "");
            if name.is_empty() || !e.path().join("SKILL.md").is_file() {
                skipped += 1;
                continue;
            }
            if !seen.insert(name.clone()) {
                skipped += 1;
                continue;
            }
            let dest = skills_dir.join("ecc").join(&name);
            let _ = std::fs::remove_dir_all(&dest);
            match copy_dir_recursive(&e.path(), &dest) {
                Ok(()) => skills += 1,
                Err(err) => {
                    skipped += 1;
                    details.push(format!("skill {name} — error: {err}"));
                }
            }
        }
    }

    // Regles comunes: cada .md esdevé una skill amb frontmatter propi perquè
    // el gestor de skills la detecte igual que la resta.
    let common = repo.join("rules").join("common");
    if let Ok(entries) = std::fs::read_dir(&common) {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().map(|x| x != "md").unwrap_or(true) {
                continue;
            }
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let name = safe_name(&stem, "");
            if name.is_empty() {
                skipped += 1;
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&path) else {
                skipped += 1;
                continue;
            };
            let (_, _, body) = parse_ecc_md(&raw, &name);
            let md = format!(
                "---\nname: ECC · regla {name}\ndescription: Regla comuna d'ECC ({name}) per a agents de codi.\n---\n\n{body}\n"
            );
            let dest = skills_dir.join("ecc-rules").join(&name);
            let _ = std::fs::create_dir_all(&dest);
            match std::fs::write(dest.join("SKILL.md"), md) {
                Ok(()) => rules += 1,
                Err(err) => {
                    skipped += 1;
                    details.push(format!("regla {name} — error: {err}"));
                }
            }
        }
    }

    details.push(format!(
        "{skills} skills i {rules} regles comunes importades (font oficial affaan-m/ECC)"
    ));
    (skills, rules, skipped, details)
}

/// Converteix els agents d'ECC (`agents/*.md`) en Especialistes de NoOrbit.
/// Es dedueixen pel nom (minúscules): reinstal·lar ECC actualitza skills
/// però NO duplica especialistes. Retorna (agents, saltats, detalls).
pub fn import_ecc_agents(
    repo: &Path,
    experts: &crate::experts::ExpertManager,
) -> (usize, usize, Vec<String>) {
    let mut agents = 0;
    let mut skipped = 0;
    let mut details = Vec::new();
    let mut nous: HashSet<String> = experts
        .list()
        .iter()
        .map(|e| e.name.to_lowercase())
        .collect();
    if let Ok(entries) = std::fs::read_dir(repo.join("agents")) {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().map(|x| x != "md").unwrap_or(true) {
                continue;
            }
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let Ok(raw) = std::fs::read_to_string(&path) else { continue };
            let (name, description, body) = parse_ecc_md(&raw, &stem);
            // El nom amb prefix es comprova contra els ja creats: reinstal·lar
            // ECC no duplica especialistes.
            let nom_final = format!("ECC · {name}");
            if body.is_empty() || !nous.insert(nom_final.to_lowercase()) {
                skipped += 1;
                continue;
            }
            // El rol és la descripció curta; el model d'ECC (opus…) no es
            // mapeja: cada especialista usarà el model actiu de NoOrbit.
            let role: String = description.chars().take(160).collect();
            let input = ExpertInput {
                name: nom_final,
                role,
                system_prompt: body,
                model: None,
            };
            match experts.add(input) {
                Ok(_) => agents += 1,
                Err(err) => {
                    skipped += 1;
                    details.push(format!("agent {name} — error: {err}"));
                }
            }
        }
    }
    (agents, skipped, details)
}

/// Sembra l'ECC DE SÈRIE al directori de skills de l'usuari: només el PRIMER
/// cop (marcardor `ecc/.noorbit-ecc-sembrat`). Si l'usuari esborra o edita les
/// skills després, no se li tornen a imposar: el marcardor ja hi és. Sense
/// xarxa: tot ve de la còpia local del bundle. Retorna None si ja estava
/// sembrat o si no hi ha còpia disponible.
pub fn seed_ecc_builtin(
    app: Option<&tauri::AppHandle>,
    skills_dir: &Path,
    experts: &crate::experts::ExpertManager,
) -> Option<EccReport> {
    let marker = skills_dir.join("ecc").join(".noorbit-ecc-sembrat");
    if marker.exists() {
        return None;
    }
    let repo = bundled_ecc_dir(app)?;
    let (skills, rules, mut skipped, mut details) = import_ecc_tree(&repo, skills_dir);
    let (agents, saltats_agents, det_agents) = import_ecc_agents(&repo, experts);
    skipped += saltats_agents;
    details.extend(det_agents);
    // Marcardor: versió + data perquè es puga saber quan es va sembrar.
    let _ = std::fs::write(
        &marker,
        format!("ECC de sèrie sembrat el {} (font: ecc-builtin del bundle)\n", chrono::Local::now().format("%Y-%m-%d %H:%M")),
    );
    details.push(format!("{agents} agents d'ECC convertits en especialistes"));
    Some(EccReport { skills, agents, rules, skipped, details })
}

/// Instal·la ECC d'un clic: fa servir la còpia DE SÈRIE del bundle si hi és
/// (sense xarxa); si no, clona la font oficial. Importa skills i regles, i
/// converteix els agents d'ECC en especialistes de NoOrbit. Els hooks i
/// MCP d'ECC NO s'executen (són d'altres harnessos).
#[command]
pub async fn ecc_install(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<EccReport, String> {
    let mgr = state.skills_manager.clone();
    let dir = {
        let guard = mgr.lock().map_err(|e| e.to_string())?;
        guard.skills_dir().to_path_buf()
    };
    let experts = state.experts.clone();

    // Còpia local de sèrie: camí ràpid, sense git ni xarxa.
    let bundled = bundled_ecc_dir(Some(&app));
    let tmp = if bundled.is_none() {
        let t = std::env::temp_dir().join(format!("noorbit-ecc-{}", uuid::Uuid::new_v4()));
        let tmp_clone = t.clone();
        // El clonatge pot trigar (xarxa): fil a part perquè la interfície no es
        // congeli (mateix patró que skills_install_github).
        let cloned = tauri::async_runtime::spawn_blocking(move || {
            std::process::Command::new("git")
                .args(["clone", "--depth", "1", ECC_REPO])
                .arg(&tmp_clone)
                .status()
                .map_err(|e| format!("No s'ha pogut executar git: {}", e))
        })
        .await
        .map_err(|e| e.to_string())?;
        let status = cloned?;
        if !status.success() {
            let _ = std::fs::remove_dir_all(&t);
            return Err("git clone ha fallat per a la font oficial d'ECC (github.com/affaan-m/ECC)".into());
        }
        Some(t)
    } else {
        None
    };
    let repo = bundled.clone().unwrap_or_else(|| tmp.clone().expect("repo"));

    let dir_clone = dir.clone();
    let repo_clone = repo.clone();
    let (skills, rules, mut skipped, mut details) =
        tauri::async_runtime::spawn_blocking(move || import_ecc_tree(&repo_clone, &dir_clone))
            .await
            .map_err(|e| e.to_string())?;

    // Agents → especialistes (dedup per nom, compartit amb la sembra).
    let experts_clone = experts.clone();
    let repo_clone = repo.clone();
    let (agents, saltats_agents, det_agents) =
        tauri::async_runtime::spawn_blocking(move || import_ecc_agents(&repo_clone, &experts_clone))
            .await
            .map_err(|e| e.to_string())?;
    skipped += saltats_agents;
    details.extend(det_agents);
    if let Some(t) = tmp {
        let _ = std::fs::remove_dir_all(&t);
    }
    // Sembrat de sèrie fet: el marcardor evita que l'arrencada següent ho
    // torne a fer (i que se sobreescriuen edicions de l'usuari).
    let _ = std::fs::create_dir_all(dir.join("ecc"));
    let _ = std::fs::write(
        dir.join("ecc").join(".noorbit-ecc-sembrat"),
        format!("ECC importat el {} ({})\n", chrono::Local::now().format("%Y-%m-%d %H:%M"), if bundled.is_some() { "còpia de sèrie" } else { "font oficial clonada" }),
    );

    // Recarrega la llista perquè la UI vegi les skills noves ja.
    if let Ok(mut guard) = mgr.lock() {
        guard.reload();
    }
    details.push(format!("{agents} agents d'ECC convertits en especialistes"));
    details.push(
        "Els hooks, MCP i AgentShield d'ECC no s'executen: són d'altres harnessos. Ací s'ha importat el coneixement (skills, agents i regles)."
            .into(),
    );
    Ok(EccReport { skills, agents, rules, skipped, details })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_ecc(root: &Path) {
        // Dues skills a skills/, una duplicada a .agents/skills/ i una de nova.
        for (rel, name) in [
            ("skills/alpha", "alpha"),
            ("skills/beta", "beta"),
            (".agents/skills/alpha", "alpha"),
            (".agents/skills/gamma", "gamma"),
        ] {
            let d = root.join(rel);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(
                d.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: prova\n---\n\ncos\n"),
            )
            .unwrap();
        }
        // Una carpeta sense SKILL.md: s'ha de saltar.
        std::fs::create_dir_all(root.join("skills/sense")).unwrap();
        // Regles comunes.
        std::fs::create_dir_all(root.join("rules/common")).unwrap();
        std::fs::write(root.join("rules/common/testing.md"), "# Regla de proves\nSempre fes tests.\n").unwrap();
        // Agents.
        std::fs::create_dir_all(root.join("agents")).unwrap();
        std::fs::write(
            root.join("agents/architect.md"),
            "---\nname: architect\ndescription: Especialista en arquitectura\nmodel: opus\n---\n\nPrompt de l'agent.\n",
        )
        .unwrap();
    }

    #[test]
    fn importa_skills_deduplica_i_regles() {
        let tmp = std::env::temp_dir().join(format!("noorbit-ecc-test-{}", uuid::Uuid::new_v4()));
        let repo = tmp.join("repo");
        let dest = tmp.join("skills");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        fake_ecc(&repo);

        let (skills, rules, skipped, _) = import_ecc_tree(&repo, &dest);
        assert_eq!(skills, 3); // alpha, beta, gamma (alpha duplicada saltada)
        assert_eq!(rules, 1);
        assert_eq!(skipped, 2); // duplicada + carpeta sense SKILL.md
        assert!(dest.join("ecc/alpha/SKILL.md").is_file());
        assert!(dest.join("ecc/gamma/SKILL.md").is_file());
        assert!(dest.join("ecc-rules/testing/SKILL.md").is_file());
        let regla = std::fs::read_to_string(dest.join("ecc-rules/testing/SKILL.md")).unwrap();
        assert!(regla.starts_with("---\nname: ECC · regla testing"));
        assert!(regla.contains("Sempre fes tests."));

        // Reinstal·lar: mateixos resultats, sense duplicar carpetes.
        let (skills2, _, _, _) = import_ecc_tree(&repo, &dest);
        assert_eq!(skills2, 3);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn parseja_agent_amb_frontmatter() {
        let raw = "---\nname: architect\ndescription: Rol curt\nmodel: opus\n---\n\nCos del prompt.\n";
        let (name, desc, body) = parse_ecc_md(raw, "fitxer");
        assert_eq!(name, "architect");
        assert_eq!(desc, "Rol curt");
        assert_eq!(body, "Cos del prompt.");
    }

    #[test]
    fn parseja_md_sense_frontmatter() {
        let raw = "# Títol\nContingut sense frontmatter.\n";
        let (name, desc, body) = parse_ecc_md(raw, "fitxer");
        assert_eq!(name, "fitxer");
        assert_eq!(desc, "");
        assert!(body.starts_with("# Títol"));
    }
}