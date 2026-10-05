//! Especialistes: agents d'IA personalitzats que l'usuari crea.
//!
//! Cada especialista té un rol, un prompt de sistema i (opcionalment) un model
//! propi. Es poden usar per treballar sempre amb un agent concret o en equip
//! (treball múltiple en paral·lel sobre un mateix objectiu).

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Expert {
    pub id: String,
    pub name: String,
    /// Rol curt (p. ex. «Desenvolupador front-end»).
    pub role: String,
    /// Instruccions de sistema que defineixen com treballa.
    pub system_prompt: String,
    /// Model d'Ollama assignat; si és None, usa el model actiu.
    #[serde(default)]
    pub model: Option<String>,
    /// Cert si va ser «forjat» pel mode autònom: treballa sempre amb recursos
    /// externs (internet + altres IAs) i pot tenir feines en segon pla.
    #[serde(default)]
    pub autonomous: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpertInput {
    pub name: String,
    pub role: String,
    pub system_prompt: String,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct FileData {
    #[serde(default)]
    experts: Vec<Expert>,
}

pub struct ExpertManager {
    path: PathBuf,
    experts: Mutex<Vec<Expert>>,
}

impl ExpertManager {
    pub fn new(data_dir: PathBuf) -> Self {
        let path = data_dir.join("experts.json");
        let experts = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<FileData>(&raw).ok())
            .map(|f| f.experts)
            .unwrap_or_default();
        Self {
            path,
            experts: Mutex::new(experts),
        }
    }

    fn save(&self, list: &[Expert]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let raw = serde_json::to_string_pretty(&FileData {
            experts: list.to_vec(),
        })
        .map_err(|e| anyhow!("No s'ha pogut serialitzar: {}", e))?;
        fs::write(&self.path, raw).map_err(|e| anyhow!("No s'ha pogut desar: {}", e))?;
        Ok(())
    }

    pub fn list(&self) -> Vec<Expert> {
        self.experts.lock().unwrap().clone()
    }

    pub fn get(&self, id: &str) -> Option<Expert> {
        self.experts.lock().unwrap().iter().find(|e| e.id == id).cloned()
    }

    pub fn add(&self, input: ExpertInput) -> Result<Expert> {
        if input.name.trim().is_empty() {
            return Err(anyhow!("Cal donar un nom a l'especialista"));
        }
        let expert = Expert {
            id: format!("exp-{}", chrono::Utc::now().timestamp_millis()),
            name: input.name.trim().to_string(),
            role: input.role.trim().to_string(),
            system_prompt: input.system_prompt.clone(),
            model: input.model.filter(|m| !m.trim().is_empty()),
            autonomous: false,
            created_at: chrono::Utc::now().timestamp_millis(),
        };
        let mut list = self.experts.lock().unwrap();
        list.push(expert.clone());
        self.save(&list)?;
        Ok(expert)
    }

    pub fn update(&self, id: &str, input: ExpertInput) -> Result<Expert> {
        let mut list = self.experts.lock().unwrap();
        let e = list
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow!("L'especialista no existeix"))?;
        e.name = input.name.trim().to_string();
        e.role = input.role.trim().to_string();
        e.system_prompt = input.system_prompt.clone();
        e.model = input.model.filter(|m| !m.trim().is_empty());
        let updated = e.clone();
        self.save(&list)?;
        Ok(updated)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let mut list = self.experts.lock().unwrap();
        list.retain(|e| e.id != id);
        self.save(&list)
    }
}
