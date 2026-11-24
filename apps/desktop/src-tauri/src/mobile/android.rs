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
//!
//! DES DE 0.71.19 l'SDK es pot **baixar sota demanda**: si el projecte obert
//! és d'Android i no hi ha SDK, el panell ofereix un sol clic que executa
//! `scripts/install-android-sdk.sh` (embotit al binari) amb la carpeta que
//! l'usuari tria — cap ruta escrita a codi, cap nom de volum.
//!
//! COMPILAR ≠ EMULAR (era el defecte que feia baixar gigabytes de més): un SDK
//! sense el paquet «emulator» ni «system images» compila APK i fa servir un
//! telèfon connect per USB perfectament. Per això «installed» ara significa
//! «serveix per compilar» i l'emulador va al camp «emulator_ok».

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AndroidStatus {
    /// Cert si hi ha un SDK **usable per compilar** (abans exigia l'emulador).
    pub installed: bool,
    /// Mateix que «installed», amb el nom explícit per a la UI.
    pub compile_ok: bool,
    /// Cert si hi ha el paquet «emulator» (només cal per a AVDs).
    pub emulator_ok: bool,
    /// Cert si hi ha «cmdline-tools»: amb ell es pot actualitzar l'SDK sol.
    pub cmdline_ok: bool,
    /// Ruta de l'SDK (si s'ha trobat).
    pub sdk_path: Option<String>,
    /// Ruta del binari `emulator` (si existeix).
    pub emulator_bin: Option<String>,
    /// Ruta del binari `adb` (si existeix).
    pub adb_bin: Option<String>,
    /// Ruta de `sdkmanager` (si existeix).
    pub sdkmanager_bin: Option<String>,
    /// Carpeta d'SDK que l'usuari ha configurat (buida = ruta estàndard).
    pub sdk_dir_configured: String,
    /// Nivell d'API que es baixarà si cal instal·lar.
    pub api: u32,
    /// Si es pot oferir el clic d'instal·lació (POSIX: hi ha bash + curl).
    pub one_click_supported: bool,
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

/// Intenta resoldre la ruta de l'SDK. Ordre: carpeta configurada per
/// l'usuari («android.sdk_dir», pot ser un disc extern) → ANDROID_HOME /
/// ANDROID_SDK_ROOT → rutes estàndard.
pub fn detect_sdk_root() -> Option<PathBuf> {
    let cfg_dir = configured_sdk_dir();
    if !cfg_dir.is_empty() {
        let p = PathBuf::from(&cfg_dir);
        if p.exists() {
            return Some(p);
        }
    }
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
    let (sdk_path, emulator_bin, adb_bin, sdkmanager_bin) = match root.as_ref() {
        Some(r) => {
            let pick = |p: PathBuf| p.exists().then(|| p.to_string_lossy().into_owned());
            (
                Some(r.to_string_lossy().into_owned()),
                pick(r.join("emulator").join(binary("emulator"))),
                pick(r.join("platform-tools").join(binary("adb"))),
                pick(r.join("cmdline-tools").join("latest").join("bin").join(binary("sdkmanager"))),
            )
        }
        None => (None, None, None, None),
    };
    // Compilació: amb `adb` (platform-tools) o amb `build-tools` ja es pot
    // fabricar un APK; l'emulador NO hi té res a veure.
    let build_tools = root
        .as_ref()
        .map(|r| r.join("build-tools").is_dir())
        .unwrap_or(false);
    let compile_ok = sdk_path.is_some() && (adb_bin.is_some() || build_tools);
    let emulator_ok = emulator_bin.is_some();
    let cmdline_ok = sdkmanager_bin.is_some();
    let installed = compile_ok;
    let cfg = crate::config::AppConfig::load().ok();
    let sdk_dir_configured = cfg
        .as_ref()
        .map(|c| c.android.sdk_dir.trim().to_string())
        .unwrap_or_default();
    let api = cfg.as_ref().map(|c| c.android.api).unwrap_or(34);
    let hint = if sdk_path.is_none() {
        Some(format!(
            "No s'ha trobat cap Android SDK. Es pot baixar només el que cal per compilar (API {api}, sense emulator) a la carpeta que tu triïs."
        ))
    } else if !compile_ok {
        Some(format!(
            "Hi ha una carpeta d'SDK a {} però no és usable per compilar (hi falta «platform-tools» o «build-tools»). Es pot reparar des d'aquí.",
            sdk_path.as_deref().unwrap_or("")
        ))
    } else if !emulator_ok {
        Some(
            "L'SDK de compilació és correcte. Sense el paquet «emulator» no hi haurà dispositius virtuals (AVDs); per compilar i instal·lar en un telèfon per USB no en cal.".to_string(),
        )
    } else {
        None
    };
    AndroidStatus {
        installed,
        compile_ok,
        emulator_ok,
        cmdline_ok,
        sdk_path,
        emulator_bin,
        adb_bin,
        sdkmanager_bin,
        sdk_dir_configured,
        api,
        one_click_supported: cfg!(unix),
        hint,
    }
}

/// Carpeta d'SDK triada per l'usuari (buida = sense configurar).
pub fn configured_sdk_dir() -> String {
    crate::config::AppConfig::load()
        .map(|c| c.android.sdk_dir.trim().to_string())
        .unwrap_or_default()
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

// ── Instal·lador sota demanda ───────────────────────────────────────────────

/// El script oficial d'instal·lació, EMBOTIT al binari. Viu al repo en
/// `scripts/install-android-sdk.sh` (auditable i provable per l'usuari), però
/// inclòs així funciona també dins el `.app` distribuït, on no hi ha repo.
pub const INSTALLER_SCRIPT: &str = include_str!("../../../../../scripts/install-android-sdk.sh");

/// Escriu (o refresca) el script a la carpeta de dades de NoOrbit i en retorna
/// la ruta. Només reescriu si el contingut ha canviat, i el deixa executable.
pub fn installer_script_path() -> anyhow::Result<PathBuf> {
    let dir = crate::config::AppConfig::data_dir()
        .ok_or_else(|| anyhow::anyhow!("No es pot resoldre la carpeta de dades"))?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("install-android-sdk.sh");
    let actual = std::fs::read_to_string(&path).unwrap_or_default();
    if actual != INSTALLER_SCRIPT {
        std::fs::write(&path, INSTALLER_SCRIPT)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(path)
}

/// Carpetes que MAI no es recorren en buscar marques d'Android: són dependències
/// o sortides de build que farien l'escaneig lent (i donaria falsos positius).
const SCAN_SKIP: &[&str] = &[
    "node_modules", ".git", "target", "build", "dist", ".gradle", ".venv", "venv",
    "__pycache__", "Library", "DerivedData", "Intermediate", "Saved", "Pods", ".next",
    ".qoder", "cache",
];

/// Cert si el projecte indicat és (o conté) un projecte Android. Retorna el
/// marcador trobat perquè la UI ho puga explicar. Es fa servir per a OFERIR la
/// descàrrega de l'SDK només quan el projecte realment el demana — no abans.
pub fn android_project_marker(root: &Path) -> Option<String> {
    if !root.is_dir() {
        return None;
    }
    // Marques definitives (nom de fitxer exacte).
    const DEFINITIVE: &[&str] = &["AndroidManifest.xml"];
    // Marques de Gradle: soles no ho garanteixen (pot ser JVM pur), però amb un
    // mòdul «app» o «android» a prop ja ho són.
    const GRADLE: &[&str] = &[
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
        "gradlew",
    ];
    let mut queue: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    let mut visited = 0usize;
    let mut gradle_vist = false;
    let mut moduls_android = false;
    while let Some((dir, depth)) = queue.pop() {
        if depth > 4 || visited > 3000 {
            continue;
        }
        visited += 1;
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            let is_dir = path.is_dir();
            if is_dir {
                if SCAN_SKIP.contains(&name.as_str()) || name.starts_with('.') {
                    continue;
                }
                if name == "app" || name == "android" {
                    moduls_android = true;
                }
                if depth < 4 {
                    queue.push((path, depth + 1));
                }
                continue;
            }
            if DEFINITIVE.contains(&name.as_str()) {
                return Some(name);
            }
            if GRADLE.contains(&name.as_str()) {
                gradle_vist = true;
            }
        }
    }
    if gradle_vist && moduls_android {
        return Some("projecte Gradle amb mòdul Android".to_string());
    }
    None
}
