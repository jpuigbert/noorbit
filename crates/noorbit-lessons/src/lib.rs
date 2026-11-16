use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tracing::warn;
use noorbit_jam::Phase;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson { pub ts: u64, pub phase: String, pub task_id: String, pub error: String, pub attempt: String }

pub struct LessonsStore { path: PathBuf, write_lock: Mutex<()>, max_lines: usize }
impl LessonsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self { Self { path: path.into(), write_lock: Mutex::new(()), max_lines: 500 } }
    pub fn with_max_lines(mut self, n: usize) -> Self { self.max_lines = n.max(10); self }
    pub async fn append(&self, phase: Phase, task_id: &str, error: &str, attempt: &str) -> Result<(), String> {
        let _guard = self.write_lock.lock().await;
        let lesson = Lesson { ts: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0), phase: format!("{phase:?}"), task_id: task_id.to_string(), error: truncate(error,500), attempt: truncate(attempt,500) };
        let line = serde_json::to_string(&lesson).map_err(|e| e.to_string())? + "\n";
        if let Some(parent) = self.path.parent() { tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?; }
        let mut f = tokio::fs::OpenOptions::new().create(true).append(true).open(&self.path).await.map_err(|e| format!("open: {e}"))?;
        f.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
        f.flush().await.map_err(|e| e.to_string())?;
        Ok(())
    }
    pub async fn recent(&self, phase: Option<Phase>, n: usize, max_chars: usize) -> Vec<Lesson> {
        let content = match tokio::fs::read_to_string(&self.path).await { Ok(c) => c, Err(_) => return Vec::new() };
        let phase_str = phase.map(|p| format!("{p:?}"));
        let mut out: Vec<Lesson> = content.lines().filter_map(|l| serde_json::from_str::<Lesson>(l).ok()).filter(|l| phase_str.as_ref().map_or(true, |p| &l.phase == p)).rev().take(n).collect();
        out.reverse();
        let mut total = 0usize; let mut kept = Vec::new();
        for l in out.into_iter().rev() {
            let sz = l.error.len() + l.attempt.len();
            if total + sz > max_chars { break; }
            total += sz; kept.push(l);
        }
        kept.reverse();
        kept
    }
    pub async fn compact(&self) -> Result<(), String> {
        let _guard = self.write_lock.lock().await;
        let content = match tokio::fs::read_to_string(&self.path).await { Ok(c) => c, Err(_) => return Ok(()) };
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() <= self.max_lines { return Ok(()); }
        let keep = &lines[lines.len() - self.max_lines..];
        let mut new_content = keep.join("\n"); new_content.push('\n');
        let tmp = self.path.with_extension("jsonl.tmp");
        tokio::fs::write(&tmp, new_content).await.map_err(|e| e.to_string())?;
        tokio::fs::rename(&tmp, &self.path).await.map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn format_for_prompt(lessons: &[Lesson]) -> String {
        if lessons.is_empty() { return String::new(); }
        let mut s = String::from("\nINTENTS FALLITS ANTERIORS:\n");
        for (i, l) in lessons.iter().enumerate() { s.push_str(&format!("{}. [{}] {} → {}\n", i + 1, l.phase, l.task_id, l.error)); }
        s.push('\n');
        s
    }
}
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { return s.to_string(); }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) { end -= 1; }
    format!("{}...", &s[..end])
}
#[allow(unused)]
fn _warn() { let _ = warn!(""); }

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn append_i_recent() {
        let dir = TempDir::new().unwrap();
        let store = LessonsStore::new(dir.path().join("lessons.jsonl"));
        store.append(Phase::Modeling, "t01", "error 1", "diff 1").await.unwrap();
        store.append(Phase::Animation, "t02", "error 2", "diff 2").await.unwrap();

        let all = store.recent(None, 10, 10000).await;
        assert_eq!(all.len(), 2);

        let modeling = store.recent(Some(Phase::Modeling), 10, 10000).await;
        assert_eq!(modeling.len(), 1);
        assert_eq!(modeling[0].task_id, "t01");
    }

    #[tokio::test]
    async fn recent_limita_chars() {
        let dir = TempDir::new().unwrap();
        let store = LessonsStore::new(dir.path().join("lessons.jsonl"));
        store.append(Phase::Modeling, "t01", "a".repeat(100).as_str(), "b").await.unwrap();
        store.append(Phase::Modeling, "t02", "c".repeat(100).as_str(), "d").await.unwrap();

        let r = store.recent(Some(Phase::Modeling), 10, 50).await;
        assert!(r.len() <= 1, "hauria de limitar per chars");
    }

    #[tokio::test]
    async fn compact_deixa_només_últimes() {
        let dir = TempDir::new().unwrap();
        let store = LessonsStore::new(dir.path().join("lessons.jsonl")).with_max_lines(10);
        for i in 0..15 {
            store.append(Phase::Modeling, &format!("t{i}"), "err", "diff").await.unwrap();
        }
        store.compact().await.unwrap();
        let all = store.recent(None, 100, 100000).await;
        assert_eq!(all.len(), 10);
    }

    #[tokio::test]
    async fn format_for_prompt_buit() {
        assert_eq!(LessonsStore::format_for_prompt(&[]), "");
    }

    #[tokio::test]
    async fn format_for_prompt_no_buit() {
        let l = Lesson {
            ts: 1,
            phase: "Modeling".into(),
            task_id: "t01".into(),
            error: "fail".into(),
            attempt: "diff".into(),
        };
        let s = LessonsStore::format_for_prompt(&[l]);
        assert!(s.contains("t01"));
        assert!(s.contains("fail"));
    }
}
