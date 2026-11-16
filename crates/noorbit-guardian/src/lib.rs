// noorbit-guardian v3.3
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub mod child {
    pub use super::ChildRuntime;
    pub use super::ChildCommand;
}

const WARMUP: Duration = Duration::from_secs(15);
const COOLDOWN: Duration = Duration::from_secs(10);
const SIGTERM_GRACE: Duration = Duration::from_millis(500);
const STOP_GRACE: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct GuardianConfig {
    pub agent_rlimit_bytes: u64,
    pub swap_soft_bytes: u64,
    pub swap_hard_bytes: u64,
    pub available_soft_bytes: u64,
    pub available_hard_bytes: u64,
    pub agent_rss_soft_bytes: u64,
    pub agent_rss_hard_bytes: u64,
    pub poll_slow: Duration,
    pub poll_fast: Duration,
    pub max_restarts_per_hour: u32,
    pub nice: i32,
    /// Límit de TEMPS de CPU del fill (segons). 0 = sense límit.
    /// A macOS és l'únic límit de recursos que el kernel aplica de veritat
    /// (a part del temps real), i evita que un agent en bucle infiní
    /// consumeixi la màquina.
    pub agent_cpu_limit_secs: u64,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            agent_rlimit_bytes: 6 * 1024 * 1024 * 1024,
            swap_soft_bytes: 1024 * 1024 * 1024,
            swap_hard_bytes: 3 * 1024 * 1024 * 1024,
            available_soft_bytes: 2 * 1024 * 1024 * 1024,
            available_hard_bytes: 512 * 1024 * 1024,
            agent_rss_soft_bytes: 4 * 1024 * 1024 * 1024,
            agent_rss_hard_bytes: 5500 * 1024 * 1024,
            poll_slow: Duration::from_secs(2),
            poll_fast: Duration::from_millis(500),
            max_restarts_per_hour: 10,
            nice: 10,
            agent_cpu_limit_secs: 0,
        }
    }
}

