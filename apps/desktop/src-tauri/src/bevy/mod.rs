//! Integració amb **Bevy** (motor de joc en Rust): detecció de `cargo`,
//! arrencada del projecte i estat d'instal·lació.
//!
//! Bevy no té «editor» com Unreal o Godot: el projecte és un programa Rust
//! (`Cargo.toml` + `src/main.rs`) i s'EDITA amb `cargo run` (l'IA compila i
//! executa amb les directrius `@bevy:` del `engine_tools`). Aquí es resol el
//! `cargo` de l'usuari i s'obre el projecte en un Terminal perquè l'usuari
//! hi treballi igual que ho faria amb qualsevol projecte Rust.

pub mod setup;

use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Fitxer de manifest d'un projecte Rust (Bevy hi viu).
pub const MANIFEST_FILE: &str = "Cargo.toml";

/// Versió de Bevy que es posa als projectes nous.
pub const DEFAULT_BEVY_VERSION: &str = "0.14";

/// Carpeta de l'usuari (`$HOME`).
fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// Llegeix un valor de la configuració de NoOrbit sense escriure res.
fn config_bevy_value(selector: fn(&crate::config::BevyConfig) -> &str) -> Option<String> {
    let path = crate::config::AppConfig::config_path().ok()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let cfg: crate::config::AppConfig = toml::from_str(&raw).ok()?;
    let value = selector(&cfg.bevy).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Carpeta del «cargo» (CARGO_HOME) triada per l'usuari a l'assistent.
fn configured_cargo_home() -> Option<PathBuf> {
    config_bevy_value(|b| &b.cargo_home).map(PathBuf::from)
}

/// Carpeta del «rustup» (RUSTUP_HOME) triada per l'usuari a l'assistent.
fn configured_rustup_home() -> Option<PathBuf> {
    config_bevy_value(|b| &b.rustup_home).map(PathBuf::from)
}

/// Carpeta efectiva del «cargo»: la de l'entorn, la triada per l'usuari o,
/// si no n'hi ha cap, la de rustup (`~/.cargo`).
pub fn cargo_home() -> Option<PathBuf> {
    if let Ok(h) = std::env::var("CARGO_HOME") {
        if !h.trim().is_empty() {
            return Some(PathBuf::from(h.trim()));
        }
    }
    configured_cargo_home().or_else(|| home_dir().map(|h| h.join(".cargo")))
}

/// Carpeta efectiva del «rustup» (`~/.rustup` per defecte).
pub fn rustup_home() -> Option<PathBuf> {
    if let Ok(h) = std::env::var("RUSTUP_HOME") {
        if !h.trim().is_empty() {
            return Some(PathBuf::from(h.trim()));
        }
    }
    configured_rustup_home().or_else(|| home_dir().map(|h| h.join(".rustup")))
}

/// Binari de `cargo`: el del PATH o el de l'usuari (`~/.cargo/bin/cargo`).
///
/// Les apps llançades des del Finder (com NoOrbit) NO tenen el PATH de la
/// shell, i allà és on viu el cargo d'un Rust instal·lat amb rustup. Per això
/// es proven, en ordre: la carpeta de l'entorn, la que l'usuari ha triat a
/// l'assistent de Bevy, `~/.cargo`, les rutes de Homebrew i, al final, el PATH.
pub fn detect_cargo() -> Option<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(h) = std::env::var("CARGO_HOME") {
        if !h.trim().is_empty() {
            candidates.push(PathBuf::from(h.trim()).join("bin").join("cargo"));
        }
    }
    if let Some(h) = configured_cargo_home() {
        candidates.push(h.join("bin").join("cargo"));
    }
    if let Some(h) = home_dir() {
        candidates.push(h.join(".cargo").join("bin").join("cargo"));
    }
    #[cfg(target_os = "macos")]
    {
        // Homebrew: Apple Silicon i Intel.
        candidates.push(PathBuf::from("/opt/homebrew/bin/cargo"));
        candidates.push(PathBuf::from("/usr/local/bin/cargo"));
    }
    for p in candidates {
        if p.is_file() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    if let Ok(p) = which::which("cargo") {
        return Some(p.to_string_lossy().to_string());
    }
    None
}

/// La taula del manifest és de dependències? (`dependencies`,
/// `dev-dependencies`, `build-dependencies`, `target.'…'.dependencies`…).
fn is_dependencies_table(table: &str) -> bool {
    let t = table.trim();
    t == "dependencies" || t.ends_with("-dependencies") || t.ends_with(".dependencies")
}

/// Segment final d'una taula de manifest: el nom de la dependència.
fn table_leaf(table: &str) -> &str {
    table
        .rsplit('.')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
}

/// El `Cargo.toml` declara la dependència `bevy` (exactament)?
///
/// Es parseja el manifest en lloc de buscar «bevy» a text lliure: així
/// `bevy_ecs` o `bevy_render` NO tenen perquè fer pensar que és un projecte
/// de Bevy, i es detecten les taules `[dependencies.bevy]`, `[dev-dependencies]`
/// i les de `[target.'…'.dependencies]`.
pub fn manifest_has_bevy(text: &str) -> bool {
    manifest_bevy_version(text).is_some()
}

/// Versió de Bevy declarada al manifest (la del `Cargo.toml`).
///
/// Retorna `None` si el manifest no depèn de `bevy`.
pub fn manifest_bevy_version(text: &str) -> Option<String> {
    // Sense cap taula, una clau suelta es tracta com a dependència (el
    // format habitual és `[dependencies]` + `bevy = "0.14"`).
    let mut in_deps = true;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            let table = line.trim_start_matches('[').trim_end_matches(']').trim();
            let leaf = table_leaf(table);
            let prefix = table.rsplitn(2, '.').nth(1).unwrap_or("").trim();
            // `[dependencies.bevy]` (i variacions): la versió és la del
            // paquet, que és la que tria el Cargo.
            if leaf.eq_ignore_ascii_case("bevy") && is_dependencies_table(prefix) {
                return Some(DEFAULT_BEVY_VERSION.to_string());
            }
            in_deps = is_dependencies_table(table);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_matches('"').trim_matches('\'');
        if !key.eq_ignore_ascii_case("bevy") || !in_deps {
            continue;
        }
        let raw_version = value.trim().trim_matches('"').trim_matches('\'');
        // «0.14», «=0.14.1», «^0.14» o «{ version = "0.14" }» → «0.14»
        let version: String = raw_version
            .trim_start_matches(|c: char| !c.is_ascii_digit())
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let version = version.trim_end_matches('.');
        if version.is_empty() {
            return Some(DEFAULT_BEVY_VERSION.to_string());
        }
        return Some(version.to_string());
    }
    None
}

