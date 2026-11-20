//! Registre de fallides («crash») i INFORME per enviar a l'autor.
//!
//! En mode release el perfil fa `panic = "abort"`: quan el backend peta, el
//! procés mor tot seguit i no queda cap finestra per explicar què ha passat.
//! Aquest mòdul deixa rastre:
//!   1. un hook de panic que escriu la fallida a `crash.log` (dins de la
//!      carpeta de dades de NoOrbit) i la conserva en memòria;
//!   2. un informe en text pla (Markdown) amb la versió, el sistema, les
//!      darreres fallides i el final del registre. L'usuari el pot copiar
//!      d'un clic i enviar-lo tal qual (GitHub Issues o correu).
//!
//! El registre és Cronòlògic: cada arrencada hi deixa una marca
//! (`=== sessió iniciada …`), de manera que es pot saber quina fallida és de
//! la sessió actual i quina és d'una sessió anterior (perquè l'informe
//! serveixi també quan el programa ja s'ha tornat a obrir).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use chrono::Local;

/// Fitxer on s'escriuen les fallides, dins de la carpeta de dades.
pub const LOG_FILE: &str = "crash.log";

/// Quantes fallides es conserven en memòria (les més recents).
const MAX_ENTRIES: usize = 25;

/// Quantes línies del registre s'ajunten a l'informe.
const LOG_TAIL_LINES: usize = 80;

/// A partir d'aquest tamany el registre es retalla (es queda amb les darreres
/// línies) perquè no creixi sense fi.
const MAX_LOG_BYTES: u64 = 512 * 1024;

/// Línies que es conserven quan es retalla el registre.
const KEEP_LINES_ON_TRIM: usize = 200;

/// Prefix de les línies de marca de sessió.
const SESSION_MARKER: &str = "=== ";

/// Una fallida (un panic) amb tot el que cal per explicar-la.
#[derive(Debug, Clone)]
pub struct CrashEntry {
    /// Data i hora local: `YYYY-MM-DD HH:MM:SS`.
    pub at: String,
    /// Marca temporal en mil·lisegons (per saber si és d'aquesta sessió).
    pub epoch_ms: i64,
    /// Text del panic.
    pub message: String,
    /// Fitxer i línia on ha petat: `src/lib.rs:70:5`.
    pub location: Option<String>,
    /// Pila de la crida (backtrace), si s'ha pogut capturar.
    pub backtrace: Option<String>,
}

/// Informe complet de fallides, ready per enviar.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReport {
    /// Versió de l'aplicació.
    pub app_version: String,
    /// Sistema operatiu (`macos`, `windows`, `linux`).
    pub os: String,
    /// Arquitectura (`aarch64`, `x86_64`…).
    pub arch: String,
    /// Data i hora en què va arrencar aquesta sessió.
    pub started_at: String,
    /// Carpeta on NoOrbit guarda les dades.
    pub data_dir: Option<String>,
    /// Ruta del registre de fallides.
    pub log_path: Option<String>,
    /// Quantes fallides hi ha hagut en aquesta sessió.
    pub session_panics: usize,
    /// Quantes fallides hi ha hagut en sessions anteriors.
    pub previous_panics: usize,
    /// Data i hora de la darrera fallida (de qualsevol sessió).
    pub last_crash_at: Option<String>,
    /// Antiguitat de la darrera fallida, en minuts.
    pub last_crash_age_minutes: Option<i64>,
    /// L'informe en Markdown (el text que es copia).
    pub markdown: String,
}

// ── Memòria ────────────────────────────────────────────────────────────────

fn buffer() -> &'static Mutex<Vec<CrashEntry>> {
    static BUF: OnceLock<Mutex<Vec<CrashEntry>>> = OnceLock::new();
    BUF.get_or_init(|| Mutex::new(Vec::new()))
}

