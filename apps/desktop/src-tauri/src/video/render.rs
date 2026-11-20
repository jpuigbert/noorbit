//! Motor de muntatge i renderització.
//!
//! Decisió d'arquitectura important: **el render autoritzat es fa amb
//! `ffmpeg`, no amb `melt`**. `xfade`/`acrossfade` donen dissolucions reals i
//! mesurables (s'ha verificat píxel a píxel: al mig del fús surt exactament el
//! 50% de cada pla), mentre que les transicions MLT depenen de la semàntica
//! interna del producer de la transició i no rampen llevat que l'XML reprodueixi
//! exactament com ho escriu Kdenlive. El projecte `.kdenlive` (`mlt.rs`) sí que
//! es genera, perquè l'humà el pugui obrir i polir a la GUI.
//!
//! El graf es construeix amb un acumulador: cada clip nou es fusiona amb el
//! resultat acumulat amb `xfade` (si hi ha transició) o `concat` (tall sec).
//! Vídeo i àudio es construeixen per separat amb la MATEIXA aritmètica de
//! temps, de manera que les dues pistes mai no es desencaixen.

use anyhow::{anyhow, Result};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::video::detect;
use crate::video::edl::{Clip, Edl, TextItem};
use crate::video::ff;

/// Format de sortida final.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Container {
    Mp4,
    Mov,
    Webm,
}

impl Container {
    pub fn ext(&self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mov => "mov",
            Container::Webm => "webm",
        }
    }

    pub fn parse(s: &str) -> Container {
        match s.trim().to_ascii_lowercase().as_str() {
            "mov" => Container::Mov,
            "webm" => Container::Webm,
            _ => Container::Mp4,
        }
    }
}

/// Graf ja resolt: inputs, filtres i durades.
struct Graph {
    /// Opcions d'entrada (`-ss/-t/-i` per clip).
    inputs: Vec<String>,
    /// Pieces del `filter_complex` (se uneixen amb `;`).
    filters: Vec<String>,
    /// Etiqueta del vídeo final (`[vfin]`) — buida si `video=false`.
    vlabel: String,
    /// Etiqueta de l'àudio final (`[afin]`).
    alabel: String,
    /// Durada total resultant.
    len: f64,
    /// Camins temporals generats (`.ass`, `.trf`).
    temps: Vec<PathBuf>,
    /// Coses que el motor HA hagut de deixar córrer (s'ensenyen a la UI).
    notes: Vec<String>,
}

/// El que cal executar: ordre resolt més dades per a la UI.
pub struct Plan {
    pub exe: String,
    pub args: Vec<String>,
    pub out: PathBuf,
    pub dur_s: f64,
    pub temps: Vec<PathBuf>,
    pub graph_note: String,
}

impl Plan {
    /// Ordre completa (per mostrar-la a la UI i per al registre de l'operació).
    pub fn command_line(&self) -> String {
        let mut s = self.exe.clone();
        for a in &self.args {
            s.push(' ');
            if a.contains(' ') || a.contains('\'') || a.contains(';') {
                s.push('"');
                s.push_str(a);
                s.push('"');
            } else {
                s.push_str(a);
            }
        }
        s
    }
}

