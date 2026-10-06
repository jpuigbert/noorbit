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

/// Resultat de normalitzar una «comanda» que pot haver arribat com a
/// llenguatge natural (p. ex. un model local que reenvia «obre blender» tal
/// qual, o l'usuari escrivint-ho al panell). Evita el `command not found`
/// convertint ordres senzilles en comandes reals i rebutjant la resta.
pub enum Norm {
    /// Ja és una comanda de shell real: executa-la tal com és.
    Kept,
    /// S'ha traduït llenguatge natural a una comanda real.
    Translated(String),
    /// És llenguatge natural que no es pot traduir de forma segura.
    Rejected(String),
}

/// Plegament simple: minúscules + sense accents (per reconèixer verbs en
/// català/castellà/espanyol independentment de l'ortografia exacta).
fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'à' | 'á' | 'ä' | 'â' | 'ã' => 'a',
            'è' | 'é' | 'ë' | 'ê' => 'e',
            'ì' | 'í' | 'ï' | 'î' => 'i',
            'ò' | 'ó' | 'ö' | 'ô' | 'õ' => 'o',
            'ù' | 'ú' | 'ü' | 'û' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        })
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Reconeix una aplicació coneguda dins el text i en retorna el nom real
/// (per a `open -a "<Nom>"`). Cobreix els casos més freqüents a macOS.
fn find_app(flat: &str) -> Option<&'static str> {
    let mapa: &[(&[&str], &str)] = &[
        (&["blender"], "Blender"),
        (&["opera"], "Opera"),
        (&["safari"], "Safari"),
        (&["chrome", "google chrome", "chromium"], "Google Chrome"),
        (&["firefox"], "Firefox"),
        (&["notes", "apunts", "notas"], "Notes"),
        (&["calculator", "calculadora"], "Calculator"),
        (&["terminal", "consola"], "Terminal"),
        (&["music", "musica", "itunes"], "Music"),
        (&["messages", "missatgeria", "imessage"], "Messages"),
        (&["mail", "correu"], "Mail"),
        (&["photos", "fotos"], "Photos"),
        (&["unity", "unity editor"], "Unity"),
        (&["unreal", "ue5", "ue4", "unreal engine"], "UnrealEngine"),
        (&["vscode", "visual studio code", "code"], "Visual Studio Code"),
        (&["finder", "cercador"], "Finder"),
    ];
    for (keys, name) in mapa {
        if keys.iter().any(|k| flat.contains(k)) {
            return Some(name);
        }
    }
    None
}

