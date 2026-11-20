//! COMANDES Tauri del muntatge de vídeo.
//!
//! Són la frontissa entre el panell «Vídeo» i el motor (`director`, `render`,
//! `mlt`). Tot el que es pot fer des del menú també es pot fer des del xat amb
//! `@video:` (`crate::engine_tools`), i viceversa: els dos camins acaben aquí.
//!
//! Dubte de disseny: el treball BREU (escaneig, export, verificació) es fa
//! dins `spawn_blocking` perquè cada `ffprobe`/`ffmpeg` pot tardar segons o
//! minuts i la UI no s'ha de congelar mai. El que és LLARG de veritat (el
//! director i el render final) a més té un handle amb estat i cancel·lació, per
//! poder tancar el panell i que la feina continuï en segon pla.

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::video::{detect, director, edl::{self, Edl}, ff, mlt, render};
use crate::AppState;

/// Temporitzador del render «a mà» (el botó «Renderitza» del panell).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderState {
    pub running: bool,
    /// `preparant` · `sonoritat` · `codificant` · `verificant` · `exportant`.
    pub stage: String,
    pub progress: f64,
    pub detail: String,
    pub preview: bool,
    pub out_path: Option<String>,
    /// Ordre `ffmpeg` real (perquè l'usuari vegui què s'ha executat).
    pub command_line: Option<String>,
    pub started_at: i64,
    pub elapsed_ms: u64,
    pub error: Option<String>,
    pub report: Option<serde_json::Value>,
}

/// Handle global del render a mà: un sol render alhora (com la Jam).
#[derive(Default)]
pub struct RenderHandle {
    pub state: std::sync::Mutex<RenderState>,
    /// El bucle de `render::execute` la mira per matar `ffmpeg` de seguida.
    pub stop: AtomicBool,
}

impl RenderHandle {
    pub fn snapshot(&self) -> RenderState {
        self.state.lock().unwrap().clone()
    }

    fn set(&self, f: impl FnOnce(&mut RenderState)) {
        if let Ok(mut s) = self.state.lock() {
            f(&mut s);
        }
    }
}

fn emit_render(app: &AppHandle, h: &RenderHandle) {
    let _ = app.emit("video://render", h.snapshot());
}

/// Un origen ja mesurat, amb la seva graella visual per al panell.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    #[serde(flatten)]
    pub source: ff::Source,
    /// PNG amb 6 fotogrames repartits (el que la IA veu per decidir els talls).
    pub sheet: Option<String>,
    pub luma: f64,
    pub saturation: f64,
    pub motion: f64,
    /// Fracció de mostres gairebé negres (plans morts).
    pub black: f64,
    pub scene_changes: usize,
    pub silence_blocks: usize,
    pub total_silence_s: f64,
}

/// Resum de l'escaneig (el que ve la UI a «Inventari»).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub tools_ready: bool,
    pub bundle: Option<String>,
    pub sources: Vec<SourceInfo>,
    /// Orígens que l'usuari ha anat afegint a mà.
    pub saved: Vec<String>,
    pub edl: Option<Edl>,
    pub last_render: Option<String>,
    pub work_dir: String,
}

/// Arrel del projecte, tolerante amb cadenes brutes.
fn root_of(project_path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(project_path.trim());
    if p.is_dir() {
        Ok(p)
    } else {
        Err(format!("la carpeta del projecte no existeix: {}", p.display()))
    }
}

/// Camí de l'últim render trobat (per tornar-hi des del panell).
fn last_render_of(work: &Path) -> Option<String> {
    let rd = work.join("renders");
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    let Ok(rd) = std::fs::read_dir(&rd) else {
        return None;
    };
    for e in rd.flatten() {
        let p = e.path();
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if !["mp4", "mov", "webm"].contains(&ext.as_str()) {
            continue;
        }
        let ok = e.metadata().map(|m| m.len() > 4096).unwrap_or(false);
        if !ok {
            continue;
        }
        let t = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().map(|(bt, _)| t > *bt).unwrap_or(true) {
            best = Some((t, p));
        }
    }
    best.map(|(_, p)| p.to_string_lossy().to_string())
}

