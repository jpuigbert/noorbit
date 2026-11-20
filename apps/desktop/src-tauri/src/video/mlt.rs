//! Exportador de projecte `.kdenlive` (XML MLT).
//!
//! Limit conscient i honest: les DISSOLUCIONS creuades (clip A i clip B
//! solapats) només rampen si l'XML imita exactament el que escriu Kdenlive,
//! que embolica cada transició en un tractor amb la seva pròpia durada. Ho
//! provem de tres maneres (tractor anidat, `luma` amb abast, `qtblend` amb
//! opacitat) i el resultat mesurat va ser sempre l'últim pla sense barreja.
//!
//! Per això:
//! - el **render autoritzat és `ffmpeg`** (`render.rs`), on `xfade` sí que
//!   barreja (verificat píxel a píxel);
//! - el **`.kdenlive` que es genera és un muntatge sequencial correcte amb
//!   foses a negre** (`autofade`, que sí que funciona sobre una llista de
//!   reproducció) i tots els plans, durades i retalls perquè l'usuari els
//!   obri a la GUI i hi pose manualment les transicions que vulga.
//!
//! Acompanyament: un `.srt` amb tots els textos, que Kdenlive importa com a
//! pista de subtítols sense dependre de productors de títols propis.

use anyhow::{anyhow, Result};
use std::path::Path;

use crate::video::edl::Edl;

/// Escapa text per anar dins d'un atribut o contingut XML.
fn x(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            _ if (c as u32) < 0x20 => o.push(' '),
            _ => o.push(c),
        }
    }
    o
}

/// Segons → frames del perfil.
pub fn f(t: f64, fps: f64) -> i64 {
    (t * fps).round() as i64
}

/// Estructura MLT completa a partir de l'EDL.
pub fn to_mlt(edl: &Edl, root: &Path, mlt_version: &str) -> Result<String> {
    if !edl.has_clips() {
        return Err(anyhow!("l'EDL no té cap clip per exportar"));
    }
    let fps = edl.profile.fps.max(1.0);
    let mut ids: Vec<String> = Vec::new();
    let mut producers = String::new();
    let mut bin = String::new();
    let mut entries = String::new();
    let mut total_frames = 0i64;

    for (i, c) in edl.clips.iter().enumerate() {
        let src = if Path::new(c.source.trim()).is_absolute() {
            c.source.trim().to_string()
        } else {
            root.join(c.source.trim()).to_string_lossy().to_string()
        };
        let s = crate::video::ff::probe(&src)
            .map_err(|e| anyhow!("el projecte no es pot construir: {e}"))?;
        let sf = f(s.duration, fps).max(2);
        let id = format!("clip{i}");
        ids.push(id.clone());

        producers.push_str(&format!(
            "  <chain id=\"{id}\" in=\"0\" out=\"{}\">\n",
            sf - 1
        ));
        producers.push_str(&format!(
            "    <property name=\"mlt_service\">avformat-novalidate</property>\n"
        ));
        producers.push_str(&format!(
            "    <property name=\"resource\">{}</property>\n",
            x(&src)
        ));
        producers.push_str(&format!(
            "    <property name=\"length\">{}</property>\n",
            sf
        ));
        producers.push_str("    <property name=\"video_index\">0</property>\n");
        if s.has_audio {
            producers.push_str("    <property name=\"audio_index\">1</property>\n");
        } else {
            producers.push_str("    <property name=\"audio_index\">-1</property>\n");
        }
        if c.fade_in_s > 0.01 || c.fade_out_s > 0.01 {
            // Foses locals al clip: en MLT això sí que és fidel (alpha 0→1).
            let len = f(c.len_s(), fps).max(2);
            if c.fade_in_s > 0.01 {
                let d = f(c.fade_in_s, fps).max(1);
                producers.push_str(&format!(
                    "    <filter id=\"fin{i}\">\n      <property name=\"mlt_service\">brightness</property>\n      <property name=\"alpha\">1</property>\n      <property name=\"level\">0=0;{d}=1</property>\n    </filter>\n    <filter id=\"vfin{i}\">\n      <property name=\"mlt_service\">volume</property>\n      <property name=\"level\">0=0%;{d}=100%</property>\n    </filter>\n"
                ));
            }
            if c.fade_out_s > 0.01 {
                let d = f(c.fade_out_s, fps).max(1);
                let st = (len - d).max(0);
                producers.push_str(&format!(
                    "    <filter id=\"fout{i}\">\n      <property name=\"mlt_service\">brightness</property>\n      <property name=\"alpha\">1</property>\n      <property name=\"level\">{st}=1;{len}=0</property>\n    </filter>\n    <filter id=\"vfout{i}\">\n      <property name=\"mlt_service\">volume</property>\n      <property name=\"level\">{st}=100%;{len}=0%</property>\n    </filter>\n"
                ));
            }
        }
        producers.push_str("  </chain>\n");

        bin.push_str(&format!(
            "    <entry producer=\"{id}\" in=\"0\" out=\"{}\"/>\n",
            sf - 1
        ));

        let in_s = c.in_s.clamp(0.0, s.duration);
        let a = f(in_s, fps);
        let b = (f(c.out_s.min(s.duration), fps) - 1).max(a);
        entries.push_str(&format!(
            "    <entry producer=\"{id}\" in=\"{a}\" out=\"{b}\"/>\n"
        ));
        total_frames += b - a + 1;
    }

    let fade_ms = ((edl
        .clips
        .first()
        .and_then(|c| c.transition.as_ref())
        .map(|t| t.dur_s)
        .unwrap_or(0.4)
        * 1000.0) as i64)
        .clamp(40, 1500);

    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" standalone=\"no\"?>\n");
    s.push_str(&format!(
        "<mlt LC_NUMERIC=\"C\" version=\"{}\" title=\"NoOrbit — {}\" producer=\"main_bin\" root=\"{}\">\n",
        x(mlt_version),
        x(&edl.name),
        x(&root.to_string_lossy())
    ));
    s.push_str(&format!(
        "  <profile description=\"noorbit_{}\" width=\"{}\" height=\"{}\" frame_rate_num=\"{}\" frame_rate_den=\"{}\" sample_aspect_num=\"1\" sample_aspect_den=\"1\" display_aspect_num=\"{}\" display_aspect_den=\"{}\" colorspace=\"709\"/>\n",
        edl.profile.width,
        edl.profile.width,
        edl.profile.height,
        (fps * 1000.0).round() as i64,
        1000,
        16,
        9
    ));
    s.push_str(&format!(
        "  <producer id=\"black\" in=\"0\" out=\"{}\">\n    <property name=\"mlt_service\">color</property>\n    <property name=\"resource\">#000000</property>\n  </producer>\n",
        (total_frames + f(30.0, fps)).max(2)
    ));
    s.push_str(&producers);
    // Cistell del projecte (el que Kdenlive mostra a la vista d'arranjaments).
    s.push_str("  <playlist id=\"main_bin\" title=\"NoOrbit\">\n    <property name=\"xml_retain\">1</property>\n");
    s.push_str(&bin);
    s.push_str("  </playlist>\n");
    s.push_str(&format!(
        "  <playlist id=\"background\">\n    <entry producer=\"black\" in=\"0\" out=\"{}\"/>\n  </playlist>\n",
        (total_frames - 1).max(0)
    ));
    // Pista única de vídeo + àudio amb foses automàtiques entre retalls.
    s.push_str("  <playlist id=\"VA0\">\n");
    s.push_str(&entries);
    if fade_ms >= 40 {
        s.push_str(&format!(
            "    <filter id=\"fades\">\n      <property name=\"mlt_service\">autofade</property>\n      <property name=\"fade_duration\">{fade_ms}</property>\n      <property name=\"fade_video\">1</property>\n      <property name=\"fade_audio\">1</property>\n      <property name=\"fade_color\">#000000</property>\n    </filter>\n"
        ));
    }
    s.push_str("  </playlist>\n");
    s.push_str(&format!(
        "  <tractor id=\"tractor0\" title=\"NoOrbit\" global_feed=\"1\" in=\"0\" out=\"{}\">\n    <track producer=\"background\"/>\n    <track producer=\"VA0\"/>\n    <transition id=\"mixAll\">\n      <property name=\"mlt_service\">mix</property>\n      <property name=\"a\">-1</property>\n      <property name=\"b\">-1</property>\n      <property name=\"start\">-1</property>\n      <property name=\"accepts_blanks\">1</property>\n    </transition>\n  </tractor>\n",
        (total_frames - 1).max(0)
    ));
    s.push_str("</mlt>\n");
    Ok(s)
}