/// Projecte Bevy dins d'una carpeta: un `Cargo.toml` que depèn de `bevy`.
pub fn find_project(root: &Path) -> Option<PathBuf> {
    let mut dirs = vec![root.to_path_buf()];
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                dirs.push(e.path());
            }
        }
    }
    for dir in dirs {
        let p = dir.join(MANIFEST_FILE);
        if !p.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        if manifest_has_bevy(&text) {
            return Some(p);
        }
    }
    None
}

/// Nom de projecte vàlid per a Cargo (lletres, números, «-» i «_»).
pub fn valid_project_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !name.starts_with('-')
        && !name.starts_with('_')
}

/// Carpeta del projecte (la del `Cargo.toml`).
pub fn project_dir(cargo_toml: &Path) -> PathBuf {
    cargo_toml
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Nom vàlid de paquet Cargo derivat de la carpeta: «Meu Joc» → «meu-joc».
///
/// Cargo no accepta majúscules ni espais al nom del paquet, així que el nom
/// de la carpeta es normalitza abans de posar-lo al `Cargo.toml`.
pub fn package_name(dir: &Path) -> String {
    let raw = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut out = String::new();
    let mut guio = false;
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            guio = false;
        } else if !out.is_empty() && !guio {
            out.push('-');
            guio = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() || !valid_project_name(&out) {
        "joc".to_string()
    } else {
        out
    }
}

/// Projecte Bevy del workspace: el busca i, si no n'hi ha, el CREA.
///
/// Es crea a l'arrel del workspace, que és el que l'usuari vol dir quan
/// demana «treballa en Bevy»: així `cargo run` arrenca el joc directament.
/// Si l'arrel ja té un `Cargo.toml` que NO és de Bevy, no es pisa mai: el
/// projecte nou va a una subcarpeta.
pub fn ensure_project(root: &Path) -> Result<PathBuf, String> {
    if let Some(p) = find_project(root) {
        return Ok(p);
    }
    let name = package_name(root);
    // Conflicte a l'arrel: hi ha un manifest que no és de Bevy, o un `main.rs`
    // que ja hi havia. En cap dels dos casos no es pisa res.
    let dir = if root.join(MANIFEST_FILE).exists() || root.join("src").join("main.rs").exists() {
        root.join(&name)
    } else {
        root.to_path_buf()
    };
    let manifest = dir.join(MANIFEST_FILE);
    if manifest.exists() {
        return Err(format!(
            "ja existeix «{}» i no és un projecte Bevy: no s'ha pogut crear el projecte nou.",
            manifest.display()
        ));
    }
    std::fs::create_dir_all(dir.join("src"))
        .map_err(|e| format!("no s'ha pogut crear «{}»: {e}", dir.display()))?;
    let (toml, main_rs) = crate::bevy::setup::project_template(&name);
    std::fs::write(&manifest, toml.as_bytes())
        .map_err(|e| format!("no s'ha pogut escriure el Cargo.toml: {e}"))?;
    std::fs::write(dir.join("src").join("main.rs"), main_rs.as_bytes())
        .map_err(|e| format!("no s'ha pogut escriure el main.rs: {e}"))?;
    // El «target/» de Rust pot dec de tenir gigues: mai al Git.
    crate::fsutil::append_gitignore(&dir, &["/target"])?;

    // Es torna a detectar: el projecte nou ha de ser un projecte Bevy de debò.
    match find_project(&dir) {
        Some(p) => Ok(p),
        None => Err("el projecte s'ha creat però la detecció no el reconeix.".into()),
    }
}

/// Escapa un valor per a una comanda SHELL: el tanca entre cometes simples
/// (l'única forma segura amb espais) i escapa les cometes simples interiors.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Variables d'entorn que el Terminal necessita per trobar un Rust
/// personalitzat (instal·lat fora de `~/.cargo`).
fn env_prefix(cargo: &str) -> String {
    let mut prefix = String::new();
    if let Some(ch) = cargo_home() {
        let ch = ch.display().to_string();
        if cargo.starts_with(&ch) {
            prefix.push_str(&format!("export CARGO_HOME={}; ", shell_quote(&ch)));
        }
    }
    if let Some(rh) = rustup_home() {
        let rh = rh.display().to_string();
        let per_defecte = home_dir()
            .map(|h| h.join(".rustup").display().to_string() == rh)
            .unwrap_or(false);
        if !per_defecte {
            prefix.push_str(&format!("export RUSTUP_HOME={}; ", shell_quote(&rh)));
        }
    }
    prefix
}

/// Obre el projecte en un Terminal amb `cargo run` (a macOS i Linux amb
/// Terminal; a la resta, arrenca el procés directament).
pub fn launch_project(project_dir: &Path) -> Result<(), String> {
    let Some(cargo) = detect_cargo() else {
        return Err("Rust (cargo) no està instal·lat: cal per compilar Bevy.".into());
    };
    #[cfg(target_os = "macos")]
    {
        // L'«osascript» s'ESPERA: si macOS denega el permís
        // d'automatització (o el Terminal no s'obre), l'error ha d'arribar a
        // l'usuari en català en lloc de deixar un botó que no fa res.
        let dir = shell_quote(&project_dir.display().to_string());
        let prefix = env_prefix(&cargo);
        let cargo = shell_quote(&cargo);
        let script = format!(
            "tell application \"Terminal\" to do script \"{}cd {} && {} run\"",
            prefix,
            dir,
            cargo
        );
        let out = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| format!("no s'ha pogut obrir el Terminal: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(if err.is_empty() {
                "no s'ha pogut obrir el Terminal: macOS pot demanar permís \
                 d'automatització a NoOrbit (Ajustos del Sistema ▸ Privacitat \
                 ▸ Automatització)."
                    .to_string()
            } else {
                format!("no s'ha pogut obrir el Terminal: {err}")
            });
        }
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::process::Command::new(cargo)
            .arg("run")
            .current_dir(project_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("no s'ha pogut arrencar el projecte Bevy: {e}"))
    }
}