/// Mesura un origen per a la llista d'inventari (estadística + graella).
fn describe_source(path: &str, sheet_dir: Option<&Path>, index: usize) -> SourceInfo {
    let src = ff::probe(path).unwrap_or_else(|_| ff::Source {
        path: path.to_string(),
        name: Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string()),
        readable: false,
        ..Default::default()
    });
    let st = ff::stats(path, 6).unwrap_or_default();
    let sc = ff::scenes(path, 0.35, 24).unwrap_or_default();
    let sil = ff::silences(path, -45.0, 0.45).unwrap_or_default();
    let mean = |v: Vec<f64>| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
    let sheet = sheet_dir.map(|d| {
        let png = d.join(format!("origen-{:02}.png", index + 1));
        match ff::contact_sheet(path, &png, 3, 2, 320) {
            Ok(p) => Some(p.to_string_lossy().to_string()),
            Err(_) => None,
        }
    });
    SourceInfo {
        source: src,
        sheet: sheet.flatten(),
        luma: mean(st.iter().map(|f| f.luma).collect()),
        saturation: mean(st.iter().map(|f| f.saturation).collect()),
        motion: mean(st.iter().skip(1).map(|f| f.motion).collect()),
        black: mean(st.iter().map(|f| f.black_ratio).collect()),
        scene_changes: sc.len(),
        silence_blocks: sil.len(),
        total_silence_s: sil.iter().map(|s| s.dur).sum(),
    }
}

// ─── EINES DEL SISTEMA ───────────────────────────────────────────────────────

/// Quines eines de muntatge hi ha (ffmpeg/ffprobe/melt/kdenlive).
#[tauri::command]
pub async fn video_tools() -> detect::VideoTools {
    tokio::task::spawn_blocking(detect::detect)
        .await
        .unwrap_or_default()
}

/// Inventari complet: orígens, mesures i graella de cadascun.
#[tauri::command]
pub async fn video_scan(project_path: String, sheets: bool) -> Result<ScanReport, String> {
    let root = root_of(&project_path)?;
    let work = director::workdir(&root);
    let sheet_dir = work.join("analysis").join("inventari");
    if sheets {
        let _ = std::fs::create_dir_all(&sheet_dir);
    }
    let extra = director::saved_sources(&root);
    let found = director::sources_from_dir(&root, &extra);
    let infos = tokio::task::spawn_blocking(move || {
        let sd = if sheets { Some(sheet_dir.as_path()) } else { None };
        found
            .iter()
            .enumerate()
            .map(|(i, p)| describe_source(p, sd, i))
            .collect::<Vec<SourceInfo>>()
    })
    .await
    .map_err(|e| format!("l'escaneig s'ha tallat: {e}"))?;
    let tools = detect::detect();
    Ok(ScanReport {
        tools_ready: tools.ready,
        bundle: tools.bundle.clone(),
        sources: infos,
        saved: extra,
        edl: director::load_edl(&root),
        last_render: last_render_of(&work),
        work_dir: work.to_string_lossy().to_string(),
    })
}

// ─── EDL (el muntatge desat) ─────────────────────────────────────────────────

/// Llegeix l'EDL desat del projecte.
#[tauri::command]
pub fn video_edl_get(project_path: String) -> Option<Edl> {
    let root = PathBuf::from(project_path.trim());
    director::load_edl(&root)
}

/// Desa un EDL (sanejat). Torna els avisos del sanejament.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizeReport {
    pub dropped: u32,
    pub notes: Vec<String>,
    pub edl: Edl,
    pub duration_s: f64,
    pub path: String,
}

