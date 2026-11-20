// noorbit-jam v1.0 (core)
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub use noorbit_guardian::child::ChildCommand;
pub use noorbit_guardian::ChildRuntime;

pub type TaskId = String;
pub type Result<T, E = String> = std::result::Result<T, E>;

#[derive(Debug, Clone)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub description: String,
    pub acceptance: Vec<String>,
    pub phase: Option<Phase>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Phase { Modeling, Animation, Video, Integration, General }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimeBudget {
    pub modeling: Duration,
    pub animation: Duration,
    pub video: Duration,
    pub integration: Duration,
}
impl TimeBudget {
    pub fn from_total(total: Duration) -> Self {
        let pct = |p: u64| Duration::from_secs(total.as_secs() * p / 100);
        Self { modeling: pct(55), animation: pct(15), video: pct(20), integration: pct(10) }
    }
    pub fn for_phase(&self, phase: Phase) -> Duration {
        match phase { Phase::Modeling => self.modeling, Phase::Animation => self.animation, Phase::Video => self.video, Phase::Integration => self.integration, Phase::General => Duration::MAX }
    }
    pub fn total(&self) -> Duration { self.modeling + self.animation + self.video + self.integration }
}

pub struct PhaseTracker {
    start: Instant,
    spent: std::collections::HashMap<Phase, Duration>,
    budget: TimeBudget,
}
impl PhaseTracker {
    pub fn new(budget: TimeBudget) -> Self { Self { start: Instant::now(), spent: std::collections::HashMap::new(), budget } }
    pub fn record(&mut self, phase: Phase, delta: Duration) { *self.spent.entry(phase).or_default() += delta; }
    pub fn is_over_budget(&self, phase: Phase) -> bool {
        let limit = self.budget.for_phase(phase);
        if limit == Duration::MAX { return false; }
        self.spent.get(&phase).copied().unwrap_or_default() >= limit
    }
    pub fn spent(&self, phase: Phase) -> Duration { self.spent.get(&phase).copied().unwrap_or_default() }
    pub fn elapsed(&self) -> Duration { self.start.elapsed() }
    pub fn budget(&self) -> &TimeBudget { &self.budget }
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub title: String,
    pub tasks: Vec<Task>,
    pub duration: Duration,
    pub budget: TimeBudget,
}

