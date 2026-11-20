//! Detecció i arrencada d'Unreal Engine 5.

use super::client::UnrealClient;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time::sleep;

const DEFAULT_RC_PORT: u16 = crate::ports::UNREAL_RC_PORT;

/// Port REAL de la Remote Control (el de la configuració, si n'hi ha un).
///
/// No pot ser una constant: `launch_editor` INJECTA el port configurat a
/// l'editor (bootstrap.py l'obre), així que esperar al 30010 fixe era una
/// contradicció — amb un port canviat l'editor s'aixecava i la connexió
/// acabava sempre en temps exhaurit.
fn rc_port() -> u16 {
    let port = super::client::configured_port();
    if port == 0 {
        DEFAULT_RC_PORT
    } else {
        port
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchReport {
    pub engine_path: String,
    pub project_path: String,
    pub launched: bool,
    pub message: String,
}

/// Comprova si el port de la Remote Control API està obert.
pub async fn is_rc_open(port: u16) -> bool {
    tokio::time::timeout(
        Duration::from_millis(500),
        TcpStream::connect(format!("127.0.0.1:{}", port)),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false)
}

/// Bases on hi pot haver un motor d'Unreal Engine: les del sistema I TOTES
/// les unitats montades (discs externs, NAS, unitats de xarxa).
///
/// Abans només es miraven tres rutes fixes, de manera que un motor instal·lat
/// en un disc extern era INVISIBLE: NoOrbit deia «no s'ha detectat cap
/// instal·lació» tot i que l'editor hi fos a `/Volumes/SSD/Epic Games`.
fn engine_base_dirs() -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();

    if cfg!(target_os = "windows") {
        // Totes les unitats del sistema: D:\ ha de comptar com C:\.
        for letter in b'A'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", letter as char));
            if !root.is_dir() {
                continue;
            }
            bases.push(root.join("Program Files"));
            bases.push(root.join("Program Files (x86)"));
            bases.push(root);
        }
    } else if cfg!(target_os = "macos") {
        bases.push(PathBuf::from("/Applications"));
        bases.push(PathBuf::from("/Users/Shared"));
        // Discs externs, unitats de xarxa i USB montats al sistema.
        if let Ok(entries) = std::fs::read_dir("/Volumes") {
            for e in entries.flatten() {
                bases.push(e.path());
            }
        }
    } else {
        bases.push(PathBuf::from("/opt"));
        bases.push(PathBuf::from("/usr/local"));
        // Unitats montades: /mnt/<vol>, /media/<vol>, /run/media/<usuari>/<vol>
        for mount in ["/mnt", "/media", "/run/media"] {
            if let Ok(entries) = std::fs::read_dir(mount) {
                for e in entries.flatten() {
                    bases.push(e.path());
                    if let Ok(sub) = std::fs::read_dir(e.path()) {
                        for s in sub.flatten() {
                            bases.push(s.path());
                        }
                    }
                }
            }
        }
    }

    // La carpeta de l'usuari (a Linux hi viu el motor: ~/UnrealEngine).
    if let Some(home) = dirs::home_dir() {
        bases.push(home);
    }

    bases
}

/// Un directori és un motor d'Unreal si té la carpeta `Engine` (Binaries o
/// Build). Filre aquest perquè una carpeta buida o una descàrrega a mitges
/// no s'ofereix com una instal·lació vàlida.
fn is_engine_root(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    path.join("Engine").join("Binaries").is_dir() || path.join("Engine").join("Build").is_dir()
}

/// Una instal·lació A MITGES: l'Epic Games Launcher ja ha creat la carpeta
/// del motor (hi és el seu «.egstore») però encara no hi ha l'editor, perquè
/// la descàrrega continua o es va cancel·lar. No es pot fer servir, però
/// l'usuari ha de saber que hi és (i on).
fn is_pending_install(path: &Path) -> bool {
    path.join(".egstore").is_dir() && !is_engine_root(path)
}

