//! SNAPSHOTS GIT SHADOW per pas d'agent + desfer fiable.
//!
//! Patró «shadow git» d'OpenCode (MIT): un repositori git OCULT, amb el
//! `--git-dir` FORA del projecte (dades de l'app: `snapshots/<hash-ruta>`),
//! que captura l'estat del work-tree ABANS de cada lot d'escriptures de
//! l'agent (`add -A` + `write-tree`). Mai es toca el `.git` de l'usuari ni
//! el seu historial: només objectes (trees/blobs) al git-dir ocult.
//!
//! Desfer = pop del darrer tree-hash + `read-tree` + `checkout-index -a -f`
//! sobre el work-tree, i esborrat dels fitxers NOU creats després del
//! snapshot (si no, «desfer» deixaria la brossa de l'agent al projecte).
//!
//! Degradació honesta: si `git` no existeix o el projecte no es pot llegir,
//! `take`/`begin_turn` tornen Err amb nota i el flux d'escriptura CONTINUA
//! (el snapshot és una xarxa de seguretat, no un requisit). El projecte NO
//! cal que siga un repo git: el shadow funciona sobre qualsevol carpeta.
//!
//! Piles en memòria per projecte (Mutex/HashMap global, com els altres
//! handles): es perden en tancar l'app — els objectes git queden al disc
//! però sense pila no s'hi pot tornar (disseny deliberat: «desfer el pas»
//! és per a la sessió de treball actual).

use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

/// Màxim de snapshots per projecte: els més antics es descarten (la pila
/// és per desfer passos recents, no un historial infinit).
const MAX_SNAPSHOTS: usize = 50;

/// Sostre de temps d'una operació del shadow git.
///
/// El snapshot és una XARXA DE SEGURETAT, no un requisit: si el projecte és en
/// un volum lent (exFAT sobre USB, on un `add -A` pot trigar minuts), el xat
/// no es pot quedar penjat esperant-lo. Passat el sostre, es dona el per
/// perdut i l'escriptura CONTINUA (degradació honesta, com la del mòdul).
const SHADOW_TIMEOUT: Duration = Duration::from_secs(120);

/// Contingut de `info/exclude` del shadow: la brossa pesada NO es captura
/// (ni es restaura ni s'esborra en desfer). `.git` ja l'ignora git sempre.
///
/// `._*` són els sidecars AppleDouble que macOS crea en discs exFAT/FAT: no
/// tenen cap valor (són les metadades del fitxer original, que ja hi és) i
/// en un volum extern multipliquen el nombre de fitxers que git ha de
/// indexar a cada torn de xat.
const SHADOW_EXCLUDES: &str = "node_modules/\ntarget/\ndist/\n.git/\n.noorbit/\n.noorbit-portable\n.DS_Store\n._*\n.godot/\n";

struct Turn {
    /// Tree-hash de l'estat del projecte ABANS del torn de xat.
    baseline: String,
    /// Si el baseline s'ha apilat (si era idèntic al cim, no).
    pushed: bool,
}

/// Un snapshot de la pila: el tree-hash capturat ABANS del pas i el conjunt
/// de rutes RELATIVES que l'agent ha escrit REALMENT en eixe pas. En desfer,
/// només s'esborren els fitxers NOU (untracked) que siguen a `written`: els
/// fitxers creats manualment per l'USUARI després del snapshot es conserven
/// intactes (la confirmació de «Desfés el pas» promet només fitxers de l'agent).
struct Snapshot {
    hash: String,
    written: HashSet<String>,
}

struct State {
    /// Directori de dades de l'app (app_data_dir de Tauri, inicialitzat a
    /// `lib.rs::setup`). Sense init, es degrada al data_dir de config.
    data_dir: Option<PathBuf>,
    /// Pila de snapshots per ruta canònica de projecte.
    stacks: HashMap<String, Vec<Snapshot>>,
    /// Torns de xat oberts: mentre un torn és obert, els `take` dels lots
    /// d'escriptura interns NO apilen res (el baseline del torn ja ho
    /// cobreix tot, incloses les escriptures «en viu» del frontend).
    turns: HashMap<String, Turn>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();
/// Serialitza les operacions git del shadow (dos xats del mateix projecte
/// no poden entrellaçar `add`/`write-tree`/`read-tree`).
static OPS: Mutex<()> = Mutex::new(());

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| {
        Mutex::new(State {
            data_dir: None,
            stacks: HashMap::new(),
            turns: HashMap::new(),
        })
    })
}