impl GuardianConfig {
    pub fn for_ram(total_bytes: u64) -> Self {
        let mut c = Self::default();
        let gb = total_bytes / (1024 * 1024 * 1024);
        match gb {
            0..=8 => {
                c.agent_rlimit_bytes = 3 * 1024 * 1024 * 1024;
                c.agent_rss_hard_bytes = 2500 * 1024 * 1024;
                c.agent_rss_soft_bytes = 2 * 1024 * 1024 * 1024;
                c.swap_hard_bytes = 1500 * 1024 * 1024;
            }
            9..=16 => {}
            17..=32 => {
                c.agent_rlimit_bytes = 12 * 1024 * 1024 * 1024;
                c.agent_rss_hard_bytes = 10 * 1024 * 1024 * 1024;
                c.agent_rss_soft_bytes = 8 * 1024 * 1024 * 1024;
                c.swap_soft_bytes = 2 * 1024 * 1024 * 1024;
                c.swap_hard_bytes = 4 * 1024 * 1024 * 1024;
            }
            _ => {
                c.agent_rlimit_bytes = 24 * 1024 * 1024 * 1024;
                c.agent_rss_hard_bytes = 20 * 1024 * 1024 * 1024;
                c.agent_rss_soft_bytes = 16 * 1024 * 1024 * 1024;
            }
        }
        c
    }
    pub fn for_7gb() -> Self {
        Self {
            agent_rlimit_bytes: 7 * 1024 * 1024 * 1024,
            agent_rss_hard_bytes: 6500 * 1024 * 1024,
            agent_rss_soft_bytes: 6000 * 1024 * 1024,
            swap_soft_bytes: 1024 * 1024 * 1024,
            swap_hard_bytes: 2 * 1024 * 1024 * 1024,
            available_soft_bytes: 1024 * 1024 * 1024,
            available_hard_bytes: 512 * 1024 * 1024,
            poll_slow: Duration::from_secs(2),
            poll_fast: Duration::from_millis(500),
            max_restarts_per_hour: 5,
            nice: 15,
            agent_cpu_limit_secs: 0,
        }
    }
    pub fn with_blender(mut self) -> Self {
        self.agent_rlimit_bytes = self.agent_rlimit_bytes.max(16 * 1024 * 1024 * 1024);
        self.agent_rss_hard_bytes = self.agent_rss_hard_bytes.max(12 * 1024 * 1024 * 1024);
        self.agent_rss_soft_bytes = self.agent_rss_soft_bytes.max(9 * 1024 * 1024 * 1024);
        self.swap_soft_bytes = self.swap_soft_bytes.max(2 * 1024 * 1024 * 1024);
        self.swap_hard_bytes = self.swap_hard_bytes.max(5 * 1024 * 1024 * 1024);
        self
    }
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.agent_rlimit_bytes == 0 { return Err("rlimit > 0".into()); }
        if self.swap_soft_bytes >= self.swap_hard_bytes { return Err("swap_soft < swap_hard".into()); }
        if self.available_hard_bytes >= self.available_soft_bytes { return Err("available_hard < available_soft".into()); }
        if self.agent_rss_soft_bytes >= self.agent_rss_hard_bytes { return Err("rss_soft < rss_hard".into()); }
        if self.agent_rss_hard_bytes >= self.agent_rlimit_bytes { return Err("rss_hard < rlimit".into()); }
        if self.poll_slow.is_zero() || self.poll_fast.is_zero() { return Err("poll_* > 0".into()); }
        if !(0..=19).contains(&self.nice) { return Err("nice a [0, 19]".into()); }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum GuardianError {
    #[error("spawn del fill: {0}")]
    Spawn(String),
    #[error("protocol fill: {0}")]
    Protocol(String),
    #[error("timeout: {0}")]
    Timeout(&'static str),
    #[error("config: {0}")]
    Config(String),
    #[error("E/S: {0}")]
    Io(#[from] std::io::Error),
}
pub type Result<T> = std::result::Result<T, GuardianError>;

/// El `pre_exec` corre DESPRÉS del `fork` i ABANS del `exec`: només hi són
/// permeses crides async-signal-safe (cap allocació, cap mutex, cap log).
/// `tracing`/`warn!` agafen un mutex que un altre fil pot tenir agafat en el
/// moment del fork → el fill ES QUEDA PENJAT PER SEMPRE abans d'arrencar.
#[cfg(unix)]
fn note(msg: &str) {
    let bytes = msg.as_bytes();
    let mut written = 0usize;
    while written < bytes.len() {
        // `write(2)` és async-signal-safe; si falla (disc/pipe tancat) parem.
        let n = unsafe { libc::write(libc::STDERR_FILENO, bytes[written..].as_ptr() as *const _, bytes.len() - written) };
        if n <= 0 {
            break;
        }
        written += n as usize;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StopReason { SwapHard, AvailableHard, RssHard, ChildDied, RlimitHit, Manual }

#[derive(Debug, Clone, Serialize)]
pub struct StatsSummary {
    pub samples: u64,
    pub peak_swap_bytes: u64,
    pub peak_rss_bytes: u64,
    pub min_available_bytes: Option<u64>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GuardianEvent {
    Started { pid: u32, rlimit_bytes: u64 },
    Sample { swap_bytes: u64, available_bytes: u64, agent_rss_bytes: u64, uptime_ms: u64 },
    CompactionRequested { reason: &'static str },
    AgentStopping { reason: StopReason },
    AgentStopped { took_ms: u128, forced: bool },
    AgentRestarted { new_pid: u32, restarts_this_hour: u32 },
    GivingUp { restarts_this_hour: u32 },
    Summary(StatsSummary),
    /// Línia de sortida del fill (el protocol `PING`→`PONG` i els avisos).
    ChildLine(String),
    Stopped,
}

#[derive(Default)]
struct Stats {
    samples: u64,
    peak_swap: u64,
    peak_rss: u64,
    min_available: Option<u64>,
}
impl Stats {
    fn update(&mut self, swap: u64, avail: Option<u64>, rss: u64) {
        self.samples += 1;
        self.peak_swap = self.peak_swap.max(swap);
        self.peak_rss = self.peak_rss.max(rss);
        if let Some(a) = avail {
            self.min_available = Some(match self.min_available { Some(m) => m.min(a), None => a });
        }
    }
    fn summary(&self, duration: Duration) -> StatsSummary {
        StatsSummary { samples: self.samples, peak_swap_bytes: self.peak_swap, peak_rss_bytes: self.peak_rss, min_available_bytes: self.min_available, duration_ms: duration.as_millis() as u64 }
    }
}

// metrics (macos)
#[cfg(target_os = "macos")]
mod macos_metrics {
    use std::ffi::CString;
    use std::mem;
    use std::os::raw::{c_int, c_void};
    extern "C" { fn os_proc_available_memory() -> usize; }
    pub fn available_before_jetsam() -> Option<u64> {
        let n = unsafe { os_proc_available_memory() };
        if n == 0 { None } else { Some(n as u64) }
    }
    pub fn swap_used() -> u64 {
        let name = match CString::new("vm.swapusage") { Ok(c) => c, Err(_) => return 0 };
        let mut size: libc::size_t = 0;
        let ret = unsafe { libc::sysctlbyname(name.as_ptr(), std::ptr::null_mut(), &mut size, std::ptr::null_mut(), 0) };
        if ret != 0 || size < 24 { return 0; }
        let mut buf = vec![0u8; size];
        let ret = unsafe { libc::sysctlbyname(name.as_ptr(), buf.as_mut_ptr() as *mut c_void, &mut size, std::ptr::null_mut(), 0) };
        if ret != 0 { return 0; }
        let mut b = [0u8; 8];
        b.copy_from_slice(&buf[16..24]);
        u64::from_ne_bytes(b)
    }
    #[repr(C)]
    struct ProcTaskInfo {
        pti_virtual_size: u64, pti_resident_size: u64, pti_total_user: u64, pti_total_system: u64,
        pti_threads_user: u64, pti_threads_system: u64, pti_policy: i32, pti_faults: i32,
        pti_pageins: i32, pti_cow_faults: i32, pti_messages_sent: i32, pti_messages_received: i32,
        pti_syscalls_mach: i32, pti_syscalls_unix: i32, pti_csw: i32, pti_threadnum: i32,
        pti_numrunning: i32, pti_priority: i32,
    }
    extern "C" { fn proc_pidinfo(pid: c_int, flavor: c_int, arg: u64, buffer: *mut c_void, buffersize: c_int) -> c_int; }
    const PROC_PIDTASKINFO: c_int = 4;
    pub fn rss(pid: u32) -> u64 {
        if pid == 0 { return 0; }
        let mut info: ProcTaskInfo = unsafe { mem::zeroed() };
        let size = mem::size_of::<ProcTaskInfo>() as c_int;
        let n = unsafe { proc_pidinfo(pid as c_int, PROC_PIDTASKINFO, 0, &mut info as *mut _ as *mut c_void, size) };
        if n != size { return 0; }
        info.pti_resident_size
    }
}
#[cfg(target_os = "macos")]
fn available_before_jetsam() -> Option<u64> { macos_metrics::available_before_jetsam() }
#[cfg(target_os = "macos")]
fn swap_used() -> u64 { macos_metrics::swap_used() }
#[cfg(target_os = "macos")]
fn rss(pid: u32) -> u64 { macos_metrics::rss(pid) }

#[cfg(not(target_os = "macos"))]
fn swap_used() -> u64 {
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let get = |key: &str| -> u64 {
        meminfo.lines().find_map(|l| l.strip_prefix(key)).and_then(|s| s.split_whitespace().next()).and_then(|s| s.parse().ok()).unwrap_or(0)
    };
    get("SwapTotal:").saturating_sub(get("SwapFree:")) * 1024
}
#[cfg(not(target_os = "macos"))]
fn available_before_jetsam() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    meminfo.lines().find_map(|l| l.strip_prefix("MemAvailable:")).and_then(|s| s.split_whitespace().next()).and_then(|s| s.parse::<u64>().ok()).map(|kb| kb * 1024)
}
#[cfg(not(target_os = "macos"))]
fn rss(pid: u32) -> u64 {
    if pid == 0 { return 0; }
    let s = std::fs::read_to_string(format!("/proc/{pid}/statm")).unwrap_or_default();
    let r: u64 = s.split_whitespace().nth(1).and_then(|x| x.parse().ok()).unwrap_or(0);
    r * 4096
}

#[derive(Clone)]
pub struct SpawnSpec { pub program: String, pub args: Vec<String>, pub env: Vec<(String,String)>, pub cwd: Option<String> }

fn spawn_child(spec: &SpawnSpec, cfg: &GuardianConfig) -> Result<Child> {
    use std::process::Stdio;
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).kill_on_drop(true);
    #[cfg(unix)] cmd.process_group(0);
    if let Some(cwd) = &spec.cwd { cmd.current_dir(cwd); }
    for (k,v) in &spec.env { cmd.env(k,v); }
    #[cfg(unix)]
    unsafe {
        // A macOS `RLIMIT_AS` no existeix: ho avisem UNA vegada i DES DEL
        // PARE (dins `pre_exec` està prohibit qualsevol log: vegeu `note`).
        #[cfg(target_os = "macos")]
        {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                warn!("RLIMIT_AS no existeix a macOS: el límit de memòria del fill el fa la supervisió (RSS/swap)");
            });
        }
        #[cfg(not(target_os = "macos"))]
        let rlimit = cfg.agent_rlimit_bytes as libc::rlim_t;
        let nice = cfg.nice;
        let cpu_secs = cfg.agent_cpu_limit_secs;
        cmd.pre_exec(move || {
            // ── LÍMIT DE MEMÒRIA ────────────────────────────────────────────
            // BUG CRÍTIC (v3.4): a macOS/Darwin `RLIMIT_AS` NO EXISTEIX.
            // `setrlimit(RLIMIT_AS, …)` falla amb EINVAL, i com que el
            // `pre_exec` retornava Err, el fill NO S'ARRIBAVA A ESPAWNAR MAI:
            // el guardian era inútil al Mac (el supervisor es queixava de
            // «Invalid argument» i la Jam no arrencava).
            //
            // Ara: a macOS NI EL CRIDEM (falla sempre i brutes el stderr a
            // cada spawn). La protecció REAL a macOS la dona la
            // supervisió (RSS del fill + swap + RAM disponible), que atura el
            // fill a temps; la resta de UNIX sí que aplica el límit dur.
            #[cfg(not(target_os = "macos"))]
            if libc::setrlimit(libc::RLIMIT_AS, &libc::rlimit { rlim_cur: rlimit, rlim_max: rlimit }) != 0 {
                note("noorbit-guardian: no s'ha pogut posar RLIMIT_AS; el límit el fa la supervisió\n");
            }
            // Límit de TEMPS de CPU (sí que funciona a macOS): evita que un
            // agent en bucle infiní consumeixi la màquina.
            if cpu_secs > 0 {
                let lim_cpu = libc::rlimit { rlim_cur: cpu_secs, rlim_max: cpu_secs };
                libc::setrlimit(libc::RLIMIT_CPU, &lim_cpu);
            }
            // Prioritat: si no es pot, tampoc és motiu per no arrencar.
            if libc::setpriority(libc::PRIO_PROCESS, 0, nice) != 0 {
                note("noorbit-guardian: no s'ha pogut baixar la prioritat del fill\n");
            }
            #[cfg(target_os = "linux")] libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        });
    }
    cmd.spawn().map_err(|e| GuardianError::Spawn(format!("{}: {}", spec.program, e)))
}

struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    pid: u32,
    pgid: i32,
    started: Instant,
    reaped: bool,
    /// Tasca que llegeix la sortida del fill i l'emet com a esdeveniments.
    /// SENSE això el pipe s'omple, el fill es queda penjat escrivint i el
    /// supervisor no ho nota (bug clàssic de supervisors).
    stdout_task: Option<tokio::task::JoinHandle<()>>,
}
enum ExitOutcome { Running, Natural, Crashed(StopReason) }
impl Running {
    /// Espera (com a molt) la tasca de drenatge perquè acabi de llegir el que
    /// el fill ja tenia al pipe. Amb un `abort()` immediat es PERDEN les
    /// últimes línies `JAM|…`, que són justament les de l'última tasca acabada
    /// (i el resum final que el fill imprimeix abans de morir).
    async fn finish_drain(&mut self) {
        let Some(task) = self.stdout_task.take() else { return };
        tokio::pin!(task);
        if timeout(Duration::from_millis(250), &mut task).await.is_err() {
            // Un altre procés del grup té el pipe obert: no esperem més.
            task.abort();
        }
    }

    fn poll_exit(&mut self) -> ExitOutcome {
        if self.reaped { return ExitOutcome::Running; }
        match self.child.try_wait() {
            Ok(Some(status)) => {
                #[cfg(unix)] unsafe { if self.pgid > 0 && libc::kill(-self.pgid, 0) == 0 { libc::kill(-self.pgid, libc::SIGKILL); } }
                self.reaped = true;
                // NO fem `abort()` aquí: la tasca encara ha de veure EOF i
                // abocar les últimes línies (`finish_drain`).
                if status.success() { ExitOutcome::Natural } else {
                    #[cfg(unix)] { use std::os::unix::process::ExitStatusExt; if status.signal() == Some(libc::SIGKILL) { return ExitOutcome::Crashed(StopReason::RlimitHit); } }
                    ExitOutcome::Crashed(StopReason::ChildDied)
                }
            }
            _ => ExitOutcome::Running,
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        if let Some(t) = self.stdout_task.take() { t.abort(); }
        if self.reaped || self.pid == 0 || self.pgid <= 0 { return; }
        #[cfg(unix)] unsafe { if libc::kill(-self.pgid, 0) == 0 { libc::kill(-self.pgid, libc::SIGKILL); } }
    }
}

pub struct Supervisor {
    cfg: GuardianConfig,
    factory: Arc<dyn Fn() -> SpawnSpec + Send + Sync>,
    events: mpsc::UnboundedSender<GuardianEvent>,
    cancel: CancellationToken,
    stats: Stats,
    last_swap: u64,
    last_avail: u64,
}
impl Supervisor {
    pub fn new(cfg: GuardianConfig, factory: Arc<dyn Fn() -> SpawnSpec + Send + Sync>, events: mpsc::UnboundedSender<GuardianEvent>) -> std::result::Result<(Self, CancellationToken), String> {
        cfg.validate()?;
        if available_before_jetsam().is_none() { warn!("os_proc_available_memory no disponible"); }
        let cancel = CancellationToken::new();
        Ok((Self { cfg, factory, events, cancel: cancel.clone(), stats: Stats::default(), last_swap: 0, last_avail: u64::MAX }, cancel))
    }
    pub async fn run(mut self) -> Result<()> {
        let mut restarts: Vec<Instant> = Vec::new();
        let mut running = self.spawn_running()?;
        let mut last_restart = Instant::now();
        let session_start = Instant::now();
        self.emit(GuardianEvent::Started { pid: running.pid, rlimit_bytes: self.cfg.agent_rlimit_bytes });
        // Mostra inicial: així el resum SEMPRE té com a mínim una mesura,
        // encara que el fill surti immediatament (o que mori al primer instant).
        {
            let swap = swap_used();
            let avail = available_before_jetsam();
            let child_rss = rss(running.pid);
            self.last_swap = swap;
            self.last_avail = avail.unwrap_or(u64::MAX);
            self.stats.update(swap, avail, child_rss);
            self.emit(GuardianEvent::Sample {
                swap_bytes: swap,
                available_bytes: avail.unwrap_or(u64::MAX),
                agent_rss_bytes: child_rss,
                uptime_ms: 0,
            });
        }
        loop {
            let delay = self.next_delay();
            tokio::select! {
                _ = self.cancel.cancelled() => {
                    info!("cancel·lació rebuda");
                    self.stop_child(&mut running, true).await;
                    running.finish_drain().await;
                    self.emit(GuardianEvent::Summary(self.stats.summary(session_start.elapsed())));
                    self.emit(GuardianEvent::Stopped);
                    return Ok(());
                }
                _ = tokio::time::sleep(delay) => {
                    match running.poll_exit() {
                        ExitOutcome::Running => {}
                        ExitOutcome::Natural => {
                            info!("agent acabat naturalment");
                            running.finish_drain().await;
                            self.emit(GuardianEvent::Summary(self.stats.summary(session_start.elapsed())));
                            self.emit(GuardianEvent::Stopped);
                            return Ok(());
                        }
                        ExitOutcome::Crashed(reason) => {
                            running.finish_drain().await;
                            self.emit(GuardianEvent::AgentStopping { reason });
                            self.emit(GuardianEvent::AgentStopped { took_ms: 0, forced: false });
                            match self.handle_restart(&mut restarts)? {
                                Some(r) => { running = r; last_restart = Instant::now(); }
                                None => { self.give_up(&mut running, session_start).await; return Ok(()); }
                            }
                            continue;
                        }
                    }
                    let swap = swap_used();
                    let avail = available_before_jetsam();
                    let child_rss = rss(running.pid);
                    let uptime = running.started.elapsed();
                    self.last_swap = swap;
                    self.last_avail = avail.unwrap_or(u64::MAX);
                    self.stats.update(swap, avail, child_rss);
                    self.emit(GuardianEvent::Sample { swap_bytes: swap, available_bytes: avail.unwrap_or(u64::MAX), agent_rss_bytes: child_rss, uptime_ms: uptime.as_millis() as u64 });
                    if uptime < WARMUP || last_restart.elapsed() < COOLDOWN { continue; }
                    if let Some(reason) = self.hard_reason(swap, avail, child_rss) {
                        self.emit(GuardianEvent::AgentStopping { reason });
                        let (took_ms, forced) = self.stop_child(&mut running, false).await;
                        running.finish_drain().await;
                        self.emit(GuardianEvent::AgentStopped { took_ms, forced });
                        match self.handle_restart(&mut restarts)? {
                            Some(r) => { running = r; last_restart = Instant::now(); }
                            None => { self.give_up(&mut running, session_start).await; return Ok(()); }
                        }
                        continue;
                    }
                    if let Some(reason) = self.soft_reason(swap, avail, child_rss) {
                        self.emit(GuardianEvent::CompactionRequested { reason });
                        self.request_compact(&mut running).await;
                    }
                }
            }
        }
    }
    async fn give_up(&self, running: &mut Running, session_start: Instant) {
        info!("give-up");
        self.stop_child(running, true).await;
        // Drenem ABANS del resum: l'usuari ha de veure l'últim estat del fill.
        running.finish_drain().await;
        self.cancel.cancelled().await;
        self.emit(GuardianEvent::Summary(self.stats.summary(session_start.elapsed())));
        self.emit(GuardianEvent::Stopped);
    }
    fn next_delay(&self) -> Duration {
        let close = self.last_swap >= self.cfg.swap_soft_bytes || self.last_avail <= self.cfg.available_soft_bytes;
        if close { self.cfg.poll_fast } else { self.cfg.poll_slow }
    }
    fn hard_reason(&self, swap: u64, avail: Option<u64>, rss: u64) -> Option<StopReason> {
        if swap >= self.cfg.swap_hard_bytes { return Some(StopReason::SwapHard); }
        if let Some(a) = avail { if a <= self.cfg.available_hard_bytes { return Some(StopReason::AvailableHard); } }
        if rss >= self.cfg.agent_rss_hard_bytes { return Some(StopReason::RssHard); }
        None
    }
    fn soft_reason(&self, swap: u64, avail: Option<u64>, rss: u64) -> Option<&'static str> {
        if swap >= self.cfg.swap_soft_bytes { return Some("swap-soft"); }
        if let Some(a) = avail { if a <= self.cfg.available_soft_bytes { return Some("available-soft"); } }
        if rss >= self.cfg.agent_rss_soft_bytes { return Some("rss-soft"); }
        None
    }
    fn handle_restart(&self, restarts: &mut Vec<Instant>) -> Result<Option<Running>> {
        prune_old(restarts);
        if restarts.len() as u32 >= self.cfg.max_restarts_per_hour {
            warn!("limite reinicios superado");
            self.emit(GuardianEvent::GivingUp { restarts_this_hour: restarts.len() as u32 });
            return Ok(None);
        }
        let r = self.spawn_running()?;
        restarts.push(Instant::now());
        self.emit(GuardianEvent::AgentRestarted { new_pid: r.pid, restarts_this_hour: restarts.len() as u32 });
        Ok(Some(r))
    }
    fn spawn_running(&self) -> Result<Running> {
        let spec = (self.factory)();
        let mut child = spawn_child(&spec, &self.cfg)?;
        let pid = child.id().unwrap_or(0);
        let stdin = child.stdin.take();
        let pgid = pid as i32;
        // Drenatge de la sortida del fill: si no, el pipe s'omple i el fill
        // es queda penjat escrivint (sense que el supervisor se'n'ajudi).
        let stdout_task = child.stdout.take().map(|out| {
            let tx = self.events.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let line = line.trim().to_string();
                    if line.is_empty() { continue; }
                    if tx.send(GuardianEvent::ChildLine(line)).is_err() { break; }
                }
            })
        });
        Ok(Running { child, stdin, pid, pgid, started: Instant::now(), reaped: false, stdout_task })
    }
    async fn request_compact(&self, running: &mut Running) {
        let Some(stdin) = running.stdin.as_mut() else { return; };
        let _ = stdin.write_all(b"COMPACT\n").await;
        let _ = stdin.flush().await;
    }
    async fn stop_child(&self, running: &mut Running, force: bool) -> (u128, bool) {
        let start = Instant::now();
        if running.reaped { return (0, false); }
        if running.pid == 0 {
            let _ = running.child.kill().await;
            let _ = running.child.wait().await;
            running.reaped = true;
            return (start.elapsed().as_millis(), true);
        }
        if !force {
            if let Some(mut stdin) = running.stdin.take() {
                let _ = stdin.write_all(b"STOP\n").await;
                let _ = stdin.flush().await;
            }
            if timeout(STOP_GRACE, running.child.wait()).await.is_ok() {
                running.reaped = true;
                #[cfg(unix)] unsafe { if libc::kill(-running.pgid, 0) == 0 { libc::kill(-running.pgid, libc::SIGKILL); } }
                return (start.elapsed().as_millis(), false);
            }
        }
        #[cfg(unix)] unsafe { libc::kill(-running.pgid, libc::SIGTERM); }
        if timeout(SIGTERM_GRACE, running.child.wait()).await.is_ok() {
            running.reaped = true;
            #[cfg(unix)] unsafe { if libc::kill(-running.pgid, 0) == 0 { libc::kill(-running.pgid, libc::SIGKILL); } }
            return (start.elapsed().as_millis(), false);
        }
        #[cfg(unix)] unsafe { libc::kill(-running.pgid, libc::SIGKILL); }
        let _ = running.child.kill().await;
        let _ = running.child.wait().await;
        running.reaped = true;
        (start.elapsed().as_millis(), true)
    }
    fn emit(&self, ev: GuardianEvent) { let _ = self.events.send(ev); }
}
fn prune_old(v: &mut Vec<Instant>) {
    prune_older_than(v, Duration::from_secs(3600));
}
/// Retalla els instants amb més de `max_age` d'edat.
///
/// S'expressa amb `t.elapsed()` i NO amb `Instant::now() - max_age`: la
/// substracció PANICA quan el rellotge monotònic porta menys temps encès que
/// `max_age` (Mac acabat d'arrencar), i amb `panic = "abort"` al release
/// aquell pànic mata tota l'app just quan es volia reiniciar el fill.
fn prune_older_than(v: &mut Vec<Instant>, max_age: Duration) {
    v.retain(|&t| t.elapsed() < max_age);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildCommand { Running, Compact, Stop }

pub struct ChildRuntime { tx: watch::Sender<ChildCommand> }
impl ChildRuntime {
    pub fn new() -> Self { let (tx, _) = watch::channel(ChildCommand::Running); Self { tx } }
    pub async fn run<F>(self, agent: F) where F: std::future::Future<Output = ()> + Send + 'static {
        let tx = self.tx.clone();
        let mut stop_rx = tx.subscribe();
        tokio::spawn(async move {
            if stop_rx.wait_for(|c| *c == ChildCommand::Stop).await.is_ok() {
                tokio::time::sleep(Duration::from_millis(800)).await;
                std::process::exit(0);
            }
        });
        let tx_reader = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(tokio::io::stdin()).lines();
            let mut out = tokio::io::stdout();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        match line.trim() {
                            "PING" => { let _ = out.write_all(b"PONG\n").await; }
                            "COMPACT" => { let _ = tx_reader.send(ChildCommand::Compact); let _ = out.write_all(b"OK\n").await; }
                            "STOP" => { let _ = tx_reader.send(ChildCommand::Stop); let _ = out.write_all(b"STOPPED\n").await; let _ = out.flush().await; }
                            _ => {}
                        }
                        let _ = out.flush().await;
                    }
                    Ok(None) | Err(_) => { std::process::exit(0); }
                }
            }
        });
        agent.await;
        std::process::exit(0);
    }
    pub fn subscribe(&self) -> watch::Receiver<ChildCommand> { self.tx.subscribe() }
}
impl Default for ChildRuntime { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod basic_tests {
    use super::*;

