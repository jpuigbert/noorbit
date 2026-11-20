//! Resposta de la IA nativa a les preguntes sobre EL PROJECTE OBERT (el
//! workspace actual), no sobre l'aplicació NoOrbit.
//!
//! Abans d'existir aquest mòdul, preguntar «què fa aquest projecte» coincidia
//! amb l'entrada `noorbit` del cervell (`native.rs`) —perquè els seus
//! *triggers* inclouen «projecte», «què és» i «funciona»— i la nativa responia
//! la descripció de l'IDE, no una anàlisi del que l'usuari té obert. El motor
//! simbòlic no inventa prosa, però SÍ que pot llegir els fitxers reals del
//! workspace i descriure'ls amb honestetat: nom, pila tecnològica detectada,
//! fitxers clau, resum del README i si el projecte és buit.
//!
//! Va al pipeline de `answer_or_none` ABANS del cervell (com el `wander::
//! informe_demana`), perquè cap pregunta sobre el projecte no sigui segrestada
//! per la FAQ genèrica de NoOrbit.

use std::path::Path;

/// Carpetes que mai no s'exploren: dependències, build i cachés no defineixen
/// «què fa» el projecte i només el poden ofuscar.
const SKIP_DIRS: &[&str] = &[
    "node_modules", "vendor", ".git", "target", "build", "dist", ".cache", "__pycache__",
    ".venv", "venv", ".godot", "Library", "Temp", "Logs", "Saved", "Intermediate",
];

/// Paraules (normalitzades, sense accents) que indiquen que es parla del
/// projecte obert, no de NoOrbit.
const PROJECT_WORDS: &[&str] = &["projecte", "worksp", "repositori", "carpeta de treball"];

/// Verbs i girs que demanen una DESCRIPCIÓ del projecte (què fa, com està,
/// què hi ha, què falta…).
const INQUIRY_WORDS: &[&str] = &[
    "que fa", "que fan", "que hauria de fer", "de que va", "de que tracta", "explica",
    "resum", "que hi ha", "estructura", "com esta", "estat", "que falta", "que inclou",
    "que conte", "sobre el projecte", "aquest projecte", "este projecte", "projecte actual",
    "estic treballant", "projecte que", "del projecte", "al projecte", "que te", "que porta",
];

/// Extrau la pregunta real del prompt del xat: el text darrere de
/// «PREGUNTA / TASCA:» si hi és (el tram anterior és el context del projecte,
/// que conté «projecte» milers de vegades i falsejaria la detecció); si no,
/// tot el prompt.
pub(crate) fn question_of(prompt: &str) -> String {
    match prompt.rfind("PREGUNTA / TASCA:") {
        Some(idx) => prompt[idx + "PREGUNTA / TASCA:".len()..].to_string(),
        None => prompt.to_string(),
    }
}

/// La pregunta es refereix al projecte obert (i no a NoOrbit mateix)?
pub fn is_project_query(prompt: &str) -> bool {
    let low = crate::ai::codegen::pla(&question_of(prompt));
    // Una pregunta que anomena NoOrbit vol saber què és l'app, no el projecte.
    if low.contains("noorbit") {
        return false;
    }
    let parla_de_projecte = PROJECT_WORDS.iter().any(|w| low.contains(w));
    let demana_descripcio = INQUIRY_WORDS.iter().any(|w| low.contains(w));
    parla_de_projecte && demana_descripcio
}

/// Resposta completa (detecció + anàlisi real del workspace), o `None` si la
/// pregunta no és sobre el projecte obert.
pub fn answer(prompt: &str) -> Option<String> {
    if !is_project_query(prompt) {
        return None;
    }
    Some(match crate::commands::workspace::current_root() {
        Some(root) => describe_root(&root),
        None => NO_PROJECT.to_string(),
    })
}

const NO_PROJECT: &str = "Ara mateix no hi ha cap projecte obert a NoOrbit, així que no \
     puc dir-te què fa. Obre la carpeta del projecte (Fitxer → Obre carpeta, o el botó \
     «Obre projecte») i torna-m'ho a demanar: llavors llegiré els seus fitxers reals i \
     t'explicaré què conté, amb què està fet i què hi falta.";

