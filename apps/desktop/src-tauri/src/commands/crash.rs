//! Ordres del backend per als INFORMES DE FALLIDA («crash»).
//!
//! L'informe reuneix la versió, el sistema, les darreres fallides del
//! backend i el final del registre `crash.log`. L'usuari el pot copiar d'un
//! clic i enviar-lo a l'autor sense haver de buscar res pel sistema.

use tauri::command;

use crate::config::AppConfig;
use crate::crash::{self, CrashReport};

/// Informe de fallides del backend (Markdown + dades resumides).
#[command]
pub fn crash_report() -> CrashReport {
    crash::build_report(AppConfig::data_dir().as_deref())
}

/// Ruta del registre de fallides (`crash.log`), si existeix la carpeta.
#[command]
pub fn crash_log_path() -> Option<String> {
    crash::log_path(AppConfig::data_dir().as_deref()).map(|p| p.to_string_lossy().to_string())
}