/// El nom d'un motor: «UE_5.8» o «UE5.8», i també «Unreal Engine» (com ho
/// deixa Steam) o «UnrealEngine» (compilació des del codi).
///
/// La carpeta «UE» que hi ha dins l'app del launcher d'Epic Games NO compta:
/// no és cap motor.
fn looks_like_engine(name: &str) -> bool {
    let lower = name.to_lowercase();
    if lower.starts_with("unreal engine") || lower == "unrealeengine" {
        return true;
    }
    let Some(rest) = name.strip_prefix("UE") else {
        return false;
    };
    let rest = rest.trim_start_matches('_');
    rest.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// Dins d'un bundle `.app` no hi ha motors: el launcher d'Epic Games en té un
/// («Epic Games Launcher.app/Contents/UE») i es confondria amb una
/// instal·lació.
fn inside_app_bundle(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().to_string_lossy().to_lowercase().ends_with(".app"))
}

/// Motors dins d'una carpeta: subdirectoris «UE…» que tenen l'estructura
/// real d'un motor.
fn collect_engine_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !looks_like_engine(&name) {
            continue;
        }
        let p = e.path();
        if is_engine_root(&p) && !inside_app_bundle(&p) {
            out.push(p);
        }
    }
}

/// Deixa només una vegada cada ruta (el mateix motor es pot veure des de
/// dues bases o dues comprovacions).
fn dedup_paths(v: &mut Vec<PathBuf>) {
    let mut seen = std::collections::HashSet::new();
    v.retain(|p| seen.insert(p.clone()));
}

/// El motor que arriba per Steam viu a
/// `<biblioteca>/steamapps/common/Unreal Engine`. Es mira la base, les
/// carpetes filles que puguin ser una biblioteca («SteamLibrary») i la ruta
/// estàndard del macOS.
fn collect_steam_engines(base: &Path, out: &mut Vec<PathBuf>) {
    let mut commons: Vec<PathBuf> = vec![base.join("steamapps").join("common")];
    if let Ok(entries) = std::fs::read_dir(base) {
        for e in entries.flatten() {
            let child = e.path();
            if child.is_dir() {
                commons.push(child.join("steamapps").join("common"));
            }
        }
    }
    if cfg!(target_os = "macos") {
        commons.push(
            base.join("Library")
                .join("Application Support")
                .join("Steam")
                .join("steamapps")
                .join("common"),
        );
    }

    for common in commons {
        let Ok(entries) = std::fs::read_dir(&common) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if is_engine_root(&p) && !inside_app_bundle(&p) {
                out.push(p);
            }
        }
    }
}

/// Recull els motors d'un base, a tres nivells:
///
/// 0. el motor a la base mateixa (`/Volumes/SSD/UE_5.8`);
/// 1. dins de qualsevol carpeta filla — el contenidor es diu com es digui
///    («Epic Games», «epic games toshi», «Jocs»…) i així un disc extern amb
///    la carpeta renomenada també es troba;
/// 2. dins de la carpeta filla d'una que fa referència a Epic/Games
///    (`Games/Epic Games` a Windows, `Applications/Epic Games` al Mac).
fn collect_engines_in(base: &Path, out: &mut Vec<PathBuf>) {
    collect_engine_dirs(base, out);
    walk_engine_dirs(base, |dir| collect_engine_dirs(dir, &mut *out));
    collect_steam_engines(base, out);

    // Layout de Linux: la base ÉS el motor (~/UnrealEngine, /opt/UnrealEngine).
    let direct = base.join("UnrealEngine");
    if is_engine_root(&direct) {
        out.push(direct);
    }

    dedup_paths(out);
}

/// Recull les carpetes que PODEN contenir motors: les filles de la base (es
/// diguin com es diguin) i, si la filla fa referència a Epic/Games, les
/// seves netes (`Games/Epic Games`, `Applications/Epic Games`).
fn walk_engine_dirs(base: &Path, mut f: impl FnMut(&Path)) {
    let Ok(children) = std::fs::read_dir(base) else {
        return;
    };
    for e in children.flatten() {
        let child = e.path();
        if !child.is_dir() {
            continue;
        }
        f(&child);

        let name = e.file_name().to_string_lossy().to_lowercase();
        if name.contains("epic") || name.contains("games") {
            if let Ok(grand) = std::fs::read_dir(&child) {
                for g in grand.flatten() {
                    f(&g.path());
                }
            }
        }
    }
}