/// Intenta traduir una ordre en llenguatge natural a una comanda de shell
/// real. Si no és una comanda ja real i no es pot traduir, la rebutja amb un
/// missatge clar (en lloc de deixar que el shell done `command not found`).
pub fn normalize_command(raw: &str) -> Norm {
    let cmd = raw.trim();
    if cmd.is_empty() {
        return Norm::Kept;
    }
    let head = cmd.split_whitespace().next().unwrap_or("");
    // 1) Sembla ja una comanda real? executable al PATH, un embolcall propi de
    //    NoOrbit, un camí/URL, una assignació d'entorn o una sintaxi de shell.
    let looks_real = head.contains('/')
        || head.contains('.')
        || head.contains('=')
        || head.starts_with('~')
        || head.starts_with('$')
        || head.starts_with('(')
        || head.starts_with('"')
        || head.starts_with('\'')
        || matches!(
            head,
            "cd" | "sudo" | "env" | "export" | "source" | "if" | "for" | "while" | "time" | "nohup" | "true" | "false"
        )
        || which::which(head).is_ok();
    if looks_real {
        return Norm::Kept;
    }
    // 2) Llenguatge natural: plegament i patrons senzills.
    let flat = fold(cmd);
    let verb_obrir = ["obre", "obri", "obrir", "obriu", "arrenca", "arrenque", "engega", "lanca", "llanca", "inicia", "executa", "obri la", "open", "launch", "start"]
        .iter()
        .any(|v| flat.starts_with(v));
    if verb_obrir {
        // «obre una URL» → open "<url>"
        if flat.contains("http") || flat.contains("www.") || looks_like_domain(&flat) {
            if let Some(tok) = cmd.split_whitespace().find(|t| {
                let f = fold(t);
                f.contains("http") || f.contains("www.") || f.contains('.')
            }) {
                return Norm::Translated(format!("open '{}'", tok.replace('\'', r"'\''")));
            }
        }
        if let Some(app) = find_app(&flat) {
            return Norm::Translated(format!("open -a '{}'", app));
        }
        // Només un o dos mots restants i purament alfabètics: podria ser el
        // nom d'una app no catalogada. S'intenta «open -a» amb el candidat.
        let words: Vec<&str> = flat.split_whitespace().collect();
        if words.len() <= 3 {
            if let Some(cand) = words.iter().rev().find(|w| {
                w.chars().all(|c| c.is_alphabetic()) && !is_stopword(w)
            }) {
                let mut name = cand.to_string();
                if let Some(first) = name.get_mut(0..1) {
                    first.make_ascii_uppercase();
                }
                // Només si el nom no fa pinta de paraula comuna d'accio.
                return Norm::Translated(format!("open -a '{}'", name));
            }
        }
        return Norm::Rejected(reject_msg(cmd));
    }
    if ["llista", "ls", "ensenya", "mostra", "enseny", "quins", "que hi ha", "veur", "veure"]
        .iter()
        .any(|v| flat.starts_with(v))
    {
        return Norm::Translated("ls -la".into());
    }
    if ["crea", "crear", "fes", "fer", "mk", "nova carpeta", "nou directori"]
        .iter()
        .any(|v| flat.starts_with(v))
        && (flat.contains("carpeta") || flat.contains("directori") || flat.contains("folder") || flat.contains("dir"))
    {
        if let Some(name) = cmd
            .split_whitespace()
            .rev()
            .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_'))
            .find(|t| !t.is_empty() && fold(t) != "carpeta" && fold(t) != "directory")
        {
            return Norm::Translated(format!("mkdir -p '{}'", name.replace('\'', r"'\''")));
        }
    }
    Norm::Rejected(reject_msg(cmd))
}

fn is_stopword(w: &str) -> bool {
    [
        "el", "la", "ls", "les", "un", "una", "uns", "unes", "de", "del", "al", "a", "en", "per",
        "que", "i", "o", "the", "and", "for", "with", "coses", "ara", "si", "vull", "pot",
    ]
    .contains(&w)
}

/// Detecta un domini («obre noorbit.com») sense confondre'l amb frases.
fn looks_like_domain(flat: &str) -> bool {
    flat.split_whitespace().any(|t| {
        let has_dot = t.contains('.');
        let ends_known = [".com", ".cat", ".org", ".net", ".io", ".es"]
            .iter()
            .any(|e| t.ends_with(e));
        has_dot && (ends_known || t.matches('.').count() == 1)
    })
}

fn reject_msg(cmd: &str) -> String {
    format!(
        "«{}» és llenguatge natural, no una comanda del sistema. Escriu una \
         comanda real (p. ex. «open -a Blender», «ls -la», «python3 main.py») o \
         demana-ho al xat de la IA, que redacta la comanda i l'executa per a tu.",
        cmd.chars().take(80).collect::<String>()
    )
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

        // Guarda lingüística: si ha arribat llenguatge natural («obre
        // blender», «Crea un submarí») el traduïm a una comanda real quan és
        // senzill i el rebutgem quan no, en lloc d'enviar-lo cru al shell i
        // obtindre «command not found: Crea» (eixit 127).
        let effective: String = match normalize_command(command) {
            Norm::Kept => command.to_string(),
            Norm::Translated(real) => real,
            Norm::Rejected(msg) => return Err(anyhow!(msg)),
        };
        let command = effective.as_str();

        let perms = self.permissions();
        if !perms.enabled {
            return Err(anyhow!(
                "El control de l'ordinador està DESACTIVAT: la IA no pot tocar \
                 el teu Mac. Activa'l al panell «Ordinador» de la dreta (icona \
                 d'escut ▸ «Permet que la IA controli l'ordinador»); cada \
                 comanda es confirmarà en pantalla abans d'executar-se."
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
