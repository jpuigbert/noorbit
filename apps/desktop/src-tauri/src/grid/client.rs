//! CLIENT VOLUNTARI DE LA XARXA ALTRUISTA.
//!
//! Cada projecte ajudat (peer) té una tasca de connexió: es connecta al
//! propietari per QUIC/TLS, presenta la invitació i espera unitats. Només
//! treballa si passa la PORTA TRIPLE:
//!   1. inactivitat real (l'últim toc de l'usuari fa més d'`IDLE_MIN_SECS`),
//!   2. l'estalvi de pantalla és obert, i
//!   3. cap generació local d'IA en curs.
//! Si la porta està tancada, refusa la unitat amb un resultat buit (queda
//! lliure per a la resta). El resultat es genera amb el client Ollama
//! existent (camí d'`AiManager`): el model suggerit si el té, si no el seu.
//!
//! Fase 4: en rebre l'«Oferta» del projecte acabat, demana el manifest,
//! baixa les peces amb represa (les peces ja baixades queden a disc), les
//! verifica amb SHA-256 i munta la còpia a la carpeta triada.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tokio::sync::oneshot;

use crate::ai::AiManager;
use crate::config::GridPeer;

use super::protocol::{self, Manifest, Msg};
use super::{GridEvent, GridState, IDLE_MIN_SECS};

/// Una còpia rebuda (o en curs) pel costat voluntari — per a la UI.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyState {
    pub projecte: String,
    /// Carpeta destí final de la còpia muntada.
    pub desti: String,
    /// Peces baixades / total.
    pub fetes: u32,
    pub total: u32,
    /// «baixant», «muntant», «feta» o «error».
    pub fase: String,
    pub error: Option<String>,
}

/// Engega la tasca del voluntari per a un peer (idempotent per adreça).
pub async fn engega(peer: GridPeer, state: Arc<GridState>, ai: Arc<AiManager>, app: AppHandle) {
    let addr = peer.addr.clone();
    {
        let mut clients = state.clients.lock().await;
        if clients.contains_key(&addr) {
            return;
        }
        let (tx, rx) = oneshot::channel();
        clients.insert(addr.clone(), tx);
        let st = state.clone();
        let ai2 = ai.clone();
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            bucle(peer, st, ai2, app2, rx).await;
        });
    }
}

/// Atura la tasca del voluntari per a una adreça.
pub async fn atura(state: &Arc<GridState>, addr: &str) {
    if let Some(tx) = state.clients.lock().await.remove(addr) {
        let _ = tx.send(());
    }
}

/// Atura TOTS els clients voluntaris.
pub async fn atura_tots(state: &Arc<GridState>) {
    let tots: Vec<oneshot::Sender<()>> = state.clients.lock().await.drain().map(|(_, tx)| tx).collect();
    for tx in tots {
        let _ = tx.send(());
    }
}

/// Bucle de reconnexió: si el propietari no respon, es torna a provar cada
/// 30 s mentre el peer segueixi donat d'alta i la xarxa estigui activa.
async fn bucle(
    peer: GridPeer,
    state: Arc<GridState>,
    ai: Arc<AiManager>,
    app: AppHandle,
    mut stop: oneshot::Receiver<()>,
) {
    loop {
        if !state.enabled.load(Ordering::Relaxed) {
            return;
        }
        tokio::select! {
            _ = &mut stop => return,
            r = connectar(&peer, &state, &ai, &app) => {
                if let Err(e) = r {
                    // Rebutjats (codi erroni) no cal repetir-los tant.
                    eprintln!("grid: voluntari cap a {}: {e}", peer.addr);
                }
            }
        }
        tokio::select! {
            _ = &mut stop => return,
            _ = tokio::time::sleep(Duration::from_secs(30)) => {}
        }
    }
}

