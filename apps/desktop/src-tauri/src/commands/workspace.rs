//! Gestió de l'arrel del workspace obert.

use once_cell::sync::Lazy;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use tauri::{command, AppHandle, Emitter, Manager};

static WORKSPACE_ROOT: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));

/// .gitignore de conveni per als projectes NOUS: sense ell, la Autonomous Jam
/// (que fa `git add -A` a cada tasca) ompliria l'historial de cachés
/// (`node_modules`, `.godot`, `__pycache__`…) i els commits serien enormes.
const DEFAULT_GITIGNORE: &str = "# SO i editors\n.DS_Store\nThumbs.db\n.idea/\n.vscode/\n\n# Dependències i cachés\nnode_modules/\n__pycache__/\n*.pyc\n.venv/\nvenv/\n\n# Motors de joc\n.godot/\n*.import\nLibrary/\nTemp/\nLogs/\nSaved/\nIntermediate/\nBuilds/\n\n# Sortides\ndist/build/\nbuild/\nexport/\n";

/// Inicialitza un repo git NET amb un primer commit. Es fa servir perquè la
/// Autonomous Jam VOL git per desfer feina (el checkpointer torna enrere amb
/// `reset --hard` i desa cada tasca com un commit), però JA NO és un
/// requisit de l'usuari: «Fitxer ▸ Nou projecte» el crida en crear-ne un, i
/// la Jam l'aplica sola quan obre un projecte sense repo.
///
/// MAI toca la configuració global de l'usuari: les dades de l'autor es passen
/// amb `-c` només per a aquest commit.
pub(crate) async fn init_git_repo(dir: &Path) -> Result<(), String> {
    // Git < 2.28 no coneix `--initial-branch`: s'intenta i, si falla, es fa un
    // `git init` pla (la branca per defecta és la que tingui l'usuari).
    if run_git(dir, vec!["init", "-q", "--initial-branch=main"])
        .await
        .is_err()
    {
        run_git(dir, vec!["init", "-q"]).await?;
    }
    // Sense `.gitignore` el `git add -A` de la Jam comitaria `node_modules`,
    // `.godot/` o `__pycache__/`. Només si no n'hi ha (els projectes nous en
    // porten un d'escriptura pròpia).
    let gi = dir.join(".gitignore");
    if !gi.exists() {
        let _ = std::fs::write(&gi, DEFAULT_GITIGNORE);
    }
    run_git(dir, vec!["add", "-A"]).await?;
    run_git(
        dir,
        vec![
            "-c",
            "user.name=NoOrbit",
            "-c",
            "user.email=noorbit@local",
            "commit",
            "-q",
            "--no-verify",
            "-m",
            "Projecte creat amb NoOrbit",
        ],
    )
    .await
}

/// La carpeta és JA un repo git aprofitable per a la Jam —amb un commit
/// base—? El control que fa `GitCheckpointer::new` només mira que el repo
/// EXISTEIXI, però el bucle de la Jam arrenca llegint `HEAD`: un repo creat
/// per `git init` sense cap commit (passa amb projectes d'alguns motors) no
/// en té i llavors `run_inner` se salta totes les tasques en silenci — la
/// Jam sembla viva però no escriu cap fitxer.
pub(crate) fn te_base_commit(dir: &Path) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Executa git a `dir` sense bloquejar el runtime (aquests comandos van dins
/// d'una `async fn` que corre al thread pool de Tauri).
async fn run_git(dir: &Path, args: Vec<&str>) -> Result<(), String> {
    let out = tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("git {}: {e}", args.first().copied().unwrap_or("?")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {} ha fallat: {}",
            args.first().copied().unwrap_or("?"),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// retorna l'arrel del workspace actual (si n'hi ha).
pub fn current_root() -> Option<PathBuf> {
    WORKSPACE_ROOT.read().ok().and_then(|g| g.clone())
}

/// resol un path relatiu a l'arrel del workspace.
pub fn resolve_in_root(path: &str) -> PathBuf {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        current_root().map(|r| r.join(p)).unwrap_or_else(|| p.to_path_buf())
    }
}

#[command]
pub async fn open_workspace(app: AppHandle, path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    // `is_dir()` tornava `false` TANT per «no existeix» COM per accés denegat
    // (p. ex. el permís TCC de l'Escriptori pendent després d'una re-firma):
    // el frontend no podia distingir una fallada definitiva d'una transitòria
    // i esborrava el lastRoot per sempre. NotFound manté el missatge històric
    // (contracte intacte); la resta d'errors d'accés en tenen un de diferent.
    match std::fs::metadata(&p) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(format!("La carpeta {} no existeix", path)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("La carpeta {} no existeix", path));
        }
        Err(e) => {
            return Err(format!("No s'ha pogut accedir a la carpeta {}: {}", path, e));
        }
    }
    // unwrap_or_else: si el lock està ENVERINAT (un panic en un altre fil), no
    // cal enfonsar tota l'app: es recupera el valor i es continua.
    match WORKSPACE_ROOT.write() {
        Ok(mut w) => *w = Some(p.clone()),
        Err(e) => *e.into_inner() = Some(p.clone()),
    }

    let title = format!("NoOrbit — {}", p.file_name().unwrap_or_default().to_string_lossy());
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title(&title);
    }

    let display = p.to_string_lossy().to_string();
    let _ = app.emit("workspace://opened", display.clone());
    Ok(display)
}

