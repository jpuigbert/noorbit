//! Execució de comandes dins el projecte obert per al terminal integrat.
//!
//! DUES generacions conviuen en este mòdul:
//!
//! 1. ONE-SHOT (legacy): `run_command`/`kill_command` executen `sh -c`
//!    sense stdin, amb timeout de 600 s, i emeten `term://out` amb
//!    `{ call_id, kind, text }` + `term://exit` amb `{ call_id, code }`.
//!    Es mantenen registrats i funcionals per compatibilitat.
//! 2. PTY REAL MULTI-SESSIÓ (Fase 3b): `term_start`/`term_write`/
//!    `term_resize`/`term_kill`/`term_list` (+ `term_scrollback` per al
//!    re-attach de la finestra separada) sobre `portable-pty`. Emeten
//!    `term://out` amb `{ id, data }` i `term://exit` amb
//!    `{ id, code: Option<i32> }`. Els consumidors es distingeixen pel
//!    camp del payload (`call_id` vs `id`): cap xoc entre generacions.
//!
//! Cada comanda one-shot rep un `callId` (escollit per la UI) i la seva
//! sortida es va enviant per trossos amb l'event `term://out`.
//!
//! Les comandes EN EXECUCIÓ es poden aturar («term_kill»): es desen en un
//! registre global amb el seu PID i el seu grup de processos. Abans no hi
//! havia manera d'aturar una comanda: els fils de lectura i els processos
//! fills (servidors, `npm run dev`…) quedaven viu per sempre.

use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use tauri::{command, Emitter, AppHandle};

#[derive(Clone, Serialize)]
struct TermOut {
    call_id: String,
    kind: String, // "out" | "err"
    text: String,
}

#[derive(Clone, Serialize)]
struct TermExit {
    call_id: String,
    code: i32,
}

/// PID de les comandes que ara mateix estan corrent (per aturar-les).
static RUNNING: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();

/// Temps límit d'una comanda del terminal (10 min): passat aquest temps es
/// mata el grup de processos i el terminal es queda lliure.
const MAX_CMD_SECS: u64 = 600;

fn running() -> &'static Mutex<HashMap<String, u32>> {
    RUNNING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Executa `cmd` amb la shell al directori del projecte obert.
