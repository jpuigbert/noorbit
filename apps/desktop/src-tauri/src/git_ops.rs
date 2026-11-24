//! Eines de GIT que li falten a NoOrbit: BRANQUES i WORKTREES.
//!
//! Fins ara NoOrbit sabia fer commit, push, pull, stash i veure l'historial,
//! però de les branques no en sabia res: no hi havia manera de crear-ne una,
//! canviar-hi, esborrar-ne ni veure quines hi ha. I quan la IA esborrava una
//! carpeta, els worktrees i les branques que hi vivien es quedaven penjats:
//! el repositori mantenia el registre d'un worktree que ja no existia i la
//! branca es quedava al repositori per sempre (branches orfes).
//!
//! Aquest mòdul és l'única font de la veritat sobre branques: el backend
//! (comandaments Tauri), la IA (capes deterministes) i el netejat en
//! esborrar una carpeta passen per aquí.
//!
//! Decisió d'implementació: el binari `git` del sistema, com a
//! `commands/git.rs` (cap de `git2`, que arrossega compilar libgit2 C i
//! infla el binari). Totes les comandes són curtes i bloquejants: els
//! comandaments Tauri les executen amb `spawn_blocking`.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Una branca local del repositori.
#[derive(Debug, Clone, Serialize)]
pub struct BranchInfo {
    /// Nom curt de la branca (sense «refs/heads/»).
    pub name: String,
    /// `true` si és la branca on som ara mateix.
    pub current: bool,
    /// Branca remota de seguiment, si en té («origin/main»).
    pub upstream: Option<String>,
    /// Ruta del worktree on la branca està comprovada, si en té.
    pub worktree: Option<String>,
    /// Commits locals que encara no s'han pujat.
    pub ahead: i64,
    /// Commits remots que encara no s'han baixat.
    pub behind: i64,
}

/// Un worktree registrat al repositori.
#[derive(Debug, Clone, Serialize)]
pub struct WorktreeInfo {
    /// Ruta del worktree.
    pub path: String,
    /// Branca comprovada-hi, si n'hi ha (podeix ser «detached»).
    pub branch: Option<String>,
    /// `false` si la carpeta ja no existeix: és un registre ORFE que cal
    /// netejar amb `git worktree prune`.
    pub exists: bool,
}

/// Resultat de netejar el que penja d'una carpeta esborrada.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Cleanup {
    /// Worktrees desregistrats (i eliminats).
    pub removed_worktrees: Vec<String>,
    /// Branches esborrades perquè eren d'aquells worktrees.
    pub removed_branches: Vec<String>,
    /// Branches que NO s'han esborrat perquè tenen feina que no es pot
    /// perdre: (nom, motiu).
    pub kept_branches: Vec<(String, String)>,
}

// ── Execució de git ─────────────────────────────────────────────────────────

/// Executa `git` a `dir` i en torna la sortida. El codi de sortida NO és
/// error: moltes comandes de git «no hi ha res» retornen 1.
fn git_raw(dir: &Path, args: &[&str]) -> Result<(bool, String), String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("git: {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        Ok((true, stdout))
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if err.is_empty() {
            Ok((false, stdout))
        } else {
            Ok((false, err))
        }
    }
}

/// Executa `git` i, si falla, torna l'error de git (traduït al català pel
/// caller).
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let (ok, out) = git_raw(dir, args)?;
    if ok {
        Ok(out)
    } else {
        Err(out)
    }
}

/// La carpeta és dins d'un repositori git?
pub fn is_repo(dir: &Path) -> bool {
    git_raw(dir, &["rev-parse", "--is-inside-work-tree"])
        .map(|(ok, out)| ok && out.trim() == "true")
        .unwrap_or(false)
}

/// L'arrel del repositori que conté `dir` (pot ser el mateix `dir`).
pub fn repo_root(dir: &Path) -> Option<PathBuf> {
    let (ok, out) = git_raw(dir, &["rev-parse", "--show-toplevel"]).ok()?;
    if !ok {
        return None;
    }
    let t = out.trim();
    if t.is_empty() {
        None
    } else {
        Some(PathBuf::from(t))
    }
}

