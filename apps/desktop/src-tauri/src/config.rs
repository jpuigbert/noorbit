//! Configuració de NoOrbit — es desa com a TOML a l'aplicació de dades.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// URL base d'Ollama.
    pub ollama_url: String,
    /// Model de text per defecte.
    pub default_model: String,
    /// URL base de ComfyUI per generació d'imatges.
    pub comfyui_url: String,
    /// Model de generació d'imatges (prompt final).
    pub image_model: String,
    /// Backend 3D local ("tripo", "comfy3d" o buit = cap).
    pub backend_3d: String,
    /// Carpeta on Ollama desa els models (buit = la per defecte del sistema).
    /// Permet posar-los en un USB extern per estalviar espai al disc.
    #[serde(default)]
    pub models_dir: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            ollama_url: "http://localhost:11434".into(),
            default_model: "qwen3:8b".into(),
            comfyui_url: "http://localhost:8188".into(),
            image_model: "flux-schnell".into(),
            backend_3d: String::new(),
            models_dir: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    pub font_size: u32,
    pub tab_size: u32,
    pub theme: String,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self { font_size: 13, tab_size: 2, theme: "vs-dark".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlenderConfig {
    /// Port del socket de l'add-on NoOrbit dins Blender.
    pub socket_port: u16,
    /// Ruta del binari blender (buit = autodetect).
    pub binary: String,
    /// vigilant de fons que connecta Blender sol quan detecta el port 9876
    /// obert (obrir Blender → connectat sense prémer res).
    #[serde(default)]
    pub auto_connect: bool,
}

impl Default for BlenderConfig {
    fn default() -> Self {
        Self { socket_port: 9876, binary: String::new(), auto_connect: true }
    }
}

/// Presupesto de temps per a tasques llargues de l'agent/IA: mentre la tasca
/// és dins del llindar de primer pla s'executa normal (la UI la segueix en
/// directe); si se'n surt, es degrada a segon pla (prioritat baixa del procés,
/// Ollama amb menys fils i context, polls espaiats) i la UI només en mostra
/// un indicador compacte. El límit total talla la feina si s'esgota.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLimitsConfig {
    /// Segons de primer pla abans de degradar la tasca a segon pla.
    #[serde(default = "default_foreground_limit")]
    pub foreground_limit_s: u64,
    /// Segons totals abans d'abandonar la tasca (0 = sense límit).
    #[serde(default = "default_total_limit")]
    pub total_limit_s: u64,
}

fn default_foreground_limit() -> u64 {
    60
}

fn default_total_limit() -> u64 {
    3600
}

impl Default for TaskLimitsConfig {
    fn default() -> Self {
        Self {
            foreground_limit_s: default_foreground_limit(),
            total_limit_s: default_total_limit(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnrealConfigFile {
    /// Port de la Remote Control API d'Unreal.
    pub rc_port: u16,
    /// Carpeta on es deseen captures del viewport.
    pub capture_dir: String,
}

impl Default for UnrealConfigFile {
    fn default() -> Self {
        Self { rc_port: 30010, capture_dir: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub blender: BlenderConfig,
    #[serde(default)]
    pub unreal: UnrealConfigFile,
    /// Límits de temps de les tasques llargues (degradació a segon pla).
    #[serde(default)]
    pub tasks: TaskLimitsConfig,
    /// Carpeta d'arrels de skills (buit = per defecte).
    #[serde(default)]
    pub skills_dir: String,
}

impl AppConfig {
    /// Cert si l'executable s'ha d'executar en mode portàtil (les dades es
    /// desen al costat de l'app). Detecta el marcador `.noorbit-portable` o
    /// que s'execute des d'un volum muntat (p. ex. `/Volumes/…` en macOS).
    fn is_portable(exe: &std::path::Path, app_root: Option<&std::path::Path>) -> bool {
        // Marcador explícit al costat de l'executable o de l'app.
        if exe.parent().map(|d| d.join(".noorbit-portable").exists()).unwrap_or(false) {
            return true;
        }
        if app_root.map(|d| d.join(".noorbit-portable").exists()).unwrap_or(false) {
            return true;
        }
        // Execució des d'un volum/USB extern.
        let s = exe.to_string_lossy();
        s.starts_with("/Volumes/") || s.contains("/media/")
    }

    /// Arrel del paquet `*.app` que conté l'executable, si n'hi ha.
    fn app_root(exe: &std::path::Path) -> Option<PathBuf> {
        exe.ancestors()
            .find(|anc| anc.extension().map(|e| e == "app").unwrap_or(false))
            .map(|p| p.to_path_buf())
    }

    /// Retorna la carpeta on es desa la configuració. En mode portàtil (app
    /// dentro d'un USB o amb el marcador `.noorbit-portable`), les dades es
    /// desen en una carpeta `NoOrbitData` al costat de l'aplicació: així es
    /// pot arrossegar l'app a un USB i emportar-se configuració i models.
    pub fn data_dir() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let root = Self::app_root(&exe);
        if Self::is_portable(&exe, root.as_deref()) {
            let base = root
                .as_ref()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .or_else(|| exe.parent().map(|p| p.to_path_buf()))
                .unwrap_or_else(std::env::temp_dir);
            let data = base.join("NoOrbitData");
            let _ = std::fs::create_dir_all(&data);
            return Some(data);
        }
        dirs::data_dir().map(|d| d.join("no-orbit"))
    }

    pub fn config_path() -> Result<PathBuf> {
        let dir = Self::data_dir().context("No es pot resoldre el directori de dades")?;
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            let cfg: AppConfig = toml::from_str(&raw).context("Config TOML invàlida")?;
            Ok(cfg)
        } else {
            let cfg = AppConfig::default();
            let _ = cfg.save();
            Ok(cfg)
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}
