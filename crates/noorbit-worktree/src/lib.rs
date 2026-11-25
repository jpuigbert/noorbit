use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use async_trait::async_trait;
use tokio::process::Command;
use tracing::{info, warn};
use noorbit_jam::{Checkpointer, Task};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct GitWorktreeCheckpointer {
    main_repo: PathBuf,
    worktree: PathBuf,
    branch: String,
    timeout: Duration,
    user_name: String,
    user_email: String,
    cleaned: AtomicBool,
}
impl GitWorktreeCheckpointer {
    pub async fn new(main_repo: impl Into<PathBuf>, base_sha: Option<String>) -> Result<Self, String> {
        let main_repo = main_repo.into();
        if !main_repo.exists() { return Err(format!("repo no existeix: {}", main_repo.display())); }
        let check = std::process::Command::new("git").arg("-C").arg(&main_repo).args(["rev-parse","--git-dir"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
        match check { Ok(s) if s.success() => {}, Ok(_) => return Err(format!("no és un repo git: {}", main_repo.display())), Err(e) => return Err(format!("git no disponible: {e}")) }
        let base = match base_sha {
            Some(s) => { if s.len() < 7 || !s.chars().all(|c| c.is_ascii_hexdigit()) { return Err(format!("base_sha invàlid: {s}")); } s }
            None => {
                let out = std::process::Command::new("git").arg("-C").arg(&main_repo).args(["rev-parse","HEAD"]).output().map_err(|e| format!("rev-parse HEAD: {e}"))?;
                if !out.status.success() { return Err(format!("repo buit? {}", String::from_utf8_lossy(&out.stderr).trim())); }
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            }
        };
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let id = format!("{}-{}", std::process::id(), nanos);
        let branch = format!("noorbit-jam-{id}");
        let worktree = std::env::temp_dir().join(format!("noorbit-jam-{id}"));
        if worktree.exists() { return Err(format!("worktree path ja existeix: {}", worktree.display())); }
        if worktree.starts_with(&main_repo) { return Err(format!("worktree path dins del repo principal: {}", worktree.display())); }
        // El `worktree add` pot fallar per un lock fals o transitari (a CI
        // s'ha vist: «index.lock: File exists» en repositoris acabats de
        // crear). Reintentem netejant les restes de l'intent mig fet:
        // el directori fantasma, la branca òrfena i les metadades orfes.
        let mut ultim_error = String::new();
        let mut creat = false;
        for intent in 1..=3u64 {
            let out = match std::process::Command::new("git")
                .arg("-C")
                .arg(&main_repo)
                .args(["worktree", "add", "-b", &branch, worktree.to_str().ok_or("worktree path no UTF-8")?, &base])
                .output()
            {
                Ok(o) => o,
                Err(e) => return Err(format!("git worktree add: {e}")),
            };
            if out.status.success() {
                creat = true;
                break;
            }
            ultim_error = String::from_utf8_lossy(&out.stderr).trim().to_string();
            warn!(intent, err = %ultim_error, "git worktree add fallà; netejant i reintentant");
            let _ = std::process::Command::new("git").arg("-C").arg(&main_repo).args(["worktree", "prune"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
            let _ = std::fs::remove_dir_all(&worktree);
            let _ = std::process::Command::new("git").arg("-C").arg(&main_repo).args(["branch", "-D", "--", &branch]).stdout(Stdio::null()).stderr(Stdio::null()).status();
            if intent < 3 { std::thread::sleep(Duration::from_millis(150 * intent)); }
        }
        if !creat { return Err(format!("git worktree add fallà: {ultim_error}")); }
        info!(main = %main_repo.display(), worktree = %worktree.display(), branch = %branch, base = %base, "worktree creat");
        Ok(Self { main_repo, worktree, branch, timeout: DEFAULT_TIMEOUT, user_name: "NoOrbit Agent".into(), user_email: "agent@noorbit.local".into(), cleaned: AtomicBool::new(false) })
    }
    pub fn worktree_path(&self) -> &Path { &self.worktree }
    pub fn branch(&self) -> &str { &self.branch }
    pub fn with_timeout(mut self, t: Duration) -> Self { self.timeout = t; self }
    pub fn with_user(mut self, name: impl Into<String>, email: impl Into<String>) -> Self { self.user_name = name.into(); self.user_email = email.into(); self }
    async fn run_git(&self, args: &[&str]) -> Result<std::process::Output, String> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(&self.worktree).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        let child = cmd.spawn().map_err(|e| format!("spawn git: {e}"))?;
        let out = tokio::time::timeout(self.timeout, child.wait_with_output()).await.map_err(|_| format!("git timeout"))?.map_err(|e| format!("wait git: {e}"))?;
        Ok(out)
    }
    pub async fn cleanup(&self) -> Result<(), String> {
        if self.cleaned.swap(true, Ordering::SeqCst) { return Ok(()); }
        let out = Command::new("git").arg("-C").arg(&self.main_repo).args(["worktree","remove","--force"]).arg(&self.worktree).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).output().await.map_err(|e| format!("git worktree remove: {e}"))?;
        if !out.status.success() { warn!("git worktree remove fallà: {}", String::from_utf8_lossy(&out.stderr).trim()); }
        let out = Command::new("git").arg("-C").arg(&self.main_repo).args(["branch","-D",&self.branch]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).output().await.map_err(|e| format!("git branch -D: {e}"))?;
        if !out.status.success() { warn!("git branch -D fallà: {}", String::from_utf8_lossy(&out.stderr).trim()); }
        info!("worktree netejat");
        Ok(())
    }
}
impl Drop for GitWorktreeCheckpointer {
    fn drop(&mut self) {
        if self.cleaned.load(Ordering::SeqCst) { return; }
        let _ = std::process::Command::new("git").arg("-C").arg(&self.main_repo).args(["worktree","remove","--force"]).arg(&self.worktree).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
        let _ = std::process::Command::new("git").arg("-C").arg(&self.main_repo).args(["branch","-D",&self.branch]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}
#[async_trait]
impl Checkpointer for GitWorktreeCheckpointer {
    async fn head(&self) -> Result<String, String> {
        let out = self.run_git(&["rev-parse","HEAD"]).await?;
        if !out.status.success() { return Err(format!("rev-parse HEAD fallà: {}", String::from_utf8_lossy(&out.stderr).trim())); }
        let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if sha.len() < 7 || !sha.chars().all(|c| c.is_ascii_hexdigit()) { return Err(format!("SHA inesperat: {sha}")); }
        Ok(sha)
    }
    async fn reset_hard(&self, sha: &str) -> Result<(), String> {
        if sha.len() < 7 || !sha.chars().all(|c| c.is_ascii_hexdigit()) { return Err(format!("SHA invàlid: {sha}")); }
        let out = self.run_git(&["reset","--hard",sha]).await?;
        if !out.status.success() { return Err(format!("git reset fallà: {}", String::from_utf8_lossy(&out.stderr).trim())); }
        let out = self.run_git(&["clean","-fd"]).await?;
        if !out.status.success() { warn!("git clean -fd: {}", String::from_utf8_lossy(&out.stderr).trim()); }
        info!(sha, "worktree reset_hard ok");
        Ok(())
    }
    async fn commit(&self, task: &Task) -> Result<String, String> {
        let out = self.run_git(&["add","-A"]).await?;
        if !out.status.success() { return Err(format!("git add fallà: {}", String::from_utf8_lossy(&out.stderr).trim())); }
        let out = self.run_git(&["status","--porcelain"]).await?;
        if !out.status.success() { return Err(format!("git status fallà: {}", String::from_utf8_lossy(&out.stderr).trim())); }
        if out.stdout.is_empty() { return Err("no hi ha canvis a commitar".into()); }
        let safe_title: String = task.title.chars().map(|c| if c=='\n'||c=='\r'||c=='\0' {' '} else {c}).collect();
        let msg = format!("[jam] {} ({})", safe_title.trim(), task.id);
        let out = self.run_git(&["-c", &format!("user.name={}", self.user_name), "-c", &format!("user.email={}", self.user_email), "commit","--no-verify","-m",&msg]).await?;
        if !out.status.success() { return Err(format!("git commit fallà: {}", String::from_utf8_lossy(&out.stderr).trim())); }
        let sha = self.head().await?;
        info!(sha, task = %task.id, "worktree commit ok");
        Ok(sha)
    }
    async fn tag(&self, name: &str) -> Result<(), String> {
        if name.is_empty() || name.contains('\n') || name.contains('\0') || name.starts_with('-') { return Err(format!("nom de tag invàlid: {name:?}")); }
        let out = self.run_git(&["tag","-f",name]).await?;
        if !out.status.success() { return Err(format!("git tag fallà: {}", String::from_utf8_lossy(&out.stderr).trim())); }
        info!(name, "worktree tag ok");
        Ok(())
    }
    async fn cleanup(&self) -> Result<(), String> { GitWorktreeCheckpointer::cleanup(self).await }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn init_repo() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&path)
                .args(args)
                .output()
                .expect("git");
            assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
        };
        run(&["init", "-q"]);
        run(&["config", "user.name", "Test"]);
        run(&["config", "user.email", "test@local"]);
        fs::write(path.join("README.md"), "hello").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "init"]);
        (dir, path)
    }

    fn task(id: &str) -> Task {
        Task {
            id: id.to_string(),
            title: format!("task {id}"),
            description: "d".into(),
            acceptance: vec!["ok".into()],
            phase: None,
        }
    }

    #[tokio::test]
    async fn crea_worktree_i_head() {
        let (_dir, repo) = init_repo();
        let main_sha = {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };

        let ckpt = GitWorktreeCheckpointer::new(&repo, None).await.unwrap();
        assert!(ckpt.worktree_path().exists());
        let head = ckpt.head().await.unwrap();
        assert_eq!(head, main_sha);

        ckpt.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn commit_al_worktree_no_afecta_el_repo_principal() {
        let (_dir, repo) = init_repo();
        let main_head_before = {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };

        let ckpt = GitWorktreeCheckpointer::new(&repo, None).await.unwrap();
        fs::write(ckpt.worktree_path().join("new.txt"), "x").unwrap();
        let new_sha = ckpt.commit(&task("t01")).await.unwrap();
        assert_ne!(new_sha, main_head_before);

        // El repo principal NO ha canviat de HEAD.
        let main_head_after = {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(main_head_before, main_head_after);

        ckpt.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn cleanup_es_idempotent() {
        let (_dir, repo) = init_repo();
        let ckpt = GitWorktreeCheckpointer::new(&repo, None).await.unwrap();
        ckpt.cleanup().await.unwrap();
        ckpt.cleanup().await.unwrap(); // segona vegada OK
    }

    #[tokio::test]
    async fn drop_netega_worktree() {
        let (_dir, repo) = init_repo();
        let wt_path;
        {
            let ckpt = GitWorktreeCheckpointer::new(&repo, None).await.unwrap();
            wt_path = ckpt.worktree_path().to_path_buf();
            assert!(wt_path.exists());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(!wt_path.exists(), "worktree no s'ha netejat al Drop");
    }

    #[tokio::test]
    async fn rebutja_base_sha_invalid() {
        let (_dir, repo) = init_repo();
        assert!(
            GitWorktreeCheckpointer::new(&repo, Some("--hard".into()))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn rebutja_no_repo() {
        let dir = TempDir::new().unwrap();
        assert!(
            GitWorktreeCheckpointer::new(dir.path(), None)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn commit_sense_canvis_falla() {
        let (_dir, repo) = init_repo();
        let ckpt = GitWorktreeCheckpointer::new(&repo, None).await.unwrap();
        assert!(ckpt.commit(&task("t01")).await.is_err());
        ckpt.cleanup().await.unwrap();
    }
}