/// El repositori PRINCIPAL d'un worktree (el `.git` compartit). Per a un
/// repositori normal és el mateix directori.
pub fn main_repo(dir: &Path) -> Option<PathBuf> {
    let (ok, out) = git_raw(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .ok()?;
    if !ok {
        return repo_root(dir);
    }
    let common = out.trim();
    if common.is_empty() {
        return repo_root(dir);
    }
    let p = PathBuf::from(common);
    // «.git» → el pare és l'arrel del repositori principal.
    if p.file_name().map(|n| n == ".git").unwrap_or(false) {
        p.parent().map(|x| x.to_path_buf())
    } else {
        Some(p)
    }
}

/// La branca on som ara mateix («HEAD» detached → cadena buida).
pub fn current_branch(dir: &Path) -> Result<String, String> {
    let (ok, out) = git_raw(dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if !ok {
        return Err("No es pot saber la branca actual.".into());
    }
    let b = out.trim();
    if b == "HEAD" {
        // HEAD detached: no hi ha branca.
        Ok(String::new())
    } else {
        Ok(b.to_string())
    }
}

// ── Branques ────────────────────────────────────────────────────────────────

/// Llista les branques locals amb la seva situació (actual, upstream, worktree,
/// ahead/behind). L'orden és el del git: alfabètic.
pub fn branches(dir: &Path) -> Result<Vec<BranchInfo>, String> {
    if !is_repo(dir) {
        return Err("Això no és un repositori Git.".into());
    }
    // `%(worktreepath)` és buit si la branca no és comprovada en cap worktree.
    let (ok, out) = git_raw(
        dir,
        &[
            "for-each-ref",
            "--format=%(refname:short)|%(HEAD)|%(upstream:short)|%(worktreepath)",
            "refs/heads",
        ],
    )?;
    if !ok {
        return Err("No es poden llegir les branques.".into());
    }
    let mut list: Vec<BranchInfo> = Vec::new();
    for line in out.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let mut it = line.split('|');
        let name = it.next().unwrap_or("").trim().to_string();
        if name.is_empty() {
            continue;
        }
        let head = it.next().unwrap_or("").trim();
        let upstream = it.next().unwrap_or("").trim();
        let worktree = it.next().unwrap_or("").trim();
        list.push(BranchInfo {
            name,
            current: head == "*",
            upstream: if upstream.is_empty() {
                None
            } else {
                Some(upstream.to_string())
            },
            worktree: if worktree.is_empty() {
                None
            } else {
                Some(worktree.to_string())
            },
            ahead: 0,
            behind: 0,
        });
    }
    // Ahead/behind respecte de l'upstream, sense xarxa (no hi ha `fetch`):
    // `rev-list --left-right --count upstream...branch` → «darrere davant».
    for b in list.iter_mut() {
        let Some(up) = b.upstream.clone() else { continue };
        let counts = git_raw(
            dir,
            &["rev-list", "--left-right", "--count", &format!("{up}...{}", b.name)],
        )
        .ok()
        .filter(|(ok, _)| *ok)
        .map(|(_, out)| out);
        if let Some(c) = counts {
            let mut it = c.split_whitespace();
            let behind = it.next().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            let ahead = it.next().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            b.behind = behind;
            b.ahead = ahead;
        }
    }
    Ok(list)
}

/// Els worktrees registrats al repositori (inclosos els orfes, és a dir, els
/// que tenen el registre però la carpeta ja no hi és).
pub fn worktrees(dir: &Path) -> Result<Vec<WorktreeInfo>, String> {
    let (ok, out) = git_raw(dir, &["worktree", "list", "--porcelain"])?;
    if !ok {
        // Sense worktrees, `git worktree list` pot fallar en repositoris
        // molt antics: no és un error, simply no n'hi ha.
        return Ok(vec![]);
    }
    let mut out_list: Vec<WorktreeInfo> = Vec::new();
    let mut path = String::new();
    let mut branch: Option<String> = None;
    for line in out.lines() {
        let l = line.trim();
        if l.is_empty() {
            // Fi del registre actual.
            if !path.is_empty() {
                out_list.push(WorktreeInfo {
                    exists: Path::new(&path).exists(),
                    path: path.clone(),
                    branch: branch.take(),
                });
            }
            path.clear();
            branch = None;
            continue;
        }
        if let Some(rest) = l.strip_prefix("worktree ") {
            if !path.is_empty() {
                out_list.push(WorktreeInfo {
                    exists: Path::new(&path).exists(),
                    path: path.clone(),
                    branch: branch.take(),
                });
            }
            path = rest.trim().to_string();
        } else if let Some(rest) = l.strip_prefix("branch ") {
            // «refs/heads/feature» → «feature».
            let b = rest.trim();
            branch = Some(
                b.strip_prefix("refs/heads/")
                    .unwrap_or(b)
                    .to_string(),
            );
        }
    }
    if !path.is_empty() {
        out_list.push(WorktreeInfo {
            exists: Path::new(&path).exists(),
            path: path.clone(),
            branch: branch.take(),
        });
    }
    Ok(out_list)
}

/// El nom de la branca és vàlid? (git ho rebutja, però el missatge del git és
/// en anglès i no diu QUÈ s'ha fet malament).
pub fn valid_branch_name(name: &str) -> Result<(), String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("El nom de la branca és buit.".into());
    }
    if n.starts_with('-') {
        return Err("El nom de la branca no pot començar amb «-».".into());
    }
    if n.ends_with('/') || n.ends_with('.') || n.ends_with(".lock") {
        return Err(format!("«{}» no és un nom de branca vàlid.", n));
    }
    if n.contains("..") {
        return Err(format!("«{}» no és un nom de branca vàlid (conté «..»).", n));
    }
    if n.contains(" ") {
        return Err("El nom de la branca no pot contenir espais.".into());
    }
    for c in ['~', '^', ':', '?', '*', '[', '\\', '@', '{'] {
        if n.contains(c) {
            return Err(format!(
                "«{}» no és un nom de branca vàlid (conté «{}»).",
                n, c
            ));
        }
    }
    Ok(())
}