/// Emet `term://out` per cada bloc de sortida i `term://exit` en acabar.
#[command]
pub async fn run_command(app: AppHandle, cmd: String, call_id: String) -> Result<(), String> {
    let cmd = cmd.trim().to_string();
    if cmd.is_empty() {
        return Err("Comanda buida.".into());
    }
    let root = super::workspace::current_root()
        .ok_or_else(|| "No hi ha cap projecte obert.".to_string())?;

    tokio::task::spawn_blocking(move || {
        let shell = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };
        let mut command = Command::new(shell.0);
        command
            .arg(shell.1)
            .arg(&cmd)
            .current_dir(&root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Grup de processos propi: en Unix permet matar TAMBÉ els fills
        // (servidors, compiladors) en aturar la comanda.
        #[cfg(unix)]
        unsafe {
            use std::os::unix::process::CommandExt;
            command.pre_exec(|| {
                // setpgid(0, 0): el procés nou és líder del seu grup.
                if libc::setpgid(0, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("No s'ha pogut executar: {}", e))?;
        let pid = child.id();
        running()
            .lock()
            .map(|mut m| {
                m.insert(call_id.clone(), pid);
            })
            .ok();

        let stdout = child.stdout.take().expect("stdout");
        let stderr = child.stderr.take().expect("stderr");

        // Un fil per corrent: llegeix per trossos i els emet mentre avança.
        let emit = |stream: &'static str, mut reader: Box<dyn Read + Send>| {
            let app2 = app.clone();
            let cid = call_id.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let text = String::from_utf8_lossy(&buf[..n]).to_string();
                            let _ = app2.emit(
                                "term://out",
                                TermOut {
                                    call_id: cid.clone(),
                                    kind: stream.to_string(),
                                    text,
                                },
                            );
                        }
                    }
                }
            });
        };
        emit("out", Box::new(stdout));
        emit("err", Box::new(stderr));

        // Espera amb TEMPS LÍMIT: una comanda que no acaba (un servidor, un
        // `tail -f`, un build penjat) ocupava el procés per sempre fins que
        // l'usuari la matés a mà. Passat el límit, es mata el grup.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(MAX_CMD_SECS);
        let status = loop {
            match child.try_wait() {
                Ok(Some(st)) => break Some(st),
                Ok(None) => {}
                Err(e) => return Err(e.to_string()),
            }
            if std::time::Instant::now() >= deadline {
                kill_group(child.id());
                let _ = child.wait();
                let _ = app.emit(
                    "term://exit",
                    TermExit {
                        call_id: call_id.clone(),
                        code: -2,
                    },
                );
                if let Ok(mut m) = running().lock() {
                    m.remove(&call_id);
                }
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(120));
        };
        // Ja no està en execució: es treu del registre.
        if let Ok(mut m) = running().lock() {
            m.remove(&call_id);
        }
        let _ = app.emit(
            "term://exit",
            TermExit {
                call_id,
                code: status.and_then(|s| s.code()).unwrap_or(-1),
            },
        );
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Mata el GRUP de processos d'un PID (Unix) o l'arbre (Windows).
fn kill_group(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
        // Si el grup no existeix, es mata el procés directament.
        libc::kill(pid as i32, libc::SIGTERM);
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// Atura una comanda que està corrent (botó «Atura» del terminal).
/// En Unix mata el GRUP de processos (fills inclosos); a Windows, l'arbre.
#[command]
pub async fn kill_command(call_id: String) -> Result<(), String> {
    let pid = running()
        .lock()
        .ok()
        .and_then(|mut m| m.remove(&call_id));
    let Some(pid) = pid else {
        return Ok(()); // ja havia acabat
    };
    #[cfg(unix)]
    {
        // -pid: el grup sencer (el `kill` negatiu demana al grup).
        unsafe {
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
    }
    Ok(())
}

// ============================================================================
// FASE 3b · TERMINAL PTY REAL MULTI-SESSIÓ (portable-pty, MIT — vegeu NOTICE)
// ============================================================================
//
// Cada sessió és una shell INTERACTIVA de l'usuari dins un pseudoterminal:
// stdin real (npm init, composer, php -a funcionen), resize amb SIGWINCH i
// sortida en streaming. Un fil lector per sessió alimenta un buffer circular
// (per al re-attach de la finestra separada) i emet `term://out` { id, data }.
// Quan el procés mor (o es mata), s'emet `term://exit` { id, code } UNA sola
// vegada: qui aconsegueix TREURE la sessió del registre n'és l'amo i espera
// el codi de sortida (el fil lector en EOF, o `term_kill`/`kill_all`).

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::Write;
use std::sync::Arc;

/// Màxim de sessions PTY vives alhora (RAM i fds limitats).
const MAX_SESSIONS: usize = 8;
/// Buffer de sortida per sessió (bytes, UTF-8 lossy): el scrollback complet
/// viu a la UI (xterm, 5000 línies); ací només cal el suficient per repintar
/// una finestra que es reenganxa.
const MAX_BUFFER: usize = 64 * 1024;

struct PtySession {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    /// Escriptor PER SESSIÓ (Arc clonable): `term_write` pren el lock del
    /// mapa NOMÉS per clonar aquest Arc i escriu FORA del lock global. Així,
    /// si el buffer stdin del tty s'omple (paste gran amb un job que no
    /// llegeix), el bloqueig de `write_all`+`flush` NO congela la resta del
    /// terminal (kill/Ctrl+C d'altres sessions continuen funcionant).
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    title: String,
    buffer: Arc<Mutex<String>>,
}

static SESSIONS: OnceLock<Mutex<HashMap<String, PtySession>>> = OnceLock::new();
/// AppHandle per emetre events des del fil lector (s'instal·la amb la
/// primera comanda que en rep un; als tests d'integració és absent i els
/// events simplement no s'emeten — el buffer sí que s'omple).
static PTY_EMITTER: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();

fn sessions() -> &'static Mutex<HashMap<String, PtySession>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn set_pty_emitter(app: &AppHandle) {
    let m = PTY_EMITTER.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = m.lock() {
        if g.is_none() {
            *g = Some(app.clone());
        }
    }
}

fn emit_pty(event: &str, payload: serde_json::Value) {
    if let Some(m) = PTY_EMITTER.get() {
        if let Ok(g) = m.lock() {
            if let Some(app) = g.as_ref() {
                let _ = app.emit(event, payload);
            }
        }
    }
}

/// Afegeix sortida al buffer circular retallant per FRONTERA de caràcter
/// (un tall al mig d'un UTF-8 multi-byte corrompria el repintat).
fn push_buffer(buf: &mut String, data: &str) {
    buf.push_str(data);
    if buf.len() > MAX_BUFFER {
        let mut cut = buf.len() - MAX_BUFFER;
        while cut < buf.len() && !buf.is_char_boundary(cut) {
            cut += 1;
        }
        buf.drain(..cut);
    }
}

/// Decodifica UTF-8 amb ARRASTRE entre reads: afig `chunk` a `pending` i
/// emet NOMÉS el prefix vàlid; la cua incompleta d'un caràcter multibyte
/// (partit entre dos reads de 4096) es conserva a `pending` per al següent.
/// Els bytes genuïnament invàlids (no una cua incompleta) es substitueixen
/// per U+FFFD i es salten, perquè el flux no quede encallat per sempre.
fn decode_utf8_pending(pending: &mut Vec<u8>, chunk: &[u8]) -> String {
    pending.extend_from_slice(chunk);
    let mut out = String::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(s) => {
                out.push_str(s);
                pending.clear();
                break;
            }
            Err(e) => {
                let v = e.valid_up_to();
                out.push_str(std::str::from_utf8(&pending[..v]).unwrap_or(""));
                match e.error_len() {
                    Some(len) => {
                        out.push('\u{FFFD}');
                        pending.drain(..v + len);
                    }
                    None => {
                        // Cua incompleta: es guarda per al read següent.
                        pending.drain(..v);
                        break;
                    }
                }
            }
        }
    }
    out
}

