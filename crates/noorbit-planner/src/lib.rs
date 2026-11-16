use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use serde::Deserialize;
use thiserror::Error;
use tracing::{info, warn};
use noorbit_jam::{Plan, Task, TimeBudget};

const MAX_TASKS: usize = 100;
const MAX_TASK_ID_LEN: usize = 64;
const MAX_TITLE_LEN: usize = 200;
const MAX_DESC_LEN: usize = 2000;
const MAX_ACCEPTANCE_ITEMS: usize = 10;
const MAX_ACCEPTANCE_LEN: usize = 200;
/// Durada màxima del pla: 7 dies. Ha de coincidir amb `MAX_JAM_DURATION_SECS`
/// de l'app (jam/mod.rs) perquè les feines multi-dia no es rebutgen ací.
const MAX_PLAN_DURATION: Duration = Duration::from_secs(7 * 24 * 3600);
const LLM_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RETRIES: u32 = 2;

#[derive(Debug, Error)]
pub enum PlannerError {
    #[error("brief buit")] EmptyBrief,
    #[error("engine desconegut: {0}")] UnknownEngine(String),
    #[error("durada invàlida: {0:?}")] InvalidDuration(Duration),
    #[error("llm: {0}")] Llm(String),
    #[error("json invàlid: {0}")] Json(String),
    #[error("validació: {0}")] Validation(String),
    #[error("esgotats {0} intents")] RetriesExhausted(u32),
}
pub type Result<T> = std::result::Result<T, PlannerError>;

#[derive(Debug, Clone)]
pub struct Brief {
    pub text: String,
    pub engine: Engine,
    pub duration: Duration,
    pub budget: Option<TimeBudget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine { Unreal5, Godot4, Unity, Custom, Blender }
impl Engine {
    fn as_str(self) -> &'static str {
        match self { Engine::Unreal5 => "unreal5", Engine::Godot4 => "godot4", Engine::Unity => "unity", Engine::Custom => "custom", Engine::Blender => "blender" }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "unreal5" | "unreal" | "ue5" => Ok(Engine::Unreal5),
            "godot4" | "godot" => Ok(Engine::Godot4),
            "unity" => Ok(Engine::Unity),
            "custom" => Ok(Engine::Custom),
            "blender" => Ok(Engine::Blender),
            other => Err(PlannerError::UnknownEngine(other.into())),
        }
    }
}

#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, prompt: &str) -> std::result::Result<String, String>;
    async fn complete_with_usage(&self, prompt: &str) -> std::result::Result<(String, u64), String> {
        let s = self.complete(prompt).await?;
        Ok((s, 0))
    }
}

