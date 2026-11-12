//! Publicació de temes WordPress al marketplace (themes.thepuigberts.com).
//!
//! Fa tres coses:
//!
//! 1. **Inspecciona** la carpeta del tema: llegeix la capçalera de
//!    `style.css` (Theme Name, Version, Description, Requires PHP…), mira si
//!    hi ha `screenshot.png` i quina mida té, i avisa de què no quadra.
//! 2. **Empaqueta** la carpeta en un ZIP amb TOTS els fitxers dins d'una
//!    única carpeta (el que demana WordPress).
//! 3. **Puja** el ZIP al lloc amb l'API REST de WordPress (autenticació
//!    bàsica amb una «contrasenya d'aplicació»), i retorna l'id del fitxer
//!    pujat perquè l'usauri acabi la fitxa a la pestanya «Vendes».
//!
//! El compte i les credencials del lloc els posa l'usuari (es desen al compte
//! de NoOrbit, mai en el codi).

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

/// Informació i comprovacions del tema escollit.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeInfo {
    /// Nom del tema (capçalera «Theme Name»).
    pub name: String,
    pub version: String,
    pub description: String,
    pub requires_php: String,
    pub requires_at_least: String,
    pub author: String,
    pub has_style_css: bool,
    pub has_screenshot: bool,
    /// Mida del screenshot (ample × alçada) si s'ha pogut llegir.
    pub screenshot_size: Option<(u32, u32)>,
    /// Fitxers del tema (relatius a la carpeta arrel).
    pub files: Vec<String>,
    /// Avisos (no impedeixen la pujada).
    pub warnings: Vec<String>,
    /// Errors (sí que impedeixen la pujada).
    pub errors: Vec<String>,
    /// Carpeta triada.
    pub dir: String,
}

fn header_value(css: &str, key: &str) -> String {
    for line in css.lines().take(80) {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(v) = rest.strip_prefix(':') {
                return v.trim().trim_matches('"').to_string();
            }
        }
    }
    String::new()
}

/// Llegeix i comprova la carpeta d'un tema WordPress.
pub fn inspect(dir: &Path) -> ThemeInfo {
    let mut info = ThemeInfo {
        name: String::new(),
        version: String::new(),
        description: String::new(),
        requires_php: String::new(),
        requires_at_least: String::new(),
        author: String::new(),
        has_style_css: false,
        has_screenshot: false,
        screenshot_size: None,
        files: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
        dir: dir.to_string_lossy().to_string(),
    };

    let style = dir.join("style.css");
    info.has_style_css = style.is_file();
    if info.has_style_css {
        if let Ok(css) = std::fs::read_to_string(&style) {
            info.name = header_value(&css, "Theme Name");
            info.version = header_value(&css, "Version");
            info.description = header_value(&css, "Description");
            info.requires_php = header_value(&css, "Requires PHP");
            info.requires_at_least = header_value(&css, "Requires at least");
            info.author = header_value(&css, "Author");
        }
    } else {
        info.errors
            .push("No hi ha «style.css» a l'arrel del tema: WordPress no el reconeixerà.".into());
    }
    if info.name.is_empty() {
        info.errors
            .push("El «style.css» no té capçalera «Theme Name».".into());
    }
    if info.version.is_empty() {
        info.warnings
            .push("El tema no declara la versió («Version» al style.css).".into());
    }

    let shot = dir.join("screenshot.png");
    info.has_screenshot = shot.is_file();
    if info.has_screenshot {
        if let Ok(img) = image::open(&shot) {
            let (w, h) = (img.width(), img.height());
            info.screenshot_size = Some((w, h));
            if w != 1200 || h != 900 {
                info.warnings.push(format!(
                    "El screenshot és de {}×{}; el recomanat pel marketplace és 1200×900.",
                    w, h
                ));
            }
        }
    } else {
        info.warnings
            .push("No hi ha «screenshot.png»: la fitxa del tema quedarà sense imatge.".into());
    }

    // Llista de fitxers (relatius) i avisos sobre el que no s'ha de pujar.
    for entry in walkdir::WalkDir::new(dir).max_depth(4).into_iter().flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if let Ok(rel) = p.strip_prefix(dir) {
            let rel_s = rel.to_string_lossy().replace('\\', "/");
            info.files.push(rel_s.clone());
            let low = rel_s.to_lowercase();
            if low.starts_with("node_modules/")
                || low.starts_with(".git/")
                || low.ends_with(".ds_store")
                || low.ends_with(".map")
            {
                info.warnings
                    .push(format!("«{}» no hauria de pujar-se al marketplace.", rel_s));
            }
        }
    }
    info.files.sort();
    info
}

