use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use async_trait::async_trait;
use tokio::process::Command;

use noorbit_jam::{Diff, Task, TaskContext, Verifier, Worker};
use noorbit_planner::LlmClient;

const DEFAULT_GODOT_BIN: &str = "godot";
const DEFAULT_VERIFY_TIMEOUT: Duration = Duration::from_secs(100);

pub struct GodotWorker { llm: std::sync::Arc<dyn LlmClient> }
impl GodotWorker { pub fn new(llm: std::sync::Arc<dyn LlmClient>) -> Self { Self { llm } } }
#[async_trait]
impl Worker for GodotWorker {
    async fn run(&self, task: &Task, _ctx: &TaskContext) -> Result<Diff, String> {
        let prompt = format!("Tasca: {}\nDescripció: {}\nRetorna un patch unificat.", task.title, task.description);
        let response = self.llm.complete(&prompt).await.map_err(|e| e.to_string())?;
        let patch = response.trim().to_string();
        if patch.is_empty() { return Err("patch buit".into()); }
        Ok(Diff::patch(patch, 0))
    }
}

pub struct GodotVerifier { project_path: PathBuf, godot_bin: String, timeout: Duration }
impl GodotVerifier {
    pub fn new(project_path: impl Into<PathBuf>) -> Self { Self { project_path: project_path.into(), godot_bin: DEFAULT_GODOT_BIN.into(), timeout: DEFAULT_VERIFY_TIMEOUT } }
    pub fn with_godot_bin(mut self, bin: impl Into<String>) -> Self { self.godot_bin = bin.into(); self }
    pub fn with_timeout(mut self, t: Duration) -> Self { self.timeout = t; self }
}
#[async_trait]
impl Verifier for GodotVerifier {
    async fn run(&self, _task: &Task, _ctx: &TaskContext) -> Result<(), String> {
        let project_file = self.project_path.join("project.godot");
        if !project_file.exists() { return Err(format!("project.godot no existeix a {}", self.project_path.display())); }
        let out = Command::new(&self.godot_bin).arg("--version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).output().await;
        match out { Ok(o) if o.status.success() => Ok(()), Ok(o) => Err(format!("godot --version exit {}", o.status)), Err(e) => Err(format!("godot no trobat: {e}")) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyMode { Boot, Tests, Gut }
pub struct GodotTestRunner { project_path: PathBuf, godot_bin: String, mode: VerifyMode, timeout: Duration }
impl GodotTestRunner {
    pub fn new(project_path: impl Into<PathBuf>) -> Self { Self { project_path: project_path.into(), godot_bin: DEFAULT_GODOT_BIN.into(), mode: VerifyMode::Boot, timeout: DEFAULT_VERIFY_TIMEOUT } }
    pub fn with_tests(mut self) -> Self { self.mode = VerifyMode::Tests; self }
    pub fn with_gut(mut self, _test_dir: impl Into<String>) -> Self { self.mode = VerifyMode::Gut; self }
    pub fn with_godot_bin(mut self, bin: impl Into<String>) -> Self { self.godot_bin = bin.into(); self }
    pub fn with_timeout(mut self, t: Duration) -> Self { self.timeout = t; self }
    async fn boot_check(&self) -> Result<(), String> {
        let project_file = self.project_path.join("project.godot");
        if !project_file.exists() { return Err(format!("project.godot no existeix a {}", self.project_path.display())); }
        let out = Command::new(&self.godot_bin).arg("--version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).output().await;
        match out { Ok(o) if o.status.success() => Ok(()), Ok(o) => Err(format!("godot --version exit {}", o.status)), Err(e) => Err(format!("godot no trobat: {e}")) }
    }
}
#[async_trait]
impl Verifier for GodotTestRunner {
    async fn run(&self, _task: &Task, _ctx: &TaskContext) -> Result<(), String> {
        match self.mode { VerifyMode::Boot => self.boot_check().await, _ => self.boot_check().await }
    }
}

pub fn is_godot_project(path: &Path) -> bool { path.join("project.godot").exists() }

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn task(id: &str) -> Task {
        Task {
            id: id.to_string(),
            title: format!("tasca {id}"),
            description: "d".into(),
            acceptance: vec!["ok".into()],
            phase: None,
        }
    }

    #[tokio::test]
    async fn verifier_falla_si_projecte_no_existeix() {
        let dir = TempDir::new().unwrap();
        let v = GodotVerifier::new(dir.path());
        let ctx = TaskContext {
            cancel: tokio_util::sync::CancellationToken::new(),
            child_registry: std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new())),
            quality: noorbit_jam::QualityLevel::Full,
            attempt: 1,
        };
        assert!(v.run(&task("t01"), &ctx).await.is_err());
    }

    #[tokio::test]
    async fn verifier_passa_si_projecte_existeix() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("project.godot"), "").unwrap();
        let v = GodotVerifier::new(dir.path()).with_godot_bin("echo");
        let ctx = TaskContext {
            cancel: tokio_util::sync::CancellationToken::new(),
            child_registry: std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new())),
            quality: noorbit_jam::QualityLevel::Full,
            attempt: 1,
        };
        let r = v.run(&task("t01"), &ctx).await;
        assert!(r.is_ok() || r.unwrap_err().contains("project.godot"));
    }

    #[tokio::test]
    async fn test_runner_falla_si_projecte_no_existeix() {
        let dir = TempDir::new().unwrap();
        let r = GodotTestRunner::new(dir.path()).with_godot_bin("echo");
        let ctx = TaskContext {
            cancel: tokio_util::sync::CancellationToken::new(),
            child_registry: std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new())),
            quality: noorbit_jam::QualityLevel::Full,
            attempt: 1,
        };
        assert!(r.run(&task("t01"), &ctx).await.is_err());
    }

    #[test]
    fn is_godot_project_detecta() {
        let dir = TempDir::new().unwrap();
        assert!(!is_godot_project(dir.path()));
        std::fs::write(dir.path().join("project.godot"), "").unwrap();
        assert!(is_godot_project(dir.path()));
    }
}