/// Instal·lacions a mitges dins d'una carpeta (descàrrega en curs).
fn collect_pending_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !looks_like_engine(&name) {
            continue;
        }
        let p = e.path();
        if is_pending_install(&p) && !inside_app_bundle(&p) {
            out.push(p);
        }
    }
}

/// Recull les instal·lacions A MITGES d'un base (l'Epic Launcher encara
/// descarregant el motor, o la descàrrega cancel·lada).
fn collect_pending_in(base: &Path, out: &mut Vec<PathBuf>) {
    collect_pending_dirs(base, out);
    walk_engine_dirs(base, |dir| collect_pending_dirs(dir, &mut *out));
    dedup_paths(out);
}

/// Versió del motor per ordenar-lo: «UE_5.4» → (5, 4, 0).
fn version_of(path: &Path) -> (u32, u32, u32) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let digits: String = name.chars().skip_while(|c| !c.is_ascii_digit()).collect();
    let mut it = digits.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

/// Cerca instal·lacions d'Unreal Engine a TOT el sistema, INCLOSES les que
/// viuen en un disc extern, un NAS o una unitat de xarxa.
pub fn detect_engine_installations() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for base in engine_base_dirs() {
        collect_engines_in(&base, &mut found);
    }

    // Una mateixa instal·lació pot sortir per dues bases (p. ex. la carpeta de
    // l'usuari i la del sistema): es deixa només una vegada.
    let mut seen = std::collections::HashSet::new();
    found.retain(|p| seen.insert(p.clone()));

    // Les més noves primer (UE_5.4 abans que UE_5.3).
    found.sort_by(|a, b| version_of(b).cmp(&version_of(a)).then_with(|| a.cmp(b)));
    found
}

/// Cerca instal·lacions A MITGES: l'Epic Games Launcher ja ha creat la
/// carpeta del motor (hi és el seu «.egstore») però encara hi ha poc o res
/// d'editor, perquè la descàrrega continua o es va cancel·lar.
///
/// No es poden fer servir, però l'usuari ha de saber que hi són: si no, creu
/// que NoOrbit no en troba cap i busca el motor a mà sense motiu.
pub fn detect_pending_installations() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for base in engine_base_dirs() {
        collect_pending_in(&base, &mut found);
    }

    let mut seen = std::collections::HashSet::new();
    found.retain(|p| seen.insert(p.clone()));

    found.sort_by(|a, b| version_of(b).cmp(&version_of(a)).then_with(|| a.cmp(b)));
    found
}

/// Retorna el binari de l'editor per a una instal·lació.
pub fn editor_binary(engine_root: &Path) -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        let p = engine_root
            .join("Engine")
            .join("Binaries")
            .join("Win64")
            .join("UnrealEditor.exe");
        if p.exists() { return Some(p); }
    } else if cfg!(target_os = "macos") {
        let p = engine_root
            .join("Engine")
            .join("Binaries")
            .join("Mac")
            .join("UnrealEditor.app")
            .join("Contents")
            .join("MacOS")
            .join("UnrealEditor");
        if p.exists() { return Some(p); }
    } else {
        let p = engine_root
            .join("Engine")
            .join("Binaries")
            .join("Linux")
            .join("UnrealEditor");
        if p.exists() { return Some(p); }
    }
    None
}

/// Assegura que el projecte té els plugins que necessita NoOrbit
/// (PythonScriptPlugin + RemoteControl) ACTIVATS al fitxer `.uproject`.
///
/// Sense això, molts projectes recents (i tots els creats sense marcar-los)
/// arriben amb el servidor Remote Control apagat: l'editor s'obre però el
/// port 30010 no respon mai, i és la causa més freqüent de «no connecta».
/// Retorna `true` si ha HAGUT de modificar el fitxer.
pub fn ensure_project_plugins(uproject: &Path) -> Result<bool> {
    ensure_plugins_named(uproject, &["PythonScriptPlugin", "RemoteControl"])
}

