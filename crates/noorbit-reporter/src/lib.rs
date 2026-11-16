use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::process::Command;
use noorbit_jam::{JamStats, Plan, Reporter};

const GIT_LOG_TIMEOUT: Duration = Duration::from_secs(10);
const GIT_LOG_MAX_LINES: usize = 100;

#[derive(Debug, Clone, Deserialize)]
pub struct GuardianSummary {
    pub samples: u64,
    pub peak_swap_bytes: u64,
    pub peak_rss_bytes: u64,
    pub min_available_bytes: Option<u64>,
    pub duration_ms: u64,
}

pub struct MarkdownReporter { report_path: PathBuf, repo_path: Option<PathBuf>, guardian_summary_path: Option<PathBuf> }
impl MarkdownReporter {
    pub fn new(report_path: impl Into<PathBuf>) -> Result<Self, String> {
        let report_path = report_path.into();
        if report_path.parent().is_none() { return Err(format!("report_path sense directori pare: {}", report_path.display())); }
        Ok(Self { report_path, repo_path: None, guardian_summary_path: None })
    }
    pub fn with_repo(mut self, repo: impl Into<PathBuf>) -> Self { self.repo_path = Some(repo.into()); self }
    pub fn with_guardian_summary(mut self, path: impl Into<PathBuf>) -> Self { self.guardian_summary_path = Some(path.into()); self }
    pub async fn read_guardian_summary(&self) -> Option<GuardianSummary> {
        let path = self.guardian_summary_path.as_ref()?;
        let bytes = match tokio::fs::read(path).await { Ok(b) => b, Err(_) => return None };
        serde_json::from_slice::<GuardianSummary>(&bytes).ok()
    }
    pub async fn read_commits(&self) -> Vec<String> {
        let Some(repo) = self.repo_path.as_ref() else { return Vec::new(); };
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo).args(["log", &format!("-n{GIT_LOG_MAX_LINES}"), "--pretty=format:%h %s"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        let child = match cmd.spawn() { Ok(c) => c, Err(_) => return Vec::new() };
        let out = match tokio::time::timeout(GIT_LOG_TIMEOUT, child.wait_with_output()).await { Ok(Ok(o)) if o.status.success() => o, _ => return Vec::new() };
        String::from_utf8_lossy(&out.stdout).lines().map(|l| l.trim_end().to_string()).filter(|l| !l.is_empty()).collect()
    }
    async fn build_markdown(&self, plan: &Plan, stats: &JamStats) -> String {
        let guardian = self.read_guardian_summary().await;
        let commits = self.read_commits().await;
        let started = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let mut md = String::with_capacity(4096);
        md.push_str(&format!("# Jam Report — {}\n\n", md_escape(&plan.title)));
        md.push_str(&format!("- **Generat:** {started}\n"));
        md.push_str(&format!("- **Durada planificada:** {}\n", fmt_duration(plan.duration)));
        md.push_str(&format!("- **Durada real:** {}\n\n", fmt_ms(stats.duration_ms)));
        md.push_str("## Resultats\n\n| Mètrica | Valor |\n|---|---|\n");
        md.push_str(&format!("| Completades | {} |\n", stats.completed));
        md.push_str(&format!("| Bloquejades | {} |\n", stats.blocked));
        md.push_str(&format!("| Saltades | {} |\n", stats.skipped));
        md.push_str(&format!("| Intents totals | {} |\n", stats.total_attempts));
        let total = plan.tasks.len() as u32;
        let ratio = if total == 0 { 0.0 } else { 100.0 * stats.completed as f64 / total as f64 };
        md.push_str(&format!("| Èxit | {ratio:.1}% |\n\n"));
        md.push_str("## Tasques\n\n");
        if plan.tasks.is_empty() { md.push_str("_(cap tasca)_\n\n"); }
        else {
            md.push_str("| ID | Títol | Estat |\n|---|---|---|\n");
            let processed = (stats.completed + stats.blocked) as usize;
            for (i, task) in plan.tasks.iter().enumerate() {
                let estat = if i < processed { "processada" } else { "no processada" };
                md.push_str(&format!("| {} | {} | {} |\n", md_escape(&task.id), md_escape(&task.title), estat));
            }
            md.push('\n');
        }
        if !commits.is_empty() { md.push_str("## Commits\n\n```\n"); for c in &commits { md.push_str(c); md.push('\n'); } md.push_str("```\n\n"); }
        if let Some(g) = guardian {
            md.push_str("## Memòria del guardian\n\n| Mètrica | Valor |\n|---|---|\n");
            md.push_str(&format!("| Mostres | {} |\n", g.samples));
            md.push_str(&format!("| Peak swap | {} |\n", fmt_bytes(g.peak_swap_bytes)));
            md.push_str(&format!("| Peak RSS | {} |\n", fmt_bytes(g.peak_rss_bytes)));
            md.push_str(&format!("| Min available | {} |\n", g.min_available_bytes.map(fmt_bytes).unwrap_or_else(|| "n/d".to_string())));
            md.push_str(&format!("| Durada sessió | {} |\n\n", fmt_ms(g.duration_ms)));
        }
        md
    }
}
#[async_trait]
impl Reporter for MarkdownReporter {
    async fn report(&self, plan: &Plan, stats: &JamStats) -> Result<(), String> {
        let md = self.build_markdown(plan, stats).await;
        let parent = self.report_path.parent().ok_or_else(|| "report_path sense pare".to_string())?;
        tokio::fs::create_dir_all(parent).await.map_err(|e| format!("create_dir_all: {e}"))?;
        let tmp = parent.join(format!(".{}.tmp-{}", self.report_path.file_name().and_then(|n| n.to_str()).unwrap_or("report.md"), std::process::id()));
        tokio::fs::write(&tmp, md.as_bytes()).await.map_err(|e| format!("escriptura tmp: {e}"))?;
        tokio::fs::rename(&tmp, &self.report_path).await.map_err(|e| format!("rename: {e}"))?;
        Ok(())
    }
}
fn md_escape(s: &str) -> String { s.replace('|',"\\|").replace('\n'," ").replace('\r'," ") }
fn fmt_bytes(b: u64) -> String {
    const KB: u64 = 1024; const MB: u64 = 1024*KB; const GB: u64 = 1024*MB;
    if b >= GB { format!("{:.2} GB", b as f64 / GB as f64) } else if b >= MB { format!("{:.1} MB", b as f64 / MB as f64) } else if b >= KB { format!("{:.1} KB", b as f64 / KB as f64) } else { format!("{b} B") }
}
fn fmt_ms(ms: u64) -> String {
    let s = ms / 1000; let h = s / 3600; let m = (s % 3600) / 60; let sec = s % 60;
    if h > 0 { format!("{h}h {m}m {sec}s") } else if m > 0 { format!("{m}m {sec}s") } else { format!("{sec}s") }
}
fn fmt_duration(d: Duration) -> String { fmt_ms(d.as_millis() as u64) }

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    fn plan() -> Plan {
        Plan {
            title: "Test | Plan".into(),
            tasks: vec![noorbit_jam::Task {
                id: "t01".into(),
                title: "Tasca amb | pipe".into(),
                description: "d".into(),
                acceptance: vec!["ok".into()],
                phase: None,
            }],
            duration: Duration::from_secs(3600),
            budget: noorbit_jam::TimeBudget::from_total(Duration::from_secs(3600)),
        }
    }

