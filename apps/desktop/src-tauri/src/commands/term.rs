//! Execució de comandes dins el projecte obert per al terminal integrat.
//!
//! Cada comanda rep un `callId` (escollit per la UI) i la seva sortida es
//! va enviant per trossos amb l'event `term://out`, així el terminal mostra
//! el que produeix una comanda llarga (python, npm…) sense esperar que acabi.
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