/// Llegeix el workspace i descriu el projecte amb dades reals.
pub fn describe_root(root: &Path) -> String {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "el projecte obert".to_string());

    // 1) Recull fitxers rellevants (sense dependències ni build), amb límits
    //    perquè la resposta sigui ràpida encara que el disc estigui ple.
    let mut total_files: usize = 0;
    let mut by_ext: std::collections::BTreeMap<String, usize> = Default::default();
    collect(root, 0, &mut total_files, &mut by_ext);

    if total_files == 0 {
        return format!(
            "El projecte obert és «{name}», però de moment no hi he trobat cap fitxer de \
             codi o documentació llegible. Està buit o només en conté carpetes de \
             dependències/build. Quan hi afegeixis fitxers (o me'n demanis crear-los amb \
             les meves plantilles), et podré dir què fa."
        );
    }

    // 2) Pila tecnològica: la dedueixo de fitxers Senya i extensions reals.
    let stack = detect_stack(root, &by_ext);

    // 3) Resum del README o de la descripció de package.json/Cargo.toml.
    let readme = read_summary(root);

    // 4) Fitxers clau (manifestos i entrades que existeixen a l'arrel).
    let key = key_files();

    let mut out = String::new();
    out.push_str(&format!("El projecte obert és «{name}».\n"));
    if !stack.is_empty() {
        out.push_str(&format!("Està fet amb: {}.\n", stack.join(", ")));
    }
    // Extensió amb més presència, per donar una idea del llenguatge dominant.
    let mut exts: Vec<(String, usize)> = by_ext.into_iter().collect();
    exts.sort_by(|a, b| b.1.cmp(&a.1));
    if !exts.is_empty() {
        let top: Vec<String> = exts
            .iter()
            .take(5)
            .map(|(e, n)| format!(".{e} ({n})"))
            .collect();
        out.push_str(&format!(
            "Hi ha {total_files} fitxers de codi/text, dominats per: {}.\n",
            top.join(", ")
        ));
    }
    if !key.is_empty() {
        out.push_str(&format!("Fitxers clau: {}.\n", key.join(", ")));
    }
    if let Some(r) = readme {
        out.push_str(&format!("El que hi ha escrit al projecte diu: «{r}».\n"));
    }
    out.push_str(
        "\n\nSóc la IA nativa (motor simbòlic): puc llegir i resumir el que HI HA, però no \
         deduir-ne la intenció com un LLM. Si vols una explicació més lliure del que FA \
         aquest projecte, engega Ollama o connecta un proveïdor i torna-m'ho a demanar. \
         Per contra, puc crear fitxers nous, editar-los o esborrar-los si em dius \
         exactament què canviar.",
    );
    out
}

/// Afegeix a `total` el nombre de fitxers de text/codi i a `by_ext` el
/// recompte per extensió. Profunditat i volum limitats.
fn collect(
    dir: &Path,
    depth: usize,
    total: &mut usize,
    by_ext: &mut std::collections::BTreeMap<String, usize>,
) {
    if depth > 4 || *total >= 400 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if *total >= 400 {
            return;
        }
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                continue;
            }
            collect(&p, depth + 1, total, by_ext);
        } else if p.is_file() {
            let ext = p
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !is_text_ext(&ext) {
                continue;
            }
            *total += 1;
            if !ext.is_empty() {
                *by_ext.entry(ext).or_insert(0) += 1;
            }
        }
    }
}

/// Extensions considerades codi/documentació (les que defineixen el projecte).
fn is_text_ext(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "py" | "rb" | "go" | "java" | "kt"
            | "swift" | "c" | "h" | "cpp" | "hpp" | "cs" | "php" | "html" | "htm" | "css" | "scss"
            | "vue" | "svelte" | "json" | "toml" | "yaml" | "yml" | "md" | "markdown" | "txt"
            | "sql" | "sh" | "gd" | "gdshader" | "tscn" | "tres" | "uproject" | "ini" | "xml"
    )
}

