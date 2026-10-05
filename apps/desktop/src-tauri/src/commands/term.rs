//! Execució de comandes dins el projecte obert per al terminal integrat.
//!
//! Cada comanda rep un `callId` (escollit per la UI) i la seva sortida es
//! va enviant per trossos amb l'event `term://out`, així el terminal mostra
//! el que produeix una comanda llarga (python, npm…) sense esperar que acabi.

use serde::Serialize;
use std::io::Read;
use std::process::{Command, Stdio};
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
        let mut child = Command::new(shell.0)
            .arg(shell.1)
            .arg(&cmd)
            .current_dir(&root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("No s'ha pogut executar: {}", e))?;

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

        let status = child.wait().map_err(|e| e.to_string())?;
        let _ = app.emit(
            "term://exit",
            TermExit {
                call_id,
                code: status.code().unwrap_or(-1),
            },
        );
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