/// Shell de l'usuari: $SHELL amb fallback /bin/zsh (i /bin/sh si el sistema
/// no té zsh, p. ex. Linux mínim). A Windows, %ComSpec%.
fn default_shell() -> String {
    #[cfg(unix)]
    {
        if let Ok(s) = std::env::var("SHELL") {
            if !s.is_empty() && std::path::Path::new(&s).exists() {
                return s;
            }
        }
        if std::path::Path::new("/bin/zsh").exists() {
            return "/bin/zsh".into();
        }
        "/bin/sh".into()
    }
    #[cfg(windows)]
    {
        std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into())
    }
}

fn basename(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string())
}

/// Arrenca una sessió PTY REAL. Públic sense AppHandle perquè els tests
/// d'integració la puguen exercir sense una app Tauri viva.
pub fn spawn_session(cwd: &str, cols: u16, rows: u16) -> Result<String, String> {
    let mut map = sessions()
        .lock()
        .map_err(|_| "Registre de sessions malmès.".to_string())?;
    if map.len() >= MAX_SESSIONS {
        return Err(format!(
            "Límit de {} sessions de terminal assolit. Tanca'n alguna abans d'obrir-ne una de nova.",
            MAX_SESSIONS
        ));
    }
    // cwd buit → projecte obert → carpeta de l'usuari.
    let dir = if cwd.trim().is_empty() {
        super::workspace::current_root()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "/".into())
            })
    } else {
        cwd.to_string()
    };
    if !std::path::Path::new(&dir).is_dir() {
        return Err(format!("La carpeta no existeix: {}", dir));
    }
    let shell = default_shell();
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("No s'ha pogut obrir el PTY: {}", e))?;
    let mut cmd = CommandBuilder::new(&shell);
    // Shell de LOGIN: carrega el PATH de l'usuari (/etc/zprofile, .zprofile…)
    // perquè npm, composer, php… es troben com al terminal del sistema.
    #[cfg(unix)]
    cmd.arg("-l");
    cmd.cwd(&dir);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("No s'ha pogut arrencar la shell: {}", e))?;
    // El slave es tanca al pare: si no, el reader no veuria mai l'EOF.
    drop(pair.slave);
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("No s'ha pogut clonar el lector del PTY: {}", e))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("No s'ha pogut obtenir l'escriptor del PTY: {}", e))?;
    let writer = Arc::new(Mutex::new(writer));
    let id = format!("pty-{}", uuid::Uuid::new_v4());
    let title = format!("{} · {}", basename(&shell), basename(&dir));
    let buffer = Arc::new(Mutex::new(String::new()));
    map.insert(
        id.clone(),
        PtySession {
            master: pair.master,
            child,
            writer,
            title,
            buffer: buffer.clone(),
        },
    );
    drop(map);

    // Fil lector: streaming cap a la UI + buffer circular per al re-attach.
    let id2 = id.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        // Arrastre UTF-8: un chunk de 4096 pot partir un caràcter multibyte
        // (català!) entre dos reads. Es decodifica NOMÉS el prefix vàlid i
        // la cua incompleta es guarda per al read següent (sense corrompre).
        let mut pending: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let text = decode_utf8_pending(&mut pending, &buf[..n]);
                    if text.is_empty() {
                        continue;
                    }
                    if let Ok(mut b) = buffer.lock() {
                        push_buffer(&mut b, &text);
                    }
                    emit_pty(
                        "term://out",
                        serde_json::json!({ "id": id2, "data": text }),
                    );
                }
            }
        }
        // EOF: si la sessió encara és al registre, la prenem (ownership) i
        // esperem el codi. Si `term_kill` ja l'havia treta, no dupliquem
        // l'event d'eixida.
        let taken = sessions().lock().ok().and_then(|mut m| m.remove(&id2));
        if let Some(mut s) = taken {
            let code = s.child.wait().ok().map(|st| st.exit_code() as i32);
            emit_pty("term://exit", serde_json::json!({ "id": id2, "code": code }));
        }
    });
    Ok(id)
}

