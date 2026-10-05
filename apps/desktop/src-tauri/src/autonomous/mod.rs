//! Feines autònomes acotades de NoOrbit.
//!
//! Quan el model local no arriba per resoldre un problema, el mode profund
//! pot «forjar» un especialista autònom: una feina en segon pla que avança
//! pas a pas cap a l'objectiu combinant el model local (lligat a la capacitat
//! real de la màquina), la cerca a internet i les altres IAs en línia.
//!
//! Decisions de disseny (honestes amb el «sobreviure sola»):
//! - L'estat viu a `jobs.json` (dins la carpeta de dades, també en mode
//!   portàtil): les feines i el seu diari SOBREVIUEN reinicis de l'app.
//! - L'aplicació, a l'arrencar, reempren automàticament les feines actives:
//!   «arrenca sola».
//! - Cada feina té un PRESSUPOST dur (passos màxims, pauses llargues en
//!   errors) per no superar mai la capacitat de l'ordinador. Es pot pausar i
//!   tornar a activar en qualsevol moment.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

pub mod commands;

/// Passos màxims de raonament per feina (pressupost dur).
pub const MAX_STEPS: usize = 12;
/// Pausa entre passos (ms).
pub const STEP_INTERVAL_MS: u64 = 30_000;
/// Pausa després d'un pas que ha fallat (ms): la feina «descansa» i ho prova
/// de nou més tard, en lloc de cremar la màquina.
pub const ERROR_BACKOFF_MS: u64 = 5 * 60_000;
/// Si un error repeteix 3 vegades seguides, la feina passa a dorment:
/// sobreviu a disc però noconsumeix res fins que l'usuari la reactivi.
pub const MAX_CONSECUTIVE_ERRORS: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalNote {
    pub step: usize,
    pub at: i64,
    /// Cert si aquest pas va acabar en error.
    pub ok: bool,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    /// Objectiu original, sense retocar.
    pub objective: String,
    /// Fila de treball: objectiu + últims aprenentatges.
    pub focus: String,
    pub expert_id: String,
    pub expert_name: String,
    pub steps: usize,
    pub errors: usize,
    pub consecutive_errors: usize,
    /// Activa = la màquina la segueix en segon pla. Dorment = sobreviú a disc
    /// però no executa (l'usuari la pot reactivar).
    pub active: bool,
    pub solved: bool,
    pub journal: Vec<JournalNote>,
    pub created_at: i64,
}

#[derive(Default, Serialize, Deserialize)]
struct FileData {
    #[serde(default)]
    jobs: Vec<Job>,
}

pub struct AutonomousStore {
    path: PathBuf,
    jobs: Mutex<Vec<Job>>,
}

impl AutonomousStore {
    pub fn new(data_dir: PathBuf) -> Self {
        let path = data_dir.join("jobs.json");
        let jobs = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<FileData>(&raw).ok())
            .map(|f| f.jobs)
            .unwrap_or_default();
        Self {
            path,
            jobs: Mutex::new(jobs),
        }
    }

    fn save(&self, list: &[Job]) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string_pretty(&FileData {
            jobs: list.to_vec(),
        }) {
            let _ = fs::write(&self.path, raw);
        }
    }

    pub fn list(&self) -> Vec<Job> {
        self.jobs.lock().unwrap().clone()
    }

    /// Feines que caldria reprendre en arrencar l'aplicació.
    pub fn resumable(&self) -> Vec<Job> {
        self.jobs
            .lock()
            .unwrap()
            .iter()
            .filter(|j| j.active && !j.solved && j.steps < MAX_STEPS)
            .cloned()
            .collect()
    }

    pub fn add(&self, job: Job) {
        let mut list = self.jobs.lock().unwrap();
        list.retain(|j| j.id != job.id);
        list.push(job.clone());
        self.save(&list);
    }

    /// Insereix una feina nova conservant l'ordre cronològic.
    pub fn insert(&self, job: &Job) {
        self.add(job.clone());
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        self.jobs.lock().unwrap().iter().find(|j| j.id == id).cloned()
    }

    /// Aplica una mutació a una feina i la desa. Retorna l'últim estat.
    pub fn mutate<F>(&self, id: &str, f: F) -> Option<Job>
    where
        F: FnOnce(&mut Job),
    {
        let mut list = self.jobs.lock().unwrap();
        let job = list.iter_mut().find(|j| j.id == id)?;
        f(job);
        let snapshot = job.clone();
        self.save(&list);
        Some(snapshot)
    }

    pub fn remove(&self, id: &str) {
        let mut list = self.jobs.lock().unwrap();
        list.retain(|j| j.id != id);
        self.save(&list);
    }

    pub fn set_active(&self, id: &str, active: bool) -> Option<Job> {
        self.mutate(id, |j| {
            j.active = active;
            // Reactivar una feina esgotada per pressupost li donava una nova
            // oportunitat: tornem el comptador d'errors a zero.
            if active {
                j.consecutive_errors = 0;
            }
        })
    }
}
