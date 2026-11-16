//! Execució de scripts Python dins Unreal Editor.
//!
//! Com funciona REALMENT (i per què abans «no passava res»):
//!
//! La Remote Control API executa el script a la *game thread* i la resposta
//! HTTP només conté el que la funció retorni. `PythonScriptLibrary` té dues
//! variants segons la versió de l'editor:
//!
//! - UE 5.0-5.3: `ExecutePythonCommand` retorna `void` → la resposta és buida.
//! - UE 5.4+: `ExecutePythonCommand` retorna `bool` (ha funcionat o no).
//! - `ExecutePythonCommandEx` retorna `CommandResult` + `LogOutput`, però no
//!   existeix en totes les versions.
//!
//! Per tant, si només mirem la resposta HTTP mai veiem ni el `print()` del
//! script ni la seva excepció: `run_script` ho donava per bo (`Success`
//! absent = èxit) i la IA no rebia mai l'error per corregir-se. La solució
//! és un **wrapper** que executa el script de veritat i escriu la sortida i
//! el traceback a un fitxer temporal del SISTEMA LOCAL (NoOrbit i l'editor
//! van amb el mateix usuari, així que el camí absolut és reachable des dels
//! dos costats). El fitxer és la font de veritat; la resposta HTTP només
//! s'usa de reserva.

use super::client::UnrealClient;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Marques del protocol de captura (ASCII pur: surten tal qual al fitxer).
const TOKEN_MARK: &str = "NOORBIT-TOKEN:";
const ERROR_MARK: &str = "NOORBIT-ERROR:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonResult {
    pub success: bool,
    pub output: Vec<String>,
    pub errors: Vec<String>,
}

/// Carpeta de treball: `<temp>/noorbit/ue`. NoOrbit i l'editor comparteixen
/// el mateix sistema de fitxers (la integració és sempre localhost).
fn workdir() -> PathBuf {
    std::env::temp_dir().join("noorbit").join("ue")
}

/// Identificador únic d'una execució: evita barrejar sortides de dues crides
/// concurrents (agent + panell d'Unreal poden executar alhora).
fn unique_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Converteix un text en un literal Python segur per a qualsevol ruta
/// (barres invertides de Windows, accents, espais): bytes codificats en hex.
fn py_hex(s: &str) -> String {
    s.as_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

/// Script Python que executa `src` i escriu la seva sortida a `out`.
///
/// El contingut de l'script NO s'envia dins la cadena (els scripts de la IA
/// fan centenars de línies i els caràcters especials hi trenquen el JSON):
/// primer es desa a disc i aqui el wrapper el llegeix i l'executa.
pub fn wrap_script(src: &Path, out: &Path, id: &str) -> String {
    format!(
        r##"# NoOrbit — wrapper d'execució (el genera NoOrbit, no cal editar-lo)
import sys
import traceback

NOORBIT_ID = bytes.fromhex("{id_hex}").decode("utf-8")
NOORBIT_SRC = bytes.fromhex("{src_hex}").decode("utf-8")
NOORBIT_OUT = bytes.fromhex("{out_hex}").decode("utf-8")
NOORBIT_HEAD = "{token}"
NOORBIT_ERROR = "{error}"

_NOORBIT_FILE = [None]


def _noorbit_emit(text):
    """Afegeix text al fitxer de sortida (el reobre si ja era tancat)."""
    handle = _NOORBIT_FILE[0]
    if handle is None:
        try:
            handle = open(NOORBIT_OUT, "a", encoding="utf-8")
            _NOORBIT_FILE[0] = handle
        except Exception:
            return
    try:
        handle.write(text)
        handle.flush()
    except Exception:
        pass


def _noorbit_close():
    handle = _NOORBIT_FILE[0]
    _NOORBIT_FILE[0] = None
    if handle is not None:
        try:
            handle.close()
        except Exception:
            pass


class _NoOrbitStream(object):
    """Duplica la sortida: al fitxer de NoOrbit i al log de l'editor."""

    def __init__(self, inner):
        self._inner = inner

    def write(self, text):
        _noorbit_emit(text)
        try:
            self._inner.write(text)
        except Exception:
            pass
        return len(text)

    def flush(self):
        try:
            self._inner.flush()
        except Exception:
            pass

    def isatty(self):
        return False


def _noorbit_run():
    with open(NOORBIT_SRC, "r", encoding="utf-8") as script:
        source = script.read()
    _NOORBIT_FILE[0] = open(NOORBIT_OUT, "w", encoding="utf-8")
    _noorbit_emit(NOORBIT_HEAD + NOORBIT_ID + "\n")
    _noorbit_close()
    old_out, old_err = sys.stdout, sys.stderr
    sys.stdout = _NoOrbitStream(old_out)
    sys.stderr = _NoOrbitStream(old_err)
    try:
        scope = {{"__name__": "__main__", "__file__": NOORBIT_SRC}}
        exec(compile(source, NOORBIT_SRC, "exec"), scope)
    finally:
        sys.stdout, sys.stderr = old_out, old_err


try:
    _noorbit_run()
except BaseException:
    # BaseException: també SystemExit i companyia, que si no deixarien el
    # fitxer sense cap senyal i NoOrbit ho donaria per bo.
    _noorbit_emit(NOORBIT_ERROR + "\n" + traceback.format_exc() + "\n")
finally:
    _noorbit_close()
"##,
        id_hex = py_hex(id),
        src_hex = py_hex(&src.to_string_lossy()),
        out_hex = py_hex(&out.to_string_lossy()),
        token = TOKEN_MARK,
        error = ERROR_MARK,
    )
}

/// Llegeix la sortida capturada, amb una petita espera: la crida Remote
/// Control ja és síncrona (l'script acaba abans de respondre), però si la
/// connexió s'ha tallat l'editor pot continuar treballant uns segons més.
async fn read_capture(path: &Path, budget: Duration) -> Option<String> {
    let step = Duration::from_millis(120);
    let mut waited = Duration::ZERO;
    while waited < budget {
        if let Ok(text) = tokio::fs::read_to_string(path).await {
            if !text.is_empty() {
                return Some(text);
            }
        }
        tokio::time::sleep(step).await;
        waited += step;
    }
    None
}

/// Cerca un camp d'un JSON sense importar la caixa (l'editor uns cops seri
/// els noms reflectits del C++ —`LogOutput`— i altres en camelCase).
fn field<'a>(value: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    let obj = value.as_object()?;
    obj.get(name).or_else(|| {
        obj.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    })
}

