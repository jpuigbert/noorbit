//! Tests del supervisor: drenatge de la sortida del fill, reinici i
//! aturada ordenada.

#![cfg(test)]

use std::sync::Arc;
use std::time::Duration;

use noorbit_guardian::{GuardianConfig, GuardianEvent, SpawnSpec, Supervisor};

/// Config de proves: llindars molt alts perquè no dispari cap reinici.
fn test_config() -> GuardianConfig {
    GuardianConfig {
        agent_rlimit_bytes: 2 * 1024 * 1024 * 1024,
        swap_soft_bytes: 1024 * 1024 * 1024 * 1024,
        swap_hard_bytes: 2 * 1024 * 1024 * 1024 * 1024,
        available_soft_bytes: 1024 * 1024 * 1024,
        available_hard_bytes: 512 * 1024 * 1024,
        agent_rss_soft_bytes: 1024 * 1024 * 1024,
        agent_rss_hard_bytes: 1500 * 1024 * 1024,
        poll_slow: Duration::from_millis(200),
        poll_fast: Duration::from_millis(100),
        max_restarts_per_hour: 3,
        nice: 5,
        agent_cpu_limit_secs: 0,
    }
}

#[tokio::test]
async fn el_supervisor_drena_la_sortida_del_fill() {
    // El fill escriu moltes línies: si el supervisor no les llegeix, el pipe
    // s'omple i el fill es queda penjat (el bug que va quedar fixat).
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<GuardianEvent>();

    let factory: Arc<dyn Fn() -> SpawnSpec + Send + Sync> = Arc::new(|| SpawnSpec {
        program: "/bin/sh".into(),
        args: vec![
            "-c".into(),
            "i=0; while [ $i -lt 500 ]; do echo \"JAM|linea $i\"; i=$((i+1)); done; sleep 5"
                .into(),
        ],
        env: vec![],
        cwd: None,
    });

    let (sup, cancel) = Supervisor::new(test_config(), factory, tx).unwrap();
    let task = tokio::spawn(async move { let r = sup.run().await; eprintln!("DEBUG run: {:?}", r); r });

    // Arriben línies del fill.
    let mut lines = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while lines < 100 && tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
            Ok(Some(GuardianEvent::ChildLine(l))) => {
                assert!(l.starts_with("JAM|"), "línia inesperada: {l}");
                lines += 1;
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => break,
        }
    }
    assert!(lines >= 100, "només van arribar {lines} línies (pipe bloquejat?)");

    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
}

#[tokio::test]
async fn el_supervisor_emet_resum_i_aturada() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<GuardianEvent>();

    let factory: Arc<dyn Fn() -> SpawnSpec + Send + Sync> = Arc::new(|| SpawnSpec {
        program: "/bin/sh".into(),
        args: vec!["-c".into(), "exit 0".into()],
        env: vec![],
        cwd: None,
    });

    let (sup, _cancel) = Supervisor::new(test_config(), factory, tx).unwrap();
    let task = tokio::spawn(async move { sup.run().await });

    let mut saw_summary = false;
    let mut saw_stopped = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline && !(saw_summary && saw_stopped) {
        match tokio::time::timeout(Duration::from_secs(3), rx.recv()).await {
            Ok(Some(GuardianEvent::Summary(s))) => {
                saw_summary = true;
                assert!(s.samples >= 1);
                assert!(s.peak_rss_bytes <= s.peak_rss_bytes);
            }
            Ok(Some(GuardianEvent::Stopped)) => saw_stopped = true,
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => break,
        }
    }
    assert!(saw_summary, "no ha arribat el resum");
    assert!(saw_stopped, "no ha arribat l'aturada");
    let _ = task.await;
}

#[tokio::test]
async fn les_ultimes_linies_del_fill_no_es_perden() {
    // El fill imprimeix el seu últim estat i surt de seguida. Si el
    // supervisor fa `abort()` a la tasca de drenatge en veure la sortida,
    // l'última línia ES PERD: l'usuari veuria una tasca menys de les que
    // realment s'han acabat.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<GuardianEvent>();

    let factory: Arc<dyn Fn() -> SpawnSpec + Send + Sync> = Arc::new(|| SpawnSpec {
        program: "/bin/sh".into(),
        args: vec!["-c".into(), "echo 'JAM|primera'; echo 'JAM|ultima'; exit 0".into()],
        env: vec![],
        cwd: None,
    });

    let (sup, _cancel) = Supervisor::new(test_config(), factory, tx).unwrap();
    let task = tokio::spawn(async move { sup.run().await });

    let mut lines: Vec<String> = vec![];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(3), rx.recv()).await {
            Ok(Some(GuardianEvent::ChildLine(l))) => lines.push(l),
            Ok(Some(GuardianEvent::Stopped)) => break,
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }
    assert!(
        lines.iter().any(|l| l == "JAM|ultima"),
        "s'han perdut línies finals: {lines:?}"
    );
    let _ = task.await;
}
