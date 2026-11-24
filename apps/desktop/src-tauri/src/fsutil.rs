//! Utilitats de fitxers compartides (motors de joc, espais de treball).

use std::path::Path;

/// Afegeix línies al `.gitignore` d'una carpeta SENSE esborrar les que hi ha.
///
/// Si el projecte viu a l'arrel d'un workspace amb un `.gitignore` propi
/// (node_modules, build…), sobreescriure'l seria destructiu: només hi
/// afegim les línies que falten, i si ja hi són no hi tornem a escriure.
pub fn append_gitignore(dir: &Path, lines: &[&str]) -> Result<(), String> {
    let path = dir.join(".gitignore");
    let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut changed = false;
    for line in lines {
        let wanted = line.trim();
        if wanted.is_empty() || existing.lines().any(|l| l.trim() == wanted) {
            continue;
        }
        if !existing.is_empty() && !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str(wanted);
        existing.push('\n');
        changed = true;
    }
    if changed {
        std::fs::write(&path, existing)
            .map_err(|e| format!("no s'ha pogut escriure el .gitignore: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn afegeix_sense_pisar() {
        let dir = std::env::temp_dir().join(format!("noorbit-fsutil-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".gitignore"), "node_modules\n").unwrap();
        append_gitignore(&dir, &[".godot/"]).unwrap();
        append_gitignore(&dir, &[".godot/"]).unwrap();
        let t = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
        assert!(t.contains("node_modules"), "{t}");
        assert_eq!(t.matches(".godot/").count(), 1, "{t}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
