//! Capa d'anàlisi: tot el que es pot MESURAR d'un fitxer de vídeo abans que
//! la IA prengui decisions. Són dades deterministes (ffmpeg/ffprobe), de
//! manera que el muntatge no es basa en impressions sinó en xifres.
//!
//! - `probe`  → contenidor, codecs, mida, fps, durada, àudio.
//! - `silences` → on no hi ha veu (per tallar pauses mortes).
//! - `scenes` → on hi ha canvi de pla (per tallar on toca, no a l'atzar).
//! - `loudness` → LUFS / true peak reals (R128) per normalitzar bé.
//! - `beats` → atacs d'energia aproximats (per sincronitzar el ritme amb la música).
//! - `contact_sheet` → una graella de fotogrames: permet que un model amb
//!   visió VEURI un vídeo sencer en una sola imatge.
//! - `frame_stats` → lluentor, saturació, negres, congelacions i moviment.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::video::detect;

/// Extensions de vídeo que considerem «origen» en un projecte.
pub const VIDEO_EXT: &[&str] = &[
    "mp4", "mov", "mkv", "webm", "avi", "m4v", "mts", "m2ts", "f4v", "glb", "gltf",
];

/// Dades d'un origen de vídeo.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Source {
    pub path: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration: f64,
    pub has_audio: bool,
    pub vcodec: String,
    pub acodec: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub size_bytes: u64,
    /// `true` si ffprobe l'ha pogut llegir (si no, el fitxer està malmès).
    pub readable: bool,
}

/// Mesura de sonoritat d'un track (sortida del primer pas de `loudnorm`).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loudness {
    /// LUFS integrats mesurats.
    pub input_i: f64,
    /// `true peak` en dBTP.
    pub input_tp: f64,
    /// Rang de sonoritat (dinàmica percebuda).
    pub input_lra: f64,
    pub input_thresh: f64,
    pub target_offset: f64,
}

/// Estadístiques d'un fotograma concret (o de diversos).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameStat {
    pub t: f64,
    pub luma: f64,
    pub saturation: f64,
    /// Fracció de píxels per sota del llindar de negre (pla mort).
    pub black_ratio: f64,
    /// Detall/activitat d'edges (proxy d'enfocament i de soroll).
    pub detail: f64,
    /// Diferència mitjana respecte al fotograma anterior (moviment).
    pub motion: f64,
}

/// Un silenci detectat.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Silence {
    pub start: f64,
    pub end: f64,
    pub dur: f64,
}

fn bin(name: &str) -> Result<String> {
    detect::locate(name)
        .ok_or_else(|| anyhow::anyhow!("NoOrbit no ha trobat «{name}»: instal·la Kdenlive o afegeix-lo al PATH"))
}

/// Executa un binari i retorna (stdout, stderr). No fa cap assumpció sobre el
/// codi de sortida: alguns filtres d'anàlisi escriuen dades i surten amb avís.
pub fn run(prog: &str, args: &[&str]) -> Result<(String, String)> {
    let out = Command::new(prog)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| anyhow!("no s'ha pogut executar {prog}: {e}"))?;
    let so = String::from_utf8_lossy(&out.stdout).to_string();
    let se = String::from_utf8_lossy(&out.stderr).to_string();
    Ok((so, se))
}

