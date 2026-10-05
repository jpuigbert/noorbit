//! Integració amb Blender: client socket JSON + execució headless.
//!
//! L'add-on "NoOrbit Bridge" dins Blender ha d'escoltar a 127.0.0.1:9876
//! amb un protocol simple de línies JSON:
//!   -> {"key":"noorbit", "command":"execute", "argument":{"code":"..."}}
//!   <- {"exec":"...captura base64 o null..."}
//!
//! Compatibilitat addicional: si qui escolta al 9876 és l'add-on comunitari
//! **blender-mcp** (github.com/ahujasid/mcp-for-blender), parlem directament
//! amb el seu protocol:
//!   -> {"type":"execute_code", "params":{"code":"..."}}
//!   <- {"status":"success", "result":{"executed":true, "result":"stdout..."}}
//! d'aquesta manera l'usuari no cal que instal·li el pont de NoOrbit.

pub mod auto_connect;
pub mod commands;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};

pub const DEFAULT_SOCKET_PORT: u16 = 9876;
const AUTH_KEY: &str = "noorbit";

/// Acumula bytes del socket fins a tenir un objecte JSON vàlid. L'add-on
/// blender-mcp respon sense cap delimitador de línia, així que read_line
/// es blocaria: parsejam intent rere intent amb lectura per bytes.
fn read_json_object(reader: &mut BufReader<TcpStream>) -> Result<serde_json::Value> {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        // només objectes/arrels JSON: el blender-mcp sempre respon un {}
        if !buf.is_empty() {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&buf) {
                return Ok(v);
            }
        }
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            return Err(anyhow!("Connexió tancada per Blender abans de respondre"));
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > 32 * 1024 * 1024 {
            return Err(anyhow!("Resposta de Blender massa gran"));
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlenderInfo {
    pub version: Option<String>,
    pub file: Option<String>,
    pub objects: Option<usize>,
}

/// Client TCP de l'add-on de Blender.
pub struct BlenderSocketClient {
    port: u16,
}

impl BlenderSocketClient {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    fn connect(&self) -> Result<TcpStream> {
        TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], self.port)),
            Duration::from_millis(800),
        )
        .map_err(|e| anyhow!("Blender no respon al port {}: {}", self.port, e))
    }

    fn request(&self, command: &str, argument: serde_json::Value) -> Result<serde_json::Value> {
        let mut stream = self.connect()?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        let payload = serde_json::json!({
            "key": AUTH_KEY,
            "command": command,
            "argument": argument,
        });
        writeln!(stream, "{}", payload)?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.trim().is_empty() {
            return Err(anyhow!("Resposta buida de Blender"));
        }
        // l'add-on pot enviar diverses línies; agafem la darrera JSON vàlida
        let mut value: serde_json::Value = serde_json::from_str(&line)?;
        if value.get("final").and_then(|f| f.as_bool()) == Some(false) {
            let mut next = String::new();
            while reader.read_line(&mut next)? > 0 {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(next.trim()) {
                    let done = v.get("final").and_then(|f| f.as_bool()) != Some(false);
                    value = v;
                    if done {
                        break;
                    }
                }
                next.clear();
            }
        }
        Ok(value)
    }

    pub fn ping(&self) -> bool {
        self.connect().is_ok()
    }

    /// Pregunta al qui escolta al 9876 si parla el protocol blender-mcp
    /// (`{"type":"ping"}` → `{"status":"success","result":{"pong":true}}`).
    /// El pont propi de NoOrbit l'ignora, així que la sonda és inofensiva.
    pub fn is_mcp_addon(&self) -> bool {
        self.request_mcp(serde_json::json!({ "type": "ping" }), Duration::from_millis(900))
            .map(|resp| {
                resp.get("result")
                    .and_then(|r| r.get("pong"))
                    .and_then(|p| p.as_bool())
                    == Some(true)
            })
            .unwrap_or(false)
    }

    /// Consulta al protocol blender-mcp: un únic JSON de resposta sense
    /// delimitadors de línia (per això usem read_json_object).
    fn request_mcp(&self, payload: serde_json::Value, timeout: Duration) -> Result<serde_json::Value> {
        let mut stream = self.connect()?;
        stream.set_read_timeout(Some(timeout))?;
        let mut json = serde_json::to_string(&payload)?;
        json.push('\n');
        stream.write_all(json.as_bytes())?;
        stream.flush()?;
        let mut reader = BufReader::new(stream);
        read_json_object(&mut reader)
    }

    /// Executa codi amb el protocol de l'add-on comunitari **blender-mcp**
    /// (execute_code). La sortida de `print` ve en result.result.
    fn execute_mcp(&self, code: &str) -> Result<String> {
        let payload = serde_json::json!({
            "type": "execute_code",
            "params": { "code": code },
        });
        let resp = self.request_mcp(payload, Duration::from_secs(60))?;
        // El blender-mcp embolica l'excepció com un JSON string dins message.
        if resp.get("status").and_then(|s| s.as_str()) != Some("success") {
            let msg = resp
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Blender ha retornat error");
            return Err(anyhow!("{}", msg));
        }
        let result = resp.get("result").cloned().unwrap_or(serde_json::Value::Null);
        // execute_code → {"executed":true,"result":"stdout"}; altres → objecte cru.
        if let Some(inner) = result.get("result") {
            if let Some(s) = inner.as_str() {
                return Ok(s.to_string());
            }
            return Ok(inner.to_string());
        }
        Ok(result.to_string())
    }

    pub fn info(&self) -> Result<BlenderInfo> {
        let code = r#"
import bpy, json
print(json.dumps({
    "version": bpy.app.version_string,
    "file": bpy.data.filepath,
    "objects": len(bpy.context.scene.objects),
}))
"#;
        // execute_auto: funciona tant amb el pont propi com amb blender-mcp;
        // ambdós protocols retornen el stdout del `print` com a text.
        let stdout = self.execute_auto(code)?;
        let parsed: serde_json::Value = stdout
            .lines()
            .rev()
            .find_map(|l| serde_json::from_str(l.trim()).ok())
            .unwrap_or(serde_json::Value::Null);
        Ok(BlenderInfo {
            version: parsed.get("version").and_then(|v| v.as_str()).map(String::from),
            file: parsed.get("file").and_then(|v| v.as_str()).map(String::from),
            objects: parsed.get("objects").and_then(|v| v.as_u64()).map(|v| v as usize),
        })
    }

    /// Executa codi Python dins l'editor connectat.
    pub fn execute(&self, code: &str) -> Result<String> {
        let resp = self.request("execute", serde_json::json!({ "code": code }))?;
        if let Some(err) = resp.get("exec").and_then(|e| e.as_str()) {
            if err.contains("Error") {
                return Err(anyhow!("{}", err));
            }
        }
        Ok(resp
            .get("exec")
            .and_then(|e| e.as_str())
            .unwrap_or("")
            .to_string())
    }

    /// Demana una captura del viewport en base64 (PNG).
    pub fn screenshot(&self) -> Result<Option<String>> {
        let resp = self.request("execute", serde_json::json!({ "code": SCREENSHOT_CODE }))?;
        let raw = resp.get("exec").and_then(|e| e.as_str()).unwrap_or("");
        let value: serde_json::Value = raw
            .lines()
            .rev()
            .find_map(|l| serde_json::from_str(l.trim()).ok())
            .unwrap_or(serde_json::Value::Null);
        Ok(value.get("img").and_then(|i| i.as_str()).map(String::from))
    }

    /// Executa codi Python dins l'editor connectat, **autodejectant** quin
    /// add-on escolta al 9876: el pont propi de NoOrbit o el comunitari
    /// blender-mcp. Així l'usuari pot instal·lar qualsevol dels dos.
    pub fn execute_auto(&self, code: &str) -> Result<String> {
        if self.is_mcp_addon() {
            return self.execute_mcp(code);
        }
        self.execute(code)
    }
}

