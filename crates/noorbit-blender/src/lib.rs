use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;
use tokio::process::Command;
use noorbit_jam::{Diff, Phase, Task, TaskContext, Verifier, Worker};
use noorbit_planner::LlmClient;
use noorbit_lessons::LessonsStore;

const DEFAULT_BLENDER_BIN: &str = "blender";
const DEFAULT_SCRIPT_TIMEOUT: Duration = Duration::from_secs(300);

pub struct BlenderWorker {
    llm: Arc<dyn LlmClient>,
    lessons: Arc<LessonsStore>,
    project_path: PathBuf,
    threads: usize,
}
impl BlenderWorker {
    pub fn new(llm: Arc<dyn LlmClient>, lessons: Arc<LessonsStore>, project_path: impl Into<PathBuf>) -> Self {
        Self { llm, lessons, project_path: project_path.into(), threads: 4 }
    }
    pub fn with_threads(mut self, n: usize) -> Self { self.threads = n.max(1); self }
}
#[async_trait]
impl Worker for BlenderWorker {
    async fn run(&self, task: &Task, _ctx: &TaskContext) -> Result<Diff, String> {
        let prompt = format!("Tasca Blender: {}\nDescripció: {}\nRetorna un script Python bpy.", task.title, task.description);
        let response = self.llm.complete(&prompt).await.map_err(|e| e.to_string())?;
        let code = response.trim().to_string();
        if code.is_empty() { return Err("script buit".into()); }
        let scripts_dir = self.project_path.join("scripts");
        tokio::fs::create_dir_all(&scripts_dir).await.map_err(|e| e.to_string())?;
        let script_path = scripts_dir.join(format!("{}.py", task.id));
        tokio::fs::write(&script_path, &code).await.map_err(|e| e.to_string())?;
        // run blender
        let mut cmd = Command::new(DEFAULT_BLENDER_BIN);
        cmd.arg("-b").arg("--factory-startup").arg("-t").arg(self.threads.to_string()).arg("-P").arg(&script_path).args(["--python-exit-code","1"]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
        #[cfg(unix)] cmd.process_group(0);
        let child = cmd.spawn().map_err(|e| format!("spawn blender: {e}"))?;
        let out = tokio::time::timeout(DEFAULT_SCRIPT_TIMEOUT, child.wait_with_output()).await.map_err(|_| "blender timeout".to_string())?.map_err(|e| format!("wait blender: {e}"))?;
        if !out.status.success() {
            let e = format!("blender exit {}: {}", out.status, String::from_utf8_lossy(&out.stderr));
            let _ = self.lessons.append(task.phase.unwrap_or(Phase::Modeling), &task.id, &e, &code).await;
            return Err(e);
        }
        let patch = format!("--- a/scripts/{}.py\n+++ b/scripts/{}.py\n", task.id, task.id);
        Ok(Diff::patch(patch, 0))
    }
}
pub struct BlenderVerifier { project_path: PathBuf }
impl BlenderVerifier { pub fn new(project_path: impl Into<PathBuf>) -> Self { Self { project_path: project_path.into() } } }
#[async_trait]
impl Verifier for BlenderVerifier {
    async fn run(&self, task: &Task, _ctx: &TaskContext) -> Result<(), String> {
        let glb = self.project_path.join("assets").join(format!("{}.glb", task.id));
        if !glb.exists() { return Err(format!("glb no existeix: {}", glb.display())); }
        let meta = std::fs::metadata(&glb).map_err(|e| e.to_string())?;
        if meta.len() < 100 { return Err(format!("glb massa petit: {}", meta.len())); }
        Ok(())
    }
}
pub struct VideoRenderer { #[allow(dead_code)] project_path: PathBuf }
impl VideoRenderer { pub fn new(project_path: impl Into<PathBuf>) -> Self { Self { project_path: project_path.into() } } }
pub struct BlenderImporter { #[allow(dead_code)] project_path: PathBuf }
impl BlenderImporter { pub fn new(project_path: impl Into<PathBuf>) -> Self { Self { project_path: project_path.into() } } }
#[allow(unused)]
fn _use() { let _ = Path::new(""); let _ = PathBuf::new(); }

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
    async fn verifier_falla_si_glb_no_existeix() {
        let dir = TempDir::new().unwrap();
        let v = BlenderVerifier::new(dir.path());
        let ctx = TaskContext {
            cancel: tokio_util::sync::CancellationToken::new(),
            child_registry: std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new())),
            quality: noorbit_jam::QualityLevel::Full,
            attempt: 1,
        };
        assert!(v.run(&task("t01"), &ctx).await.is_err());
    }

    #[tokio::test]
    async fn verifier_passa_si_glb_valid() {
        let dir = TempDir::new().unwrap();
        let assets = dir.path().join("assets");
        std::fs::create_dir_all(&assets).unwrap();
        // GLB magic bytes + versió 2 + mida > 100
        let mut buf = Vec::new();
        buf.extend_from_slice(b"glTF");
        buf.extend_from_slice(&2u32.to_le_bytes());
        buf.extend_from_slice(&120u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 108]);
        std::fs::write(assets.join("t01.glb"), buf).unwrap();

        let v = BlenderVerifier::new(dir.path());
        let ctx = TaskContext {
            cancel: tokio_util::sync::CancellationToken::new(),
            child_registry: std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new())),
            quality: noorbit_jam::QualityLevel::Full,
            attempt: 1,
        };
        assert!(v.run(&task("t01"), &ctx).await.is_ok());
    }

    #[tokio::test]
    async fn verifier_falla_si_glb_massa_petit() {
        let dir = TempDir::new().unwrap();
        let assets = dir.path().join("assets");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join("t01.glb"), b"glTF").unwrap();

        let v = BlenderVerifier::new(dir.path());
        let ctx = TaskContext {
            cancel: tokio_util::sync::CancellationToken::new(),
            child_registry: std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new())),
            quality: noorbit_jam::QualityLevel::Full,
            attempt: 1,
        };
        assert!(v.run(&task("t01"), &ctx).await.is_err());
    }

    #[test]
    fn with_threads_no_permet_zero() {
        let w = BlenderWorker::new(
            std::sync::Arc::new(StubLlm),
            std::sync::Arc::new(LessonsStore::new("/tmp/x")),
            "/tmp",
        )
        .with_threads(0);
        assert_eq!(w.threads, 1);
    }

    struct StubLlm;
    #[async_trait::async_trait]
    impl LlmClient for StubLlm {
        async fn complete(&self, _prompt: &str) -> Result<String, String> {
            Ok("@file: scripts/test.py\nprint('hi')\n".into())
        }
    }
}