#[derive(Debug, Deserialize)]
struct RawPlan { title: String, tasks: Vec<RawTask> }
#[derive(Debug, Deserialize)]
struct RawTask { id: String, title: String, description: String, #[serde(default)] acceptance: Vec<String> }

pub struct Planner { llm: Arc<dyn LlmClient> }
impl Planner { pub fn new(llm: Arc<dyn LlmClient>) -> Self { Self { llm } } }

impl Planner {
    pub async fn plan(&self, brief: &Brief) -> Result<Plan> {
        if brief.text.trim().is_empty() { return Err(PlannerError::EmptyBrief); }
        if brief.duration.is_zero() || brief.duration > MAX_PLAN_DURATION { return Err(PlannerError::InvalidDuration(brief.duration)); }
        let mut last_err: Option<String> = None;
        for attempt in 0..=MAX_RETRIES {
            let prompt = build_prompt(brief, last_err.as_deref());
            let raw = match tokio::time::timeout(LLM_TIMEOUT, self.llm.complete(&prompt)).await {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => { warn!(attempt, "llm error: {e}"); last_err = Some(format!("la crida anterior va fallar: {e}")); continue; }
                Err(_) => { warn!(attempt, "llm timeout"); last_err = Some("la crida anterior va fer timeout".into()); continue; }
            };
            let json = strip_markdown_json(&raw);
            let parsed: RawPlan = match serde_json::from_str(&json) {
                Ok(p) => p,
                Err(e) => { warn!(attempt, "json invàlid: {e}"); last_err = Some(format!("JSON invàlid: {e}")); continue; }
            };
            match validate(parsed, brief.duration, brief.budget.clone()) {
                Ok(plan) => { info!(attempt, tasks = plan.tasks.len(), "pla generat"); return Ok(plan); }
                Err(e) => { warn!(attempt, "validació: {e}"); last_err = Some(format!("validació fallida: {e}")); continue; }
            }
        }
        Err(PlannerError::RetriesExhausted(MAX_RETRIES + 1))
    }
}

fn build_prompt(brief: &Brief, feedback: Option<&str>) -> String {
    let engine = brief.engine.as_str();
    let dur_min = brief.duration.as_secs() / 60;
    let mut p = String::new();
    p.push_str("Ets un planificador expert en desenvolupament de videojocs.\n");
    p.push_str("Retorna NOMÉS un objecte JSON vàlid, sense text addicional.\n\n");
    p.push_str("REGLES: tasques petites (≤300 línies), acceptance 1..=10, IDs únics sense espais, màx 100 tasques.\n");
    p.push_str("FORMAT: {\"title\":...,\"tasks\":[{\"id\":...,\"title\":...,\"description\":...,\"acceptance\":[...]},...]}\n\n");
    p.push_str(&format!("ENGINE: {engine}\nDURADA: {dur_min} min\n\n"));
    p.push_str("BRIEF (user input):\n-----\n");
    p.push_str(brief.text.trim());
    p.push_str("\n-----\n");
    if let Some(fb) = feedback { p.push_str(&format!("\nINTENT ANTERIOR FALLÀ: {fb}\n")); }
    p
}

fn strip_markdown_json(s: &str) -> String {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("```json") { if let Some(end) = rest.rfind("```") { return rest[..end].trim().to_string(); } }
    if let Some(rest) = s.strip_prefix("```") { if let Some(end) = rest.rfind("```") { return rest[..end].trim().to_string(); } }
    s.to_string()
}

fn validate(raw: RawPlan, duration: Duration, budget: Option<TimeBudget>) -> std::result::Result<Plan, String> {
    let title = raw.title.trim().to_string();
    if title.is_empty() { return Err("plan.title buit".into()); }
    if title.chars().count() > MAX_TITLE_LEN { return Err("plan.title massa llarg".into()); }
    if raw.tasks.is_empty() { return Err("pla sense tasques".into()); }
    if raw.tasks.len() > MAX_TASKS { return Err("massa tasques".into()); }
    let mut ids: HashSet<String> = HashSet::with_capacity(raw.tasks.len());
    let mut tasks = Vec::with_capacity(raw.tasks.len());
    for (i, rt) in raw.tasks.into_iter().enumerate() {
        let id = rt.id.trim().to_string();
        if id.is_empty() || id.chars().count() > MAX_TASK_ID_LEN { return Err(format!("task[{i}].id invàlid")); }
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { return Err(format!("task[{i}].id caràcters no permesos")); }
        if !ids.insert(id.clone()) { return Err(format!("task[{i}].id duplicat")); }
        let title = rt.title.trim().to_string();
        if title.is_empty() || title.chars().count() > MAX_TITLE_LEN { return Err(format!("task[{i}].title invàlid")); }
        let description = rt.description.trim().to_string();
        if description.is_empty() || description.chars().count() > MAX_DESC_LEN { return Err(format!("task[{i}].description invàlid")); }
        if rt.acceptance.is_empty() || rt.acceptance.len() > MAX_ACCEPTANCE_ITEMS { return Err(format!("task[{i}].acceptance invàlid")); }
        let mut acceptance = Vec::with_capacity(rt.acceptance.len());
        for (j, a) in rt.acceptance.into_iter().enumerate() {
            let a = a.trim().to_string();
            if a.is_empty() || a.chars().count() > MAX_ACCEPTANCE_LEN { return Err(format!("task[{i}].acceptance[{j}] invàlid")); }
            acceptance.push(a);
        }
        tasks.push(Task { id, title, description, acceptance, phase: None });
    }
    let budget = budget.unwrap_or_else(|| TimeBudget::from_total(duration));
    Ok(Plan { title, tasks, duration, budget })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockLlm {
        replies: Mutex<Vec<std::result::Result<String, String>>>,
    }

    #[async_trait::async_trait]
    impl LlmClient for MockLlm {
        async fn complete(&self, _prompt: &str) -> std::result::Result<String, String> {
            let mut r = self.replies.lock().unwrap();
            if r.is_empty() {
                Err("sense respostes".to_string())
            } else {
                r.remove(0)
            }
        }
    }

    fn brief() -> Brief {
        Brief {
            text: "joc de prova".into(),
            engine: Engine::Custom,
            duration: Duration::from_secs(3600),
            budget: None,
        }
    }

    fn valid_json() -> String {
        r#"{"title":"P","tasks":[{"id":"t01","title":"a","description":"b","acceptance":["c"]}]}"#.into()
    }

    #[tokio::test]
    async fn ok_a_la_primera() {
        let llm = Arc::new(MockLlm {
            replies: Mutex::new(vec![Ok(valid_json())]),
        });
        let p = Planner::new(llm).plan(&brief()).await.unwrap();
        assert_eq!(p.tasks.len(), 1);
    }

    #[tokio::test]
    async fn reintenta_si_json_invalid() {
        let llm = Arc::new(MockLlm {
            replies: Mutex::new(vec![Ok("no és json".into()), Ok(valid_json())]),
        });
        let p = Planner::new(llm).plan(&brief()).await.unwrap();
        assert_eq!(p.tasks.len(), 1);
    }

    #[tokio::test]
    async fn strip_markdown() {
        let llm = Arc::new(MockLlm {
            replies: Mutex::new(vec![Ok(format!("```json\n{}\n```", valid_json()))]),
        });
        let p = Planner::new(llm).plan(&brief()).await.unwrap();
        assert_eq!(p.tasks.len(), 1);
    }

    #[tokio::test]
    async fn rebutja_id_duplicat() {
        let bad = r#"{"title":"P","tasks":[
            {"id":"t1","title":"a","description":"b","acceptance":["c"]},
            {"id":"t1","title":"a","description":"b","acceptance":["c"]}
        ]}"#;
        let llm = Arc::new(MockLlm {
            replies: Mutex::new(vec![Ok(bad.into()), Ok(bad.into()), Ok(bad.into())]),
        });
        assert!(Planner::new(llm).plan(&brief()).await.is_err());
    }

    #[tokio::test]
    async fn rebutja_acceptance_buit() {
        let bad = r#"{"title":"P","tasks":[{"id":"t1","title":"a","description":"b","acceptance":[]}]}"#;
        let llm = Arc::new(MockLlm {
            replies: Mutex::new(vec![Ok(bad.into()), Ok(bad.into()), Ok(bad.into())]),
        });
        assert!(Planner::new(llm).plan(&brief()).await.is_err());
    }

    #[tokio::test]
    async fn rebutja_brief_buit() {
        let llm = Arc::new(MockLlm {
            replies: Mutex::new(vec![]),
        });
        let mut b = brief();
        b.text = "   ".into();
        assert!(matches!(
            Planner::new(llm).plan(&b).await,
            Err(PlannerError::EmptyBrief)
        ));
    }

    #[test]
    fn engine_parse() {
        assert_eq!(Engine::parse("UE5").unwrap(), Engine::Unreal5);
        assert!(Engine::parse("foo").is_err());
    }
}