/// Crea una branca nova. `from` és el punt de partida (per defecte, l'actual).
/// Amb `checkout` la deixa comprovada; sense, només la crea.
pub fn create_branch(
    dir: &Path,
    name: &str,
    from: Option<&str>,
    checkout: bool,
) -> Result<String, String> {
    if !is_repo(dir) {
        return Err("Això no és un repositori Git.".into());
    }
    valid_branch_name(name)?;
    let name = name.trim();
    if branches(dir)?.iter().any(|b| b.name == name) {
        return Err(format!("La branca «{}» ja existeix.", name));
    }
    let mut args: Vec<&str> = vec!["branch", name];
    if let Some(f) = from.map(str::trim).filter(|f| !f.is_empty()) {
        // Un punt de partida que comenci per «-» seria un FLAG de git, no
        // una referència: mai no l'acceptem (mateixa protecció que el nom).
        if f.starts_with('-') {
            return Err("El punt de partida no pot començar amb «-».".into());
        }
        args.push(f);
    }
    git(dir, &args)?;
    if checkout {
        checkout_branch(dir, name)?;
    }
    Ok(format!(
        "Branca «{}» creada{}.",
        name,
        if checkout { " i comprovada" } else { "" }
    ))
}

/// Canvia a una branca. Es rebutja si la branca ja està comprovada en un altre
/// worktree (git també ho rebutja, però el seu missatge no diu on és).
pub fn checkout_branch(dir: &Path, name: &str) -> Result<String, String> {
    if !is_repo(dir) {
        return Err("Això no és un repositori Git.".into());
    }
    let name = name.trim();
    if name.is_empty() {
        return Err("Cal el nom de la branca.".into());
    }
    if !branches(dir)?.iter().any(|b| b.name == name) {
        return Err(format!("No hi ha cap branca «{}».", name));
    }
    // Ja comprovada en un altre worktree? git ho rebutja amb un missatge
    // sec; nosaltres donem la ruta, que és el que l'usuari vol saber.
    for w in worktrees(dir)? {
        if let Some(b) = &w.branch {
            if b == name && !same_path(Path::new(&w.path), dir) {
                return Err(format!(
                    "La branca «{}» ja està comprovada a «{}». Tanca'l o esborra'l abans de canviar-hi.",
                    name,
                    w.path
                ));
            }
        }
    }
    git(dir, &["checkout", name])?;
    Ok(format!("Branca «{}» activada.", name))
}

