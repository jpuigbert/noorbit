//! Mòdul d'integració amb Unreal Engine 5.
//!
//! Funciona mitjançant:
//! - Python Editor Script Plugin (execució de codi Python dins l'editor)
//! - Remote Control API (HTTP a :30010 i WebSocket a :30020)
//! - Model Context Protocol (servidor oficial UE 5.8+ a :8000) — agent local
//! - RunUAT / BuildGraph per compilar i empaquetar

pub mod agent;
pub mod builder;
pub mod client;
pub mod commands;
pub mod launcher;
pub mod mcp;
pub mod python;
pub mod remote_control;
pub mod snippets;

use anyhow::{anyhow, Result};

pub use client::UnrealClient;
pub use launcher::LaunchReport;

/// Guarda per a allò que s'envia a Unreal: ha de ser CODI Python, no una
/// frase. Sense aquesta comprovació la prosa arribava a l'interpretador de
/// l'editor, que responia un `SyntaxError` opac que la IA no sabia corregir
/// (i l'usuari veia «✅ canvis aplicats» sense que hagués canviat res).
/// Aprofita la mateixa heurística que el connector de Blender.
pub fn guard_python(code: &str) -> Result<()> {
    if crate::blender::looks_like_python(code) {
        return Ok(());
    }
    let shown: String = code.trim().chars().take(100).collect();
    Err(anyhow!(
        "«{}» NO és codi Python: no s'ha executat res a Unreal. Per editar \
l'escena d'Unreal cal un script d'Editor Scripting que comenci per «import \
unreal» (escriu l'script sencer dins d'un bloc ```python, sense prosa al \
seu voltant).",
        shown
    ))
}