/// Crea un projecte nou: carpeta amb un README inicial, dins de `parent`
/// (per defecte ~/Projects). Retornarà el camí creat per obrir-lo com a workspace.
#[command]
pub async fn create_project(name: String, parent: Option<String>) -> Result<String, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("El nom del projecte no pot ser buit".into());
    }
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err("El nom no pot contenir bars".into());
    }

    let base = match parent {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => dirs::home_dir()
            .ok_or_else(|| "Carpeta personal no disponible".to_string())?
            .join("Projects"),
    };
    let dir = base.join(&name);
    if dir.exists() {
        return Err(format!("Ja existeix la carpeta {}", dir.display()));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("No s'ha pogut crear: {}", e))?;

    let readme = format!(
        "# {}\n\nProjecte creat amb NoOrbit.\n",
        name
    );
    let _ = std::fs::write(dir.join("README.md"), readme);
    let _ = std::fs::write(dir.join(".gitignore"), DEFAULT_GITIGNORE);

    // Repo git DES DEL PRIMER MOMENT: la Autonomous Jam necessita commits per
    // poder tornar enrere si una tasca trenca el projecte. Si no hi ha git, el
    // projecte igualment es crea (es avisa al log) i la Jam ho dirà quan
    // s'intenti arrencar.
    if let Err(e) = init_git_repo(&dir).await {
        eprintln!("create_project: {e}");
    }
    Ok(dir.to_string_lossy().to_string())
}

/// Clona la plantilla «Claude Code Game Studios» (donchitos/claude-code-game-studios)
/// a `dest` (carpeta que no ha d'existir). Retorna el camí creat.
#[command]
pub async fn clone_game_studios(name: String, parent: Option<String>) -> Result<String, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("El nom del projecte no pot ser buit".into());
    }
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err("El nom no pot contenir bars".into());
    }
    let base = match parent {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => dirs::home_dir()
            .ok_or_else(|| "Carpeta personal no disponible".to_string())?
            .join("Projects"),
    };
    let p = base.join(&name);
    if p.exists() {
        return Err(format!("Ja existeix la carpeta {}", p.display()));
    }
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("No s'ha pogut crear el pare: {}", e))?;
    }
    let dest = p.to_string_lossy().to_string();
    // `tokio::process`: amb el `std::process::Command::output()` bloquejant,
    // el CLON (xarxa, minuts segons la connexió) es menjava un fil del runtime
    // de Tauri i durant l'estona qualsevol altra comanda —incloent-hi el
    // streaming de la IA i el progrés de l'actualització— es quedava esperant.
    let out = tokio::process::Command::new("git")
        .args(["clone", "--depth", "1", "https://github.com/donchitos/claude-code-game-studios.git", &dest])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("No s'ha pogut executar git: {}", e))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("git clone ha fallat: {}", stderr.trim()));
    }
    // Elimina el .git perquè sigui una plantilla neta (no un clone lligat) i en
    // crea un de NOU amb un primer commit: si no, el projecte neix sense repo i
    // la Autonomous Jam el rebutja («el projecte ha de ser un repositori git»).
    let gitdir = p.join(".git");
    let _ = std::fs::remove_dir_all(&gitdir);
    // La plantilla ja porta el seu `.gitignore`: només s'afegeix el de NoOrbit si
    // no n'hi ha cap (si no, es esborrarien les regles de l'autor).
    let gi = p.join(".gitignore");
    if !gi.exists() {
        let _ = std::fs::write(&gi, DEFAULT_GITIGNORE);
    }
    if let Err(e) = init_git_repo(&p).await {
        eprintln!("clone_game_studios: {e}");
    }
    Ok(p.to_string_lossy().to_string())
}
