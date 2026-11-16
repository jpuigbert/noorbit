//! Test d'integració de l'Autonomous Jam: un LLM fals escriu un fitxer,
//! el worker el materialitza, el verifier valida i el jam fa commit.

#![cfg(test)]

use std::sync::Arc;

use noorbit_jam::{
    Cache, Checkpointer, JamRuntime, Plan, Reporter, Task, TaskContext, Verifier, Worker,
};
use noorbit_planner::LlmClient;

use crate::jam::{NoOrbitVerifier, NoOrbitWorker};

// ─────────────────────────────────────────────────────────────────────────────
//  Dobles de prova
// ─────────────────────────────────────────────────────────────────────────────

/// LLM que sempre respon el mateix fitxer.
struct FakeLlm {
    response: String,
    calls: std::sync::atomic::AtomicUsize,
}
impl FakeLlm {
    fn new(response: &str) -> Self {
        Self {
            response: response.to_string(),
            calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}
#[async_trait::async_trait]
impl LlmClient for FakeLlm {
    async fn complete(&self, _prompt: &str) -> Result<String, String> {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self.response.clone())
    }
}

/// Cache buit.
struct NoopCache;
#[async_trait::async_trait]
impl Cache for NoopCache {
    async fn clear(&self) {}
}

/// Reporter que mem el pla i les stats.
#[derive(Default)]
struct MemReporter {
    plan: std::sync::Mutex<Option<Plan>>,
    stats: std::sync::Mutex<Option<noorbit_jam::JamStats>>,
}
#[async_trait::async_trait]
impl Reporter for MemReporter {
    async fn report(
        &self,
        plan: &Plan,
        stats: &noorbit_jam::JamStats,
    ) -> Result<(), String> {
        *self.plan.lock().unwrap() = Some(plan.clone());
        *self.stats.lock().unwrap() = Some(stats.clone());
        Ok(())
    }
}

/// Checkpointer mem (no fa git): només compta commits.
#[derive(Default)]
struct MemCheckpointer {
    head: std::sync::Mutex<String>,
    commits: std::sync::atomic::AtomicUsize,
    cleaned: std::sync::atomic::AtomicBool,
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
        let sha = format!("sha{}", self.commits.load(std::sync::atomic::Ordering::SeqCst));
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

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────

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
        duration: std::time::Duration::from_secs(300),
        budget: noorbit_jam::TimeBudget::from_total(std::time::Duration::from_secs(300)),
    }
}

#[tokio::test]
async fn el_worker_materialitza_el_fitxer_anunciat() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().to_path_buf();

    let llm = Arc::new(FakeLlm::new(
        "@file: src/joc.gd\nextends Node2D\n\nfunc _ready():\n\tpass\n",
    ));
    let worker = NoOrbitWorker::new(llm as Arc<dyn LlmClient>, root.clone());

    let ctx = TaskContext {
        cancel: tokio_util::sync::CancellationToken::new(),
        child_registry: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        quality: noorbit_jam::QualityLevel::Full,
        attempt: 1,
    };

    let diff = worker.run(&task("t01"), &ctx).await.unwrap();
    assert!(diff.materialized, "el diff hauria de ser materialitzat");
    assert_eq!(
        std::fs::read_to_string(root.join("src/joc.gd")).unwrap(),
        "extends Node2D\n\nfunc _ready():\n\tpass\n"
    );
}

#[tokio::test]
async fn el_worker_rebutja_resposta_sense_fitxers() {
    let dir = tempfile::TempDir::new().unwrap();
    let llm = Arc::new(FakeLlm::new("no tinc res a escriure"));
    let worker = NoOrbitWorker::new(llm as Arc<dyn LlmClient>, dir.path().to_path_buf());
    let ctx = TaskContext {
        cancel: tokio_util::sync::CancellationToken::new(),
        child_registry: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        quality: noorbit_jam::QualityLevel::Full,
        attempt: 1,
    };
    assert!(worker.run(&task("t01"), &ctx).await.is_err());
}

#[tokio::test]
async fn el_worker_ignora_paths_perillosos() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    let llm = Arc::new(FakeLlm::new(
        "@file: ../escapat.txt\nhola\n\n@file: /etc/passwd\nno\n",
    ));
    let worker = NoOrbitWorker::new(llm as Arc<dyn LlmClient>, root.clone());
    let ctx = TaskContext {
        cancel: tokio_util::sync::CancellationToken::new(),
        child_registry: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        quality: noorbit_jam::QualityLevel::Full,
        attempt: 1,
    };
    // Els dos paths són perillosos: no ha de materialitzar cap.
    let r = worker.run(&task("t01"), &ctx).await;
    assert!(r.is_err() || r.unwrap().file_count() == 0);
    assert!(!dir.path().parent().unwrap().join("escapat.txt").exists());
}

#[tokio::test]
async fn el_verifier_valida_un_projecte_amb_fitxers() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("main.gd"), "extends Node\n").unwrap();
    let v = NoOrbitVerifier::new(dir.path().to_path_buf(), None);
    let ctx = TaskContext {
        cancel: tokio_util::sync::CancellationToken::new(),
        child_registry: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        quality: noorbit_jam::QualityLevel::Full,
        attempt: 1,
    };
    assert!(v.run(&task("t01"), &ctx).await.is_ok());
}

