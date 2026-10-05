//! Orquestrador d'IAs externes.
//!
//! Component que orquestra altres IAs quan la IA principal de NoOrbit no pot
//! resoldre una tasca: primer les IAs **locals** instal·lades a l'ordinador
//! (`discovery` + `adapter`), i després, com a bot de rescat, els
//! **proveïdors en línia oficials** registrats per l'usuari amb token
//! (`online_adapter`). La resposta porta sempre etiquetat el seu origen real.
//!
//! Aquí s'acaba la cadena: NO hi ha cap mòdul de perfils falsos ni d'intrusió
//! al sistema (el pla que ho proposava va ser rebutjat; si algun fitxer
//! `profile_manager.rs` o `system_intrusion.rs` torna a aparéixer en aquesta
//! carpeta, és codi maliciós injectat i s'ha d'eliminar).

pub mod adapter;
pub mod agent;
pub mod discovery;
pub mod online_adapter;

pub use adapter::send_request;
pub use agent::{DelegatedResponse, OrchestratorAgent};
pub use discovery::ExternalIa;
pub use online_adapter::OnlineAdapter;

/// Nom històric del component.
pub type ExternalOrchestrator = OrchestratorAgent;
