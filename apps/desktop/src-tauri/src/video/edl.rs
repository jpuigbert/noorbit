//! Model de dades del muntatge (EDL: *edit decision list*).
//!
//! L'EDL és l'única font de veritat: a partir d'ella es genera el graf de
//! filtres d'`ffmpeg` (el render autoritzat), el projecte `.kdenlive`/MLT
//! (perquè l'humà el poleixi a la GUI) i el resum que veu la UI. La IA no
//! escriu ordres ffmpeg — escriu EDL en JSON, que és validable i revisable.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Perfil de sortida.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// Qualitat final (25 = web lleugera, 20 = nitidesa, 16 = arxiu).
    pub crf: u32,
    /// `ultrafast`…`slow`. `veryfast` per a previsualitzacions.
    pub preset: String,
}

impl Default for Profile {
    fn default() -> Self {
        Self { width: 1920, height: 1080, fps: 25.0, crf: 20, preset: "medium".into() }
    }
}

impl Profile {
    /// Versió reduïda per previsualitzar de pressa (mateix temps, menys píxels).
    pub fn preview(&self) -> Profile {
        Profile {
            width: 640,
            height: 360,
            fps: self.fps,
            crf: 30,
            preset: "ultrafast".into(),
        }
    }
}

/// Transició cap al clip SEGÜENT. `kind` és un nom vàlid d'`xfade`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Transition {
    pub kind: String,
    pub dur_s: f64,
}

impl Default for Transition {
    fn default() -> Self {
        Self { kind: "fade".into(), dur_s: 0.5 }
    }
}

/// Correcció de color lleugera per clip (valors neutres = sense efecte).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Grade {
    /// -0,2…0,2 (brillantor de `eq`).
    pub brightness: f64,
    /// 0,5…2 (contrast de `eq`).
    pub contrast: f64,
    /// 0…2 (saturació de `eq`).
    pub saturation: f64,
    /// -1 (fred)…1 (càlid): desplaçament de balances de `colorbalance`.
    pub warmth: f64,
    /// Estabilització (recorta un 8% i suavitza): `true` | `false`.
    pub stabilize: bool,
}

/// Un tall d'un fitxer origen.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Clip {
    /// Camí absolut o relatiu al projecte.
    pub source: String,
    /// Nom curt per a la UI i per a la IA (p. ex. «pla obert del dron»).
    pub label: String,
    pub in_s: f64,
    pub out_s: f64,
    /// 1,0 normal; 2,0 = càmera ràpida; 0,5 = càmera lenta.
    pub speed: f64,
    /// Guany lineal de l'àudio d'aquest clip (1,0 = sense canvis).
    pub gain: f64,
    pub fade_in_s: f64,
    pub fade_out_s: f64,
    /// Silenciar el so original (per posar-hi música a sobre, p. ex.).
    pub mute: bool,
    pub transition: Option<Transition>,
    pub grade: Option<Grade>,
}

impl Default for Clip {
    fn default() -> Self {
        Self {
            source: String::new(),
            label: String::new(),
            in_s: 0.0,
            out_s: 0.0,
            speed: 1.0,
            gain: 1.0,
            fade_in_s: 0.0,
            fade_out_s: 0.0,
            mute: false,
            transition: None,
            grade: None,
        }
    }
}

impl Clip {
    /// Durada real del clip un cop aplicat `speed`.
    pub fn len_s(&self) -> f64 {
        let raw = (self.out_s - self.in_s).max(0.04);
        let sp = if self.speed.abs() < 0.1 { 1.0 } else { self.speed };
        raw / sp
    }
}

/// Text en pantalla: títol, subtítol o cartell inferior.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextItem {
    pub text: String,
    pub at_s: f64,
    pub dur_s: f64,
    /// Mida en punts relativa a 1080 (s'escala amb el perfil).
    pub size: f64,
    /// `top` | `center` | `bottom`.
    pub pos: String,
    /// `title` (gran, centrat) | `subtitle` (petit, a baix) | `lower` ( terç inferior).
    pub kind: String,
}

impl Default for TextItem {
    fn default() -> Self {
        Self {
            text: String::new(),
            at_s: 0.0,
            dur_s: 2.5,
            size: 48.0,
            pos: "bottom".into(),
            kind: "subtitle".into(),
        }
    }
}