/// Resol un origen relatiu contra l'arrel del projecte.
pub fn resolve(root: &Path, source: &str) -> PathBuf {
    let p = Path::new(source.trim());
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

/// Camí relativ a l'arrel del projecte si es pot (evita escapars `:` als filtres).
fn rel_to(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .map(|r| r.to_string_lossy().to_string())
        .unwrap_or_else(|_| p.to_string_lossy().to_string())
        .replace('\\', "/")
}

/// Durada crua (abans de velocitat) retallada al que realment dura l'origen.
fn raw_len(c: &Clip, src_dur: f64) -> (f64, f64) {
    let in_s = c.in_s.clamp(0.0, src_dur.max(0.0));
    let raw = (c.out_s - c.in_s).max(0.04).min((src_dur - in_s).max(0.04));
    (in_s, raw)
}

/// Velocitat usable del clip, amb la MATEIXA regla que `edl::sanitize`.
/// No s'usa `abs()`: una velocitat negativa donaria un `-t` negatiu a
/// `ffmpeg` (i per tant un render descartat) i una de pròxima a zero, una
/// durada desorbitada. Fora del rang raonable es tracta com si fos 1,0.
fn speed_of(c: &Clip) -> f64 {
    if c.speed < 0.25 || c.speed > 8.0 {
        1.0
    } else {
        c.speed
    }
}

/// Solapament efectiu de la transició del clip `prev` amb el clip següent.
fn overlap(prev: &Clip, prev_len: f64, next_len: f64) -> f64 {
    match &prev.transition {
        Some(t) => t.dur_s.clamp(0.0, (prev_len * 0.5).min(next_len * 0.5).min(4.0)),
        None => 0.0,
    }
}

/// Durada total prevista d'un EDL (mateixa aritmètica que el graf).
pub fn timeline_len(edl: &Edl) -> f64 {
    let mut acc = 0.0f64;
    for (i, c) in edl.clips.iter().enumerate() {
        if i > 0 {
            acc -= overlap(&edl.clips[i - 1], acc, c.len_s());
        }
        acc += c.len_s();
    }
    acc.max(0.0)
}

/// Cadena de vídeo d'un clip: fps, encaix al perfil, velocitat, color,
/// estabilització i fusos.
fn video_chain(i: usize, c: &Clip, edl: &Edl, len: f64, trf: Option<&str>, out: &str) -> String {
    let p = &edl.profile;
    let mut f: Vec<String> = Vec::new();
    f.push(format!("fps={:.4}", p.fps));
    f.push(format!(
        "scale={}:{}:force_original_aspect_ratio=decrease:flags=lanczos",
        p.width, p.height
    ));
    f.push(format!("pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=black", p.width, p.height));
    f.push("setsar=1".into());
    f.push("settb=AVTB".into());
    let sp = speed_of(c);
    if (sp - 1.0).abs() > 0.001 {
        f.push(format!("setpts=PTS/{sp:.4}"));
    } else {
        f.push("setpts=PTS-STARTPTS".into());
    }
    if let Some(t) = trf {
        // Estabilització veritable: `vidstabdetect` (fas 1) + transformació.
        f.push(format!("vidstabtransform=input='{t}':smoothing=30:optzoom=1:interpolation=bicubic"));
        f.push("unsharp=5:5:0.6:5:5:0.0".into());
        f.push(format!("scale={}:{}:flags=lanczos", p.width, p.height));
    }
    if let Some(g) = &c.grade {
        if g.brightness.abs() > 0.005 || (g.contrast - 1.0).abs() > 0.005 || (g.saturation - 1.0).abs() > 0.005 {
            f.push(format!(
                "eq=brightness={:.3}:contrast={:.3}:saturation={:.3}",
                g.brightness.clamp(-0.25, 0.25),
                g.contrast.clamp(0.5, 2.0),
                g.saturation.clamp(0.0, 2.2)
            ));
        }
        if g.warmth.abs() > 0.01 {
            let w = g.warmth.clamp(-1.0, 1.0) * 0.14;
            f.push(format!(
                "colorbalance=rs={:.3}:bs={:.3}:rm={:.3}:bm={:.3}",
                w * 0.7,
                -w * 0.7,
                w,
                -w
            ));
        }
    }
    let fo = (len - c.fade_out_s).max(0.0);
    if c.fade_in_s > 0.01 {
        f.push(format!("fade=t=in:st=0:d={:.3}", c.fade_in_s));
    }
    if c.fade_out_s > 0.01 {
        f.push(format!("fade=t=out:st={fo:.3}:d={:.3}", c.fade_out_s));
    }
    f.push("format=yuv420p".into());
    format!("[{i}:v]{}[{out}]", f.join(","))
}

/// Cadena d'àudio d'un clip (o silenci sintètic si no en té o està tallat).
fn audio_chain(i: usize, c: &Clip, len: f64, has_audio: bool, out: &str) -> String {
    if c.mute || !has_audio {
        // Cal una corrent REAL per no perdre temps a la línia de temps.
        return format!(
            "anullsrc=r=48000:cl=stereo,atrim=duration={len:.4},asetpts=PTS-STARTPTS,asettb=AVTB[{out}]"
        );
    }
    let mut f: Vec<String> = Vec::new();
    let sp = speed_of(c);
    if (sp - 1.0).abs() > 0.001 {
        // `atempo` només accepta 0,5…2: se'n concatenen tantes com calguin.
        let mut rem = sp;
        while rem > 2.0 {
            f.push("atempo=2.0".into());
            rem /= 2.0;
        }
        while rem < 0.5 {
            f.push("atempo=0.5".into());
            rem /= 0.5;
        }
        if (rem - 1.0).abs() > 0.001 {
            f.push(format!("atempo={rem:.4}"));
        }
    }
    f.push("aformat=sample_rates=48000:channel_layouts=stereo".into());
    f.push("asetpts=PTS-STARTPTS".into());
    let g = if c.gain > 0.0 { c.gain.clamp(0.05, 4.0) } else { 1.0 };
    if (g - 1.0).abs() > 0.005 {
        f.push(format!("volume={g:.3}"));
    }
    let fo = (len - c.fade_out_s).max(0.0);
    if c.fade_in_s > 0.01 {
        f.push(format!("afade=t=in:st=0:d={:.3}", c.fade_in_s));
    }
    if c.fade_out_s > 0.01 {
        f.push(format!("afade=t=out:st={fo:.3}:d={:.3}", c.fade_out_s));
    }
    format!("[{i}:a]{}[{out}]", f.join(","))
}

/// Hi ha aquest filtre a la `ffmpeg` detectada? Serveix per DEGRADAR
/// funcions opcionals (`vidstab*`) en lloc de fer fallar un render sencer.
fn has_filter(exe: &str, name: &str) -> bool {
    Command::new(exe)
        .args(["-hide_banner", "-filters"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.split_whitespace().nth(1) == Some(name))
        })
        .unwrap_or(false)
}

/// Fas 1 de l'estabilització: analitza un origen i desa la traça `.trf`.
fn stabilize_detect(source: &Path, trf: &Path, exe: &str) -> Result<()> {
    let status = Command::new(exe)
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(source)
        .arg("-vf")
        .arg(format!("vidstabdetect=shakiness=6:accuracy=18:result={}", trf.display()))
        .arg("-an")
        .arg("-f")
        .arg("null")
        .arg("-")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| anyhow!("no s'ha pogut analitzar el moviment: {e}"))?;
    if !status.success() {
        return Err(anyhow!("l'anàlisi de moviment ha fallat"));
    }
    Ok(())
}