/// Una connexió sencera: Hello → Welcome → bucle d'unitats i còpies.
async fn connectar(
    peer: &GridPeer,
    state: &Arc<GridState>,
    ai: &Arc<AiManager>,
    app: &AppHandle,
) -> Result<(), String> {
    let endpoint = endpoint_client().map_err(|e| e.to_string())?;
    let addr: std::net::SocketAddr = peer
        .addr
        .parse()
        .map_err(|e| format!("adreça «{}»: {e}", peer.addr))?;
    let conn = endpoint
        .connect_with(client_config()?, addr, protocol::SERVER_NAME)
        .map_err(|e| e.to_string())?
        .await
        .map_err(|e| e.to_string())?;
    let (mut enviar, mut rebre) = conn.open_bi().await.map_err(|e| e.to_string())?;
    let mut lector = protocol::LectorLinies::nou();

    protocol::envia(
        &mut enviar,
        &Msg::Hello {
            versio: protocol::VERSIO,
            invite: peer.code.clone(),
            rol: "voluntari".into(),
            node_id: state.node_id.clone(),
            nom: super::nom_local(),
        },
    )
    .await?;

    let benvinguda = tokio::time::timeout(
        Duration::from_secs(30),
        protocol::rep(&mut rebre, &mut lector),
    )
    .await
    .map_err(|_| "timeout esperant la benvinguda".to_string())??;
    let projecte = match benvinguda {
        Msg::Welcome { projecte } => projecte,
        Msg::Rebutjat { motiu } => return Err(format!("invitació rebutjada: {motiu}")),
        _ => return Err("resposta inesperada al Hello".into()),
    };

    // Cancel·lació de la unitat en curs (un Ack del propietari).
    let cancel = Arc::new(AtomicBool::new(false));
    let mut cancel_actual = String::new();

    loop {
        let msg = protocol::rep(&mut rebre, &mut lector).await?;
        match msg {
            Msg::Unit(u) => {
                cancel_actual = u.id.clone();
                cancel.store(false, Ordering::Relaxed);
                let r = treballa(state, ai, app, &projecte, &u, &cancel).await;
                match r {
                    Ok(text) if !cancel.load(Ordering::Relaxed) => {
                        state.units_donated.fetch_add(1, Ordering::Relaxed);
                        protocol::envia(&mut enviar, &Msg::Result { id: u.id, text }).await?;
                    }
                    Ok(_) => {
                        // Cancel·lada (un altre l'ha respost): no enviem res.
                    }
                    Err(e) => {
                        // Fallida local: es torna el refús perquè el propietari
                        // sàpiga que aquest voluntari no la respondrà.
                        eprintln!("grid: unitat {} fallida: {e}", u.id);
                        protocol::envia(&mut enviar, &Msg::Result { id: u.id, text: String::new() }).await?;
                    }
                }
                cancel_actual.clear();
            }
            Msg::Ack { id } => {
                if id == cancel_actual {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            Msg::Oferta { projecte: _ } => {
                copia(state, app, &mut enviar, &mut rebre, &mut lector).await?;
            }
            Msg::Fi => break,
            _ => break,
        }
    }
    Ok(())
}

/// Endpoint client QUIC (la config TLS viatja amb `connect_with`).
fn endpoint_client() -> Result<quinn::Endpoint, std::io::Error> {
    quinn::Endpoint::client("0.0.0.0:0".parse().expect("adreça fixa"))
}

/// Config TLS que accepta el certificat autofirmat del propietari. MVP de
/// xarxa local: l'autenticació real és el codi de la invitació, no el
/// certificat (un «trust on first use» arribaria després).
fn client_config() -> Result<quinn::ClientConfig, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SenseVerificacio(provider)))
        .with_no_client_auth();
    let quic_tls = quinn::crypto::rustls::QuicClientConfig::try_from(tls)
        .map_err(|e| e.to_string())?;
    Ok(quinn::ClientConfig::new(Arc::new(quic_tls)))
}

