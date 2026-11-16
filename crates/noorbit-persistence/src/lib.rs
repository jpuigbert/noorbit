use std::path::{Path, PathBuf};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{info, warn};
use noorbit_jam::{JamStats, Phase, PhaseTracker, Plan, QualityLevel, TimeBudget};

const SCHEMA_VERSION: u32 = 1;
/// Màxima mida raonable de l'estat desat (les tasques d'un pla poden ser
/// centenars; MB és molt de marge). Més enllà, el fitxer s'ha corromput o
/// l'ha escrit una altra cosa: el JSON intentaria de llegir-se sencer.
const MAX_STATE_BYTES: u64 = 1024 * 1024;
/// Edat màxima d'una represa: set dies. Un estat més vell apunta a un SHA que
/// l'usuari pot haver rebasejat/esborrat i a un pla que ja no té res a veure
/// amb el repo actual: és menys arriscat començar de zero.
const MAX_STATE_AGE_SECS: u64 = 7 * 24 * 3600;
/// Tolerància per a rellotges desviats: un timestamp de futur llunyà significa
/// que el sistema on es va desar i aquest no tenen la mateixa hora.
const FUTURE_SKEW_SECS: u64 = 3600;

#[derive(Debug, Error)]
pub enum PersistError { #[error("E/S: {0}")] Io(#[from] std::io::Error), #[error("JSON: {0}")] Json(String) }
pub type Result<T> = std::result::Result<T, PersistError>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PersistedQuality { Full, Reduced, Minimal }
impl From<QualityLevel> for PersistedQuality {
    fn from(q: QualityLevel) -> Self { match q { QualityLevel::Full => Self::Full, QualityLevel::Reduced => Self::Reduced, QualityLevel::Minimal => Self::Minimal } }
}
impl From<PersistedQuality> for QualityLevel {
    fn from(q: PersistedQuality) -> Self { match q { PersistedQuality::Full => Self::Full, PersistedQuality::Reduced => Self::Reduced, PersistedQuality::Minimal => Self::Minimal } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedStats { pub completed: u32, pub blocked: u32, pub skipped: u32, pub total_attempts: u32, pub duration_ms: u64 }
impl From<&JamStats> for PersistedStats { fn from(s: &JamStats) -> Self { Self { completed: s.completed, blocked: s.blocked, skipped: s.skipped, total_attempts: s.total_attempts, duration_ms: s.duration_ms } } }
impl From<PersistedStats> for JamStats { fn from(s: PersistedStats) -> Self { Self { completed: s.completed, blocked: s.blocked, skipped: s.skipped, total_attempts: s.total_attempts, duration_ms: s.duration_ms } } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedPlan { pub title: String, pub duration_secs: u64, pub task_count: usize, pub task_ids: Vec<String>, pub budget_secs: [u64; 4] }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedState {
    pub schema_version: u32,
    pub jam_id: String,
    pub brief_fingerprint: String,
    pub plan: PersistedPlan,
    pub last_good_sha: String,
    pub next_task_idx: usize,
    pub quality: PersistedQuality,
    pub stats: PersistedStats,
    pub phase_spent: Vec<(String, u64)>,
    pub saved_at_unix: u64,
}

pub struct ProgressStore { path: PathBuf, lock: Mutex<()> }
impl ProgressStore {
    pub fn new(path: impl Into<PathBuf>) -> Self { Self { path: path.into(), lock: Mutex::new(()) } }
    pub fn default_for_repo(repo: &Path) -> Self { Self::new(repo.join(".noorbit").join("state.json")) }
    pub async fn save(&self, state: &PersistedState) -> Result<()> {
        let _guard = self.lock.lock().await;
        let json = serde_json::to_vec_pretty(state).map_err(|e| PersistError::Json(e.to_string()))?;
        let parent = self.path.parent().ok_or_else(|| PersistError::Json("path sense pare".into()))?;
        tokio::fs::create_dir_all(parent).await?;
        let tmp = parent.join(format!(".{}.tmp-{}", self.path.file_name().and_then(|n| n.to_str()).unwrap_or("state.json"), std::process::id()));
        tokio::fs::write(&tmp, &json).await?;
        tokio::fs::rename(&tmp, &self.path).await?;
        info!(next = state.next_task_idx, "progress guardat");
        Ok(())
    }
    pub async fn load(&self, expected_jam_id: &str, expected_brief_fp: &str, repo: &Path) -> Option<PersistedState> {
        // Mida PRIMER: un fitxer enorme o ple de brossa no s'ha ni de parsejar.
        if let Ok(meta) = tokio::fs::metadata(&self.path).await {
            if meta.len() > MAX_STATE_BYTES {
                warn!(len = meta.len(), max = MAX_STATE_BYTES, "estat desat: fitxer massa gran");
                return None;
            }
        }
        let bytes = tokio::fs::read(&self.path).await.ok()?;
        let state: PersistedState = match serde_json::from_slice(&bytes) {
            Ok(s) => s,
            Err(e) => { warn!("estat desat: JSON invàlid: {e}"); return None; }
        };
        if state.schema_version != SCHEMA_VERSION { warn!("schema diferent"); return None; }
        if state.jam_id != expected_jam_id { warn!("jam_id diferent"); return None; }
        if state.brief_fingerprint != expected_brief_fp { warn!("brief_fp diferent"); return None; }
        if !plan_sane(&state) { return None; }
        if !age_sane(state.saved_at_unix) { return None; }
        if state.next_task_idx > state.plan.task_count { warn!("index fora de rang"); return None; }
        if !sha_exists(repo, &state.last_good_sha).await { warn!("sha inexistent"); return None; }
        Some(state)
    }
    pub async fn clear(&self) -> Result<()> {
        let _guard = self.lock.lock().await;
        match tokio::fs::remove_file(&self.path).await { Ok(()) => Ok(()), Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), Err(e) => Err(PersistError::Io(e)) }
    }
}
async fn sha_exists(repo: &Path, sha: &str) -> bool {
    if sha.len() < 7 || !sha.chars().all(|c| c.is_ascii_hexdigit()) { return false; }
    use std::process::Stdio;
    let out = tokio::process::Command::new("git").arg("-C").arg(repo).args(["cat-file","-e",&format!("{sha}^{{commit}}")]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).output().await;
    matches!(out, Ok(o) if o.status.success())
}
/// Coherència interna del pla desat: `task_count` i `task_ids` han de
/// concordar (si no, un `next_task_idx` «vàlid» pot apuntar fora del pla real
/// i la represa saltaria tasques o moriria intentant-ho).
fn plan_sane(state: &PersistedState) -> bool {
    let p = &state.plan;
    if p.task_ids.len() != p.task_count {
        warn!("pla desat: task_count {} != task_ids {}", p.task_count, p.task_ids.len());
        return false;
    }
    if p.task_count == 0 {
        warn!("pla desat buit");
        return false;
    }
    if p.task_ids.iter().any(|id| id.trim().is_empty()) {
        warn!("pla desat: id de tasca buida");
        return false;
    }
    if p.duration_secs == 0 {
        warn!("pla desat: durada zero");
        return false;
    }
    if state.stats.completed + state.stats.blocked + state.stats.skipped > p.task_count as u32 + 1 {
        warn!("pla desat: estadístiques impossibles");
        return false;
    }
    true
}
/// L'estat no pot ser del futur ni tenir més de `MAX_STATE_AGE_SECS`.
fn age_sane(saved_at_unix: u64) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if now == 0 || saved_at_unix == 0 {
        warn!("estat desat sense timestamp");
        return false;
    }
    if saved_at_unix > now + FUTURE_SKEW_SECS {
        warn!("estat desat del futur (rellotges desviats)");
        return false;
    }
    if now.saturating_sub(saved_at_unix) > MAX_STATE_AGE_SECS {
        warn!("estat desat massa vell (més de 7 dies)");
        return false;
    }
    true
}

/// Afegeix `patterns` a `.git/info/exclude` del repo (exclusió NOMÉS local: no
/// toca el `.gitignore` de l'usuari).
///
/// Sense això, els fitxers interns de la Jam escrits dins el repo:
///  • es commiten amb `git add -A` (brutícia a l'historial), i
///  • s'esborren amb `git clean -fd` abans de cada tasca → l'estat desat
///    DESAPAREIX i ni la represa ni el reinici per memòria funcionen.
pub async fn ensure_git_excluded(repo: &Path, patterns: &[&str]) -> Result<()> {
    let gitdir = resolve_gitdir(repo).await?;
    let info = gitdir.join("info");
    tokio::fs::create_dir_all(&info).await?;
    let path = info.join("exclude");
    let mut current = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    if !current.is_empty() && !current.ends_with('\n') {
        current.push('\n');
    }
    let mut added = String::new();
    for pat in patterns {
        let pat = pat.trim();
        if pat.is_empty() {
            continue;
        }
        let already = current.lines().any(|l| l.trim() == pat);
        if !already {
            added.push_str(pat);
            added.push('\n');
        }
    }
    if added.is_empty() {
        return Ok(());
    }
    // Escriptura atòmica: si es talla a mitja, no perdem les regles antigues.
    let tmp = info.join(".exclude.tmp");
    tokio::fs::write(&tmp, format!("{current}{added}")).await?;
    tokio::fs::rename(&tmp, &path).await?;
    Ok(())
}
/// `.git` pot ser un directori (repo normal) o un fitxer `gitdir: …`
/// (worktree/submòdul). Cal saber-ho per trobar `info/exclude`.
async fn resolve_gitdir(repo: &Path) -> Result<PathBuf> {
    let dot_git = repo.join(".git");
    if tokio::fs::metadata(&dot_git).await.map(|m| m.is_dir()).unwrap_or(false) {
        return Ok(dot_git);
    }
    if let Ok(text) = tokio::fs::read_to_string(&dot_git).await {
        if let Some(rest) = text.trim().strip_prefix("gitdir:") {
            let p = PathBuf::from(rest.trim());
            return Ok(if p.is_absolute() { p } else { repo.join(p) });
        }
    }
    Err(PersistError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "el directori no és un repo git",
    )))
}
pub fn make_state(jam_id: &str, brief_fingerprint: &str, plan: &Plan, last_good_sha: &str, next_task_idx: usize, quality: QualityLevel, stats: &JamStats, tracker: &PhaseTracker) -> PersistedState {
    let budget = tracker.budget();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    PersistedState {
        schema_version: SCHEMA_VERSION,
        jam_id: jam_id.into(),
        brief_fingerprint: brief_fingerprint.into(),
        plan: PersistedPlan {
            title: plan.title.clone(),
            duration_secs: plan.duration.as_secs(),
            task_count: plan.tasks.len(),
            task_ids: plan.tasks.iter().map(|t| t.id.clone()).collect(),
            budget_secs: [budget.modeling.as_secs(), budget.animation.as_secs(), budget.video.as_secs(), budget.integration.as_secs()],
        },
        last_good_sha: last_good_sha.into(),
        next_task_idx,
        quality: quality.into(),
        stats: stats.into(),
        phase_spent: vec![],
        saved_at_unix: now,
    }
}
pub fn brief_fingerprint(brief_text: &str, engine: &str, duration_secs: u64) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    brief_text.hash(&mut h);
    engine.hash(&mut h);
    duration_secs.hash(&mut h);
    format!("{:016x}", h.finish())
}