const SCREENSHOT_CODE: &str = r#"
import bpy, json, base64, os, tempfile
try:
    path = os.path.join(tempfile.gettempdir(), "noorbit_blender_capture.png")
    scene = bpy.context.scene
    scene.render.resolution_x = 640
    scene.render.resolution_y = 360
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    with open(path, "rb") as f:
        print(json.dumps({"img": base64.b64encode(f.read()).decode()}))
except Exception as e:
    print(json.dumps({"img": None, "error": str(e)}))
"#;

/// Detecta el binari de Blender al sistema.
pub fn detect_blender_binary() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let candidates = [
            "/Applications/Blender.app/Contents/MacOS/Blender",
        ];
        for c in candidates {
            if std::path::Path::new(c).exists() {
                return Some(c.into());
            }
        }
        // Versions amb sufix: /Applications/Blender 4.x.app
        if let Ok(entries) = std::fs::read_dir("/Applications") {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("Blender") && name.ends_with(".app") {
                    let p = e
                        .path()
                        .join("Contents")
                        .join("MacOS")
                        .join("Blender");
                    if p.exists() {
                        return Some(p.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(p) = which::which("blender") {
            return Some(p.to_string_lossy().to_string());
        }
    }
    which::which("blender").ok().map(|p| p.to_string_lossy().to_string())
}

/// Executa un script headless amb el binari de Blender.
pub fn run_blender_headless(code: &str) -> Result<String> {
    let binary = detect_blender_binary().ok_or_else(|| anyhow!("Blender no instal·lat"))?;
    let tmp = std::env::temp_dir().join(format!("noorbit_blender_{}.py", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, code)?;
    let out = std::process::Command::new(&binary)
        .arg("-b")
        .arg("-P")
        .arg(&tmp)
        .output()?;
    let _ = std::fs::remove_file(&tmp);
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(anyhow!(
            "Blender ha fallat: {}",
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

/// Estat global de Blender: instal·lat (binari) i connectat (port obert).
#[derive(Debug, Clone, Serialize)]
pub struct BlenderStatus {
    pub installed: bool,
    pub running: bool,
    pub binary: Option<String>,
}

/// Diu si el port del pont està escoltant.
pub fn port_open(port: u16) -> bool {
    TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(400),
    )
    .is_ok()
}

pub fn status() -> BlenderStatus {
    let binary = detect_blender_binary();
    BlenderStatus {
        installed: binary.is_some(),
        running: port_open(DEFAULT_SOCKET_PORT),
        binary,
    }
}

/// Localitza el script pont `blender-scripts/noorbit_bridge.py`, ja siga dins
/// del bundle (recursos) o a la carpeta del repo en desenvolupament.
pub fn locate_bridge(app: &AppHandle) -> Option<PathBuf> {
    const REL: &str = "blender-scripts/noorbit_bridge.py";
    // 1) recursos resoltos per Tauri (funciona en el .app distribuït)
    if let Ok(p) = app.path().resolve(REL, BaseDirectory::Resource) {
        if p.exists() {
            return Some(p);
        }
    }
    // 2) costat de l'executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("blender-scripts").join("noorbit_bridge.py");
            if p.exists() {
                return Some(p);
            }
            // .app/Contents/Resources/blender-scripts/...
            if let Some(contents) = dir.parent() {
                let p2 = contents
                    .join("Resources")
                    .join("blender-scripts")
                    .join("noorbit_bridge.py");
                if p2.exists() {
                    return Some(p2);
                }
            }
        }
    }
    // 3) carpeta de treball (en desenvolupament)
    let p = PathBuf::from(REL);
    if p.exists() {
        return Some(p);
    }
    None
}

/// Arrenca Blender amb el pont (via `-P`) perquè obri el port 9876.
pub fn launch_with_bridge(app: &AppHandle) -> Result<String> {
    if port_open(DEFAULT_SOCKET_PORT) {
        return Ok("Ja hi ha un Blender connectat".into());
    }
    let binary = detect_blender_binary().ok_or_else(|| anyhow!("Blender no instal·lat"))?;
    let bridge = locate_bridge(app)
        .ok_or_else(|| anyhow!("No trobo el connector noorbit_bridge.py"))?;
    std::process::Command::new(&binary)
        .arg("-P")
        .arg(&bridge)
        .spawn()
        .map_err(|e| anyhow!("No s'ha pogut arrencar Blender: {}", e))?;
    // Espera fins que el port obri (fins a ~30 s).
    for _ in 0..60 {
        if port_open(DEFAULT_SOCKET_PORT) {
            return Ok("Blender connectat".into());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Ok("Blender arrencat; pot trigar a obrir el connector".into())
}

/// Embolica un `.py` en un `.zip` (Blender 4.2+ pot refusar fitxers solts a
/// `addon_install`). Retorna el camí del zip creat.
fn wrap_py_as_zip(py_path: &PathBuf) -> Result<PathBuf> {
    use std::io::Write;
    let stem = py_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("addon");
    let zip_path = std::env::temp_dir().join(format!("noorbit-{}.zip", stem));
    let file = std::fs::File::create(&zip_path)
        .map_err(|e| anyhow!("No s'ha pogut crear el zip: {}", e))?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file(format!("{}.py", stem), opts)
        .map_err(|e| anyhow!("zip: {}", e))?;
    let content = std::fs::read(py_path).map_err(|e| anyhow!("llegeix py: {}", e))?;
    writer
        .write_all(&content)
        .map_err(|e| anyhow!("escriu zip: {}", e))?;
    writer.finish().map_err(|e| anyhow!("tanca zip: {}", e))?;
    Ok(zip_path)
}

/// Instal·la i activa l'add-on dins Blender (headless), perquè arrencue sol
/// cada vegada que l'usuari obri Blender normalment.
pub fn install_addon(app: &AppHandle) -> Result<String> {
    let binary = detect_blender_binary().ok_or_else(|| anyhow!("Blender no instal·lat"))?;
    let bridge = locate_bridge(app)
        .ok_or_else(|| anyhow!("No trobo el connector noorbit_bridge.py"))?;
    // Empaquetem en zip per compatibilitat amb Blender 4.2+, que pot refusar
    // l'instal·lació de fitxers .py solts.
    let target = if bridge.extension() == Some(std::ffi::OsStr::new("zip")) {
        bridge
    } else {
        wrap_py_as_zip(&bridge)?
    };
    let path = target.to_string_lossy().replace('\'', "\\'");
    let expr = format!(
        "import bpy; bpy.ops.preferences.addon_install(filepath=r'{}'); \
         bpy.ops.preferences.addon_enable(module='noorbit_bridge'); \
         bpy.ops.wm.save_userpref(); print('NOORBIT_ADDON_OK')",
        path
    );
    let out = std::process::Command::new(&binary)
        .arg("--background")
        .arg("--python-expr")
        .arg(&expr)
        .output()
        .map_err(|e| anyhow!("Blender ha fallat: {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if stdout.contains("NOORBIT_ADDON_OK") {
        Ok("Connector instal·lat i activat".into())
    } else {
        Err(anyhow!(
            "No s'ha pogut instal·lar: {}",
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

/// URL cru del codi de l'add-on comunitari blender-mcp (ahujasid). Obre el
/// mateix port 9876 que NoOrbit parla, així que no cal servidor MCP extern.
const MCP_ADDON_URL: &str =
    "https://raw.githubusercontent.com/ahujasid/blender-mcp/main/addon.py";

/// Descarrega i instal·la automàticament l'add-on comunitari **blender-mcp**
/// dins Blender (headless). Es copia directament a la carpeta d'add-ons i
/// s'activa amb un mòdul propi (`noorbit_blender_mcp`), perquè així NoOrbit
/// li pot parlar pel protocol del port 9876 sense cap client MCP intermedi.
pub fn install_mcp_addon() -> Result<String> {
    let binary = detect_blender_binary().ok_or_else(|| anyhow!("Blender no instal·lat"))?;

    // 1) Descarregar addon.py (amb reqwest bloquejant; s'executa en spawn_blocking)
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent("NoOrbit")
        .build()
        .map_err(|e| anyhow!("client HTTP: {}", e))?;
    let resp = client
        .get(MCP_ADDON_URL)
        .send()
        .map_err(|e| anyhow!("No s'ha pogut descarregar l'add-on: {}", e))?;
    if !resp.status().is_success() {
        return Err(anyhow!(
            "Descàrrega de l'add-on fallida (HTTP {})",
            resp.status()
        ));
    }
    let body = resp.bytes().map_err(|e| anyhow!("cos de la resposta: {}", e))?;
    if body.len() < 2000 {
        return Err(anyhow!("Fitxer descarregat massa petit; sembla invàlid"));
    }

    // 2) esbrinar la carpeta d'add-ons d'aquest Blender (headless, imprimeix ruta)
    let out = std::process::Command::new(&binary)
        .arg("--background")
        .arg("--python-expr")
        .arg("import bpy; print('NOORBIT_ADDONS_DIR=' + bpy.utils.script_paths_pref()['addons'])")
        .output()
        .map_err(|e| anyhow!("Blender ha fallat: {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let addons_dir = stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("NOORBIT_ADDONS_DIR=").map(String::from))
        .ok_or_else(|| anyhow!("No s'ha pogut determinar la carpeta d'add-ons de Blender"))?;
    std::fs::create_dir_all(&addons_dir)
        .map_err(|e| anyhow!("crea carpeta d'add-ons: {}", e))?;

    // 3) copiar el codi descarregat com a mòdul propi dins la carpeta
    let dest = PathBuf::from(&addons_dir).join("noorbit_blender_mcp.py");
    std::fs::write(&dest, &body).map_err(|e| anyhow!("escriu l'add-on: {}", e))?;

    // 4) activar-lo i desar preferències (headless)
    let expr = "import bpy; \
                bpy.ops.preferences.addon_enable(module='noorbit_blender_mcp'); \
                bpy.ops.wm.save_userpref(); print('NOORBIT_MCP_ADDON_OK')";
    let out2 = std::process::Command::new(&binary)
        .arg("--background")
        .arg("--python-expr")
        .arg(expr)
        .output()
        .map_err(|e| anyhow!("Blender ha fallat: {}", e))?;
    let stdout2 = String::from_utf8_lossy(&out2.stdout).to_string();
    if stdout2.contains("NOORBIT_MCP_ADDON_OK") {
        Ok("Add-on blender-mcp descarregat, instal·lat i activat. Quan obres Blender, obri el panell N › pestanya «MCP for Blender» i prem «Connect» si cal; NoOrbit el detectarà sol al port 9876.".into())
    } else {
        Err(anyhow!(
            "No s'ha pogut activar l'add-on: {}",
            String::from_utf8_lossy(&out2.stderr)
        ))
    }
}