    fn stats() -> JamStats {
        JamStats {
            completed: 1,
            blocked: 0,
            skipped: 0,
            total_attempts: 1,
            duration_ms: 12345,
        }
    }

    #[tokio::test]
    async fn escriu_markdown_basic() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("REPORT.md");
        let r = MarkdownReporter::new(&path).unwrap();
        r.report(&plan(), &stats()).await.unwrap();
        let md = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(md.contains("# Jam Report"));
        assert!(md.contains("Completades"));
        assert!(md.contains("\\|")); // pipe escapat
    }

    #[tokio::test]
    async fn funciona_sense_repo_ni_guardian() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("REPORT.md");
        let r = MarkdownReporter::new(&path).unwrap();
        r.report(&plan(), &stats()).await.unwrap();
        let md = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(!md.contains("## Commits"));
        assert!(!md.contains("## Memòria"));
    }

    #[tokio::test]
    async fn llegeix_guardian_summary() {
        let dir = TempDir::new().unwrap();
        let summary_path = dir.path().join("summary.json");
        let json = r#"{"samples":42,"peak_swap_bytes":1073741824,"peak_rss_bytes":2147483648,"min_available_bytes":536870912,"duration_ms":60000}"#;
        tokio::fs::write(&summary_path, json).await.unwrap();

        let path = dir.path().join("REPORT.md");
        let r = MarkdownReporter::new(&path)
            .unwrap()
            .with_guardian_summary(&summary_path);
        r.report(&plan(), &stats()).await.unwrap();
        let md = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(md.contains("## Memòria"));
        assert!(md.contains("42")); // samples
        assert!(md.contains("1.00 GB")); // peak swap
    }

    #[tokio::test]
    async fn guardian_summary_invalid_no_peta() {
        let dir = TempDir::new().unwrap();
        let summary_path = dir.path().join("summary.json");
        tokio::fs::write(&summary_path, "no és json").await.unwrap();

        let path = dir.path().join("REPORT.md");
        let r = MarkdownReporter::new(&path)
            .unwrap()
            .with_guardian_summary(&summary_path);
        r.report(&plan(), &stats()).await.unwrap();
        let md = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(!md.contains("## Memòria"));
    }

    #[tokio::test]
    async fn no_deixa_tmp() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("REPORT.md");
        let r = MarkdownReporter::new(&path).unwrap();
        r.report(&plan(), &stats()).await.unwrap();
        let mut entries = tokio::fs::read_dir(dir.path()).await.unwrap();
        while let Some(e) = entries.next_entry().await.unwrap() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            assert!(!name.contains(".tmp-"), "tmp left: {name}");
        }
    }

    #[test]
    fn md_escape_funciona() {
        assert_eq!(md_escape("a|b\nc"), "a\\|b c");
    }

    #[test]
    fn fmt_bytes_correcte() {
        assert_eq!(fmt_bytes(0), "0 B");
        assert_eq!(fmt_bytes(1024), "1.0 KB");
        assert_eq!(fmt_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(fmt_bytes(2 * 1024 * 1024 * 1024), "2.00 GB");
    }

    #[test]
    fn constructor_rebutja_path_sense_pare() {
        assert!(MarkdownReporter::new("").is_err());
        assert!(MarkdownReporter::new("/").is_err());
    }
}
