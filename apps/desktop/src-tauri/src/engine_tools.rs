//! Eines de MOTOR per a la feina autònoma (Jam i treball autopilotat).
//!
//! El model, a més d'escriure fitxers (`@file:` / `@edit:`), pot dirigir
//! motors reals amb directrius dins del text de la seva resposta:
//!
//! ```text
//! @unreal: {"tool":"spawn_actor","args":{...}}   → eina MCP d'Unreal (UE 5.8+)
//! @unreal-py: <codi python>                      → Python dins Unreal via Remote Control
//! @blender: <codi bpy>                           → Python dins Blender connectat
//! @blender-headless: <codi bpy>                  → Blender en segon pla (exports/renders)
//! @blender-video: <ruta .blend>                  → renderitza el vídeo de l'animació
//! @godot: import | run [escena] | script <gd>    → Godot 4 en headless (importa, executa, depura)
//! @bevy: check | build | run | test               → Bevy (Rust) via cargo
//! @import: <ruta> [motor]                         → importa un asset al motor del joc
//! @video: <acció> [arguments]                    → muntatge de vídeo (Blender → Kdenlive/ffmpeg)
//! ```
//!
//! Cada execució torna el resultat al model perquè continuï o corregeixi.
//! Les operacions queden registrades a `EngineWork/noorbit-engine.json` dins
//! del projecte: commitejable, visible per al verificador i per a l'usuari.
//!
//! Autodetecció: si la carpeta del projecte conté un `.uproject` (arrel o un
//! nivell), s'ofereixen les eines d'Unreal; si Blender és a l'abast (pont al
//! port o binari instal·lat), les de Blender; si hi ha un `project.godot` les
//! de Godot; si hi ha un `Cargo.toml` amb `bevy`, les de Bevy. Així la IA
//! decideix sola quan li convé cada motor — i pot encadenar-los (Blender crea
//! i exporta, Unreal importa) sense que l'usuari hagi de configurar res.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Rondes d'eines MÀXIMES per tasca/passada (evita bucles infinits amb
/// models petits que sempre volen «una eina més»).
pub const MAX_TOOL_ROUNDS: usize = 4;
/// Text de resultat d'eina retallat per no inflar el context del model.
pub const MAX_RESULT_TEXT: usize = 3000;

/// Carpeta dins del projecte on viuen els artefactes de motor (registre,
/// exports, vídeos renderitzats).
pub const ENGINE_WORK_DIR: &str = "EngineWork";
const ENGINE_LOG: &str = "EngineWork/noorbit-engine.json";

// ─────────────────────────────────────────────────────────────────────────────
//  CONTEXT: què hi ha disponible per al projecte actual
// ─────────────────────────────────────────────────────────────────────────────

/// Motors detectats per a la carpeta del projecte.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineContext {
    pub root: PathBuf,
    /// Projecte d'Unreal detectat (fitxer `.uproject`).
    pub uproject: Option<PathBuf>,
    /// Blender a l'abast: pont obert al port o binari instal·lat.
    pub blender: bool,
    /// Eines de muntatge a l'abast (`ffmpeg` + `ffprobe`, normalment dins el
    /// paquet de Kdenlive). Només es miren els camins: cap subprocess.
    pub video: bool,
    /// Projecte de Godot detectat (fitxer `project.godot`).
    pub godot: Option<PathBuf>,
    /// Projecte de Bevy detectat (`Cargo.toml` amb la dependència `bevy`).
    pub bevy: Option<PathBuf>,
}

impl EngineContext {
    /// Detecta els motors del projecte: `.uproject` a l'arrel o un nivell,
    /// Blender (port del pont obert o binari instal·lat), les eines de
    /// muntatge de vídeo, `project.godot` (Godot) i `Cargo.toml` amb `bevy`.
    pub fn detect(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            uproject: find_uproject(root),
            blender: blender_available(),
            video: crate::video::detect::locate("ffmpeg").is_some()
                && crate::video::detect::locate("ffprobe").is_some(),
            godot: find_godot_project(root),
            bevy: find_bevy_project(root),
        }
    }

    /// Hi ha alguna eina de motor disponible?
    pub fn any(&self) -> bool {
        self.uproject.is_some()
            || self.blender
            || self.video
            || self.godot.is_some()
            || self.bevy.is_some()
    }
}

/// Projecte de Godot: `project.godot` a l'arrel o a un nivell.
///
/// La detecció viu a `crate::godot` (mateix detector que l'editor i
/// l'assistent d'instal·lació): aquí només s'en serveix el context.
fn find_godot_project(root: &Path) -> Option<PathBuf> {
    crate::godot::find_project(root)
}

/// Projecte de Bevy: `Cargo.toml` (arrel o un nivell) que depèn de `bevy`.
fn find_bevy_project(root: &Path) -> Option<PathBuf> {
    crate::bevy::find_project(root)
}

/// Cerca un `.uproject` a l'arrel o a un nivell de subcarpetes (els
/// projectes UE solen tenir el fitxer a l'arrel o dins d'una carpeta).
pub fn find_uproject(root: &Path) -> Option<PathBuf> {
    let mut scan = vec![root.to_path_buf()];
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                scan.push(e.path());
            }
        }
    }
    for dir in scan {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some("uproject") {
                    return Some(p);
                }
            }
        }
    }
    None
}

fn blender_port() -> u16 {
    crate::config::AppConfig::load()
        .ok()
        .map(|c| c.blender.socket_port)
        .filter(|p| *p != 0)
        .unwrap_or(crate::blender::DEFAULT_SOCKET_PORT)
}

fn blender_available() -> bool {
    crate::blender::port_open(blender_port()) || crate::blender::detect_blender_binary().is_some()
}

// ─────────────────────────────────────────────────────────────────────────────
//  DIRECTRIUS: detecció i extracció del text de la resposta
// ─────────────────────────────────────────────────────────────────────────────

/// Tipus de directriu de motor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DKind {
    /// Eina MCP d'Unreal (`@unreal: {"tool": …, "args": {…}}`).
    Unreal,
    /// Python dins l'editor Unreal via Remote Control (`@unreal-py: <codi>`).
    UnrealPy,
    /// Python dins Blender connectat (`@blender: <codi bpy>`).
    Blender,
    /// Python en Blender headless (`@blender-headless: <codi bpy>`).
    BlenderHeadless,
    /// Render de vídeo d'un `.blend` (`@blender-video: <ruta>`).
    BlenderVideo,
    /// Godot 4 en headless (`@godot: import | run [escena] | script <gd>`).
    Godot,
    /// Bevy (Rust) via cargo (`@bevy: check | build | run | test`).
    Bevy,
    /// Importació d'un asset al motor del joc (`@import: <ruta>`).
    Import,
    /// Muntatge de vídeo dirigit per IA (`@video: <acció> [arguments]`).
    Video,
}