/// Crea el ZIP del tema: TOTS els fitxers dins d'una única carpeta (el que
/// demana WordPress). Retorna la ruta del ZIP i el nom del tema.
pub fn build_zip(dir: &Path, out_dir: &Path) -> Result<(PathBuf, String)> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let info = inspect(dir);
    let base_name = if info.name.is_empty() {
        dir.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "tema".to_string())
    } else {
        info.name.clone()
    };
    let slug = slugify(&base_name);
    std::fs::create_dir_all(out_dir)?;
    let zip_path = out_dir.join(format!("{}.zip", slug));
    let file = std::fs::File::create(&zip_path)?;
    let mut zw = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for entry in walkdir::WalkDir::new(dir).max_depth(6).into_iter().flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let rel = match p.strip_prefix(dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        let low = rel_s.to_lowercase();
        // No s'hi queda la brossa de desenvolupament.
        if low.starts_with("node_modules/")
            || low.starts_with(".git/")
            || low.ends_with(".ds_store")
            || low.ends_with(".map")
            || low.ends_with(".zip")
        {
            continue;
        }
        let name = format!("{}/{}", slug, rel_s);
        zw.start_file(name, opts)?;
        let data = std::fs::read(p)?;
        zw.write_all(&data)?;
    }
    zw.finish()?;
    Ok((zip_path, slug))
}

/// Nom curt i segur per a fitxers i carpetes (minúscules, sense espais).
pub fn slugify(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else if c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "tema".into()
    } else {
        out
    }
}

#[cfg(test)]
mod proves {
    use super::*;

    /// Crea un tema WordPress deExemple i en comprova la inspecció i el ZIP.
    #[test]
    fn inspecciona_i_empaqueta_un_tema() {
        let tmp = std::env::temp_dir().join("noorbit_theme_test");
        let _ = std::fs::remove_dir_all(&tmp);
        let dir = tmp.join("el-meu-tema");
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(
            dir.join("style.css"),
            "/*\nTheme Name: El Meu Tema\nVersion: 1.2.0\nRequires PHP: 7.4\nDescription: Un tema de prova\n*/\n",
        )
        .unwrap();
        std::fs::write(dir.join("index.php"), "<?php // índex\n").unwrap();
        std::fs::write(dir.join("assets/app.js"), "console.log('hola');\n").unwrap();

        let info = inspect(&dir);
        assert_eq!(info.name, "El Meu Tema");
        assert_eq!(info.version, "1.2.0");
        assert!(info.has_style_css);
        assert!(!info.has_screenshot); // avís, no error
        assert!(info.errors.is_empty(), "errors: {:?}", info.errors);

        let out = tmp.join("out");
        let (zip, slug) = build_zip(&dir, &out).unwrap();
        assert_eq!(slug, "el-meu-tema");
        assert!(zip.exists());
        // El ZIP ha de contenir TOTS els fitxers dins d'una única carpeta.
        let f = std::fs::File::open(&zip).unwrap();
        let mut ar = zip::ZipArchive::new(f).unwrap();
        let names: Vec<String> = (0..ar.len())
            .map(|i| ar.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"el-meu-tema/style.css".to_string()), "{:?}", names);
        assert!(names.contains(&"el-meu-tema/assets/app.js".to_string()), "{:?}", names);
        assert!(names.iter().all(|n| n.starts_with("el-meu-tema/")), "{:?}", names);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Un tema sense «style.css» ha de donar error (WordPress no el veurà).
    #[test]
    fn detecta_una_carpeta_que_no_es_un_tema() {
        let tmp = std::env::temp_dir().join("noorbit_theme_test2");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let info = inspect(&tmp);
        assert!(!info.errors.is_empty());
        assert!(info.errors.iter().any(|e| e.contains("style.css")));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

/// Resultat de la pujada al lloc.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadResult {
    pub ok: bool,
    pub message: String,
    /// Id del fitxer pujat a la biblioteca de mitjans (WordPress).
    pub attachment_id: Option<u64>,
    /// URL del fitxer pujat.
    pub url: Option<String>,
}

/// Obre la pàgina de LOGIN del marketplace al navegador INTERN de NoOrbit
/// (visible): l'usuari hi entra amb el seu compte i la sessió queda al
/// navegador. Després, la pujada es fa amb aquesta sessió (cookies), sense
/// cap contrasenya d'aplicació.
pub fn open_login(app: &tauri::AppHandle, site: &str) -> Result<String> {
    let base = site.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(anyhow!("Indica l'adreça del lloc."));
    }
    let url = format!("{}/login/", base);
    crate::browser::open(app, &url, false)?;
    let _ = crate::browser::set_visible(app, true);
    Ok(format!(
        "He obert {} al navegador de NoOrbit (el veuràs a la dreta). Entra-hi amb el \
         teu compte i, quan siguis dins, torna aquí i prem «Puja el ZIP».",
        url
    ))
}