/// Extreu les entrades de log (`LogOutput`, amb o sense `Entries`) com a
/// parelles (tipus, text).
fn log_entries(value: &serde_json::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(log) = field(value, "LogOutput") else {
        return out;
    };
    let arr = match log {
        serde_json::Value::Array(a) => Some(a.clone()),
        other => field(other, "Entries").and_then(|v| v.as_array()).cloned(),
    };
    for e in arr.unwrap_or_default() {
        let type_ = field(&e, "Type")
            .and_then(|v| v.as_str())
            .unwrap_or("Display")
            .to_string();
        let msg = field(&e, "Output")
            .or_else(|| field(&e, "Message"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if !msg.is_empty() {
            out.push((type_, msg));
        }
    }
    out
}

/// Intèrpret del fitxer de captura: és la font de veritat de l'execució.
pub fn parse_capture(raw: &str, id: &str) -> PythonResult {
    let (head, trace) = match raw.find(ERROR_MARK) {
        Some(i) => (&raw[..i], Some(raw[i + ERROR_MARK.len()..].trim().to_string())),
        None => (raw, None),
    };
    let output: Vec<String> = head
        .lines()
        .map(|l| l.trim_end().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with(TOKEN_MARK))
        .collect();

    if let Some(trace) = trace {
        return PythonResult {
            success: false,
            output,
            errors: vec![trace],
        };
    }

    // Sense traceback: només és èxit si el fitxer porta el segell d'AQUESTA
    // execució (si no, podria ser la sortida d'una crida anterior).
    if raw.contains(&format!("{}{}", TOKEN_MARK, id)) {
        PythonResult {
            success: true,
            output,
            errors: Vec::new(),
        }
    } else {
        PythonResult {
            success: false,
            output,
            errors: vec![
                "La sortida capturada no correspon a aquesta execució (fitxer barrejat o incomplet)."
                    .to_string(),
            ],
        }
    }
}

/// Reserva: quan no hi ha fitxer de captura (editor antic o wrapper bloquejat)
/// aprofitem el que la resposta HTTP sí que porta.
fn from_response(value: &serde_json::Value) -> PythonResult {
    let mut res = PythonResult {
        success: true,
        output: Vec::new(),
        errors: Vec::new(),
    };
    let root = field(value, "ReturnValue").unwrap_or(value);

    if let Some(ok) = root.as_bool() {
        res.success = ok;
    } else if let Some(ok) = field(root, "Success").and_then(|v| v.as_bool()) {
        res.success = ok;
    }

    for (type_, msg) in log_entries(value).into_iter().chain(log_entries(root)) {
        if type_.eq_ignore_ascii_case("Error") {
            res.errors.push(msg);
        } else {
            res.output.push(msg);
        }
    }
    if let Some(cmd_result) = field(root, "CommandResult").and_then(|v| v.as_str()) {
        if !cmd_result.trim().is_empty() {
            res.output.push(cmd_result.trim().to_string());
        }
    }
    if !res.success && res.errors.is_empty() {
        res.errors
            .push("L'editor ha executat l'script amb errors (vegeu el Log de Unreal).".into());
    }
    res
}

/// Executa un script Python dins Unreal Editor i retorna la sortida REAL.
///
/// Si l'editor no escriu cap fitxer de captura, l'script NO s'ha executat:
/// ho diem clar (abans això passava per un èxit buit i la IA insistia).
pub async fn run_script(client: &UnrealClient, code: &str) -> Result<PythonResult> {
    let dir = workdir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Err(anyhow!("No puc crear la carpeta temporal per a Unreal: {}", e));
    }
    cleanup_stale(&dir);

    let id = unique_id();
    let src = dir.join(format!("script-{}.py", id));
    let out = dir.join(format!("out-{}.txt", id));
    let _ = std::fs::remove_file(&out);

    if let Err(e) = std::fs::write(&src, code) {
        return Err(anyhow!("No puc escriure l'script per a Unreal: {}", e));
    }

    let call = client.execute_python(&wrap_script(&src, &out, &id)).await;

    // Amb la resposta HTTP resolta, l'script ja ha acabat; sense resposta pot
    // seguir corrent a l'editor (tall de connexió o script llarg).
    let budget = if call.is_ok() {
        Duration::from_secs(3)
    } else {
        Duration::from_secs(20)
    };
    let raw = read_capture(&out, budget).await;
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&out);

    if let Some(text) = raw {
        return Ok(parse_capture(&text, &id));
    }

    match call {
        Ok(value) => {
            let res = from_response(&value);
            if res.output.is_empty() && res.errors.is_empty() && res.success {
                Err(anyhow!(
                    "Unreal ha rebut l'script però no n'ha executat res: no hi ha sortida \
                     ni errors.\n\
                     Causes habituals: el plugin «Python Editor Script Plugin» no està \
                     activat al projecte, la Remote Control API no té permisos per executar \
                     Python, o l'editor està bloquejat (finestra modal, compilar, cook).\n\
                     Carpeta de captura: {}",
                    dir.display()
                ))
            } else {
                Ok(res)
            }
        }
        Err(e) => Err(e),
    }
}

