//! XARXA ALTRUISTA P2P («grid»).
//!
//! Sense servidor central: el PROPIETARI d'un projecte és el «servidor» (un
//! endpoint QUIC/TLS amb certificat autofirmat) i els VOLUNTARIS s'hi
//! connecten directament amb una invitació (`noorbit-grid://codi@ip:port`).
//!
//! El propietari comparteix els torns de PENSAMENT de la Jam: cada torn
//! surt com a `Unit` a la xarxa i, si un voluntari el respon abans de la
//! data límit (per defecte 120 s), el seu text és el resultat; si no, la
//! Jam cau a la IA local (fallback transparent). El voluntari només aporta
//! INFERÈNCIA amb el seu model local: mai no executa codi del projecte.
//!
//! Quan la Jam acaba (Fase 4), el propietari pot activar la distribució
//! estil torrent: manifest amb peces de 256 KB i hashes SHA-256, descàrrega
//! amb represa i verificació per als participants (≥1 unitat donada).
//!
//! El `Runtime` es publica un cop al setup de lib.rs (patró dels motors:
//! els fils de la Jam no reben `State<'_, …>` de Tauri).

pub mod client;
pub mod commands;
pub mod protocol;
pub mod server;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::{oneshot, Mutex, RwLock};

use crate::ai::AiManager;
use crate::config::{AppConfig, GridConfig};
use crate::jam::commands::JamHandle;
use protocol::Unit;

/// Temps que una unitat espera un voluntari abans de caure a la IA local.
pub const DEADLINE_UNITAT_SECS: u64 = 120;

/// Temps d'ociositat mínim perquè un voluntari treballi (el mateix llindar
/// que l'aprenentatge en repòs: «realment ningú no toca res»).
pub const IDLE_MIN_SECS: u64 = crate::ai::wander::IDLE_SECS;

/// Event global cap a la interfície: «grid://event».
pub const EVENT_GRID: &str = "grid://event";

/// Runtime del grid (instal·lat un cop al setup de lib.rs).
pub struct Runtime {
    pub app: AppHandle,
    pub ai: Arc<AiManager>,
    pub cfg: Arc<RwLock<AppConfig>>,
    pub jam: Arc<JamHandle>,
    pub state: Arc<GridState>,
}

pub static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Publica el runtime (cridat una vegada, abans de res del grid).
pub fn set_runtime(rt: Runtime) {
    let _ = RUNTIME.set(rt);
}

/// Estat compartit de la xarxa: la cua d'unitats, els voluntaris connectats,
/// els comptadors i (Fase 4) la distribució en curs i les còpies rebudes.
pub struct GridState {
    /// Interruptor mestre (persistit a la configuració).
    pub enabled: AtomicBool,
    /// Estem compartint la Jam actual (servidor escoltant).
    pub sharing: AtomicBool,
    /// Nom del projecte compartit (per a l'estalvi i les invitacions).
    pub project_name: RwLock<String>,
    /// L'estalvi de pantalla del voluntari és obert? (porta triple).
    pub saver_open: AtomicBool,
    /// Torns DONATS com a voluntari.
    pub units_donated: AtomicU64,
    /// Torns REBUTS com a propietari.
    pub units_received: AtomicU64,
    /// Unitat que el voluntari està processant ara (per a la pantalla).
    pub current_work: RwLock<Option<WorkView>>,
    /// Unitats pendents de resposta (id → canal del torn de la Jam).
    pending: Mutex<HashMap<String, PendingUnit>>,
    /// Voluntaris connectats (node_id → connexió viva).
    pub volunteers: Mutex<HashMap<String, VolConn>>,
    /// Stop del servidor QUIC (si escolta).
    pub server_stop: Mutex<Option<oneshot::Sender<()>>>,
    /// Stop dels clients voluntaris (addr → canal).
    pub clients: Mutex<HashMap<String, oneshot::Sender<()>>>,
    /// Distribució en curs del projecte acabat (Fase 4, costat propietari).
    pub dist: Mutex<Option<server::DistState>>,
    /// Còpies rebudes en curs o acabades (Fase 4, costat voluntari).
    pub copies: RwLock<Vec<client::CopyState>>,
    /// Identitat estable d'aquest node (per als comptadors i la Fase 4).
    pub node_id: String,
    /// Carpeta de dades del grid (certificat, peces, còpies).
    pub grid_dir: PathBuf,
    /// Carpeta base de les còpies rebudes (configurable).
    pub copies_dir: RwLock<PathBuf>,
}

/// Unitat que el voluntari té entre mans (per pintar-la a l'estalvi).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkView {
    pub projecte: String,
    pub label: String,
    pub detail: String,
}

/// Event cap a la interfície.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridEvent {
    pub kind: String,
    pub detail: String,
    pub projecte: String,
    pub ok: bool,
}

