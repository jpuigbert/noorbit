//! Integració de Git al projecte obert, estil VS Code: estat dels canvis,
//! diff de cada fitxer, commit, push, pull, stash i gràfic de branques.
//!
//! Decisió d'implementació: en lloc del crate `git2` (que arrossega compilar
//! libgit2 C i infla el binari), usem el binari `git` del sistema, que a
//! macOS sempre hi és. Són comandes curtes i bloquejants, per això totes
//! corren amb `spawn_blocking`.

use serde::Serialize;
use std::path::Path;
use std::process::Command;
use tauri::command;

/// Una fila de `git status --porcelain` traduïda a alguna cosa llegible.
#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub path: String,
    /// Codi curt d'estat: M (modificat), A (afegit), D (esborrat),
    /// U (sense seguiment), R (reanomenat), C (conflicte).
    pub status: String,
    pub staged: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitStatus {
    /// `false` si la carpeta oberta no és un repositori.
    pub repo: bool,
    pub branch: String,
    pub changes: Vec<Change>,
}

/// Resultat de `git diff` per a un fitxer: el pedaç, la versió de HEAD i la
/// versió de treball, perquè la UI pugui mostrar el costat a costat de Monaco
/// (el mateix motor de diff que fa servir VS Code).
#[derive(Debug, Clone, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub patch: String,
    pub head_content: String,
    pub working_content: String,
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git: {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("git {} va fallar", args.first().copied().unwrap_or(""))
        } else {
            err
        })
    }
}

fn root_or_err() -> Result<std::path::PathBuf, String> {
    super::workspace::current_root().ok_or_else(|| "No hi ha cap projecte obert.".to_string())
}

/// Converteix el codi porcelànic (XY) en un estat curt per mostrar.
fn translate(xy: &str) -> (String, bool) {
    let x = xy.as_bytes().first().copied().unwrap_or(b' ');
    let y = xy.as_bytes().get(1).copied().unwrap_or(b' ');
    let staged = !matches!(x, b' ' | b'?');
    let status = match (x, y) {
        (b'?', _) => "U",
        (_, b'D') | (b'D', _) => "D",
        (_, b'A') | (b'A', _) => "A",
        (_, b'R') | (b'R', _) => "R",
        (_, b'C') | (b'C', _) => "C",
        _ => "M",
    };
    (status.to_string(), staged)
}