/// Bloqueja el registre de memòria sense poder entrar en pànic (un panics
/// dins d'un hook de panic és un abort immediate: hi ha un codi que pot
/// fallar, i el hook ha de sobreviure).
fn lock_buffer() -> MutexGuard<'static, Vec<CrashEntry>> {
    match buffer().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn now_stamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Anota una fallida al registre de memòria (substituint-la si ja hi era:
/// el backtrace s'afegeix després del missatge).
fn record(entry: CrashEntry) {
    let mut buf = lock_buffer();
    if let Some(slot) = buf.iter_mut().find(|e| e.epoch_ms == entry.epoch_ms) {
        *slot = entry;
    } else {
        buf.push(entry);
        let extra = buf.len().saturating_sub(MAX_ENTRIES);
        if extra > 0 {
            buf.drain(0..extra);
        }
    }
}

/// Fallides d'aquesta sessió que tenim a memòria.
pub fn session_entries() -> Vec<CrashEntry> {
    lock_buffer().clone()
}

// ── Hook de panic ─────────────────────────────────────────────────────────

/// Enganxa el hook de panic: cada fallida s'escriu al registre i es conserva
/// en memòria. S'ha d'instal·lar el PRIMER (abans de qualsevol altra cosa),
/// perquè un panic a l'arrencada també quedi documentat.
pub fn install_hook(data_dir: Option<PathBuf>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // `info.to_string()` evita nombrar el tipus del hook (`PanicInfo` a
        // Rust < 1.81, `PanicHookInfo` a partir d'aquí) i sempre hi ha.
        let message = info.to_string();
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let entry = CrashEntry {
            at: now_stamp(),
            epoch_ms: now_ms(),
            message,
            location,
            backtrace: capture_backtrace(),
        };
        record(entry.clone());
        append_to_log(data_dir.as_deref(), &entry);
        // També per consola: si el programa s'ha obert des del terminal, el
        // rastre hi és a primera fila.
        eprintln!("NoOrbit: error fatal: {}", entry.message);
        previous(info);
    }));
}

/// Captura la pila de la crida. En un moment de crisi val la pena el cost.
fn capture_backtrace() -> Option<String> {
    let bt = std::backtrace::Backtrace::force_capture().to_string();
    if bt.trim().is_empty() {
        None
    } else {
        Some(bt)
    }
}

// ── El registre en disc ───────────────────────────────────────────────────

/// Ruta del registre de fallides (dins de la carpeta de dades).
pub fn log_path(data_dir: Option<&Path>) -> Option<PathBuf> {
    data_dir.map(|d| d.join(LOG_FILE))
}

/// Marca d'arrencada de sessió al registre: serveix per separar les fallides
/// d'aquesta sessió de les de sessions anteriors.
pub fn mark_session_start(data_dir: Option<&Path>) {
    let Some(path) = log_path(data_dir) else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    trim_log_if_needed(&path);
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    use std::io::Write;
    // L'hora de la marca és l'inici real de la sessió: l'informe la mostra.
    let stamp = session_start_stamp();
    let _ = writeln!(
        f,
        "{SESSION_MARKER}sessió iniciada {stamp} · v{} ===",
        env!("CARGO_PKG_VERSION")
    );
}

/// Afegeix una fallida al registre en disc.
fn append_to_log(data_dir: Option<&Path>, entry: &CrashEntry) {
    let Some(path) = log_path(data_dir) else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    trim_log_if_needed(&path);
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    use std::io::Write;
    let _ = writeln!(f, "[{}] {}", entry.at, entry.message);
    if let Some(loc) = &entry.location {
        let _ = writeln!(f, "  a {loc}");
    }
    if let Some(bt) = &entry.backtrace {
        let _ = writeln!(f, "  --- backtrace ---");
        for line in bt.lines() {
            let _ = writeln!(f, "    {line}");
        }
    }
}

