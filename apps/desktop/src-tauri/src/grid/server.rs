//! SERVIDOR QUIC DEL PROPIETARI.
//!
//! Quan el propietari activa «Comparteix amb la comunitat», aquest endpoint
//! escolta amb un certificat autofirmat (generat un cop amb `rcgen` i desat
//! a l'appdata) i accepta voluntaris que presentin la invitació correcta.
//!
//! Cada connexió té un sol stream bidireccional: el servidor hi envia les
//! unitats de la cua (via l'`Ordre` del canal intern) i hi rep els resultats;
//! a la Fase 4 hi serveix el manifest i les peces de la còpia final (només
//! als participants amb ≥1 unitat donada, registrats per node id).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, oneshot};

use crate::config::GridConfig;
use tauri::AppHandle;

use super::protocol::{self, FitxerManifest, Manifest, Msg};
use super::{GridEvent, GridState};

/// Comandaments interns del servidor cap a una connexió de voluntari.
#[derive(Debug, Clone)]
pub enum Ordre {
    /// Una unitat de pensament per respondre.
    Unit(protocol::Unit),
    /// Abandona aquesta unitat (un altre l'ha respost o ha caducat).
    Ack(String),
    /// El projecte ha acabat: tens dret a una còpia.
    Oferta(String),
    /// El manifest de la còpia (resposta a ManifestReq).
    Manifest(Manifest),
    /// Tanca la connexió (compartició aturada).
    Fi,
}

/// Distribució en curs del projecte acabat (Fase 4, costat propietari).
pub struct DistState {
    pub manifest: Manifest,
    /// Carpeta amb les peces desades (una per fitxer, índex = nom).
    pub dir_peces: PathBuf,
    pub activa: bool,
    /// Còpies completes servides (comptador per a la UI).
    pub baixades: u32,
}

// ── Certificat autofirmat (generat un cop, desat a l'appdata) ─────────────

struct Certs {
    der: Vec<u8>,
    clau: Vec<u8>,
}

/// Carrega el certificat desat o en genera un de nou i el desa. Es desa en
/// DER directament (cert + clau PKCS#8): es el que rustls necessita i es
/// pot tornar a llegir sense tornar a parsejar cap PEM.
fn certificat(grid_dir: &Path) -> Result<Certs, String> {
    let cert_der = grid_dir.join("grid_cert.der");
    let key_der = grid_dir.join("grid_key.der");
    if let (Ok(c), Ok(k)) = (std::fs::read(&cert_der), std::fs::read(&key_der)) {
        if !c.is_empty() && !k.is_empty() {
            return Ok(Certs { der: c, clau: k });
        }
        eprintln!("grid: certificat desat il·legible: se'n genera un de nou");
    }
    let generat = rcgen::generate_simple_self_signed(vec![protocol::SERVER_NAME.to_string()])
        .map_err(|e| format!("rcgen: {e}"))?;
    let der = generat.cert.der().as_ref().to_vec();
    let clau = generat.key_pair.serialize_der();
    let _ = std::fs::write(&cert_der, &der);
    let _ = std::fs::write(&key_der, &clau);
    Ok(Certs { der, clau })
}

// ── Arrencada i aturada del servidor ──────────────────────────────────────

/// Engega el servidor QUIC (idempotent: si ja escolta, no fa res).
pub async fn engega(app: AppHandle, state: &Arc<GridState>, cfg: &GridConfig) -> Result<(), String> {
    if state.server_stop.lock().await.is_some() {
        return Ok(());
    }
    let certs = certificat(&state.grid_dir)?;
    let cert_der = rustls::pki_types::CertificateDer::from(certs.der);
    let key_der = rustls::pki_types::PrivateKeyDer::try_from(certs.clau)
        .map_err(|e| format!("clau privada: {e}"))?;
    let server_config = quinn::ServerConfig::with_single_cert(vec![cert_der], key_der)
        .map_err(|e| format!("config QUIC: {e}"))?;
    let addr: SocketAddr = format!("0.0.0.0:{}", cfg.port)
        .parse()
        .map_err(|e| format!("adreça: {e}"))?;
    let endpoint = quinn::Endpoint::server(server_config, addr)
        .map_err(|e| format!("no s'ha pogut obrir el port {}: {e}", cfg.port))?;
    let (tx, rx) = oneshot::channel();
    let st = state.clone();
    let cfg2 = cfg.clone();
    let port = cfg.port;
    tauri::async_runtime::spawn(async move {
        accept_loop(app, endpoint, st, cfg2, rx).await;
    });
    *state.server_stop.lock().await = Some(tx);
    eprintln!("grid: servidor escoltant al port {port}");
    Ok(())
}

