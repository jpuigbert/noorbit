//! Mode PORTÀTIL (USB / disc extern): consultes i activació.
//!
//! Quan l'aplicació corre des d'un volum extern (`/Volumes/…`) o troba el
//! marcador `.noorbit-portable` al seu costat, totes les dades (config.toml,
//! proveïdors, especialistes, skills, imatges generades…) es desen a una
//! carpeta `NoOrbitData/` al LLOS DE L'APLICACIÓ, no al Mac. `AppConfig::data_dir()`
//! ja resol això; ací exposem l'estat a la UI i la manera de migrar/activar.

use serde::Serialize;
use tauri::command;

/// Marcador de mode portàtil.
const MARKER: &str = ".noorbit-portable";

#[derive(Debug, Serialize)]
pub struct PortableStatus {
    /// En marxa ja en mode portàtil (dades a NoOrbitData/).
    pub portable: bool,
    /// Carpeta de dades en ús, si es pot resoldre.
    pub data_dir: Option<String>,
    /// Carpeta de dades estàndard del Mac (~(Library)/Application Support/no-orbit),
    /// per mostrar què es migraria.
    pub mac_dir: Option<String>,
    /// Arrel al costat de la qual s'escriuria NoOrbitData/ si s'activa.
    pub portable_root: Option<String>,
    /// Volums externs muntats, per mostrar suggeriments a la UI.
    pub volumes: Vec<String>,
    /// Cert si es pot activar (l'app no viu a un lloc de sistema — clàssicament
    /// sempre es pot escriure el marcador; la UI avisa si és a /Applications).
    pub activatable: bool,
    /// L'aplicació és al Mac (/Applications o carpeta d'usuari): activar-hi el
    /// mode portàtil posaria les dades al costat del binari, cosa normalment
    /// no desitjada — la UI ho mostra com a diferent de "còpia a l'USB".
    pub in_system_location: bool,
}

#[derive(Debug, Serialize)]
pub struct PortableMigrateReport {
    /// Destinació final on queden les dades (o l'app copiada).
    pub destination: Option<String>,
    /// Fitxers i carpetes copiats (noms relatius).
    pub copied: Vec<String>,
    /// Elements que s'han saltat per què ja existien a la destinació.
    pub skipped: Vec<String>,
    /// Elements exclosos a propòsit (carpetes de reserva de connectors, etc.).
    pub excluded: Vec<String>,
    /// Error, si n'hi ha hagut en mig (es pot haver copiat la resta igualment).
    pub error: Option<String>,
}

/// Escriu el marcador `.noorbit-portable` al COSTAT de l'aplicació (o del seu
/// paquet .app) i retorna allà on han anat a parar les dades.
fn marker_target() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let root = exe
        .ancestors()
        .find(|a| a.extension().map(|e| e == "app").unwrap_or(false))
        .map(|p| p.to_path_buf());
    root.or_else(|| exe.parent().map(|p| p.to_path_buf()))
}

/// Retorna una descripció completa de mode portàtil per a la UI.
#[command]
pub async fn portable_status() -> Result<PortableStatus, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let s = exe.to_string_lossy().to_string();
    let portable = crate::config::AppConfig::data_dir()
        .map(|d| {
            // Portàtil <=> la carpeta en ús != la del Mac.
            d != dirs::data_dir().map(|x| x.join("no-orbit")).unwrap_or_default()
        })
        .unwrap_or(false);
    let volumes = super::ai::list_external_volumes().await.unwrap_or_default();
    let portable_root = marker_target()
        .and_then(|root| root.parent().map(|p| p.join("NoOrbitData")))
        .map(|p| p.to_string_lossy().to_string());
    Ok(PortableStatus {
        portable,
        data_dir: crate::config::AppConfig::data_dir().map(|p| p.to_string_lossy().to_string()),
        mac_dir: dirs::data_dir().map(|d| d.join("no-orbit").to_string_lossy().to_string()),
        portable_root,
        volumes,
        activatable: marker_target().is_some(),
        in_system_location: s.starts_with("/Users/") || s.starts_with("/Applications/"),
    })
}

/// ACTIVA el mode portàtil: escriu el marcador i (opcionalment) migra les
/// dades existents del Mac cap a `NoOrbitData/` al costat de l'app.
#[command]
pub async fn portable_enable(migrate: bool) -> Result<PortableMigrateReport, String> {
    let base = marker_target().ok_or("No s'ha trobat la carpeta de l'aplicació")?;
    let marker = base.join(MARKER);
    if !marker.exists() {
        std::fs::write(&marker, b"portable")
            .map_err(|e| format!("No es pot escriure el marcador: {e}"))?;
    }
    if migrate {
        migrate_mac_to(&base)
    } else {
        Ok(PortableMigrateReport {
            destination: Some(base.join("NoOrbitData").to_string_lossy().to_string()),
            copied: vec![],
            skipped: vec![],
            excluded: vec![],
            error: None,
        })
    }
}

