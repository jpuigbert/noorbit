//! Detecció d'ACTUALITZACIONS: consulta a GitHub si hi ha una versió nova.
//!
//! Les publicacions de NoOrbit viatgen com a *releases* del repositori públic
//! `jpuigbert/noorbit` (generades per .github/workflows/release.yml en pujar un
//! tag `v*`). L'API `/releases/latest` dona exactament l'última versió marcada
//! com a «Latest», amb els seus instal·ladors (dmg/exe/AppImage) com a assets.
//! La interfície pregunta ací al arrancar i des del panell «Informació»; si
//! la versió remota és més nova que la que s'està executant, ho avisa.

use serde::Serialize;
use tauri::command;

/// Repositori públic on es publiquen els releases.
const REPO: &str = "jpuigbert/noorbit";

/// Resultat de la comparació entre la versió en execució i l'última publicada.
#[derive(Serialize)]
pub struct UpdateInfo {
    /// Versió d'aquest NoOrbit que s'està executant.
    pub current: String,
    /// Versió «Latest» publicada a GitHub (sense la «v» del tag).
    pub latest: String,
    /// Cert només si `latest` és més nova que `current`.
    pub available: bool,
    /// Pàgina web del release (per a obrir-la amb el navegador).
    pub url: String,
    /// Títol del release, si en té.
    pub name: String,
    /// Data de publicació (ISO 8601).
    pub date: String,
    /// Enllaç de descàrrega de l'instal·ador adequat per a EST sistema
    /// (dmg a macOS, -setup.exe a Windows, AppImage a Linux), si existeix.
    pub asset: Option<String>,
}

/// Converteix «0.6.1» o «v0.6» en un tuple comparable (0,6,1)/(0,6,0).
/// Tot el que no sigui numèric (sufixos «-beta»…) s'ignora component a
/// component; així la comparació mai no falla amb tags estranys.
fn semver(s: &str) -> (u32, u32, u32) {
    let clean = s.trim().trim_start_matches('v').trim_start_matches('V');
    let mut parts = clean.split(|c| c == '.' || c == '-');
    let num = |p: Option<&str>| -> u32 {
        p.unwrap_or("0")
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    };
    (num(parts.next()), num(parts.next()), num(parts.next()))
}

/// Tria, among els assets del release, el descargable útil per al sistema actual.
fn pick_asset(assets: &[serde_json::Value]) -> Option<String> {
    // Preferències per sufix del nom del fitxer, en ordre de preferència.
    let prefs: &[&str] = if cfg!(target_os = "macos") {
        &["universal.dmg", ".dmg"]
    } else if cfg!(target_os = "windows") {
        &["-setup.exe", ".exe", ".msi"]
    } else {
        &[".AppImage", ".deb", ".rpm"]
    };
    for p in prefs {
        for a in assets {
            let name = a["name"].as_str().unwrap_or("");
            let url = a["browser_download_url"].as_str().unwrap_or("");
            // Els «.AppImage.tar.gz» són checksums auxiliars: no són per a
            // l'usuari final, els descartem.
            if name.ends_with(p) && !name.ends_with(".tar.gz") && !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

/// Pregunta a GitHub si hi ha una versió nova. És una crida de xarxa curta
/// (20 s de tall) i SENSE clau: l'API de releases públics és anònima.
/// Qualsevol error (sense internet, sense límit de rate…) retorna Err i la
/// interfície el mostra suaument, mai com a tall.
#[command]
pub async fn check_update() -> Result<UpdateInfo, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(20))
        // GitHub EXIGEIX un User-Agent; si no n'hi ha, respon 403.
        .user_agent(concat!("NoOrbit/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .send()
        .await
        .map_err(|e| format!("No s'ha pogut contactar amb GitHub: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("GitHub ha respost {} (la app pot estar sense connexió o sense releases publicats)", status.as_u16()));
    }

    let j: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let tag = j["tag_name"].as_str().unwrap_or("").to_string();
    if tag.is_empty() {
        return Err("La resposta de GitHub no inclou cap versió".into());
    }

    let current = env!("CARGO_PKG_VERSION").to_string();
    let available = semver(&tag) > semver(&current);

    Ok(UpdateInfo {
        asset: j["assets"].as_array().and_then(|a| pick_asset(a)),
        available,
        current,
        date: j["published_at"].as_str().unwrap_or("").to_string(),
        latest: tag.trim_start_matches('v').trim_start_matches('V').to_string(),
        name: j["name"].as_str().unwrap_or("").to_string(),
        url: j["html_url"].as_str().unwrap_or("").to_string(),
    })
}
