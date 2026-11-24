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
    /// Carpeta de models que el servidor Ollama en marxa fa servir REALMENT
    /// (aplicada amb OLLAMA_MODELS). Serveix per saber si cal reiniciar-lo.
    #[serde(default)]
    pub models_dir_applied: String,
    /// Quant de temps Ollama manté el model carregat a la RAM després de
    /// respondre ("5m", "2m", "1m" o "0" = allibera'l de seguida). Buit =
    /// el recomanat «1m»: menys RAM en repòs que el valor d'Ollama.
    #[serde(default)]
    pub keep_alive: String,
    /// Carpeta on es desen els models GGUF (Hugging Bay) i el motor local
    /// (llama.cpp). Buida = les dades de NoOrbit. Si hi posem un DISC
    /// EXTERN (un USB, un HDD), els models hi queden i es poden fer servir
    /// des d'allà: unplugant el disc, el model desapareix de la llista.
    #[serde(default)]
    pub gguf_dir: String,
    /// Carpeta on s'instal·la el motor local (llama-server). Buida = les dades
    /// de NoOrbit. Si es posa en un disc extern, tot el motor local hi viu.
    #[serde(default)]
    pub llamacpp_dir: String,
    /// APRENENTATGE AUTÒNOM EN REPÒS: la IA nativa navega sola i aprén de
    /// contingut lliure (StackOverflow, Viquipèdia, web) quan ningú no fa
    /// servir el programa i la RAM està lliure. ACTIU per defecte.
    #[serde(default = "default_repos_aprenent")]
    pub aprenentatge_repos: bool,
    /// Punts d'entrada a la Internet profunda: URLs `.onion` separades per
    /// comes o salts de línia. BUIT = mai no hi naveguem (no inventem
    /// accessos: cal un proxy HTTP local, Privoxy/Tor, a més d'aquestes
    /// adreces).
    #[serde(default)]
    pub internet_profunda: String,
}

/// L'aprenentatge en repòs ve ACTIVAT: és el comportament que l'usuari
/// demana («que es dediqui a aprendre sola») i no costa ni CPU ni xarxa
/// mentre algú fa servir el programa.
fn default_repos_aprenent() -> bool {
    true
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            ollama_url: "http://localhost:11434".into(),
            // Model per defecte per a UN USUARI NOU (el primer que veurà):
            // 4b perquè capiga en un portàtil normal. Si l'usuari ja té la
            // config desada no afecta res, i «resolve_usable_model» sempre
            // cau al primer model instal·lat si aquest no hi és.
            default_model: "qwen3:4b".into(),
            comfyui_url: "http://localhost:8188".into(),
            image_model: "flux-schnell".into(),
            backend_3d: String::new(),
            models_dir: String::new(),
            models_dir_applied: String::new(),
            keep_alive: "1m".into(),
            gguf_dir: String::new(),
            llamacpp_dir: String::new(),
            aprenentatge_repos: default_repos_aprenent(),
            internet_profunda: String::new(),
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
        Self {
            socket_port: crate::ports::BLENDER_BRIDGE_PORT,
            binary: String::new(),
            auto_connect: true,
        }
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
        Self {
            rc_port: crate::ports::UNREAL_RC_PORT,
            capture_dir: String::new(),
        }
    }
}

/// Aprenentatge local: NoOrbit observa les converses de l'usuari amb TOTES
/// les IAs (local, remotes, especialistes, IAs web) i en treu lliçons que
/// tornen al context dels xats futures — a més d'un registre exportable per
/// entrenar un model local (LoRA) més endavant.
///
/// Tot és LOCAL: res surt de l'ordinador tret que l'usuari ho exporti.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningConfig {
    /// Interruptor principal: engega l'observació de les converses i
    /// l'injecció de les lliçons al context.
    #[serde(default)]
    pub enabled: bool,
    /// Destil·la les lliçons automàticament en segon pla (no bloqueja el xat).
    #[serde(default = "default_true")]
    pub auto_distill: bool,
    /// Injecta les lliçons apreses al system prompt dels xats.
    #[serde(default = "default_true")]
    pub inject: bool,
    /// Model local per destil·lar (buit = el model global d'Ollama).
    #[serde(default)]
    pub model: String,
    /// Nombre màxim de lliçons conservades.
    #[serde(default = "default_max_lessons")]
    pub max_lessons: usize,
}

fn default_true() -> bool {
    true
}

fn default_max_lessons() -> usize {
    120
}

/// Android: on es posa l'SDK i què es baixa quan cal instal·lar-lo.
///
/// La carpeta es tria LLIURE: pot ser un disc extern (avui un volum, demà un
/// altre). NoOrbit no coneix ni escriu mai cap nom de volum: guarda el camí
/// que li doni l'usuari i prou — igual que fa amb «ai.models_dir».
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AndroidConfig {
    /// Carpeta destí de l'SDK (buit = la ruta estàndard del sistema).
    #[serde(default)]
    pub sdk_dir: String,
    /// Nivell d'API que es baixa per defecte (34: àmpliament compatible).
    #[serde(default = "default_android_api")]
    pub api: u32,
    /// Baixa també l'«emulator» i les system images (pesen diversos GB i només
    /// fan falta per arrencar AVDs). FALSO per defecte: compilar no els needs.
    #[serde(default)]
    pub with_emulator: bool,
}

fn default_android_api() -> u32 {
    34
}

impl Default for AndroidConfig {
    fn default() -> Self {
        Self {
            sdk_dir: String::new(),
            api: default_android_api(),
            with_emulator: false,
        }
    }
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            // Desactivat per defecte: aprendre de les converses és una
            // decisió de l'usuari (privacitat), no un comportament implícit.
            enabled: false,
            auto_distill: true,
            inject: true,
            model: String::new(),
            max_lessons: default_max_lessons(),
        }
    }
}