struct PendingUnit {
    tx: oneshot::Sender<String>,
}

/// Connexió viva d'un voluntari al servidor del propietari.
pub struct VolConn {
    pub nom: String,
    pub tx: tokio::sync::mpsc::UnboundedSender<server::Ordre>,
    /// Unitat que està processant ara (`None` = lliure).
    pub ocupat: Option<String>,
    /// Torns donats en aquesta sessió (Fase 4: elegibilitat).
    pub torns: u32,
}

impl GridState {
    pub fn new(cfg: &GridConfig, grid_dir: PathBuf) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&grid_dir);
        Arc::new(Self {
            enabled: AtomicBool::new(cfg.enabled),
            sharing: AtomicBool::new(false),
            project_name: RwLock::new(String::new()),
            saver_open: AtomicBool::new(false),
            units_donated: AtomicU64::new(0),
            units_received: AtomicU64::new(0),
            current_work: RwLock::new(None),
            pending: Mutex::new(HashMap::new()),
            volunteers: Mutex::new(HashMap::new()),
            server_stop: Mutex::new(None),
            clients: Mutex::new(HashMap::new()),
            dist: Mutex::new(None),
            copies: RwLock::new(Vec::new()),
            node_id: node_id(&grid_dir),
            copies_dir: RwLock::new(
                cfg.copies_dir
                    .trim()
                    .is_empty()
                    .then(|| grid_dir.join("copies"))
                    .unwrap_or_else(|| PathBuf::from(cfg.copies_dir.trim())),
            ),
            grid_dir,
        })
    }

    /// Emet un event «grid://event» a totes les finestres.
    pub fn emet(&self, app: &AppHandle, ev: GridEvent) {
        let _ = app.emit(EVENT_GRID, ev);
    }

    /// Registra un voluntari connectat. Si torna a connectar-se, conserva
    /// els torns donats (l'elegibilitat de la Fase 4 no es perd).
    pub async fn registra(
        &self,
        node_id: String,
        nom: String,
        tx: tokio::sync::mpsc::UnboundedSender<server::Ordre>,
    ) {
        let mut vols = self.volunteers.lock().await;
        let torns = vols.get(&node_id).map(|v| v.torns).unwrap_or(0);
        vols.insert(
            node_id.clone(),
            VolConn { nom, tx, ocupat: None, torns },
        );
    }

    /// Desconnecta un voluntari i allibera la unitat que tingués entre mans.
    pub async fn desregistra(&self, node_id: &str) {
        let ocupat = {
            let mut vols = self.volunteers.lock().await;
            vols.remove(node_id).and_then(|v| v.ocupat)
        };
        if let Some(id) = ocupat {
            self.allibera(node_id, &id).await;
        }
    }

    /// El voluntari queda lliure (refús, desconnexió o resultat tardà).
    async fn allibera(&self, node_id: &str, id: &str) {
        let mut vols = self.volunteers.lock().await;
        if let Some(v) = vols.get_mut(node_id) {
            if v.ocupat.as_deref() == Some(id) {
                v.ocupat = None;
            }
        }
        let enfeina = vols.values().filter(|v| v.ocupat.as_deref() == Some(id)).count();
        drop(vols);
        if enfeina == 0 {
            // Ningú no la respondrà: resolem AVIAT (la Jam cau a la IA local
            // sense esperar la data límit sencera).
            if let Some(p) = self.pending.lock().await.remove(id) {
                drop(p);
            }
        }
    }

    /// Resultat d'un voluntari per a una unitat pendent.
    pub async fn rep_result(&self, app: &AppHandle, node_id: &str, id: &str, text: &str) {
        // TEXT BUIT = refús (porta triple tancada): la unitat segueix
        // pendent per a la resta i aquest voluntari queda lliure.
        if text.trim().is_empty() {
            self.allibera(node_id, id).await;
            return;
        }
        let p = self.pending.lock().await.remove(id);
        let Some(p) = p else {
            // Ja havia caducat: el resultat arriba tard, no es fa servir.
            self.allibera(node_id, id).await;
            return;
        };
        // TORN REBUT: comptadors i alliberament dels altres voluntaris.
        self.units_received.fetch_add(1, Ordering::Relaxed);
        let nom = {
            let mut vols = self.volunteers.lock().await;
            let mut nom = node_id.to_string();
            if let Some(v) = vols.get_mut(node_id) {
                v.torns += 1;
                v.ocupat = None;
                nom = v.nom.clone();
            }
            for v in vols.values_mut() {
                if v.ocupat.as_deref() == Some(id) {
                    v.ocupat = None;
                    let _ = v.tx.send(server::Ordre::Ack(id.to_string()));
                }
            }
            nom
        };
        let projecte = self.project_name.read().await.clone();
        let _ = p.tx.send(text.to_string());
        self.emet(
            app,
            GridEvent {
                kind: "rebut".into(),
                detail: format!("«{nom}» ha respost el torn"),
                projecte,
                ok: true,
            },
        );
    }
}

