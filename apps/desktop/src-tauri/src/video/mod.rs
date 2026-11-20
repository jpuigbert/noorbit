//! MUNTATGE de vídeo professional (Blender → Kdenlive/ffmpeg) amb IA.
//!
//! Submòduls:
//! - `detect`   · troba ffmpeg/ffprobe/melt/kdenlive (el bundle de Kdenlive).
//! - `ff`       · mesures reals: ffprobe, silencis, canvis de pla, LUFS,
//!                graella de fotogrames, estadística d'imatge.
//! - `edl`      · model de dades del muntatge (el que la IA escriu i valida).
//! - `render`   · graf `ffmpeg` (talls, `xfade`, fusos, texts, `loudnorm`).
//! - `mlt`      · exportació `.kdenlive` + `.srt` per polir-ho a la GUI.
//! - `director` · motor autopilotat: mira, dissenya, es critica i renderitza.
//! - `commands` · la interfície Tauri (el que crida la UI).

pub mod commands;
pub mod detect;
pub mod director;
pub mod edl;
pub mod ff;
pub mod mlt;
pub mod render;

pub use detect::VideoTools;
pub use edl::{edl_slug_of, Edl};
