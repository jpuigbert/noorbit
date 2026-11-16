use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub struct MemoryGuardHandle { pub cancel: CancellationToken, pub registry: Arc<Mutex<Vec<u32>>>, pub peak: Arc<AtomicU64>, task: tokio::task::JoinHandle<()> }
impl MemoryGuardHandle {
    pub fn stop(&self) { self.cancel.cancel(); }
    pub async fn join(self) { let _ = self.task.await; }
    pub fn peak_bytes(&self) -> u64 { self.peak.load(Ordering::Relaxed) }
}
pub struct MemoryGuard;
impl MemoryGuard {
    pub fn spawn(budget_bytes: u64) -> MemoryGuardHandle {
        let cancel = CancellationToken::new();
        let registry = Arc::new(Mutex::new(Vec::new()));
        let peak = Arc::new(AtomicU64::new(0));
        let cancel_bg = cancel.clone();
        let registry_bg = registry.clone();
        let peak_bg = peak.clone();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_bg.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {
                        let total = measure_total_rss(&registry_bg).await;
                        peak_bg.fetch_max(total, Ordering::Relaxed);
                        if total > budget_bytes {
                            warn!(total_mb = total / 1024 / 1024, budget_mb = budget_bytes / 1024 / 1024, "memory budget excedit");
                            cancel_bg.cancel();
                            return;
                        }
                    }
                }
            }
        });
        MemoryGuardHandle { cancel, registry, peak, task }
    }
}
async fn measure_total_rss(registry: &Arc<Mutex<Vec<u32>>>) -> u64 {
    let self_rss = rss(std::process::id());
    let pids = registry.lock().await.clone();
    let mut children_rss = 0u64;
    for pid in pids { children_rss += rss(pid); }
    self_rss + children_rss
}
#[cfg(target_os = "macos")]
fn rss(pid: u32) -> u64 {
    if pid == 0 { return 0; }
    use std::mem;
    use std::os::raw::{c_int, c_void};
    #[repr(C)] struct ProcTaskInfo { pti_virtual_size: u64, pti_resident_size: u64, pti_total_user: u64, pti_total_system: u64, pti_threads_user: u64, pti_threads_system: u64, pti_policy: i32, pti_faults: i32, pti_pageins: i32, pti_cow_faults: i32, pti_messages_sent: i32, pti_messages_received: i32, pti_syscalls_mach: i32, pti_syscalls_unix: i32, pti_csw: i32, pti_threadnum: i32, pti_numrunning: i32, pti_priority: i32 }
    extern "C" { fn proc_pidinfo(pid: c_int, flavor: c_int, arg: u64, buffer: *mut c_void, buffersize: c_int) -> c_int; }
    const PROC_PIDTASKINFO: c_int = 4;
    let mut info: ProcTaskInfo = unsafe { mem::zeroed() };
    let size = mem::size_of::<ProcTaskInfo>() as c_int;
    let n = unsafe { proc_pidinfo(pid as c_int, PROC_PIDTASKINFO, 0, &mut info as *mut _ as *mut c_void, size) };
    if n != size { return 0; }
    info.pti_resident_size
}
#[cfg(not(target_os = "macos"))]
fn rss(pid: u32) -> u64 {
    if pid == 0 { return 0; }
    let s = std::fs::read_to_string(format!("/proc/{pid}/statm")).unwrap_or_default();
    let r: u64 = s.split_whitespace().nth(1).and_then(|x| x.parse().ok()).unwrap_or(0);
    r * 4096
}
pub use MemoryGuard as Guard;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_del_procés_actual_no_es_zero() {
        // El procés de test té RSS > 0.
        let r = rss(std::process::id());
        assert!(r > 0, "RSS del procés actual ha de ser > 0, got {r}");
    }

    #[test]
    fn rss_de_zero_es_zero() {
        assert_eq!(rss(0), 0);
    }

    #[tokio::test]
    async fn memory_guard_no_cancela_quan_hi_ha_memòria() {
        // Amb un pressupost de 10 GB (molt alt), el guard no cancel·la.
        let guard = MemoryGuard::spawn(10 * 1024 * 1024 * 1024);
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        assert!(!guard.cancel.is_cancelled(), "no hauria de cancel·lar");
        guard.stop();
    }

    #[tokio::test]
    async fn memory_guard_exposa_peak() {
        let guard = MemoryGuard::spawn(10 * 1024 * 1024 * 1024);
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        assert!(guard.peak_bytes() > 0, "peak ha de ser > 0");
        guard.stop();
    }
}