/// Atura el servidor (si escolta).
pub async fn atura(state: &Arc<GridState>) {
    if let Some(tx) = state.server_stop.lock().await.take() {
        let _ = tx.send(());
        eprintln!("grid: servidor aturat");
    }
}

async fn accept_loop(
    app: AppHandle,
    endpoint: quinn::Endpoint,
    state: Arc<GridState>,
    cfg: GridConfig,
    mut stop: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = &mut stop => {
                endpoint.close(0u32.into(), b"comparticio aturada");
                return;
            }
            incoming = endpoint.accept() => match incoming {
                Some(incoming) => match incoming.await {
                    Ok(conn) => {
                        let st = state.clone();
                        let cfg = cfg.clone();
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = atendre(conn, &st, &cfg, &app).await {
                                eprintln!("grid: connexió tancada: {e}");
                            }
                        });
                    }
                    Err(e) => eprintln!("grid: connexió rebutjada: {e}"),
                },
                None => return,
            }
        }
    }
}

/// Una connexió de voluntari: autenticació, registre i bucle de missatges.
async fn atendre(
    conn: quinn::Connection,
    state: &Arc<GridState>,
    cfg: &GridConfig,
    app: &AppHandle,
) -> Result<(), String> {
    let (mut enviar, mut rebre) = conn
        .accept_bi()
        .await
        .map_err(|e| format!("accept_bi: {e}"))?;
    let mut lector = protocol::LectorLinies::nou();

    // AUTHENTICACIÓ: el primer missatge és el Hello amb la invitació.
    let hola = tokio::time::timeout(Duration::from_secs(30), protocol::rep(&mut rebre, &mut lector))
        .await
        .map_err(|_| "timeout esperant el Hello".to_string())?
        .map_err(|e| format!("Hello: {e}"))?;
    let (invite, node_id, nom) = match hola {
        Msg::Hello { versio, invite, node_id, nom, .. } => {
            if versio != protocol::VERSIO {
                let _ = protocol::envia(&mut enviar, &Msg::Rebutjat { motiu: format!("versió de protocol {versio} no suportada") }).await;
                return Err("versió de protocol".into());
            }
            (invite, node_id, nom)
        }
        _ => return Err("primer missatge no és un Hello".into()),
    };
    let codi_correcte = !cfg.share_code.trim().is_empty()
        && invite.trim().eq_ignore_ascii_case(cfg.share_code.trim());
    if !codi_correcte {
        let _ = protocol::envia(&mut enviar, &Msg::Rebutjat { motiu: "codi d'invitació incorrecte".into() }).await;
        return Err("invitació rebutjada".into());
    }

    let projecte = state.project_name.read().await.clone();
    protocol::envia(&mut enviar, &Msg::Welcome { projecte: projecte.clone() }).await?;

    let (tx, mut rx) = mpsc::unbounded_channel::<Ordre>();
    state.registra(node_id.clone(), nom, tx).await;

    let resultat = loop {
        tokio::select! {
            msg = protocol::rep(&mut rebre, &mut lector) => {
                let Ok(msg) = msg else { break };
                match msg {
                    Msg::Result { id, text } => state.rep_result(app, &node_id, &id, &text).await,
                    Msg::ManifestReq => {
                        if let Some(dist) = state.dist.lock().await.as_ref() {
                            if dist.activa && elegible(state, &node_id).await {
                                let _ = protocol::envia(&mut enviar, &Msg::Manifest(dist.manifest.clone())).await;
                            }
                        }
                    }
                    Msg::PieceReq { bitfield } => {
                        if servir_peces(state, &mut enviar, &node_id, &bitfield).await.is_err() {
                            break;
                        }
                    }
                    Msg::Copiada { } => {
                        if let Some(dist) = state.dist.lock().await.as_mut() {
                            dist.baixades += 1;
                            let ev = GridEvent {
                                kind: "copi".into(),
                                detail: format!("«{}» ha rebut la còpia completa", nom_voluntari(state, &node_id).await),
                                projecte: projecte.clone(),
                                ok: true,
                            };
                            state.emet(app, ev);
                        }
                    }
                    _ => break,
                }
            }
            Some(ordre) = rx.recv() => {
                let msg = match ordre {
                    Ordre::Unit(u) => Msg::Unit(u),
                    Ordre::Ack(id) => Msg::Ack { id },
                    Ordre::Oferta(p) => Msg::Oferta { projecte: p },
                    Ordre::Manifest(m) => Msg::Manifest(m),
                    Ordre::Fi => {
                        let _ = protocol::envia(&mut enviar, &Msg::Fi).await;
                        break;
                    }
                };
                if protocol::envia(&mut enviar, &msg).await.is_err() {
                    break;
                }
            }
        }
    };
    state.desregistra(&node_id).await;
    let _ = resultat;
    Ok(())
}

