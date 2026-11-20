//! Comandes del BUCLE DE DADES d'aprenentatge (vots + paquet Soup + porta
//! d'avaluació del cervell).
//!
//! S'han posat aquí i no a `commands/learning.rs` perquè aquest segon fitxer
//! és gran i molt tocat: així el que fa créixer el dataset queda apart del
//! que el destil·la, i la UI pot cridar l'un sense esperar l'altre.
//!
//! Recordem el criteri de Soup (Apache-2.0) que apliquem: una dada no
//! mesurada és `null`, no un zero; un fitxer de registre només s'afegeix; i
//! el que no es pot comprovar es DIU que no s'ha pogut comprovar.

use crate::ai::AiManager;
use crate::learning::{Exchange, SoupPack};
use crate::AppState;
use serde::Serialize;
use tauri::{command, AppHandle, Emitter, State};

/// Resultat d'un vot: el torn votat (amb el vot ja posat) i els recomptes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteInfo {
    pub exchange: Exchange,
    pub thumbs_up: usize,
    pub thumbs_down: usize,
}

/// Vota una resposta del xat: `1` = 👍, `-1` = 👎, `0` = retirar el vot.
///
/// El que fa servir un dataset NO és tenir converses, sinó saber quines van
/// bé. La interfície pot donar l'`id` del registre o la pregunta (el xat només
/// guarda el text); si no es troba cap torn, s'avisa en lloc de fingir que
/// s'ha desat res.
#[command]
pub async fn learning_vote(
    app: AppHandle,
    state: State<'_, AppState>,
    vote: i8,
    id: Option<String>,
    question: Option<String>,
) -> Result<VoteInfo, String> {
    let trobat = match (id.as_deref(), question.as_deref()) {
        (Some(id), _) => state.learning.vote(id, vote),
        (None, Some(q)) => state.learning.vote_by_question(q, vote),
        (None, None) => return Err("Cal donar un id o la pregunta per votar.".to_string()),
    };
    let Some(exchange) = trobat else {
        return Err(
            "Aquesta resposta no és al registre d'aprenentatge (potser l'aprenentatge \
             està desactivat o el torn ja n'ha caigut fora)."
                .to_string(),
        );
    };
    // El vot es reflecteix de seguida als fitxers que entén Soup: el bucle de
    // dades no espera que l'usuari obri un diàleg d'exportació.
    let (up, down) = state.learning.votes();
    let base = base_per_entrenar(&state).await;
    tauri::async_runtime::spawn({
        let store = state.learning.clone();
        async move {
            if let Err(e) = store.soup_sync(&base) {
                eprintln!("NoOrbit/aprenentatge: {e}");
            }
        }
    });
    let _ = app.emit("learning://changed", ());
    Ok(VoteInfo {
        exchange,
        thumbs_up: up,
        thumbs_down: down,
    })
}

/// Aboca el registre al format de Soup (`.soup-loops/`): fitxers de conversa
/// i de feedback, `train.jsonl` ChatML, `preference.jsonl` per a DPO i els
/// `soup.yaml`. No instal·la res i no entrena res: escriu dades i diu què
/// falta (vots, files, fugues de validació i si aquest equip hi pot entrenar).
#[command]
pub async fn learning_soup_export(state: State<'_, AppState>) -> Result<SoupPack, String> {
    let base = base_per_entrenar(&state).await;
    state.learning.soup_sync(&base)
}

/// Últim veredicte de la PORTA d'avaluació del cervell natiu: quan la nativa
/// intenta aprendre alguna cosa, primer es comprova si empitjora les
/// respostes que l'usuari ja va aprovar; si les empitjora, es desfà.
///
/// Va aquí perquè la porta juga amb els mateixos 👍 que alimenten el paquet
/// de Soup. `None` vol dir «encara no s'ha intentat cap fusió des que
/// l'aplicació és oberta», no «tot bé».
#[command]
pub async fn learning_brain_verdict() -> Option<crate::ai::brain_gate::Resultat> {
    crate::ai::brain_gate::darrer()
}

/// Quin model es posa com a `base` del `soup.yaml`: el que l'usuari ha triat
/// per destil·lar; si no n'hi ha, el seleccionat al xat; i si tampoc, el model
/// inicial per a aquesta màquina. No s'inventa cap nom.
async fn base_per_entrenar(state: &AppState) -> String {
    let del_cfg = { state.config.read().await.learning.model.trim().to_string() };
    if !del_cfg.is_empty() {
        return del_cfg;
    }
    let ai = state.ai_manager.clone();
    match ai.selected_model().await {
        Some(m) if !m.trim().is_empty() => m,
        _ => AiManager::starter_model_name(),
    }
}