/// Activa els plugins del servidor MCP oficial (UE 5.8+) al `.uproject`:
/// `ModelContextProtocol` i `AllToolsets`. Com els de Remote Control, cal
/// REINICIAR l'editor perquè un plugin acabat d'activar es carregui.
/// Retorna `true` si ha HAGUT de modificar el fitxer.
pub fn ensure_agent_plugins(uproject: &Path) -> Result<bool> {
    ensure_plugins_named(uproject, &["ModelContextProtocol", "AllToolsets"])
}

/// Lògica comuna: assegura que `names` són a la llista `Plugins` del
/// `.uproject` amb `Enabled: true`, afegint-los si falten (idempotent).
fn ensure_plugins_named(uproject: &Path, names: &[&str]) -> Result<bool> {
    let raw = std::fs::read_to_string(uproject)
        .map_err(|e| anyhow!("No puc llegir el .uproject: {}", e))?;
    let mut v: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| anyhow!("El .uproject no és JSON vàlid: {}", e))?;

    let plugins = v
        .as_object_mut()
        .ok_or_else(|| anyhow!("El .uproject no és un objecte"))?
        .entry("Plugins")
        .or_insert_with(|| serde_json::json!([]));
    if !plugins.is_array() {
        *plugins = serde_json::json!([]);
    }

    let mut changed = false;
    for name in names {
        let already = plugins
            .as_array()
            .map(|a| {
                a.iter().any(|p| {
                    p.get("Name").and_then(|n| n.as_str()) == Some(*name)
                        && p.get("Enabled").and_then(|e| e.as_bool()) == Some(true)
                })
            })
            .unwrap_or(false);
        if !already {
            plugins
                .as_array_mut()
                .ok_or_else(|| anyhow!("Plugins no és una llista"))?
                .push(serde_json::json!({ "Name": name, "Enabled": true }));
            changed = true;
        }
    }

    if changed {
        // Còpia de seguretat d'una sola escriptura (la primera vegada que
        // toquem el fitxer): l'usuari pot tornar-ho endarrere a mà.
        let backup = uproject.with_extension("uproject.bak");
        if !backup.exists() {
            let _ = std::fs::copy(uproject, &backup);
        }
        let pretty = serde_json::to_string_pretty(&v)
            .map_err(|e| anyhow!("No puc tornar a escriure el .uproject: {}", e))?;
        std::fs::write(uproject, pretty)
            .map_err(|e| anyhow!("No puc escriure el .uproject: {}", e))?;
    }
    Ok(changed)
}

/// Escriu la configuració persistent perquè el servidor MCP d'Unreal
/// s'aixequi SOL cada cop que l'usuari obri el projecte (idempotent):
///
/// ```ini
/// [/Script/ModelContextProtocolEngine.ModelContextProtocolSettings]
/// bAutoStartServer=True
/// ```
///
/// El fitxer és `Saved/Config/<Plataforma>Editor/EditorPerProjectUserSettings.ini`,
/// al costat del `.uproject` (mai a Config/Default: aquesta secció és per
/// ajustos d'usuari per projecte). Retorna la ruta del fitxer escrit.
pub fn write_mcp_autostart(uproject: &Path) -> Result<PathBuf> {
    let dir = uproject
        .parent()
        .ok_or_else(|| anyhow!("El .uproject no té carpeta pare"))?;
    let platform = if cfg!(target_os = "windows") {
        "WindowsEditor"
    } else if cfg!(target_os = "macos") {
        "MacEditor"
    } else {
        "LinuxEditor"
    };
    let ini = dir
        .join("Saved")
        .join("Config")
        .join(platform)
        .join("EditorPerProjectUserSettings.ini");
    std::fs::create_dir_all(ini.parent().expect("ini sempre té pare"))
        .map_err(|e| anyhow!("No puc crear la carpeta de configuració: {}", e))?;

    const SECTION: &str = "[/Script/ModelContextProtocolEngine.ModelContextProtocolSettings]";
    const KEY: &str = "bAutoStartServer";

    let existing = std::fs::read_to_string(&ini).unwrap_or_default();
    let mut lines: Vec<String> = existing.lines().map(String::from).collect();

    // 1) Ja hi ha la secció i la clau? Actualitzem només el valor.
    let mut in_section = false;
    let mut key_idx: Option<usize> = None;
    let mut section_idx: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t == SECTION;
            if in_section && section_idx.is_none() {
                section_idx = Some(i);
            }
            continue;
        }
        if in_section && t.starts_with(KEY) && t.contains('=') {
            key_idx = Some(i);
            break;
        }
    }

    match (section_idx, key_idx) {
        (Some(_), Some(ki)) => {
            lines[ki] = format!("{}={}", KEY, "True");
        }
        (Some(si), None) => {
            lines.insert(si + 1, format!("{}={}", KEY, "True"));
        }
        (None, _) => {
            if !lines.is_empty() && !lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
                lines.push(String::new());
            }
            lines.push(SECTION.to_string());
            lines.push(format!("{}={}", KEY, "True"));
        }
    }

    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    std::fs::write(&ini, out).map_err(|e| anyhow!("No puc escriure {}: {}", ini.display(), e))?;
    Ok(ini)
}

