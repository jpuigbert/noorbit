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

/// Publica el tema al marketplace (endpoint natiu `tpb/v1/vendor/publish`)
/// amb autenticació bàsica (usuari + contrasenya d'aplicació).
#[command]
pub async fn theme_upload(
    state: State<'_, AppState>,
    zip: String,
    site: String,
    user: String,
    app_password: String,
    meta: crate::themes::ThemeMeta,
) -> Result<crate::themes::UploadResult, String> {
    let http = state.ai_manager.http_client();
    let p = PathBuf::from(&zip);
    if !p.is_file() {
        return Err(format!("No trobo el ZIP «{}».", zip));
    }
    crate::themes::upload_zip(&http, &p, &site, &user, &app_password, &meta)
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

/// Publica el tema amb la SESSIÓ del navegador (cookies del lloc).
#[command]
pub async fn theme_upload_session(
    app: AppHandle,
    zip: String,
    site: String,
    meta: crate::themes::ThemeMeta,
) -> Result<crate::themes::UploadResult, String> {
    let p = PathBuf::from(&zip);
    if !p.is_file() {
        return Err(format!("No trobo el ZIP «{}».", zip));
    }
    crate::themes::upload_with_session(&app, &p, &site, &meta)
        .await
        .map_err(|e| e.to_string())
}

/// Comprova les credencials (usuari + contrasenya d'aplicació) ABANS de
/// publicar: fa una petició autenticada a l'API REST de WordPress. Així
/// l'usuari sap abans de pujar el ZIP si el compte i la contrasenya són
/// correctes (i que el compte té permisos), en lloc de descobrir-ho
/// després d'esperar 2 minuts.
#[command]
pub async fn theme_check_auth(
    state: State<'_, AppState>,
    site: String,
    user: String,
    app_password: String,
) -> Result<String, String> {
    let http = state.ai_manager.http_client();
    let base = site.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("Indica l'adreça del lloc.".into());
    }
    if user.trim().is_empty() || app_password.trim().is_empty() {
        return Err("Fan falta l'usuari i la contrasenya d'aplicació.".into());
    }
    use base64::Engine as _;
    let token = base64::engine::general_purpose::STANDARD
        .encode(format!("{}:{}", user.trim(), app_password.trim()));
    let resp = http
        .get(format!("{}/wp-json/wp/v2/users/me", base))
        .header("Authorization", format!("Basic {}", token))
        .send()
        .await
        .map_err(|e| format!("No s'ha pogut contactar amb el lloc: {}", e))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let motiu = if status.as_u16() == 401 {
            "l'usuari o la contrasenya d'aplicació no són correctes (recorda: ha de ser una \
             CONTRASENYA D'APLICACIÓ de WordPress, no la contrasenya normal d'entrada, i l'usuari \
             és el nom d'usuari de WordPress, no el correu)."
                .to_string()
        } else if status.as_u16() == 403 {
            "el compte no té permisos per publicar en aquest lloc".to_string()
        } else {
            v.get("message")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| body.chars().take(200).collect::<String>())
        };
        return Err(format!("Connexió fallida ({}): {}", status, motiu));
    }
    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .or_else(|| v.get("username").and_then(|n| n.as_str()))
        .unwrap_or("usuari");
    Ok(format!("Connexió correcta: hola, {}! El compte té permisos d'API.", name))
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