/// El muntatge complet.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Edl {
    pub name: String,
    pub profile: Profile,
    pub clips: Vec<Clip>,
    pub texts: Vec<TextItem>,
    /// Objectiu de sonoritat R128 per al mestress (streaming: -16, broadcast: -23).
    pub target_lufs: f64,
    /// Si `false`, es deixa l'àudio tal com ve dels orígens.
    pub normalize_audio: bool,
    /// Justificació creativa del muntatge (el que la IA escriu per explicar-se).
    pub note: String,
}

/// Nom de fitxer segur a partir d'un text lliure (mateixa regla que `Edl::slug`).
pub fn edl_slug_of(raw: &str) -> String {
    let clean: String = raw
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let clean = clean.trim_matches('_').to_string();
    if clean.is_empty() {
        "muntatge".into()
    } else {
        clean.chars().take(48).collect()
    }
}

impl Edl {
    pub fn has_clips(&self) -> bool {
        !self.clips.is_empty()
    }

    /// Durada total: suma de clips menys els solapaments de les transicions.
    pub fn duration_s(&self) -> f64 {
        let mut total = 0.0f64;
        for (i, c) in self.clips.iter().enumerate() {
            if i > 0 {
                total -= prev_overlap(&self.clips[i - 1]);
            }
            total += c.len_s();
        }
        total.max(0.0)
    }

    /// Nom de fitxer segur per al projecte i el render.
    pub fn slug(&self) -> String {
        edl_slug_of(&self.name)
    }

    /// El que la UI mostra com a descripció ràpida del pla.
    pub fn summary_line(&self) -> String {
        format!(
            "{} · {} · {:.1} s · {} clips",
            self.name,
            self.resolution_label(),
            self.duration_s(),
            self.clips.len()
        )
    }

    pub fn resolution_label(&self) -> String {
        format!("{}x{} @ {:?}", self.profile.width, self.profile.height, self.profile.fps)
    }
}

/// Solapament que la transició del clip `i-1` menja a la línia de temps.
fn prev_overlap(prev: &Clip) -> f64 {
    match &prev.transition {
        Some(t) => t.dur_s.clamp(0.0, prev.len_s().min(4.0)),
        None => 0.0,
    }
}

/// Sèrie de transicions vàlides (subconjunt útil d'`xfade`). La IA tria d'aquí;
/// qualsevol altre valor es degrada a `fade` per no trencar el render.
pub const TRANSITIONS: &[&str] = &[
    "fade",
    "wipeleft",
    "wiperight",
    "wipeup",
    "wipedown",
    "slideleft",
    "slideright",
    "slideup",
    "slidedown",
    "circleopen",
    "circleclose",
    "radial",
    "smoothleft",
    "smoothright",
    "dissolve",
    "pixelize",
    "hblur",
    "fadegrays",
    "distance",
    "zoomin",
];

pub fn normalize_transition(kind: &str) -> String {
    let k = kind.trim().to_ascii_lowercase().replace([' ', '-'], "");
    if TRANSITIONS.contains(&k.as_str()) {
        k
    } else {
        "fade".into()
    }
}