impl DKind {
    pub fn name(&self) -> &'static str {
        match self {
            DKind::Unreal => "unreal",
            DKind::UnrealPy => "unreal-py",
            DKind::Blender => "blender",
            DKind::BlenderHeadless => "blender-headless",
            DKind::BlenderVideo => "blender-video",
            DKind::Godot => "godot",
            DKind::Bevy => "bevy",
            DKind::Import => "import",
            DKind::Video => "video",
        }
    }
}

/// Una directriu extreta de la resposta del model.
#[derive(Debug, Clone, PartialEq)]
pub struct Directive {
    pub kind: DKind,
    pub payload: String,
}

/// Prefixos de directriu ordenats del més llarg al més curt (perquè
/// `@blender-headless:` no es confongui amb `@blender:`).
const DIRECTIVES: &[(DKind, &str); 9] = &[
    (DKind::BlenderHeadless, "@blender-headless:"),
    (DKind::BlenderVideo, "@blender-video:"),
    (DKind::UnrealPy, "@unreal-py:"),
    (DKind::Unreal, "@unreal:"),
    (DKind::Blender, "@blender:"),
    (DKind::Godot, "@godot:"),
    (DKind::Bevy, "@bevy:"),
    (DKind::Import, "@import:"),
    (DKind::Video, "@video:"),
];

/// Línia de protocol que TANCA un bloc de codi (una altra directriu, un
/// fitxer o el resum): el codi de motor no ha d'empassar-se'ls.
fn is_block_ender(t: &str) -> bool {
    DIRECTIVES.iter().any(|(_, p)| t.starts_with(p))
        || t.starts_with("@file:")
        || t.starts_with("@edit:")
        || t.starts_with("RESUM:")
}

/// Línia amb pinta de JSON (per ajuntar `@unreal:` multi-línia).
fn is_jsonish(t: &str) -> bool {
    t.starts_with('{')
        || t.starts_with('}')
        || t.starts_with('"')
        || t.starts_with(',')
        || (t.contains(':') && (t.contains('{') || t.contains('}') || t.contains('"')))
}

