//! Tests d'integració del SNAPSHOTS GIT SHADOW (Fase 1d) amb git REAL en
//! directoris temporals: escriure → snapshot → modificar → undo → el
//! contingut original torna; pila buida → error clar; node_modules/target/
//! dist/.noorbit queden exclosos; el git-dir viu FORA del projecte.
//!
//! L'estat del mòdul és global (piles en memòria): els tests es serialitzen
//! amb un mutex i cadascun inicialitza el seu propi directori de dades.

use no_orbit_lib::agent::snapshot;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

static GUARD: Mutex<()> = Mutex::new(());

fn guard() -> MutexGuard<'static, ()> {
    GUARD.lock().unwrap_or_else(|e| e.into_inner())
}

fn git_available() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn tmp_dir(tag: &str) -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("noorbit-snap-test-{tag}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap()
}

struct Sandbox {
    data: PathBuf,
    proj: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let data = tmp_dir(&format!("{tag}-data"));
        let proj = tmp_dir(&format!("{tag}-proj"));
        snapshot::init(data.clone());
        Self { data, proj }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.data);
        let _ = std::fs::remove_dir_all(&self.proj);
    }
}

#[test]
fn undo_restaura_el_contingut_original_i_esborra_els_fitxers_nous() {
    let _g = guard();
    if !git_available() {
        eprintln!("git no disponible: test omès");
        return;
    }
    let sb = Sandbox::new("restore");
    write(&sb.proj, "a.txt", "original\n");
    write(&sb.proj, "sub/b.txt", "b original\n");

    snapshot::take(&sb.proj).unwrap();
    assert_eq!(snapshot::count(&sb.proj), 1);
    // El shadow NO és dins del projecte i no li ha creat cap .git.
    assert!(!sb.proj.join(".git").exists());
    assert!(sb.data.join("snapshots").is_dir());

    // L'agent modifica, esborra i crea fitxers nous.
    write(&sb.proj, "a.txt", "MODIFICAT per l'agent\n");
    std::fs::remove_file(sb.proj.join("sub/b.txt")).unwrap();
    write(&sb.proj, "nou.txt", "brossa nova\n");
    write(&sb.proj, "carpeta_nova/c.txt", "més brossa\n");
    // L'agent REGISTRA les rutes que ha escrit (com fa runtools en producció):
    // només eixes s'esborraran en desfer.
    snapshot::record_written(&sb.proj, &["nou.txt".to_string(), "carpeta_nova/c.txt".to_string()]);

    let paths = snapshot::undo(&sb.proj).unwrap();
    assert!(paths.contains(&"a.txt".to_string()), "rutes: {paths:?}");
    assert!(paths.contains(&"sub/b.txt".to_string()), "rutes: {paths:?}");
    assert!(paths.iter().any(|p| p.starts_with("nou.txt")), "rutes: {paths:?}");
    assert!(paths.iter().any(|p| p.starts_with("carpeta_nova")), "rutes: {paths:?}");
    assert_eq!(read(&sb.proj, "a.txt"), "original\n");
    assert_eq!(read(&sb.proj, "sub/b.txt"), "b original\n");
    assert!(!sb.proj.join("nou.txt").exists());
    assert!(!sb.proj.join("carpeta_nova").exists());
    assert_eq!(snapshot::count(&sb.proj), 0);
}

#[test]
fn pila_buida_error_clar() {
    let _g = guard();
    if !git_available() {
        eprintln!("git no disponible: test omès");
        return;
    }
    let sb = Sandbox::new("empty");
    write(&sb.proj, "a.txt", "hola\n");
    let err = snapshot::undo(&sb.proj).unwrap_err();
    assert!(err.contains("cap pas"), "error inesperat: {err}");
    // Un segon undo (després d'un de bo) també ha de donar l'error clar.
    snapshot::take(&sb.proj).unwrap();
    write(&sb.proj, "a.txt", "canvi\n");
    snapshot::undo(&sb.proj).unwrap();
    let err2 = snapshot::undo(&sb.proj).unwrap_err();
    assert!(err2.contains("cap pas"), "error inesperat: {err2}");
}

