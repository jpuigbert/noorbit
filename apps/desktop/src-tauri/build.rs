use std::{env, fs};
use std::path::{Path, PathBuf};

/// Recorre `skills-builtin/` i genera una taula estàtica amb tots els
/// fitxers (ruta relativa, contingut) perquè quedin incrustats al binari.
fn collect_files(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, base, out);
        } else if path.extension().map(|e| e == "md").unwrap_or(false) {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let content = fs::read_to_string(&path).unwrap_or_default();
            out.push((rel, content));
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=skills-builtin");
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("skills-builtin");
    let mut files = Vec::new();
    collect_files(&root, &root, &mut files);
    files.sort();

    let mut body =
        String::from("pub static BUILTIN_SKILL_FILES: &[(&str, &str)] = &[\n");
    for (rel, content) in &files {
        body.push_str(&format!("  ({:?}, {:?}),\n", rel, content));
    }
    body.push_str("];\n");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("builtin_skills.rs"), body).expect("no s'ha pogut escriure builtin_skills.rs");

    tauri_build::build()
}