/// Extrau TOTES les directrius de motor del text de la resposta, en ordre.
/// Els blocs de codi multi-línia s'acaben quan apareix una línia no buida
/// que no sembla codi (prosa), una altra directriu o un marcador de protocol.
pub fn parse_directives(text: &str) -> Vec<Directive> {
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    let mut in_fence = false;
    while let Some(line) = lines.next() {
        let t = line.trim();
        // El codi d'exemple dins de ``` mai s'executa: si el model escriu un
        // fitxer amb text que sembla una directriu, no ha de disparar cap eina.
        if t.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let Some((kind, prefix)) = DIRECTIVES.iter().find(|(_, p)| t.starts_with(p)) else {
            continue;
        };
        let rest = t[prefix.len()..].trim().to_string();
        let payload = match kind {
            // JSON d'una línia o multi-línia (s'ajunten línies amb pinta de JSON
            // fins que el document parsegi o s'acabi el que sembla JSON).
            DKind::Unreal => {
                let mut text_json = rest;
                while serde_json::from_str::<Value>(&text_json).is_err() {
                    match lines.peek() {
                        Some(l) if l.trim().is_empty() => {
                            lines.next();
                            continue;
                        }
                        Some(l) if is_jsonish(l.trim()) => {
                            text_json.push_str(l.trim());
                            lines.next();
                        }
                        _ => break,
                    }
                }
                text_json
            }
            // Ruta o comanda d'una sola línia.
            DKind::BlenderVideo | DKind::Godot | DKind::Bevy | DKind::Import => rest,
            // Acció de muntatge: `status`, `scan`, `auto 20`, `open <ruta>` són
            // d'una línia; `mount` i `director` poden portar un JSON rere.
            DKind::Video => {
                let mut text_v = rest;
                if text_v.trim_start().starts_with('{') {
                    while serde_json::from_str::<Value>(&text_v).is_err() {
                        match lines.peek() {
                            Some(l) if l.trim().is_empty() => {
                                lines.next();
                                continue;
                            }
                            Some(l) if is_jsonish(l.trim()) => {
                                text_v.push_str(l.trim());
                                lines.next();
                            }
                            _ => break,
                        }
                    }
                }
                text_v
            }
            // Blocs de codi multi-línia.
            _ => {
                let mut code: Vec<String> = Vec::new();
                if !rest.is_empty() {
                    code.push(rest);
                }
                let mut in_block_fence = false;
                while let Some(next) = lines.peek().copied() {
                    let nt = next.trim();
                    // El codi embolcallat en ```python dins de la directriu
                    // s'inclou SENSE els marcadors de tanca.
                    if in_block_fence {
                        lines.next();
                        if nt.starts_with("```") {
                            in_block_fence = false;
                        } else {
                            code.push(next.to_string());
                        }
                        continue;
                    }
                    if nt.starts_with("```") {
                        in_block_fence = true;
                        lines.next();
                        continue;
                    }
                    if nt.is_empty() {
                        code.push(next.to_string());
                        lines.next();
                        continue;
                    }
                    if is_block_ender(nt) {
                        break;
                    }
                    let cont = crate::blender::looks_like_code_line(next)
                        || next.starts_with(' ') // indentació = continuació de bloc
                        || next.starts_with('\t');
                    if !cont {
                        break; // prosa: el bloc s'ha acabat
                    }
                    code.push(next.to_string());
                    lines.next();
                }
                while code.last().map(|s| s.trim().is_empty()).unwrap_or(false) {
                    code.pop();
                }
                code.join("\n")
            }
        };
        if !payload.trim().is_empty() {
            out.push(Directive { kind: *kind, payload });
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
//  EXECUCIÓ: cada directriu contra el motor real
// ─────────────────────────────────────────────────────────────────────────────

/// Resultat d'executar una directriu (torna al model i a l'usuari).
#[derive(Debug, Clone)]
pub struct EngineOutcome {
    pub kind: &'static str,
    pub ok: bool,
    pub summary: String,
    /// Fitxers creats dins el projecte (inclòs el registre d'artefactes).
    pub artifacts: Vec<PathBuf>,
}

pub fn clip(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= n {
        return t.to_string();
    }
    let mut out: String = t.chars().take(n).collect();
    out.push_str("…");
    out
}

/// Executa una directriu contra el motor corresponent i registra
/// l'artefacte al projecte. Mai falla el procés sencer: els errors tornen
/// com a `ok = false` amb un missatge que el model pot entendre i corregir.
pub async fn execute(ctx: &EngineContext, d: &Directive) -> EngineOutcome {
    let outcome = match d.kind {
        DKind::Unreal => run_unreal_tool(&d.payload).await,
        DKind::UnrealPy => run_unreal_python(&d.payload).await,
        DKind::Blender => run_blender_live(&d.payload, ctx).await,
        DKind::BlenderHeadless => run_blender_headless(&d.payload).await,
        DKind::BlenderVideo => run_blender_video(&d.payload, ctx).await,
        DKind::Godot => run_godot(&d.payload, ctx).await,
        DKind::Bevy => run_bevy(&d.payload, ctx).await,
        DKind::Import => run_import(&d.payload, ctx).await,
        DKind::Video => run_video_directive(&d.payload, ctx).await,
    };
    let artifacts = match log_outcome(ctx, &outcome) {
        Ok(p) => vec![p],
        Err(_) => Vec::new(),
    };
    EngineOutcome {
        artifacts,
        ..outcome
    }
}

/// Eina MCP d'Unreal: `{"tool":"spawn_actor","args":{...}}` (també accepta
/// `name`/`arguments`, que és com alguns models ho generen).
async fn run_unreal_tool(payload: &str) -> EngineOutcome {
    let fail = |msg: String| EngineOutcome {
        kind: "unreal",
        ok: false,
        summary: msg,
        artifacts: Vec::new(),
    };
    let parsed: Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            return fail(format!(
                "@unreal: arguments invàlids ({e}). Format: @unreal: {{\"tool\":\"nom\",\"args\":{{...}}}}"
            ))
        }
    };
    let name = parsed
        .get("tool")
        .or_else(|| parsed.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if name.is_empty() {
        return fail("faltava el nom de l'eina al JSON d'@unreal:".into());
    }
    let args = parsed
        .get("args")
        .or_else(|| parsed.get("arguments"))
        .cloned()
        .unwrap_or(Value::Object(serde_json::Map::new()));

    let sess = match crate::unreal::mcp::open_session().await {
        Ok(s) => s,
        Err(e) => {
            return fail(format!(
                "no s'ha pogut connectar amb el servidor MCP d'Unreal (port 8000): {e}. \
Si l'editor és UE 5.8+ amb el plugin MCP actiu, obre'l o reinicia'l. \
Alternativa: fes servir @unreal-py: amb Python de la Remote Control."
            ))
        }
    };
    if !sess.tools.iter().any(|t| t.name == name) {
        let list: Vec<&str> = sess.tools.iter().map(|t| t.name.as_str()).take(12).collect();
        return fail(format!(
            "l'eina «{name}» no existeix al catàleg MCP d'Unreal. Primeres eines disponibles: {}",
            list.join(", ")
        ));
    }
    match sess.client.tools_call(name, args).await {
        Ok(r) => {
            if r.is_error {
                EngineOutcome {
                    kind: "unreal",
                    ok: false,
                    summary: format!("l'eina «{name}» ha tornat error: {}", clip(&r.text, MAX_RESULT_TEXT)),
                    artifacts: Vec::new(),
                }
            } else {
                EngineOutcome {
                    kind: "unreal",
                    ok: true,
                    summary: format!("eina «{name}» executada: {}", clip(&r.text, MAX_RESULT_TEXT)),
                    artifacts: Vec::new(),
                }
            }
        }
        Err(e) => fail(format!("la crida de l'eina «{name}» ha fallat: {e}")),
    }
}

/// Python dins l'editor Unreal via Remote Control (funciona des de UE 5.0;
/// és la via de reserva quan el plugin MCP no hi és).
async fn run_unreal_python(payload: &str) -> EngineOutcome {
    let fail = |msg: String| EngineOutcome {
        kind: "unreal-py",
        ok: false,
        summary: msg,
        artifacts: Vec::new(),
    };
    let client = crate::unreal::client::UnrealClient::new();
    match crate::unreal::python::run_script(&client, payload).await {
        Ok(r) if r.success => EngineOutcome {
            kind: "unreal-py",
            ok: true,
            summary: {
                let out = clip(&r.output.join("\n"), MAX_RESULT_TEXT);
                if out.is_empty() {
                    "Python executat a Unreal (sense sortida)".to_string()
                } else {
                    format!("Python executat a Unreal:\n{out}")
                }
            },
            artifacts: Vec::new(),
        },
        Ok(r) => fail(format!(
            "el Python dins Unreal ha fallat: {}",
            clip(&r.errors.join("\n"), MAX_RESULT_TEXT)
        )),
        Err(e) => fail(format!(
            "no s'ha pogut executar Python a Unreal (Remote Control): {e}"
        )),
    }
}

/// Python dins Blender CONNECTAT (editor obert amb el pont). Si la sortida
/// porta el marcador `NOORBIT_VIDEO_OK:`, es llança el render del vídeo
/// automàticament i la ruta final torna al model.
async fn run_blender_live(payload: &str, ctx: &EngineContext) -> EngineOutcome {
    let fail = |msg: String| EngineOutcome {
        kind: "blender",
        ok: false,
        summary: msg,
        artifacts: Vec::new(),
    };
    let port = blender_port();
    if !crate::blender::port_open(port) {
        return fail(format!(
            "Blender no està connectat (port {port}). Obre'l des de NoOrbit (panell Blender ▸ Connecta) \
o fes servir @blender-headless: per treballar en segon pla."
        ));
    }
    let code = payload.to_string();
    let stdout = match tokio::task::spawn_blocking(move || {
        crate::blender::BlenderSocketClient::new(port).execute_auto(&code)
    })
    .await
    {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return fail(format!("Blender ha tornat error: {e}")),
        Err(e) => return fail(format!("execució a Blender interrompuda: {e}")),
    };
    // La IA ha animat una escena i n'ha desat una còpia? Render automàtic.
    if let Some(pos) = stdout.find(crate::blender::VIDEO_MARKER) {
        let path = stdout[pos + crate::blender::VIDEO_MARKER.len()..]
            .trim()
            .trim_matches('"')
            .to_string();
        return match render_video_and_copy(&path, ctx).await {
            Ok((relative, mp4)) => EngineOutcome {
                kind: "blender",
                ok: true,
                summary: format!(
                    "escena preparada i vídeo renderitzat: {relative} ({})",
                    clip(&stdout, 400)
                ),
                artifacts: vec![mp4],
            },
            Err(e) => EngineOutcome {
                kind: "blender",
                ok: false,
                summary: format!("escena preparada, però el render ha fallat: {e}"),
                artifacts: Vec::new(),
            },
        };
    }
    EngineOutcome {
        kind: "blender",
        ok: true,
        summary: format!("Python executat a Blender:\n{}", clip(&stdout, MAX_RESULT_TEXT)),
        artifacts: Vec::new(),
    }
}

/// Blender headless en segon pla: exporta FBX/OBJ/glTF, renderitza imatges,
/// genera escenes sense dependre de l'editor obert. Amb temps límit perquè
/// un render massa llarg no pengi el bucle (mateix esperit que el render
/// de vídeo).
async fn run_blender_headless(payload: &str) -> EngineOutcome {
    let fail = |msg: String| EngineOutcome {
        kind: "blender-headless",
        ok: false,
        summary: msg,
        artifacts: Vec::new(),
    };
    if crate::blender::detect_blender_binary().is_none() {
        return fail("Blender no està instal·lat".into());
    }
    let code = payload.to_string();
    match tokio::task::spawn_blocking(move || run_headless_timed(&code)).await {
        Ok(Ok(s)) => EngineOutcome {
            kind: "blender-headless",
            ok: true,
            summary: format!("Blender (segon pla) ha acabat:\n{}", clip(&s, MAX_RESULT_TEXT)),
            artifacts: Vec::new(),
        },
        Ok(Err(e)) => fail(format!("Blender (segon pla) ha fallat: {e}")),
        Err(e) => fail(format!("execució de Blender interrompuda: {e}")),
    }
}

/// Executa un script amb el binari de Blender i un temps límit (20 min).
fn run_headless_timed(code: &str) -> Result<String> {
    let binary = crate::blender::detect_blender_binary()
        .ok_or_else(|| anyhow!("Blender no instal·lat"))?;
    let tmp = std::env::temp_dir().join(format!("noorbit_engine_{}.py", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, code)?;
    let mut child = std::process::Command::new(&binary)
        .arg("-b")
        .arg("-P")
        .arg(&tmp)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("no s'ha pogut arrencar Blender: {e}"))?;
    let (mut so, mut se) = (child.stdout.take(), child.stderr.take());
    let (tx_out, rx_out) = std::sync::mpsc::channel();
    let (tx_err, rx_err) = std::sync::mpsc::channel();
    let to = std::thread::spawn(move || {
        use std::io::Read;
        let mut b = String::new();
        if let Some(s) = so.as_mut() {
            let _ = s.read_to_string(&mut b);
        }
        let _ = tx_out.send(b);
    });
    let te = std::thread::spawn(move || {
        use std::io::Read;
        let mut b = String::new();
        if let Some(s) = se.as_mut() {
            let _ = s.read_to_string(&mut b);
        }
        let _ = tx_err.send(b);
    });
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20 * 60);
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if start.elapsed() >= TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&tmp);
                    return Err(anyhow!("Blender ha superat els 20 minuts i s'ha aturat"));
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = std::fs::remove_file(&tmp);
                return Err(anyhow!("error esperant Blender: {e}"));
            }
        }
    };
    let _ = std::fs::remove_file(&tmp);
    let _ = to.join();
    let _ = te.join();
    let stdout = rx_out.recv().unwrap_or_default();
    let stderr = rx_err.recv().unwrap_or_default();
    if status.success() {
        Ok(stdout)
    } else {
        let tail: String = stderr.lines().rev().take(8).collect::<Vec<_>>().join(" | ");
        Err(anyhow!("Blender ha fallat: {tail}"))
    }
}