#[tokio::test]
async fn el_verifier_falla_si_el_build_falla() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("main.gd"), "extends Node\n").unwrap();
    let v = NoOrbitVerifier::new(dir.path().to_path_buf(), Some("exit 3".into()));
    let ctx = TaskContext {
        cancel: tokio_util::sync::CancellationToken::new(),
        child_registry: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        quality: noorbit_jam::QualityLevel::Full,
        attempt: 1,
    };
    assert!(v.run(&task("t01"), &ctx).await.is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
//  Bucle complet
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn el_bucle_complet_completa_les_tasques_i_fa_commit() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().to_path_buf();

    let llm = Arc::new(FakeLlm::new(
        "@file: src/joc.gd\nextends Node2D\n",
    ));
    let worker_inner = Arc::new(NoOrbitWorker::new(
        llm as Arc<dyn LlmClient>,
        root.clone(),
    ));
    let worker = worker_inner.clone() as Arc<dyn Worker>;
    let verifier_inner = Arc::new(NoOrbitVerifier::new(root.clone(), None));
    let verifier = verifier_inner.clone() as Arc<dyn Verifier>;
    let ckpt_inner = Arc::new(MemCheckpointer {
        head: std::sync::Mutex::new("base".into()),
        ..Default::default()
    });
    let ckpt = ckpt_inner.clone() as Arc<dyn Checkpointer>;
    let reporter_inner = Arc::new(MemReporter::default());
    let reporter = reporter_inner.clone() as Arc<dyn Reporter>;
    let cache = Arc::new(NoopCache) as Arc<dyn Cache>;

    let deps = noorbit_jam::JamDeps {
        worker,
        verifier,
        ckpt: ckpt.clone(),
        reporter: reporter.clone(),
        cache,
        repo_path: root.clone(),
        progress: None,
        save_fn: None,
    };

    let p = plan(vec![task("t01"), task("t02")]);
    let rt = JamRuntime::standalone();
    let cfg = noorbit_jam::JamConfig::default();

    noorbit_jam::run_autonomous_loop(rt, p, deps, cfg).await;

    // Stats: les dues tasques completades.
    let stats = reporter_inner.stats.lock().unwrap().clone().unwrap();
    assert_eq!(stats.completed, 2, "haurien de completar 2 tasques");
    assert_eq!(stats.blocked, 0);

    // Commits: un per tasca.
    assert_eq!(
        ckpt_inner.commits.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "hauria d'haver fet 2 commits"
    );

    // Cleanup: el jam el crida SEMPRE al final.
    assert!(ckpt_inner.cleaned.load(std::sync::atomic::Ordering::SeqCst));

    // Fitxer materialitzat.
    assert!(root.join("src/joc.gd").exists());
}

// ─────────────────────────────────────────────────────────────────────────────
//  Progrés
// ─────────────────────────────────────────────────────────────────────────────

/// Receptor que mem els esdeveniments.
#[derive(Default)]
struct MemProgress {
    events: std::sync::Mutex<Vec<noorbit_jam::Progress>>,
}
#[async_trait::async_trait]
impl noorbit_jam::ProgressSink for MemProgress {
    async fn on_progress(&self, ev: noorbit_jam::Progress) {
        self.events.lock().unwrap().push(ev);
    }
}

#[tokio::test]
async fn el_bucle_emet_progres_per_a_cada_tasca() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().to_path_buf();

    let llm = Arc::new(FakeLlm::new("@file: a.txt\nhola\n"));
    let worker = Arc::new(NoOrbitWorker::new(
        llm as Arc<dyn LlmClient>,
        root.clone(),
    )) as Arc<dyn Worker>;
    let verifier = Arc::new(NoOrbitVerifier::new(root.clone(), None)) as Arc<dyn Verifier>;
    let ckpt = Arc::new(MemCheckpointer {
        head: std::sync::Mutex::new("base".into()),
        ..Default::default()
    }) as Arc<dyn Checkpointer>;
    let reporter = Arc::new(MemReporter::default()) as Arc<dyn Reporter>;
    let cache = Arc::new(NoopCache) as Arc<dyn Cache>;
    let progress = Arc::new(MemProgress::default());

    let deps = noorbit_jam::JamDeps {
        worker,
        verifier,
        ckpt,
        reporter,
        cache,
        repo_path: root.clone(),
        progress: Some(progress.clone() as Arc<dyn noorbit_jam::ProgressSink>),
        save_fn: None,
    };

    let p = plan(vec![task("t01")]);
    noorbit_jam::run_autonomous_loop(JamRuntime::standalone(), p, deps, noorbit_jam::JamConfig::default()).await;

    let evs = progress.events.lock().unwrap().clone();
    assert!(
        evs.iter()
            .any(|e| matches!(e, noorbit_jam::Progress::TaskStart { id, .. } if id == "t01")),
        "hauria d'haver-hi un TaskStart: {:?}",
        evs
    );
    assert!(
        evs.iter()
            .any(|e| matches!(e, noorbit_jam::Progress::TaskDone { id, .. } if id == "t01")),
        "hauria d'haver-hi un TaskDone: {:?}",
        evs
    );
    assert!(
        evs.iter()
            .any(|e| matches!(e, noorbit_jam::Progress::Finished)),
        "hauria d'haver-hi un Finished: {:?}",
        evs
    );
}