/// Construeix el graf complet (o només l'àudio, per mesurar LUFS).
fn build_graph(
    edl: &Edl,
    root: &Path,
    work: &Path,
    preview: bool,
    want_video: bool,
    measured: Option<&ff::Loudness>,
) -> Result<Graph> {
    if !edl.has_clips() {
        return Err(anyhow!("l'EDL no té cap clip"));
    }
    let exe = detect::locate("ffmpeg")
        .ok_or_else(|| anyhow!("NoOrbit no ha trobat `ffmpeg` (ve amb Kdenlive)"))?;
    std::fs::create_dir_all(work).ok();

    let prof = if preview { edl.profile.preview() } else { edl.profile.clone() };
    let mut e = edl.clone();
    e.profile = prof;

    let mut g = Graph {
        inputs: Vec::new(),
        filters: Vec::new(),
        vlabel: String::new(),
        alabel: String::new(),
        len: 0.0,
        temps: Vec::new(),
        notes: Vec::new(),
    };
    // L'estabilització és OPCIONAL: el `ffmpeg` que ve dins el `.app` de
    // Kdenlive no porta compilat `vidstab*`. Si faltés, un pla amb
    // `stabilize: true` hauria fallat de cap; aquí avall simplement s'omet i
    // es deixa constància visible a la nota del pla.
    let stab_ok = !preview && want_video && has_filter(&exe, "vidstabtransform");

    let mut lens: Vec<f64> = Vec::new();
    let mut haud: Vec<bool> = Vec::new();
    let mut trfs: Vec<Option<String>> = Vec::new();

    for c in &e.clips {
        let src = resolve(root, &c.source);
        if !src.exists() {
            return Err(anyhow!("l'origen no existeix: {}", src.display()));
        }
        let s = ff::probe(&src.to_string_lossy())?;
        let (in_s, raw) = raw_len(c, s.duration);
        let len = raw / speed_of(c);
        lens.push(len);
        haud.push(s.has_audio);

        g.inputs.push("-ss".into());
        g.inputs.push(format!("{in_s:.4}"));
        g.inputs.push("-t".into());
        g.inputs.push(format!("{raw:.4}"));
        g.inputs.push("-i".into());
        g.inputs.push(rel_to(root, &src));

        let stab = c.grade.as_ref().map(|x| x.stabilize).unwrap_or(false);
        if stab && stab_ok {
            let stem = src
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "clip".into());
            let trf = work.join(format!("stab-{}.trf", crate::video::edl_slug_of(&stem)));
            stabilize_detect(&src, &trf, &exe)?;
            if trf.exists() {
                g.temps.push(trf.clone());
                trfs.push(Some(rel_to(root, &trf)));
            } else {
                trfs.push(None);
            }
        } else {
            if stab && !g.notes.iter().any(|n| n.contains("estabilització")) {
                g.notes.push(
                    "estabilització omesa: aquesta ffmpeg no porta el filtre `vidstab`".into(),
                );
            }
            trfs.push(None);
        }
    }

    // Cadenes per clip.
    let mut vl: Vec<String> = Vec::new();
    let mut al: Vec<String> = Vec::new();
    for (i, c) in e.clips.iter().enumerate() {
        let o = format!("v{i}");
        if want_video {
            g.filters.push(video_chain(i, c, &e, lens[i], trfs[i].as_deref(), &o));
            vl.push(format!("[{o}]"));
        }
        let a = format!("a{i}");
        g.filters.push(audio_chain(i, c, lens[i], haud[i], &a));
        al.push(format!("[{a}]"));
    }

    // Fusió acumulativa.
    let mut acc_v = vl.first().cloned().unwrap_or_default();
    let mut acc_a = al.first().cloned().unwrap_or_default();
    let mut acc_len = *lens.first().unwrap_or(&0.0);
    for i in 1..e.clips.len() {
        let prev = &e.clips[i - 1];
        let ov = overlap(prev, acc_len, lens[i]);
        let nv = format!("[vx{i}]");
        let na = format!("[ax{i}]");
        if ov > 0.04 {
            let kind = prev
                .transition
                .as_ref()
                .map(|t| crate::video::edl::normalize_transition(&t.kind))
                .unwrap_or_else(|| "fade".into());
            let off = (acc_len - ov).max(0.0);
            if want_video {
                g.filters.push(format!(
                    "{acc_v}[v{i}]xfade=transition={kind}:duration={ov:.4}:offset={off:.4},settb=AVTB{nv}"
                ));
            }
            g.filters.push(format!(
                "{acc_a}[a{i}]acrossfade=d={ov:.4}:c1=tri:c2=tri,asettb=AVTB{na}"
            ));
            acc_len = acc_len + lens[i] - ov;
        } else {
            // Tall sec: `concat` de vídeo (si en volem) i d'àudio.
            if want_video {
                g.filters.push(format!("{acc_v}[v{i}]concat=n=2:v=1:a=0{nv}"));
            }
            g.filters.push(format!("{acc_a}[a{i}]concat=n=2:v=0:a=1{na}"));
            acc_len += lens[i];
        }
        acc_v = nv;
        acc_a = na;
    }

    g.alabel = acc_a;
    g.vlabel = if want_video { acc_v } else { String::new() };
    g.len = acc_len;

    // Textos en pantalla (subtitlat ASS) sobre el vídeo ja muntat.
    if want_video && !e.texts.is_empty() {
        let ass = work.join(format!("{}.ass", e.slug()));
        write_ass(&e, &ass)?;
        let rel = rel_to(root, &ass);
        g.temps.push(ass);
        g.filters.push(format!("{}subtitles={rel}[vtext]", g.vlabel));
        g.vlabel = "[vtext]".into();
    }

    // Normalització R128 en dos passos (amb mesures reals) o dinàmica.
    if e.normalize_audio {
        let mut lf = format!("loudnorm=I={:.1}:TP=-1.5:LRA=11", e.target_lufs);
        if let Some(m) = measured {
            // Mode lineal: aplica el guany calculat al pas 1 (qualitat de mestress).
            let off = if m.target_offset.abs() > 0.01 {
                m.target_offset
            } else {
                (e.target_lufs - m.input_i).clamp(-9.0, 9.0)
            };
            lf.push_str(&format!(
                ":linear=true:measured_I={:.2}:measured_TP={:.2}:measured_LRA={:.2}:measured_thresh={:.2}:offset={:.2}",
                m.input_i, m.input_tp, m.input_lra, m.input_thresh, off
            ));
        }
        g.filters.push(format!("{}{lf}[anorm]", g.alabel));
        g.alabel = "[anorm]".into();
    }

    Ok(g)
}

/// Nom de fitxer net per a temporals d'un origen.
pub fn edl_slug_of(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let t = out.trim_matches('-').to_string();
    if t.is_empty() {
        "clip".into()
    } else {
        t.chars().take(40).collect()
    }
}

/// Mesura la sonoritat REAL del mestress d'àudio (fas 1 de `loudnorm`).
pub fn measure_audio(edl: &Edl, root: &Path, work: &Path) -> Option<ff::Loudness> {
    let exe = detect::locate("ffmpeg")?;
    let g = build_graph(edl, root, work, false, false, None).ok()?;
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-nostdin".into(), "-loglevel".into(), "info".into()];
    args.extend(g.inputs);
    args.extend(["-filter_complex".into(), g.filters.join(";")]);
    args.extend(["-map".into(), g.alabel.clone(), "-vn".into(), "-f".into(), "null".into(), "-".into()]);
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = Command::new(&exe)
        .current_dir(root)
        .args(&refs)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    let se = String::from_utf8_lossy(&out.stderr).to_string();
    ff::parse_loudnorm_json(&se)
}

