//! Gestor de plugins de NoOrbit (Blender, Unreal, etc.).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

pub struct PluginManager {
    data_dir: PathBuf,
    plugins: Vec<Plugin>,
}

impl PluginManager {
    pub fn new(data_dir: PathBuf) -> Self {
        let mut manager = Self {
            data_dir,
            plugins: default_plugins(),
        };
        manager.load_state();
        manager
    }

    fn state_path(&self) -> PathBuf {
        self.data_dir.join("plugins.json")
    }

    fn load_state(&mut self) {
        if let Ok(raw) = std::fs::read_to_string(self.state_path()) {
            if let Ok(saved) = serde_json::from_str::<Vec<Plugin>>(&raw) {
                for p in &mut self.plugins {
                    if let Some(s) = saved.iter().find(|s| s.id == p.id) {
                        p.enabled = s.enabled;
                    }
                }
            }
        }
    }

    fn save_state(&self) -> Result<()> {
        std::fs::write(self.state_path(), serde_json::to_string_pretty(&self.plugins)?)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<Plugin> {
        self.plugins.clone()
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<()> {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == id) {
            p.enabled = enabled;
            self.save_state()?;
        }
        Ok(())
    }
}

fn default_plugins() -> Vec<Plugin> {
    vec![
        Plugin {
            id: "blender".into(),
            name: "Blender Live Bridge".into(),
            description: "Connecta amb Blender via socket per executar scripts i veure l'escena en viu.".into(),
            enabled: true,
        },
        Plugin {
            id: "unreal".into(),
            name: "Unreal Engine 5 Remote Control".into(),
            description: "Controla Unreal Editor via Remote Control API i Python: Blueprints, actors, llums i packaging.".into(),
            enabled: true,
        },
        Plugin {
            id: "preview".into(),
            name: "Servidor de Preview".into(),
            description: "Serveix la carpeta del workspace per previsualitzar webs locals.".into(),
            enabled: true,
        },
    ]
}