/// Esborra una branca. Sense `force` es rebutja si no està fusionada (com el
/// `git branch -d`); amb `force`, s'esborra igual (`-D`). Mai s'esborra la
/// branca on som ni una que estigui comprovada en un worktree.
pub fn delete_branch(dir: &Path, name: &str, force: bool) -> Result<String, String> {
    if !is_repo(dir) {
        return Err("Això no és un repositori Git.".into());
    }
    let name = name.trim();
    if name.is_empty() {
        return Err("Cal el nom de la branca.".into());
    }
    // El nom ve de la IA i HA de passar la mateixa validació que a la
    // creació: sense això, «delete -a» arribaria a git com a FLAG i
    // `git branch -d -a` esborraria TOTES les branques fusionades.
    valid_branch_name(name)?;
    let cur = current_branch(dir)?;
    if cur == name {
        return Err(format!(
            "No es pot esborrar la branca «{}» perquè és la que estàs usant. Canvia't abans.",
            name
        ));
    }
    for w in worktrees(dir)? {
        if let Some(b) = &w.branch {
            if b == name {
                return Err(format!(
                    "La branca «{}» està comprovada a «{}»: esborra primer el worktree.",
                    name,
                    w.path
                ));
            }
        }
    }
    let flag = if force { "-D" } else { "-d" };
    // `--` separa les opcions del nom: ni validat, cap nom amb forma
    // d'opció pot colar-se com a flag.
    let (ok, out) = git_raw(dir, &["branch", flag, "--", name])?;
    if !ok {
        let msg = if out.contains("not fully merged") {
            format!(
                "La branca «{}» té commits que no s'han fusionat. Si la vols esborrar igual, digues «esborra la branca {} amb força».",
                name, name
            )
        } else {
            out
        };
        return Err(msg);
    }
    Ok(format!("Branca «{}» esborrada.", name))
}

/// Neteja els registres de worktree ORFES (la carpeta ja no hi és) i en torna
/// el text de git.
pub fn prune_worktrees(dir: &Path) -> Result<String, String> {
    if !is_repo(dir) {
        return Err("Això no és un repositori Git.".into());
    }
    git(dir, &["worktree", "prune"])
}

/// La branca té commits que NO s'han fusionat en cap altre? (és a dir: hi ha
/// feina que es perdria si l'esborréssim)
pub fn has_unmerged_commits(dir: &Path, branch: &str) -> bool {
    let cur = current_branch(dir).unwrap_or_default();
    let base = if cur.is_empty() {
        "HEAD".to_string()
    } else {
        cur
    };
    let count = git_raw(
        dir,
        &["rev-list", "--count", branch, "--not", &base],
    )
    .ok()
    .filter(|(ok, _)| *ok)
    .map(|(_, out)| out.trim().parse::<i64>().unwrap_or(0))
    .unwrap_or(0);
    count > 0
}

// ── Netejat en esborrar una carpeta ─────────────────────────────────────────

