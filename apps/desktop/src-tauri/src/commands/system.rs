//! Comandes de sistema operatiu (terminal, Finder…).

use tauri::command;

/// Memòria RAM total del sistema, en bytes. Serveix per triar automàticament
/// el millor model local que hi càpiga còmodament.
#[command]
pub async fn system_total_memory() -> Result<u64, String> {
    if cfg!(target_os = "macos") {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .map_err(|e| format!("sysctl: {}", e))?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        return s.parse::<u64>().map_err(|e| format!("hw.memsize: {}", e));
    }
    if cfg!(target_os = "linux") {
        let f = std::fs::read_to_string("/proc/meminfo").map_err(|e| e.to_string())?;
        for line in f.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kb: u64 = rest
                    .trim()
                    .trim_end_matches("kB")
                    .trim()
                    .parse()
                    .map_err(|e| format!("MemTotal: {}", e))?;
                return Ok(kb * 1024);
            }
        }
        return Err("MemTotal no trobat".into());
    }
    Err("Sistema no suportat".into())
}

/// Obre una finestra de Terminal situant-la a la carpeta indicada
/// (o a $HOME si no n'hi ha cap).
#[command]
pub async fn open_terminal(path: Option<String>) -> Result<(), String> {
    let dir = path
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().to_string_lossy().to_string());

    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "tell application \"Terminal\" to do script \"cd '{}'\"",
                dir.replace('\'', "'\\''")
            ))
            .spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd").arg("/C").arg("start").arg("cmd").spawn()
    } else {
        std::process::Command::new("x-terminal-emulator").spawn()
    };

    result.map(|_| ()).map_err(|e| format!("No s'ha pogut obrir el terminal: {}", e))
}

/// Mostra un fitxer o carpeta al Finder (macOS) / explorador equivalent.
#[command]
pub async fn reveal_in_finder(path: String) -> Result<(), String> {
    if path.is_empty() {
        return Err("Ruta buida".into());
    }
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("explorer").arg("/select,").arg(&path).spawn()
    } else {
        std::process::Command::new("xdg-open")
            .arg(
                std::path::Path::new(&path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or(path.clone()),
            )
            .spawn()
    };
    result.map(|_| ()).map_err(|e| format!("No s'ha pogut mostrar al Finder: {}", e))
}