/// Esborra residus d'execucions anteriors (scripts i sortides amb més d'un
/// dia): sense això la carpeta temporal creix per sempre.
fn cleanup_stale(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let modified = meta.modified().unwrap_or(now);
        let old = match now.duration_since(modified) {
            Ok(d) => d.as_secs() > 86_400,
            Err(_) => false,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if old && (name.starts_with("script-") || name.starts_with("out-")) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Sonda d'informació de l'editor: versió de motor i nom del projecte.
///
/// Cal perquè `GET /remote/info` NOMÉS retorna la llista de rutes HTTP — no
/// diu res de la versió ni del projecte, que abans es quedava en `null` a la
/// UI (i el panell no mostrava mai quina Unreal estàs tocant).
pub const PROBE_INFO: &str = r#"import os
import unreal


def _engine_version():
    try:
        return str(unreal.SystemLibrary.get_engine_version())
    except Exception:
        return ""


def _project_name():
    ctx = getattr(unreal, "ProjectContext", None)
    if ctx is not None and hasattr(ctx, "get_project_name"):
        try:
            return str(ctx.get_project_name())
        except Exception:
            pass
    try:
        content = unreal.SystemLibrary.get_project_content_directory()
        return os.path.basename(os.path.dirname(os.path.normpath(content)))
    except Exception:
        return ""


print("NOORBIT-INFO|" + _engine_version() + "|" + _project_name())
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorInfo {
    pub engine_version: Option<String>,
    pub project_name: Option<String>,
}

/// Interpreta la línia `NOORBIT-INFO|<versió>|<projecte>` de la sonda.
pub fn parse_probe(lines: &[String]) -> EditorInfo {
    let mut info = EditorInfo {
        engine_version: None,
        project_name: None,
    };
    for line in lines {
        if let Some(rest) = line.trim().strip_prefix("NOORBIT-INFO|") {
            let mut parts = rest.split('|');
            if let Some(v) = parts.next() {
                if !v.trim().is_empty() {
                    info.engine_version = Some(v.trim().to_string());
                }
            }
            if let Some(p) = parts.next() {
                if !p.trim().is_empty() {
                    info.project_name = Some(p.trim().to_string());
                }
            }
        }
    }
    info
}

/// Demana a l'editor la seva versió i projecte (millor esforç: si falla,
/// retorna camp buit i la UI segueix funcionant).
pub async fn probe_info(client: &UnrealClient) -> EditorInfo {
    match run_script(client, PROBE_INFO).await {
        Ok(res) => parse_probe(&res.output),
        Err(_) => EditorInfo {
            engine_version: None,
            project_name: None,
        },
    }
}

/// Script: crea un Blueprint buit.
///
/// Fa servir `EditorAssetSubsystem` (API actual): `EditorAssetLibrary` i
/// `EditorLevelLibrary` estan deprecades des de UE 5.0 i els avisos de
/// deprecació omplen el log sense fer res d'útil.
pub fn script_create_blueprint(name: &str, parent_class: &str) -> String {
    format!(
        r#"import unreal

parent = unreal.load_class(None, "/Script/Engine.{parent}")
if parent is None:
    raise RuntimeError("No s'ha trobat la classe /Script/Engine.{parent}")

factory = unreal.BlueprintFactory()
factory.set_editor_property("ParentClass", parent)

asset_tools = unreal.AssetToolsHelpers.get_asset_tools()
bp = asset_tools.create_asset("{name}", "/Game/Blueprints", None, factory)
if bp is None:
    raise RuntimeError("L'editor no ha pogut crear el Blueprint «{name}»")

saved = unreal.get_editor_subsystem(unreal.EditorAssetSubsystem).save_loaded_asset(bp)
print("Blueprint creat: " + bp.get_path_name() + (" (desat)" if saved else " (sense desar)"))
"#,
        name = name,
        parent = parent_class
    )
}

/// Script: fa spawn d'un actor al nivell actual via `EditorActorSubsystem`.
pub fn script_spawn_actor(class_path: &str, location: [f64; 3]) -> String {
    format!(
        r#"import unreal

actor_class = unreal.load_class(None, "{class_path}")
if actor_class is None:
    raise RuntimeError("No s'ha trobat la classe {class_path}")

actors = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)
actor = actors.spawn_actor_from_class(actor_class, unreal.Vector({x}, {y}, {z}))
if actor is None:
    raise RuntimeError("L'editor no ha pogut crear l'actor {class_path}")

print("Actor creat: " + actor.get_name() + " a ({x}, {y}, {z})")
"#,
        class_path = class_path,
        x = location[0],
        y = location[1],
        z = location[2]
    )
}

/// Script: importa un asset (FBX, OBJ, GLB…) dins el projecte.
///
/// `import_assets_automated` retorna els assets que REALMENT s'han importat:
/// així sabem si l'importació ha fallat (abans sempre deia «correctament»,
/// encara que Unreal no hagués creat res).
pub fn script_import_asset(file_path: &str, dest_path: &str) -> String {
    format!(
        r#"import os
import unreal

src = os.path.abspath(r"{file}")
if not os.path.isfile(src):
    raise RuntimeError("El fitxer no existeix: " + src)

tools = unreal.AssetToolsHelpers.get_asset_tools()
assets = unreal.get_editor_subsystem(unreal.EditorAssetSubsystem)
imported = []
try:
    data = unreal.AutomatedAssetImportData()
    data.set_editor_property("filenames", [src])
    data.set_editor_property("destination_path", "{dest}")
    data.set_editor_property("replace_existing", True)
    data.set_editor_property("group_with_root", False)
    imported = list(tools.import_assets_automated(data) or [])
except Exception:
    # Editors sense AutomatedAssetImportData: AssetImportTask, i com que no
    # retorna res, comparem el contingut de la carpeta abans i després.
    try:
        before = set(assets.list_assets("{dest}", True, True))
    except Exception:
        before = set()
    task = unreal.AssetImportTask()
    task.set_editor_property("filename", src)
    task.set_editor_property("destination_path", "{dest}")
    task.set_editor_property("automated", True)
    task.set_editor_property("replace_existing", True)
    task.set_editor_property("save", True)
    tools.import_asset_tasks([task])
    fresh = sorted(set(assets.list_assets("{dest}", True, True)) - before)
    imported = [unreal.load_asset(p) for p in fresh]

names = [a.get_path_name() for a in imported if a is not None]
if not names:
    msg = "Unreal no ha importat res des de " + src
    msg += " (format no suportat o fitxer invalid: prova un FBX o GLB exportat de Blender)"
    raise RuntimeError(msg)

for asset in imported:
    try:
        assets.save_loaded_asset(asset)
    except Exception:
        pass
print("Importat a Unreal [" + str(len(names)) + "]: " + ", ".join(names))
"#,
        file = file_path,
        dest = dest_path
    )
}

/// Script: construeix la il·luminació del nivell.
///
/// `BuildLighting <qualitat>` NO és una comanda de consola de UE5: abans
/// l'editor simplement no feia res i NoOrbit ho deia fet. Ara es crida
/// `LevelEditorSubsystem.build_light_maps`, que respon amb l'èxit real, i
/// amb Lumen ho diu clar (il·luminació dinàmica: no cal construir mapes).
pub fn script_build_lighting(quality: &str) -> String {
    format!(
        r#"import unreal

ENUM = {{
    "Preview": "QUALITY_PREVIEW",
    "Medium": "QUALITY_MEDIUM",
    "High": "QUALITY_HIGH",
    "Production": "QUALITY_PRODUCTION",
}}

name = ENUM.get("{quality}", "QUALITY_PRODUCTION")
quality_value = getattr(unreal.LightingBuildQuality, name, None)
if quality_value is None:
    raise RuntimeError("Aquesta versió de Unreal no coneix la qualitat " + name)

sub = unreal.get_editor_subsystem(unreal.LevelEditorSubsystem)
ok = sub.build_light_maps(quality_value, False)
if not ok:
    raise RuntimeError(
        "Unreal ha refusat construir els lightmaps. Si el projecte fa servir Lumen "
        "(il·luminació dinàmica), no cal construir-los: activa'l a "
        "Window > Platform > Lighting."
    )
print("Il·luminació construïda amb qualitat " + name)
"#,
        quality = quality
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_embuteix_rutes_segures() {
        // Rutes amb espais, accents i barres invertides (Windows) no poden
        // trencar el literal Python.
        let w = wrap_script(
            Path::new(r"C:\Users\Jordi\Temp\no orbit\script-ñ.py"),
            Path::new("/tmp/no orbit/out-ñ.txt"),
            "abc123",
        );
        assert!(w.contains("NOORBIT_ID = bytes.fromhex("));
        assert!(w.contains("exec(compile(source, NOORBIT_SRC, \"exec\"), scope)"));
        assert!(!w.contains("script-ñ.py\""), "la ruta no ha d'anar en clar");
    }

    #[test]
    fn captura_dona_per_bo_nomes_amb_el_segell_d_aquesta_execucio() {
        let ok = parse_capture("NOORBIT-TOKEN:xyz\nActors: 3\n", "xyz");
        assert!(ok.success);
        assert_eq!(ok.output, vec!["Actors: 3".to_string()]);
        assert!(ok.errors.is_empty());

        // Un fitxer d'una altra execució (o tallat) mai és èxit.
        let alien = parse_capture("NOORBIT-TOKEN:altre\nfeta\n", "xyz");
        assert!(!alien.success);

        let buit = parse_capture("", "xyz");
        assert!(!buit.success);
    }

    #[test]
    fn traceback_es_converteix_en_error_visible() {
        let raw = "NOORBIT-TOKEN:xyz\nabans\nNOORBIT-ERROR:\nTraceback (most recent call last):\n  File \"x\", line 1\nZeroDivisionError: division by zero\n";
        let res = parse_capture(raw, "xyz");
        assert!(!res.success);
        assert_eq!(res.output, vec!["abans".to_string()]);
        assert!(res.errors[0].contains("ZeroDivisionError"));
    }

    #[test]
    fn resposta_antic_format_structura_log() {
        // UE 5.2: ExecutePythonCommandEx → ReturnValue { Success, LogOutput.Entries }
        let v = serde_json::json!({
            "ReturnValue": {
                "Success": false,
                "LogOutput": {
                    "Entries": [
                        {"Type": "Display", "Output": "hola"},
                        {"Type": "Error", "Output": "NameError: nom"}
                    ]
                }
            }
        });
        let res = from_response(&v);
        assert!(!res.success);
        assert_eq!(res.output, vec!["hola".to_string()]);
        assert_eq!(res.errors, vec!["NameError: nom".to_string()]);
    }

    #[test]
    fn resposta_booleana_de_ue_nou() {
        // UE 5.6: ExecutePythonCommand → ReturnValue = bool
        let v = serde_json::json!({ "ReturnValue": false });
        let res = from_response(&v);
        assert!(!res.success);
        assert!(!res.errors.is_empty());

        let v = serde_json::json!({ "ReturnValue": true });
        assert!(from_response(&v).success);
    }

    #[test]
    fn sonda_extreu_versio_i_projecte() {
        let lines = vec![
            "alguna cosa".to_string(),
            "NOORBIT-INFO|5.6.1-40000000|MiJoc".to_string(),
        ];
        let info = parse_probe(&lines);
        assert_eq!(info.engine_version.as_deref(), Some("5.6.1-40000000"));
        assert_eq!(info.project_name.as_deref(), Some("MiJoc"));
    }

    #[test]
    fn scripts_no_usen_apis_deprecades() {
        // EditorLevelLibrary / EditorAssetLibrary deprecades: llençaven avís
        // i deixaven la UI sense informació.
        for script in [
            script_create_blueprint("BP_X", "Actor"),
            script_spawn_actor("/Script/Engine.StaticMeshActor", [0., 0., 0.]),
            script_import_asset("/tmp/model.fbx", "/Game/Meshes"),
            script_build_lighting("High"),
        ] {
            assert!(script.contains("import unreal"), "{script}");
            assert!(!script.contains("EditorLevelLibrary"), "{script}");
            assert!(!script.contains("EditorAssetLibrary"), "{script}");
        }
        assert!(script_build_lighting("Production").contains("build_light_maps"));
    }

    /// Els scripts que generatem han de ser Python VÀLID: si el wrapper o una
    /// plantilla tenen un error de sintaxi, l'editor ho responia amb un missatge
    /// opac i la IA «corregia» alguna cosa que no existia.
    /// Es comprova amb el `python3` del sistema; si no n'hi ha (alguns
    /// servidors de Windows), el test no pot opinar i se'n va.
    #[test]
    fn scripts_generats_són_python_vàlid() {
        let candidates = vec![
            wrap_script(Path::new("/tmp/a.py"), Path::new("/tmp/a.out"), "id-demo"),
            script_create_blueprint("BP_X", "Actor"),
            script_spawn_actor("/Script/Engine.StaticMeshActor", [1., 2., 3.]),
            script_import_asset("/tmp/model.fbx", "/Game/Meshes"),
            script_build_lighting("High"),
            PROBE_INFO.to_string(),
        ];
        let dir = std::env::temp_dir();
        for (i, code) in candidates.iter().enumerate() {
            let path = dir.join(format!("noorbit-ue-sintaxi-{i}.py"));
            if std::fs::write(&path, code).is_err() {
                continue;
            }
            let out = std::process::Command::new("python3")
                .arg("-c")
                .arg("import sys; compile(open(sys.argv[1], encoding='utf-8').read(), 'noorbit', 'exec')")
                .arg(&path)
                .output();
            let _ = std::fs::remove_file(&path);
            let Ok(o) = out else { return };
            assert!(
                o.status.success(),
                "sintaxi Python trencada:\n{}\n---\n{}",
                code,
                String::from_utf8_lossy(&o.stderr)
            );
        }
    }
}