/// Envia un torn de pensament de la Jam a la xarxa altruista.
///
/// Torna `None` si no hi ha voluntaris, si tots refusen o si ningú no
/// respon abans de la data límit — aleshores la Jam cau a la IA local.
pub async fn submet_unitat(
    prompt: &str,
    system: Option<&str>,
    model_hint: Option<&str>,
) -> Option<String> {
    let Some(rt) = RUNTIME.get() else { return None };
    rt.submet(prompt, system, model_hint).await
}

impl Runtime {
    async fn submet(&self, prompt: &str, system: Option<&str>, model_hint: Option<&str>) -> Option<String> {
        let st = &self.state;
        if !st.sharing.load(Ordering::Relaxed) {
            return None;
        }
        if prompt.len() > protocol::MAX_PROMPT {
            return None;
        }
        // Sense voluntaris connectats, caiguda immediata (res d'esperar).
        let mut vols = st.volunteers.lock().await;
        if vols.is_empty() {
            return None;
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let label = self.etiqueta_tasca().await;
        let unit = Unit {
            id: id.clone(),
            system: system.unwrap_or_default().to_string(),
            prompt: prompt.to_string(),
            model_hint: model_hint.unwrap_or_default().to_string(),
            max_tokens: protocol::MAX_TOKENS,
            label,
        };
        let (tx, rx) = oneshot::channel();
        st.pending.lock().await.insert(id.clone(), PendingUnit { tx });
        // Es reparteix a TOTS els voluntaris lliures: el primer resultat
        // guanya i la resta reben l'Ack.
        for v in vols.values_mut() {
            if v.ocupat.is_none() {
                v.ocupat = Some(id.clone());
                let _ = v.tx.send(server::Ordre::Unit(unit.clone()));
            }
        }
        drop(vols);
        match tokio::time::timeout(Duration::from_secs(DEADLINE_UNITAT_SECS), rx).await {
            Ok(Ok(text)) => Some(text),
            _ => {
                // Caducat: allibera els voluntaris que encara hi treballaven.
                st.pending.lock().await.remove(&id);
                let mut vols = st.volunteers.lock().await;
                for v in vols.values_mut() {
                    if v.ocupat.as_deref() == Some(&id) {
                        v.ocupat = None;
                        let _ = v.tx.send(server::Ordre::Ack(id.clone()));
                    }
                }
                None
            }
        }
    }

    /// Etiqueta del torn actual per a la pantalla del voluntari.
    async fn etiqueta_tasca(&self) -> String {
        let j = self.jam.snapshot();
        if !j.running {
            return String::new();
        }
        let total = j.total.max(1) as u32;
        let fetes = (j.completed + j.blocked + j.skipped).min(total.saturating_sub(1));
        let de = (fetes + 1).min(total);
        if j.current_task.is_empty() {
            format!("tasca {de} de {total}")
        } else {
            format!("tasca {de} de {total} · «{}»", j.current_task)
        }
    }
}

/// Identitat estable del node: un uuid desat a la carpeta del grid.
fn node_id(grid_dir: &std::path::Path) -> String {
    let path = grid_dir.join("node_id");
    if let Ok(existent) = std::fs::read_to_string(&path) {
        let t = existent.trim();
        if t.len() >= 8 {
            return t.to_string();
        }
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let _ = std::fs::write(&path, &id);
    id
}

/// Nom local de la màquina (per presentar-se al propietari).
pub fn nom_local() -> String {
    for var in ["HOSTNAME", "COMPUTERNAME", "USER", "USERNAME"] {
        if let Ok(v) = std::env::var(var) {
            if !v.trim().is_empty() {
                return format!("voluntari·{}", v.trim());
            }
        }
    }
    "voluntari".to_string()
}

/// Arrenca el que la configuració demani (interruptor mestre + peers).
pub async fn arranca_segons_config(cfg: GridConfig) {
    let Some(rt) = RUNTIME.get() else { return };
    if cfg.enabled {
        for p in &cfg.peers {
            client::engega(p.clone(), rt.state.clone(), rt.ai.clone(), rt.app.clone()).await;
        }
    }
}

/// L'estalvi del voluntari s'ha obert o tancat (porta triple).
pub fn nota_estalvi(obert: bool) {
    if let Some(rt) = RUNTIME.get() {
        rt.state.saver_open.store(obert, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod proves {
    use super::*;

    #[test]
    fn node_id_es_estable() {
        let dir = std::env::temp_dir().join(format!("no-orbit-grid-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = node_id(&dir);
        let b = node_id(&dir);
        assert_eq!(a, b, "la identitat ha de ser estable");
        std::fs::remove_dir_all(&dir).ok();
    }
}
