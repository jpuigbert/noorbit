//! Tests d'integració del TERMINAL PTY REAL (Fase 3b) amb pseudoterminals
//! de veritat en directoris temporals: arrencar la shell de l'usuari,
//! escriure `echo hola\n` per l'stdin, esperar «hola» a la sortida i matar
//! la sessió. L'estat és global (registre de sessions): els tests es
//! serialitzen amb un mutex, com a snapshot_git.rs.
//!
//! Sense AppHandle els events Tauri no s'emeten (l'emitter és absent), però
//! el buffer circular sí que s'omple: és el que els tests inspeccionen.

use no_orbit_lib::commands::term;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

static GUARD: Mutex<()> = Mutex::new(());

fn guard() -> MutexGuard<'static, ()> {
    GUARD.lock().unwrap_or_else(|e| e.into_inner())
}

/// Espera que el buffer de la sessió continga `needle` (fins a 10 s).
fn wait_output(id: &str, needle: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(buf) = term::term_scrollback(id.to_string()) {
            if buf.contains(needle) {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn pty_echo_hola_i_kill() {
    let _g = guard();
    let dir = tempfile::tempdir().expect("tempdir");
    let id = term::spawn_session(dir.path().to_str().unwrap(), 80, 24)
        .expect("la sessió PTY ha d'arrencar");

    // La sessió apareix al llistat amb un títol shell · carpeta.
    let list = term::term_list();
    assert!(list.iter().any(|s| s.id == id));
    assert!(list.iter().any(|s| s.id == id && s.title.contains("·")));

    // stdin REAL: la shell executa la comanda i la sortida torna pel PTY.
    term::term_write(id.clone(), "echo hola\n".into()).expect("escriptura stdin");
    let ok = wait_output(&id, "hola");

    // Resize no ha de fallar amb la sessió viva (SIGWINCH).
    let resize = term::term_resize(id.clone(), 120, 40);

    // Kill net: la sessió marxa del registre i no queden fills.
    term::kill_all();
    let buida = term::term_list().is_empty();

    assert!(ok, "«hola» no ha aparegut a la sortida del PTY");
    resize.expect("resize");
    assert!(buida, "el registre ha de quedar buit després del kill");
}

#[test]
fn pty_cwd_i_exit_natural() {
    let _g = guard();
    let dir = tempfile::tempdir().expect("tempdir");
    // macOS: /var → /private/var; comparem amb el camí canònic.
    let canonical = std::fs::canonicalize(dir.path()).expect("canonicalize");
    let id = term::spawn_session(dir.path().to_str().unwrap(), 80, 24).expect("spawn");

    term::term_write(id.clone(), "pwd -P\n".into()).expect("write pwd");
    let cwd_ok = wait_output(&id, canonical.to_str().unwrap());

    // `exit` mata la shell: el fil lector ha de treure la sessió del
    // registre tot sol (EOF → wait → fora), sense cap term_kill.
    term::term_write(id.clone(), "exit\n".into()).expect("write exit");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut gone = false;
    while Instant::now() < deadline {
        if !term::term_list().iter().any(|s| s.id == id) {
            gone = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    term::kill_all(); // xarxa de seguretat si l'exit no ha arribat

    assert!(cwd_ok, "pwd -P no mostra el cwd del tempdir");
    assert!(gone, "la sessió ha de desaparèixer del registre en morir la shell");
}

#[test]
fn pty_limit_de_sessions() {
    let _g = guard();
    let dir = tempfile::tempdir().expect("tempdir");
    let cwd = dir.path().to_str().unwrap();
    let mut ids = Vec::new();
    for _ in 0..8 {
        ids.push(term::spawn_session(cwd, 80, 24).expect("spawn fins al límit"));
    }
    // La novena ha de fallar amb un missatge clar en català.
    let err = term::spawn_session(cwd, 80, 24).unwrap_err();
    assert!(err.contains("Límit"), "error inesperat: {err}");
    term::kill_all();
    assert!(term::term_list().is_empty());
    // Després del kill es pot tornar a obrir.
    let id = term::spawn_session(cwd, 80, 24).expect("spawn després del kill");
    term::kill_all();
    assert!(!id.is_empty());
}

#[test]
fn pty_operacions_sobre_sessio_inexistent() {
    let _g = guard();
    assert!(term::term_write("pty-fantasma".into(), "x".into()).is_err());
    assert!(term::term_resize("pty-fantasma".into(), 80, 24).is_err());
    assert!(term::term_scrollback("pty-fantasma".into()).is_err());
    // term_list no falla mai: retorna el que hi ha (ací, res).
    let _ = term::term_list();
}