/// Mata una sessió i emet l'exit (true si existia). Sense AppHandle: la
/// fan servir tant `term_kill` com `kill_all` com els tests.
fn kill_session(id: &str) -> bool {
    let taken = sessions().lock().ok().and_then(|mut m| m.remove(id));
    let Some(mut s) = taken else { return false };
    let _ = s.child.kill();
    let code = s.child.wait().ok().map(|st| st.exit_code() as i32);
    emit_pty("term://exit", serde_json::json!({ "id": id, "code": code }));
    true
}

/// Tanca TOTES les sessions PTY (tancament de l'app): cap shell òrfena.
pub fn kill_all() {
    let ids: Vec<String> = sessions()
        .lock()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    for id in ids {
        kill_session(&id);
    }
}

/// Obri una sessió PTY nova i en torna l'identificador.
#[command]
pub fn term_start(app: AppHandle, cwd: String, cols: u16, rows: u16) -> Result<String, String> {
    set_pty_emitter(&app);
    spawn_session(&cwd, cols, rows)
}

/// Escriu a l'stdin de la sessió (tecles de l'xterm, incloent \r i control).
#[command]
pub fn term_write(id: String, data: String) -> Result<(), String> {
    // Lock del mapa NOMÉS per clonar l'Arc de l'escriptor: la IO bloquejant
    // (write_all+flush) va FORA, perquè un stdin ple no congela kill/Ctrl+C.
    let writer = {
        let map = sessions()
            .lock()
            .map_err(|_| "Registre de sessions malmès.".to_string())?;
        map.get(&id)
            .ok_or_else(|| "Sessió de terminal inexistent.".to_string())?
            .writer
            .clone()
    };
    let mut w = writer
        .lock()
        .map_err(|_| "Escriptor del terminal malmès.".to_string())?;
    w.write_all(data.as_bytes())
        .and_then(|_| w.flush())
        .map_err(|e| format!("No s'ha pogut escriure al terminal: {}", e))
}