/// `ffprobe` en JSON d'un fitxer.
pub fn probe(path: &str) -> Result<Source> {
    let fp = bin("ffprobe")?;
    let (so, se) = run(
        &fp,
        &[
            "-v", "error", "-print_format", "json", "-show_format", "-show_streams", path,
        ],
    )?;
    if !Path::new(path).exists() {
        return Err(anyhow!("el fitxer no existeix: {path}"));
    }
    let v: serde_json::Value = match serde_json::from_str(&so) {
        Ok(v) => v,
        Err(_) => {
            return Err(anyhow!(
                "ffprobe no ha entès «{}»: {}",
                Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                se.lines().next().unwrap_or("sortida buida")
            ))
        }
    };

    let mut s = Source {
        path: path.into(),
        name: Path::new(path)
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "clip".into()),
        ..Default::default()
    };
    s.size_bytes = Path::new(path).metadata().map(|m| m.len()).unwrap_or(0);
    if let Some(d) = v.get("format").and_then(|f| f.get("duration")).and_then(|d| d.as_str()) {
        s.duration = d.parse().unwrap_or(0.0);
    }
    let streams = v.get("streams").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    for st in &streams {
        let t = st.get("codec_type").and_then(|x| x.as_str()).unwrap_or("");
        if t == "video" && s.width == 0 {
            s.width = st.get("width").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            s.height = st.get("height").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            s.vcodec = st.get("codec_name").and_then(|x| x.as_str()).unwrap_or("").into();
            // `r_frame_rate` ve com "25/1"; si és absurd usem `avg_frame_rate`.
            for key in ["avg_frame_rate", "r_frame_rate"] {
                if let Some(r) = st.get(key).and_then(|x| x.as_str()) {
                    if let Some(v) = parse_ratio(r) {
                        if v > 0.5 && v < 240.0 {
                            s.fps = v;
                            break;
                        }
                    }
                }
            }
            if s.duration == 0.0 {
                if let Some(n) = st.get("duration").and_then(|x| x.as_str()) {
                    s.duration = n.parse().unwrap_or(0.0);
                }
            }
        } else if t == "audio" && !s.has_audio {
            s.has_audio = true;
            s.acodec = st.get("codec_name").and_then(|x| x.as_str()).unwrap_or("").into();
            s.sample_rate = st
                .get("sample_rate")
                .and_then(|x| x.as_str().and_then(|v| v.parse::<u32>().ok()).or_else(|| x.as_u64().map(|v| v as u32)))
                .unwrap_or(48000);
            s.channels = st.get("channels").and_then(|x| x.as_u64()).unwrap_or(2) as u16;
        }
    }
    s.readable = s.width > 0 && s.duration > 0.0;
    if !s.readable {
        return Err(anyhow!(
            "«{}» no sembla un vídeo vàlid (largada {:.2} s, {}x{})",
            s.name,
            s.duration,
            s.width,
            s.height
        ));
    }
    Ok(s)
}

fn parse_ratio(r: &str) -> Option<f64> {
    let mut it = r.split('/');
    let n: f64 = it.next()?.parse().ok()?;
    let d: f64 = it.next().unwrap_or("1").parse().unwrap_or(1.0);
    if d.abs() < 1e-6 {
        None
    } else {
        Some(n / d)
    }
}

pub fn probe_all(paths: &[String]) -> Vec<Source> {
    let mut out = Vec::new();
    for p in paths {
        match probe(p) {
            Ok(s) => out.push(s),
            Err(_) => out.push(Source {
                path: p.clone(),
                name: Path::new(p)
                    .file_stem()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                readable: false,
                ..Default::default()
            }),
        }
    }
    out
}

/// Intervals de silenci. `noise_db` en dBFS (per a veu, -45…-35 va bé) i
/// `min_s` la pausa mínima que ens interessa.
pub fn silences(path: &str, noise_db: f64, min_s: f64) -> Result<Vec<Silence>> {
    let ff = bin("ffmpeg")?;
    let noise = format!("{noise_db:.0}dB");
    let dur = format!("{min_s:.2}");
    let (_so, se) = run(
        &ff,
        &[
            "-v", "info", "-i", path, "-af",
            &format!("silencedetect=noise={noise}:d={dur}:monitor=0"),
            "-f", "null", "-",
        ],
    )?;
    let mut v = Vec::new();
    let mut start: Option<f64> = None;
    for line in se.lines() {
        if let Some(x) = grab_f64(line, "silence_start:") {
            start = Some(x);
        } else if let Some(x) = grab_f64(line, "silence_end:") {
            let s0 = start.take().unwrap_or(0.0);
            v.push(Silence { start: s0, end: x, dur: (x - s0).max(0.0) });
        }
    }
    Ok(v)
}