    #[test]
    fn config_default_valida() {
        let c = GuardianConfig::default();
        assert!(c.validate().is_ok());
    }

    #[test]
    fn config_for_ram_8gb() {
        let c = GuardianConfig::for_ram(8 * 1024 * 1024 * 1024);
        assert!(c.validate().is_ok());
        assert!(c.agent_rlimit_bytes <= 4 * 1024 * 1024 * 1024);
    }

    #[test]
    fn config_for_ram_16gb() {
        let c = GuardianConfig::for_ram(16 * 1024 * 1024 * 1024);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn config_for_ram_32gb() {
        let c = GuardianConfig::for_ram(32 * 1024 * 1024 * 1024);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn config_for_7gb() {
        let c = GuardianConfig::for_7gb();
        assert!(c.validate().is_ok());
    }

    #[test]
    fn config_swap_soft_major_hard_falla() {
        let mut c = GuardianConfig::default();
        c.swap_soft_bytes = c.swap_hard_bytes;
        assert!(c.validate().is_err());
    }

    #[test]
    fn config_rss_soft_major_hard_falla() {
        let mut c = GuardianConfig::default();
        c.agent_rss_soft_bytes = c.agent_rss_hard_bytes;
        assert!(c.validate().is_err());
    }

    #[test]
    fn config_rss_hard_major_rlimit_falla() {
        let mut c = GuardianConfig::default();
        c.agent_rss_hard_bytes = c.agent_rlimit_bytes;
        assert!(c.validate().is_err());
    }

    #[test]
    fn config_poll_zero_falla() {
        let mut c = GuardianConfig::default();
        c.poll_slow = Duration::from_secs(0);
        assert!(c.validate().is_err());
    }

    #[test]
    fn config_nice_fora_rang_falla() {
        let mut c = GuardianConfig::default();
        c.nice = 20;
        assert!(c.validate().is_err());
    }

    #[test]
    fn config_nice_dins_rang() {
        let mut c = GuardianConfig::default();
        c.nice = 19;
        assert!(c.validate().is_ok());
    }

    #[test]
    fn with_blender_puja_limits() {
        let c = GuardianConfig::default().with_blender();
        assert!(c.agent_rlimit_bytes >= 16 * 1024 * 1024 * 1024);
        assert!(c.agent_rss_hard_bytes >= 12 * 1024 * 1024 * 1024);
    }

    /// Un `rlimit` de 0 bytes significaria «el fill no pot reservar ni un
    /// byte»: el procés mor en néixer i el guardian entra en bucle de
    /// reinici. S'ha de rebutjar a la configuració, no en runtime.
    #[test]
    fn config_rlimit_zero_falla() {
        let mut c = GuardianConfig::default();
        c.agent_rlimit_bytes = 0;
        assert!(c.validate().is_err());
    }

    /// REGRESSIÓ: `Instant::now() - Duration::from_secs(3600)` panica quan el
    /// rellotge monotònic porta menys d'una hora (Mac acabat d'arrencar), i
    /// amb `panic = "abort"` mata l'app just a l'hora de reiniciar el fill.
    #[test]
    fn prune_old_no_panica_si_rellotge_nou() {
        let mut v = vec![Instant::now(), Instant::now()];
        prune_old(&mut v);
        assert_eq!(v.len(), 2, "un instant d'ara mateix és dins l'última hora");
    }

    #[test]
    fn prune_old_els_vells_els_tira() {
        // Sense restar `Instant` (no se sap quant fa que la màquina és encesa):
        // creem un instant, deixem passar el temps i el considerem vell.
        let mut v = vec![Instant::now()];
        std::thread::sleep(Duration::from_millis(30));
        prune_older_than(&mut v, Duration::from_millis(10));
        assert!(v.is_empty(), "un reinici fa 30 ms amb edat màxima 10 ms ha de caure");
    }

    #[test]
    fn prune_old_els_recents_es_queden() {
        let mut v = vec![Instant::now(), Instant::now()];
        prune_older_than(&mut v, Duration::from_secs(3600));
        assert_eq!(v.len(), 2, "dos reinicius d'ara mateix són recents");
    }
}