/// El voluntari té dret a la còpia? (≥1 unitat donada en aquesta sessió).
async fn elegible(state: &Arc<GridState>, node_id: &str) -> bool {
    state
        .volunteers
        .lock()
        .await
        .get(node_id)
        .map(|v| v.torns >= 1)
        .unwrap_or(false)
}

async fn nom_voluntari(state: &Arc<GridState>, node_id: &str) -> String {
    state
        .volunteers
        .lock()
        .await
        .get(node_id)
        .map(|v| v.nom.clone())
        .unwrap_or_else(|| "un participant".to_string())
}

/// Serveix les peces que el voluntari no té (com a molt 4 per petició).
async fn servir_peces(
    state: &Arc<GridState>,
    enviar: &mut quinn::SendStream,
    node_id: &str,
    bitfield: &[u8],
) -> Result<(), String> {
    let dist_guard = state.dist.lock().await;
    let Some(dist) = dist_guard.as_ref() else {
        return Ok(());
    };
    if !dist.activa || !elegible(state, node_id).await {
        return Ok(());
    }
    let total = dist.manifest.num_peces() as usize;
    let mut enviades = 0;
    for i in 0..total {
        let te = bitfield
            .get(i / 8)
            .map(|b| b & (1 << (i % 8)) != 0)
            .unwrap_or(false);
        if te {
            continue;
        }
        let path = dist.dir_peces.join(format!("{i}"));
        let bytes = std::fs::read(&path).map_err(|e| format!("peça {i}: {e}"))?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        protocol::envia(enviar, &Msg::PieceData { index: i as u32, bytes: b64 }).await?;
        enviades += 1;
        if enviades >= 4 {
            break;
        }
    }
    Ok(())
}

// ── Construcció del manifest (Fase 4) ────────────────────────────────────

/// Carpetes que mai entren a la còpia (les mateixes exclusions que els
/// snapshots del projecte).
const EXCLUSIONS: &[&str] = &["target", "node_modules", ".git", ".godot"];

/// Mida màxima d'un fitxer individual dins de la còpia (128 MiB): més gran
/// es salta amb un avís (MVP: no volem omplir el disc de peces gegants).
const MAX_FITXER: u64 = 128 * 1024 * 1024;