fn grab_f64(line: &str, key: &str) -> Option<f64> {
    let i = line.find(key)? + key.len();
    let rest = line[i..].trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Temps dels canvis de pla (`select=gt(scene,x)`). `thresh` 0,2…0,6.
pub fn scenes(path: &str, thresh: f64, max: usize) -> Result<Vec<f64>> {
    let ff = bin("ffmpeg")?;
    let vf = format!("select='gt(scene\\,{thresh:.2})',metadata=print:file=-");
    let (so, _se) = run(&ff, &["-v", "error", "-i", path, "-vf", &vf, "-an", "-f", "null", "-"])?;
    let mut v = Vec::new();
    for line in so.lines() {
        if let Some(x) = grab_f64(line, "pts_time:") {
            v.push(x);
            if v.len() >= max {
                break;
            }
        }
    }
    Ok(v)
}

/// Mesura R128 en un pas (el que després s'alimenta a `loudnorm` passe 2).
pub fn loudness(path: &str) -> Result<Loudness> {
    let ff = bin("ffmpeg")?;
    let (_so, se) = run(
        &ff,
        &[
            "-v", "info", "-i", path, "-af",
            "loudnorm=I=-16:TP=-1.5:LRA=11:print_format=json", "-f", "null", "-",
        ],
    )?;
    parse_loudnorm_json(&se).ok_or_else(|| anyhow!("loudnorm no ha informat mesures"))
}

/// Extreure el bloc JSON que `loudnorm` imprimeix al final del registre.
/// El format real és multiaamb TAB i claus entre cometes:
///   `{\n\t"input_i" : "-21.80", … "target_offset" : "-0.00"\n}`
pub fn parse_loudnorm_json(s: &str) -> Option<Loudness> {
    let key = s.find("\"input_i\"")?;
    let open = s[..key].rfind('{')?;
    let close = s[key..].find('}')? + key + 1;
    let v: serde_json::Value = serde_json::from_str(&s[open..close]).ok()?;
    let g = |k: &str| {
        v.get(k)
            .map(|x| x.as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0))
            .unwrap_or(0.0)
    };
    let i = g("input_i");
    let tp = g("input_tp");
    if i == 0.0 && tp == 0.0 {
        // Àudio absolutament mut o illegible: no és una mesura útil.
        return None;
    }
    Some(Loudness {
        input_i: i,
        input_tp: tp,
        input_lra: g("input_lra"),
        input_thresh: g("input_thresh"),
        target_offset: g("target_offset"),
    })
}