#[test]
fn exclou_node_modules_target_dist_i_noorbit() {
    let _g = guard();
    if !git_available() {
        eprintln!("git no disponible: test omès");
        return;
    }
    let sb = Sandbox::new("excludes");
    write(&sb.proj, "a.txt", "original\n");
    write(&sb.proj, "node_modules/paquet/index.js", "v1\n");
    write(&sb.proj, "target/debug/binari", "v1\n");
    write(&sb.proj, "dist/bundle.js", "v1\n");
    write(&sb.proj, ".noorbit/config", "v1\n");

    snapshot::take(&sb.proj).unwrap();
    // Canvis DESPRÉS del snapshot: la brossa exclouida no es captura ni es
    // restaura (no forma part del tree).
    write(&sb.proj, "a.txt", "modificat\n");
    write(&sb.proj, "node_modules/paquet/index.js", "v2\n");
    write(&sb.proj, "target/debug/binari", "v2\n");
    write(&sb.proj, "dist/bundle.js", "v2\n");
    write(&sb.proj, ".noorbit/config", "v2\n");

    let paths = snapshot::undo(&sb.proj).unwrap();
    assert_eq!(paths, vec!["a.txt".to_string()]);
    assert_eq!(read(&sb.proj, "a.txt"), "original\n");
    assert_eq!(read(&sb.proj, "node_modules/paquet/index.js"), "v2\n");
    assert_eq!(read(&sb.proj, "target/debug/binari"), "v2\n");
    assert_eq!(read(&sb.proj, "dist/bundle.js"), "v2\n");
    assert_eq!(read(&sb.proj, ".noorbit/config"), "v2\n");
}

#[test]
fn take_dedup_i_projecte_sense_git() {
    let _g = guard();
    if !git_available() {
        eprintln!("git no disponible: test omès");
        return;
    }
    let sb = Sandbox::new("dedup");
    // El projecte NO és un repo git: el shadow ha de funcionar igualment.
    write(&sb.proj, "a.txt", "hola\n");
    assert!(!sb.proj.join(".git").exists());
    snapshot::take(&sb.proj).unwrap();
    snapshot::take(&sb.proj).unwrap(); // sense canvis → no apila
    assert_eq!(snapshot::count(&sb.proj), 1);
    write(&sb.proj, "a.txt", "canvi\n");
    snapshot::take(&sb.proj).unwrap();
    assert_eq!(snapshot::count(&sb.proj), 2);
    write(&sb.proj, "a.txt", "final\n");
    // Dos undos tornen als dos estats capturats (el darrer primer).
    snapshot::undo(&sb.proj).unwrap();
    assert_eq!(read(&sb.proj, "a.txt"), "canvi\n");
    snapshot::undo(&sb.proj).unwrap();
    assert_eq!(read(&sb.proj, "a.txt"), "hola\n");
    assert_eq!(snapshot::count(&sb.proj), 0);
}

#[test]
fn cicle_de_torn_baseline_unic_i_descart_de_prosa() {
    let _g = guard();
    if !git_available() {
        eprintln!("git no disponible: test omès");
        return;
    }
    let sb = Sandbox::new("turn");
    write(&sb.proj, "a.txt", "original\n");

    // Torn de PROSA: begin + end sense canvis → cap snapshot.
    snapshot::begin_turn(&sb.proj).unwrap();
    snapshot::end_turn(&sb.proj, false, &[]);
    assert_eq!(snapshot::count(&sb.proj), 0);

    // Torn amb escriptures: el baseline del torn cobreix els lots interns
    // (take durant el torn NO apila piles extra).
    snapshot::begin_turn(&sb.proj).unwrap();
    write(&sb.proj, "a.txt", "escrit en viu\n");
    snapshot::take(&sb.proj).unwrap(); // lot intern del backend
    write(&sb.proj, "a.txt", "definitiu\n");
    snapshot::take(&sb.proj).unwrap(); // un altre lot intern
    assert_eq!(snapshot::count(&sb.proj), 1);
    snapshot::end_turn(&sb.proj, true, &[]);
    assert_eq!(snapshot::count(&sb.proj), 1);

    // Un sol «Desfés el pas» torna a l'estat PRE-TORN (no a un intermedi).
    let paths = snapshot::undo(&sb.proj).unwrap();
    assert_eq!(paths, vec!["a.txt".to_string()]);
    assert_eq!(read(&sb.proj, "a.txt"), "original\n");
    assert_eq!(snapshot::count(&sb.proj), 0);
}