#[derive(Debug, Clone)]
pub struct Diff {
    pub patch: String,
    pub tokens_used: u64,
    /// `true` quan el worker JA ha escrit els fitxers al working tree
    /// (mode «materialitzat»: el jam NO aplica el patch, perquè el codi
    /// ja és a terra i el `git apply` fallaria).
    pub materialized: bool,
    /// Fitxers REALMENT tocats (camins relatius al repo). Si el worker no els
    /// posa, el bucle els pregunta a `git status`: sense això, un diff
    /// materialitzat amb `patch` buit arribava al verifier amb 0 fitxers i
    /// els criteris d'acceptació no es podien comprovar.
    pub files: Vec<String>,
}
impl Diff {
    /// Diff de patch (el jam l'ha d'aplicar).
    pub fn patch(patch: String, tokens_used: u64) -> Self {
        Self { patch, tokens_used, materialized: false, files: Vec::new() }
    }
    /// Diff ja materialitzat (el worker ha escrit els fitxers).
    pub fn materialized(patch: String, tokens_used: u64) -> Self {
        Self { patch, tokens_used, materialized: true, files: Vec::new() }
    }
    /// Diff materialitzat declarant QUINS fitxers ha escrit.
    pub fn materialized_files(files: Vec<String>, tokens_used: u64) -> Self {
        let patch = files
            .iter()
            .map(|f| format!("+++ b/{f}\n"))
            .collect::<Vec<_>>()
            .join("");
        Self { patch, tokens_used, materialized: true, files }
    }
    /// Diff de patch declarant QUINS fitxers conté.
    pub fn patch_files(patch: String, tokens_used: u64, files: Vec<String>) -> Self {
        Self { patch, tokens_used, materialized: false, files }
    }
}
impl Diff {
    pub fn line_count(&self) -> usize { self.patch.lines().count() }
    pub fn file_count(&self) -> usize {
        if !self.files.is_empty() {
            return self.files.len();
        }
        self.patch.lines().filter(|l| l.starts_with("+++ b/") || l.starts_with("+++ /dev/null")).count()
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct JamStats {
    pub completed: u32,
    pub blocked: u32,
    pub skipped: u32,
    pub total_attempts: u32,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityLevel { Full, Reduced, Minimal }

#[async_trait::async_trait]
pub trait Worker: Send + Sync {
    async fn run(&self, task: &Task, ctx: &TaskContext) -> Result<Diff, String>;
}
#[async_trait::async_trait]
pub trait Verifier: Send + Sync {
    async fn run(&self, task: &Task, ctx: &TaskContext) -> Result<(), String>;

    /// Versió amb els fitxers REALMENT tocats pel worker. Per defecte
    /// delega a `run`, així que cap verifier existent es trenca; un verifier
    /// que vulgui comprovar els criteris d'acceptació contra el que hi ha
    /// a terra (el punt flac de la v1) sobreescriu aquest mètode.
    async fn run_with_files(
        &self,
        task: &Task,
        ctx: &TaskContext,
        _files: &[String],
    ) -> Result<(), String> {
        self.run(task, ctx).await
    }
}
#[async_trait::async_trait]
pub trait Checkpointer: Send + Sync {
    async fn head(&self) -> Result<String, String>;
    async fn reset_hard(&self, sha: &str) -> Result<(), String>;
    async fn commit(&self, task: &Task) -> Result<String, String>;
    async fn tag(&self, name: &str) -> Result<(), String>;
    async fn cleanup(&self) -> Result<(), String> { Ok(()) }
}
#[async_trait::async_trait]
pub trait Cache: Send + Sync { async fn clear(&self); }
#[async_trait::async_trait]
pub trait Reporter: Send + Sync { async fn report(&self, plan: &Plan, stats: &JamStats) -> Result<(), String>; }

/// Esdeveniments de progrés del bucle (per a la UI o el log).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    TaskStart { id: String, title: String },
    TaskDone { id: String, files: usize },
    TaskError { id: String, error: String },
    TaskBlocked { id: String, error: String },
    MemoryPressure { id: String, peak_bytes: u64 },
    /// Què està fent la IA ARA mateix dins d'una tasca (p. ex. «cercant a
    /// internet…», «generant codi…»). Va a la UI perquè l'usuari vegi que
    /// la Jam està viva, no penjada.
    Note { id: String, text: String },
    /// Extracte del raonament del model (prosa + línia RESUM, sense blocs de
    /// codi). És el «procés de pensament» que l'usuari vol veure.
    Thinking { id: String, text: String },
    Compacted,
    Finished,
}

/// Receptor de progrés (opcional).
#[async_trait::async_trait]
pub trait ProgressSink: Send + Sync {
    async fn on_progress(&self, ev: Progress);
}

/// Receptor buit (no fa res).
pub struct NoopProgress;
#[async_trait::async_trait]
impl ProgressSink for NoopProgress {
    async fn on_progress(&self, _ev: Progress) {}
}

pub struct TaskContext {
    pub cancel: CancellationToken,
    pub child_registry: Arc<tokio::sync::Mutex<Vec<u32>>>,
    pub quality: QualityLevel,
    pub attempt: u32,
}

/// Punt de desament després de cada tasca.
///
/// Abans era `Fn(usize, &JamStats)`: sense SHA ni qualitat, l'estat desat no
/// deia a QUIN commit del repo s'havia de tornar en reprendre (i el
/// `save_fn` de l'app havia de fer el seu `git rev-parse HEAD` propi, amb el
/// risc de desar un sha que encara no existia).
#[derive(Debug, Clone)]
pub struct SaveInfo {
    /// Índex de la següent tasca per executar.
    pub next_idx: usize,
    pub stats: JamStats,
    /// SHA del darrer commit BO (`last_good`).
    pub last_good_sha: String,
    /// Qualitat real amb què s'ha fet la darrera tasca.
    pub quality: QualityLevel,
    /// Id de la tasca que just s'ha tancat.
    pub task_id: String,
}

pub struct JamDeps {
    pub worker: Arc<dyn Worker>,
    pub verifier: Arc<dyn Verifier>,
    pub ckpt: Arc<dyn Checkpointer>,
    pub reporter: Arc<dyn Reporter>,
    pub cache: Arc<dyn Cache>,
    pub repo_path: PathBuf,
    /// Receptor de progrés (opcional): rep els esdeveniments del bucle.
    pub progress: Option<Arc<dyn ProgressSink>>,
    /// Funció de persistència: es crida després de cada tasca amb el punt de
    /// desament complet (idx, stats, sha i qualitat) per poder reprendre la
    /// Jam després d'un crash O d'un reinici del procés per pressió de memòria.
    pub save_fn: Option<Arc<dyn Fn(&SaveInfo) + Send + Sync>>,
}

impl JamDeps {
    /// Deps sense receptor de progrés (per comoditat).
    pub fn without_progress(
        worker: Arc<dyn Worker>,
        verifier: Arc<dyn Verifier>,
        ckpt: Arc<dyn Checkpointer>,
        reporter: Arc<dyn Reporter>,
        cache: Arc<dyn Cache>,
        repo_path: PathBuf,
    ) -> Self {
        Self { worker, verifier, ckpt, reporter, cache, repo_path, progress: None, save_fn: None }
    }
    async fn progress(&self, ev: Progress) {
        if let Some(p) = &self.progress {
            p.on_progress(ev).await;
        }
    }
}

pub struct JamConfig {
    pub task_memory_budget_bytes: u64,
    pub max_memory_failures_per_task: u32,
    /// Temps màxim d'UNA intent de worker (generar + escriure els fitxers).
    /// Amb models locals sobre CPU (un Mac intel amb pocs nuclis) un fitxer
    /// de 300 línies pot tardar MOLTS minuts: amb un sostre baix (abans eren
    /// 5 minuts fixes) TODUS els intents morien en timeout, la tasca es
    /// bloquejava i la Jam acabava sense haver escrit codi.
    pub worker_timeout: Duration,
}
impl Default for JamConfig {
    fn default() -> Self {
        Self {
            task_memory_budget_bytes: 6*1024*1024*1024,
            max_memory_failures_per_task: 2,
            worker_timeout: Duration::from_secs(45 * 60),
        }
    }
}

pub struct JamRuntime {
    compact: Arc<AtomicBool>,
    stop: CancellationToken,
    bg: Option<JoinHandle<()>>,
    /// Sender del canal `watch` en mode standalone (evita que el canal es
    /// tanqui i el runtime es cancel·li sol).
    keepalive: Option<watch::Sender<ChildCommand>>,
}
impl JamRuntime {
    pub fn new(mut cmd_rx: watch::Receiver<ChildCommand>) -> Self {
        let compact = Arc::new(AtomicBool::new(false));
        let stop = CancellationToken::new();
        match *cmd_rx.borrow_and_update() {
            ChildCommand::Compact => compact.store(true, Ordering::SeqCst),
            ChildCommand::Stop => stop.cancel(),
            ChildCommand::Running => {}
        }
        let compact_bg = compact.clone();
        let stop_bg = stop.clone();
        let mut rx_bg = cmd_rx.clone();
        let bg = tokio::spawn(async move {
            loop {
                if rx_bg.changed().await.is_err() { stop_bg.cancel(); return; }
                match *rx_bg.borrow_and_update() {
                    ChildCommand::Compact => compact_bg.store(true, Ordering::SeqCst),
                    ChildCommand::Stop => { stop_bg.cancel(); return; }
                    ChildCommand::Running => {}
                }
            }
        });
        Self { compact, stop, bg: Some(bg), keepalive: None }
    }
    pub fn is_stopped(&self) -> bool { self.stop.is_cancelled() }
    pub fn take_compact(&self) -> bool { self.compact.swap(false, Ordering::SeqCst) }
    pub async fn cancelled(&self) { self.stop.cancelled().await; }
    pub fn stop_token(&self) -> CancellationToken { self.stop.clone() }
    pub fn attach_external_cancel(&self, external: CancellationToken) {
        let internal = self.stop.clone();
        tokio::spawn(async move { external.cancelled().await; internal.cancel(); });
    }
    /// Runtime AUTÒNOM (sense guardian): crea el seu propi canal `watch`
    /// i el manté viu, de manera que el bucle no s'atura sol.
    pub fn standalone() -> Self {
        let (tx, rx) = watch::channel(ChildCommand::Running);
        // El sender viu dins el runtime: el canal no es tanca mai.
        let mut rt = Self::new(rx);
        rt.keepalive = Some(tx);
        rt
    }
}
impl Drop for JamRuntime {
    fn drop(&mut self) { if let Some(h) = self.bg.take() { h.abort(); } }
}

const MAX_ATTEMPTS_PER_TASK: u32 = 3;
const VERIFY_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const APPLY_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_DIFF_LINES: usize = 500;
const MAX_DIFF_FILES: usize = 10;

pub async fn run_autonomous_loop(rt: JamRuntime, plan: Plan, deps: JamDeps, cfg: JamConfig) {
    run_autonomous_loop_with(rt, plan, deps, cfg, 0).await
}

pub async fn run_autonomous_loop_with(rt: JamRuntime, plan: Plan, deps: JamDeps, cfg: JamConfig, start_idx: usize) {
    run_autonomous_loop_resumable(rt, plan, deps, cfg, start_idx, None).await
}

pub async fn run_autonomous_loop_resumable(
    rt: JamRuntime,
    plan: Plan,
    deps: JamDeps,
    cfg: JamConfig,
    start_idx: usize,
    initial_stats: Option<JamStats>,
) {
    let start = Instant::now();
    let mut stats = run_inner(&rt, &plan, &deps, &cfg, start_idx, initial_stats).await;
    let _ = deps.ckpt.cleanup().await;
    stats.duration_ms = start.elapsed().as_millis() as u64;
    // L'informe s'escriu DESPRÉS de l'últim `reset_hard` de `run_inner`: si no,
    // el `git clean -fd` del rollback s'enduria el REPORT.md (és a terra sense
    // comitar).
    let _ = deps.reporter.report(&plan, &stats).await;
}

async fn run_inner(
    rt: &JamRuntime,
    plan: &Plan,
    deps: &JamDeps,
    cfg: &JamConfig,
    start_idx: usize,
    initial_stats: Option<JamStats>,
) -> JamStats {
    let mut stats = initial_stats.unwrap_or_default();
    let deadline = Instant::now().checked_add(plan.duration).unwrap_or_else(Instant::now);
    let mut last_good = match deps.ckpt.head().await {
        Ok(sha) => sha,
        Err(e) => { warn!("head: {e}"); stats.skipped = plan.tasks.len() as u32; return stats; }
    };
    let mut tracker = PhaseTracker::new(plan.budget.clone());
    let mut global_quality = QualityLevel::Full;
    let mut last_quality = QualityLevel::Full;
    let mut consecutive_memory_fails: u32 = 0;

    'outer: for (idx, task) in plan.tasks.iter().enumerate().skip(start_idx) {
        if rt.is_stopped() || Instant::now() >= deadline { stats.skipped += (plan.tasks.len() - idx) as u32; break; }
        if rt.take_compact() { deps.cache.clear().await; deps.progress(Progress::Compacted).await; }
        let phase = task.phase.unwrap_or(Phase::General);
        if tracker.is_over_budget(phase) { stats.skipped += 1; continue; }
        let task_start = Instant::now();
        let mut task_ok = false;
        let mut memory_fails: u32 = 0;
        deps.progress(Progress::TaskStart { id: task.id.clone(), title: task.title.clone() }).await;
        for attempt in 1..=MAX_ATTEMPTS_PER_TASK {
            stats.total_attempts += 1;
            if rt.is_stopped() { stats.skipped += (plan.tasks.len() - idx) as u32; break 'outer; }
            let quality = if memory_fails >= cfg.max_memory_failures_per_task { QualityLevel::Minimal }
                else if memory_fails > 0 { QualityLevel::Reduced } else { global_quality };
            last_quality = quality;
            let guard = noorbit_memguard::MemoryGuard::spawn(cfg.task_memory_budget_bytes);
            let ctx = TaskContext { cancel: guard.cancel.clone(), child_registry: guard.registry.clone(), quality, attempt };
            if let Err(e) = deps.ckpt.reset_hard(&last_good).await { warn!(task = %task.id, attempt, "reset: {e}"); guard.stop(); break; }
            let worker_result = tokio::select! {
                biased;
                _ = rt.cancelled() => { guard.stop(); stats.skipped += (plan.tasks.len() - idx) as u32; break 'outer; }
                r = tokio::time::timeout(cfg.worker_timeout, deps.worker.run(task, &ctx)) => r,
            };
            let peak = guard.peak_bytes();
            guard.stop(); let _ = guard.join().await;
            let diff = match worker_result {
                Ok(Ok(d)) => d,
                Ok(Err(e)) if e.contains("memory") => {
                    memory_fails += 1; consecutive_memory_fails += 1;
                    warn!(task = %task.id, attempt, peak_mb = peak / 1024 / 1024, "memory cancel");
                    deps.progress(Progress::MemoryPressure { id: task.id.clone(), peak_bytes: peak }).await;
                    if consecutive_memory_fails >= 3 {
                        global_quality = match global_quality { QualityLevel::Full => QualityLevel::Reduced, QualityLevel::Reduced => QualityLevel::Minimal, QualityLevel::Minimal => QualityLevel::Minimal };
                        warn!(?global_quality, "baixant qualitat global");
                        consecutive_memory_fails = 0;
                    }
                    continue;
                }
                Ok(Err(e)) => { warn!(task = %task.id, attempt, "worker: {e}"); deps.progress(Progress::TaskError { id: task.id.clone(), error: e }).await; continue; }
                Err(_) => { warn!(task = %task.id, attempt, "worker timeout"); deps.progress(Progress::TaskError { id: task.id.clone(), error: "temps esgotat".into() }).await; continue; }
            };
            memory_fails = 0; consecutive_memory_fails = 0;
            // Un diff materialitzat pot no tenir text de patch (només els
            // fitxers tocats): el límit de mida s'aplica al patch real.
            if !diff.materialized {
                if diff.line_count() == 0 { warn!(task = %task.id, attempt, "diff buit"); continue; }
                if diff.line_count() > MAX_DIFF_LINES || diff.file_count() > MAX_DIFF_FILES { warn!(task = %task.id, attempt, "diff massa gran"); continue; }
            }
            // Patch normal: s'aplica. Materialitzat: el worker ja ha escrit
            // els fitxers (no s'aplica res o el `git apply` fallaria).
            if !diff.materialized {
                let apply_result = tokio::select! {
                    biased;
                    _ = rt.cancelled() => { let _ = deps.ckpt.reset_hard(&last_good).await; stats.skipped += (plan.tasks.len() - idx) as u32; break 'outer; }
                    r = apply_diff(&deps.repo_path, &diff.patch) => r,
                };
                if let Err(e) = apply_result { warn!(task = %task.id, attempt, "apply: {e}"); continue; }
            }
            // QUÈ hi ha realment a terra: si el worker no ha declarat els
            // fitxers, els preguntem a git. Això és el que permet al verifier
            // comprovar els criteris d'acceptació i a la UI dir quins fitxers
            // s'han tocat (abans rebeia un recompte fals o zero).
            let mut touched = diff.files.clone();
            if touched.is_empty() {
                touched = changed_files(&deps.repo_path).await;
            }
            let verify_result = tokio::select! {
                biased;
                _ = rt.cancelled() => { let _ = deps.ckpt.reset_hard(&last_good).await; stats.skipped += (plan.tasks.len() - idx) as u32; break 'outer; }
                r = tokio::time::timeout(VERIFY_TIMEOUT, deps.verifier.run_with_files(task, &ctx, &touched)) => r,
            };
            let verify_ok = match verify_result {
                Ok(Ok(())) => true,
                Ok(Err(m)) => { warn!(task = %task.id, attempt, "verify: {m}"); deps.progress(Progress::TaskError { id: task.id.clone(), error: m }).await; false }
                Err(_) => { warn!(task = %task.id, attempt, "verify timeout"); deps.progress(Progress::TaskError { id: task.id.clone(), error: "verificació: temps esgotat".into() }).await; false }
            };
            if !verify_ok { continue; }
            match deps.ckpt.commit(task).await {
                Ok(sha) => { last_good = sha; stats.completed += 1; task_ok = true; deps.progress(Progress::TaskDone { id: task.id.clone(), files: touched.len() }).await; break; }
                Err(e) => { warn!(task = %task.id, attempt, "commit: {e}"); deps.progress(Progress::TaskError { id: task.id.clone(), error: format!("commit: {e}") }).await; let _ = deps.ckpt.reset_hard(&last_good).await; continue; }
            }
        }
        if !task_ok && !rt.is_stopped() {
            stats.blocked += 1;
            deps.progress(Progress::TaskBlocked { id: task.id.clone(), error: format!("bloquejada després de {MAX_ATTEMPTS_PER_TASK} intents") }).await;
        }
        tracker.record(phase, task_start.elapsed());

        // Persistència: desa l'estat després de cada tasca, amb el SHA del
        // darrer commit bo i la qualitat REAL. Sense això la represa tornava a
        // un estat desconegut (o refasia feina ja comitada).
        if let Some(save_fn) = &deps.save_fn {
            save_fn(&SaveInfo {
                next_idx: idx + 1,
                stats: stats.clone(),
                last_good_sha: last_good.clone(),
                quality: last_quality,
                task_id: task.id.clone(),
            });
        }
    }
    let _ = deps.ckpt.reset_hard(&last_good).await;
    let _ = deps.ckpt.tag("jam-final").await;
    deps.progress(Progress::Finished).await;
    stats
}

async fn apply_diff(repo: &Path, patch: &str) -> Result<(), String> {
    if patch.trim().is_empty() { return Err("patch buit".into()); }
    for line in patch.lines() {
        let rest = line.strip_prefix("+++ b/").or_else(|| line.strip_prefix("--- a/"));
        if let Some(rest) = rest {
            let p = Path::new(rest);
            if p.is_absolute() { return Err(format!("patch amb path absolut: {rest}")); }
            for comp in p.components() { if matches!(comp, std::path::Component::ParentDir) { return Err(format!("patch amb ..: {rest}")); } }
        }
    }
    let mut child = Command::new("git").arg("-C").arg(repo).args(["apply", "--whitespace=nowarn", "-"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped()).kill_on_drop(true).spawn().map_err(|e| format!("spawn git apply: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() { stdin.write_all(patch.as_bytes()).await.map_err(|e| format!("escriptura stdin git: {e}"))?; }
    let out = tokio::time::timeout(APPLY_TIMEOUT, child.wait_with_output()).await.map_err(|_| "git apply timeout".to_string())?.map_err(|e| format!("wait git apply: {e}"))?;
    if !out.status.success() { return Err(format!("git apply fallà: {}", String::from_utf8_lossy(&out.stderr).trim())); }
    Ok(())
}

/// Llista de fitxers REALMENT tocats al working tree (camins relatius al
/// repo). Retorna buit si git falla o si el comandament trigui massa: qui el
/// crida ho tracta com a «no ho sé», mai com a «no ha canviat res».
pub async fn changed_files(repo: &Path) -> Vec<String> {
    const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
    let spawned = tokio::time::timeout(
        STATUS_TIMEOUT,
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["status", "--porcelain", "-uall"])
            .output(),
    )
    .await;
    let Ok(Ok(out)) = spawned else { return Vec::new(); };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            // Format `XY <camí>`: dos caràcters d'estat + un espai.
            let path = l.get(3..)?;
            // Un rename ve com `R  vell -> nou`: ens interessa el nou.
            let path = path.rsplit(" -> ").next().unwrap_or(path);
            let path = path.trim().trim_matches('"');
            if path.is_empty() {
                None
            } else {
                Some(path.to_string())
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;

    // ── Dobles de prova ────────────────────────────────────────────────────

    struct MemCheckpointer {
        head: Mutex<String>,
        commits: std::sync::atomic::AtomicUsize,
        cleaned: std::sync::atomic::AtomicBool,
    }
    impl Default for MemCheckpointer {
        fn default() -> Self {
            Self {
                head: Mutex::new("base".into()),
                commits: std::sync::atomic::AtomicUsize::new(0),
                cleaned: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }
    #[async_trait::async_trait]
    impl Checkpointer for MemCheckpointer {
        async fn head(&self) -> Result<String, String> {
            Ok(self.head.lock().unwrap().clone())
        }
        async fn reset_hard(&self, _sha: &str) -> Result<(), String> {
            Ok(())
        }
        async fn commit(&self, _task: &Task) -> Result<String, String> {
            self.commits
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let sha = format!(
                "sha{}",
                self.commits.load(std::sync::atomic::Ordering::SeqCst)
            );
            *self.head.lock().unwrap() = sha.clone();
            Ok(sha)
        }
        async fn tag(&self, _name: &str) -> Result<(), String> {
            Ok(())
        }
        async fn cleanup(&self) -> Result<(), String> {
            self.cleaned.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    struct MemReporter {
        stats: Mutex<Option<JamStats>>,
    }
    impl Default for MemReporter {
        fn default() -> Self {
            Self {
                stats: Mutex::new(None),
            }
        }
    }
    #[async_trait::async_trait]
    impl Reporter for MemReporter {
        async fn report(&self, _plan: &Plan, stats: &JamStats) -> Result<(), String> {
            *self.stats.lock().unwrap() = Some(stats.clone());
            Ok(())
        }
    }

    struct NoopCache;
    #[async_trait::async_trait]
    impl Cache for NoopCache {
        async fn clear(&self) {}
    }

    struct MemProgress {
        events: Mutex<Vec<Progress>>,
    }
    impl Default for MemProgress {
        fn default() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }
    }
    #[async_trait::async_trait]
    impl ProgressSink for MemProgress {
        async fn on_progress(&self, ev: Progress) {
            self.events.lock().unwrap().push(ev);
        }
    }

    fn task(id: &str) -> Task {
        Task {
            id: id.to_string(),
            title: format!("tasca {id}"),
            description: "escriu un fitxer".into(),
            acceptance: vec!["hi ha fitxer".into()],
            phase: None,
        }
    }

    fn plan(tasks: Vec<Task>) -> Plan {
        Plan {
            title: "Jam de prova".into(),
            tasks,
            duration: Duration::from_secs(300),
            budget: TimeBudget::from_total(Duration::from_secs(300)),
        }
    }

    #[tokio::test]
    async fn el_bucle_complet_completa_les_tasques_i_fa_commit() {
        // Use a worker that materializes the file. We need a real worker
        // but the noorbit-jam crate doesn't have one. Use a stub that
        // returns a materialized diff.
        struct StubWorker;
        #[async_trait::async_trait]
        impl Worker for StubWorker {
            async fn run(&self, task: &Task, _ctx: &TaskContext) -> Result<Diff, String> {
                Ok(Diff::materialized(format!("--- a/{}\n+++ b/{}\n", task.id, task.id), 0))
            }
        }
        let worker: Arc<dyn Worker> = Arc::new(StubWorker);
        let verifier: Arc<dyn Verifier> = Arc::new(StubVerifier);
        let ckpt_inner = Arc::new(MemCheckpointer::default());
        let ckpt = ckpt_inner.clone() as Arc<dyn Checkpointer>;
        let reporter_inner = Arc::new(MemReporter::default());
        let reporter = reporter_inner.clone() as Arc<dyn Reporter>;
        let cache = Arc::new(NoopCache) as Arc<dyn Cache>;

        let deps = JamDeps {
            worker,
            verifier,
            ckpt,
            reporter,
            cache,
            repo_path: std::path::PathBuf::from("/tmp"),
            progress: None,
            save_fn: None,
        };

        let p = plan(vec![task("t01"), task("t02")]);
        let rt = JamRuntime::standalone();
        let cfg = JamConfig::default();

        run_autonomous_loop(rt, p, deps, cfg).await;

        let stats = reporter_inner.stats.lock().unwrap().clone().unwrap();
        assert_eq!(stats.completed, 2, "haurien de completar 2 tasques");
        assert_eq!(stats.blocked, 0);

        assert_eq!(
            ckpt_inner.commits.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "hauria d'haver fet 2 commits"
        );

        assert!(ckpt_inner.cleaned.load(std::sync::atomic::Ordering::SeqCst));
    }

    struct StubVerifier;
    #[async_trait::async_trait]
    impl Verifier for StubVerifier {
        async fn run(&self, _task: &Task, _ctx: &TaskContext) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn el_bucle_emet_progres_per_a_cada_tasca() {
        struct StubWorker;
        #[async_trait::async_trait]
        impl Worker for StubWorker {
            async fn run(&self, task: &Task, _ctx: &TaskContext) -> Result<Diff, String> {
                Ok(Diff::materialized(format!("--- a/{}\n+++ b/{}\n", task.id, task.id), 0))
            }
        }
        let worker: Arc<dyn Worker> = Arc::new(StubWorker);
        let verifier: Arc<dyn Verifier> = Arc::new(StubVerifier2);
        let ckpt = Arc::new(MemCheckpointer::default()) as Arc<dyn Checkpointer>;
        let reporter_inner = Arc::new(MemReporter::default());
        let reporter = reporter_inner.clone() as Arc<dyn Reporter>;
        let cache = Arc::new(NoopCache) as Arc<dyn Cache>;
        let progress = Arc::new(MemProgress::default());

        let deps = JamDeps {
            worker,
            verifier,
            ckpt,
            reporter,
            cache,
            repo_path: std::path::PathBuf::from("/tmp"),
            progress: Some(progress.clone() as Arc<dyn ProgressSink>),
            save_fn: None,
        };

        let p = plan(vec![task("t01")]);
        run_autonomous_loop(JamRuntime::standalone(), p, deps, JamConfig::default()).await;

        let evs = progress.events.lock().unwrap().clone();
        assert!(
            evs.iter()
                .any(|e| matches!(e, Progress::TaskStart { id, .. } if id == "t01")),
            "hauria d'haver-hi un TaskStart: {:?}",
            evs
        );
        assert!(
            evs.iter()
                .any(|e| matches!(e, Progress::TaskDone { id, .. } if id == "t01")),
            "hauria d'haver-hi un TaskDone: {:?}",
            evs
        );
        assert!(
            evs.iter()
                .any(|e| matches!(e, Progress::Finished)),
            "hauria d'haver-hi un Finished: {:?}",
            evs
        );
    }

    struct StubVerifier2;
    #[async_trait::async_trait]
    impl Verifier for StubVerifier2 {
        async fn run(&self, _task: &Task, _ctx: &TaskContext) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn el_bucle_bloqueja_si_el_worker_falla() {
        struct FailWorker;
        #[async_trait::async_trait]
        impl Worker for FailWorker {
            async fn run(&self, _task: &Task, _ctx: &TaskContext) -> Result<Diff, String> {
                Err("boom".into())
            }
        }
        let worker: Arc<dyn Worker> = Arc::new(FailWorker);
        let verifier: Arc<dyn Verifier> = Arc::new(StubVerifier3);
        let ckpt = Arc::new(MemCheckpointer::default()) as Arc<dyn Checkpointer>;
        let reporter_inner = Arc::new(MemReporter::default());
        let reporter = reporter_inner.clone() as Arc<dyn Reporter>;
        let cache = Arc::new(NoopCache) as Arc<dyn Cache>;

        let deps = JamDeps {
            worker,
            verifier,
            ckpt,
            reporter: reporter.clone(),
            cache,
            repo_path: std::path::PathBuf::from("/tmp"),
            progress: None,
            save_fn: None,
        };

        let p = plan(vec![task("t01")]);
        run_autonomous_loop(JamRuntime::standalone(), p, deps, JamConfig::default()).await;

        let stats = reporter_inner.stats.lock().unwrap().clone().unwrap();
        assert_eq!(stats.completed, 0);
        assert_eq!(stats.blocked, 1);
    }

    struct StubVerifier3;
    #[async_trait::async_trait]
    impl Verifier for StubVerifier3 {
        async fn run(&self, _task: &Task, _ctx: &TaskContext) -> Result<(), String> {
            Ok(())
        }
    }

    /// REGRESSIÓ: el verifier rep els fitxers REALMENT tocats. Amb un diff
    /// materialitzat i `patch` buit, abans arribava una llista buida i cap
    /// criteri d'acceptació es podia comprovar.
    #[tokio::test]
    async fn el_verifier_rep_els_fitxers_tocats() {
        struct FilesWorker;
        #[async_trait::async_trait]
        impl Worker for FilesWorker {
            async fn run(&self, _task: &Task, _ctx: &TaskContext) -> Result<Diff, String> {
                Ok(Diff::materialized_files(vec!["src/hud.gd".into(), "src/player.gd".into()], 0))
            }
        }
        struct SpyVerifier {
            seen: Mutex<Vec<Vec<String>>>,
        }
        #[async_trait::async_trait]
        impl Verifier for SpyVerifier {
            async fn run(&self, _task: &Task, _ctx: &TaskContext) -> Result<(), String> {
                unreachable!("el bucle ha de cridar run_with_files")
            }
            async fn run_with_files(
                &self,
                _task: &Task,
                _ctx: &TaskContext,
                files: &[String],
            ) -> Result<(), String> {
                self.seen.lock().unwrap().push(files.to_vec());
                Ok(())
            }
        }

        let spy = Arc::new(SpyVerifier { seen: Mutex::new(Vec::new()) });
        let deps = JamDeps {
            worker: Arc::new(FilesWorker),
            verifier: spy.clone() as Arc<dyn Verifier>,
            ckpt: Arc::new(MemCheckpointer::default()) as Arc<dyn Checkpointer>,
            reporter: Arc::new(MemReporter::default()) as Arc<dyn Reporter>,
            cache: Arc::new(NoopCache) as Arc<dyn Cache>,
            repo_path: std::path::PathBuf::from("/tmp"),
            progress: None,
            save_fn: None,
        };
        run_autonomous_loop(JamRuntime::standalone(), plan(vec![task("t01")]), deps, JamConfig::default()).await;

        let seen = spy.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 1, "el verifier s'ha cridat una vegada");
        assert_eq!(seen[0], vec!["src/hud.gd".to_string(), "src/player.gd".to_string()]);
    }

    /// REGRESSIÓ: el desament porta el SHA del darrer commit bo i la
    /// qualitat real. Sense SHA, una represa no sap a on ha de tornar el repo.
    #[tokio::test]
    async fn el_desament_porta_sha_i_qualitat() {
        struct StubWorker;
        #[async_trait::async_trait]
        impl Worker for StubWorker {
            async fn run(&self, task: &Task, _ctx: &TaskContext) -> Result<Diff, String> {
                Ok(Diff::materialized_files(vec![format!("{}", task.id)], 0))
            }
        }
        let saves: Arc<Mutex<Vec<SaveInfo>>> = Arc::new(Mutex::new(Vec::new()));
        let saves2 = saves.clone();
        let deps = JamDeps {
            worker: Arc::new(StubWorker),
            verifier: Arc::new(StubVerifier) as Arc<dyn Verifier>,
            ckpt: Arc::new(MemCheckpointer::default()) as Arc<dyn Checkpointer>,
            reporter: Arc::new(MemReporter::default()) as Arc<dyn Reporter>,
            cache: Arc::new(NoopCache) as Arc<dyn Cache>,
            repo_path: std::path::PathBuf::from("/tmp"),
            progress: None,
            save_fn: Some(Arc::new(move |info: &SaveInfo| saves2.lock().unwrap().push(info.clone()))),
        };
        run_autonomous_loop(
            JamRuntime::standalone(),
            plan(vec![task("t01"), task("t02")]),
            deps,
            JamConfig::default(),
        )
        .await;

        let v = saves.lock().unwrap().clone();
        assert_eq!(v.len(), 2, "una desada per tasca");
        assert_eq!(v[0].next_idx, 1);
        assert_eq!(v[1].next_idx, 2);
        assert_eq!(v[0].last_good_sha, "sha1", "el SHA ha de ser el de la tasca recent comitada");
        assert_eq!(v[1].last_good_sha, "sha2");
        assert_eq!(v[1].quality, QualityLevel::Full);
        assert_eq!(v[1].task_id, "t02");
        assert_eq!(v[1].stats.completed, 2);
    }

    /// `changed_files` ha de veure fitxers nous (sense `git add`), que és el
    /// cas normal d'un worker que escriu i encara no ha comitat.
    #[tokio::test]
    async fn changed_files_veu_fitxers_nous() {
        let dir = std::env::temp_dir().join(format!("noorbit-jam-changed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let init = std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["init", "-q"])
            .status();
        if !matches!(init, Ok(s) if s.success()) {
            // Entorn sense git: no té sentit fallar per això.
            eprintln!("git no disponible: test omès");
            return;
        }
        std::fs::write(dir.join("novet.txt"), "hola\n").unwrap();
        let files = changed_files(&dir).await;
        assert!(
            files.iter().any(|f| f == "novet.txt"),
            "ha de veure l'arxiu sense trackejar: {files:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