/// Renderitza el vídeo d'un `.blend` i el COPIA dins el projecte
/// (`EngineWork/Exports/`), perquè el repo el commiteji i Unreal el pugui
/// importar. Retorna la ruta RELATIVA al projecte i l'absoluta.
async fn run_blender_video(payload: &str, ctx: &EngineContext) -> EngineOutcome {
    match render_video_and_copy(payload, ctx).await {
        Ok((relative, mp4)) => EngineOutcome {
            kind: "blender-video",
            ok: true,
            summary: format!("vídeo renderitzat: {relative}"),
            artifacts: vec![mp4],
        },
        Err(e) => EngineOutcome {
            kind: "blender-video",
            ok: false,
            summary: format!("el render del vídeo ha fallat: {e}"),
            artifacts: Vec::new(),
        },
    }
}

async fn render_video_and_copy(blend: &str, ctx: &EngineContext) -> Result<(String, PathBuf)> {
    let blend = blend.trim().to_string();
    if blend.is_empty() {
        return Err(anyhow!("cal la ruta del fitxer .blend"));
    }
    let exports = ctx.root.join(ENGINE_WORK_DIR).join("Exports");
    std::fs::create_dir_all(&exports)?;
    let blend_abs = if Path::new(&blend).is_absolute() {
        PathBuf::from(&blend)
    } else {
        ctx.root.join(&blend)
    };
    if !blend_abs.exists() {
        return Err(anyhow!("no trobo el fitxer .blend: {}", blend_abs.display()));
    }
    let rendered = tokio::task::spawn_blocking(move || {
        crate::blender::render_blender_video(&blend_abs.to_string_lossy())
    })
    .await
    .map_err(|e| anyhow!("render interromput: {e}"))??;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dest = exports.join(format!("render-{ts}.mp4"));
    std::fs::copy(&rendered, &dest)?;
    let relative = dest
        .strip_prefix(&ctx.root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| dest.to_string_lossy().to_string());
    Ok((relative, dest))
}

// ─────────────────────────────────────────────────────────────────────────────
//  GODOT I BEVY: el motor DE VERITAT, en headless
// ─────────────────────────────────────────────────────────────────────────────

/// Sortida d'un procés executat amb temps límit.
struct CmdOutput {
    status: Option<std::process::ExitStatus>,
    stdout: String,
    stderr: String,
    /// El procés s'ha tallat perquè ha passat el temps límit.
    timed_out: bool,
}

/// Executa una ordre amb temps límit i en recull la sortida.
///
/// El temps límit és essencial: un joc (Godot o Bevy) NO s'acaba mai sol, i
/// la primera compilació de Bevy pot trigar mitja hora. En cap dels dos casos
/// el bucle de la Jam es pot quedar esperant indefinidament.
async fn run_capture(cmd: &mut Command, timeout: Duration) -> Result<CmdOutput> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| anyhow!("no s'ha pogut arrencar: {e}"))?;
    // Els lectors arrenquen ABANS d'esperar: si no, el procés s'omple el
    // tub i es queda penjat sense que en sapiguem res.
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let so = tokio::spawn(async move {
        let mut buf = String::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_string(&mut buf).await;
        }
        buf
    });
    let se = tokio::spawn(async move {
        let mut buf = String::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_string(&mut buf).await;
        }
        buf
    });
    let (status, timed_out) = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => (Some(status), false),
        Ok(Err(e)) => return Err(anyhow!("error esperant el procés: {e}")),
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            (None, true)
        }
    };
    let stdout = so.await.unwrap_or_default();
    let stderr = se.await.unwrap_or_default();
    Ok(CmdOutput { status, stdout, stderr, timed_out })
}