/// Noms de fitxers clau (arrels de projecte, entrades, manifestos) que mereixen
/// citar-se a la resposta.
fn key_files() -> Vec<String> {
    let marks = [
        "package.json", "Cargo.toml", "pyproject.toml", "requirements.txt", "index.html",
        "main.py", "main.rs", "App.tsx", "app.tsx", "index.ts", "index.js", "README.md",
        "project.godot", ".uproject", "style.css", "functions.php", "wp-config.php",
        "docker-compose.yml", "Makefile", "go.mod",
    ];
    // Per no dependre d'una llista de tots els fitxers, buscant-los directament a l'arrel.
    let root = crate::commands::workspace::current_root();
    let mut out = Vec::new();
    if let Some(r) = root {
        for m in marks {
            if r.join(m).exists() {
                out.push(m.to_string());
            } else {
                // Alguns marcadors (p. ex. .uproject) poden tenir un nom variable.
                if m == ".uproject" {
                    if let Ok(rd) = std::fs::read_dir(&r) {
                        for e in rd.flatten() {
                            if e.path().extension().map(|x| x == "uproject").unwrap_or(false) {
                                out.push(e.file_name().to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Dedueix la pila tecnològica de fitxers Senya i extensions.
fn detect_stack(root: &Path, by_ext: &std::collections::BTreeMap<String, usize>) -> Vec<String> {
    let mut s = Vec::new();
    if root.join("Cargo.toml").exists() {
        s.push("Rust".to_string());
    }
    if root.join("package.json").exists() {
        let pj = std::fs::read_to_string(root.join("package.json")).unwrap_or_default();
        let low = pj.to_lowercase();
        let framework = if low.contains("\"react\"") {
            "React"
        } else if low.contains("\"vue\"") {
            "Vue"
        } else if low.contains("\"svelte\"") {
            "Svelte"
        } else if low.contains("\"@angular") {
            "Angular"
        } else if low.contains("\"next\"") {
            "Next.js"
        } else {
            "Node/JavaScript"
        };
        s.push(framework.to_string());
    }
    if root.join("pyproject.toml").exists() || root.join("requirements.txt").exists() {
        s.push("Python".to_string());
    }
    if root.join("go.mod").exists() {
        s.push("Go".to_string());
    }
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            if e.path().extension().map(|x| x == "uproject").unwrap_or(false) {
                s.push("Unreal Engine".to_string());
                break;
            }
        }
    }
    if root.join("project.godot").exists() {
        s.push("Godot".to_string());
    }
    if let Ok(css) = std::fs::read_to_string(root.join("style.css")) {
        if css.contains("Theme Name:") {
            s.push("Tema de WordPress".to_string());
        }
    }
    // Compensa els marcadors anteriors amb extensions.
    if s.is_empty() {
        let guess = [
            ("rs", "Rust"),
            ("py", "Python"),
            ("php", "PHP"),
            ("go", "Go"),
            ("java", "Java"),
            ("swift", "Swift"),
            ("tsx", "TypeScript/React"),
            ("ts", "TypeScript"),
            ("js", "JavaScript"),
            ("gd", "GDScript (Godot)"),
        ];
        for (ext, label) in guess {
            if by_ext.get(ext).map(|n| *n > 0).unwrap_or(false) {
                s.push(label.to_string());
            }
        }
    }
    // Deduplica conservant l'ordre.
    let mut seen = std::collections::HashSet::new();
    s.retain(|x| seen.insert(x.clone()));
    s
}

/// Primeres línies del README o la «description» de package.json/Cargo.toml,
/// retallades per cabre en una frase.
fn read_summary(root: &Path) -> Option<String> {
    for cand in ["README.md", "readme.md", "README.txt", "README"] {
        if let Ok(txt) = std::fs::read_to_string(root.join(cand)) {
            let clean: String = txt
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("<"))
                .collect::<Vec<_>>()
                .join(" ");
            if clean.chars().count() > 20 {
                return Some(clean.chars().take(400).collect());
            }
        }
    }
    if let Ok(pj) = std::fs::read_to_string(root.join("package.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&pj) {
            if let Some(d) = v.get("description").and_then(|x| x.as_str()) {
                if !d.trim().is_empty() {
                    return Some(d.chars().take(300).collect());
                }
            }
        }
    }
    if let Ok(ct) = std::fs::read_to_string(root.join("Cargo.toml")) {
        for line in ct.lines() {
            let l = line.trim();
            if let Some(rest) = l.strip_prefix("description") {
                if let Some(q) = rest.split('"').nth(1) {
                    if !q.trim().is_empty() {
                        return Some(q.chars().take(300).collect());
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod proves {
    use super::*;

    #[test]
    fn detecta_pregunta_sobre_el_projecte_obert() {
        assert!(is_project_query("què fa el projecte que estic treballant?"));
        assert!(is_project_query("Què fa aquest projecte?"));
        assert!(is_project_query("fes un resum del projecte"));
        assert!(is_project_query("de què va aquest projecte"));
        assert!(is_project_query("què hi ha al projecte"));
    }

    #[test]
    fn no_segresta_preguntes_sobre_noorbit_i_tasques() {
        // Pregunta sobre l'app: ha de respondre el cervell (noorbit), no això.
        assert!(!is_project_query("què és noorbit?"));
        assert!(!is_project_query("com funciona noorbit"));
        // Tasques que no demanen descripció del projecte.
        assert!(!is_project_query("crea un component React"));
        assert!(!is_project_query("calcula 2+2"));
    }

    #[test]
    fn la_pregunta_s_isola_del_context() {
        // El context del projecte (abans del marcador) va ple de «projecte»:
        // la detecció només mira la pregunta final.
        let amb_ctx = format!(
            "CONTEXT DEL PROJECTE\n@file: projecte/a.rs\nprojecte projecte projecte\n\n---\nPREGUNTA / TASCA:\n{}",
            "hola"
        );
        assert!(!is_project_query(&amb_ctx));
    }

    #[test]
    fn text_ext_conegudes() {
        assert!(is_text_ext("rs"));
        assert!(is_text_ext("tsx"));
        assert!(!is_text_ext("png"));
        assert!(!is_text_ext(""));
    }
}