/// Espera que el servidor MCP d'Unreal (port 8000) respongui.
pub async fn wait_for_mcp(port: u16, timeout_secs: u64) -> Result<bool> {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    while std::time::Instant::now() < deadline {
        if is_rc_open(port).await {
            return Ok(true);
        }
        sleep(Duration::from_millis(1500)).await;
    }
    Ok(false)
}

/// Retorna el binari de RunUAT per a una instal·lació.
pub fn runuat_binary(engine_root: &Path) -> Option<PathBuf> {
    let batch = engine_root
        .join("Engine")
        .join("Build")
        .join("BatchFiles");

    if cfg!(target_os = "windows") {
        let p = batch.join("RunUAT.bat");
        if p.exists() { return Some(p); }
    } else {
        let p = batch.join("RunUAT.sh");
        if p.exists() { return Some(p); }
    }
    None
}

/// Llança l'editor d'Unreal amb un projecte `.uproject`.
pub async fn launch_editor(
    editor: &Path,
    project: &Path,
    extra_args: &[String],
) -> Result<()> {
    let mut cmd = Command::new(editor);
    cmd.arg(project.to_string_lossy().to_string());

    for arg in extra_args {
        cmd.arg(arg);
    }

    // El port CONFIGURAT (panell d'Unreal ▸ Port de Remote Control, que desa
    // `rc_port` a config.toml) s'injecta per entorn: el bootstrap.py el
    // llegeix i obre AQUELL port. Abans el Python sempre obria el 30010 i la
    // configuració era ignorada.
    let port = rc_port();
    cmd.env(crate::ports::ENV_UNREAL_PORT, port.to_string());

    // Habilita Remote Control API i Python Editor Script (per si el .uproject
    // no ha pogut ser parchejat; l'un no treu l'altre).
    cmd.arg("-EnablePlugins=PythonScriptPlugin,RemoteControl");

    // Script d'inicialització que obre el servidor HTTP de Remote Control.
    // EXECUTEPYTHONSCRIPT és el mètode FIABLE: s'executa un cop el motor
    // inicialitzat i admet rutes amb espais. L'antic «-ExecCmds=python <ruta>»
    // es trencava amb rutes amb espais («/Users/Shared/Epic Games/…») i
    // s'executava abans que el plugin Python estigués llest.
    if let Some(init_script) = ensure_bootstrap_script() {
        cmd.arg(format!(
            "-ExecutePythonScript={}",
            init_script.to_string_lossy()
        ));
    }

    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    cmd.spawn()
        .map_err(|e| anyhow!("No s'ha pogut arrencar Unreal Editor: {}", e))?;

    Ok(())
}