/// Redimensiona el PTY (SIGWINCH): vim, htop… es repinten sols.
#[command]
pub fn term_resize(id: String, cols: u16, rows: u16) -> Result<(), String> {
    let map = sessions()
        .lock()
        .map_err(|_| "Registre de sessions malmès.".to_string())?;
    let s = map
        .get(&id)
        .ok_or_else(|| "Sessió de terminal inexistent.".to_string())?;
    s.master
        .resize(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("No s'ha pogut redimensionar el terminal: {}", e))
}

/// Tanca una sessió (pestanya ×): mata la shell i espera el fill.
#[command]
pub fn term_kill(app: AppHandle, id: String) -> Result<(), String> {
    set_pty_emitter(&app);
    kill_session(&id);
    Ok(())
}

#[derive(Clone, Serialize)]
pub struct TermInfo {
    pub id: String,
    pub title: String,
}

/// Sessions vives (per sincronitzar pestanyes entre finestres).
#[command]
pub fn term_list() -> Vec<TermInfo> {
    sessions()
        .lock()
        .map(|m| {
            m.iter()
                .map(|(id, s)| TermInfo {
                    id: id.clone(),
                    title: s.title.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Buffer de sortida recent (64 KiB): el repinta la finestra que es
/// reenganxa després d'un detach. Addició al contracte base, necessària
/// perquè la sessió sobrevisca amb scrollback al canvi de finestra.
#[command]
pub fn term_scrollback(id: String) -> Result<String, String> {
    let map = sessions()
        .lock()
        .map_err(|_| "Registre de sessions malmès.".to_string())?;
    let s = map
        .get(&id)
        .ok_or_else(|| "Sessió de terminal inexistent.".to_string())?;
    Ok(s.buffer.lock().map(|b| b.clone()).unwrap_or_default())
}

#[cfg(test)]
mod proves {
    use super::decode_utf8_pending;

    /// Finding #5: un caràcter multibyte PARTIT entre dos reads es recompon
    /// sense corrompre's (abans `from_utf8_lossy` per chunk el trencava).
    #[test]
    fn multibyte_partit_entre_reads_es_recompon() {
        let mut p = Vec::new();
        let s = "cançó èpica ú ñ 中文";
        let bytes = s.as_bytes();
        // Tallem just després del PRIMER byte de 'ç' (0xC3): la cua (0xA7)
        // queda incompleta i s'ha d'arrastrar al read següent.
        let idx = s.find('ç').unwrap();
        let a = decode_utf8_pending(&mut p, &bytes[..idx + 1]);
        assert_eq!(a, "can", "prefix vàlid fins al caràcter incomplet");
        assert!(!p.is_empty(), "la cua incompleta s'ha d'arrastrar");
        let b = decode_utf8_pending(&mut p, &bytes[idx + 1..]);
        assert_eq!(format!("{a}{b}"), s, "el text complet s'ha de recompondre");
        assert!(p.is_empty(), "no ha de quedar arrastre pendent");
    }

    /// Un caràcter de 3 bytes partit en TRES reads (un byte cada vegada).
    #[test]
    fn multibyte_de_tres_bytes_partit_en_trossets() {
        let mut p = Vec::new();
        let s = "中文";
        let b = s.as_bytes();
        let a = decode_utf8_pending(&mut p, &b[..1]);
        let c = decode_utf8_pending(&mut p, &b[1..4]);
        let d = decode_utf8_pending(&mut p, &b[4..]);
        assert_eq!(format!("{a}{c}{d}"), s);
        assert!(p.is_empty());
    }

    /// Chunk vàlid sencer i bytes genuïnament invàlids (no encallar el flux).
    #[test]
    fn chunk_valid_i_bytes_invalids_substituits() {
        let mut p = Vec::new();
        assert_eq!(decode_utf8_pending(&mut p, "hola món".as_bytes()), "hola món");
        assert!(p.is_empty());
        let mut p2 = Vec::new();
        let out = decode_utf8_pending(&mut p2, &[b'a', 0xFF, b'b']);
        assert_eq!(out, "a\u{FFFD}b", "byte invàlid substituït, flux no encallat");
        assert!(p2.is_empty());
    }
}