/// Construeix el manifest i desa les peces a disc. Torna (manifest, dir).
pub fn construir_manifest(arrel: &Path, grid_dir: &Path) -> Result<(Manifest, PathBuf, Vec<String>), String> {
    let nom = arrel
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "projecte".into());
    let dir_peces = grid_dir.join("dist").join(format!("{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir_peces).map_err(|e| e.to_string())?;

    let mut fitxers: Vec<(PathBuf, PathBuf)> = Vec::new(); // (absolut, relatiu)
    recull(arrel, arrel, &mut fitxers)?;
    fitxers.sort_by(|a, b| a.1.cmp(&b.1));

    let mut files: Vec<FitxerManifest> = Vec::new();
    let mut piece_hashes: Vec<String> = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut salts: Vec<String> = Vec::new();

    for (abs, rel) in &fitxers {
        let mida = std::fs::metadata(abs).map(|m| m.len()).unwrap_or(0);
        if mida > MAX_FITXER {
            salts.push(format!("{} ({} MB, massa gran)", rel.display(), mida / (1024 * 1024)));
            continue;
        }
        let bytes = std::fs::read(abs).map_err(|e| format!("{}: {e}", rel.display()))?;
        let piece_ini = piece_hashes.len() as u32;
        for tros in bytes.chunks(protocol::PIECE_SIZE as usize) {
            let hash = Sha256::digest(tros);
            piece_hashes.push(format!("{:x}", hash));
            let idx = piece_hashes.len() as u32 - 1;
            std::fs::write(dir_peces.join(format!("{idx}")), tros).map_err(|e| e.to_string())?;
        }
        let piece_fi = piece_hashes.len() as u32 - 1;
        files.push(FitxerManifest {
            path: rel.to_string_lossy().replace('\\', "/"),
            piece_ini,
            piece_fi,
            mida,
        });
        total_bytes += mida;
    }

    let manifest = Manifest {
        projecte: nom,
        files,
        piece_size: protocol::PIECE_SIZE,
        piece_hashes,
        total_bytes,
    };
    Ok((manifest, dir_peces, salts))
}

/// Recull tots els fitxers del projecte, fora de les carpetes excloses.
fn recull(arrel: &Path, dir: &Path, out: &mut Vec<(PathBuf, PathBuf)>) -> Result<(), String> {
    let rd = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in rd.flatten() {
        let path = entry.path();
        let nom = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if EXCLUSIONS.contains(&nom.as_str()) {
                continue;
            }
            recull(arrel, &path, out)?;
        } else if path.is_file() {
            if let Ok(rel) = path.strip_prefix(arrel) {
                out.push((path.clone(), rel.to_path_buf()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod proves {
    use super::*;

    /// El certificat autofirmat es pot generar, desar i tornar a carregar.
    #[test]
    fn certificat_es_genera_i_es_torna_a_carregar() {
        let dir = std::env::temp_dir().join(format!("no-orbit-grid-cert-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let c1 = certificat(&dir).unwrap();
        assert!(!c1.der.is_empty());
        let c2 = certificat(&dir).unwrap();
        // El segon cop carrega el desat: mateixa clau pública (mateix der).
        assert_eq!(c1.der, c2.der);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// El manifest aplica les exclusions i parteix els fitxers en peces.
    #[test]
    fn manifest_exclou_i_parteix() {
        let arrel = std::env::temp_dir().join(format!("no-orbit-grid-proj-{}", uuid::Uuid::new_v4()));
        let grid = std::env::temp_dir().join(format!("no-orbit-grid-data-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(arrel.join("src")).unwrap();
        std::fs::create_dir_all(arrel.join("target")).unwrap();
        std::fs::create_dir_all(&grid).unwrap();
        std::fs::write(arrel.join("src/main.rs"), "fn main() {}".repeat(2000)).unwrap();
        std::fs::write(arrel.join("target/brossa.bin"), vec![1u8; 9999]).unwrap();

        let (manifest, dir_peces, salts) = construir_manifest(&arrel, &grid).unwrap();
        assert!(salts.is_empty());
        // Només src/main.rs (target/ exclòs).
        assert_eq!(manifest.files.len(), 1);
        assert_eq!(manifest.files[0].path, "src/main.rs");
        let mida = "fn main() {}".len() as u64 * 2000;
        assert_eq!(manifest.files[0].mida, mida);
        // Peces de 256 KB: 26000 bytes → 1 peça; peces desades i hash correcte.
        assert_eq!(manifest.num_peces(), 1);
        let bytes = std::fs::read(dir_peces.join("0")).unwrap();
        let hash = Sha256::digest(&bytes);
        assert_eq!(format!("{:x}", hash), manifest.piece_hashes[0]);
        std::fs::remove_dir_all(&arrel).ok();
        std::fs::remove_dir_all(&grid).ok();
    }

    /// Un fitxer de més d'una peça queda partit amb els hashes correctes.
    #[test]
    fn manifest_parteix_fitxers_grans() {
        let arrel = std::env::temp_dir().join(format!("no-orbit-grid-gran-{}", uuid::Uuid::new_v4()));
        let grid = std::env::temp_dir().join(format!("no-orbit-grid-gran-data-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&arrel).unwrap();
        std::fs::create_dir_all(&grid).unwrap();
        let mida = protocol::PIECE_SIZE as usize * 2 + 100;
        std::fs::write(arrel.join("gran.bin"), vec![7u8; mida]).unwrap();

        let (manifest, dir_peces, _) = construir_manifest(&arrel, &grid).unwrap();
        assert_eq!(manifest.num_peces(), 3);
        assert_eq!(manifest.files[0].piece_ini, 0);
        assert_eq!(manifest.files[0].piece_fi, 2);
        for i in 0..3 {
            let bytes = std::fs::read(dir_peces.join(format!("{i}"))).unwrap();
            let hash = Sha256::digest(&bytes);
            assert_eq!(format!("{:x}", hash), manifest.piece_hashes[i as usize]);
        }
        std::fs::remove_dir_all(&arrel).ok();
        std::fs::remove_dir_all(&grid).ok();
    }
}
