//! Detecció de l'Android SDK, llista d'AVD (Android Virtual Devices) i control
//! de l'emulador. Tot es fa cridant les eines oficials del SDK:
//!
//! - `$ANDROID_HOME/emulator/emulator -list-avds` — llistat d'AVD
//! - `$ANDROID_HOME/emulator/emulator -avd <nom>` — arrencar-ne un
//! - `$ANDROID_HOME/platform-tools/adb emu kill`   — aturar-lo
//! - `$ANDROID_HOME/platform-tools/adb exec-out screencap -p` — captura PNG
//!
//! NoOrbit NO inclou cap SDK propietari: només invoca el que l'usuari haja
//! instal·lat (Android Studio o cmdline-tools).

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AndroidStatus {
    /// Cert si s'ha trobat un SDK (ANDROID_HOME o una ruta estàndard).
    pub installed: bool,
    /// Ruta de l'SDK (si s'ha trobat).
    pub sdk_path: Option<String>,
    /// Ruta del binari `emulator` (si existeix).
    pub emulator_bin: Option<String>,
    /// Ruta del binari `adb` (si existeix).
    pub adb_bin: Option<String>,
    /// missatge llegible quan no està instal·lat (per a la UI).
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AndroidAvd {
    pub name: String,
    /// Nom llegible (marca + model) si `avdmanager` el pot resoldre.
    pub pretty: Option<String>,
    pub running: bool,
}

/// Rutes habituals on l'usuari pot tindre l'SDK. S'usen com a reserva quan
/// ANDROID_HOME no està definit.
fn candidate_sdk_roots() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        let h = PathBuf::from(home);
        // macOS (Android Studio per defecte) i Linux.
        out.push(h.join("Library/Android/sdk"));
        out.push(h.join("Android/Sdk"));
        out.push(h.join("android-sdk"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        // Windows: %LOCALAPPDATA%\Android\Sdk
        out.push(PathBuf::from(local).join("Android\\Sdk"));
    }
    out
}

/// Intenta resoldre la ruta de l'SDK. Prèvia: ANDROID_HOME / ANDROID_SDK_ROOT.
pub fn detect_sdk_root() -> Option<PathBuf> {
    for key in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(v) = std::env::var(key) {
            let p = PathBuf::from(&v);
            if p.exists() {
                return Some(p);
            }
        }
    }
    candidate_sdk_roots().into_iter().find(|p| p.exists())
}

/// Detecta l'SDK + els binaris rellevants.
pub fn detect() -> AndroidStatus {
    let root = detect_sdk_root();
    let (sdk_path, emulator_bin, adb_bin) = match root.as_ref() {
        Some(r) => {
            let emu = r.join("emulator").join(binary("emulator"));
            let adb = r.join("platform-tools").join(binary("adb"));
            (
                Some(r.to_string_lossy().into_owned()),
                emu.exists().then(|| emu.to_string_lossy().into_owned()),
                adb.exists().then(|| adb.to_string_lossy().into_owned()),
            )
        }
        None => (None, None, None),
    };
    let installed = sdk_path.is_some() && emulator_bin.is_some();
    let hint = if installed {
        None
    } else if sdk_path.is_some() {
        Some("S'ha trobat l'Android SDK però falta el component «emulator». Obre Android Studio ▸ SDK Manager i instal·la «Android Emulator» i «Android SDK Platform-Tools».".to_string())
    } else {
        Some("No s'ha trobat cap Android SDK. Instal·la Android Studio (o els «cmdline-tools») i exporta ANDROID_HOME.".to_string())
    };
    AndroidStatus {
        installed,
        sdk_path,
        emulator_bin,
        adb_bin,
        hint,
    }
}

fn binary(name: &str) -> String {
    if cfg!(windows) {
        format!("{}.exe", name)
    } else {
        name.to_string()
    }
}

/// Llista els AVDs instal·lats (nom).
pub fn list_avds() -> anyhow::Result<Vec<AndroidAvd>> {
    let status = detect();
    let Some(emu) = status.emulator_bin.as_ref() else {
        return Err(anyhow::anyhow!(
            "{}",
            status.hint.unwrap_or_else(|| "Android SDK no disponible".into())
        ));
    };
    let out = Command::new(emu)
        .arg("-list-avds")
        .stderr(Stdio::piped())
        .output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(anyhow::anyhow!("emulator -list-avds ha fallat: {}", err));
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let running = running_avds(&status);
    let list: Vec<AndroidAvd> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|name| AndroidAvd {
            name: name.to_string(),
            pretty: None,
            running: running.iter().any(|r| r == name),
        })
        .collect();
    Ok(list)
}

