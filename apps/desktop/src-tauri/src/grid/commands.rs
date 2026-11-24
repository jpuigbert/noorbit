//! Comandaments Tauri de la xarxa altruista (grid).
//!
//! La UI (Ajustos ▸ «Xarxa altruista», el panell de la Jam i l'estalvi) hi
//! parla amb aquests comandaments. Tots llegeixen el `Runtime` global (els
//! fils de la Jam no reben `State` de Tauri, així que l'estat viu allà).

use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::AppHandle;

use super::client::CopyState;
use super::protocol;
use super::server::DistState;
use super::{client, server, Runtime, WorkView, RUNTIME};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfo {
    pub name: String,
    pub addr: String,
    /// Connectat ara mateix?
    pub connectat: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoluntariInfo {
    pub node_id: String,
    pub nom: String,
    /// Treballant ara?
    pub ocupat: bool,
    /// Torns donats en aquesta sessió.
    pub torns: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DistInfo {
    pub activa: bool,
    pub projecte: String,
    pub fitxers: u32,
    pub peces: u32,
    pub total_bytes: u64,
    pub baixades: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridStatus {
    pub enabled: bool,
    pub port: u16,
    /// Compartint la Jam actual (servidor escoltant)?
    pub compartint: bool,
    pub share_code: String,
    /// Invitació sencera «noorbit-grid://codi@ip:port» (si compartim).
    pub invitacio: String,
    pub peers: Vec<PeerInfo>,
    pub voluntaris: Vec<VoluntariInfo>,
    pub torns_donats: u64,
    pub torns_rebuts: u64,
    /// Unitat que aquest node té entre mans (vista del voluntari).
    pub treball: Option<WorkView>,
    pub copies: Vec<CopyState>,
    pub distribucio: Option<DistInfo>,
}

fn rt() -> Result<&'static Runtime, String> {
    RUNTIME.get().ok_or_else(|| "xarxa altruista no inicialitzada".to_string())
}

/// Estat complet de la xarxa (per a la UI).
#[tauri::command]
pub async fn grid_status(_app: AppHandle) -> Result<GridStatus, String> {
    let r = rt()?;
    let cfg = r.cfg.read().await.clone();
    let st = &r.state;
    let mut voluntaris = Vec::new();
    for (node_id, v) in st.volunteers.lock().await.iter() {
        voluntaris.push(VoluntariInfo {
            node_id: node_id.clone(),
            nom: v.nom.clone(),
            ocupat: v.ocupat.is_some(),
            torns: v.torns,
        });
    }
    voluntaris.sort_by(|a, b| a.nom.cmp(&b.nom));
    let mut peers = Vec::new();
    for p in &cfg.grid.peers {
        peers.push(PeerInfo {
            name: p.name.clone(),
            addr: p.addr.clone(),
            connectat: st.clients.lock().await.contains_key(&p.addr),
        });
    }
    let distribucio = st.dist.lock().await.as_ref().map(|d| DistInfo {
        activa: d.activa,
        projecte: d.manifest.projecte.clone(),
        fitxers: d.manifest.files.len() as u32,
        peces: d.manifest.num_peces(),
        total_bytes: d.manifest.total_bytes,
        baixades: d.baixades,
    });
    Ok(GridStatus {
        enabled: cfg.grid.enabled,
        port: cfg.grid.port,
        compartint: st.sharing.load(Ordering::Relaxed),
        share_code: cfg.grid.share_code.clone(),
        invitacio: invitacio(&cfg.grid),
        peers,
        voluntaris,
        torns_donats: st.units_donated.load(Ordering::Relaxed),
        torns_rebuts: st.units_received.load(Ordering::Relaxed),
        treball: st.current_work.read().await.clone(),
        copies: st.copies.read().await.clone(),
        distribucio,
    })
}

/// Interruptor mestre + port. Engega o atura els clients voluntaris.
#[tauri::command]
pub async fn grid_set_enabled(app: AppHandle, enabled: bool, port: Option<u16>) -> Result<GridStatus, String> {
    let r = rt()?;
    let mut cfg = r.cfg.read().await.clone();
    cfg.grid.enabled = enabled;
    if let Some(p) = port {
        cfg.grid.port = p;
    }
    cfg.save().map_err(|e| e.to_string())?;
    r.state.enabled.store(enabled, Ordering::Relaxed);
    if enabled {
        for p in &cfg.grid.peers {
            client::engega(p.clone(), r.state.clone(), r.ai.clone(), app.clone()).await;
        }
        // Si estàvem compartint amb un altre port, el servidor es reinicia.
        if r.state.sharing.load(Ordering::Relaxed) {
            server::atura(&r.state).await;
            server::engega(app.clone(), &r.state, &cfg.grid).await?;
        }
    } else {
        client::atura_tots(&r.state).await;
        server::atura(&r.state).await;
        r.state.sharing.store(false, Ordering::Relaxed);
    }
    grid_status(app).await
}

/// Comença a compartir la Jam actual: genera el codi si cal, engega el
/// servidor i torna la invitació per copiar.
#[tauri::command]
pub async fn grid_share_new(app: AppHandle) -> Result<GridStatus, String> {
    let r = rt()?;
    let mut cfg = r.cfg.read().await.clone();
    if cfg.grid.share_code.trim().is_empty() {
        cfg.grid.share_code = protocol::genera_codi();
    }
    cfg.grid.enabled = true;
    cfg.save().map_err(|e| e.to_string())?;
    r.state.enabled.store(true, Ordering::Relaxed);
    // Nom del projecte: el de la Jam (corrent o acabada de fa poc).
    let jam = r.jam.snapshot();
    if !jam.title.is_empty() {
        *r.state.project_name.write().await = jam.title.clone();
    }
    r.state.sharing.store(true, Ordering::Relaxed);
    server::engega(app.clone(), &r.state, &cfg.grid).await?;
    grid_status(app).await
}

/// Atura la compartició (la distribució en curs també es tanca).
#[tauri::command]
pub async fn grid_share_stop(app: AppHandle) -> Result<GridStatus, String> {
    let r = rt()?;
    r.state.sharing.store(false, Ordering::Relaxed);
    *r.state.dist.lock().await = None;
    server::atura(&r.state).await;
    grid_status(app).await
}

/// Afegeix un projecte ajudat amb la seva invitació.
#[tauri::command]
pub async fn grid_peer_add(app: AppHandle, invitacio: String) -> Result<GridStatus, String> {
    let r = rt()?;
    let (codi, addr) = protocol::parse_invitacio(&invitacio)?;
    let mut cfg = r.cfg.read().await.clone();
    let peer = crate::config::GridPeer {
        name: String::new(),
        addr: addr.clone(),
        code: codi,
    };
    if !cfg.grid.peers.iter().any(|p| p.addr == addr) {
        cfg.grid.peers.push(peer);
        cfg.save().map_err(|e| e.to_string())?;
    }
    if r.state.enabled.load(Ordering::Relaxed) {
        let p = cfg
            .grid
            .peers
            .iter()
            .find(|p| p.addr == addr)
            .cloned()
            .ok_or_else(|| "peer no trobat".to_string())?;
        client::engega(p, r.state.clone(), r.ai.clone(), app.clone()).await;
    }
    grid_status(app).await
}

/// Esborra un projecte ajudat.
#[tauri::command]
pub async fn grid_peer_remove(app: AppHandle, addr: String) -> Result<GridStatus, String> {
    let r = rt()?;
    let mut cfg = r.cfg.read().await.clone();
    cfg.grid.peers.retain(|p| p.addr != addr);
    cfg.save().map_err(|e| e.to_string())?;
    client::atura(&r.state, &addr).await;
    grid_status(app).await
}

/// L'estalvi del voluntari s'obre o es tanca (porta triple).
#[tauri::command]
pub async fn grid_note_saver(obert: bool) -> Result<(), String> {
    let r = rt()?;
    r.state.saver_open.store(obert, Ordering::Relaxed);
    Ok(())
}

// ── Fase 4: distribució estil torrent del projecte acabat ─────────────────

/// El propietari activa la distribució: manifest + peces + oferta als
/// participants (≥1 unitat donada). MAI no s'activa sol.
#[tauri::command]
pub async fn grid_distribute_start(app: AppHandle) -> Result<GridStatus, String> {
    let r = rt()?;
    let jam = r.jam.snapshot();
    if jam.running {
        return Err("la Jam encara corre: atura-la abans de distribuir el projecte".to_string());
    }
    let arrel = std::path::PathBuf::from(&jam.project_path);
    if jam.project_path.is_empty() || !arrel.is_dir() {
        return Err("no hi ha cap projecte de Jam acabat per distribuir".to_string());
    }
    let (manifest, dir_peces, salts) = server::construir_manifest(&arrel, &r.state.grid_dir)?;
    let mut cfg = r.cfg.read().await.clone();
    cfg.grid.enabled = true;
    cfg.save().map_err(|e| e.to_string())?;
    r.state.enabled.store(true, Ordering::Relaxed);
    r.state.sharing.store(true, Ordering::Relaxed);
    server::engega(app.clone(), &r.state, &cfg.grid).await?;

    *r.state.dist.lock().await = Some(DistState {
        manifest: manifest.clone(),
        dir_peces,
        activa: true,
        baixades: 0,
    });
    // OFERTA als participants (≥1 unitat donada).
    let elegibles: Vec<String> = r
        .state
        .volunteers
        .lock()
        .await
        .iter()
        .filter(|(_, v)| v.torns >= 1)
        .map(|(id, _)| id.clone())
        .collect();
    let projecte = manifest.projecte.clone();
    let vols = r.state.volunteers.lock().await;
    for id in &elegibles {
        if let Some(v) = vols.get(id) {
            let _ = v.tx.send(server::Ordre::Oferta(projecte.clone()));
        }
    }
    drop(vols);

    let avisos = if salts.is_empty() {
        String::new()
    } else {
        format!(" ({} fitxers massa grans s'han saltat)", salts.len())
    };
    r.state.emet(
        &app,
        super::GridEvent {
            kind: "copi".into(),
            detail: format!(
                "còpia oferta a {} participants: {} fitxers, {} peces, {} bytes{avisos}",
                elegibles.len(),
                manifest.files.len(),
                manifest.num_peces(),
                manifest.total_bytes
            ),
            projecte: projecte.clone(),
            ok: true,
        },
    );
    grid_status(app).await
}

/// Revoca la distribució (les còpies a mitges queden a mig fer).
#[tauri::command]
pub async fn grid_distribute_stop(app: AppHandle) -> Result<GridStatus, String> {
    let r = rt()?;
    *r.state.dist.lock().await = None;
    r.state.emet(
        &app,
        super::GridEvent {
            kind: "copi".into(),
            detail: "distribució revocada".to_string(),
            projecte: String::new(),
            ok: false,
        },
    );
    grid_status(app).await
}

/// Estat de les còpies rebudes (costat voluntari).
#[tauri::command]
pub async fn grid_copy_status(_app: AppHandle) -> Result<Vec<CopyState>, String> {
    let r = rt()?;
    Ok(r.state.copies.read().await.clone())
}

/// Carpeta on es munten les còpies rebudes.
#[tauri::command]
pub async fn grid_copy_set_dir(app: AppHandle, dir: String) -> Result<Vec<CopyState>, String> {
    let r = rt()?;
    let dir = dir.trim().to_string();
    if dir.is_empty() {
        return Err("la carpeta és buida".to_string());
    }
    let mut cfg = r.cfg.read().await.clone();
    cfg.grid.copies_dir = dir.clone();
    cfg.save().map_err(|e| e.to_string())?;
    *r.state.copies_dir.write().await = std::path::PathBuf::from(dir);
    grid_copy_status(app).await
}

/// Invitació sencera per copiar («noorbit-grid://codi@ip:port»).
fn invitacio(cfg: &crate::config::GridConfig) -> String {
    if cfg.share_code.trim().is_empty() {
        return String::new();
    }
    let ip = ip_local();
    format!("noorbit-grid://{}@{}:{}", cfg.share_code.trim(), ip, cfg.port)
}

/// Adreça IP local (la que veuen els altres ordinadors de la xarxa).
fn ip_local() -> String {
    // El truc del socket UDP connectat: no envia res, només resol la ruta.
    if let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if s.connect("8.8.8.8:80").is_ok() {
            if let Ok(local) = s.local_addr() {
                return local.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}