/// Atacs d'energia aproximats (proxy de «ba塔s»). Decodifica a PCM mono 8 kHz
/// i mira on l'envolupant puja de cop: prou per a sincronitzacions de ritme.
pub fn beats(path: &str, max: usize) -> Result<Vec<f64>> {
    let ff = bin("ffmpeg")?;
    let out = Command::new(&ff)
        .args([
            "-v", "error", "-i", path, "-ac", "1", "-ar", "8000", "-f", "f32le", "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| anyhow!("no s'ha pogut llegir l'àudio: {e}"))?;
    if !out.status.success() && out.stdout.len() < 1024 {
        return Err(anyhow!("l'àudio no s'ha pogut decodificar"));
    }
    Ok(onsets(&out.stdout, 8000, max))
}

/// Detecció d'atacs per envolupant d'energia amb finestres de 23 ms.
fn onsets(pcm: &[u8], rate: usize, max: usize) -> Vec<f64> {
    let samples: Vec<f32> = pcm
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    if samples.len() < rate / 2 {
        return Vec::new();
    }
    let win = ((rate as f64 * 0.023).round() as usize).max(16);
    let mut env: Vec<f32> = Vec::with_capacity(samples.len() / win + 1);
    for w in samples.chunks_exact(win) {
        let e = w.iter().map(|s| (s * s) as f64).sum::<f64>() / win as f64;
        env.push((e + 1e-9).ln() as f32);
    }
    if env.len() < 8 {
        return Vec::new();
    }
    // Flux positiu de l'envolupant en escala logarítmica.
    let mut flux: Vec<f32> = Vec::with_capacity(env.len());
    for i in 1..env.len() {
        flux.push((env[i] - env[i - 1]).max(0.0));
    }
    let mean = flux.iter().sum::<f32>() / flux.len() as f32;
    let sd = (flux.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / flux.len() as f32).sqrt();
    let thr = mean + 1.4 * sd;
    let hop = win as f64 / rate as f64;
    let mut peaks = Vec::new();
    let mut since = 0usize;
    let gap = (0.18 / hop) as usize; // mínim 180 ms entre atacs
    for i in 1..flux.len() - 1 {
        since += 1;
        if flux[i] > thr && flux[i] >= flux[i - 1] && flux[i] > flux[i + 1] && since >= gap {
            peaks.push((i as f64 + 1.0) * hop);
            since = 0;
            if peaks.len() >= max {
                break;
            }
        }
    }
    peaks
}

/// Talla caràcters problemàtics d'un nom de fitxer de destí.
fn safe_stem(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let t = out.trim_matches('-').to_string();
    if t.is_empty() {
        "clip".into()
    } else {
        t.chars().take(36).collect()
    }
}

/// Extreu N fotogrames a `dir` (moments repartits per tot el clip). Retorna
/// els camins PNG creats.
pub fn frames_at(path: &str, dir: &Path, times: &[f64], width: u32) -> Result<Vec<PathBuf>> {
    let ff = bin("ffmpeg")?;
    std::fs::create_dir_all(dir).ok();
    let stem = safe_stem(&Path::new(path).file_stem().map(|s| s.to_string_lossy()).unwrap_or_default());
    let mut out = Vec::new();
    for (i, t) in times.iter().enumerate() {
        let p = dir.join(format!("{stem}-{i:03}.png"));
        let ts = format!("{t:.3}");
        let _ = run(
            &ff,
            &[
                "-v", "error", "-y", "-ss", &ts, "-i", path,
                "-vf", &format!("scale={width}:-2:flags=bicubic"),
                "-frames:v", "1",
                p.to_str().unwrap_or("x.png"),
            ],
        )?;
        if p.exists() {
            out.push(p);
        }
    }
    Ok(out)
}

/// Graella de fotogrames de tot el clip: una sola imatge que un model amb
/// visió pot llegir sencera. `cols x rows` cel·les.
pub fn contact_sheet(path: &str, out_png: &Path, cols: u32, rows: u32, cell_w: u32) -> Result<PathBuf> {
    let ff = bin("ffmpeg")?;
    let total = (cols * rows).max(1);
    let dur = probe(path).map(|s| s.duration).unwrap_or(0.0);
    if dur <= 0.0 {
        return Err(anyhow!("no es pot fer la graella: fitxer illegible"));
    }
    std::fs::create_dir_all(out_png.parent().unwrap_or(Path::new("."))).ok();
    // Mostreig a intervals regulars, saltant el primer fotograma (sovint negre).
    let every = (dur / (total as f64 + 1.0)).max(0.04);
    let vf = format!(
        "fps=1/{every:.4},scale={cell_w}:-2:flags=bicubic,tile={cols}x{rows}"
    );
    let _ = run(
        &ff,
        &[
            "-v", "error", "-y", "-i", path, "-vf", &vf, "-frames:v", "1",
            out_png.to_str().unwrap_or("sheet.png"),
        ],
    )?;
    if out_png.exists() {
        Ok(out_png.to_path_buf())
    } else {
        Err(anyhow!("ffmpeg no ha generat la graella"))
    }
}

/// Graella combinada de VÀRIOS orígens en una sola imatge: cada origen es
/// converteix en una franja de `per_clip` cel·les de la MATEIXA mida i després
/// es munten les franges horitzontalment. La IA pot veure tot el material en
/// un sol missatge, encara que els clips tinguin resolucions diferents.
pub fn multi_sheet(paths: &[String], out_png: &Path, per_clip: u32, cell_w: u32) -> Result<PathBuf> {
    let ff = bin("ffmpeg")?;
    std::fs::create_dir_all(out_png.parent().unwrap_or(Path::new("."))).ok();
    let cell_h = ((cell_w * 9 / 16) as i64).max(2).to_string();
    let cell_w = cell_w.max(2).to_string();
    let mut args: Vec<String> = vec!["-v".into(), "error".into(), "-y".into()];
    let mut filters = String::new();
    let mut labels: Vec<String> = Vec::new();
    let mut idx = 0usize;
    for p in paths {
        let src = match probe(p) {
            Ok(s) => s,
            Err(_) => continue,
        };
        args.extend(["-i".into(), p.clone()]);
        // Mostreig regular saltant els extrems (el primer fotograma sol ser negre).
        let every = (src.duration / (per_clip as f64 + 1.0)).max(0.04);
        let c = format!("c{idx}");
        filters.push_str(&format!(
            "[{idx}:v]fps=1/{every:.4},scale={cell_w}:{cell_h}:force_original_aspect_ratio=decrease:flags=bicubic,\
             pad={cell_w}:{cell_h}:(ow-iw)/2:(oh-ih)/2:color=0x202020,setsar=1[{c}];"
        ));
        labels.push(format!("[{c}]"));
        idx += 1;
        if idx >= 8 {
            break;
        }
    }
    if labels.is_empty() {
        return Err(anyhow!("cap origen llegible per a la graella"));
    }
    let n = labels.len();
    if n == 1 {
        filters.push_str("[c0]copy[out]");
    } else {
        filters.push_str(&format!("{}hstack=inputs={n}[out]", labels.join("")));
    }
    args.extend([
        "-filter_complex".into(),
        filters,
        "-map".into(),
        "[out]".into(),
        "-frames:v".into(),
        "1".into(),
        out_png.to_str().unwrap_or("multi.png").into(),
    ]);
    let argrefs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (_so, se) = run(&ff, &argrefs)?;
    if out_png.exists() {
        Ok(out_png.to_path_buf())
    } else {
        Err(anyhow!(
            "no s'ha pogut construir la graella múltiple: {}",
            se.lines().next().unwrap_or("")
        ))
    }
}

/// Estadístiques d'un sol fotograma (lluentor, saturació, negres, detall).
pub fn frame_stat(png: &Path, t: f64, prev_gray: Option<&[f32]>) -> Option<(FrameStat, Vec<f32>)> {
    let img = image::open(png).ok()?.to_rgb8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let mut gray = vec![0f32; (w * h) as usize];
    let mut luma = 0f64;
    let mut sat = 0f64;
    let mut black = 0u64;
    for (x, y, px) in img.enumerate_pixels() {
        let (r, g, b) = (px[0] as f64, px[1] as f64, px[2] as f64);
        let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        gray[(y * w + x) as usize] = l as f32 / 255.0;
        luma += l;
        let mx = r.max(g).max(b);
        let mn = r.min(g).min(b);
        sat += if mx > 1.0 { (mx - mn) / mx } else { 0.0 };
        if l < 12.0 {
            black += 1;
        }
    }
    let n = (w * h) as f64;
    // Detall: suma dels gradients vertical/horizontal normalitzada.
    let mut detail = 0f64;
    let mut motion = 0f64;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = (y * w + x) as usize;
            let gx = (gray[i + 1] - gray[i - 1]).abs() as f64;
            let gy = (gray[i + w as usize] - gray[i - w as usize]).abs() as f64;
            detail += gx + gy;
            if let Some(p) = prev_gray {
                if let Some(&pv) = p.get(i) {
                    motion += (gray[i] - pv).abs() as f64;
                }
            }
        }
    }
    let inner = ((w as u64 - 2) * (h as u64 - 2)).max(1) as f64;
    let st = FrameStat {
        t,
        luma: luma / n / 255.0,
        saturation: sat / n,
        black_ratio: black as f64 / n,
        detail: detail / inner / 2.0,
        motion: if prev_gray.is_some() { motion / inner } else { 0.0 },
    };
    Some((st, gray))
}