/// Pujada amb la SESSIÓ DEL NAVEGADOR: la petició es fa des de la mateixa
/// pàgina del lloc (cookies + nonce de WordPress), així que no cal cap
/// contrasenya d'aplicació ni programmatic auth.
///
/// Procés:
///  1. Assegura que el navegador és al lloc (i a una pàgina d'administració,
///     que és on viu el nonce).
///  2. Llegeix el nonce i comprova que hi ha sessió iniciada.
///  3. Engega la pujada (fetch) i en.poll el resultat fins que acaba.
pub async fn upload_with_session(
    app: &tauri::AppHandle,
    zip: &Path,
    site: &str,
) -> Result<UploadResult> {
    use base64::Engine as _;
    let base = site.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(anyhow!("Indica l'adreça del lloc."));
    }
    let st = crate::browser::status(app);
    let host = base
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("");
    // Si el navegador no és al lloc (o no hi ha sessió), l'hi portem: una
    // pàgina d'administració és on WordPress deixa el nonce de l'API REST.
    let need_nav = !st.open || !st.url.contains(host);
    if need_nav {
        crate::browser::open(app, &format!("{}/wp-admin/", base), false)?;
        let _ = crate::browser::set_visible(app, true);
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    }
    // 1) Nonce i estat de la sessió.
    let js_probe = r#"(function(){
      try{
        var n = (window.wpApiSettings && window.wpApiSettings.nonce) || '';
        if(!n){ var m = document.documentElement.innerHTML.match(/"nonce":"([a-f0-9]{8,20})"/); if(m) n = m[1]; }
        var loginForm = !!document.querySelector('form[action*="wp-login"], #loginform');
        var title = document.title || '';
        return JSON.stringify({nonce:n, login:loginForm, url:location.href, title:title});
      }catch(e){ return JSON.stringify({error:String(e)}); }
    })()"#;
    let probe = crate::browser::run_js(app, js_probe, std::time::Duration::from_secs(10))
        .await
        .unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(probe.trim()).unwrap_or(serde_json::Value::Null);
    let nonce = v.get("nonce").and_then(|n| n.as_str()).unwrap_or("").to_string();
    let on_login = v.get("login").and_then(|b| b.as_bool()).unwrap_or(false);
    if nonce.is_empty() {
        return Err(anyhow!(
            "No he trobat el nonce de WordPress: obre la teva sessió a {} (el botó \
             «Inicia sessió» obre la pàgina de login) i torna-ho a provar.",
            format!("{}/login/", base)
        ));
    }
    if on_login {
        return Err(anyhow!(
            "Encara no has iniciat la sessió a {}. Obre «Inicia sessió», entra-hi amb el \
             teu compte i torna a provar.",
            base
        ));
    }

    // 2) Engega la pujada (el resultat es deixa en window.__noorbitUpload).
    let data = std::fs::read(zip)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
    let name = zip
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "tema.zip".into());
    let js_start = format!(
        r#"(function(){{
  try{{
    window.__noorbitUpload = {{state:'pending', message:''}};
    var b64 = {b64};
    var name = {name};
    var nonce = {nonce};
    var bin = atob(b64);
    var arr = new Uint8Array(bin.length);
    for (var i=0;i<bin.length;i++) arr[i] = bin.charCodeAt(i);
    fetch('/wp-json/wp/v2/media', {{
      method:'POST', credentials:'include',
      headers:{{
        'Content-Disposition':'attachment; filename="'+name+'"',
        'Content-Type':'application/zip',
        'X-WP-Nonce':nonce
      }},
      body: arr
    }}).then(function(res){{
      return res.text().then(function(txt){{
        if(!res.ok){{ window.__noorbitUpload = {{state:'error', message:'El lloc ha rebutjat la pujada ('+res.status+'): '+txt.slice(0,240)}}; return; }}
        var j = {{}}; try{{ j = JSON.parse(txt); }}catch(e){{}}
        window.__noorbitUpload = {{state:'done', message:'ZIP pujat (fitxer #'+(j.id||'?')+').', id:j.id||null, url:j.source_url||null}};
      }});
    }}).catch(function(e){{ window.__noorbitUpload = {{state:'error', message:'Error de pujada: '+String(e)}}; }});
  }}catch(e){{ window.__noorbitUpload = {{state:'error', message:String(e)}}; }}
}})();"#,
        b64 = serde_json::to_string(&b64).unwrap_or_else(|_| "\"\"".into()),
        name = serde_json::to_string(&name).unwrap_or_else(|_| "\"tema.zip\"".into()),
        nonce = serde_json::to_string(&nonce).unwrap_or_else(|_| "\"\"".into()),
    );
    let _ = crate::browser::run_js(app, &js_start, std::time::Duration::from_secs(15)).await;

    // 3) Espera el resultat (pujada d'un ZIP: pot trigar).
    let js_poll = r#"(function(){ try{ return JSON.stringify(window.__noorbitUpload || {state:'pending'}); }catch(e){ return '{"state":"error","message":"poll"}'; } })()"#;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(900);
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        let raw = crate::browser::run_js(app, js_poll, std::time::Duration::from_secs(10))
            .await
            .unwrap_or_default();
        let r: serde_json::Value = serde_json::from_str(raw.trim()).unwrap_or(serde_json::Value::Null);
        match r.get("state").and_then(|s| s.as_str()).unwrap_or("pending") {
            "done" => {
                return Ok(UploadResult {
                    ok: true,
                    message: format!(
                        "{} Ara obre «Vendes ▸ Pujar Tema» per acabar la fitxa (preu, descripció i captures).",
                        r.get("message").and_then(|m| m.as_str()).unwrap_or("ZIP pujat.")
                    ),
                    attachment_id: r.get("id").and_then(|i| i.as_u64()),
                    url: r.get("url").and_then(|u| u.as_str()).map(|s| s.to_string()),
                });
            }
            "error" => {
                return Err(anyhow!(
                    "{}",
                    r.get("message").and_then(|m| m.as_str()).unwrap_or("Error de pujada.")
                ));
            }
            _ => {}
        }
        if std::time::Instant::now() > deadline {
            return Err(anyhow!(
                "La pujada ha trigat massa (15 min) i s'ha aturat. Torna-ho a provar."
            ));
        }
    }
}