fn lock<'a, T>(m: &'a Mutex<T>) -> MutexGuard<'a, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Inicialitza el directori de dades on viuen els git-dirs ocults.
/// Es crida UNA vegada des del setup de Tauri (app_data_dir).
pub fn init(data_dir: PathBuf) {
    lock(state()).data_dir = Some(data_dir);
}

fn data_root() -> PathBuf {
    lock(state())
        .data_dir
        .clone()
        .or_else(crate::config::AppConfig::data_dir)
        .unwrap_or_else(|| std::env::temp_dir().join("no-orbit"))
}

/// Ruta canònica del projecte (clau de la pila i base del hash).
fn norm_root(root: &Path) -> Result<PathBuf, String> {
    let r = root
        .canonicalize()
        .map_err(|_| format!("la carpeta del projecte no existeix: {}", root.display()))?;
    if !r.is_dir() {
        return Err(format!("no és una carpeta: {}", r.display()));
    }
    Ok(r)
}

/// Git-dir ocult: `<dades>/snapshots/<sha256(ruta)[..16]>`. MAI dins el
/// projecte ni dins del seu `.git`.
fn shadow_dir(root: &Path) -> PathBuf {
    let mut h = Sha256::new();
    h.update(root.to_string_lossy().as_bytes());
    let hex = format!("{:x}", h.finalize());
    data_root().join("snapshots").join(&hex[..16])
}

/// Executa git amb el git-dir ocult i el work-tree del projecte. Es netegen
/// les variables GIT_* heretades i es neutralitza la config GLOBAL de
/// l'usuari que podria falsejar la captura (excludesFile propi,
/// showUntrackedFiles=no): només mana `info/exclude` del shadow.
fn git(shadow: &Path, root: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    let mut cmd = Command::new("git");
    cmd.arg(format!("--git-dir={}", shadow.display()))
        .arg(format!("--work-tree={}", root.display()))
        .arg("-c")
        .arg("core.quotepath=false")
        .arg("-c")
        .arg("core.autocrlf=false")
        .arg("-c")
        .arg(format!("core.excludesFile={}", shadow.join("info").join("exclude").display()))
        .arg("-c")
        .arg("status.showUntrackedFiles=all")
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("git no disponible al sistema: {e}"))?;
    // L' E/S del child es llegeix en fils PARAL·LELS: un `status` de milers de
    // fitxers omple el pipe i, sense llegir-lo, el procés es quedaria
    // bloquejat (i el timeout de sota no serviria de res).
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let t_out = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut s) = stdout_pipe {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let t_err = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut s) = stderr_pipe {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    // Espera amb SOSTRE DE TEMPS (el xat no pot quedar penjat per un volum
    // lent: el snapshot és una xarxa de seguretat, no un requisit).
    let deadline = Instant::now() + SHADOW_TIMEOUT;
    let mut sleep = Duration::from_millis(10);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                return Err(format!("wait git: {e}"));
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "git (shadow) ha trigat massa (més de {} s) en `{}`",
                SHADOW_TIMEOUT.as_secs(),
                args.first().unwrap_or(&"?")
            ));
        }
        std::thread::sleep(sleep);
        sleep = (sleep * 2).min(Duration::from_millis(100));
    };
    let stdout = t_out.join().unwrap_or_default();
    let stderr = t_err.join().unwrap_or_default();
    Ok(std::process::Output { status, stdout, stderr })
}