/// Separa l'ordre del seu argument: «run res://main.tscn» → («run», «res://…»).
fn split_command(payload: &str) -> (String, String) {
    let t = payload.trim();
    match t.split_once(char::is_whitespace) {
        Some((a, b)) => (a.trim().to_ascii_lowercase(), b.trim().to_string()),
        None => (t.to_ascii_lowercase(), String::new()),
    }
}

/// Últimes línies no buides de la sortida (el que interessa al model).
fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim_end())
        .filter(|l| !l.trim().is_empty())
        .collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// Escena principal d'un projecte de Godot (`run/main_scene`).
fn godot_main_scene(project_godot: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(project_godot) else {
        return String::new();
    };
    for line in text.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("run/main_scene") {
            let v = rest.trim_start().trim_start_matches('=').trim();
            return v.trim_matches('"').to_string();
        }
    }
    String::new()
}

/// Errors de GDScript que Godot ha escrit a la sortida.
fn godot_errors(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim_end())
        .filter(|l| {
            l.contains("SCRIPT ERROR")
                || l.contains("Parse Error")
                || l.contains("USER ERROR")
                || l.contains("ERROR:")
        })
        .take(30)
        .map(|l| l.to_string())
        .collect()
}

/// Directriu `@godot:` — el motor de Godot 4 en headless.
///
/// Ordres: `import` (importa els assets), `run [escena]` (executa l'escena i
/// torna els errors de GDScript), `script <fitxer.gd>` (executa un script) i
/// `version`.
async fn run_godot(payload: &str, ctx: &EngineContext) -> EngineOutcome {
    let fail = |msg: String| EngineOutcome {
        kind: "godot",
        ok: false,
        summary: msg,
        artifacts: Vec::new(),
    };
    let (action, arg) = split_command(payload);
    // Sense projecte, es CREA: l'usuari demana treballar en Godot i la IA no
    // es queda bloquejada. «version» és l'única ordre que no en necessita
    // (només pregunta la versió del binari).
    //
    // El projecte es crea ABANS de comprovar el binari (igual que fa «Obre al
    // motor»): així l'usuari sempre es queda amb el projecte a punt, encara que
    // el motor encara no estigui instal·lat.
    let project: Option<PathBuf> = match ctx.godot.as_ref() {
        Some(p) => Some(p.clone()),
        None if action == "version" => None,
        None => match crate::godot::setup::ensure_project(&ctx.root) {
            Ok(p) => Some(p.project_godot),
            Err(e) => return fail(format!("no s'ha pogut crear el projecte de Godot: {e}")),
        },
    };
    let Some(bin) = crate::godot::detect_binary() else {
        let extra = match project.as_deref() {
            Some(p) => format!(
                " El projecte de Godot és a «{}».",
                p.parent().unwrap_or(p).display()
            ),
            None => String::new(),
        };
        return fail(
            "Godot no està instal·lat (no s'ha trobat el binari «godot»). \
             Instal·la'l des de l'assistent d'instal·lació de NoOrbit."
                .to_string()
                + &extra,
        );
    };
    let dir = project
        .as_deref()
        .and_then(|p| p.parent())
        .unwrap_or(&ctx.root)
        .to_path_buf();
    let mut cmd = Command::new(&bin);
    cmd.current_dir(&dir).arg("--headless").arg("--path").arg(&dir);
    let timeout = match action.as_str() {
        "import" => {
            cmd.arg("--import");
            Duration::from_secs(300)
        }
        "run" => {
            let scene = if arg.is_empty() {
                project.as_deref().map(godot_main_scene).unwrap_or_default()
            } else {
                arg.clone()
            };
            if scene.is_empty() {
                return fail(
                    "no s'ha indicat cap escena i el project.godot no defineix run/main_scene"
                        .into(),
                );
            }
            cmd.arg(&scene);
            Duration::from_secs(180)
        }
        "script" => {
            if arg.is_empty() {
                return fail("cal la ruta del script (p. ex. «@godot: script res://test.gd»)".into());
            }
            cmd.arg("--script").arg(&arg);
            Duration::from_secs(180)
        }
        "version" => {
            cmd.arg("--version");
            Duration::from_secs(60)
        }
        "" => return fail(
            "ordre de Godot buida. Fes servir: import | run [escena] | script <fitxer.gd> | version"
                .into(),
        ),
        other => {
            return fail(format!(
                "ordre de Godot desconeguda: «{other}». Fes servir: import | run [escena] | script <fitxer.gd> | version"
            ))
        }
    };
    let out = match run_capture(&mut cmd, timeout).await {
        Ok(o) => o,
        Err(e) => return fail(format!("{e}")),
    };
    let text = format!("{}{}", out.stdout, out.stderr);
    let errors = godot_errors(&text);
    let mut summary = String::new();
    if out.timed_out {
        summary.push_str(&format!(
            "Godot s'ha executat durant {} s i s'ha tallat (un joc no s'acaba sol).",
            timeout.as_secs()
        ));
    }
    if errors.is_empty() {
        summary.push_str("Cap error de GDScript.");
        let tail = tail_lines(&text, 12);
        if !tail.is_empty() {
            summary.push_str("\nSortida:\n");
            summary.push_str(&tail);
        }
    } else {
        summary.push_str("Errors de Godot:\n");
        summary.push_str(&errors.join("\n"));
    }
    EngineOutcome { kind: "godot", ok: errors.is_empty(), summary, artifacts: Vec::new() }
}