/// AVDs en marxa: llista `adb devices` i filtra emuladors (`emulator-XXXX`).
/// Després fa `adb -s emulator-XXXX emu avd name` per saber quin nom és.
fn running_avds(status: &AndroidStatus) -> Vec<String> {
    let Some(adb) = status.adb_bin.as_ref() else {
        return Vec::new();
    };
    let Ok(out) = Command::new(adb).arg("devices").output() else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let mut names: Vec<String> = Vec::new();
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 2 || cols[1].trim() != "device" {
            continue;
        }
        let serial = cols[0].trim();
        if !serial.starts_with("emulator-") {
            continue;
        }
        // Preguntem el nom de l'AVD.
        if let Ok(info) = Command::new(adb)
            .args(["-s", serial, "emu", "avd", "name"])
            .output()
        {
            let name = String::from_utf8_lossy(&info.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !name.is_empty() {
                names.push(name);
            }
        }
    }
    names
}

/// Arrenca un AVD en segon pla. Retorna la ruta del procés-fill (no el
/// seguim: quan l'usuari el tanque, tanca des de l'emulador mateix).
pub fn start_avd(name: &str) -> anyhow::Result<()> {
    let status = detect();
    let Some(emu) = status.emulator_bin.as_ref() else {
        return Err(anyhow::anyhow!(
            "{}",
            status.hint.unwrap_or_else(|| "Android SDK no disponible".into())
        ));
    };
    let mut cmd = Command::new(emu);
    cmd.arg("-avd").arg(name);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Deslligar: l'emulador sobreviu a NoOrbit (com faria Android Studio).
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // `pre_exec` és insegur; ací només fem setsid per desconnectar-lo del
        // grup de procés del pare. Equivalent a `nohup` de shell.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let _child = cmd.spawn()?;
    Ok(())
}

/// Atura un AVD en execució.
pub fn stop_avd(name: &str) -> anyhow::Result<()> {
    let status = detect();
    let Some(adb) = status.adb_bin.as_ref() else {
        return Err(anyhow::anyhow!("adb no disponible"));
    };
    // Trobem el serial `emulator-XXXX` corresponent a aquest AVD.
    let out = Command::new(adb).arg("devices").output()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 2 || cols[1].trim() != "device" {
            continue;
        }
        let serial = cols[0].trim();
        if !serial.starts_with("emulator-") {
            continue;
        }
        if let Ok(info) = Command::new(adb)
            .args(["-s", serial, "emu", "avd", "name"])
            .output()
        {
            let owned = String::from_utf8_lossy(&info.stdout).to_string();
            let n = owned.lines().next().unwrap_or("").trim();
            if n == name {
                let _ = Command::new(adb)
                    .args(["-s", serial, "emu", "kill"])
                    .output();
                return Ok(());
            }
        }
    }
    Err(anyhow::anyhow!("Aquest AVD no està en marxa"))
}

/// Captura la pantalla de l'AVD en marxa i la retorna com a PNG en base64.
pub fn screenshot() -> anyhow::Result<String> {
    let status = detect();
    let Some(adb) = status.adb_bin.as_ref() else {
        return Err(anyhow::anyhow!("adb no disponible"));
    };
    // Primer device tipus emulator
    let out = Command::new(adb).arg("devices").output()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let serial = text
        .lines()
        .skip(1)
        .filter_map(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            if cols.len() >= 2 && cols[1].trim() == "device" && cols[0].starts_with("emulator-") {
                Some(cols[0].trim().to_string())
            } else {
                None
            }
        })
        .next()
        .ok_or_else(|| anyhow::anyhow!("Cap emulator Android en marxa"))?;

    // `exec-out screencap -p` envia bytes PNG directament a stdout.
    let mut child = Command::new(adb)
        .args(["-s", &serial, "exec-out", "screencap", "-p"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut buf: Vec<u8> = Vec::new();
    if let Some(so) = child.stdout.as_mut() {
        so.read_to_end(&mut buf)?;
    }
    let _ = child.wait();
    if buf.is_empty() {
        return Err(anyhow::anyhow!("La captura ha eixit buida"));
    }
    // Normalitzem salts de línia CR/LF que algunes versions d'adb introdueixen.
    let cleaned = strip_lf_inflate(&buf);
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    Ok(format!("data:image/png;base64,{}", B64.encode(cleaned)))
}

/// En algunes versions antigues d'adb, `exec-out` converteia \n a \r\n.
/// Si detectem patró \r\n massiu i el PNG comença bé, revertim.
fn strip_lf_inflate(bytes: &[u8]) -> Vec<u8> {
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        // Podria ser un PNG amb \r afegits; intente desfer-los.
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\r' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                out.push(b'\n');
                i += 2;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        if out.len() >= 8 && &out[..8] == b"\x89PNG\r\n\x1a\n" {
            return out;
        }
    }
    bytes.to_vec()
}

/// Ruta a l'SDK on es desen els AVD (per a futurs «crear AVD»).
pub fn avd_home(root: &Path) -> PathBuf {
    root.join(".android").join("avd")
}