fn git_ok(shadow: &Path, root: &Path, args: &[&str]) -> Result<String, String> {
    let out = git(shadow, root, args)?;
    if !out.status.success() {
        return Err(format!(
            "git {} ha fallat: {}",
            args.first().unwrap_or(&"?"),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Crea (si cal) el git-dir ocult i escriu els excludes. El projecte no
/// cal que siga git: `git init` es fa al directori OCULT, no al projecte.
fn ensure_shadow(root: &Path) -> Result<PathBuf, String> {
    let dir = shadow_dir(root);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("no es pot crear el directori de snapshots: {e}"))?;
    if !dir.join("HEAD").exists() {
        // `--bare`: el git-dir és el DIRECTORI mateix (HEAD/objects/refs a
        // dins), no un `.git` fill. Després es marca core.bare=false perquè
        // git accepta el --work-tree explícit del projecte en cada crida.
        let out = Command::new("git")
            .arg("init")
            .arg("--bare")
            .arg("-q")
            .arg(&dir)
            .output()
            .map_err(|e| format!("git no disponible al sistema: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "git init del shadow ha fallat: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let out = Command::new("git")
            .arg(format!("--git-dir={}", dir.display()))
            .args(["config", "core.bare", "false"])
            .output()
            .map_err(|e| format!("git no disponible al sistema: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "git config del shadow ha fallat: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }
    let info = dir.join("info");
    std::fs::create_dir_all(&info)
        .map_err(|e| format!("no es pot crear info/ del shadow: {e}"))?;
    std::fs::write(info.join("exclude"), SHADOW_EXCLUDES)
        .map_err(|e| format!("no es pot escriure info/exclude del shadow: {e}"))?;
    Ok(dir)
}

/// `add -A` + `write-tree`: captura l'estat actual del work-tree com a
/// tree-hash (sense commits, sense tocar cap branca de l'usuari).
fn capture_tree(shadow: &Path, root: &Path) -> Result<String, String> {
    git_ok(shadow, root, &["add", "-A"])?;
    let hash = git_ok(shadow, root, &["write-tree"])?;
    if hash.len() != 40 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("write-tree ha tornat un hash inesperat: {hash}"));
    }
    Ok(hash)
}

fn push_hash(key: &str, hash: &str) -> bool {
    let mut st = lock(state());
    let stack = st.stacks.entry(key.to_string()).or_default();
    if stack.last().map(|s| s.hash == hash).unwrap_or(false) {
        return false; // res no ha canviat des del darrer snapshot
    }
    stack.push(Snapshot { hash: hash.to_string(), written: HashSet::new() });
    if stack.len() > MAX_SNAPSHOTS {
        stack.remove(0);
    }
    true
}

/// Normalitza una ruta escrita per l'agent perquè coincidisca amb la forma
/// que torna `git status --porcelain -z` (relativa, amb «/», sense «./» ni
/// barra final). Les rutes absolutes o amb «..» es descarten (no són del
/// projecte i mai s'han d'esborrar en desfer).
fn norm_written(p: &str) -> Option<String> {
    let t = p.trim().replace('\\', "/");
    let t = t.trim_start_matches("./").trim_end_matches('/');
    if t.is_empty() || t.starts_with('/') || t.split('/').any(|s| s == "..") {
        return None;
    }
    Some(t.to_string())
}

/// Registra les rutes que l'agent ha escrit REALMENT en el pas actual (el
/// cim de la pila). Es crida DESPRÉS de cada lot d'escriptures (runtools) i
/// al tancament del torn amb els fitxers que el frontend materialitzarà.
/// Mentre hi ha un torn obert, el cim és el baseline del torn: les rutes
/// s'hi acumulen perquè un sol «Desfés el pas» cobrisca tot el torn.
pub fn record_written(root: &Path, paths: &[String]) {
    let Ok(r) = norm_root(root) else { return };
    let key = r.to_string_lossy().to_string();
    let mut st = lock(state());
    if let Some(stack) = st.stacks.get_mut(&key) {
        if let Some(top) = stack.last_mut() {
            for p in paths {
                if let Some(n) = norm_written(p) {
                    top.written.insert(n);
                }
            }
        }
    }
}

/// Snapshot AUTOMÀTIC abans d'un lot d'escriptures de l'agent (runtools).
/// Si hi ha un torn de xat obert per aquest projecte, no fa res: el
/// baseline del torn (pres ABANS de qualsevol escriptura, incloses les
/// «en viu» del frontend) ja cobreix el lot.
pub fn take(root: &Path) -> Result<(), String> {
    let r = norm_root(root)?;
    let key = r.to_string_lossy().to_string();
    // La comprovació de torn obert i la captura van SOTA el mateix lock OPS
    // (abans es mirava fora: un begin_turn concurrent podia apilar un baseline
    // entre la comprovació i el push, duplicant snapshots del mateix pas).
    let _ops = lock(&OPS);
    if lock(state()).turns.contains_key(&key) {
        return Ok(());
    }
    let shadow = ensure_shadow(&r)?;
    let hash = capture_tree(&shadow, &r)?;
    push_hash(&key, &hash);
    Ok(())
}

/// Inici d'un torn de xat amb eines: captura el baseline ABANS que el
/// model escriga res (ni el backend ni la materialització en viu).
pub fn begin_turn(root: &Path) -> Result<(), String> {
    let r = norm_root(root)?;
    let key = r.to_string_lossy().to_string();
    let _ops = lock(&OPS);
    let shadow = ensure_shadow(&r)?;
    let hash = capture_tree(&shadow, &r)?;
    let mut st = lock(state());
    if st.turns.contains_key(&key) {
        return Ok(()); // un altre xat del mateix projecte ja té torn obert
    }
    let pushed = {
        let stack = st.stacks.entry(key.clone()).or_default();
        if stack.last().map(|s| s.hash == hash).unwrap_or(false) {
            false
        } else {
            stack.push(Snapshot { hash: hash.clone(), written: HashSet::new() });
            if stack.len() > MAX_SNAPSHOTS {
                stack.remove(0);
            }
            true
        }
    };
    st.turns.insert(key, Turn { baseline: hash, pushed });
    Ok(())
}

/// Final del torn de xat. `may_write`: la resposta final pot portar codi
/// que el frontend materialitzarà DESPRÉS (blocs ``` o @file:/@edit:/…):
/// en eixe cas el baseline es conserva encara que el disc no haja canviat
/// durant el torn backend. `written`: rutes que eixa materialització final
/// crearà/modificarà (s'acumulen al cim perquè l'undo les cobrisca). Si no
/// hi ha res materialitzable i el tree actual és idèntic al baseline, el
/// snapshot es descarta (torn de prosa: no ha d'aparéixer cap «Desfés el pas»).
pub fn end_turn(root: &Path, may_write: bool, written: &[String]) {
    let Ok(r) = norm_root(root) else { return };
    let key = r.to_string_lossy().to_string();
    // Tot sota OPS: la extracció del torn, el registre de rutes i el possible
    // descart del baseline no es poden entrellaçar amb un take/undo concurrent.
    let _ops = lock(&OPS);
    let turn = lock(state()).turns.remove(&key);
    let Some(turn) = turn else { return };
    record_written(&r, written);
    if !turn.pushed || may_write {
        return;
    }
    let Ok(shadow) = ensure_shadow(&r) else { return };
    let Ok(now) = capture_tree(&shadow, &r) else { return };
    if now == turn.baseline {
        let mut st = lock(state());
        if let Some(stack) = st.stacks.get_mut(&key) {
            if stack.last().map(|s| s.hash == turn.baseline).unwrap_or(false) {
                stack.pop();
            }
        }
    }
}

/// Mida de la pila de snapshots d'un projecte (0 si no n'hi ha cap).
pub fn count(root: &Path) -> usize {
    let Ok(r) = norm_root(root) else { return 0 };
    let key = r.to_string_lossy().to_string();
    lock(state()).stacks.get(&key).map(|s| s.len()).unwrap_or(0)
}

const NO_STEPS: &str = "No hi ha cap pas d'agent per desfer en aquest projecte.";

/// Esborra NOMÉS els fitxers NOU (no presents al tree restaurat) que l'agent
/// va escriure realment (`untracked ∩ written`) i poda les carpetes que queden
/// buides. Els fitxers untracked de l'USUARI (no escrits per l'agent) es
/// conserven intactes. Mai `remove_dir_all` sobre carpetes no creades per
/// l'agent: una carpeta untracked només s'esborra si l'agent la va crear
/// explícitament (consta a `written`). Els ignorats (node_modules…) no ixen
/// mai a `status --porcelain`: no es toquen.
fn remove_untracked(root: &Path, untracked: &[String], written: &HashSet<String>) -> Vec<String> {
    let mut removed = Vec::new();
    for rel in untracked {
        let clean = rel.trim_end_matches('/');
        // Xarxa de seguretat: només s'esborra el que l'agent va escriure.
        if !written.contains(clean) {
            continue;
        }
        let path = root.join(clean);
        let ok = if rel.ends_with('/') {
            // Carpeta creada per l'agent: només s'esborra si és REALMENT una
            // carpeta (mai remove_dir_all sobre un fitxer o un enllaç).
            path.is_dir() && std::fs::remove_dir_all(&path).is_ok()
        } else {
            std::fs::remove_file(&path).is_ok()
        };
        if !ok {
            continue;
        }
        removed.push(clean.to_string());
        // Poda els pares que hagen quedat buits (mai més amunt de l'arrel).
        let mut par = path.parent().map(|p| p.to_path_buf());
        while let Some(d) = par {
            if d == root || !d.starts_with(root) {
                break;
            }
            if std::fs::read_dir(&d).map(|mut it| it.next().is_none()).unwrap_or(false) {
                let _ = std::fs::remove_dir(&d);
                par = d.parent().map(|p| p.to_path_buf());
            } else {
                break;
            }
        }
    }
    removed
}

/// Desfés el darrer pas: pop del tree-hash, `read-tree` + `checkout-index
/// -a -f` al work-tree i esborrat dels fitxers creats per l'agent després.
/// Retorna les rutes RELATIVES restaurades/esborrades. NO toca l'historial
/// git de l'usuari (ni branques, ni commits, ni el seu índex).
pub fn undo(root: &Path) -> Result<Vec<String>, String> {
    let r = norm_root(root)?;
    let key = r.to_string_lossy().to_string();
    // El pop i la restauració van SOTA el mateix lock OPS (abans el pop era
    // fora: dos «Desfés» concurrents podien perdre un pas o restaurar el
    // mateix tree dues vegades). restore_tree ja NO pren OPS per si mateix.
    let _ops = lock(&OPS);
    let snap = {
        let mut st = lock(state());
        st.stacks
            .get_mut(&key)
            .and_then(|s| s.pop())
            .ok_or_else(|| NO_STEPS.to_string())?
    };
    let result = restore_tree(&r, &snap);
    if result.is_err() {
        // La restauració ha fallat: torna el snapshot a la pila perquè l'usuari
        // puga reintentar (un pop perdut seria un pas irrecuperable).
        lock(state()).stacks.entry(key).or_default().push(snap);
    }
    result
}

/// Parseja `git status --porcelain -z` (separadors NUL, sense cometes C: les
/// rutes amb accents catalans ixen en UTF-8 cru). Els renames/còpies (R/C)
/// porten un camp extra NUL amb la ruta ORIGEN (format `XY nou\0origen\0`):
/// es consumeix perquè no es interprete com una entrada independent.
fn parse_status_z(stdout: &[u8]) -> (Vec<String>, Vec<String>) {
    let mut changed: Vec<String> = Vec::new();
    let mut untracked: Vec<String> = Vec::new();
    let mut it = stdout.split(|b| *b == 0u8);
    while let Some(entry) = it.next() {
        if entry.len() < 4 {
            continue;
        }
        let xy = [entry[0], entry[1]];
        let path = String::from_utf8_lossy(&entry[3..]).to_string();
        if path.is_empty() {
            continue;
        }
        // Rename/còpia: el camp següent (NUL) és la ruta origen; es descarta
        // per al còmput de canvis (checkout-index ja restaura l'origen).
        if xy[0] == b'R' || xy[0] == b'C' || xy[1] == b'R' || xy[1] == b'C' {
            let _origen = it.next();
        }
        if xy == *b"??" {
            untracked.push(path);
        } else {
            changed.push(path);
        }
    }
    (changed, untracked)
}

/// Restaura el tree d'un snapshot. PRECONDICIÓ: el lock OPS ja està pres
/// (el crida `undo` sota OPS). L'índex del shadow passa a ser EXACTAMENT el
/// tree guardat; els fitxers modificats/esborrats es restauraren i els NOU
/// escrits per l'agent s'esborren (els de l'usuari es conserven).
fn restore_tree(root: &Path, snap: &Snapshot) -> Result<Vec<String>, String> {
    let hash = &snap.hash;
    if hash.len() != 40 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("tree-hash invàlid: {hash}"));
    }
    let shadow = ensure_shadow(root)?;
    git_ok(&shadow, root, &["read-tree", hash])?;
    // Diferències entre el tree restaurat i el work-tree actual: fitxers
    // modificats/esborrats (es restauraran) i fitxers nous (s'esborraran NOMÉS
    // si l'agent els va escriure). `-z`: rutes amb accents i renames segurs.
    let out = git(&shadow, root, &["status", "--porcelain", "-z"])?;
    if !out.status.success() {
        return Err(format!(
            "git status ha fallat: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let (mut changed, untracked) = parse_status_z(&out.stdout);
    git_ok(&shadow, root, &["checkout-index", "-a", "-f"])?;
    let removed = remove_untracked(root, &untracked, &snap.written);
    changed.extend(removed);
    changed.sort();
    changed.dedup();
    Ok(changed)
}