/// Resultat d'importar un asset al projecte Bevy.
#[derive(Debug, Clone)]
pub struct BevyImport {
    /// Nom del fitxer dins de `assets/`.
    pub file_name: String,
    /// Fitxer copiat dins el projecte.
    pub copied: PathBuf,
}

/// Importa un asset al projecte Bevy: el copia a `assets/`, on el codi Rust
/// el llegeix amb `asset_server.load("nom.glb")`.
///
/// Bevy no té un «importador» com Unreal o Godot: el joc carrega els fitxers
/// de `assets/` en temps d'execució, així que copiar-los és tot el que cal.
pub fn import_asset(cargo_toml: &Path, src: &Path) -> Result<BevyImport, String> {
    if !src.is_file() {
        return Err(format!("no trobo l'asset: {}", src.display()));
    }
    let dir = project_dir(cargo_toml);
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| "l'asset no té nom de fitxer".to_string())?;
    let assets_dir = dir.join("assets");
    std::fs::create_dir_all(&assets_dir)
        .map_err(|e| format!("no s'ha pogut crear «{}»: {e}", assets_dir.display()))?;
    let dest = assets_dir.join(&name);
    std::fs::copy(src, &dest)
        .map_err(|e| format!("no s'ha pogut copiar l'asset a «{}»: {e}", dest.display()))?;
    Ok(BevyImport { file_name: name, copied: dest })
}