/// Neteja el que penja d'una carpeta que s'ha esborrat: els WORKTREES
/// registrats dins la carpeta i les seves branques.
///
/// La situació que ho fa necessari: la IA esborra «la carpeta del joc» i el
/// repositori es queda amb el registre d'un worktree que ja no existeix
/// (`git worktree list` el mostra com «prunable») i amb la branca
/// «noorbit-jam-…» per sempre. La següent vegada que l'usuari fa `git branch`
/// veu branques que no tenen res a veure amb el projecte.
///
/// Retorna què s'ha netejat i què s'ha conservat (amb el motiu).
pub fn cleanup_folder(folder: &Path, force: bool) -> Result<Cleanup, String> {
    let mut cleanup = Cleanup::default();
    // El repositori principal: pot ser el mateix `folder` (repo normal) o un
    // altre (el folder és un worktree d'aquest).
    let Some(main) = main_repo(folder) else {
        // No és un repo: no hi ha branques a netejar.
        return Ok(cleanup);
    };
    let Ok(wts) = worktrees(&main) else {
        return Ok(cleanup);
    };
    let cur = current_branch(&main).unwrap_or_default();
    for w in wts {
        // Només els worktrees que viuen DINS la carpeta esborrada (o que són
        // la carpeta mateixa: esborrar un worktree ha de netejar el seu
        // registre i la seva branca).
        let wp = Path::new(&w.path);
        if !same_path(wp, folder) && !is_inside(wp, folder) {
            continue;
        }
        // 1) Desregistra el worktree: si la carpeta ja no hi és, `prune`
        //    neteja el registre; si encara hi és (cas normal: esborrem la
        //    carpeta però git encara la veu), la treu amb `--force`.
        if w.exists {
            let _ = git_raw(&main, &["worktree", "remove", "--force", &w.path]);
        }
        let _ = git_raw(&main, &["worktree", "prune"]);
        // Només s'anuncia com a netejat si git JA no el llista: si el
        // `remove` ha fallat (worktree bloquejat, volum de només lectura o
        // un `.lock` en contenció amb un git concurrent) el registre queda
        // penjat, i dir el contrari amagaria el bug que aquesta neteja
        // pretén evitar.
        let encara_registrat = worktrees(&main)
            .map(|rest| rest.iter().any(|x| same_path(Path::new(&x.path), wp)))
            .unwrap_or(true);
        if !encara_registrat {
            cleanup.removed_worktrees.push(w.path.clone());
        }
        // 2) La seva branca: esborra-la si no té feina que es perdi.
        let Some(b) = w.branch.clone() else { continue };
        if b == cur {
            // La branca on som: no es toca (ara és impossible, però per si el
            // repositori és un altre).
            cleanup.kept_branches.push((
                b,
                "és la branca activa del repositori".to_string(),
            ));
            continue;
        }
        if !force && has_unmerged_commits(&main, &b) {
            cleanup
                .kept_branches
                .push((b, "té commits que no s'han fusionat".to_string()));
            continue;
        }
        let flag = if force { "-D" } else { "-d" };
        let (ok, _) = git_raw(&main, &["branch", flag, "--", &b])?;
        if ok {
            cleanup.removed_branches.push(b);
        } else {
            cleanup.kept_branches.push((
                b,
                "no s'ha pogut esborrar (té commits sense fusionar)".to_string(),
            ));
        }
    }
    // 3) Una branca que NOMÉS comparteix el nom amb la carpeta esborrada
    //    (p. ex. la carpeta «api» i la branca «feature/api») NO s'esborra:
    //    pot no tenir-hi cap relació, i es perdirien commits fusionats que
    //    només viuen allí. Les branques que sí que esborrem són les dels
    //    worktrees de dins la carpeta (pas 2). Aquí com a molt es REPORTEN
    //    com a candidates perquè l'usuari decideixi.
    if let Some(nom) = folder
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty() && n != "." && n != "..")
    {
        let candidates: Vec<String> = branches(&main)
            .unwrap_or_default()
            .into_iter()
            .filter(|b| b.name == nom || b.name.ends_with(&format!("/{nom}")))
            .filter(|b| b.name != cur)
            .map(|b| b.name)
            .collect();
        for b in candidates {
            if cleanup.removed_branches.contains(&b) || cleanup.kept_branches.iter().any(|(k, _)| k == &b) {
                continue;
            }
            cleanup.kept_branches.push((
                b,
                "coincideix amb el nom de la carpeta però no tenia cap worktree associat".to_string(),
            ));
        }
    }
    Ok(cleanup)
}

