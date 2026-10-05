//! Adaptador EN LÍNIA per al bot de rescat de l'orquestrador.
//!
//! El pla original proposava fer scraping de la interfície web de tercers
//! suplantant perfils falsos; aquest codi fa el mateix servei d'una forma
//! legítima i estable: consulta els **proveïdors d'IA en línia registrats
//! per l'usuari amb token** (mòdul `api`), que parlen OpenAI/Claude i són
//! API oficials — Perplexity (api.perplexity.ai), OpenRouter, Venice,
//! Groq… qualsivolguts. Cap identitat falsa, cap rotació de capçaleres:
//! la petició s'envia amb el compte del propi usuari, que és qui n'ha
//! autoritzat l'ús.

use crate::api::{self, ApiManager};
use anyhow::{anyhow, Result};
use std::sync::{Arc, Mutex};

pub struct OnlineAdapter {
    remote: Arc<Mutex<ApiManager>>,
}

impl OnlineAdapter {
    pub fn new(remote: Arc<Mutex<ApiManager>>) -> Self {
        Self { remote }
    }

    /// Hi ha algun proveïdor en línia amb token i activat? (per a la UI i
    /// per a no intentar un bot de rescat que no pot respondre).
    pub fn has_providers(&self) -> bool {
        self.remote
            .lock()
            .unwrap()
            .providers
            .iter()
            .any(|p| p.enabled && !p.token.trim().is_empty())
    }

    /// Envia la petició als proveïdors en línia configurats i retorna
    /// (text, nom del proveïdor, model) de la primera resposta vàlida, perquè
    /// l'origen es pugua etiquetar. Les fallades individuals s'ignorenen,
    /// igual que fa `ask_peers` amb les consultes col·lectives.
    pub async fn send_request(
        &self,
        prompt: &str,
        system: Option<&str>,
    ) -> Result<(String, String, String)> {
        // Fotografia els ids habilitats (allibera el guard abans dels awaits).
        let ids: Vec<String> = {
            let guard = self.remote.lock().unwrap();
            guard
                .providers
                .iter()
                .filter(|p| p.enabled && !p.token.trim().is_empty())
                .map(|p| p.id.clone())
                .collect()
        };
        if ids.is_empty() {
            return Err(anyhow!(
                "Cap proveïdor en línia configurat amb token: l'adaptador online no pot respondre"
            ));
        }
        let sys = system.unwrap_or("Respon en català, de forma concisa i directa.");
        let mut last_err = String::new();
        for id in ids {
            let prepared = self.remote.lock().unwrap().prepare(&id);
            let (provider, client) = match prepared {
                Ok(v) => v,
                Err(e) => {
                    last_err = e.to_string();
                    continue;
                }
            };
            let one_turn = [("user".to_string(), prompt.to_string())];
            match api::chat(&client, &provider, &one_turn, Some(sys)).await {
                Ok((content, _)) if !content.trim().is_empty() => {
                    return Ok((content, provider.name, provider.model))
                }
                Ok(_) => last_err = format!("{}: resposta buida", provider.name),
                Err(e) => last_err = format!("{}: {}", provider.name, e),
            }
        }
        Err(anyhow!("Cap proveïdor en línia ha pogut respondre ({}). L'usuari pot tindre més proveïdors activats i amb token (menú Proveïdors d'IA).", last_err))
    }
}