/// Estat del repositori: branca + fitxers canviats. Si no és un repositori
/// retorna `repo: false` (la UI oferirà iniciarlo).
#[command]
pub async fn git_status() -> Result<GitStatus, String> {
    let root = root_or_err()?;
    tokio::task::spawn_blocking(move || {
        if run_git(&root, &["rev-parse", "--is-inside-work-tree"]).is_err() {
            return Ok(GitStatus {
                repo: false,
                branch: String::new(),
                changes: vec![],
            });
        }
        let branch = run_git(&root, &["rev-parse", "--abbrev-ref", "HEAD"])
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "?".into());
        let porcelain = run_git(&root, &["status", "--porcelain"]).unwrap_or_default();
        let changes = porcelain
            .lines()
            .filter_map(|line| {
                if line.len() < 4 {
                    return None;
                }
                let xy = &line[..2];
                // Per files reanomenats (R old -> new) prenem el destí.
                let mut p = line[3..].trim().trim_matches('"').to_string();
                if let Some((_, to)) = p.split_once(" -> ") {
                    p = to.trim_matches('"').to_string();
                }
                let (status, staged) = translate(xy);
                Some(Change {
                    path: p,
                    status,
                    staged,
                })
            })
            .collect();
        Ok(GitStatus {
            repo: true,
            branch,
            changes,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Diff d'un fitxer concret. Per a fitxers sense seguiment el pedaç és buit
/// i `head_content` buida (tot el contingut és «nou»).
#[command]
pub async fn git_diff_file(path: String) -> Result<FileDiff, String> {
    let root = root_or_err()?;
    tokio::task::spawn_blocking(move || {
        let rel = path
            .strip_prefix(&format!("{}/", root.display()))
            .unwrap_or(&path)
            .to_string();
        let tracked = run_git(&root, &["ls-files", "--error-unmatch", &rel]).is_ok();
        let (patch, head_content) = if tracked {
            let patch =
                run_git(&root, &["diff", "HEAD", "--", &rel]).unwrap_or_else(|_| String::new());
            let head =
                run_git(&root, &["show", &format!("HEAD:{}", rel)]).unwrap_or_else(|_| String::new());
            (patch, head)
        } else {
            (String::new(), String::new())
        };
        let working = std::fs::read_to_string(root.join(&rel)).unwrap_or_default();
        Ok(FileDiff {
            path: rel,
            patch,
            head_content,
            working_content: working,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Desa tots els canvis (`git add -A` + `git commit -m missatge`).
#[command]
pub async fn git_commit(message: String) -> Result<String, String> {
    let root = root_or_err()?;
    if message.trim().is_empty() {
        return Err("Cal escriure un missatge de commit.".into());
    }
    tokio::task::spawn_blocking(move || {
        run_git(&root, &["add", "-A"])?;
        run_git(&root, &["commit", "-m", message.trim()])
    })
    .await
    .map_err(|e| e.to_string())?
}

#[command]
pub async fn git_push() -> Result<String, String> {
    let root = root_or_err()?;
    tokio::task::spawn_blocking(move || {
        // Si encara no remota, la lligam a origin/master (o la branca activa).
        let branch = run_git(&root, &["rev-parse", "--abbrev-ref", "HEAD"])
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "main".into());
        let has_remote = run_git(&root, &["remote"]).map(|s| !s.trim().is_empty())?;
        if !has_remote {
            return Err("El repositori no té cap remot configurat (git remote add origin …).".into());
        }
        run_git(&root, &["push", "-u", "origin", &branch])
    })
    .await
    .map_err(|e| e.to_string())?
}

#[command]
pub async fn git_pull() -> Result<String, String> {
    let root = root_or_err()?;
    tokio::task::spawn_blocking(move || run_git(&root, &["pull"]))
        .await
        .map_err(|e| e.to_string())?
}

/// Inicia un repositori nou a la carpeta oberta.
#[command]
pub async fn git_init() -> Result<String, String> {
    let root = root_or_err()?;
    tokio::task::spawn_blocking(move || {
        let out = run_git(&root, &["init"])?;
        // Un .gitignore bàsic evita sorolla (target/, node_modules/…).
        let gi = root.join(".gitignore");
        if !gi.exists() {
            let _ = std::fs::write(
                &gi,
                "target/\nnode_modules/\ndist/\n__pycache__/\n.venv/\n.DS_Store\n",
            );
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

// --- Stash (desar/restaurar canvis temporals), com VS Code -------------------

/// Una entrada de `git stash list`.
#[derive(Debug, Clone, Serialize)]
pub struct StashEntry {
    /// Índex numeral (0 = el més recent); serveix per a `pop`/`apply`.
    pub index: usize,
    /// Descripció del stash (branca + missatge, si en tenia).
    pub message: String,
}

/// Guarda els canvis no committats en un stash. `message` opcional.
#[command]
pub async fn git_stash(message: Option<String>) -> Result<String, String> {
    let root = root_or_err()?;
    tokio::task::spawn_blocking(move || {
        // Sense canvis, git stash no fa res: ho avisem en lloc de callar.
        let dirty = run_git(&root, &["status", "--porcelain"])?;
        if dirty.trim().is_empty() {
            return Err("No hi ha canvis per guardar (stash).".into());
        }
        let msg = message.unwrap_or_default();
        let msg = msg.trim();
        if msg.is_empty() {
            run_git(&root, &["stash", "push"])
        } else {
            run_git(&root, &["stash", "push", "-m", msg])
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Llista els stash guardats (més recent primer).
#[command]
pub async fn git_stash_list() -> Result<Vec<StashEntry>, String> {
    let root = root_or_err()?;
    tokio::task::spawn_blocking(move || {
        let out = run_git(&root, &["stash", "list"]).unwrap_or_default();
        let entries = out
            .lines()
            .enumerate()
            .map(|(i, line)| {
                // Format: «stash@{N}: <tipus> on <branca>: <missatge>».
                let msg = line
                    .split_once(": ")
                    .map(|(_, rest)| rest.to_string())
                    .unwrap_or_else(|| line.to_string());
                StashEntry { index: i, message: msg }
            })
            .collect();
        Ok(entries)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Recupera i esborra un stash (`git stash pop`). `index` per defecte 0.
#[command]
pub async fn git_stash_pop(index: Option<usize>) -> Result<String, String> {
    let root = root_or_err()?;
    tokio::task::spawn_blocking(move || {
        let i = index.unwrap_or(0).to_string();
        let reference = format!("stash@{{{i}}}");
        run_git(&root, &["stash", "pop", &reference])
    })
    .await
    .map_err(|e| e.to_string())?
}

// --- Gràfic de branques (historial), com la vista de VS Code -----------------

/// Retorna el text ASCII de `git log --graph` (fins a `limit` commits, totes
/// les branques). La UI el mostra en un bloc monoespaiat: és el mateix
/// dibuix del historial que fa el terminal de Git, sense parses fràgils.
#[command]
pub async fn git_log(limit: Option<usize>) -> Result<String, String> {
    let root = root_or_err()?;
    let n = limit.unwrap_or(60).clamp(1, 400).to_string();
    tokio::task::spawn_blocking(move || {
        if run_git(&root, &["rev-parse", "--git-dir"]).is_err() {
            return Err("Això no és un repositori Git.".into());
        }
        // `--decorate` mostra els noms de branca/tag al costat del commit.
        run_git(
            &root,
            &[
                "log",
                "--graph",
                "--oneline",
                "--decorate",
                "--all",
                "--color=never",
                &format!("-n{n}"),
            ],
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

// ─── Núvol: feina en equip des de diversos dispositius ──────────────────────
// Un projecte connectat a un repositori del núvol (GitHub, GitLab, un Gitea
// propi…) pot ser editat EN PARAL·LEL per més d'un NoOrbit: cada dispositiu
// baixa el que els altres han fet i puja el seu. La sincronització pot ser
// manual (1 clic) o automàtica (el frontend ho crida cada interval).

/// On apunta el projecte: la URL del remot `origin`, si en té.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteInfo {
    pub remote: bool,
    pub url: Option<String>,
}

/// URL del remot `origin` del projecte (cap si encara no està connectat).
#[command]
pub async fn git_remote_get() -> Result<RemoteInfo, String> {
    let root = root_or_err()?;
    tokio::task::spawn_blocking(move || {
        let has = run_git(&root, &["remote"]).map(|s| !s.trim().is_empty())?;
        if !has {
            return Ok(RemoteInfo {
                remote: false,
                url: None,
            });
        }
        let url = run_git(&root, &["remote", "get-url", "origin"])
            .ok()
            .map(|s| s.trim().to_string());
        Ok(RemoteInfo {
            remote: true,
            url,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// CONNECTA el projecte a un repositori del núvol (el pas de 1 clic per
/// treballar en equip): si no hi ha remot, `remote add origin URL`; si ja
/// n'hi ha, canvia la URL. A partir d'açí tots els NoOrbit que apunten al
/// mateix repositori editen el mateix projecte.
#[command]
pub async fn git_remote_set(url: String) -> Result<String, String> {
    let root = root_or_err()?;
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("Cal la URL del repositori (https://… o git@…).".into());
    }
    tokio::task::spawn_blocking(move || {
        if run_git(&root, &["rev-parse", "--git-dir"]).is_err() {
            run_git(&root, &["init"])?;
        }
        let has = run_git(&root, &["remote"]).map(|s| !s.trim().is_empty())?;
        if has {
            run_git(&root, &["remote", "set-url", "origin", &url])
        } else {
            run_git(&root, &["remote", "add", "origin", &url])
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Quant queda per sincronitzar: commits locals que falten per pujar (ahead)
/// i commits del núvol que falten per baixar (behind). Sense connexió o sense
/// upstream torna zeros: no bloqueja mai la UI.
#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub remote: bool,
    pub ahead: i64,
    pub behind: i64,
}

#[command]
pub async fn git_sync_status() -> Result<SyncStatus, String> {
    let root = root_or_err()?;
    tokio::task::spawn_blocking(move || {
        let has = run_git(&root, &["remote"]).map(|s| !s.trim().is_empty())?;
        if !has {
            return Ok(SyncStatus {
                remote: false,
                ahead: 0,
                behind: 0,
            });
        }
        // Baixa silenciosa de referències: sense això els comptadors no saben
        // el que s'han fet els altres dispositius. Si no hi ha xarxa, seguim
        // amb el que sabem (no és un error).
        let _ = run_git(&root, &["fetch", "--quiet"]);
        let branch = run_git(&root, &["rev-parse", "--abbrev-ref", "HEAD"])
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "main".into());
        let counts = run_git(
            &root,
            &["rev-list", "--left-right", "--count", &format!("origin/{branch}...HEAD")],
        )
        .map(|s| s.trim().to_string())
        .ok();
        // Format: «<darrere>\t<davant>».
        let mut st = SyncStatus {
            remote: true,
            ahead: 0,
            behind: 0,
        };
        if let Some(c) = counts {
            let mut it = c.split_whitespace();
            st.behind = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            st.ahead = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        }
        Ok(st)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// SINCRONITZACIÓ COMPLETA d'equip: commit dels canvis (si n'hi ha) + baixa
/// del que els altres han fet (`pull --rebase` amb autostash, per no perdre
/// res a mig fer) + pujada. És la passada que fa el mode automàtic cada
/// interval i el botó «Sincronitza» a mida: cap dispositiu queda sense pujar.
#[command]
pub async fn git_sync(message: Option<String>) -> Result<String, String> {
    let root = root_or_err()?;
    tokio::task::spawn_blocking(move || {
        let branch = run_git(&root, &["rev-parse", "--abbrev-ref", "HEAD"])
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "main".into());
        let mut out = String::new();
        // 1) Commit dels canvis locals (si n'hi ha) amb el missatge donat o
        //    un genèric d'equip.
        let dirty = run_git(&root, &["status", "--porcelain"])?;
        if !dirty.trim().is_empty() {
            let msg = message
                .as_deref()
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .unwrap_or("Sincronització d'equip (NoOrbit)");
            run_git(&root, &["add", "-A"])?;
            out.push_str(&run_git(&root, &["commit", "-m", msg])?);
        }
        // 2) Baixa del que han fet els altres. `--rebase --autostash` aplica
        //    el nostre treball NO committat a sobre sense perdre-lo.
        let upstream = run_git(&root, &["rev-parse", "--verify", "--quiet", &format!("origin/{branch}")]).is_ok();
        if upstream {
            out.push_str(&run_git(&root, &["pull", "--rebase", "--autostash"])?);
        }
        // 3) Puja: si la branca encara no té upstream, la lligam a origin.
        out.push_str(&run_git(&root, &["push", "-u", "origin", &branch])?);
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}
