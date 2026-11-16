use std::path::{PathBuf};
use std::process::Stdio;
use std::time::Duration;
use async_trait::async_trait;
use tokio::process::Command;
use tracing::{info, warn};
use noorbit_jam::{Checkpointer, Task};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct GitCheckpointer {
    repo: PathBuf,
    timeout: Duration,
    clean_untracked: bool,
    user_name: String,
    user_email: String,
}
impl GitCheckpointer {
    pub fn new(repo: impl Into<PathBuf>) -> Result<Self, String> {
        let repo = repo.into();
        if !repo.exists() { return Err(format!("path no existeix: {}", repo.display())); }
        let ckpt = Self { repo, timeout: DEFAULT_TIMEOUT, clean_untracked: true, user_name: "NoOrbit Agent".into(), user_email: "agent@noorbit.local".into() };
        let check = std::process::Command::new("git").arg("-C").arg(&ckpt.repo).args(["rev-parse","--git-dir"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
        match check { Ok(s) if s.success() => Ok(ckpt), Ok(_) => Err(format!("no és un repo git: {}", ckpt.repo.display())), Err(e) => Err(format!("git no disponible: {e}")) }
    }
    pub fn with_timeout(mut self, t: Duration) -> Self { self.timeout = t; self }
    pub fn with_clean_untracked(mut self, yes: bool) -> Self { self.clean_untracked = yes; self }
    pub fn with_user(mut self, name: impl Into<String>, email: impl Into<String>) -> Self { self.user_name = name.into(); self.user_email = email.into(); self }
    async fn run_git(&self, args: &[&str]) -> Result<std::process::Output, String> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(&self.repo).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        let child = cmd.spawn().map_err(|e| format!("spawn git: {e}"))?;
        let out = tokio::time::timeout(self.timeout, child.wait_with_output()).await.map_err(|_| format!("git timeout"))?.map_err(|e| format!("wait git: {e}"))?;
        Ok(out)
    }
}
#[async_trait]
impl Checkpointer for GitCheckpointer {
    async fn head(&self) -> Result<String, String> {
        let out = self.run_git(&["rev-parse","HEAD"]).await?;
        if !out.status.success() { return Err(format!("git rev-parse fallà: {}", String::from_utf8_lossy(&out.stderr).trim())); }
        let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if sha.len() < 7 || !sha.chars().all(|c| c.is_ascii_hexdigit()) { return Err(format!("SHA inesperat: {sha}")); }
        Ok(sha)
    }
    async fn reset_hard(&self, sha: &str) -> Result<(), String> {
        if sha.len() < 7 || !sha.chars().all(|c| c.is_ascii_hexdigit()) { return Err(format!("SHA invàlid: {sha}")); }
        let out = self.run_git(&["reset","--hard",sha]).await?;
        if !out.status.success() { return Err(format!("git reset fallà: {}", String::from_utf8_lossy(&out.stderr).trim())); }
        if self.clean_untracked {
            let out = self.run_git(&["clean","-fd"]).await?;
            if !out.status.success() { warn!("git clean -fd: {}", String::from_utf8_lossy(&out.stderr).trim()); }
        }
        info!(sha, "reset_hard ok");
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
        info!(sha, task = %task.id, "commit ok");
        Ok(sha)
    }
    async fn tag(&self, name: &str) -> Result<(), String> {
        if name.is_empty() || name.contains('\n') || name.contains('\0') || name.starts_with('-') { return Err(format!("nom de tag invàlid: {name:?}")); }
        let out = self.run_git(&["tag","-f",name]).await?;
        if !out.status.success() { return Err(format!("git tag fallà: {}", String::from_utf8_lossy(&out.stderr).trim())); }
        info!(name, "tag ok");
        Ok(())
    }
    async fn cleanup(&self) -> Result<(), String> { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn init_repo() -> (TempDir, GitCheckpointer) {
        let dir = TempDir::new().unwrap();
        let path = dir.path();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(path)
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

        let ckpt = GitCheckpointer::new(path).unwrap();
        (dir, ckpt)
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
    async fn head_funciona() {
        let (_dir, ckpt) = init_repo();
        let sha = ckpt.head().await.unwrap();
        assert_eq!(sha.len(), 40);
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn commit_i_reset() {
        let (dir, ckpt) = init_repo();
        let base = ckpt.head().await.unwrap();

        fs::write(dir.path().join("new.txt"), "canvis").unwrap();
        let new_sha = ckpt.commit(&task("t01")).await.unwrap();
        assert_ne!(new_sha, base);

        ckpt.reset_hard(&base).await.unwrap();
        assert!(!dir.path().join("new.txt").exists(), "clean no ha esborrat");
    }

    #[tokio::test]
    async fn commit_sense_canvis_falla() {
        let (_dir, ckpt) = init_repo();
        let r = ckpt.commit(&task("t01")).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn reset_sha_invalid() {
        let (_dir, ckpt) = init_repo();
        assert!(ckpt.reset_hard("--hard").await.is_err());
        assert!(ckpt.reset_hard("xyz").await.is_err());
    }

    #[tokio::test]
    async fn tag_força_si_existeix() {
        let (_dir, ckpt) = init_repo();
        ckpt.tag("jam-final").await.unwrap();
        ckpt.tag("jam-final").await.unwrap(); // segona vegada ha de funcionar
    }

    #[tokio::test]
    async fn tag_invalid() {
        let (_dir, ckpt) = init_repo();
        assert!(ckpt.tag("--help").await.is_err());
        assert!(ckpt.tag("a\nb").await.is_err());
    }

    #[tokio::test]
    async fn constructor_rebutja_no_repo() {
        let dir = TempDir::new().unwrap();
        assert!(GitCheckpointer::new(dir.path()).is_err());
    }
}