// ── Ajudes de ruta ─────────────────────────────────────────────────────────

/// Compara dues rutes pel seu camí normalitzat (resolent «.», «..» i
/// enllaços symlink quan es pot). No fa cap canvi al sistema de fitxers.
///
/// Resol els SYMLINKS del camí que existeix: a macOS `/tmp` és `/private/tmp`
/// i git informa dels worktrees SEMPRE amb el camí resolt; sense això, el
/// worktree dins `/tmp/…` no esuria reconegut com a «dins de la carpeta».
fn norm(p: &Path) -> PathBuf {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(p)
    };
    // Camí existent (o el seu ancestre existent més proper): el resolem.
    let mut sufix: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = abs.clone();
    loop {
        if let Ok(c) = std::fs::canonicalize(&cur) {
            let mut out = c;
            for s in sufix.into_iter().rev() {
                out.push(s);
            }
            return out;
        }
        match cur.file_name() {
            Some(name) => {
                sufix.push(name.to_owned());
                if !cur.pop() {
                    break;
                }
            }
            None => break,
        }
    }
    // Sense camí existent: normalització purament léxica.
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// `path` és el mateix que `other`?
fn same_path(a: &Path, b: &Path) -> bool {
    norm(a) == norm(b)
}

/// `child` viu dINS de `parent`? (i no és el mateix directori)
pub fn is_inside(child: &Path, parent: &Path) -> bool {
    let c = norm(child);
    let p = norm(parent);
    c != p && c.starts_with(&p)
}

#[cfg(test)]
mod proves {
    use super::*;