/// Subtítols/títols en `.srt` (Kdenlive los importa com a pista de text).
pub fn to_srt(edl: &Edl) -> String {
    let mut items = edl.texts.clone();
    items.sort_by(|a, b| a.at_s.partial_cmp(&b.at_s).unwrap_or(std::cmp::Ordering::Equal));
    let mut s = String::new();
    for (i, t) in items.iter().enumerate() {
        let end = (t.at_s + t.dur_s.max(0.6)).max(t.at_s + 0.6);
        s.push_str(&format!("{}\n{} --> {}\n{}\n\n", i + 1, stamp(t.at_s), stamp(end), t.text.trim()));
    }
    s
}

fn stamp(sec: f64) -> String {
    let ms = (sec.max(0.0) * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}

/// Escriu el projecte i el seu `.srt` al costat (Kdenlive l'importa com a
/// pista de text).
pub fn write_project(edl: &Edl, root: &Path, mlt_path: &Path, mlt_version: &str) -> Result<Vec<String>> {
    let xml = to_mlt(edl, root, mlt_version)?;
    if let Some(p) = mlt_path.parent() {
        std::fs::create_dir_all(p).ok();
    }
    std::fs::write(mlt_path, xml).map_err(|e| anyhow!("no s'ha pogut escriure el projecte: {e}"))?;
    let mut out = vec![mlt_path.to_string_lossy().to_string()];
    if !edl.texts.is_empty() {
        let srt = mlt_path.with_extension("srt");
        std::fs::write(&srt, to_srt(edl))
            .map_err(|e| anyhow!("no s'han pogut escriure els subtítols: {e}"))?;
        out.push(srt.to_string_lossy().to_string());
    }
    Ok(out)
}