/// Bevy (Rust): on viu el «cargo» i on es creen els projectes.
///
/// Buides = els valors per defecte del sistema (`~/.cargo`, `~/.rustup` i la
/// carpeta de l'usuari). L'assistent de Bevy hi desa el que l'usuari ha
/// triat, i la detecció del «cargo» el busca allà primer: així un Rust
/// instal·lat en un disc extern continua sent detectable.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BevyConfig {
    /// Carpeta del «cargo» (CARGO_HOME). Buida = `~/.cargo`.
    #[serde(default)]
    pub cargo_home: String,
    /// Carpeta del «rustup» (RUSTUP_HOME). Buida = `~/.rustup`.
    #[serde(default)]
    pub rustup_home: String,
    /// Carpeta on es creen els projectes Bevy nous. Buida = la carpeta de
    /// l'usuari.
    #[serde(default)]
    pub projects_dir: String,
}

/// Un projecte que ajudem com a voluntaris de la xarxa altruista (grid):
/// ens hi connectem directament amb la invitació rebuda del propietari.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridPeer {
    /// Nom visible (el projecte, quan el propietari el diu al «Welcome»).
    #[serde(default)]
    pub name: String,
    /// Adreça del propietari («ip:port»).
    pub addr: String,
    /// Codi de la invitació («noorbit-grid://codi@ip:port»).
    pub code: String,
}

/// Xarxa altruista P2P (grid): el propietari d'un projecte comparteix els
/// torns de pensament de la Jam i els voluntaris els responen amb el seu
/// model local. Sense servidor central: connexió directa per QUIC/TLS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridConfig {
    /// Interruptor mestre de la xarxa altruista (donar i rebre).
    #[serde(default)]
    pub enabled: bool,
    /// Port QUIC del servidor quan compartim.
    #[serde(default = "default_grid_port")]
    pub port: u16,
    /// Codi de la invitació (es renova amb «Genera invitació»).
    #[serde(default)]
    pub share_code: String,
    /// Projectes que ajudem com a voluntaris.
    #[serde(default)]
    pub peers: Vec<GridPeer>,
    /// Carpeta on es munten les còpies rebudes (Fase 4). Buida = la carpeta
    /// de dades de l'app.
    #[serde(default)]
    pub copies_dir: String,
}

fn default_grid_port() -> u16 {
    crate::ports::GRID_PORT
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_grid_port(),
            share_code: String::new(),
            peers: Vec::new(),
            copies_dir: String::new(),
        }
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
    /// Aprenentatge local (lliçons + registre per entrenar un model local).
    #[serde(default)]
    pub learning: LearningConfig,
    /// Android SDK: carpeta de destí lliure i paquets que es descarreguen.
    #[serde(default)]
    pub android: AndroidConfig,
    /// Bevy (Rust): on viu el «cargo» i on es creen els projectes.
    #[serde(default)]
    pub bevy: BevyConfig,
    /// Xarxa altruista P2P: compartir la Jam i ajudar altres projectes.
    #[serde(default)]
    pub grid: GridConfig,
}

impl AppConfig {
    /// Cert si l'executable s'ha d'executar en mode portàtil (les dades es
    /// desen al costat de l'app). Detecta el marcador `.noorbit-portable` o
    /// que s'execute des d'un volum muntat (p. ex. `/Volumes/…` en macOS).
    ///
    /// El marcador es mira a TOTES les carpetes on pot ser: al costat de
    /// l'executable, dins del paquet `.app`, i a l'arrel del disc (que és on
    /// el posa l'instal·lador portàtil). Així funciona igual en macOS
    /// (`.app`), Linux (binari) i Windows (`no-orbit.exe`), i en un USB
    /// connectat a qualsevol sistema.
    fn is_portable(exe: &std::path::Path, app_root: Option<&std::path::Path>) -> bool {
        // Marcador explícit: al costat de l'executable, dins del paquet de
        // l'app i a l'arrel del disc on viu l'app.
        let mut cerca: Vec<std::path::PathBuf> = Vec::new();
        if let Some(p) = exe.parent() {
            cerca.push(p.to_path_buf());
            if let Some(g) = p.parent() {
                cerca.push(g.to_path_buf());
            }
        }
        if let Some(r) = app_root {
            cerca.push(r.to_path_buf());
            if let Some(g) = r.parent() {
                cerca.push(g.to_path_buf());
            }
        }
        if cerca
            .iter()
            .any(|d| d.join(".noorbit-portable").exists())
        {
            return true;
        }
        // Execució des d'un volum/USB extern (macOS i Linux).
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

    /// Carrega la configuració sense poder fer caure l'aplicació. Si el fitxer
    /// és inaccessible o està corrupte (p. ex. un USB desendollat a mitges),
    /// el desa com a `config.toml.trencat-<marca>` i arrenca amb els valors per
    /// defecte. Així NoOrbit SEMPRE obri, encara que les dades vinguen d'un
    /// disc extern en mal estat.
    pub fn load_or_default() -> Self {
        match Self::load() {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!(
                    "NoOrbit: no s'ha pogut llegir la configuració ({e}); s'usa la per defecte."
                );
                if let Ok(path) = Self::config_path() {
                    if path.exists() {
                        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                        let backup = path.with_extension(format!("toml.trencat-{stamp}"));
                        let _ = std::fs::rename(&path, &backup);
                    }
                }
                let cfg = Self::default();
                let _ = cfg.save();
                cfg
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}