/// Retalla el registre quan creix massa (es queda amb les darreres línies).
fn trim_log_if_needed(path: &Path) {
    let Ok(meta) = std::fs::metadata(path) else { return };
    if meta.len() <= MAX_LOG_BYTES {
        return;
    }
    let Ok(content) = std::fs::read_to_string(path) else { return };
    let lines: Vec<&str> = content.lines().collect();
    let keep = lines.len().saturating_sub(KEEP_LINES_ON_TRIM);
    let header = "=== registre retallat (s'han conservat les darreres línies) ===\n";
    // Pressupost per al cos del registre: el capçal no es pot retallar.
    let budget = (MAX_LOG_BYTES as usize).saturating_sub(header.len());
    let mut body = String::new();
    for line in &lines[keep..] {
        body.push_str(line);
        body.push('\n');
    }
    // Una línia gegant (o moltes) pot fer que el cos torni a ser massa gran:
    // en aquest cas es tallen els últims bytes (en un límit de caràcter UTF-8).
    if body.len() > budget {
        let bytes = body.as_bytes();
        let mut start = bytes.len() - budget;
        while start < bytes.len() && !body.is_char_boundary(start) {
            start += 1;
        }
        body = String::from_utf8_lossy(&bytes[start..]).to_string();
    }
    let mut out = String::from(header);
    out.push_str(&body);
    let _ = std::fs::write(path, out);
}

// ── Lectura i informe ─────────────────────────────────────────────────────

/// Un bloc de fallida extret del registre.
#[derive(Debug, Clone, PartialEq)]
struct LogBlock {
    /// Sessió a la que pertany (-1 = abans de la primera marca).
    session: i64,
    at: String,
    epoch_ms: Option<i64>,
    message: String,
    location: Option<String>,
    backtrace: Option<String>,
}