/// Estat d'instal·lació de Rust/cargo (per a l'assistent i la UI).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BevyStatus {
    /// Hi ha `cargo` instal·lat (requisit per compilar Bevy).
    pub installed: bool,
    /// Versió de Rust detectada (buida si no s'ha pogut llegir).
    pub version: String,
    /// Ruta del binari (buida si no està instal·lat).
    pub path: String,
    /// Carpeta del «cargo» (CARGO_HOME) que s'ha detectat.
    pub cargo_home: String,
    /// Carpeta del «rustup» (RUSTUP_HOME) que s'ha detectat.
    pub rustup_home: String,
    /// Toolchain activa («stable-x86_64-apple-darwin»).
    pub toolchain: String,
}

/// Executa el binari i en retorna la sortida (buida si falla).
async fn run_tool(bin: &Path, args: &[&str]) -> String {
    tokio::process::Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Detecta `cargo` i en llegeix la versió (`cargo --version`).
pub async fn status() -> BevyStatus {
    let cargo_home = cargo_home().map(|p| p.display().to_string()).unwrap_or_default();
    let rustup_home = rustup_home().map(|p| p.display().to_string()).unwrap_or_default();
    let Some(path) = detect_cargo() else {
        return BevyStatus {
            installed: false,
            version: String::new(),
            path: String::new(),
            cargo_home,
            rustup_home,
            toolchain: String::new(),
        };
    };
    let version = run_tool(Path::new(&path), &["--version"]).await;
    // El «rustup» viu al costat del «cargo»: en diu la toolchain activa.
    let rustup_bin = PathBuf::from(&cargo_home).join("bin").join("rustup");
    let toolchain = if rustup_bin.is_file() {
        run_tool(&rustup_bin, &["show", "active-toolchain"]).await
    } else {
        String::new()
    };
    BevyStatus {
        installed: true,
        version,
        path,
        cargo_home,
        rustup_home,
        toolchain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn troba_el_projecte_bevy() {
        let dir = std::env::temp_dir().join(format!("noorbit-bevy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MANIFEST_FILE), "[dependencies]\nbevy = \"0.14\"\n").unwrap();
        let p = find_project(&dir).expect("hauria de trobar el projecte");
        assert_eq!(p, dir.join(MANIFEST_FILE));
        assert_eq!(project_dir(&p), dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_cargo_toml_sense_bevy_no_es_un_projecte_bevy() {
        let dir = std::env::temp_dir().join(format!("noorbit-bevy-buit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MANIFEST_FILE), "[dependencies]\nserde = \"1\"\n").unwrap();
        assert!(find_project(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// L'asset arriba a `assets/`, on Bevy el llegeix amb
    /// `asset_server.load("…")`.
    #[test]
    fn copia_l_asset_a_assets() {
        let dir = std::env::temp_dir().join(format!("noorbit-bevy-imp-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = dir.join(MANIFEST_FILE);
        std::fs::write(&manifest, "[dependencies]\nbevy = \"0.14\"\n").unwrap();
        let src = dir.join("model.glb");
        std::fs::write(&src, b"glb").unwrap();

        let r = import_asset(&manifest, &src).unwrap();
        assert_eq!(r.file_name, "model.glb");
        assert!(dir.join("assets").join("model.glb").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `ensure_project` crea el projecte Bevy si no n'hi ha, i és idempotent.
    #[test]
    fn assureix_el_projecte_bevy() {
        let dir = std::env::temp_dir().join(format!("noorbit-bevy-ens-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = ensure_project(&dir).unwrap();
        assert!(p.is_file());
        assert!(dir.join("src").join("main.rs").is_file());
        // Segona crida: torna el mateix projecte i no crea res nou.
        let p2 = ensure_project(&dir).unwrap();
        assert_eq!(p, p2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Un `Cargo.toml` que NO és de Bevy no es pisa mai: el projecte nou va a
    /// una subcarpeta i el manifest original queda intacte.
    #[test]
    fn no_pisa_un_cargo_toml_que_no_es_de_bevy() {
        let dir = std::env::temp_dir().join(format!("noorbit-bevy-conf-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MANIFEST_FILE), "[package]\nname = \"eina\"\n").unwrap();
        let p = ensure_project(&dir).unwrap();
        assert_ne!(p, dir.join(MANIFEST_FILE), "no ha d'haver tocat el manifest");
        let text = std::fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap();
        assert!(text.contains("eina"), "{text}");
        // El projecte de Bevy és a la subcarpeta i el detector el troba.
        assert!(find_project(&dir).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_asset_inexistent_dona_error() {
        let dir = std::env::temp_dir().join(format!("noorbit-bevy-imp2-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = dir.join(MANIFEST_FILE);
        std::fs::write(&manifest, "[dependencies]\nbevy = \"0.14\"\n").unwrap();
        let err = import_asset(&manifest, &dir.join("no-hi-es.glb")).unwrap_err();
        assert!(err.contains("no trobo"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// El manifest es llegeix com un TOML: `bevy_ecs` o `bevy_render` NO
    /// fan pensar que és un projecte de Bevy, i les taules de dependències
    /// (incloses les de `[target.'…'.dependencies]`) sí que es detecten.
    #[test]
    fn nomes_dependa_de_bevy_compta() {
        assert!(manifest_has_bevy("[dependencies]\nbevy = \"0.14\"\n"));
        assert!(manifest_has_bevy("[dependencies]\nbevy = { version = \"0.14\" }\n"));
        assert!(manifest_has_bevy("[dependencies.bevy]\nversion = \"0.14\"\n"));
        assert!(manifest_has_bevy("[dev-dependencies]\nbevy = \"0.14\"\n"));
        assert!(manifest_has_bevy(
            "[target.'cfg(unix)'.dependencies]\nbevy = \"0.14\"\n"
        ));
        assert!(!manifest_has_bevy("[dependencies]\nbevy_ecs = \"0.14\"\n"));
        assert!(!manifest_has_bevy("[dependencies]\nbevy_render = \"0.14\"\n"));
        assert!(!manifest_has_bevy("[package]\nname = \"bevy\"\n"));
        assert!(!manifest_has_bevy("[dependencies]\nserde = \"1\"\n"));
    }

    /// La versió de Bevy es llegeix del manifest (serveix per a la UI).
    #[test]
    fn llegeix_la_versio_de_bevy() {
        assert_eq!(
            manifest_bevy_version("[dependencies]\nbevy = \"0.14\"\n").as_deref(),
            Some("0.14")
        );
        assert_eq!(
            manifest_bevy_version("[dependencies]\nbevy = \"=0.15.1\"\n").as_deref(),
            Some("0.15.1")
        );
        assert_eq!(
            manifest_bevy_version("[dependencies]\nbevy = { version = \"0.14\" }\n").as_deref(),
            Some(DEFAULT_BEVY_VERSION)
        );
        assert_eq!(manifest_bevy_version("[dependencies]\nserde = \"1\"\n"), None);
    }

    /// Si hi ha «cargo» detectat, el fitxer hi és de debò (a macOS, l'app
    /// llançada des del Finder no té el PATH de la shell: la detecció ha de
    /// trobar-lo a `~/.cargo` o a la carpeta triada a l'assistent).
    #[test]
    fn el_cargo_detectat_existeix_al_disc() {
        if let Some(p) = detect_cargo() {
            assert!(Path::new(&p).is_file(), "el «cargo» detectat no hi és: {p}");
        }
    }

    /// Els noms de projecte segueixen les regles de Cargo.
    #[test]
    fn valida_el_nom_del_projecte() {
        assert!(valid_project_name("meu-joc"));
        assert!(valid_project_name("joc_2"));
        assert!(!valid_project_name("meu joc"));
        assert!(!valid_project_name("meu-joc!"));
        assert!(!valid_project_name(""));
        assert!(!valid_project_name("-joc"));
    }
}