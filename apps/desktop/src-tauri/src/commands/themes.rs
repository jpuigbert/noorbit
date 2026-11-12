//! Comandes del menú «PUJAR»: inspecciona, empaqueta i puja un tema WordPress
//! al marketplace (themes.thepuigberts.com).

use crate::AppState;
use std::path::PathBuf;
use tauri::{command, AppHandle, State};

/// Inspecciona la carpeta d'un tema WordPress i en retorna les comprovacions.
#[command]
pub async fn theme_inspect(dir: String) -> Result<crate::themes::ThemeInfo, String> {
    let p = PathBuf::from(&dir);
    if !p.is_dir() {
        return Err(format!("«{}» no és una carpeta.", dir));
    }
    Ok(crate::themes::inspect(&p))
}

/// Empaqueta el tema en un ZIP (amb una sola carpeta a dins, com demana
/// WordPress) i en retorna la ruta i el nom del tema.
#[command]
pub async fn theme_build_zip(dir: String) -> Result<(String, String), String> {
    let p = PathBuf::from(&dir);
    if !p.is_dir() {
        return Err(format!("«{}» no és una carpeta.", dir));
    }
    let info = crate::themes::inspect(&p);
    if !info.errors.is_empty() {
        return Err(format!(
            "El tema no està a punt: {}",
            info.errors.join(" ")
        ));
    }
    let out = crate::config::AppConfig::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("themes");
    let (zip, slug) = crate::themes::build_zip(&p, &out).map_err(|e| e.to_string())?;
    Ok((zip.to_string_lossy().to_string(), slug))
}

/// Puja el ZIP del tema al marketplace (API REST de WordPress) amb
/// autenticació bàsica (usuari + contrasenya d'aplicació).
#[command]
pub async fn theme_upload(
    state: State<'_, AppState>,
    zip: String,
    site: String,
    user: String,
    app_password: String,
) -> Result<crate::themes::UploadResult, String> {
    let http = state.ai_manager.http_client();
    let p = PathBuf::from(&zip);
    if !p.is_file() {
        return Err(format!("No trobo el ZIP «{}».", zip));
    }
    crate::themes::upload_zip(&http, &p, &site, &user, &app_password)
        .await
        .map_err(|e| e.to_string())
}

/// Obre la pàgina de LOGIN del marketplace al navegador intern de NoOrbit:
/// l'usuari hi entra amb el seu compte i la pujada es fa després amb aquesta
/// sessió (cookies del navegador, sense contrasenya d'aplicació).
#[command]
pub async fn theme_open_login(app: AppHandle, site: String) -> Result<String, String> {
    crate::themes::open_login(&app, &site).map_err(|e| e.to_string())
}

/// Puja el ZIP del tema amb la SESSIÓ del navegador (cookies del lloc).
#[command]
pub async fn theme_upload_session(
    app: AppHandle,
    zip: String,
    site: String,
) -> Result<crate::themes::UploadResult, String> {
    let p = PathBuf::from(&zip);
    if !p.is_file() {
        return Err(format!("No trobo el ZIP «{}».", zip));
    }
    crate::themes::upload_with_session(&app, &p, &site)
        .await
        .map_err(|e| e.to_string())
}

/// Obre la pàgina de vendes del marketplace per acabar la fitxa del tema.
#[command]
pub async fn theme_open_sales_page(site: String) -> Result<String, String> {
    let base = site.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("Indica l'adreça del lloc.".into());
    }
    let url = format!("{}/wp-admin/admin.php?tab=sales&selltab=my-upload", base);
    crate::commands::system::open_externally(url.clone()).await?;
    Ok(format!("He obert «Vendes ▸ Pujar Tema» a {}.", url))
}
