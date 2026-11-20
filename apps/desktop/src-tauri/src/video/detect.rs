//! Detecció dels binaris de muntatge de vídeo.
//!
//! NoOrbit no empata cap editor: aprofita el que ja hi ha al sistema. A macOS
//! el paquet de Kdenlive inclou dins el `.app` un `ffmpeg`, `ffprobe`, `melt`
//! i `kdenlive_render` completets (codecs propis, `libass` per a subtítols,
//! `loudnorm` per a normalització R128). Això vol dir que es pot muntar i
//! renderitzar en capçal sense que l'usuari hagi d'instal·lar res més.
//!
//! Ordre de cerca per eina: variable d'entorn `NOORBIT_*` → `PATH` → rutes
//! conegudes del sistema (el `*.app` de Kdenlive, Homebrew, /usr/local).

use std::path::PathBuf;

/// Una eina externa ja resolta (o no) a la màquina.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub path: Option<String>,
    pub version: Option<String>,
    pub ok: bool,
}

/// Joc complet d'eines de muntatge.
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoTools {
    pub ffmpeg: Tool,
    pub ffprobe: Tool,
    pub melt: Tool,
    pub kdenlive: Tool,
    /// `true` si hi ha prou per muntar i renderitzar (ffmpeg + ffprobe).
    pub ready: bool,
    /// On s'ha trobat el directori `bin` comú, per a la UI («Kdenlive 25.x»).
    pub bundle: Option<String>,
}

/// Rutes candidates segons l'eina, en l'ordre en què es proven.
fn candidates(name: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();

    // 1) Variable d'entorn explícita: l'escapatòria per a instal·lacions rares.
    let env_key = format!("NOORBIT_{}", name.to_uppercase());
    if let Some(p) = std::env::var_os(&env_key) {
        v.push(PathBuf::from(p));
    }

    // 2) PATH estàndard.
    if let Ok(p) = which::which(name) {
        v.push(p);
    }

    // 3) Dins els paquets .app de macOS: el contingut de Kdenlive és el més
    //    complet (porta ffmpeg i MLT compilats amb tot).
    #[cfg(target_os = "macos")]
    {
        for app in [
            "/Applications/kdenlive.app",
            "/Applications/Kdenlive.app",
            "/Applications/MacKdenlive.app",
        ] {
            v.push(PathBuf::from(app).join("Contents/MacOS").join(name));
        }
        // Versions amb sufix («kdenlive 24.12.app») o instal·lacions per a
        // un sol usuari.
        if let Ok(entries) = std::fs::read_dir("/Applications") {
            for e in entries.flatten() {
                let n = e.file_name().to_string_lossy().to_lowercase();
                if n.starts_with("kdenlive") && n.ends_with(".app") {
                    v.push(e.path().join("Contents/MacOS").join(name));
                }
            }
        }
        for dir in [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/opt/local/bin",
        ] {
            v.push(PathBuf::from(dir).join(name));
        }
    }
    #[cfg(target_os = "linux")]
    {
        for dir in ["/usr/bin", "/usr/local/bin", "/snap/bin", "/var/lib/flatpak/exports/bin"] {
            v.push(PathBuf::from(dir).join(name));
        }
        // Flatpak de Kdenlive: els binaris viuen dins el portal d'exportació.
        v.push(PathBuf::from("/var/lib/flatpak/exports/bin/org.kde.kdenlive"));
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            v.push(
                PathBuf::from(local)
                    .join("Programs")
                    .join("kdenlive")
                    .join(format!("{name}.exe")),
            );
        }
        v.push(PathBuf::from(r#"C:\Program Files\kdenlive\bin"#).join(format!("{name}.exe")));
    }

    v
}

fn exists(p: &PathBuf) -> bool {
    p.is_file()
}

/// Resol un nom d'eina al primer camí que existeixi.
pub fn locate(name: &str) -> Option<String> {
    candidates(name).into_iter().find(|p| exists(p)).map(|p| p.to_string_lossy().to_string())
}

/// Llegeix la versió executant `<eina> -version` (s'esgota la sortida; és curta).
pub fn read_version(path: &str) -> Option<String> {
    let out = std::process::Command::new(path)
        .arg("-version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    // Primera línia típica: `ffmpeg version 6.1.1 Copyright (c) ...`
    let first = s.lines().next()?.trim();
    let mut it = first.split_whitespace();
    let _cmd = it.next();
    let ver = it.next().unwrap_or("").trim_start_matches('v').to_string();
    let ver = ver
        .split('-')
        .next()
        .filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or(&ver)
        .to_string();
    if ver.is_empty() {
        Some(first.chars().take(48).collect())
    } else {
        Some(ver)
    }
}

fn tool(name: &str) -> Tool {
    let path = locate(name);
    let version = path.as_deref().and_then(read_version);
    Tool {
        name: name.into(),
        ok: path.is_some(),
        path,
        version,
    }
}

/// Detecta tot el conjunt. Pot ser lent (Executa `-version`); només cal en
/// obrir el panell o engegar el director, i el resultat es desa a `VideoState`.
pub fn detect() -> VideoTools {
    let mut t = VideoTools {
        ffmpeg: tool("ffmpeg"),
        ffprobe: tool("ffprobe"),
        melt: tool("melt"),
        kdenlive: tool("kdenlive"),
        ..Default::default()
    };
    if !t.melt.ok {
        // Algunes builds de Kdenlive anomenen el renderitzador diferent.
        t.melt = tool("kdenlive_render");
    }
    t.ready = t.ffmpeg.ok && t.ffprobe.ok;
    t.bundle = t
        .kdenlive
        .path
        .as_ref()
        .and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_string_lossy().to_string()))
        .or_else(|| {
            t.ffmpeg
                .path
                .as_ref()
                .and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_string_lossy().to_string()))
        });
    t
}

impl VideoTools {
    /// Nom llegible de la suita detectada, per mostrar-lo a la UI.
    pub fn label(&self) -> String {
        match (self.kdenlive.version.as_deref(), self.ffmpeg.version.as_deref()) {
            (Some(k), _) => format!("Kdenlive {k}"),
            (None, Some(f)) => format!("ffmpeg {f}"),
            _ => "sense eines de vídeo".into(),
        }
    }

    /// Versió del MLT/`melt`, si es troba (necessària per exportar .kdenlive).
    pub fn mlt_version(&self) -> Option<&str> {
        self.melt.version.as_deref()
    }
}