/// CRÍTIC (finding #1): un fitxer creat manualment per l'USUARI després del
/// snapshot SOBREVIVIU a l'undo: «Desfés el pas» només esborra els untracked
/// que l'agent va escriure (registered), mai la feina manual de l'usuari.
#[test]
fn fitxer_de_lusuari_creat_despres_del_snapshot_sobreviu_a_undo() {
    let _g = guard();
    if !git_available() {
        eprintln!("git no disponible: test omès");
        return;
    }
    let sb = Sandbox::new("user-survives");
    write(&sb.proj, "base.txt", "original\n");
    snapshot::take(&sb.proj).unwrap();

    // L'agent crea un fitxer nou i el registra.
    write(&sb.proj, "agent.txt", "brossa de l'agent\n");
    snapshot::record_written(&sb.proj, &["agent.txt".to_string()]);
    // L'USUARI crea manualment un altre fitxer (NO registrat) i una carpeta
    // amb contingut propi després del snapshot.
    write(&sb.proj, "manual_usuari.txt", "feina meva, no esborrar\n");
    write(&sb.proj, "carpeta_usuari/notes.md", "apunts personals\n");

    let paths = snapshot::undo(&sb.proj).unwrap();
    // L'agent.txt s'esborra; els de l'usuari NO apareixen com a esborrats.
    assert!(paths.iter().any(|p| p == "agent.txt"), "rutes: {paths:?}");
    assert!(!paths.iter().any(|p| p.contains("usuari")), "rutes: {paths:?}");
    assert!(!sb.proj.join("agent.txt").exists(), "agent.txt hauria d'estar esborrat");
    // La feina de l'usuari està INTACTA (pèrdua de dades irreversible evitada).
    assert_eq!(read(&sb.proj, "manual_usuari.txt"), "feina meva, no esborrar\n");
    assert_eq!(read(&sb.proj, "carpeta_usuari/notes.md"), "apunts personals\n");
    assert_eq!(read(&sb.proj, "base.txt"), "original\n");
}

/// MITJÀ (finding #2): rutes amb accents catalans (abans C-quoted amb
/// `--porcelain` sense `-z`) i renames «old -> new» es parsegen bé amb `-z`:
/// l'undo restaura el contingut accentuat i esborra l'untracked accentuat de
/// l'agent, conservant el de l'usuari.
#[test]
fn undo_amb_noms_accentuats_i_rename() {
    let _g = guard();
    if !git_available() {
        eprintln!("git no disponible: test omès");
        return;
    }
    let sb = Sandbox::new("accents");
    write(&sb.proj, "cançó_à.txt", "lletra original\n");
    write(&sb.proj, "sub/èpica.txt", "èpica original\n");
    snapshot::take(&sb.proj).unwrap();

    // L'agent modifica un fitxer accentuat (tracked) i crea un untracked
    // accentuat que registra; l'usuari crea un altre untracked accentuat.
    write(&sb.proj, "cançó_à.txt", "lletra MODIFICADA\n");
    write(&sb.proj, "nou_ó_agent.txt", "brossa accentuada\n");
    snapshot::record_written(&sb.proj, &["nou_ó_agent.txt".to_string()]);
    write(&sb.proj, "manual_ú_usuari.txt", "accent de l'usuari\n");
    // Rename d'un fitxer tracked (el parseig -z ha de consumir el camp origen).
    std::fs::rename(sb.proj.join("sub/èpica.txt"), sb.proj.join("sub/èpica_renombrada.txt")).unwrap();

    let paths = snapshot::undo(&sb.proj).unwrap();
    // El contingut accentuat original torna (restore_tree amb -z).
    assert_eq!(read(&sb.proj, "cançó_à.txt"), "lletra original\n");
    assert_eq!(read(&sb.proj, "sub/èpica.txt"), "èpica original\n");
    // L'untracked accentuat de l'agent s'esborra; el de l'usuari sobreviu.
    assert!(!sb.proj.join("nou_ó_agent.txt").exists(), "agent accentuat no esborrat");
    assert_eq!(read(&sb.proj, "manual_ú_usuari.txt"), "accent de l'usuari\n");
    assert!(paths.iter().any(|p| p == "cançó_à.txt"), "rutes: {paths:?}");
    assert!(paths.iter().any(|p| p == "nou_ó_agent.txt"), "rutes: {paths:?}");
}