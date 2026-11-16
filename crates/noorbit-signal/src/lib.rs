use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub struct SignalHandler { cancel: CancellationToken, task: Option<tokio::task::JoinHandle<()>> }
impl SignalHandler {
    pub fn spawn() -> Self {
        let cancel = CancellationToken::new();
        let cancel_bg = cancel.clone();
        let task = tokio::spawn(async move {
            #[cfg(unix)] {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sigterm = match signal(SignalKind::terminate()) { Ok(s) => s, Err(e) => { warn!("no s'ha pogut registrar SIGTERM: {e}"); return; } };
                let mut sigint = match signal(SignalKind::interrupt()) { Ok(s) => s, Err(e) => { warn!("no s'ha pogut registrar SIGINT: {e}"); return; } };
                tokio::select! { _ = sigterm.recv() => { info!("SIGTERM rebut"); } _ = sigint.recv() => { info!("SIGINT rebut"); } }
            }
            #[cfg(not(unix))] { if tokio::signal::ctrl_c().await.is_ok() { info!("Ctrl+C rebut"); } }
            cancel_bg.cancel();
        });
        Self { cancel, task: Some(task) }
    }
    pub fn token(&self) -> CancellationToken { self.cancel.clone() }
    pub fn cancel(&self) { self.cancel.cancel(); }
    pub async fn join(mut self) { if let Some(t) = self.task.take() { let _ = t.await; } }
}
impl Drop for SignalHandler {
    fn drop(&mut self) { if let Some(t) = self.task.take() { t.abort(); } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn token_cancellable() {
        let h = SignalHandler::spawn();
        let t = h.token();
        assert!(!t.is_cancelled());
        h.cancel();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(t.is_cancelled());
    }

    #[tokio::test]
    async fn drop_no_peta() {
        let h = SignalHandler::spawn();
        drop(h);
        // No ha de panic.
    }
}