/// Directriu `@bevy:` — Bevy (Rust) via `cargo`.
///
/// Ordres: `check` (compila sense generar l'executable: la ràpida per
/// iterar), `build`, `run`, `test` i `clippy`.
async fn run_bevy(payload: &str, ctx: &EngineContext) -> EngineOutcome {
    let fail = |msg: String| EngineOutcome {
        kind: "bevy",
        ok: false,
        summary: msg,
        artifacts: Vec::new(),
    };
    // Sense projecte, es CREA: l'usuari demana treballar en Bevy i la IA no
    // es queda bloquejada davant un error que no té solució automàtica.
    //
    // El projecte es crea ABANS de comprovar el «cargo» (igual que fa «Obre al
    // motor»): l'usuari es queda sempre amb el projecte a punt, encara que el
    // Rust encara no estigui instal·lat.
    let cargo_toml = match crate::bevy::ensure_project(&ctx.root) {
        Ok(p) => p,
        Err(e) => return fail(format!("no s'ha pogut crear el projecte Bevy: {e}")),
    };
    let dir = cargo_toml.parent().unwrap_or(&ctx.root).to_path_buf();
    let Some(cargo) = crate::bevy::detect_cargo() else {
        return fail(
            "Rust (cargo) no està instal·lat: cal per compilar Bevy. \
             Instal·la'l des de l'assistent d'instal·lació de NoOrbit (rustup.rs)."
                .to_string()
                + &format!(" El projecte Bevy ja és a «{}».", dir.display()),
        );
    };
    let (action, _arg) = split_command(payload);
    let action = if action.is_empty() { "check".to_string() } else { action };
    let timeout = match action.as_str() {
        "check" => Duration::from_secs(15 * 60),
        "build" => Duration::from_secs(25 * 60),
        "run" => Duration::from_secs(10 * 60),
        "test" => Duration::from_secs(15 * 60),
        "clippy" => Duration::from_secs(15 * 60),
        other => {
            return fail(format!(
                "ordre de Bevy desconeguda: «{other}». Fes servir: check | build | run | test | clippy"
            ))
        }
    };
    let mut cmd = Command::new(&cargo);
    cmd.current_dir(&dir);
    match action.as_str() {
        "check" => {
            cmd.arg("check").arg("--message-format").arg("short");
        }
        "build" => {
            cmd.arg("build");
        }
        "run" => {
            cmd.arg("run");
        }
        "test" => {
            cmd.arg("test");
        }
        "clippy" => {
            cmd.arg("clippy");
        }
        _ => {}
    }
    let out = match run_capture(&mut cmd, timeout).await {
        Ok(o) => o,
        Err(e) => return fail(format!("{e}")),
    };
    let code = out.status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
    let text = format!("{}{}", out.stdout, out.stderr);
    let mut summary = if out.timed_out {
        format!(
            "cargo {action} s'ha tallat després de {} s (temps límit).",
            timeout.as_secs()
        )
    } else {
        format!("cargo {action} ha acabat amb codi {code}.")
    };
    let tail = tail_lines(&text, 40);
    if !tail.is_empty() {
        summary.push('\n');
        summary.push_str(&tail);
    }
    EngineOutcome {
        kind: "bevy",
        ok: code == 0 && !out.timed_out,
        summary,
        artifacts: Vec::new(),
    }
}

/// Directriu `@import:` — porta un asset creat per Blender (o per altra
/// eina) dins el MOTOR del joc, sense intervenció de l'usuari.
///
/// La IA decideix quan fer-ho: sense aquesta directriu no es mou res. La ruta
/// pot ser relativa al projecte, dins de `EngineWork/Exports/` o només el nom
/// del fitxer; opcionalment s'hi pot posar el motor (`@import: godot
/// model.glb`). Si no s'indica cap motor, s'importa als detectats al projecte.
async fn run_import(payload: &str, ctx: &EngineContext) -> EngineOutcome {
    let fail = |msg: String| EngineOutcome {
        kind: "import",
        ok: false,
        summary: msg,
        artifacts: Vec::new(),
    };
    let (spec, target) = crate::engine_bridge::split_spec(payload);
    // La carpeta d'exportació sempre existeix: el model ha de poder-hi
    // exportar des de Blender sense que la ruta no existeixi.
    let _ = std::fs::create_dir_all(crate::engine_bridge::export_dir(&ctx.root));
    let src = match crate::engine_bridge::resolve_asset(&ctx.root, &spec) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    let targets = match target {
        Some(t) => vec![t],
        None => {
            let detected = crate::engine_bridge::detected_targets(ctx);
            if detected.is_empty() {
                return fail(
                    "no hi ha cap motor detectat al projecte on importar l'asset \
                     (cal un .uproject d'Unreal, un project.godot de Godot o un Cargo.toml \
                     amb bevy). Indica el motor: «@import: godot model.glb»."
                        .into(),
                );
            }
            detected
        }
    };
    let mut lines = Vec::new();
    let mut artifacts = Vec::new();
    let mut ok = true;
    for t in targets {
        let r = crate::engine_bridge::import_into(ctx, t, &src).await;
        lines.push(format!("- {}: {}", r.target.name(), r.summary));
        artifacts.extend(r.artifacts.iter().cloned());
        if !r.ok {
            ok = false;
        }
    }
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    EngineOutcome {
        kind: "import",
        ok,
        summary: format!("Importació de l'asset «{name}»:\n{}", lines.join("\n")),
        artifacts,
    }
}

