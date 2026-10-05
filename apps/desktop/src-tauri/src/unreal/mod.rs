//! Mòdul d'integració amb Unreal Engine 5.
//!
//! Funciona mitjançant:
//! - Python Editor Script Plugin (execució de codi Python dins l'editor)
//! - Remote Control API (HTTP a :30010 i WebSocket a :30020)
//! - RunUAT / BuildGraph per compilar i empaquetar

pub mod builder;
pub mod client;
pub mod commands;
pub mod launcher;
pub mod python;
pub mod remote_control;

pub use client::UnrealClient;
pub use launcher::LaunchReport;
