//! Control de l'ordinador per a la IA de NoOrbit.
//!
//! Seguretat: tot execució passa per aquest controlador, que aplica:
//!   * interruptor global "enabled" (permís de l'usuari),
//!   * "confirm_each": demana confirmació UI per cada comanda (esdeveniment
//!     "computer://confirm" + resposta amb `computer_respond`),
//!   * llista blanca (allowlist) de programes ja permesos,
//!   * blocada de comandes intrínsecament destructives (sudo, rm -rf /…),
//!   * historial persistit de tot el executat.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::Emitter;
use tokio::sync::oneshot;

const EXEC_TIMEOUT_SECS: u64 = 120;
const CONFIRM_TIMEOUT_SECS: u64 = 180;
const HISTORY_CAP: usize = 200;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Permissions {
    /// Interrruptor principal: sense això la IA no pot tocar l'ordinador.
    pub enabled: bool,
    /// Si és cert, cal confirmar cada comanda a la UI.
    pub confirm_each: bool,
    /// Comandes (per prefix) ja autoritzades permanentment per l'usuari.
    pub allowlist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub command: String,
    pub ok: bool,
    pub exit_code: i32,
    pub excerpt: String,
    pub at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecReport {
    pub command: String,
    pub ok: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

type PendingApproval = oneshot::Sender<(bool, bool)>; // (approved, remember)

pub struct ComputerController {
    path: PathBuf,
    perms: Mutex<Permissions>,
    history: Mutex<Vec<HistoryEntry>>,
    pending: Mutex<HashMap<String, PendingApproval>>,
    counter: AtomicU64,
}

/// Patrons intrinsicament destructius; no s'executen mai, encara que l'usuari
/// doni permisos (l'usuari pot fer-ho a mà, però la IA no).
fn is_forbidden(command: &str) -> bool {
    let c = command.trim();
    let lower = c.to_lowercase();
    [
        "sudo",
        "rm -rf /",
        "rm -rf ~",
        "rm -rf /*",
        "mkfs",
        "dd if=",
        ":(){",
        "shutdown",
        "reboot",
        "chmod 000",
        "chown -r root",
    ]
    .iter()
    .any(|p| lower.contains(p))
}

impl ComputerController {
    pub fn new(data_dir: PathBuf) -> Self {
        let path = data_dir.join("computer_permissions.json");
        let (perms, history) = if let Ok(raw) = std::fs::read_to_string(&path) {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct File {
                #[serde(default)]
                permissions: Permissions,
                #[serde(default)]
                history: Vec<HistoryEntry>,
            }
            match serde_json::from_str::<File>(&raw) {
                Ok(f) => (f.permissions, f.history),
                Err(_) => (Permissions::default(), Vec::new()),
            }
        } else {
            (Permissions::default(), Vec::new())
        };
        Self {
            path,
            perms: Mutex::new(perms),
            history: Mutex::new(history),
            pending: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(1),
        }
    }

    fn save(&self) {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct File<'a> {
            permissions: &'a Permissions,
            history: Vec<HistoryEntry>,
        }
        let perms = self.perms.lock().unwrap();
        let hist = self.history.lock().unwrap();
        let file = File {
            permissions: &perms,
            history: hist.clone(),
        };
        if let Ok(raw) = serde_json::to_string_pretty(&file) {
            let _ = std::fs::write(&self.path, raw);
        }
    }

    pub fn permissions(&self) -> Permissions {
        self.perms.lock().unwrap().clone()
    }

    pub fn set_permissions(&self, enabled: Option<bool>, confirm_each: Option<bool>) {
        {
            let mut p = self.perms.lock().unwrap();
            if let Some(e) = enabled {
                p.enabled = e;
            }
            if let Some(c) = confirm_each {
                p.confirm_each = c;
            }
        }
        self.save();
    }

    pub fn remove_pattern(&self, pattern: &str) {
        {
            let mut p = self.perms.lock().unwrap();
            p.allowlist.retain(|x| x != pattern);
        }
        self.save();
    }

    pub fn history(&self) -> Vec<HistoryEntry> {
        self.history.lock().unwrap().clone()
    }

    pub fn clear_history(&self) {
        self.history.lock().unwrap().clear();
        self.save();
    }

    /// Resposta de l'usuari al diàleg de confirmació.
    pub fn respond(&self, id: &str, approved: bool, remember: bool) -> Result<()> {
        let tx = self
            .pending
            .lock()
            .unwrap()
            .remove(id)
            .ok_or_else(|| anyhow!("Sol·licitud de confirmació no trobada o caducada"))?;
        let _ = tx.send((approved, remember));
        Ok(())
    }

    fn is_preapproved(&self, command: &str) -> bool {
        let p = self.perms.lock().unwrap();
        p.allowlist
            .iter()
            .any(|pat| command.trim_start().starts_with(pat.as_str()))
    }

    fn remember_pattern(command: &str) -> String {
        // Es recorda el primer "mot" de la comanda (el programa) com a prefix permès.
        command
            .trim()
            .split_whitespace()
            .next()
            .unwrap_or(command.trim())
            .to_string()
    }

    /// Punt d'entrada únic: executa una comanda amb el cascade de permisos.
    pub async fn execute(&self, app: &tauri::AppHandle, command: &str) -> Result<ExecReport> {
        let command = command.trim();
        if command.is_empty() {
            return Err(anyhow!("Comanda buida"));
        }
        if is_forbidden(command) {
            return Err(anyhow!(
                "Comanda bloquejada per seguretat (sudo, esborrat massiu, apagada…)"
            ));
        }

        let perms = self.permissions();
        if !perms.enabled {
            return Err(anyhow!(
                "El control de l'ordinador està desactivat. Activa'l a l'agent de l'IA."
            ));
        }

        // Cal demanar confirmació? (sempre si confirm_each, o si no és a la allowlist)
        if perms.confirm_each || !self.is_preapproved(command) {
            let id = format!("req-{}", self.counter.fetch_add(1, Ordering::SeqCst));
            let (tx, rx) = oneshot::channel();
            self.pending.lock().unwrap().insert(id.clone(), tx);
            let _ = app.emit(
                "computer://confirm",
                serde_json::json!({ "id": id, "command": command }),
            );

            let (approved, remember) =
                match tokio::time::timeout(std::time::Duration::from_secs(CONFIRM_TIMEOUT_SECS), rx)
                    .await
                {
                    Ok(Ok(v)) => v,
                    Ok(Err(_)) => (false, false),
                    Err(_) => {
                        self.pending.lock().unwrap().remove(&id);
                        return Err(anyhow!("Confirmació caducada: no s'ha executat res"));
                    }
                };
            if !approved {
                return Err(anyhow!("L'usuari ha rebutat l'execució"));
            }
            if remember {
                let pattern = Self::remember_pattern(command);
                {
                    let mut p = self.perms.lock().unwrap();
                    if !p.allowlist.contains(&pattern) {
                        p.allowlist.push(pattern);
                    }
                }
                self.save();
            }
        }

        self.run_raw(app, command).await
    }

    async fn run_raw(&self, app: &tauri::AppHandle, command: &str) -> Result<ExecReport> {
        use tokio::process::Command;

        let (shell, flag): (&str, &str) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("/bin/zsh", "-lc")
        };

        let child = Command::new(shell)
            .arg(flag)
            .arg(command)
            .kill_on_drop(true)
            .output();

        let report = match tokio::time::timeout(
            std::time::Duration::from_secs(EXEC_TIMEOUT_SECS),
            child,
        )
        .await
        {
            Ok(Ok(out)) => ExecReport {
                command: command.to_string(),
                ok: out.status.success(),
                exit_code: out.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                timed_out: false,
            },
            Ok(Err(e)) => return Err(anyhow!("Error executant: {}", e)),
            Err(_) => ExecReport {
                command: command.to_string(),
                ok: false,
                exit_code: -1,
                stdout: String::new(),
                stderr: format!("Timeout de {} s superat", EXEC_TIMEOUT_SECS),
                timed_out: true,
            },
        };

        // Historial (amb excerpt curi)
        let excerpt = {
            let mut s = report.stdout.clone();
            if !report.stderr.is_empty() {
                s.push_str("\n[stderr]\n");
                s.push_str(&report.stderr);
            }
            s.chars().take(4000).collect()
        };
        {
            let mut h = self.history.lock().unwrap();
            h.insert(
                0,
                HistoryEntry {
                    command: report.command.clone(),
                    ok: report.ok,
                    exit_code: report.exit_code,
                    excerpt,
                    at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                    .unwrap_or(0),
                },
            );
            if h.len() > HISTORY_CAP {
                h.truncate(HISTORY_CAP);
            }
        }
        self.save();

        let _ = app.emit(
            "computer://executed",
            serde_json::json!({ "command": report.command, "ok": report.ok }),
        );
        Ok(report)
    }
}
