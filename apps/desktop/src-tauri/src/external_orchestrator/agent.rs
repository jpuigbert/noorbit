//! Agent orquestrador: decideix **on** resol cada tasca que la IA principal
//! de NoOrbit no ha pogut resoldre.
//!
//! Flux de `delegate_task`:
//! 1. Si la màquina té prou memòria lliure, descobreix les IAs locals
//!    externes (Ollama, LM Studio, Jan, llama.cpp, vLLM) i prova els seus
//!    adaptadors de comunicació.
//! 2. Si no n'hi ha cap en marxa o totes fallen, fa el bot de rescat **en
//!    línia** amb els proveïdors oficials registrats amb token per l'usuari
//!    (`online_adapter`).
//! 3. Si res funciona, retorna un error explicatiu: mai credencials alienes,
//!    mai perfils falsos, mai accés al sistema fora del que l'usuari autoritza.
//! La resposta inclou sempre el seu ORIGEN real (sistema i model), perquè la
//! UI el puga etiquetar de manera honesta.

use crate::api::ApiManager;
use crate::external_orchestrator::{
    adapter, discovery::ExternalIa, online_adapter::OnlineAdapter,
};
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Resposta delegada amb l'origen real, per a poder etiquetar-la a la UI.
#[derive(Debug, Clone, Serialize)]
pub struct DelegatedResponse {
    pub text: String,
    /// Nom del sistema que ha respost (p. ex. «lm-studio (local externa)»).
    pub provider: String,
    /// Model concret que ha generat el text.
    pub model: String,
}

pub struct OrchestratorAgent {
    http: reqwest::Client,
    /// Adaptador en línia: present només si el gestor d'IA va crear
    /// l'orquestrador amb accés als proveïdors remotos (`with_remote`).
    online: Option<OnlineAdapter>,
}

impl Default for OrchestratorAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl OrchestratorAgent {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                // Si una IA externa no escolta, la detecció falla de seguida
                // en lloc de penjar el xat de l'usuari.
                .connect_timeout(Duration::from_millis(800))
                .build()
                .unwrap_or_default(),
            online: None,
        }
    }

    /// Orquestrador amb bot de rescat en línia: rebrà els proveïdors amb
    /// token registrats per l'usuari (APIs oficials).
    pub fn with_remote(remote: Arc<Mutex<ApiManager>>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_millis(800))
                .build()
                .unwrap_or_default(),
            online: Some(OnlineAdapter::new(remote)),
        }
    }

    /// Descobreix les IAs locals presents al sistema (en marxa o instal·lades).
    pub async fn discover(&self) -> Vec<ExternalIa> {
        crate::external_orchestrator::discovery::discover_all(&self.http).await
    }

    /// Memòria RAM lliure del sistema (bytes), si el podem determinar.
    /// Mètodes natius sense dependències noves (mateix esperit que
    /// `AiManager::total_memory_bytes`). Públic: altres mòduls (`imgen`)
    /// el consulten per saber si la màquina té prou recursos.
    pub fn free_memory_bytes() -> Option<u64> {
        #[cfg(target_os = "macos")]
        {
            let out = std::process::Command::new("vm_stat").output().ok()?;
            let text = String::from_utf8_lossy(&out.stdout);
            let page_size: u64 = text
                .lines()
                .find(|l| l.starts_with("Mach Virtual Memory Statistics"))
                .and_then(|l| l.split(|c: char| c.is_ascii_digit()).nth(1))
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(4096);
            let pages_of = |name: &str| -> u64 {
                text.lines()
                    .find(|l| l.starts_with(name))
                    .and_then(|l| {
                        l.split_whitespace()
                            .last()
                            .and_then(|v| v.trim_end_matches('.').parse().ok())
                    })
                    .unwrap_or(0)
            };
            // Lliures + inactives: el que realment pot ocupar un model nou.
            Some((pages_of("Pages free") + pages_of("Pages inactive")) * page_size)
        }
        #[cfg(target_os = "linux")]
        {
            let f = std::fs::read_to_string("/proc/meminfo").ok()?;
            let line = f.lines().find(|l| l.starts_with("MemAvailable:"))?;
            let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
            Some(kb * 1024)
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            None
        }
    }

    /// Hi ha prou memòria lliure perquè una IA local puga carregar-hi un
    /// model (llindar: 512 MB)? Si no n'hi ha, el bot de rescat va directe
    /// a l'adaptador en línia en lloc de tornar a saturar la màquina.
    pub fn has_sufficient_resources(&self) -> bool {
        match Self::free_memory_bytes() {
            Some(free) => free > 512 * 1024 * 1024,
            // Sistema no mesurable (Windows): assumim que sí.
            None => true,
        }
    }

    /// Delega una tasca: primer les IAs locals en marxa, després (si escau)
    /// l'adaptador en línia. Retorna la primera resposta vàlida amb el seu
    /// origen real identificable.
    pub async fn delegate_task(
        &self,
        prompt: &str,
        system: Option<&str>,
    ) -> Result<DelegatedResponse> {
        let available = self.discover().await;
        let running: Vec<&ExternalIa> = available.iter().filter(|ia| ia.is_running()).collect();
        let resources_ok = self.has_sufficient_resources();

        // 1) IAs locals externes: prova els adaptadors en ordre fins a una
        //    resposta vàlida. Les fallades individuals s'ignorenen. Si la
        //    màquina no té memòria lliure, no ho intentem ni carregarem més.
        let mut last_err = if running.is_empty() {
            "no hi ha cap IA local externa en marxa".to_string()
        } else if !resources_ok {
            "memòria lliure insuficient per delegar a una IA local".to_string()
        } else {
            String::new()
        };
        if resources_ok {
            for ia in running {
                match adapter::send_request(&self.http, ia, prompt, system).await {
                    Ok((text, model)) if !text.trim().is_empty() => {
                        return Ok(DelegatedResponse {
                            text,
                            provider: format!("{} (local externa)", ia.name),
                            model,
                        })
                    }
                    Ok(_) => last_err = format!("{}: resposta buida", ia.name),
                    Err(e) => last_err = e.to_string(),
                }
            }
        }

        // 2) Bot de rescat en línia (proveïdors oficials amb token de
        //    l'usuari). Si no n'hi ha cap configurat, el bot no pot fer res:
        //    l'error local original és el més informatiu.
        if let Some(online) = &self.online {
            match online.send_request(prompt, system).await {
                Ok((text, provider, model)) if !text.trim().is_empty() => {
                    return Ok(DelegatedResponse {
                        text,
                        provider: format!("{} (online)", provider),
                        model,
                    })
                }
                Ok(_) => {}
                Err(e) => last_err = format!("{} / online: {}", last_err, e),
            }
        }
        // No hi ha cap tercer pas: sense IA local ni proveïdor amb token,
        // la tasca es reporta com a no resolta, amb l'error explicatiu.
        Err(anyhow!(
            "Cap IA externa ha pogut respondre ({}). Comprova que Ollama o una altra IA local estiguen en marxa, o que hi haja un proveïdor en línia amb token.",
            last_err
        ))
    }

    /// Nom de comoditat per a `delegate_task`.
    pub async fn delegate(&self, prompt: &str, system: Option<&str>) -> Result<DelegatedResponse> {
        self.delegate_task(prompt, system).await
    }
}
