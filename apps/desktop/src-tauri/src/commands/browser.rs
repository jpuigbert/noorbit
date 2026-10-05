//! Comandes Tauri per al navegador web intern.
//!
//! La IA (i la UI) poden obrir una pàgina, llegir-ne el text ja renderitzat
//! i interactuar-hi amb JavaScript. Les pàgines que demanen una acció humana
//! (iniciar sessió, captcha) es mostren a la persona: mai se supla cap
//! identitat ni s'esquiva cap protecció.

use crate::browser::{self, BrowserStatus, ChatProbe, PageExtract, WebIa};
use tauri::{command, AppHandle};

/// Obre el navegador intern amb una URL. `hidden = true` l'obre en segon
/// pla (la IA el fa servir per llegir sense molestar; es pot mostrar després).
#[command]
pub async fn browser_open(
    app: AppHandle,
    url: String,
    hidden: Option<bool>,
) -> Result<(), String> {
    browser::open(&app, &url, hidden.unwrap_or(false)).map_err(|e| e.to_string())
}

/// Navega a una altra URL (obre la finestra si no existia).
#[command]
pub async fn browser_navigate(app: AppHandle, url: String) -> Result<(), String> {
    browser::navigate(&app, &url).map_err(|e| e.to_string())
}

/// Estat del navegador: obert, visible, URL i títol actuals.
#[command]
pub async fn browser_status(app: AppHandle) -> Result<BrowserStatus, String> {
    Ok(browser::status(&app))
}

/// Mostra o amaga la finestra del navegador.
#[command]
pub async fn browser_set_visible(app: AppHandle, show: bool) -> Result<(), String> {
    browser::set_visible(&app, show).map_err(|e| e.to_string())
}

/// Tanca la finestra del navegador.
#[command]
pub async fn browser_close(app: AppHandle) -> Result<(), String> {
    browser::close(&app).map_err(|e| e.to_string())
}

/// Llegeix el text renderitzat de la pàgina actual (URL + text net).
/// `wait_secs` (1..60, per defecte 12) és el temps màxim d'espera perquè
/// la pàgina acabe de pintar amb JavaScript.
#[command]
pub async fn browser_extract(app: AppHandle, wait_secs: Option<u64>) -> Result<PageExtract, String> {
    let secs = wait_secs.unwrap_or(12).clamp(1, 60);
    browser::extract_text(&app, std::time::Duration::from_secs(secs))
        .await
        .map_err(|e| e.to_string())
}

/// Executa JavaScript dins la finestra del navegador (la pàgina carregada).
/// Aïllat de l'aplicació: aquesta finestra no té permisos IPC ni accés a
/// fitxers de NoOrbit.
#[command]
pub async fn browser_eval(app: AppHandle, js: String) -> Result<(), String> {
    browser::eval_js(&app, &js).map_err(|e| e.to_string())
}

// ── Consulta manual a IAs web (el camí sense token) ─────────────────────
//
// Sense cap token d'API, el servei es fa com el faria qualsevol persona:
// s'obre el xat web oficial, l'usuari inicia sessió amb el SEU compte, la IA
// escriu la pregunta a la caixa (l'usuari ho veu tot) i es llegeix la
// resposta que la pàgina mostra en pantalla. Mai cookies, tokens ni sessió
// suplantada.

/// Llistat d'IAs web consultables (conegudes, amb el seu xat oficial).
#[command]
pub async fn web_ia_list() -> Result<Vec<WebIa>, String> {
    Ok(browser::WEB_IAS.to_vec())
}

/// Obre el navegador intern (VISIBLE) al xat web de la IA indicada perquè
/// l'usuari hi iniciï sessió amb el seu compte.
#[command]
pub async fn web_ia_open(app: AppHandle, name: String) -> Result<String, String> {
    let ia = browser::find_ia(&name).ok_or_else(|| {
        format!(
            "No conec cap IA web anomenada «{}». Puc obrir: {}",
            name,
            browser::WEB_IAS
                .iter()
                .map(|w| w.name)
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    browser::open(&app, ia.url, false).map_err(|e| e.to_string())?;
    Ok(format!(
        "He obert el xat web de {} ({}) — inicia-hi sessió amb el teu compte; \
         jo escriuré les preguntes a la seva caixa i llegiré el que mostra en \
         pantalla, com faries tu mateix.",
        ia.name, ia.url
    ))
}

/// Estat de la pàgina oberta: hi ha caixa d'escriure i cal sessió?
#[command]
pub async fn web_ia_status(app: AppHandle) -> Result<ChatProbe, String> {
    browser::chat_probe(&app).await.map_err(|e| e.to_string())
}

/// Escriu la pregunta al xat web de la IA (obrint-lo si cal) i retorna la
/// resposta llegida de la pantalla. `wait_secs` (10..600, per defecte 180)
/// és el temps màxim d'espera mentre la pàgina genera.
#[command]
pub async fn web_ia_ask(
    app: AppHandle,
    name: String,
    question: String,
    wait_secs: Option<u64>,
) -> Result<String, String> {
    let ia = browser::find_ia(&name)
        .ok_or_else(|| format!("No conec cap IA web anomenada «{}»", name))?;
    // Si el navegador no està obert en aquesta IA, s'obre i es deixa uns
    // segons perquè la pàgina carregui abans de sondejar res.
    let st = browser::status(&app);
    let host = ia.url.split("//").nth(1).unwrap_or(ia.url).split('/').next().unwrap_or(ia.url);
    if !st.open || !st.url.contains(host) {
        browser::open(&app, ia.url, false).map_err(|e| e.to_string())?;
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    } else if !st.visible {
        // La pregunta s'escriurà davant de l'usuari: que la finestra es vegi.
        browser::set_visible(&app, true).ok();
    }
    let probe = browser::chat_probe(&app)
        .await
        .map_err(|e| e.to_string())?;
    if probe.login_needed && !probe.has_input {
        return Err(format!(
            "{} demana iniciar sessió. La finestra és en pantalla: entra amb \
             EL TEU compte i torna-ho a demanar. (No toco credencials ni \
             sessions alienes.)",
            ia.name
        ));
    }
    let secs = wait_secs.unwrap_or(180).clamp(10, 600);
    let answer = browser::chat_ask(&app, &question, std::time::Duration::from_secs(secs))
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "Resposta obtinguda del xat web de {} ({}), amb el compte de l'usuari — \
         lectura del que la pàgina mostra en pantalla:\n\n{}",
        ia.name, ia.url, answer
    ))
}