/// Divideix el registre en blocs de fallida i diu a quina sessió pertanyen.
fn parse_blocks(content: &str) -> Vec<LogBlock> {
    let mut out: Vec<LogBlock> = Vec::new();
    let mut current: Option<LogBlock> = None;
    let mut session: i64 = -1;
    let mut in_backtrace = false;

    for line in content.lines() {
        if line.starts_with(SESSION_MARKER) {
            if let Some(b) = current.take() {
                out.push(b);
            }
            session += 1;
            in_backtrace = false;
            continue;
        }
        // Línia de capçalera d'una fallida: «[2026-11-20 10:00:00] text».
        if line.starts_with('[') && line.len() > 21 {
            if let Some(b) = current.take() {
                out.push(b);
            }
            let rest = &line[1..];
            let (at, message) = match rest.find(']') {
                Some(i) => (rest[..i].to_string(), rest[i + 1..].trim_start().to_string()),
                None => (String::new(), rest.to_string()),
            };
            let epoch_ms = parse_stamp(&at);
            current = Some(LogBlock {
                session,
                at,
                epoch_ms,
                message,
                location: None,
                backtrace: None,
            });
            in_backtrace = false;
            continue;
        }
        if let Some(b) = current.as_mut() {
            let trimmed = line.trim_start();
            if trimmed == "--- backtrace ---" {
                in_backtrace = true;
                continue;
            }
            if in_backtrace {
                let mut bt = b.backtrace.take().unwrap_or_default();
                bt.push_str(line);
                bt.push('\n');
                b.backtrace = Some(bt);
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("a ") {
                b.location = Some(rest.to_string());
            }
        }
    }
    if let Some(b) = current.take() {
        out.push(b);
    }
    out
}

/// Converteix «YYYY-MM-DD HH:MM:SS» en marca temporal (mil·lisegons).
fn parse_stamp(at: &str) -> Option<i64> {
    use chrono::NaiveDateTime;
    let naive = NaiveDateTime::parse_from_str(at.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
    let local = naive.and_local_timezone(Local).single()?;
    Some(local.timestamp_millis())
}

/// Llegeix el registre sencer (o el que en queda).
fn read_log(data_dir: Option<&Path>) -> String {
    match log_path(data_dir) {
        Some(path) => std::fs::read_to_string(path).unwrap_or_default(),
        None => String::default(),
    }
}

/// Darreres línies del registre (les més recents).
fn tail(content: &str, lines: usize) -> String {
    let all: Vec<&str> = content.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

/// Construeix l'informe de fallides.
pub fn build_report(data_dir: Option<&Path>) -> CrashReport {
    let content = read_log(data_dir);
    let blocks = parse_blocks(&content);
    let markers = content.lines().filter(|l| l.starts_with(SESSION_MARKER)).count() as i64;
    // Sense cap marca de sessió (registre antic o carpeta no escribible), totes
    // les fallides són de sessions anteriors.
    let current_session = if markers == 0 {
        None
    } else {
        Some(markers - 1)
    };

    let session_blocks: Vec<&LogBlock> = blocks
        .iter()
        .filter(|b| current_session == Some(b.session))
        .collect();
    let previous_blocks: Vec<&LogBlock> = blocks
        .iter()
        .filter(|b| current_session.map_or(true, |c| b.session < c))
        .collect();

    let last = blocks.last();
    let last_crash_age_minutes = last
        .and_then(|b| b.epoch_ms)
        .map(|ms| ((now_ms() - ms) / 60_000).max(0));

    let app_version = env!("CARGO_PKG_VERSION").to_string();
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let started_at = session_start_stamp();
    let data_dir_s = data_dir.map(|p| p.to_string_lossy().to_string());
    let log_path_s = log_path(data_dir).map(|p| p.to_string_lossy().to_string());

    let markdown = render_markdown(
        &app_version,
        &os,
        &arch,
        &started_at,
        data_dir_s.as_deref(),
        log_path_s.as_deref(),
        &session_blocks,
        &previous_blocks,
        last,
        &tail(&content, LOG_TAIL_LINES),
    );

    CrashReport {
        app_version,
        os,
        arch,
        started_at,
        data_dir: data_dir_s,
        log_path: log_path_s,
        session_panics: session_blocks.len(),
        previous_panics: previous_blocks.len(),
        last_crash_at: last.map(|b| b.at.clone()),
        last_crash_age_minutes,
        markdown,
    }
}

/// Hora d'arrencada de la sessió (la del procés actual). Es fixa quan
/// `mark_session_start` escriu la marca al registre.
fn session_start_stamp() -> String {
    static START: OnceLock<String> = OnceLock::new();
    START.get_or_init(now_stamp).clone()
}

/// Escriu l'informe en Markdown.
#[allow(clippy::too_many_arguments)]
fn render_markdown(
    app_version: &str,
    os: &str,
    arch: &str,
    started_at: &str,
    data_dir: Option<&str>,
    log_path: Option<&str>,
    session_blocks: &[&LogBlock],
    previous_blocks: &[&LogBlock],
    last: Option<&LogBlock>,
    log_tail: &str,
) -> String {
    let mut md = String::new();
    md.push_str("# Informe de fallida de NoOrbit\n\n");
    md.push_str(&format!("**Versió:** {app_version}  \n"));
    md.push_str(&format!("**Sistema:** {os} ({arch})  \n"));
    md.push_str(&format!("**Sessió iniciada:** {started_at}  \n"));
    if let Some(d) = data_dir {
        md.push_str(&format!("**Carpeta de dades:** {d}  \n"));
    }
    if let Some(p) = log_path {
        md.push_str(&format!("**Registre de fallides:** {p}  \n"));
    }
    md.push_str(&format!(
        "\n**Fallides en aquesta sessió:** {}  \n",
        session_blocks.len()
    ));
    md.push_str(&format!(
        "**Fallides en sessions anteriors:** {}\n",
        previous_blocks.len()
    ));
    if let Some(l) = last {
        md.push_str(&format!("**Darrera fallida:** {}\n", l.at));
    }

    md.push_str("\n## Fallides d'aquesta sessió\n\n");
    if session_blocks.is_empty() {
        md.push_str("Cap.\n");
    } else {
        for b in session_blocks {
            push_block(&mut md, b);
        }
    }

    md.push_str("\n## Fallides de sessions anteriors\n\n");
    if previous_blocks.is_empty() {
        md.push_str("Cap.\n");
    } else {
        for b in previous_blocks {
            push_block(&mut md, b);
        }
    }

    md.push_str("\n## Darreres línies del registre\n\n```\n");
    if log_tail.trim().is_empty() {
        md.push_str("(el registre és buit)\n");
    } else {
        md.push_str(log_tail);
        md.push('\n');
    }
    md.push_str("```\n");
    md
}

/// Escriu un bloc de fallida en Markdown.
fn push_block(md: &mut String, b: &LogBlock) {
    md.push_str(&format!("### {}\n\n", b.at));
    md.push_str(&format!("```\n{}\n```\n\n", b.message));
    if let Some(loc) = &b.location {
        md.push_str(&format!("**Ubicació:** `{loc}`\n\n"));
    }
    if let Some(bt) = &b.backtrace {
        md.push_str("```\n");
        md.push_str(bt.trim_end());
        md.push_str("\n```\n\n");
    }
}

// ── Proves ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_log() -> String {
        let mut s = String::new();
        s.push_str("[2026-11-19 09:00:00] panicked at 'no hi ha res', src/a.rs:3:4\n");
        s.push_str("  a src/a.rs:3:4\n");
        s.push_str("  --- backtrace ---\n");
        s.push_str("    frame zero\n");
        s.push_str("=== sessió iniciada 2026-11-20 10:00:00 · v0.0.1 ===\n");
        s.push_str("[2026-11-20 11:30:00] panicked at 'ara sí', src/b.rs:9:1\n");
        s.push_str("  a src/b.rs:9:1\n");
        s
    }

    #[test]
    fn el_registre_es_talla_en_blocs_amb_sessio() {
        let blocks = parse_blocks(&sample_log());
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].session, -1, "la primera fallida és anterior a la marca");
        assert_eq!(blocks[1].session, 0, "la segona és de la sessió marcada");
        assert!(blocks[0].message.contains("no hi ha res"));
        assert_eq!(blocks[1].location.as_deref(), Some("src/b.rs:9:1"));
    }

    #[test]
    fn el_backtrace_s_llegeix_del_registre() {
        let blocks = parse_blocks(&sample_log());
        let bt = blocks[0].backtrace.as_deref().unwrap_or_default();
        assert!(bt.contains("frame zero"));
    }

    #[test]
    fn l_informe_explica_la_fallida_i_on_esta_el_registre() {
        let dir = tempfile::tempdir().expect("temp");
        std::fs::write(dir.path().join(LOG_FILE), sample_log()).expect("escriu");
        let report = build_report(Some(dir.path()));
        // Una marca de sessió → la fallida antiga és de sessions anteriors.
        assert_eq!(report.session_panics, 1);
        assert_eq!(report.previous_panics, 1);
        assert_eq!(report.last_crash_at.as_deref(), Some("2026-11-20 11:30:00"));
        assert!(report.markdown.contains("Informe de fallida de NoOrbit"));
        assert!(report.markdown.contains("src/b.rs:9:1"));
        assert!(report.log_path.unwrap().ends_with(LOG_FILE));
    }

    #[test]
    fn sense_registre_l_informe_igual_s_es_crea() {
        let dir = tempfile::tempdir().expect("temp");
        let report = build_report(Some(dir.path()));
        assert_eq!(report.session_panics, 0);
        assert_eq!(report.previous_panics, 0);
        assert!(report.markdown.contains("Cap."));
        assert!(report.last_crash_age_minutes.is_none());
    }

    #[test]
    fn un_registre_sense_marca_de_sessio_tot_son_fallides_anteriors() {
        let dir = tempfile::tempdir().expect("temp");
        // Registre antic (cap marca «=== sessió iniciada»): la fallida no és
        // d'aquesta sessió, sinó d'una sessió anterior.
        std::fs::write(
            dir.path().join(LOG_FILE),
            "[2026-11-19 09:00:00] panicked at 'vella', src/a.rs:3:4\n",
        )
        .expect("escriu");
        let report = build_report(Some(dir.path()));
        assert_eq!(report.session_panics, 0);
        assert_eq!(report.previous_panics, 1);
    }

    #[test]
    fn el_registre_es_retalla_quan_cresc_be() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join(LOG_FILE);
        let gran = "x".repeat((MAX_LOG_BYTES as usize) + 10);
        std::fs::write(&path, gran).expect("escriu");
        trim_log_if_needed(&path);
        let out = std::fs::read_to_string(&path).expect("llegeix");
        assert!(out.len() <= MAX_LOG_BYTES as usize);
        assert!(out.contains("registre retallat"));
    }
}