/// Construeix l'ordre de render completa.
pub fn plan(
    edl: &Edl,
    root: &Path,
    work: &Path,
    out_dir: &Path,
    preview: bool,
    container: Container,
    measured: Option<&ff::Loudness>,
) -> Result<Plan> {
    let exe = detect::locate("ffmpeg")
        .ok_or_else(|| anyhow!("NoOrbit no ha trobat `ffmpeg` (ve amb Kdenlive)"))?;
    std::fs::create_dir_all(out_dir).ok();
    let g = build_graph(edl, root, work, preview, true, measured)?;
    let e = if preview {
        let mut x = edl.clone();
        x.profile = edl.profile.preview();
        x
    } else {
        edl.clone()
    };

    let out = out_dir.join(format!("{}.{}", e.slug(), container.ext()));
    let crf = if preview { 30 } else { e.profile.crf };
    let preset = if preview { "ultrafast" } else { e.profile.preset.as_str() };

    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "info".into(),
        "-stats_period".into(),
        "0.5".into(),
    ];
    args.extend(g.inputs);
    args.extend(["-filter_complex".into(), g.filters.join(";")]);
    args.extend(["-map".into(), g.vlabel.clone(), "-map".into(), g.alabel.clone()]);
    args.extend(["-map_metadata".into(), "-1".into()]);

    match container {
        Container::Mp4 | Container::Mov => args.extend([
            "-c:v".into(), "libx264".into(), "-preset".into(), preset.into(),
            "-crf".into(), crf.to_string(), "-pix_fmt".into(), "yuv420p".into(),
            "-profile:v".into(), "high".into(), "-level".into(), "4.2".into(),
            "-movflags".into(), "+faststart".into(),
            "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-ar".into(), "48000".into(),
        ]),
        Container::Webm => args.extend([
            "-c:v".into(), "libvpx-vp9".into(), "-b:v".into(), "0".into(),
            "-crf".into(), (crf + 10).to_string(), "-deadline".into(), "good".into(),
            "-cpu-used".into(), "2".into(), "-pix_fmt".into(), "yuv420p".into(),
            "-c:a".into(), audio_for_webm(), "-b:a".into(), "160k".into(),
        ]),
    }
    args.push(out.to_string_lossy().to_string());

    Ok(Plan {
        exe,
        args,
        out,
        dur_s: g.len,
        temps: g.temps,
        graph_note: {
            let base = format!(
                "{} clips · {:.2} s muntats · {}x{} @ {:.0} fps",
                e.clips.len(),
                g.len,
                e.profile.width,
                e.profile.height,
                e.profile.fps
            );
            // Les degradacions (p. ex. sense `vidstab`) es veuen a la UI i al
            // registre del director, no només al log intern.
            if g.notes.is_empty() {
                base
            } else {
                format!("{base} · {}", g.notes.join(" · "))
            }
        },
    })
}