fn store_edl(root: &Path, mut e: Edl) -> Result<SanitizeReport, String> {
    e.name = if e.name.trim().is_empty() {
        "Muntatge".into()
    } else {
        e.name.trim().to_string()
    };
    if e.clips.iter().any(|c| !Path::new(c.source.trim()).is_absolute()) {
        // Normalitzem les rutes relatives a l'arrel del projecte.
        for c in e.clips.iter_mut() {
            let p = render::resolve(root, &c.source);
            c.source = p.to_string_lossy().to_string();
        }
    }
    let (dropped, notes) = edl::sanitize(&mut e);
    let work = director::workdir(root);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let file = work.join("edl.json");
    let json = serde_json::to_string_pretty(&e).map_err(|e| e.to_string())?;
    std::fs::write(&file, json).map_err(|e| e.to_string())?;
    Ok(SanitizeReport {
        dropped,
        notes,
        duration_s: render::timeline_len(&e),
        edl: e,
        path: file.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn video_edl_set(project_path: String, edl: Edl) -> Result<SanitizeReport, String> {
    let root = root_of(&project_path)?;
    store_edl(&root, edl)
}

/// Accepta text de la IA (JSON amb o sense marques) i el converteix en EDL.
#[tauri::command]
pub fn video_edl_parse(project_path: String, raw: String) -> Result<SanitizeReport, String> {
    let root = root_of(&project_path)?;
    let e = director::parse_edl_text(&raw).ok_or("el text no conté cap EDL vàlid")?;
    store_edl(&root, e)
}

/// Muntatge determinista (sense IA) a partir dels orígens del projecte.
#[tauri::command]
pub async fn video_auto_edl(
    project_path: String,
    name: String,
    target_s: f64,
) -> Result<SanitizeReport, String> {
    let root = root_of(&project_path)?;
    let extra = director::saved_sources(&root);
    let found = director::sources_from_dir(&root, &extra);
    let r = root.clone();
    let list = tokio::task::spawn_blocking(move || {
        let srcs = ff::probe_all(&found);
        edl::auto_edl(&srcs, &name, target_s)
    })
    .await
    .map_err(|e| e.to_string())?;
    store_edl(&r, list)
}

/// Afegeix un origen concret a la llista del projecte.
#[tauri::command]
pub fn video_source_add(project_path: String, source: String) -> Result<String, String> {
    let root = root_of(&project_path)?;
    director::remember_source(&root, &source).map(|p| p.to_string_lossy().to_string())
}

/// Substitueix la llista d'orígens afegits a mà.
#[tauri::command]
pub fn video_sources_set(project_path: String, list: Vec<String>) -> Result<(), String> {
    let root = root_of(&project_path)?;
    director::save_sources(&root, &list)
}

// ─── RENDER ──────────────────────────────────────────────────────────────────

/// EDL per renderitzar: la de la UI, la desada, o un muntatge automàtic de reserva.
fn edl_for_render(root: &Path, given: Option<Edl>, name: &str) -> Result<(Edl, Vec<String>), String> {
    let mut notes = Vec::new();
    let base = match given {
        Some(e) if e.has_clips() => e,
        _ => director::load_edl(root).unwrap_or_default(),
    };
    if base.has_clips() {
        return Ok((base, notes));
    }
    let extra = director::saved_sources(root);
    let found = director::sources_from_dir(root, &extra);
    if found.is_empty() {
        return Err(
            "no hi ha cap origen per muntar. Genera'un amb Blender (@blender-video:) o afegeix \
             fitxers al panell «Vídeo»."
                .into(),
        );
    }
    let srcs = ff::probe_all(&found);
    let usable: Vec<ff::Source> = srcs.into_iter().filter(|s| s.readable).collect();
    if usable.is_empty() {
        return Err("cap dels vídeos trobats es pot llegir".into());
    }
    notes.push(format!(
        "no hi havia muntatge desat; muntatge automàtic de {} orígens",
        usable.len()
    ));
    Ok((edl::auto_edl(&usable, name, 0.0), notes))
}

/// Previsualitza l'ordre `ffmpeg` sense executar-la (per a la UI i per al registre).
#[tauri::command]
pub async fn video_plan(
    project_path: String,
    edl: Option<Edl>,
    preview: bool,
    container: String,
) -> Result<serde_json::Value, String> {
    let root = root_of(&project_path)?;
    let work = director::workdir(&root);
    let out_dir = if preview {
        work.join("analysis")
    } else {
        work.join("renders")
    };
    tokio::task::spawn_blocking(move || {
        let (e, _n) = edl_for_render(&root, edl, "muntatge")?;
        let pl = render::plan(
            &e,
            &root,
            &work,
            &out_dir,
            preview,
            render::Container::parse(&container),
            None,
        )
        .map_err(|x| x.to_string())?;
        Ok(serde_json::json!({
            "command": pl.command_line(),
            "out": pl.out.to_string_lossy(),
            "durationS": pl.dur_s,
            "note": pl.graph_note,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Rendera (final o previsualització) en segon terme, amb progrés i cancel·lació.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn video_render(
    app: AppHandle,
    handle: State<'_, Arc<RenderHandle>>,
    director_handle: State<'_, Arc<director::DirectorHandle>>,
    project_path: String,
    edl: Option<Edl>,
    preview: bool,
    container: String,
    open_after: bool,
) -> Result<(), String> {
    let root = root_of(&project_path)?;
    let h = handle.inner().clone();
    let snap = h.snapshot();
    if snap.running {
        return Err("ja hi ha un render en curs".into());
    }
    // El director també mou `ffmpeg` a dins: no els deixem coincidir.
    if director_handle.running.load(Ordering::SeqCst) {
        return Err(
            "el director de vídeo està treballant ara mateix; espera que acabi o atura'l".into(),
        );
    }
    let (e, notes) = edl_for_render(&root, edl, "muntatge")?;
    spawn_render(app, h, root, e, notes, preview, container, open_after);
    Ok(())
}

/// El render en si mateix, publicant estat. És el mateix camí per al botó del
/// panell i per a la directriu `@video:render` del xat; per això és una
/// funció a part i no dins la comanda (la comanda només comprova i arrenca).
#[allow(clippy::too_many_arguments)]
pub fn spawn_render(
    app: AppHandle,
    h: Arc<RenderHandle>,
    root: PathBuf,
    e: Edl,
    notes: Vec<String>,
    preview: bool,
    container: String,
    open_after: bool,
) {
    h.stop.store(false, Ordering::SeqCst);
    let work = director::workdir(&root);
    let out_dir = if preview {
        work.join("analysis")
    } else {
        work.join("renders")
    };
    let name = e.name.clone();
    let now = chrono::Local::now().timestamp();
    h.set(|s| {
        s.running = true;
        s.stage = "preparant".into();
        s.progress = 0.0;
        s.preview = preview;
        s.detail = if preview {
            "Preparant la previsualització…".into()
        } else {
            "Preparant el render final…".into()
        };
        s.out_path = None;
        s.report = None;
        s.error = None;
        s.started_at = now;
        s.elapsed_ms = 0;
    });
    emit_render(&app, &h);
    for n in &notes {
        app.emit("video://note", n).ok();
    }

    let stop = h.clone();
    tokio::task::spawn_blocking(move || {
        let t0 = std::time::Instant::now();
        let res: Result<(PathBuf, Option<serde_json::Value>), String> = (|| {
            let cont = render::Container::parse(&container);
            let measured = if e.normalize_audio && !preview {
                h.set(|s| {
                    s.stage = "sonoritat".into();
                    s.detail = "Mesurant la sonoritat (pas 1 de 2)…".into();
                });
                emit_render(&app, &h);
                render::measure_audio(&e, &root, &work)
            } else {
                None
            };
            h.set(|s| {
                s.stage = "codificant".into();
                s.detail = "Construint el graf de filtres…".into();
            });
            emit_render(&app, &h);
            let pl = render::plan(&e, &root, &work, &out_dir, preview, cont, measured.as_ref())
                .map_err(|x| x.to_string())?;
            h.set(|s| s.command_line = Some(pl.command_line()));
            emit_render(&app, &h);
            let out = pl.out.clone();
            let exec = render::execute(
                &pl,
                &root,
                &mut |f, txt| {
                    h.set(|s| {
                        s.progress = f;
                        s.detail = txt;
                        s.elapsed_ms = t0.elapsed().as_millis() as u64;
                    });
                    emit_render(&app, &h);
                },
                &|| stop.stop.load(Ordering::SeqCst),
            );
            render::cleanup(&pl);
            exec.map_err(|x| x.to_string())?;
            if stop.stop.load(Ordering::SeqCst) {
                return Err("render aturat per l'usuari".into());
            }
            let rep = if preview {
                None
            } else {
                h.set(|s| {
                    s.stage = "verificant".into();
                    s.detail = "Comprovant durada, sonoritat i plans morts…".into();
                });
                emit_render(&app, &h);
                render::verify(&out, &e).ok()
            };
            if !preview {
                h.set(|s| {
                    s.stage = "exportant".into();
                    s.detail = "Escrivint el projecte .kdenlive…".into();
                });
                emit_render(&app, &h);
                let mlt_path = out.with_extension("kdenlive");
                let ver = detect::detect().melt.version.clone().unwrap_or_else(|| "7.0.0".into());
                if let Err(err) = mlt::write_project(&e, &root, &mlt_path, &ver) {
                    app.emit("video://note", &format!("no s'ha pogut exportar el projecte: {err}"))
                        .ok();
                }
                // Un .srt al costat, per si l'usuari el vol importar a mà.
                if !e.texts.is_empty() {
                    let srt = out.with_extension("srt");
                    let _ = std::fs::write(&srt, mlt::to_srt(&e));
                }
            }
            if open_after && !preview {
                let _ = director::open_with_default(&out);
            }
            Ok((out, rep))
        })();
        let secs = t0.elapsed().as_secs();
        h.set(|s| {
            s.running = false;
            match &res {
                Ok((out, rep)) => {
                    s.stage = "fet".into();
                    s.progress = 1.0;
                    s.out_path = Some(out.to_string_lossy().to_string());
                    s.report = rep.clone();
                    s.detail = format!("{} acabat en {secs} s", if s.preview { "previsualització" } else { "render" });
                    s.error = None;
                }
                Err(err) => {
                    s.stage = "error".into();
                    s.error = Some(err.clone());
                    s.detail = err.clone();
                }
            }
            s.elapsed_ms = secs * 1000;
        });
        emit_render(&app, &h);
        let _ = name;
    });
}

/// Estat del render a mà.
#[tauri::command]
pub fn video_render_state(handle: State<'_, Arc<RenderHandle>>) -> RenderState {
    handle.snapshot()
}

/// Atura el render en curs (i el del director, si n'hi ha un).
#[tauri::command]
pub fn video_render_stop(
    handle: State<'_, Arc<RenderHandle>>,
    director: State<'_, Arc<director::DirectorHandle>>,
) {
    handle.stop.store(true, Ordering::SeqCst);
    director.stop_render.store(true, Ordering::SeqCst);
    if let Ok(c) = director.cancel.lock() {
        c.cancel();
    }
}

// ─── EXPORT I VERIFICACIÓ ────────────────────────────────────────────────────

/// Escriu el projecte `.kdenlive` (+ `.srt` si hi ha textos) i torna els fitxers.
#[tauri::command]
pub async fn video_export(
    project_path: String,
    edl: Option<Edl>,
) -> Result<Vec<String>, String> {
    let root = root_of(&project_path)?;
    tokio::task::spawn_blocking(move || {
        let (e, _n) = edl_for_render(&root, edl, "muntatge")?;
        let work = director::workdir(&root);
        let renders = work.join("renders");
        let _ = std::fs::create_dir_all(&renders);
        let mlt_path = renders.join(format!("{}.kdenlive", e.slug()));
        let ver = detect::detect().melt.version.clone().unwrap_or_else(|| "7.0.0".into());
        mlt::write_project(&e, &root, &mlt_path, &ver).map_err(|x| x.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Comprova un fitxer ja renderitzat contra l'EDL.
#[tauri::command]
pub async fn video_verify(out_path: String, edl: Edl) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        render::verify(Path::new(&out_path), &edl).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Graella de fotogrames d'un sol fitxer (per «veure'l» al panell).
#[tauri::command]
pub async fn video_sheet(project_path: String, path: String) -> Result<String, String> {
    let root = root_of(&project_path)?;
    let out = director::workdir(&root)
        .join("analysis")
        .join("inventari");
    tokio::task::spawn_blocking(move || {
        let _ = std::fs::create_dir_all(&out);
        let base = Path::new(&path)
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "origen".into());
        let clean: String = base
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
            .collect();
        let png = out.join(format!("{}.png", clean.chars().take(40).collect::<String>()));
        ff::contact_sheet(&path, &png, 4, 3, 384)
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ─── ABRIR COSES ─────────────────────────────────────────────────────────────

/// Obre el vídeo amb el reproductor per defecte del sistema.
#[tauri::command]
pub fn video_open(path: String) -> Result<(), String> {
    director::open_with_default(Path::new(path.trim()))
}

/// Obre Kdenlive amb el projecte (per polir-lo a mà).
#[tauri::command]
pub async fn video_open_kdenlive(mlt_path: String) -> Result<String, String> {
    let p = PathBuf::from(mlt_path.trim());
    if !p.exists() {
        return Err(format!("el projecte no existeix: {}", p.display()));
    }
    tokio::task::spawn_blocking(move || {
        let tools = detect::detect();
        let exe = tools
            .kdenlive
            .path
            .clone()
            .ok_or("Kdenlive no està instal·lat (el trobem amb el seu paquet: instal·la'l)")?;
        let status = std::process::Command::new(&exe)
            .arg(&p)
            .spawn()
            .map_err(|e| format!("no s'ha pogut arrencar Kdenlive: {e}"))?
            .wait()
            .ok();
        let _ = status;
        Ok(format!("Kdenlive obert amb {}", p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ─── DIRECTOR (tot autopilotat) ──────────────────────────────────────────────

/// Arrenca el director: mira, dissenya, es critica i renderitza.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn video_director_start(
    app: AppHandle,
    state: State<'_, AppState>,
    handle: State<'_, Arc<director::DirectorHandle>>,
    project_path: String,
    name: String,
    brief: String,
    sources: Vec<String>,
    budget_secs: u64,
    target_s: f64,
    provider: Option<String>,
    model: Option<String>,
    critiques: u32,
    width: Option<u32>,
    height: Option<u32>,
    fps: Option<f64>,
    crf: Option<u32>,
    preset: Option<String>,
    container: String,
    silent: bool,
) -> Result<(), String> {
    let root = root_of(&project_path)?;
    let budget = std::time::Duration::from_secs(budget_secs.clamp(60, director::MAX_BUDGET_SECS));
    let req = director::Request {
        root,
        name: if name.trim().is_empty() {
            "Muntatge".into()
        } else {
            name.trim().to_string()
        },
        brief,
        sources,
        budget,
        target_s: if target_s.is_finite() { target_s.clamp(0.0, 3600.0 * 4.0) } else { 0.0 },
        provider,
        model,
        critiques: critiques.min(director::MAX_CRITIQUES),
        width,
        height,
        fps,
        crf,
        preset,
        container,
        silent,
    };
    director::start(
        app,
        state.ai_manager.clone(),
        handle.inner().clone(),
        req,
    )
}

/// Atura el director (i el seu render si n'hi ha un en curs).
#[tauri::command]
pub fn video_director_stop(handle: State<'_, Arc<director::DirectorHandle>>) {
    if let Ok(c) = handle.cancel.lock() {
        c.cancel();
    }
    handle.stop_render.store(true, Ordering::SeqCst);
}

/// Estat del director per omplir el panell en obrir-lo.
#[tauri::command]
pub fn video_director_state(
    handle: State<'_, Arc<director::DirectorHandle>>,
) -> director::DirectorState {
    handle.snapshot()
}

/// Llista de transicions vàlides (per omplir els menús del panell).
#[tauri::command]
pub fn video_transitions() -> Vec<&'static str> {
    director::known_transitions()
}

/// Executa una directriu `@video:` des del XAT. És exactament el mateix codi
/// que empren la Jam i el treball autopilotat (`run_directive`), així que el
/// que la IA pot fer des del panell és el mateix que pot fer sola. El text que
/// torna és el que el model veuria: resum curt en verd, error explicatiu en
/// vermell.
#[tauri::command]
pub async fn video_directive(project_path: String, args: String) -> Result<String, String> {
    let root = root_of(&project_path)?;
    let (ok, text) = run_directive(root, &args).await;
    if ok {
        Ok(text)
    } else {
        Err(text)
    }
}

// ─── DIRECTRIUS `@video:` (xat, Jam i treball autopilotat) ─────────────

/// Pont amb els bucles d'IA: aquells fils NO reben `State<'_, …>` de Tauri,
/// així que els handles es publiquen una vegada des de `setup()`. Si no hi
/// ha runtime, les accions que el necessiten (render, director) ho diuen
/// clar en lloc de quedar-se mortes en silenci.
pub struct Runtime {
    pub app: AppHandle,
    pub ai: Arc<crate::ai::AiManager>,
    pub render: Arc<RenderHandle>,
    pub director: Arc<director::DirectorHandle>,
}

static RUNTIME: std::sync::OnceLock<Runtime> = std::sync::OnceLock::new();

/// Publica els handles (es crida des de `lib.rs`, un cop).
pub fn set_runtime(rt: Runtime) {
    let _ = RUNTIME.set(rt);
}

fn runtime() -> Option<&'static Runtime> {
    RUNTIME.get()
}

/// Separa `acció` d'`arguments` d'una directriu.
fn split_directive(args: &str) -> (String, String) {
    let t = args.trim();
    match t.find(char::is_whitespace) {
        Some(i) => (t[..i].to_ascii_lowercase(), t[i..].trim().to_string()),
        None => (t.to_ascii_lowercase(), String::new()),
    }
}

/// Executa `@video:<acció> [arguments]` i torna `(ok, resum)` per al model.
/// Cap error panica: tot torna en text que el model pot entendre i corregir.
pub async fn run_directive(root: PathBuf, args: &str) -> (bool, String) {
    let (action, rest) = split_directive(args);
    match action.as_str() {
        "" | "status" => (true, status_text(&root).await),
        "scan" => scan_text(&root).await,
        "mount" => match mount_edl(&root, &rest) {
            Ok(s) => (true, s),
            Err(e) => (false, e),
        },
        "auto" => auto_mount(&root, &rest).await,
        "render" => start_render(&root, &rest, false).await,
        "preview" => start_render(&root, &rest, true).await,
        "export" => export_project(&root).await,
        "verify" => verify_last(&root).await,
        "open" => match director::remember_source(&root, &rest) {
            Ok(p) => (true, format!("origen afegit a la llista: {}", p.display())),
            Err(e) => (false, e),
        },
        "director" => start_director(&root, &rest),
        other => (
            false,
            format!(
                "acció de vídeo desconeguda: «{other}». vàlides: status, scan, mount, auto, \
                 render, preview, export, verify, open, director"
            ),
        ),
    }
}

/// Quines eines hi ha, si el projecte ja té muntatge i què està corrent.
async fn status_text(root: &Path) -> String {
    let tools = tokio::task::spawn_blocking(detect::detect)
        .await
        .unwrap_or_default();
    let edl = director::load_edl(root);
    let work = director::workdir(root);
    let mut s = String::new();
    s.push_str(&format!("eines: {}", tools.label()));
    if !tools.ready {
        s.push_str(" (no es pot renderitzar: falta ffmpeg/ffprobe; instal·la Kdenlive)");
    }
    match &edl {
        Some(e) => s.push_str(&format!(
            "\nmuntatge desat: {} clips, {:.1} s, «{}»",
            e.clips.len(),
            render::timeline_len(e),
            e.name
        )),
        None => s.push_str("\nmuntatge desat: cap (fes `@video:auto 20` o `@video:mount {…}`)"),
    }
    let found = director::sources_from_dir(root, &director::saved_sources(root));
    s.push_str(&format!("\norígens de vídeo: {}", found.len()));
    if let Some(r) = last_render_of(&work) {
        s.push_str(&format!("\núltim render: {r}"));
    }
    if let Some(rt) = runtime() {
        let rs = rt.render.snapshot();
        if rs.running {
            s.push_str(&format!(
                "\nrender en curs: {} · {:.0}% · {}",
                rs.stage,
                rs.progress * 100.0,
                rs.detail
            ));
        }
        if rt.director.running.load(Ordering::SeqCst) {
            let ds = rt.director.snapshot();
            s.push_str(&format!("\ndirector: passa {} · {} · {}", ds.pass, ds.stage, ds.detail));
        }
    }
    s
}

/// Inventari mesurat de tots els vídeos del projecte.
async fn scan_text(root: &Path) -> (bool, String) {
    let extra = director::saved_sources(root);
    let found = director::sources_from_dir(root, &extra);
    if found.is_empty() {
        return (
            false,
            "no hi ha cap vídeo al projecte. Genera'n un amb `@blender-video: <ruta.blend>` o \
             afegeix-ne un amb `@video:open <ruta>`."
                .into(),
        );
    }
    let list = tokio::task::spawn_blocking(move || ff::probe_all(&found))
        .await
        .map_err(|e| format!("l'escaneig s'ha tallat: {e}"))
        .unwrap_or_default();
    let usable: Vec<&ff::Source> = list.iter().filter(|s| s.readable).collect();
    if usable.is_empty() {
        return (false, "hi ha fitxers de vídeo, però ffprobe no n'ha pogut llegir cap".into());
    }
    let mut s = format!("{} orígens llegibles:\n", usable.len());
    for src in &usable {
        s.push_str(&format!(
            "- {} · {:.2} s · {}×{} @ {:.2} fps · àudio {}\n",
            src.path,
            src.duration,
            src.width,
            src.height,
            src.fps,
            if src.has_audio { "sí" } else { "no" }
        ));
    }
    if let Some(b) = list.iter().find(|x| !x.readable) {
        s.push_str(&format!("(no es pot llegir: {})\n", b.path));
    }
    (true, s)
}

/// Guarda un EDL escrit pel model sense renderitzar res.
fn mount_edl(root: &Path, json: &str) -> Result<String, String> {
    let e = director::parse_edl_text(json).ok_or(
        "el text de `@video:mount` no conté un EDL vàlid: recorda l'estructura \
         {\"name\":…,\"clips\":[{\"source\":…,\"inS\":…,\"outS\":…}]}",
    )?;
    let r = store_edl(root, e)?;
    let mut s = format!(
        "muntatge desat: {} clips, {:.2} s, {} textos → {}",
        r.edl.clips.len(),
        r.duration_s,
        r.edl.texts.len(),
        r.path
    );
    for n in r.notes.iter().take(6) {
        s.push_str(&format!("\navís: {n}"));
    }
    if r.dropped > 0 {
        s.push_str(&format!("\nclips descartats: {}", r.dropped));
    }
    Ok(s)
}

/// Muntatge determinista (sense IA) i, si pot, render immediat.
async fn auto_mount(root: &Path, args: &str) -> (bool, String) {
    let target_s: f64 = args
        .split_whitespace()
        .next()
        .and_then(|x| x.parse().ok())
        .unwrap_or(0.0);
    let extra = director::saved_sources(root);
    let found = director::sources_from_dir(root, &extra);
    if found.is_empty() {
        return (false, "no hi ha orígens de vídeo per muntar".into());
    }
    let e = tokio::task::spawn_blocking(move || {
        let srcs = ff::probe_all(&found);
        edl::auto_edl(&srcs, "Muntatge automàtic", target_s)
    })
    .await
    .map_err(|e| format!("s'ha tallat: {e}"))
    .unwrap_or_default();
    if !e.has_clips() {
        return (false, "cap vídeo llegible: no hi ha què muntar".into());
    }
    match store_edl(&root, e) {
        Ok(rep) => (
            true,
            format!(
                "muntatge automàtic desat: {} clips, {:.2} s. Ara `@video:render` per \
                 codificar-lo (o `@video:preview` per veure-ho de pressa).",
                rep.edl.clips.len(),
                rep.duration_s
            ),
        ),
        Err(err) => (false, err),
    }
}

/// Arrenca el render del muntatge desat (o del que el model passi per JSON).
async fn start_render(root: &Path, args: &str, preview: bool) -> (bool, String) {
    let Some(rt) = runtime() else {
        return (false, "l'aplicació no ha publicat els handles de vídeo: obre el panell «Vídeo»".into());
    };
    if rt.director.running.load(Ordering::SeqCst) {
        return (
            false,
            "el director de vídeo ja està treballant; espera (`@video:status`) o atura'l".into(),
        );
    }
    // El model pot passar un EDL per renderitzar o confiar en el desat.
    let donat = director::parse_edl_text(args);
    let (e, notes) = match edl_for_render(root, donat, "muntatge") {
        Ok(v) => v,
        Err(err) => return (false, err),
    };
    let cont = args
        .split_whitespace()
        .find(|w| w.starts_with("container="))
        .and_then(|w| w.split_once('='))
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| "mp4".into());
    if rt.render.snapshot().running {
        return (false, "ja hi ha un render en curs; consulta `@video:status`".into());
    }
    let nclips = e.clips.len();
    spawn_render(
        rt.app.clone(),
        rt.render.clone(),
        root.to_path_buf(),
        e,
        notes,
        preview,
        cont,
        false,
    );
    (
        true,
        format!(
            "{} en curs amb {} clips: els progressos van al panell «Vídeo»; \
             consulta'ls amb `@video:status`.",
            if preview { "previsualització" } else { "render" },
            nclips
        ),
    )
}

/// Escriu el projecte `.kdenlive` (+ `.srt`) per polir-ho a la GUI.
async fn export_project(root: &Path) -> (bool, String) {
    let r = root.to_path_buf();
    let out = tokio::task::spawn_blocking(move || {
        let (e, _n) = edl_for_render(&r, None, "muntatge")?;
        let work = director::workdir(&r);
        let renders = work.join("renders");
        let _ = std::fs::create_dir_all(&renders);
        let mlt_path = renders.join(format!("{}.kdenlive", e.slug()));
        let ver = detect::detect()
            .melt
            .version
            .clone()
            .unwrap_or_else(|| "7.0.0".into());
        mlt::write_project(&e, &r, &mlt_path, &ver).map_err(|x| x.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|x| x);
    match out {
        Ok(files) => (
            true,
            format!("projecte exportat: {}", files.join(", ")),
        ),
        Err(e) => (false, e),
    }
}

/// Comprova l'últim render contra l'EDL desat.
async fn verify_last(root: &Path) -> (bool, String) {
    let work = director::workdir(root);
    let Some(out) = last_render_of(&work) else {
        return (false, "encara no hi ha cap render a verificar".into());
    };
    let Some(e) = director::load_edl(root) else {
        return (false, "no hi ha muntatge desat contra què comparar".into());
    };
    let p = PathBuf::from(&out);
    let rep = tokio::task::spawn_blocking(move || render::verify(&p, &e))
        .await
        .map_err(|e| e.to_string())
        .and_then(|x| x.map_err(|e| e.to_string()));
    match rep {
        Ok(v) => (
            v.get("ok").and_then(|b| b.as_bool()).unwrap_or(true),
            format!("comprovació de {out}: {}", v),
        ),
        Err(e) => (false, e),
    }
}

/// Arrenca el director autopilotat amb els paràmetres que dona el model.
fn start_director(root: &Path, json: &str) -> (bool, String) {
    let Some(rt) = runtime() else {
        return (
            false,
            "l'aplicació no ha publicat els handles de vídeo: obre el panell «Vídeo»".into(),
        );
    };
    if rt.director.running.load(Ordering::SeqCst) {
        return (false, "ja hi ha un director de vídeo treballant".into());
    }
    let parsed: Value = if json.trim().is_empty() {
        Value::Null
    } else {
        match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => {
                return (false, format!("`@video:director` espera un JSON d'una línia ({e})"))
            }
        }
    };
    let get_s = |k: &str| parsed.get(k).and_then(|v| v.as_str()).map(|s| s.to_string());
    let get_f = |k: &str| parsed.get(k).and_then(|v| v.as_f64());
    let get_u = |k: &str| parsed.get(k).and_then(|v| v.as_u64());
    let budget = get_u("budgetS").unwrap_or(2400).clamp(60, director::MAX_BUDGET_SECS);
    let sources: Vec<String> = parsed
        .get("sources")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    // Si el model no n'ha triat cap, el director descobrirà els del projecte;
    // ho fem constar al missatge perquè l'usuari sàpiga què es mirarà.
    let nsrc = if sources.is_empty() {
        director::sources_from_dir(root, &director::saved_sources(root)).len()
    } else {
        sources.len()
    };
    let req = director::Request {
        root: root.to_path_buf(),
        name: get_s("name").unwrap_or_else(|| "Muntatge".into()),
        brief: get_s("brief").unwrap_or_default(),
        sources,
        budget: std::time::Duration::from_secs(budget),
        target_s: get_f("targetS").unwrap_or(0.0).clamp(0.0, 4.0 * 3600.0),
        provider: get_s("provider"),
        model: get_s("model"),
        critiques: (get_u("critiques").unwrap_or(2) as u32).min(director::MAX_CRITIQUES),
        width: get_u("width").map(|w| w as u32),
        height: get_u("height").map(|h| h as u32),
        fps: get_f("fps"),
        crf: get_u("crf").map(|c| c as u32),
        preset: get_s("preset"),
        container: get_s("container").unwrap_or_else(|| "mp4".into()),
        silent: parsed
            .get("silent")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    };
    match director::start(rt.app.clone(), rt.ai.clone(), rt.director.clone(), req) {
        Ok(()) => (
            true,
            format!(
                "director arrencat (pressupost {budget} s, {nsrc} orígens per mirar). Va mirant els \
                 clips, escriu el muntatge, el critica i renderitza; l'estat es pot seguir amb \
                 `@video:status`.",
            ),
        ),
        Err(e) => (false, e),
    }
}