/// Puja el ZIP al marketplace amb l'API REST de WordPress.
/// Autenticació: «contrasenya d'aplicació» de WordPress (usuari + contrasenya
/// d'aplicació, autorització bàsica).
pub async fn upload_zip(
    http: &reqwest::Client,
    zip: &Path,
    site: &str,
    user: &str,
    app_password: &str,
) -> Result<UploadResult> {
    use base64::Engine as _;
    let base = site.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(anyhow!("Indica l'adreça del lloc on pujar el tema."));
    }
    if user.trim().is_empty() || app_password.trim().is_empty() {
        return Err(anyhow!(
            "Fan falta l'usuari i la contrasenya d'aplicació del lloc (WordPress ▸ «Contrasenyes d'aplicació»)."
        ));
    }
    let data = std::fs::read(zip)?;
    let token = base64::engine::general_purpose::STANDARD
        .encode(format!("{}:{}", user.trim(), app_password.trim()));
    let name = zip
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "tema.zip".into());

    let resp = http
        .post(format!("{}/wp-json/wp/v2/media", base))
        .header("Authorization", format!("Basic {}", token))
        .header("Content-Disposition", format!("attachment; filename=\"{}\"", name))
        .header("Content-Type", "application/zip")
        .body(data)
        .send()
        .await
        .map_err(|e| anyhow!("No s'ha pogut pujar el ZIP: {}", e))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        let motiu = if body.contains("rest_cannot_create") {
            "el compte no té permís per pujar fitxers (capacitat «upload_files»)".to_string()
        } else if status.as_u16() == 401 {
            "l'usuari o la contrasenya d'aplicació no són correctes".to_string()
        } else if status.as_u16() == 404 {
            "l'API del lloc no ha respost (comprova l'adreça)".to_string()
        } else {
            body.chars().take(200).collect::<String>()
        };
        return Err(anyhow!("El lloc ha rebutjat la pujada ({}): {}", status, motiu));
    }
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
    let id = v.get("id").and_then(|i| i.as_u64());
    let url = v
        .get("source_url")
        .and_then(|u| u.as_str())
        .map(|s| s.to_string());
    Ok(UploadResult {
        ok: true,
        message: format!(
            "ZIP pujat{} a {}. Ara obre «Vendes ▸ Pujar Tema» per acabar la fitxa (preu, descripció i captures).",
            match id {
                Some(i) => format!(" (fitxer #{})", i),
                None => String::new(),
            },
            base
        ),
        attachment_id: id,
        url,
    })
}