/// `libopus` no sempre està compilat; el codificador `opus` natiu sí (ffmpeg ≥ 5).
fn audio_for_webm() -> String {
    let out = detect::locate("ffmpeg")
        .map(|exe| {
            Command::new(exe)
                .args(["-hide_banner", "-encoders"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    if out.contains(" libopus") {
        "libopus".into()
    } else if out.lines().any(|l| l.split_whitespace().nth(1) == Some("opus")) {
        "opus".into()
    } else {
        "libvorbis".into()
    }
}

/// Callback de progrés: (fracció 0…1, text per a la UI).
pub type Progress<'a> = dyn FnMut(f64, String) + 'a;

/// Executa el pla: fa de progrès amb `time=` de stderr, i es pot aturar.
pub fn execute(
    plan: &Plan,
    cwd: &Path,
    progress: &mut Progress,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let t0 = Instant::now();
    let mut child = Command::new(&plan.exe)
        .current_dir(cwd)
        .args(&plan.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("no s'ha pogut arrencar el render: {e}"))?;
    let err = child.stderr.take().ok_or_else(|| anyhow!("sense sortida d'error"))?;
    let mut buf = std::io::BufReader::new(err);
    let mut tail: Vec<String> = Vec::new();
    let mut cancelled = false;

    loop {
        let mut line = String::new();
        match buf.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        let l = line.trim_end().to_string();
        let t = l.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(sec) = parse_time(t) {
            let f = (sec / plan.dur_s.max(0.001)).clamp(0.0, 0.999);
            let eta = if f > 0.015 {
                let el = t0.elapsed().as_secs_f64();
                format!(" · restant {:.0} s", (el / f - el).max(0.0))
            } else {
                String::new()
            };
            progress(f, format!("{:.0}%{eta}", f * 100.0));
        }
        if tail.len() < 400 {
            tail.push(l);
        }
        if is_cancelled() {
            cancelled = true;
            let _ = child.kill();
            break;
        }
    }
    let status = child
        .wait()
        .map_err(|e| anyhow!("l'espera del procés de render ha fallat: {e}"))?;
    if cancelled {
        let _ = std::fs::remove_file(&plan.out);
        return Err(anyhow!("render aturat per l'usuari"));
    }
    if !status.success() || !plan.out.exists() {
        return Err(anyhow!("el render ha fallat: {}", pick_error(&tail)));
    }
    Ok(())
}

fn parse_time(line: &str) -> Option<f64> {
    let i = line.find("time=")? + 5;
    let rest = &line[i..];
    let tok: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
    if tok.starts_with("N/A") {
        return None;
    }
    let mut it = tok.split(':');
    let h: f64 = it.next()?.parse().ok()?;
    let m: f64 = it.next()?.parse().ok()?;
    let s: f64 = it.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

/// Tria la línia més informativa d'un registre d'error d'ffmpeg.
fn pick_error(lines: &[String]) -> String {
    const KEYS: &[&str] = &[
        "Error", "error", "Invalid", "invalid", "No such", "not found", "Failed", "failed",
        "Cannot", "cannot", "Unrecognized", "Undefined", "Division by zero", "does not match",
    ];
    for l in lines.iter().rev() {
        if KEYS.iter().any(|k| l.contains(k)) {
            return l.chars().take(360).collect();
        }
    }
    lines
        .last()
        .map(|l| l.chars().take(360).collect())
        .unwrap_or_else(|| "cap missatge del codificador".into())
}

/// Esborra els temporals del pla (`.ass`, `.trf`).
pub fn cleanup(plan: &Plan) {
    for t in &plan.temps {
        let _ = std::fs::remove_file(t);
    }
}

/// Escriu el fitxer ASS amb tots els textos (títols, cartells, subtítols).
fn write_ass(edl: &Edl, path: &Path) -> Result<()> {
    let w = edl.profile.width;
    let h = edl.profile.height;
    let scale = (h as f64 / 1080.0).clamp(0.3, 3.0);
    let mut s = String::new();
    s.push_str("[Script Info]\nScriptType: v4.00+\n");
    s.push_str(&format!("PlayResX: {w}\nPlayResY: {h}\n"));
    s.push_str("WrapStyle: 0\nScaledBorderAndShadow: yes\n\n[V4+ Styles]\n");
    s.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");

    let mut written: Vec<String> = Vec::new();
    for t in &edl.texts {
        let name = style_name(t);
        if written.contains(&name) {
            continue;
        }
        written.push(name.clone());
        let base = (t.size * scale).clamp(14.0, 300.0) as u32;
        let align = match t.pos.as_str() {
            "top" => 8,
            "center" => 5,
            _ => 2,
        };
        let mv = match (align, t.kind.as_str()) {
            (8, _) => (h as f64 * 0.05) as u32,
            (5, _) => 0,
            (_, "lower") => (h as f64 * 0.17) as u32,
            _ => (h as f64 * 0.045) as u32,
        };
        let outline = (base as f64 * 0.05).clamp(1.0, 9.0) as u32;
        let shadow = if t.kind == "title" { (base as f64 * 0.08) as u32 } else { 2 };
        let bold = if t.kind == "subtitle" { 0 } else { 1 };
        s.push_str(&format!(
            "Style: {name},DejaVu Sans,{base},&H00FFFFFF,&H00D8D8D8,&H000A0A0A,&H80000000,{bold},0,0,0,100,100,0,0,1,{outline},{shadow},{align},60,60,{mv},1\n"
        ));
    }
    s.push_str("\n[Events]\n");
    s.push_str("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    for t in &edl.texts {
        s.push_str(&format!(
            "Dialogue: 0,{},{},{},,0,0,0,,{}\n",
            ass_time(t.at_s),
            ass_time(t.at_s + t.dur_s),
            style_name(t),
            ass_escape(&t.text)
        ));
    }
    std::fs::write(path, s).map_err(|e| anyhow!("no s'han pogut escriure els textos: {e}"))
}

fn style_name(t: &TextItem) -> String {
    let pos = match t.pos.as_str() {
        "top" => "T",
        "center" => "C",
        _ => "B",
    };
    let kind = match t.kind.as_str() {
        "title" => "titol",
        "lower" => "cartell",
        _ => "sub",
    };
    format!("{kind}{pos}{:0}", (t.size.round() as u32).clamp(12, 220))
}

fn ass_time(sec: f64) -> String {
    let total_cs = (sec.max(0.0) * 100.0).round() as u64;
    format!(
        "{}:{:02}:{:02}.{:02}",
        total_cs / 360000,
        (total_cs / 6000) % 60,
        (total_cs / 100) % 60,
        total_cs % 100
    )
}

/// ASS no accepta claus ni barres invertides literals; els saltem per a
/// caràcters visualment equivalents.
fn ass_escape(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    for c in t.chars() {
        match c {
            '{' => out.push('('),
            '}' => out.push(')'),
            '\\' => out.push('/'),
            '\n' | '\r' => out.push_str("\\N"),
            _ => out.push(c),
        }
    }
    out
}

/// Comprova el render contra l'EDL: durada, sonoritat, plans morts i congelats.
pub fn verify(out: &Path, edl: &Edl) -> Result<serde_json::Value> {
    let s = ff::probe(&out.to_string_lossy())?;
    let want = timeline_len(edl);
    let drift = s.duration - want;
    let mut notes: Vec<String> = Vec::new();
    if want > 0.5 && drift.abs() > (want * 0.06).max(1.5) {
        notes.push(format!(
            "la durada final ({:.2} s) es desvia del que prometia l'EDL ({:.2} s)",
            s.duration, want
        ));
    }
    let loud = ff::loudness(&out.to_string_lossy()).ok();
    if let Some(l) = &loud {
        if edl.normalize_audio && (l.input_i - edl.target_lufs).abs() > 3.0 {
            notes.push(format!(
                "sonoritat {:.1} LUFS, allunyada de l'objectiu de {:.1}",
                l.input_i, edl.target_lufs
            ));
        }
        if l.input_tp > -1.0 {
            notes.push(format!("pic de {:.1} dBTP: pot saturar en alguns reproduceors", l.input_tp));
        }
    }
    let st = ff::stats(&out.to_string_lossy(), 12).unwrap_or_default();
    let dead = st.iter().filter(|f| f.black_ratio > 0.86).count();
    if dead > 1 {
        notes.push(format!("{dead} mostres gairebé totalment negres al resultat"));
    }
    let frozen = st.iter().skip(1).filter(|f| f.motion < 0.004).count();
    if frozen >= 3 {
        notes.push(format!("{frozen} trams congelats (sense moviment) al resultat"));
    }
    if !s.has_audio && edl.clips.iter().any(|c| !c.mute) {
        notes.push("el resultat no té cap pista d'àudio".into());
    }
    Ok(serde_json::json!({
        "duration": s.duration,
        "expected": want,
        "driftS": drift,
        "width": s.width,
        "height": s.height,
        "fps": s.fps,
        "hasAudio": s.has_audio,
        "vcodec": s.vcodec,
        "acodec": s.acodec,
        "lufs": loud.as_ref().map(|l| l.input_i),
        "truePeak": loud.as_ref().map(|l| l.input_tp),
        "blackSamples": dead,
        "frozenSamples": frozen,
        "ok": notes.is_empty(),
        "notes": notes,
        "bytes": s.size_bytes,
    }))
}

// ── Proves ──────────────────────────────────────────────────────────────────
// proven l'ARITMÈTICA del muntatge —la que decideix quant dura cada clip i com
// es munta el graf—, no `ffmpeg`. Per això no necessiten cap origen ni cap
// binari: corren a `cargo test --lib` i a la porta de qualitat de la CI.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::edl::Transition;

    fn clip(in_s: f64, out_s: f64, speed: f64) -> Clip {
        Clip { in_s, out_s, speed, ..Clip::default() }
    }

    fn trans(dur_s: f64) -> Option<Transition> {
        Some(Transition { kind: "fade".into(), dur_s })
    }

    #[test]
    fn raw_len_retaila_a_lorigen() {
        // El que demana l'EDL (5…25) és més llarg del que dura l'arxiu (12 s).
        assert_eq!(raw_len(&clip(5.0, 25.0, 1.0), 12.0), (5.0, 7.0));
        // Ni que comenci fora de l'origen: l'entrada es queda dins i mai no
        // queda una durada zero (el mínim útil són 0,04 s).
        let (in_s, raw) = raw_len(&clip(5.0, 25.0, 1.0), 3.0);
        assert_eq!(in_s, 3.0);
        assert!((raw - 0.04).abs() < 1e-9);
        // Un tall d'un fotograma també té durada mínima.
        assert!((raw_len(&clip(1.0, 1.0, 1.0), 10.0).1 - 0.04).abs() < 1e-9);
    }

    #[test]
    fn speed_of_filtra_velocitats_fora_de_rang() {
        assert_eq!(speed_of(&clip(0.0, 4.0, 1.0)), 1.0);
        assert_eq!(speed_of(&clip(0.0, 4.0, 2.0)), 2.0);
        assert_eq!(speed_of(&clip(0.0, 4.0, 0.5)), 0.5);
        assert_eq!(speed_of(&clip(0.0, 4.0, 0.0)), 1.0);
        assert_eq!(speed_of(&clip(0.0, 4.0, 0.2)), 1.0);
        assert_eq!(speed_of(&clip(0.0, 4.0, 9.0)), 1.0);
        // La que abans passava i trencava el render:
        assert_eq!(speed_of(&clip(0.0, 4.0, -2.0)), 1.0);
    }

    #[test]
    fn durada_dun_clip_amb_velocitat() {
        // A 2x, 10 s d'origen fan 5 s de línia de temps.
        let (i, raw) = raw_len(&clip(0.0, 10.0, 2.0), 10.0);
        assert_eq!((raw / speed_of(&clip(0.0, 10.0, 2.0)), i), (5.0, 0.0));
        // Càmera lenta: 10 s a 0,5x en duren 20.
        let c = clip(0.0, 10.0, 0.5);
        assert!((raw_len(&c, 30.0).1 / speed_of(&c) - 20.0).abs() < 1e-9);
        // I l'origen mana: dels 40 s demanats només n'hi ha 10 → 5 s a 2x.
        let l = clip(0.0, 40.0, 2.0);
        assert!((raw_len(&l, 10.0).1 / speed_of(&l) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn overlap_no_passa_de_mitja_cada_part() {
        let mut prev = clip(0.0, 4.0, 1.0);
        prev.transition = trans(3.0);
        // 3 s demanats, però el clip del costat en fa 2 → només se'n pot 1.
        assert_eq!(overlap(&prev, 4.0, 2.0), 1.0);
        // Sense transició la línia de temps no es menja res.
        assert_eq!(overlap(&clip(0.0, 4.0, 1.0), 4.0, 4.0), 0.0);
    }

    #[test]
    fn timeline_len_descompta_les_transicions() {
        let mut e = Edl::default();
        let mut a = clip(0.0, 4.0, 1.0);
        a.transition = trans(1.0);
        e.clips = vec![a, clip(0.0, 4.0, 1.0)];
        assert!((timeline_len(&e) - 7.0).abs() < 1e-9);
        // Els mateixos plans a 2x: 2 s + 2 s menys el solapament d'1 s.
        let mut b = clip(0.0, 4.0, 2.0);
        b.transition = trans(1.0);
        e.clips = vec![b, clip(0.0, 4.0, 2.0)];
        assert!((timeline_len(&e) - 3.0).abs() < 1e-9);
        // Una EDL buida fa zero, mai un nombre negatiu.
        e.clips.clear();
        assert_eq!(timeline_len(&e), 0.0);
    }

    #[test]
    fn parse_time_llegeix_el_rellotge_dffmpeg() {
        let l = "frame=  123 fps= 24 q=28.0 size=     512KiB time=00:00:02.50 bitrate=1677.7kbits/s speed=1.02x";
        assert_eq!(parse_time(l), Some(2.5));
        assert_eq!(parse_time("time=01:01:01.00"), Some(3661.0));
        assert_eq!(parse_time("time=N/A"), None);
        assert_eq!(parse_time("video:123kB"), None);
    }

    #[test]
    fn ass_time_i_ass_escape() {
        assert_eq!(ass_time(0.0), "0:00:00.00");
        assert_eq!(ass_time(3661.5), "1:01:01.50");
        assert_eq!(ass_time(-4.0), "0:00:00.00");
        // L'ASS no accepta claus ni barres invertides literals.
        assert_eq!(ass_escape("a{b}c\\d\ne"), "a(b)c/d\\Ne");
    }

    #[test]
    fn edl_slug_of_deixa_noms_segurs() {
        assert_eq!(edl_slug_of("Pla Obert!! 01"), "pla-obert---01");
        assert_eq!(edl_slug_of("Muntatge_Final-v2"), "muntatge_final-v2");
        assert_eq!(edl_slug_of("   "), "clip");
    }

    #[test]
    fn container_accepta_el_que_escriu_la_ui() {
        assert_eq!(Container::parse(" MOV ").ext(), "mov");
        assert_eq!(Container::parse("webm").ext(), "webm");
        assert_eq!(Container::parse("avi").ext(), "mp4");
        assert_eq!(Container::parse("").ext(), "mp4");
    }

    #[test]
    fn command_line_entrella_el_que_té_espais() {
        let p = Plan {
            exe: "ffmpeg".into(),
            args: vec![
                "-y".into(),
                "-filter_complex".into(),
                "[0:v] scale=1920:1080 [v1]".into(),
                "-map".into(),
                "[v1];[0:a]".into(),
            ],
            out: "/tmp/a.mp4".into(),
            dur_s: 3.0,
            temps: vec![],
            graph_note: String::new(),
        };
        // El que porta espais o `;` va entre cometes; la resta, no.
        assert_eq!(
            p.command_line(),
            "ffmpeg -y -filter_complex \"[0:v] scale=1920:1080 [v1]\" -map \"[v1];[0:a]\""
        );
    }

    #[test]
    fn video_chain_posa_els_fusos_on_toca() {
        let e = Edl::default();
        let chain = video_chain(0, &clip(0.0, 6.0, 1.0), &e, 6.0, None, "v0");
        assert!(chain.starts_with("[0:v]fps=25.0000,scale=1920:1080"), "{chain}");
        assert!(chain.contains("setpts=PTS-STARTPTS"));
        assert!(chain.ends_with("format=yuv420p[v0]"));
        // El fús de sortida comença on acaba el pla, menys la seva durada.
        let mut f = clip(0.0, 6.0, 1.0);
        f.fade_out_s = 1.5;
        assert!(video_chain(2, &f, &e, 6.0, None, "v2").contains("fade=t=out:st=4.500:d=1.500"));
        // Un pla a 2x es diu amb `setpts`, no pas amb un altre nombre de fotogrames.
        assert!(video_chain(1, &clip(0.0, 6.0, 2.0), &e, 3.0, None, "v1").contains("setpts=PTS/2.0000"));
    }

    #[test]
    fn audio_chain_sense_so_no_perd_temps() {
        // Clip sense àudio: es genera una corrent REAL de la mateixa durada, si
        // no tot el que ve després es desencaixaria.
        let muted = audio_chain(1, &clip(0.0, 3.0, 1.0), 3.0, false, "a1");
        assert!(muted.starts_with("anullsrc=r=48000:cl=stereo,atrim=duration=3.0000"), "{muted}");
        assert!(muted.ends_with("[a1]"));
        // 4x: calen dos `atempo` perquè el filtre només accepta de 0,5 a 2.
        let fast = audio_chain(0, &clip(0.0, 4.0, 4.0), 1.0, true, "a0");
        assert!(fast.contains("atempo=2.0,atempo=2.0000"), "{fast}");
        // El guany només s'escriu si de veritat canvia alguna cosa.
        assert!(!audio_chain(0, &clip(0.0, 4.0, 1.0), 4.0, true, "a0").contains("volume="));
        let mut g = clip(0.0, 4.0, 1.0);
        g.gain = 1.8;
        assert!(audio_chain(0, &g, 4.0, true, "a0").contains("volume=1.800"));
    }

    #[test]
    fn resolve_i_rel_to_usen_larrel_del_projecte() {
        let root = Path::new("/tmp/proj");
        assert_eq!(resolve(root, "videos/pla1.mp4"), PathBuf::from("/tmp/proj/videos/pla1.mp4"));
        assert_eq!(resolve(root, "/directe/pla1.mp4"), PathBuf::from("/directe/pla1.mp4"));
        assert_eq!(rel_to(root, Path::new("/tmp/proj/videos/pla1.mp4")), "videos/pla1.mp4");
        // El que no és dins l'arrel es deixa intacte.
        assert_eq!(rel_to(root, Path::new("/una/altra.mp4")), "/una/altra.mp4");
    }

    // ── PROVA REAL D'EXTREM A EXTREM ──────────────────────────────────────
    // Les de dalt proven l'aritmètica; aquesta, el MOTOR: genera vídeos de
    // veritat, els escaneja, els munta, renderitza amb `ffmpeg`, verifica el
    // resultat i escriu el `.kdenlive`. Va marcada `#[ignore]` perquè la CI
    // no té `ffmpeg`; localment:
    //   cargo test --lib video::render::tests::muntatge_real -- --ignored

    /// Crea un origen sintètic real amb l'`ffmpeg` de la màquina.
    fn make_source(exe: &str, path: &Path, w: u32, h: u32, dur: f64, tone: bool) {
        // Totes les entrades VAN ABANS de qualsevol opció de sortida: si no,
        // l'ffmpeg llegeix `-c:v` com a part de la següent declaració d'input.
        let mut c = Command::new(exe);
        c.args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("testsrc2=size={w}x{h}:rate=25:duration={dur}"));
        if tone {
            c.args(["-f", "lavfi", "-i"])
                .arg(format!("sine=frequency=440:duration={dur}"));
        }
        c.args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"]);
        if tone {
            c.args(["-c:a", "aac", "-shortest"]);
        } else {
            c.arg("-an");
        }
        c.arg(path);
        let st = c.status().expect("arrencar l'ffmpeg que genera l'origen");
        assert!(st.success(), "no s'ha pogut generar {}", path.display());
    }

    #[test]
    #[ignore]
    fn muntatge_real_d_entrada_a_sortida() {
        use crate::video::edl::{Grade, Profile, TextItem};
        use crate::video::{commands, director, mlt};

        let tools = detect::detect();
        if !tools.ready {
            eprintln!("aquesta màquina no té ffmpeg/ffprobe: prova omesa");
            return;
        }
        let exe = tools.ffmpeg.path.clone().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let work = director::workdir(root);
        let outs = work.join("renders");

        // 1) MATERIAL: un horitzontal, un VERTICAL (prova l'`pad`) i un SENSE
        //    ÀUDIO (prova l'`anullsrc` que manté la línia de temps).
        make_source(&exe, &root.join("pla-a.mp4"), 1280, 720, 6.0, true);
        make_source(&exe, &root.join("pla-b.mp4"), 720, 1280, 5.0, true);
        make_source(&exe, &root.join("pla-c.mp4"), 1920, 1080, 4.0, false);

        // 2) ESCANEIG
        let found = director::sources_from_dir(root, &[]);
        assert_eq!(found.len(), 3, "l'escaneig hauria de trobar els 3 plans: {found:?}");
        let srcs = ff::probe_all(&found);
        assert!(srcs.iter().all(|s| s.readable), "tots els orígens s'haurien de poder llegir");
        assert!(srcs.iter().any(|s| s.height > s.width), "no s'ha detectat el pla vertical");
        assert!(srcs.iter().any(|s| !s.has_audio), "no s'ha detectat el pla sense so");

        // 3) MUNTE (transició, velocitat, fús, color, texts i normalització)
        let edl = Edl {
            name: "Prova real".into(),
            profile: Profile {
                width: 1280,
                height: 720,
                fps: 25.0,
                crf: 23,
                preset: "veryfast".into(),
            },
            clips: vec![
                Clip {
                    source: found[0].clone(),
                    label: "obert".into(),
                    in_s: 0.5,
                    out_s: 4.5,
                    fade_in_s: 0.4,
                    transition: Some(Transition { kind: "fade".into(), dur_s: 0.6 }),
                    grade: Some(Grade { saturation: 1.25, warmth: 0.4, ..Grade::default() }),
                    ..Clip::default()
                },
                Clip {
                    source: found[1].clone(),
                    label: "detall".into(),
                    in_s: 0.0,
                    out_s: 4.0,
                    speed: 2.0,
                    gain: 1.4,
                    transition: Some(Transition { kind: "dissolve".into(), dur_s: 0.5 }),
                    ..Clip::default()
                },
                Clip {
                    source: found[2].clone(),
                    label: "tancament".into(),
                    in_s: 0.0,
                    out_s: 3.0,
                    fade_out_s: 0.5,
                    ..Clip::default()
                },
            ],
            texts: vec![
                TextItem {
                    text: "Títol de la prova".into(),
                    at_s: 0.4,
                    dur_s: 1.6,
                    kind: "title".into(),
                    pos: "center".into(),
                    ..TextItem::default()
                },
                TextItem {
                    text: "Segon cartell, a baix".into(),
                    at_s: 3.0,
                    dur_s: 1.5,
                    ..TextItem::default()
                },
            ],
            target_lufs: -16.0,
            normalize_audio: true,
            note: "Prova d'extrem a extrem del motor de muntatge.".into(),
        };
        let rep = commands::video_edl_set(root.to_string_lossy().to_string(), edl)
            .expect("desar l'EDL del projecte");
        assert!(rep.dropped == 0, "s'hauria d'acceptar intacte: {:?}", rep.notes);
        assert!(
            rep.edl.clips.iter().all(|c| Path::new(&c.source).is_absolute()),
            "l'EDL desat ha de tenir rutes absolutes"
        );
        assert!(work.join("edl.json").exists(), "no s'ha desat l'EDL");

        // 4) RENDER FINAL REAL (dues passades: mesura i normalitza l'àudio)
        let measured = measure_audio(&rep.edl, root, &work);
        let pla = plan(&rep.edl, root, &work, &outs, false, Container::Mp4, measured.as_ref())
            .expect("preparar el pla del render");
        let volgut = timeline_len(&rep.edl);
        assert!((pla.dur_s - volgut).abs() < 0.01, "el pla no respecta l'EDL");
        let mut fites = 0usize;
        execute(&pla, root, &mut |_f, _t| fites += 1, &|| false).expect("el render final ha fallat");
        assert!(pla.out.exists(), "no hi ha fitxer de sortida");
        assert!(fites > 0, "el render no ha emès cap fita de progrés");

        let fin = ff::probe(&pla.out.to_string_lossy()).expect("probing del render");
        let drift = (fin.duration - volgut).abs();
        assert!(drift < 0.35, "durada esperada {volgut:.2} s, obtinguda {:.2} s", fin.duration);
        assert_eq!((fin.width, fin.height), (1280, 720), "el perfil de sortida no s'ha respectat");
        assert!(fin.has_audio, "el muntatge ha de tenir àudio encara que un origen no en tingui");

        // 5) VERIFICACIÓ (el que la IA llegeix per autocríticar-se)
        let v = verify(&pla.out, &rep.edl).expect("verificació del render");
        assert!(v["driftS"].as_f64().unwrap() < 0.35, "desencaix de durada: {v}");
        assert!(v["hasAudio"].as_bool().unwrap(), "sense àudio: {v}");
        assert_eq!(v["blackSamples"].as_u64().unwrap(), 0, "pla negre: {v}");
        let lufs = v["lufs"].as_f64();
        assert!(
            lufs.map(|x| (x - (-16.0)).abs() < 3.0).unwrap_or(false),
            "la normalització R128 no ha arribat a l'objectiu: {v}"
        );

        // 6) PROJECTE EDITABLE PER A L'HUMÀ
        let mlt_files = mlt::write_project(
            &rep.edl,
            root,
            &outs.join("prova.kdenlive"),
            tools.mlt_version().unwrap_or("7.0.0"),
        )
        .expect("escriure el projecte .kdenlive");
        assert!(Path::new(&mlt_files[0]).exists(), "no s'ha escrit el .kdenlive");
        assert!(
            std::fs::read_to_string(&mlt_files[0]).unwrap().contains("<mlt"),
            "l'XML del projecte no és MLT"
        );

        // 7) PREVISUALITZACIÓ: el mateix EDL, perfil reduït (640x360) i en un
        //    altre directori (com fa `spawn_render`), per no trepitjar el final.
        let prev_dir = work.join("analysis");
        let prev = plan(&rep.edl, root, &work, &prev_dir, true, Container::Mp4, None).unwrap();
        assert!(prev.graph_note.contains("640x360"), "nota del pla: {}", prev.graph_note);
        execute(&prev, root, &mut |_, _| {}, &|| false).expect("la previsualització ha fallat");
        assert!(prev.out.exists(), "no hi ha previsualització");
        assert_ne!(prev.out, pla.out, "la previsualització pisa el render final");
        let pfin = ff::probe(&prev.out.to_string_lossy()).unwrap();
        assert_eq!((pfin.width, pfin.height), (640, 360), "la previsualització no és reduïda");
        let mida_final = pla.out.metadata().map(|m| m.len()).unwrap_or(0);
        let mida_prev = prev.out.metadata().map(|m| m.len()).unwrap_or(0);
        assert!(mida_prev < mida_final, "la previsualització surt més pesada que el final");

        // 8) ESTABILITZACIÓ: si la `ffmpeg` no porta `vidstab` (el paquet de
        //    Kdenlive, no) el render HA de sortir igualment, degradat i avisat.
        let mut stab = rep.edl.clone();
        for cl in stab.clips.iter_mut() {
            cl.grade = Some(Grade { stabilize: true, ..Grade::default() });
        }
        let sp = plan(&stab, root, &work, &work.join("stab"), false, Container::Mp4, None).unwrap();
        if !has_filter(&exe, "vidstabtransform") {
            assert!(sp.graph_note.contains("estabilització omesa"), "nota: {}", sp.graph_note);
        }
        execute(&sp, root, &mut |_, _| {}, &|| false)
            .expect("el render no pot fallar per un filtre opcional absent");

        // 9) CANCEL·LACIÓ: un render aturat no deixa fitxers a mitges.
        let mut llarg = rep.edl.clone();
        llarg.name = "Per cancel·lar".into();
        let p9 = plan(&llarg, root, &work, &work.join("cancel"), false, Container::Mp4, None).unwrap();
        let r = execute(&p9, root, &mut |_, _| {}, &|| true);
        assert!(r.is_err(), "un render cancel·lat ha de donar error");
        assert!(!p9.out.exists(), "el render cancel·lat ha deixat fitxer a mitges");
    }
}