/// COPIA l'aplicació sencera a un volum extern (`/Volumes/...`), hi escriu el
/// marcador de mode portàtil i, si `migrate`, emporta la configuració i les
/// dades del Mac. Un cop acabat, desde el USB l'app ja funciona amb tot.
#[command]
pub async fn portable_copy_app_to(
    volume_path: String,
    migrate: bool,
) -> Result<PortableMigrateReport, String> {
    tokio::task::spawn_blocking(move || {
        let base = marker_target().ok_or("No s'ha trobat el paquet de l'aplicació (.app)")?;
        if base.extension().map(|e| e != "app").unwrap_or(true) {
            return Err(
                "Això només fa falta amb l'app instal·lada (paquet .app), no en desenvolupament."
                    .into(),
            );
        }
        let vroot = std::path::Path::new(&volume_path);
        if !vroot.is_dir() {
            return Err(format!("El volum no està disponible: {volume_path}"));
        }
        let dest = vroot.join(base.file_name().unwrap_or_default());
        copy_dir(&base, &dest)?;
        // Marcador a l'arrel del volum: encara si més tard muden l'app dins
        // una carpeta del disc, les dades es resolen al mateix lloc.
        std::fs::write(vroot.join(MARKER), b"portable")
            .map_err(|e| format!("No es pot escriure el marcador al volum: {e}"))?;
        // Si cal, emportem la configuració i dades del Mac al costat de l'app.
        let mut report = if migrate {
            migrate_mac_to(vroot).unwrap_or_else(|e|PortableMigrateReport {
                destination: None,
                copied: vec![],
                skipped: vec![],
                excluded: vec![],
                error: Some(e),
            })
        } else {
            PortableMigrateReport {
                destination: None,
                copied: vec![],
                skipped: vec![],
                excluded: vec![],
                error: None,
            }
        };
        report.destination = Some(dest.to_string_lossy().to_string());
        // L'app copiada va sempre al davant de la llista de copiats.
        report.copied.insert(
            0,
            base.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// DESACTIVA el mode portàtil esborrant el marcador. Les dades de
/// `NoOrbitData/` NO s'esborren (l'usuari decideix què en fa).
#[command]
pub async fn portable_disable() -> Result<(), String> {
    let base = marker_target().ok_or("No s'ha trobat la carpeta de l'aplicació")?;
    let marker = base.join(MARKER);
    if marker.exists() {
        std::fs::remove_file(&marker).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Migra el contingut estàndard del Mac (`Application Support/no-orbit/`)
/// a una destinació (la `NoOrbitData/` del costat de l'app). Còpia, no mou:
/// es pot esborrar l'original a mà quan ho confirmes tot.
fn migrate_mac_to(base: &std::path::Path) -> Result<PortableMigrateReport, String> {
    let mut report = PortableMigrateReport {
        destination: Some(base.join("NoOrbitData").to_string_lossy().to_string()),
        copied: vec![],
        skipped: vec![],
        excluded: vec![],
        error: None,
    };
    let src = dirs::data_dir().map(|d| d.join("no-orbit"));
    let Some(src) = src else { return Ok(report) };
    if !src.exists() {
        return Ok(report);
    }
    let dst = base.join("NoOrbitData");
    let _ = std::fs::create_dir_all(&dst);
    // `dsh/` s'EXCLUDE perquè és un desplegable regenerable de centenars de MB.
    let Ok(entries) = std::fs::read_dir(&src) else { return Ok(report) };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if name == "dsh" {
            report.excluded.push(name);
            continue;
        }
        let to = dst.join(&name);
        if to.exists() {
            report.skipped.push(name);
            continue;
        }
        match std::fs::copy(entry.path(), &to) {
            Ok(_) => report.copied.push(name),
            // carpetes: còpia recursiva
            Err(_) => match copy_dir(&entry.path(), &to) {
                Ok(()) => report.copied.push(name),
                Err(e) => {
                    report.error = Some(format!("{name}: {e}"));
                }
            },
        }
    }
    Ok(report)
}

/// Còpia recursiva d'una carpeta (named com cp -R, sense seguir symlinks).
fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