/// Directriu `@video:` del muntatge. Tota la feina real és a
/// `crate::video::commands::run_directive`, que és EL MATEIX camí que usa el
/// panell «Vídeo»: si una acció funciona des del xat, funciona des del menú.
async fn run_video_directive(payload: &str, ctx: &EngineContext) -> EngineOutcome {
    let (ok, text) = crate::video::commands::run_directive(ctx.root.clone(), payload).await;
    EngineOutcome {
        kind: "video",
        ok,
        summary: clip(&text, MAX_RESULT_TEXT),
        artifacts: Vec::new(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  REGISTRE D'ARTEFACTES (visible al repo i per al verificador)
// ─────────────────────────────────────────────────────────────────────────────

/// Afegeix una entrada al registre `EngineWork/noorbit-engine.json` dins el
/// projecte. Retorna la ruta del fitxer (per incloure'l al Diff del bucle).
fn log_outcome(ctx: &EngineContext, outcome: &EngineOutcome) -> Result<PathBuf> {
    let dir = ctx.root.join(ENGINE_WORK_DIR);
    std::fs::create_dir_all(&dir)?;
    let path = ctx.root.join(ENGINE_LOG);
    let mut entries: Vec<Value> = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let artifacts: Vec<String> = outcome
        .artifacts
        .iter()
        .filter_map(|p| {
            p.strip_prefix(&ctx.root)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    entries.push(serde_json::json!({
        "ts": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "tool": outcome.kind,
        "ok": outcome.ok,
        "summary": outcome.summary,
        "artifacts": artifacts,
    }));
    // Només les últimes 200 entrades: el fitxer no ha de créixer sense límit.
    if entries.len() > 200 {
        entries.drain(0..entries.len() - 200);
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&entries)?)?;
    Ok(path)
}

// ─────────────────────────────────────────────────────────────────────────────
//  PROMPT: què li expliquem al model
// ─────────────────────────────────────────────────────────────────────────────

/// Secció del prompt que descriu les eines de motor disponibles. Buid si no
/// n'hi ha cap (llavors el model treballa només amb fitxers, com fins ara).
pub fn directives_help(ctx: &EngineContext) -> String {
    let mut out = String::new();
    if ctx.uproject.is_some() {
        out.push_str(
            "\n\nEINES D'UNREAL ENGINE (pots dirigir l'editor real):\n\
- «@unreal: {\"tool\":\"nom\",\"args\":{...}}» — crida una eina MCP de l'editor (crear actors, \
blueprints, importar assets…). Si no saps el nom exacte, primer pots demanar-ne el catàleg o provar; \
l'error et dirà les eines disponibles.\n\
- «@unreal-py: <codi python>» — executa Python DINS l'editor (import unreal; …). Via de reserva \
si el MCP no està actiu.\n\
Els assets creats per Blender s'importen a Unreal amb «@import:» (vegeu la secció d'importació).",
        );
    }
    if ctx.blender {
        let exports = crate::engine_bridge::ensure_exports_dir(&ctx.root);
        let exports = exports.display().to_string();
        out.push_str(&format!(
            "\n\nEINES DE BLENDER (pots crear i renderitzar en 3D):\n\
- «@blender: <codi bpy multi-línia>» — executa Python DINS Blender connectat (import bpy; …): \
crea models, materials, animacions. Si has animat una escena i n'has desat una còpia .blend, \
imprimeix «NOORBIT_VIDEO_OK:<ruta del .blend>» i NoOrbit renderitzarà el VÍDEO sol.\n\
- «@blender-headless: <codi bpy multi-línia>» — Blender en segon pla (sense editor obert): \
exporta a FBX/OBJ/glTF (bpy.ops.export_scene.fbx(filepath=…)) o renderitza imatges. Fes servir \
sempre rutes ABSOLUTES per als exports.\n\
- «@blender-video: <ruta del .blend>» — renderitza el vídeo d'una animació ja desada; el .mp4 \
queda copiat a «EngineWork/Exports/» del projecte.\n\
CARPETA D'EXPORTACIÓ: «{exports}» — exporta-hi els models que hagin d'arribar al motor del \
joc (bpy.ops.export_scene.gltf(filepath=\"{exports}/model.glb\")).",
        ));
    }
    if ctx.video {
        // La llista d'accions i el contracte de l'EDL es mantenen en UN sol
        // lloc (`video::director::directive_help`): el panell i el xat diuen
        // el mateix, i afegir una acció nova no pot deixar de publicitar-se.
        out.push_str(crate::video::director::directive_help());
        out.push_str(
            "Regla d'or del muntatge: primer `@video:scan` (què hi ha i com és), després\
             `@video:mount` amb els talls pensats i `@video:preview` per comprovar-ho; si el\
             que es demana és un VIDEO ACABAT amb criteri, `@video:director`. Els resultats\
             van a `VideoStudio/` del projecte (renders, `.kdenlive`, `.srt`, anàlisi).",
        );
    }
    if ctx.godot.is_some() {
        out.push_str(
            "\n\nEINES DE GODOT 4 (pots executar el motor de debò, en headless):\n\
- «@godot: import» — importa els assets del projecte i actualitza la carpeta .godot.\n\
- «@godot: run [escena]» — executa l'escena (per defecte run/main_scene del project.godot) \
i et torna els errors de GDScript. ÉS LA VIA per comprovar que el joc arrenca.\n\
- «@godot: script res://fitxer.gd» — executa un script GDScript en headless.\n\
- «@godot: version» — versió de Godot instal·lada.\n\
El codi del joc s'escriu igual amb «@file:» (project.godot, .tscn, .gd).",
        );
    }
    if ctx.bevy.is_some() {
        out.push_str(
            "\n\nEINES DE BEVY (Rust, via cargo):\n\
- «@bevy: check» — compila el codi (cargo check) sense generar l'executable: és la ràpida \
per iterar i la que has d'utilitzar després d'escriure codi.\n\
- «@bevy: build» — cargo build.\n\
- «@bevy: run» — executa el joc (s'obre la finestra; es talla sol després d'uns minuts).\n\
- «@bevy: test» — cargo test.\n\
AVÍS IMPORTANT: la PRIMERA compilació de Bevy pot trigar 10-30 minuts i molta memòria. \
Fes una sola compilació amb «check» i no tornis a compilar a cada tasca. El codi s'escriu \
amb «@file:» (Cargo.toml, src/main.rs…).",
        );
    }
    if ctx.uproject.is_some() || ctx.godot.is_some() || ctx.bevy.is_some() {
        let exports = crate::engine_bridge::ensure_exports_dir(&ctx.root);
        let exports = exports.display().to_string();
        out.push_str(&format!(
            "\n\nIMPORTACIÓ D'ASSETS AL MOTOR (allò que has creat amb Blender pot arribar al joc):\n\
- «@import: <ruta>» — importa un asset al motor detectat del projecte. La ruta pot ser \
«EngineWork/Exports/model.glb» o només «model.glb».\n\
- «@import: godot <ruta>» — força el destí: godot | unreal | bevy.\n\
FOLDRE D'EXPORTACIÓ: «{exports}» — és on has d'exportar els models des de Blender \
(bpy.ops.export_scene.gltf(filepath=\"{exports}/model.glb\")); després els'importes amb «@import:».\n\
FLUX RECOMANAT: primer «@blender-headless: …» per exportar el model i després «@import:» el \
fitxer. NO s'importa res automàticament: el moment d'importar el decideixes tu. A Godot l'asset \
arriba a «assets/» i, si és un .glb, es crea l'escena «scenes/» per instanciar-la; a Bevy arriba \
a «assets/» per carregar-lo amb asset_server.load(\"…\"); a Unreal s'importa via la Remote Control \
API (cal que l'editor del projecte sigui obert).",
        ));
    }
    out
}

/// Format dels resultats d'eines que es tornen al model a la següent ronda.
pub fn format_results(results: &[EngineOutcome]) -> String {
    if results.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nRESULTATS DE LES EINES DE MOTOR (executats DE VERITAT):\n");
    for r in results {
        out.push_str(&format!("- {}: {}\n", r.kind, r.summary));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parseja_una_directriu_d_una_linia() {
        let t = "Prova.\n@unreal: {\"tool\":\"spawn_actor\",\"args\":{\"x\":1}}\nFet.";
        let d = parse_directives(t);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].kind, DKind::Unreal);
        assert_eq!(d[0].payload, "{\"tool\":\"spawn_actor\",\"args\":{\"x\":1}}");
    }

    #[test]
    fn parseja_el_bloc_de_codi_fins_a_la_prosa() {
        let t = "@blender: import bpy\nbpy.ops.mesh.primitive_cube_add(size=2)\n\nAra faig una altra cosa.";
        let d = parse_directives(t);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].kind, DKind::Blender);
        assert!(d[0].payload.contains("primitive_cube_add"));
        assert!(!d[0].payload.contains("altra cosa"));
    }

    #[test]
    fn la_indentacio_continua_el_bloc() {
        let t = "@unreal-py: import unreal\nfor i in range(3):\n    print(i)\nRESUM: fet.";
        let d = parse_directives(t);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].kind, DKind::UnrealPy);
        assert!(d[0].payload.contains("    print(i)"));
        assert!(!d[0].payload.contains("RESUM"));
    }

    #[test]
    fn una_nova_directriu_tanca_l_anterior() {
        let t = "@blender: import bpy\n@unreal: {\"tool\":\"ping\"}\n";
        let d = parse_directives(t);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].kind, DKind::Blender);
        assert_eq!(d[1].kind, DKind::Unreal);
    }

    #[test]
    fn el_prefix_llarg_no_es_confon_amb_el_curt() {
        let t = "@blender-headless: import bpy\nbpy.ops.wm.save_as_mainfile()\n";
        let d = parse_directives(t);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].kind, DKind::BlenderHeadless);
    }

    #[test]
    fn l_json_malformat_no_s_empassa_la_prosa_seguent() {
        let t = "@unreal: {\"tool\":\"x\",\nJa he acabat.";
        let d = parse_directives(t);
        assert_eq!(d.len(), 1);
        // El payload queda com el JSON (el parseig estricte el rebutjarà a
        // l'execució amb un missatge clar per al model).
        assert!(d[0].payload.contains("\"tool\""));
        assert!(!d[0].payload.contains("acabat"));
    }

    #[test]
    fn detecta_l_uproject_a_l_arrel_i_a_un_nivell() {
        let dir = std::env::temp_dir().join(format!("noorbit-engtest-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(dir.join("Sub"));
        let up = dir.join("Sub").join("Joc.uproject");
        std::fs::write(&up, "{}").unwrap();
        let ctx = EngineContext::detect(&dir);
        assert_eq!(ctx.uproject.as_deref(), Some(up.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sense_uproject_el_context_queda_buit() {
        let dir = std::env::temp_dir().join(format!("noorbit-engtest2-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&dir);
        let ctx = EngineContext::detect(&dir);
        assert!(ctx.uproject.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn les_directrius_dures_no_entren_al_codi() {
        let t = "@blender: import bpy\n@file: script.py\nprint('hola')\n";
        let d = parse_directives(t);
        assert_eq!(d.len(), 1);
        assert!(!d[0].payload.contains("print('hola')"));
    }

    #[test]
    fn les_directrius_dins_d_un_bloc_fenced_no_s_executen() {
        // El model escriu un fitxer d'exemple que CONTÉ text de directriu:
        // no s'ha de disparar cap eina.
        let t = "Aquí tens el fitxer:\n@file: docs/exemple.md\n```\n@blender: import bpy\n```\nRESUM: fet.";
        let d = parse_directives(t);
        assert!(d.is_empty(), "dins d'un fenced no hi ha directrius: {d:?}");
    }

    #[test]
    fn el_codi_embolcallat_en_fenced_es_inclou_sense_marcadors() {
        let t = "@blender:\n```python\nimport bpy\nbpy.ops.mesh.primitive_cube_add()\n```\nAra continuo.";
        let d = parse_directives(t);
        assert_eq!(d.len(), 1);
        assert!(d[0].payload.contains("import bpy"));
        assert!(d[0].payload.contains("primitive_cube_add"));
        assert!(!d[0].payload.contains("```"));
        assert!(!d[0].payload.contains("continuo"));
    }

    #[test]
    fn la_directriu_import_es_una_de_una_linea() {
        let t = "@import: EngineWork/Exports/model.glb\n";
        let d = parse_directives(t);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].kind, DKind::Import);
        assert_eq!(d[0].payload, "EngineWork/Exports/model.glb");
    }

    #[test]
    fn la_directriu_import_acepta_el_motor_davant() {
        let t = "@import: godot model.glb\n";
        let d = parse_directives(t);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].kind, DKind::Import);
        let (spec, target) = crate::engine_bridge::split_spec(&d[0].payload);
        assert_eq!(spec, "model.glb");
        assert_eq!(target, Some(crate::engine_bridge::Target::Godot));
    }

    /// La directriu d'importació ha de poder anar al costat d'una de codi:
    /// si no, el codi de Blender s'emportaria la línia de l'import.
    #[test]
    fn la_directriu_import_tanca_un_bloc_de_codi() {
        let t = "@blender: import bpy\nbpy.ops.export_scene.gltf()\n@import: model.glb\n";
        let d = parse_directives(t);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].kind, DKind::Blender);
        assert_eq!(d[1].kind, DKind::Import);
    }

    #[tokio::test]
    async fn importar_sense_motor_dona_error_clar() {
        let dir = std::env::temp_dir().join(format!("noorbit-import-buit-{}", uuid::Uuid::new_v4()));
        let exports = dir.join("EngineWork").join("Exports");
        std::fs::create_dir_all(&exports).unwrap();
        std::fs::write(exports.join("model.glb"), b"glb").unwrap();
        let ctx = EngineContext::detect(&dir);
        let d = Directive { kind: DKind::Import, payload: "model.glb".into() };
        let out = execute(&ctx, &d).await;
        assert!(!out.ok);
        assert!(out.summary.contains("no hi ha cap motor"), "{}", out.summary);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// El flux complet de la IA: Blender exporta el model i tot seguit el
    /// motor el rep. Sense l'asset exportat, la importació ha de fallar amb
    /// un missatge que indiqui on buscar-lo.
    #[tokio::test]
    async fn importar_un_asset_inexistent_dona_error_clar() {
        let dir = std::env::temp_dir().join(format!("noorbit-import-inexistent-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let ctx = EngineContext::detect(&dir);
        let d = Directive { kind: DKind::Import, payload: "no-hi-es.glb".into() };
        let out = execute(&ctx, &d).await;
        assert!(!out.ok);
        assert!(out.summary.contains("no trobo"), "{}", out.summary);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