// no re-exports needed
// avoid unused warnings
#[allow(unused)]
fn _use() { let _ = Duration::ZERO; let _ = Path::new(""); let _ = PathBuf::new(); let _ = Phase::General; let _ = TimeBudget::from_total(Duration::ZERO); }

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_repo() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let p = dir.path().to_path_buf();
        let run = |args: &[&str]| {
            Command::new("git").arg("-C").arg(&p).args(args).output().unwrap();
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
        std::fs::write(p.join("README.md"), "x").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "init"]);
        (dir, p)
    }

    fn head_sha(repo: &Path) -> String {
        let out = Command::new("git").arg("-C").arg(repo).args(["rev-parse", "HEAD"]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn dummy_plan() -> Plan {
        Plan {
            title: "test".into(),
            tasks: vec![noorbit_jam::Task {
                id: "t01".into(),
                title: "a".into(),
                description: "d".into(),
                acceptance: vec!["ok".into()],
                phase: Some(noorbit_jam::Phase::Modeling),
            }],
            duration: Duration::from_secs(3600),
            budget: noorbit_jam::TimeBudget::from_total(Duration::from_secs(3600)),
        }
    }

    #[tokio::test]
    async fn save_load_roundtrip() {
        let (_dir, repo) = init_repo();
        let store = ProgressStore::default_for_repo(&repo);
        let sha = head_sha(&repo);
        let plan = dummy_plan();
        let tracker = PhaseTracker::new(plan.budget.clone());
        let stats = JamStats::default();

        let state = make_state(
            "jam-123", "fp-abc", &plan, &sha, 0,
            QualityLevel::Full, &stats, &tracker,
        );
        store.save(&state).await.unwrap();

        let loaded = store.load("jam-123", "fp-abc", &repo).await;
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().next_task_idx, 0);
    }

    #[tokio::test]
    async fn rebutja_jam_id_diferent() {
        let (_dir, repo) = init_repo();
        let store = ProgressStore::default_for_repo(&repo);
        let sha = head_sha(&repo);
        let plan = dummy_plan();
        let tracker = PhaseTracker::new(plan.budget.clone());
        let state = make_state("jam-a", "fp", &plan, &sha, 0, QualityLevel::Full, &JamStats::default(), &tracker);
        store.save(&state).await.unwrap();

        assert!(store.load("jam-b", "fp", &repo).await.is_none());
    }

    #[tokio::test]
    async fn rebutja_brief_fp_diferent() {
        let (_dir, repo) = init_repo();
        let store = ProgressStore::default_for_repo(&repo);
        let sha = head_sha(&repo);
        let plan = dummy_plan();
        let tracker = PhaseTracker::new(plan.budget.clone());
        let state = make_state("jam", "fp-a", &plan, &sha, 0, QualityLevel::Full, &JamStats::default(), &tracker);
        store.save(&state).await.unwrap();

        assert!(store.load("jam", "fp-b", &repo).await.is_none());
    }

    #[tokio::test]
    async fn rebutja_sha_inexistent() {
        let (_dir, repo) = init_repo();
        let store = ProgressStore::default_for_repo(&repo);
        let plan = dummy_plan();
        let tracker = PhaseTracker::new(plan.budget.clone());
        let fake_sha = "0".repeat(40);
        let state = make_state("jam", "fp", &plan, &fake_sha, 0, QualityLevel::Full, &JamStats::default(), &tracker);
        store.save(&state).await.unwrap();

        assert!(store.load("jam", "fp", &repo).await.is_none());
    }

    #[tokio::test]
    async fn clear_es_idempotent() {
        let (_dir, repo) = init_repo();
        let store = ProgressStore::default_for_repo(&repo);
        store.clear().await.unwrap();
        store.clear().await.unwrap();
    }

    #[test]
    fn fingerprint_es_deterministic() {
        assert_eq!(brief_fingerprint("a", "b", 1), brief_fingerprint("a", "b", 1));
        assert_ne!(brief_fingerprint("a", "b", 1), brief_fingerprint("a", "b", 2));
    }

    /// Estat amb un pla internament incoherent (`task_count` ≠ `task_ids`):
    /// la represa podria començar per un índex que no existeix.
    #[tokio::test]
    async fn rebutja_pla_incoherent() {
        let (_dir, repo) = init_repo();
        let store = ProgressStore::default_for_repo(&repo);
        let sha = head_sha(&repo);
        let mut state = make_state("j", "f", &dummy_plan(), &sha, 0, QualityLevel::Full, &JamStats::default(), &PhaseTracker::new(dummy_plan().budget.clone()));
        state.plan.task_count = 7; // diu 7 tasques, la llista en té 1
        store.save(&state).await.unwrap();
        assert!(store.load("j", "f", &repo).await.is_none());
    }

    /// Un estat de fa vuit dies apunta a un repo que l'usuari probablement ja
    /// ha canviat; reprendre'l seria pitjor que començar de zero.
    #[tokio::test]
    async fn rebutja_estat_massa_vell() {
        let (_dir, repo) = init_repo();
        let store = ProgressStore::default_for_repo(&repo);
        let sha = head_sha(&repo);
        let tracker = PhaseTracker::new(dummy_plan().budget.clone());
        let mut state = make_state("j", "f", &dummy_plan(), &sha, 0, QualityLevel::Full, &JamStats::default(), &tracker);
        state.saved_at_unix = state.saved_at_unix.saturating_sub(8 * 24 * 3600);
        store.save(&state).await.unwrap();
        assert!(store.load("j", "f", &repo).await.is_none());
    }

    #[tokio::test]
    async fn rebutja_estat_del_futur() {
        let (_dir, repo) = init_repo();
        let store = ProgressStore::default_for_repo(&repo);
        let sha = head_sha(&repo);
        let tracker = PhaseTracker::new(dummy_plan().budget.clone());
        let mut state = make_state("j", "f", &dummy_plan(), &sha, 0, QualityLevel::Full, &JamStats::default(), &tracker);
        state.saved_at_unix += 10 * 24 * 3600;
        store.save(&state).await.unwrap();
        assert!(store.load("j", "f", &repo).await.is_none());
    }

    /// Un fitxer enorme no s'ha ni de parsejar (podria ser brossa o una
    /// bombolla de memòria).
    #[tokio::test]
    async fn rebutja_fitxer_massa_gran() {
        let (_dir, repo) = init_repo();
        let store = ProgressStore::default_for_repo(&repo);
        let path = store.path.clone();
        tokio::fs::create_dir_all(path.parent().unwrap()).await.unwrap();
        tokio::fs::write(&path, vec![b'x'; 2 * 1024 * 1024]).await.unwrap();
        assert!(store.load("j", "f", &repo).await.is_none());
    }

    #[tokio::test]
    async fn rebutja_json_corrupt() {
        let (_dir, repo) = init_repo();
        let store = ProgressStore::default_for_repo(&repo);
        tokio::fs::create_dir_all(store.path.parent().unwrap()).await.unwrap();
        tokio::fs::write(&store.path, b"{ no es json ").await.unwrap();
        assert!(store.load("j", "f", &repo).await.is_none());
    }

    /// `ensure_git_excluded` ha de fer que `git add -A` IGNORI els fitxers
    /// interns i que `git clean -fd` NO els esborri (si no, l'estat desat
    /// desapareix a cada intent i la represa mai funciona).
    #[tokio::test]
    async fn exclude_protegeix_lestat_desat() {
        let (_dir, repo) = init_repo();
        let store = ProgressStore::default_for_repo(&repo);
        let sha = head_sha(&repo);
        let tracker = PhaseTracker::new(dummy_plan().budget.clone());
        let state = make_state("j", "f", &dummy_plan(), &sha, 0, QualityLevel::Full, &JamStats::default(), &tracker);
        store.save(&state).await.unwrap();

        ensure_git_excluded(&repo, &[".noorbit/"]).await.unwrap();
        // Idempotència: cridar-ho dues vegades no duplica la regla.
        ensure_git_excluded(&repo, &[".noorbit/"]).await.unwrap();
        let exclude = std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
        assert_eq!(exclude.lines().filter(|l| l.trim() == ".noorbit/").count(), 1, "{exclude}");

        // `git clean -fd` (el que fa el checkpointer abans de cada tasca).
        let out = Command::new("git").arg("-C").arg(&repo).args(["clean", "-fd"]).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert!(
            std::fs::metadata(store.path.clone()).is_ok(),
            "git clean ha esborrat l'estat desat: la represa seria impossible"
        );

        // `git add -A` no ha de trackejar-lo.
        Command::new("git").arg("-C").arg(&repo).args(["add", "-A"]).output().unwrap();
        let staged = Command::new("git").arg("-C").arg(&repo).args(["diff", "--cached", "--name-only"]).output().unwrap();
        let staged = String::from_utf8_lossy(&staged.stdout);
        assert!(!staged.contains(".noorbit"), "s'ha comitat l'estat intern: {staged}");
    }

    #[tokio::test]
    async fn exclude_falla_fora_de_un_repo() {
        let dir = TempDir::new().unwrap();
        assert!(ensure_git_excluded(dir.path(), &["x"]).await.is_err());
    }
}