/// Sèrie d'estadístiques mostrejant el vídeo a intervals regulars.
pub fn stats(path: &str, samples: u32) -> Result<Vec<FrameStat>> {
    let src = probe(path)?;
    let dir = std::env::temp_dir().join(format!("noorbit-stats-{}", std::process::id()));
    let mut times = Vec::new();
    let n = samples.max(2) as usize;
    for i in 0..n {
        let t = src.duration * (i as f64 + 0.5) / n as f64;
        times.push(t.min((src.duration - 0.04).max(0.0)));
    }
    let pngs = frames_at(path, &dir, &times, 320)?;
    let mut out = Vec::new();
    let mut prev: Option<Vec<f32>> = None;
    for (i, p) in pngs.iter().enumerate() {
        if let Some((mut st, gray)) = frame_stat(p, times[i.min(times.len() - 1)], prev.as_deref()) {
            if i == 0 {
                st.motion = 0.0;
            }
            out.push(st);
            prev = Some(gray);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    if out.is_empty() {
        return Err(anyhow!("s'han tret fotogrames però cap no és llegible"));
    }
    Ok(out)
}

/// Diagnòstic compacte d'un origen, en text, per posar-lo al prompt de la IA.
pub fn describe(s: &Source, st: &[FrameStat], sc: &[f64], sil: &[Silence]) -> String {
    let dark = st.iter().filter(|f| f.luma < 0.10).count();
    let frozen = st.iter().skip(1).filter(|f| f.motion < 0.006).count();
    let avg_luma = if st.is_empty() { 0.0 } else { st.iter().map(|f| f.luma).sum::<f64>() / st.len() as f64 };
    let avg_sat = if st.is_empty() { 0.0 } else { st.iter().map(|f| f.saturation).sum::<f64>() / st.len() as f64 };
    let avg_mot = if st.len() < 2 { 0.0 } else { st[1..].iter().map(|f| f.motion).sum::<f64>() / (st.len() - 1) as f64 };
    let avg_det = if st.is_empty() { 0.0 } else { st.iter().map(|f| f.detail).sum::<f64>() / st.len() as f64 };
    let mut t = String::new();
    t.push_str(&format!(
        "· {} — {:.1} s, {}x{} @ {:.2} fps, so: {}.\n",
        s.name,
        s.duration,
        s.width,
        s.height,
        s.fps,
        if s.has_audio { "sí" } else { "no" }
    ));
    t.push_str(&format!(
        "  lluentor mitjana {:.2}, saturació {:.2}, detall {:.3}, moviment mitjà {:.3}\n",
        avg_luma, avg_sat, avg_det, avg_mot
    ));
    if dark > 0 {
        t.push_str(&format!("  ⚠ {} mostres gairebé negres (plans morts o fusos a negre)\n", dark));
    }
    if frozen > 0 {
        t.push_str(&format!("  ⚠ {} mostres congelades (sense moviment)\n", frozen));
    }
    if !sc.is_empty() {
        let v: Vec<String> = sc.iter().take(10).map(|x| format!("{x:.1}")).collect();
        t.push_str(&format!("  canvis de pla detectats: {}\n", v.join(", ")));
    }
    if !sil.is_empty() {
        let v: Vec<String> = sil.iter().take(8).map(|x| format!("{:.1}–{:.1}", x.start, x.end)).collect();
        t.push_str(&format!("  silencis: {}\n", v.join(", ")));
    }
    t
}

/// Llegeix un fitxer petit (per saber si realment s'ha escrit).
pub fn file_exists_nonempty(p: &Path) -> bool {
    p.metadata().map(|m| m.len() > 1024).unwrap_or(false)
}

/// Obre un fitxer binari (per a comprovacions ràpides de capçalera).
pub fn head_bytes(p: &Path, n: usize) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(p).ok()?;
    let mut buf = vec![0u8; n];
    let got = f.read(&mut buf).ok()?;
    buf.truncate(got);
    Some(buf)
}