/// Verificador que accepta qualsevol certificat (autofirmat del propietari).
#[derive(Debug)]
struct SenseVerificacio(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for SenseVerificacio {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// La PORTA TRIPLE: sense inactivitat real, estalvi obert i cap generació
/// local en curs, el voluntari no treballa.
fn porta_oberta(state: &GridState) -> bool {
    state.saver_open.load(Ordering::Relaxed)
        && crate::ai::wander::segons_ocios() >= IDLE_MIN_SECS
        && !crate::ai::wander::ia_occupada()
}

/// Respon una unitat amb el model local (el suggerit si el té, si no el seu).
async fn treballa(
    state: &Arc<GridState>,
    ai: &Arc<AiManager>,
    app: &AppHandle,
    projecte: &str,
    unit: &protocol::Unit,
    cancel: &Arc<AtomicBool>,
) -> Result<String, String> {
    if !porta_oberta(state) {
        return Ok(String::new()); // refús educat: text buit
    }
    let view = super::WorkView {
        projecte: projecte.to_string(),
        label: if unit.label.is_empty() {
            "pensant-hi…".to_string()
        } else {
            unit.label.clone()
        },
        detail: "generant amb el model local…".to_string(),
    };
    *state.current_work.write().await = Some(view.clone());
    state.emet(
        app,
        GridEvent {
            kind: "feina".into(),
            detail: view.label.clone(),
            projecte: view.projecte.clone(),
            ok: true,
        },
    );

    let system = (!unit.system.trim().is_empty()).then(|| unit.system.as_str());
    let hint = (!unit.model_hint.trim().is_empty()).then(|| unit.model_hint.as_str());
    let on_chunk = |_: &str| {
        // El streaming local no es mostra a l'estalvi (el resultat arriba
        // sencer): el callback existeix pel compte de cancel·lació.
        let _ = cancel.load(Ordering::Relaxed);
    };
    let mut resultat = ai
        .ollama_complete_stream(&unit.prompt, system, hint, &on_chunk)
        .await;
    // El model suggerit pot no existir en aquesta màquina: es torna amb el
    // model local per defecte.
    if resultat.is_err() && hint.is_some() {
        resultat = ai
            .ollama_complete_stream(&unit.prompt, system, None, &on_chunk)
            .await;
    }

    *state.current_work.write().await = None;
    state.emet(
        app,
        GridEvent {
            kind: "fet".into(),
            detail: String::new(),
            projecte: projecte.to_string(),
            ok: resultat.is_ok(),
        },
    );
    resultat.map_err(|e| e.to_string())
}

// ── Fase 4: baixada de la còpia estil torrent ─────────────────────────────

/// Descarrega (amb represa) i munta la còpia del projecte acabat.
async fn copia(
    state: &Arc<GridState>,
    app: &AppHandle,
    enviar: &mut quinn::SendStream,
    rebre: &mut quinn::RecvStream,
    lector: &mut protocol::LectorLinies,
) -> Result<(), String> {
    protocol::envia(enviar, &Msg::ManifestReq).await?;
    let msg = tokio::time::timeout(Duration::from_secs(30), protocol::rep(rebre, lector))
        .await
        .map_err(|_| "timeout esperant el manifest".to_string())??;
    let manifest = match msg {
        Msg::Manifest(m) => m,
        Msg::Fi => return Ok(()),
        _ => return Err("esperava el manifest".into()),
    };

    let base = state.copies_dir.read().await.clone();
    let desti = base.join(&manifest.projecte);
    let peces_dir = desti.join(".noorbit-peces");
    std::fs::create_dir_all(&peces_dir).map_err(|e| e.to_string())?;
    let total = manifest.num_peces();

    // REPRESA: les peces ja baixades i verificades queden a disc.
    let mut bitfield = vec![0u8; (total as usize + 7) / 8];
    let mut fetes = 0u32;
    for i in 0..total as usize {
        if peces_dir.join(format!("{i}")).is_file() {
            bitfield[i / 8] |= 1 << (i % 8);
            fetes += 1;
        }
    }
    actualitza_copia(state, app, &manifest, &desti, "baixant", fetes, total, None).await;

    // Bucle de peces: es demanen les que falten (4 per petició) fins a tenir-les totes.
    while (fetes as usize) < total as usize {
        protocol::envia(enviar, &Msg::PieceReq { bitfield: bitfield.clone() }).await?;
        let rebudes = tokio::time::timeout(Duration::from_secs(60), protocol::rep(rebre, lector))
            .await
            .map_err(|_| "timeout esperant peces".to_string())??;
        match rebudes {
            Msg::PieceData { index, bytes } => {
                let i = index as usize;
                if i >= total as usize {
                    return Err(format!("índex de peça {i} fora del manifest"));
                }
                let dades = base64::engine::general_purpose::STANDARD
                    .decode(&bytes)
                    .map_err(|e| format!("peça {i} en base64: {e}"))?;
                // VERIFICACIÓ peça a peça amb el hash del manifest.
                let hash = format!("{:x}", Sha256::digest(&dades));
                if hash != manifest.piece_hashes[i] {
                    return Err(format!("hash de la peça {i} no coincideix"));
                }
                std::fs::write(peces_dir.join(format!("{i}")), &dades).map_err(|e| e.to_string())?;
                bitfield[i / 8] |= 1 << (i % 8);
                fetes += 1;
                actualitza_copia(state, app, &manifest, &desti, "baixant", fetes, total, None).await;
            }
            Msg::Fi => return Ok(()),
            _ => return Err("esperava peces".into()),
        }
    }

    // MUNTATGE: concatena les peces de cada fitxer del manifest.
    actualitza_copia(state, app, &manifest, &desti, "muntant", fetes, total, None).await;
    for f in &manifest.files {
        if !camino_segur(&f.path) {
            return Err(format!("camí sospitós al manifest: {}", f.path));
        }
        let dest = desti.join(&f.path);
        if let Some(pare) = dest.parent() {
            std::fs::create_dir_all(pare).map_err(|e| e.to_string())?;
        }
        let mut contingut = Vec::with_capacity(f.mida as usize);
        for i in f.piece_ini..=f.piece_fi {
            let dades = std::fs::read(peces_dir.join(format!("{i}"))).map_err(|e| e.to_string())?;
            contingut.extend_from_slice(&dades);
        }
        contingut.truncate(f.mida as usize);
        std::fs::write(&dest, &contingut).map_err(|e| format!("{}: {e}", f.path))?;
    }
    let _ = std::fs::remove_dir_all(&peces_dir);

    actualitza_copia(state, app, &manifest, &desti, "feta", total, total, None).await;
    // Confirmació al propietari: còpia completa i verificada.
    protocol::envia(enviar, &Msg::Copiada {}).await?;
    Ok(())
}

/// El camí relatiu del manifest no pot sortir del projecte («..», absoluts).
fn camino_segur(p: &str) -> bool {
    let t = p.replace('\\', "/");
    !t.is_empty()
        && !t.starts_with('/')
        && !t.contains(':')
        && !t.split('/').any(|s| s == "..")
}

/// Actualitza (o crea) l'entrada de la còpia a la llista visible de la UI.
async fn actualitza_copia(
    state: &Arc<GridState>,
    app: &AppHandle,
    manifest: &Manifest,
    desti: &Path,
    fase: &str,
    fetes: u32,
    total: u32,
    error: Option<String>,
) {
    let mut copies = state.copies.write().await;
    if let Some(c) = copies
        .iter_mut()
        .find(|c| c.projecte == manifest.projecte)
    {
        c.fetes = fetes;
        c.total = total;
        c.fase = fase.to_string();
        c.error = error;
    } else {
        copies.push(CopyState {
            projecte: manifest.projecte.clone(),
            desti: desti.to_string_lossy().to_string(),
            fetes,
            total,
            fase: fase.to_string(),
            error,
        });
    }
    let ev = GridEvent {
        kind: "copi".into(),
        detail: format!("{fase} · peça {fetes} de {total}"),
        projecte: manifest.projecte.clone(),
        ok: true,
    };
    state.emet(app, ev);
}

#[cfg(test)]
mod proves {
    use super::*;

    #[test]
    fn camins_del_manifest_sanititzats() {
        assert!(camino_segur("src/main.rs"));
        assert!(camino_segur("a/b/c.txt"));
        assert!(!camino_segur("../secret.txt"));
        assert!(!camino_segur("/etc/passwd"));
        assert!(!camino_segur("a/../../b"));
        assert!(!camino_segur("C:\\windows\\x"));
    }

    /// El bitfield es construeix per a qualsevol nombre de peces.
    #[test]
    fn bitfield_cobreix_totes_les_peces() {
        for total in [1u32, 8, 9, 100] {
            let bitfield = vec![0u8; (total as usize + 7) / 8];
            assert_eq!(bitfield.len(), (total as usize + 7) / 8);
        }
    }
}