    /// Repositori temporal per a les proves: crea un repo amb un commit.
    fn repo_temporal(nom: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("noorbit-gitops-{}-{}", nom, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (ok, _) = git_raw(&dir, &["init", "-b", "main"]).unwrap();
        assert!(ok);
        std::fs::write(dir.join("a.txt"), "hola\n").unwrap();
        let _ = git_raw(&dir, &["add", "-A"]).unwrap();
        let _ = git_raw(
            &dir,
            &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-m", "init"],
        )
        .unwrap();
        dir
    }

    #[test]
    fn crea_i_llista_branques() {
        let d = repo_temporal("llista");
        create_branch(&d, "feature/x", None, false).unwrap();
        let bs = branches(&d).unwrap();
        assert_eq!(bs.len(), 2);
        let f = bs.iter().find(|b| b.name == "feature/x").unwrap();
        assert!(!f.current);
        assert!(bs.iter().any(|b| b.name == "main" && b.current));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn crea_i_canvia_de_branca() {
        let d = repo_temporal("checkout");
        create_branch(&d, "feature/y", None, true).unwrap();
        assert_eq!(current_branch(&d).unwrap(), "feature/y");
        // Canviar a una branca que no existeix: error clar.
        assert!(checkout_branch(&d, "no-existeix").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn no_esborra_la_branca_activa() {
        let d = repo_temporal("activa");
        let e = delete_branch(&d, "main", true).unwrap_err();
        assert!(e.contains("branca"), "{}", e);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn esborra_una_branca_fusionada() {
        let d = repo_temporal("esborrar");
        create_branch(&d, "temporal", None, false).unwrap();
        assert!(delete_branch(&d, "temporal", false).is_ok());
        assert!(branches(&d).unwrap().iter().all(|b| b.name != "temporal"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rebutja_noms_de_branca_amb_forma_de_flag() {
        let d = repo_temporal("flags");
        // «-a»/«-r» com a nom: no és una branca, és un flag de git (esborraria
        // totes les fusionades). S'ha de rebutjar ABANS d'executar res.
        assert!(delete_branch(&d, "-a", true).is_err());
        assert!(delete_branch(&d, "-r", false).is_err());
        assert!(create_branch(&d, "ok", Some("-a"), false).is_err());
        // El repo segueix intacte: cap branca extra, cap dany.
        assert_eq!(branches(&d).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn el_nom_de_branca_invalida_s_explica() {
        assert!(valid_branch_name(" bona ").is_ok());
        assert!(valid_branch_name("").is_err());
        assert!(valid_branch_name("-r").is_err());
        assert!(valid_branch_name("amb espai").is_err());
        assert!(valid_branch_name("a..b").is_err());
    }

    #[test]
    fn neteja_el_worktree_i_la_branca_duna_carpeta_esborrada() {
        let d = repo_temporal("worktree");
        let wt = d.join("wt-joc");
        let (ok, _) = git_raw(&d, &["worktree", "add", "-b", "noorbit-jam-1", &wt.to_string_lossy()])
            .unwrap();
        assert!(ok, "no s'ha pogut crear el worktree");
        // El worktree és dins el repo: netejar-lo HA de treure la branca.
        let c = cleanup_folder(&wt, false).unwrap();
        assert!(c.removed_worktrees.iter().any(|p| p.contains("wt-joc")));
        assert!(c.removed_branches.iter().any(|b| b == "noorbit-jam-1"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn conserva_la_branca_amb_feina_sin_fusionar() {
        let d = repo_temporal("sensefusionar");
        let wt = d.join("wt-feina");
        let (ok, _) = git_raw(&d, &["worktree", "add", "-b", "noorbit-jam-2", &wt.to_string_lossy()])
            .unwrap();
        assert!(ok);
        // Commit únic a la branca del worktree: no es pot perdre.
        std::fs::write(wt.join("b.txt"), "feina\n").unwrap();
        let _ = git_raw(&wt, &["add", "-A"]).unwrap();
        let _ = git_raw(
            &wt,
            &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-m", "feina"],
        )
        .unwrap();
        let c = cleanup_folder(&wt, false).unwrap();
        assert!(c.removed_branches.is_empty(), "{:?}", c);
        assert!(
            c.kept_branches.iter().any(|(b, _)| b == "noorbit-jam-2"),
            "{:?}",
            c
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn no_reporta_worktrees_que_git_no_ha_deixat_marxar() {
        let d = repo_temporal("lockwt");
        let wt = d.join("wt-lock");
        let (ok, _) = git_raw(&d, &["worktree", "add", "-b", "noorbit-lock", &wt.to_string_lossy()])
            .unwrap();
        assert!(ok);
        // Un worktree bloquejat fa fallar el `remove --force`: el registre
        // segueix penjat i no l'hem d'anunciar com a netejat.
        let (ok, _) = git_raw(&d, &["worktree", "lock", &wt.to_string_lossy()]).unwrap();
        assert!(ok);
        let c = cleanup_folder(&wt, false).unwrap();
        assert!(c.removed_worktrees.is_empty(), "{:?}", c);
        assert!(c.removed_branches.is_empty(), "{:?}", c);
        let _ = git_raw(&d, &["worktree", "unlock", &wt.to_string_lossy()]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn no_esborra_branques_que_nomes_comparteixen_el_nom() {
        let d = repo_temporal("nomcarpeta");
        // Branca sense cap relació amb la carpeta, nom coincident.
        create_branch(&d, "feature/api", None, false).unwrap();
        std::fs::create_dir_all(d.join("api")).unwrap();
        let c = cleanup_folder(&d.join("api"), false).unwrap();
        // No s'esborra: només es reporta com a candidate.
        assert!(c.removed_branches.is_empty(), "{:?}", c);
        assert!(
            c.kept_branches.iter().any(|(b, _)| b == "feature/api"),
            "{:?}",
            c
        );
        assert!(branches(&d).unwrap().iter().any(|b| b.name == "feature/api"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_path_dins_altre_no_es_treu_nada() {
        let d = repo_temporal("fora");
        let c = cleanup_folder(&d.join("no-existeix"), false).unwrap();
        assert!(c.removed_branches.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn detecta_si_un_path_es_dins_altre() {
        assert!(is_inside(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(!is_inside(Path::new("/a/b"), Path::new("/a/b")));
        assert!(!is_inside(Path::new("/a/bc"), Path::new("/a/b")));
    }
}