/// Neteja de l'EDL que ve de la IA: durades absurdes, rutes buides, transicions
/// invàlides i texts fora de la línia de temps. retorna (clips_escapats, esmenes).
pub fn sanitize(edl: &mut Edl) -> (u32, Vec<String>) {
    let mut dropped = 0u32;
    let mut fixes = Vec::new();

    if edl.profile.width == 0 || edl.profile.height == 0 {
        edl.profile = Profile::default();
        fixes.push("perfil de sortida invàlid; s'ha posat 1920x1080@25".into());
    }
    if edl.profile.fps < 1.0 || edl.profile.fps > 120.0 {
        edl.profile.fps = 25.0;
        fixes.push("fps invals; s'ha posat 25".into());
    }
    edl.profile.crf = edl.profile.crf.clamp(10, 40);
    if edl.target_lufs.abs() < 1.0 {
        edl.target_lufs = -16.0;
    }

    edl.clips.retain(|c| {
        if c.source.trim().is_empty() {
            dropped += 1;
            return false;
        }
        // Les rutes relatives es resolen després contra l'arrel del projecte.
        !c.source.trim().starts_with("..") && c.out_s > c.in_s + 0.03
    });
    if dropped > 0 {
        fixes.push(format!("{dropped} clips sense durada útil s'han descartat"));
    }

    for c in edl.clips.iter_mut() {
        if c.speed.abs() < 0.25 || c.speed > 8.0 {
            c.speed = 1.0;
        }
        if c.gain <= 0.0 || c.gain > 4.0 {
            c.gain = 1.0;
        }
        let max_fade = c.len_s() * 0.45;
        c.fade_in_s = c.fade_in_s.clamp(0.0, max_fade);
        c.fade_out_s = c.fade_out_s.clamp(0.0, max_fade);
        if let Some(t) = c.transition.as_mut() {
            let k = normalize_transition(&t.kind);
            if k != t.kind {
                fixes.push(format!("transició «{}» no existent; s'ha fet servir «{k}»", t.kind));
                t.kind = k;
            }
            t.dur_s = t.dur_s.clamp(0.15, 3.0);
        }
    }

    // L'últim clip no pot tenir transició (no hi ha res al darrere).
    if let Some(last) = edl.clips.last_mut() {
        if last.transition.is_some() {
            last.transition = None;
            fixes.push("s'ha tret la transició de l'últim clip".into());
        }
    }

    // Textos: within la línia de temps i sense caràcters problemàtics.
    let dur = edl.duration_s();
    edl.texts.retain(|t| !t.text.trim().is_empty());
    for t in edl.texts.iter_mut() {
        t.dur_s = t.dur_s.clamp(0.6, 12.0);
        t.at_s = t.at_s.clamp(0.0, (dur - t.dur_s).max(0.0));
        if t.size < 12.0 || t.size > 220.0 {
            t.size = 48.0;
        }
        if t.kind != "title" && t.kind != "lower" {
            t.kind = "subtitle".into();
        }
        if t.pos != "top" && t.pos != "center" {
            t.pos = "bottom".into();
        }
    }
    if edl.clips.iter().any(|c| Path::new(&c.source).is_absolute()) {
        fixes.push("hi ha origins amb ruta absoluta; cal obrir-los des del projecte".into());
    }
    (dropped, fixes)
}

/// Fallback determinista: reparteix els orígens en clips igualitaris sense
/// IA. Serveix quan el model no respon o retorna un JSON inservible, perquè
/// l'usuari SEMPRE acabi amb un vídeo.
pub fn auto_edl(sources: &[crate::video::ff::Source], name: &str, target_s: f64) -> Edl {
    let mut edl = Edl {
        name: name.into(),
        target_lufs: -16.0,
        normalize_audio: true,
        note: "Muntatge automàtic sense anàlisi de IA: durades repartides per igual."
            .into(),
        ..Default::default()
    };
    if sources.is_empty() {
        return edl;
    }
    // Perfil: el de l'origen més gran (els altres s'escalen i s'hi acompanyen).
    let best = sources
        .iter()
        .max_by(|a, b| (a.width * a.height).cmp(&(b.width * b.height)))
        .cloned()
        .unwrap_or_default();
    edl.profile = Profile {
        width: best.width.max(320),
        height: best.height.max(180),
        fps: if best.fps > 1.0 && best.fps < 120.0 { best.fps } else { 25.0 },
        ..Profile::default()
    };

    let total: f64 = sources.iter().map(|s| s.duration).sum::<f64>();
    let want = if target_s > 0.5 { target_s.min(total.max(1.0)) } else { total.max(1.0) };
    let per = (want / sources.len() as f64).clamp(0.6, 30.0);

    for (i, s) in sources.iter().enumerate() {
        let d = per.min(s.duration.max(0.6));
        let start = ((s.duration - d) * 0.15).max(0.0);
        let mut clip = Clip {
            source: s.path.clone(),
            label: s.name.clone(),
            in_s: start,
            out_s: (start + d).min(s.duration),
            speed: 1.0,
            gain: 1.0,
            fade_in_s: 0.0,
            fade_out_s: 0.0,
            mute: !s.has_audio,
            transition: if i + 1 < sources.len() {
                Some(Transition { kind: "fade".into(), dur_s: 0.4 })
            } else {
                None
            },
            grade: None,
        };
        if i == 0 {
            clip.fade_in_s = 0.5;
        }
        if i + 1 == sources.len() {
            clip.fade_out_s = 0.8;
        }
        edl.clips.push(clip);
    }
    edl
}