/// Ruta del script Python d'arrencada (unreal-scripts/bootstrap.py).
fn init_script_path() -> Option<PathBuf> {
    // 1) costat de l'executable (bundle resources)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("unreal-scripts").join("bootstrap.py");
            if p.exists() {
                return Some(p);
            }
            // .app de macOS: Contents/MacOS/<exe> -> Contents/Resources/unreal-scripts
            if let Some(contents) = dir.parent() {
                let p = contents
                    .join("Resources")
                    .join("unreal-scripts")
                    .join("bootstrap.py");
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    // 2) carpeta de treball (en desenvolupament)
    let p = PathBuf::from("unreal-scripts").join("bootstrap.py");
    if p.exists() {
        return Some(p);
    }
    None
}

/// Bootstrap d'emergència: si el bundle no porta unreal-scripts (execucions
/// en desenvolupament), l'ESCRIU a la carpeta temporal del sistema perquè
/// `-ExecutePythonScript` sempre tingui un fitxer vàlid que obrir el RC.
/// El port ve de l'entorn que injecta `launch_editor` (mai fixe: si l'usuari
/// en configura un altre, aquest fallback obriria el port equivocat).
const BOOTSTRAP_FALLBACK: &str = r#"import os
import unreal

PORT = int(os.environ.get("NOORBIT_UNREAL_RC_PORT", "30010"))


def _log(m):
    unreal.log("[NoOrbit/bootstrap] " + m)


try:
    ctx = unreal.get_editor_subsystem(unreal.LevelEditorSubsystem).get_editor_world()
except Exception:
    ctx = None
for cmd in ("RemoteControl.EnableServer", "rc.EnableServer 1",
            "RemoteControl.SetHttpPort {}".format(PORT)):
    try:
        unreal.SystemLibrary.execute_console_command(ctx, cmd)
    except Exception:
        pass
_log("Remote Control demanat al port {}".format(PORT))
"#;

pub fn ensure_bootstrap_script() -> Option<PathBuf> {
    if let Some(p) = init_script_path() {
        return Some(p);
    }
    let dir = std::env::temp_dir().join("noorbit");
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    let p = dir.join("bootstrap.py");
    std::fs::write(&p, BOOTSTRAP_FALLBACK).ok()?;
    Some(p)
}

/// Espera que la Remote Control API estigui disponible.
pub async fn wait_for_rc(port: u16, timeout_secs: u64) -> Result<bool> {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    while std::time::Instant::now() < deadline {
        if is_rc_open(port).await {
            sleep(Duration::from_millis(800)).await;
            return Ok(true);
        }
        sleep(Duration::from_millis(1000)).await;
    }
    Ok(false)
}

/// Flux complet: detecta, arrenca si cal, espera, retorna estat.
///
/// `engine_root` és el motor triat a mà per l'usuari (p. ex. un disc
/// extern). Si no es passa, es detecta sol: primer el que hagi detectat el
/// sistema (inclosos els discs externs) i, si no hi ha res, la carpeta que
/// l'usuari hagi indicat a la interfície.
pub async fn auto_connect(
    project: &Path,
    timeout: u64,
    engine_root: Option<&Path>,
) -> Result<LaunchReport> {
    let port = rc_port();
    if is_rc_open(port).await {
        return Ok(LaunchReport {
            engine_path: String::new(),
            project_path: project.to_string_lossy().to_string(),
            launched: false,
            message: "Unreal Editor ja estava obert".into(),
        });
    }

    let chosen: PathBuf = match engine_root {
        Some(p) => p.to_path_buf(),
        None => {
            let installs = detect_engine_installations();
            if installs.is_empty() {
                return Err(anyhow!(
                    "Unreal Engine 5 no s'ha trobat.\n\
                     S'han buscat les carpetes del sistema i TOTES les unitats \
                     montades (discs externs, NAS). Si el motor és en un lloc \
                     rar, tria la carpeta del motor a mà al panell d'Unreal.\n\
                     Descarrega'l des de https://www.unrealengine.com/download"
                ));
            }
            installs[0].clone()
        }
    };

    if !is_engine_root(&chosen) {
        return Err(anyhow!(
            "«{}» no sembla una instal·lació d'Unreal Engine: hi falta la carpeta «Engine».",
            chosen.display()
        ));
    }

    let engine_root = &chosen;
    let editor = editor_binary(engine_root)
        .ok_or_else(|| anyhow!("No s'ha trobat el binari de l'editor a {:?}", engine_root))?;

    if !project.exists() {
        return Err(anyhow!(
            "El fitxer de projecte '{}' no existeix",
            project.display()
        ));
    }

    // ACTIVA els plugins que necessitem al .uproject ABANS de dur el projecte
    // a l'editor: sense PythonScriptPlugin+RemoteControl el port 30010 no
    // respondria mai i l'espera acabaria en temps exhaurit.
    let patched = ensure_project_plugins(project)?;

    launch_editor(&editor, project, &[]).await?;

    let ok = wait_for_rc(port, timeout).await?;
    if !ok {
        return Err(anyhow!(
            "Unreal Editor s'ha obert però la Remote Control API no ha respost al port \
             {} en {} s.\n\
             Els plugins «Python Editor Script» i «Remote Control API» ja estan activats \
             al projecte; si no connecta, obre l'editor amb el mateix projecte i activa \
             «Window → Virtual Production → Remote Control» (servidor HTTP al port {}).\n\
             També pots tornar-ho a provar (la primera arrencada pot trigar més: \
             compilació de shaders).",
            port, timeout, port
        ));
    }

    // Assegura els dos servidors de Remote Control (HTTP ja respon; el WS és
    // el que la intefície web de RC fara servir més endavant). Best-effort:
    // si no es pot, el HTTP que ja hi ha n'és suficient per treballar.
    let client = UnrealClient::with_port(port);
    let _ = super::remote_control::start_websocket(&client, crate::ports::UNREAL_RC_WS_PORT)
        .await;
    let _ = super::remote_control::start_http(&client, port).await;

    Ok(LaunchReport {
        engine_path: engine_root.to_string_lossy().to_string(),
        project_path: project.to_string_lossy().to_string(),
        launched: true,
        message: if patched {
            "Unreal Engine arrencat i connectat (plugins Python+RemoteControl activats al projecte)".into()
        } else {
            "Unreal Engine arrencat i connectat".into()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Crea un motor d'Unreal fingit (amb la carpeta Engine/Binaries).
    fn motor_fingit(root: &Path) {
        std::fs::create_dir_all(root.join("Engine").join("Binaries")).unwrap();
    }

    /// Carpeta temporal única per a cada prova.
    fn temp(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("noorbit-ue-{}-{}", prefix, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn troba_el_motor_dins_de_epic_games() {
        let base = temp("epic");
        motor_fingit(&base.join("Epic Games").join("UE_5.3"));
        let mut found = Vec::new();
        collect_engines_in(&base, &mut found);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert!(found[0].ends_with("UE_5.3"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn troba_el_motor_directament_al_disc_extern() {
        // Layout de l'Epic Launcher quan la unitat és un disc extern:
        // /Volumes/SSD/UE_5.4 (el motor, sense carpeta «Epic Games»).
        let vol = temp("vol");
        motor_fingit(&vol.join("UE_5.4"));
        let mut found = Vec::new();
        collect_engines_in(&vol, &mut found);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert!(found[0].ends_with("UE_5.4"));
        let _ = std::fs::remove_dir_all(&vol);
    }

    #[test]
    fn ignora_les_carpetes_que_no_son_motors() {
        // Una carpeta «UE_5.1» sense l'estructura Engine no és un motor
        // (descàrrega a mitges o carpeta buida).
        let base = temp("buida");
        std::fs::create_dir_all(base.join("Epic Games").join("UE_5.1")).unwrap();
        let mut found = Vec::new();
        collect_engines_in(&base, &mut found);
        assert!(found.is_empty(), "{:?}", found);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn el_motor_de_la_carpeta_de_l_usuari_es_trobat() {
        // Layout de Linux: ~/UnrealEngine és el motor, no un contenidor.
        let home = temp("home");
        motor_fingit(&home.join("UnrealEngine"));
        let mut found = Vec::new();
        collect_engines_in(&home, &mut found);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert!(found[0].ends_with("UnrealEngine"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn el_contenidor_renomenat_del_disc_extern_es_trobat() {
        // Cas real: l'usuari canvia el nom de la carpeta de l'Epic Launcher
        // («epic games toshi») en un disc extern.
        let vol = temp("renom");
        motor_fingit(&vol.join("epic games toshi").join("UE_5.8"));
        let mut found = Vec::new();
        collect_engines_in(&vol, &mut found);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert!(found[0].ends_with("UE_5.8"));
        let _ = std::fs::remove_dir_all(&vol);
    }

    #[test]
    fn el_motor_dins_de_games_epic_games_es_trobat() {
        // Layout de Windows: D:\Games\Epic Games\UE_5.3
        let base = temp("win");
        motor_fingit(&base.join("Games").join("Epic Games").join("UE_5.3"));
        let mut found = Vec::new();
        collect_engines_in(&base, &mut found);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert!(found[0].ends_with("UE_5.3"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn les_versio_ordena_de_la_mes_nova_a_la_mes_vella() {
        assert!(version_of(Path::new("/x/UE_5.4")) > version_of(Path::new("/x/UE_5.3")));
        assert!(version_of(Path::new("/x/UE_5.10")) > version_of(Path::new("/x/UE_5.9")));
        assert_eq!(version_of(Path::new("/x/UE_5.4")), (5, 4, 0));
    }

    #[test]
    fn el_motor_instal_lat_per_steam_es_trobat() {
        // Layout de Steam en un disc extern:
        // /Volumes/SSD/SteamLibrary/steamapps/common/Unreal Engine
        let vol = temp("steam");
        motor_fingit(
            &vol.join("SteamLibrary")
                .join("steamapps")
                .join("common")
                .join("Unreal Engine"),
        );
        let mut found = Vec::new();
        collect_engines_in(&vol, &mut found);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert!(found[0].ends_with("Unreal Engine"));
        let _ = std::fs::remove_dir_all(&vol);
    }

    #[test]
    fn el_bundle_del_launcher_d_epic_no_es_un_motor() {
        // El launcher d'Epic Games porta una carpeta «UE» dins del seu .app:
        // no és cap motor i s'ha d'excloure.
        let base = temp("launcher");
        motor_fingit(
            &base.join("Epic Games Launcher.app")
                .join("Contents")
                .join("UE"),
        );
        let mut found = Vec::new();
        collect_engines_in(&base, &mut found);
        assert!(found.is_empty(), "{:?}", found);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn la_instal_lacio_a_mitges_es_detecta_al_disc_extern() {
        // Cas real: l'Epic Launcher està baixant UE_5.8 en un disc extern
        // (hi ha el «.egstore» però encara no hi ha l'editor).
        let vol = temp("pendent");
        let eng = vol.join("epic games toshi").join("UE_5.8");
        std::fs::create_dir_all(eng.join(".egstore")).unwrap();
        let mut found = Vec::new();
        collect_pending_in(&vol, &mut found);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert!(found[0].ends_with("UE_5.8"));

        // I NO ha de ser un motor vàlid: no es pot arrencar.
        let mut engines = Vec::new();
        collect_engines_in(&vol, &mut engines);
        assert!(engines.is_empty(), "{:?}", engines);
        let _ = std::fs::remove_dir_all(&vol);
    }

    #[test]
    fn un_motor_complet_no_es_marca_com_a_pendent() {
        let vol = temp("complet");
        let eng = vol.join("UE_5.6");
        motor_fingit(&eng);
        std::fs::create_dir_all(eng.join(".egstore")).unwrap();
        let mut pending = Vec::new();
        collect_pending_in(&vol, &mut pending);
        assert!(pending.is_empty(), "{:?}", pending);
        let _ = std::fs::remove_dir_all(&vol);
    }

    #[test]
    fn la_deteccio_no_repeteix_el_motor() {
        // El mateix motor es pot veure des de dues bases: només un cop.
        let base = temp("dup");
        let engine = base.join("Epic Games").join("UE_5.3");
        motor_fingit(&engine);
        let mut found = Vec::new();
        collect_engines_in(&base, &mut found);
        collect_engines_in(&base, &mut found);
        let mut seen = std::collections::HashSet::new();
        found.retain(|p| seen.insert(p.clone()));
        assert_eq!(found.len(), 1, "{:?}", found);
        let _ = std::fs::remove_dir_all(&base);
    }
}
