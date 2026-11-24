//! PROTOCOL DE LA XARXA ALTRUISTA (grid).
//!
//! Sobres JSON versionats sobre corrents QUIC/TLS: cada missatge és una línia
//! JSON delimitada per `\n` dins d'un sol stream bidireccional per connexió.
//! El missatge `Hello` obri la conversa (invitació + rol) i la resta en
//! depèn: unitats de pensament de la Jam (propietari → voluntari), resultats
//! (voluntari → propietari) i, a la Fase 4, el manifest i les peces de la
//! còpia final estil torrent.
//!
//! Límits: cap missatge pot passar de `MAX_MSG` bytes (una peça de 256 KB en
//! base64 hi cap folgada), cap prompt de `MAX_PROMPT`, cap generació de més
//! de `MAX_TOKENS`. Els voluntaris MAI executen codi: només responen prompts.

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Versió del protocol (els dos extrems l'han de coincidir).
pub const VERSIO: u32 = 1;

/// Mida màxima d'un sobre (2 MiB).
pub const MAX_MSG: usize = 2 * 1024 * 1024;

/// Mida màxima d'un prompt d'unitat (512 KB: el prompt de tasca de la Jam
/// amb el context hi cap de sobres).
pub const MAX_PROMPT: usize = 512 * 1024;

/// Sostre de tokens per unitat (el voluntari no pot gastar més).
pub const MAX_TOKENS: u32 = 4096;

/// Mida de les peces de la distribució final (Fase 4): 256 KB.
pub const PIECE_SIZE: u32 = 256 * 1024;

/// Nom del servidor QUIC (SNI del certificat autofirmat).
pub const SERVER_NAME: &str = "no-orbit-grid";

/// Una unitat de pensament: un torn de la Jam que el voluntari respon amb
/// el seu model local.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unit {
    pub id: String,
    pub system: String,
    pub prompt: String,
    /// Model suggerit (el voluntari el fa servir si el té; si no, el seu).
    pub model_hint: String,
    pub max_tokens: u32,
    /// Etiqueta per a la pantalla del voluntari («tasca 3 de 12 · títol»).
    #[serde(default)]
    pub label: String,
}

/// Un fitxer del manifest de la còpia final (Fase 4).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitxerManifest {
    /// Camí relatiu dins del projecte.
    pub path: String,
    /// Índex de la primera peça (inclosa).
    pub piece_ini: u32,
    /// Índex de l'última peça (inclosa).
    pub piece_fi: u32,
    /// Mida real del fitxer en bytes.
    pub mida: u64,
}

/// Manifest de la distribució: la llista de fitxers, les peces i els seus
/// hashes SHA-256 (verificació peça a peça, estil torrent).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub projecte: String,
    pub files: Vec<FitxerManifest>,
    pub piece_size: u32,
    /// Hash SHA-256 de cada peça, en hexadecimal.
    pub piece_hashes: Vec<String>,
    pub total_bytes: u64,
}

impl Manifest {
    /// Nombre de peces del manifest.
    pub fn num_peces(&self) -> u32 {
        self.piece_hashes.len() as u32
    }
}

/// Sobres del protocol. `Hello` sempre primer; la resta segons el rol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "m", rename_all = "snake_case")]
pub enum Msg {
    /// Voluntari → propietari: presentació amb la invitació.
    Hello {
        versio: u32,
        invite: String,
        /// «propietari» o «voluntari» (el MVP només connecta voluntaris).
        rol: String,
        /// Identitat estable del node (per als comptadors i la Fase 4).
        node_id: String,
        /// Nom de màquina del voluntari (per a la llista del propietari).
        nom: String,
    },
    /// Propietari → voluntari: invitació acceptada.
    Welcome {
        projecte: String,
    },
    /// Propietari → voluntari: invitació rebutjada (codi erroni, versió…).
    Rebutjat {
        motiu: String,
    },
    /// Propietari → voluntari: una unitat de pensament per respondre.
    Unit(Unit),
    /// Voluntari → propietari: el resultat d'una unitat. TEXT BUIT = el
    /// voluntari no pot ara (porta triple tancada): queda lliure i la
    /// unitat segueix pendent per a la resta.
    Result {
        id: String,
        text: String,
    },
    /// Propietari → voluntari: abandona aquesta unitat (un altre l'ha
    /// respost o ha caducat).
    Ack {
        id: String,
    },
    /// Propietari → voluntari: el projecte ha acabat i té dret a una còpia.
    Oferta {
        projecte: String,
    },
    /// Voluntari → propietari: demana el manifest de la còpia.
    ManifestReq,
    /// Propietari → voluntari: el manifest (només als elegibles).
    Manifest(Manifest),
    /// Voluntari → propietari: bitfield de les peces que ja té (bit i = peça i).
    PieceReq {
        bitfield: Vec<u8>,
    },
    /// Propietari → voluntari: una peça (bytes en base64).
    PieceData {
        index: u32,
        bytes: String,
    },
    /// Voluntari → propietari: còpia completa i verificada.
    Copiada {},
    /// Propietari → voluntari: la compartició s'ha aturat.
    Fi,
}

/// Lector de línies amb buffer: llegeix trossos de la xarxa i talla per `\n`.
/// Sense això, llegir byte a byte multiplicaria les crides a QUIC.
pub struct LectorLinies {
    buf: Vec<u8>,
    pos: usize,
}

impl LectorLinies {
    pub fn nou() -> Self {
        Self { buf: Vec::new(), pos: 0 }
    }

    /// Torna la línia següent (sense el `\n`). Err si el missatge passa de
    /// `MAX_MSG` o la connexió es tanca sense res a llegir.
    pub async fn linia<R: AsyncRead + Unpin>(&mut self, r: &mut R) -> std::io::Result<Vec<u8>> {
        loop {
            if let Some(i) = self.buf[self.pos..].iter().position(|b| *b == b'\n') {
                let mut line = self.buf[self.pos..self.pos + i].to_vec();
                self.pos += i + 1;
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Ok(line);
            }
            self.buf.drain(..self.pos);
            self.pos = 0;
            let mut chunk = [0u8; 8192];
            let n = r.read(&mut chunk).await?;
            if n == 0 {
                if self.buf.is_empty() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "connexió tancada",
                    ));
                }
                // EOF sense `\n` final: la resta també és un missatge.
                let rest = std::mem::take(&mut self.buf);
                self.pos = 0;
                return Ok(rest);
            }
            self.buf.extend_from_slice(&chunk[..n]);
            if self.buf.len() - self.pos > MAX_MSG {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "missatge massa gran",
                ));
            }
        }
    }
}

/// Escriu un sobre com a línia JSON.
pub async fn envia<S: AsyncWrite + Unpin>(s: &mut S, m: &Msg) -> Result<(), String> {
    let mut buf = serde_json::to_vec(m).map_err(|e| format!("sobre invàlid: {e}"))?;
    if buf.len() > MAX_MSG {
        return Err("sobre massa gran".into());
    }
    buf.push(b'\n');
    s.write_all(&buf).await.map_err(|e| format!("escriptura: {e}"))?;
    Ok(())
}

/// Llegeix una línia i la desa com a sobre.
pub async fn rep<R: AsyncRead + Unpin>(
    r: &mut R,
    lector: &mut LectorLinies,
) -> Result<Msg, String> {
    let line = lector.linia(r).await.map_err(|e| e.to_string())?;
    serde_json::from_slice(&line).map_err(|e| format!("sobre mal format: {e}"))
}

/// Genera un codi d'invitació nou: 8 caràcters sense ambigüitats (ni 0/O
/// ni 1/I/l) perquè es pugui dictar i escriure a mà.
pub fn genera_codi() -> String {
    // uuid v4 dona 32 hex; en triem 8 passats per l'alfabet segur.
    let hex = uuid::Uuid::new_v4().simple().to_string();
    const ALFABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
    let mut out = String::with_capacity(8);
    for (i, c) in hex.bytes().enumerate().take(8) {
        let idx = ((c as usize) + i * 7) % ALFABET.len();
        out.push(ALFABET[idx] as char);
    }
    out
}

/// Analitza una invitació «noorbit-grid://CODI@IP:PORT» → (codi, adreça).
pub fn parse_invitacio(inv: &str) -> Result<(String, String), String> {
    let resta = inv
        .trim()
        .strip_prefix("noorbit-grid://")
        .ok_or_else(|| "la invitació ha de començar per «noorbit-grid://»".to_string())?;
    let (codi, addr) = resta
        .split_once('@')
        .ok_or_else(|| "la invitació no té «@» (codi@ip:port)".to_string())?;
    let codi = codi.trim().to_uppercase();
    if codi.is_empty() {
        return Err("la invitació no té codi".into());
    }
    let (ip, port) = addr
        .rsplit_once(':')
        .ok_or_else(|| "l'adreça no té port (ip:port)".to_string())?;
    let _port_num: u16 = port
        .trim()
        .parse()
        .map_err(|_| format!("port «{port}» invàlid"))?;
    if ip.trim().is_empty() {
        return Err("l'adreça no té ip".into());
    }
    Ok((codi, format!("{}:{}", ip.trim(), port.trim())))
}

#[cfg(test)]
mod proves {
    use super::*;

    #[test]
    fn generacio_de_codis_unics_i_segurs() {
        let mut vistos = std::collections::HashSet::new();
        for _ in 0..200 {
            let c = genera_codi();
            assert_eq!(c.len(), 8, "codi {c}");
            assert!(c.chars().all(|x| x.is_ascii_uppercase() || x.is_ascii_digit()));
            assert!(!c.contains(['0', 'O', '1', 'I', 'L']), "codi ambigu {c}");
            assert!(vistos.insert(c.clone()), "codi repetit {c}");
        }
    }

    #[test]
    fn parseig_d_invitacions() {
        let (codi, addr) = parse_invitacio("noorbit-grid://AB23CD45@192.168.1.7:38441").unwrap();
        assert_eq!(codi, "AB23CD45");
        assert_eq!(addr, "192.168.1.7:38441");
        // Minúscules al codi: es normalitza.
        let (codi, _) = parse_invitacio("noorbit-grid://ab23cd45@10.0.0.2:1").unwrap();
        assert_eq!(codi, "AB23CD45");
        assert!(parse_invitacio("http://x@1.2.3.4:5").is_err());
        assert!(parse_invitacio("noorbit-grid://x@1.2.3.4").is_err());
        assert!(parse_invitacio("noorbit-grid://x@1.2.3.4:abc").is_err());
    }

    #[test]
    fn sobres_es_serialitzen_amb_etiqueta() {
        let u = Unit {
            id: "u1".into(),
            system: "sys".into(),
            prompt: "p".into(),
            model_hint: "m".into(),
            max_tokens: 10,
            label: "tasca 3 de 12".into(),
        };
        let s = serde_json::to_string(&Msg::Unit(u.clone())).unwrap();
        assert!(s.contains("\"m\":\"unit\""));
        assert!(s.contains("\"label\":\"tasca 3 de 12\""));
        let tornat: Msg = serde_json::from_str(&s).unwrap();
        match tornat {
            Msg::Unit(x) => assert_eq!(x.id, u.id),
            _ => panic!("hauria de ser una Unit"),
        }
    }

    #[test]
    fn manifest_serialitza_i_compta_peces() {
        let m = Manifest {
            projecte: "joc".into(),
            files: vec![],
            piece_size: PIECE_SIZE,
            piece_hashes: vec!["ab".into(), "cd".into()],
            total_bytes: 100,
        };
        let s = serde_json::to_string(&Msg::Manifest(m.clone())).unwrap();
        assert!(s.contains("\"m\":\"manifest\""));
        assert_eq!(m.num_peces(), 2);
    }

    /// El lector de línies ha de tallar bé encara que la xarxa parteixi els
    /// missatges per on vulgui (duplex amb capacitat petita = trossos curts).
    #[tokio::test]
    async fn lector_de_linies_talla_per_nova_linia() {
        let (mut a, mut b) = tokio::io::duplex(64);
        // Dos missatges, escrits en trossos minúsculs per forçar fronteres.
        let escriptor = tokio::spawn(async move {
            let trossos = ["a", "b", "\n", "c", "d", "\n"];
            for t in trossos {
                a.write_all(t.as_bytes()).await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        });
        let mut lector = LectorLinies::nou();
        let l1 = lector.linia(&mut b).await.unwrap();
        let l2 = lector.linia(&mut b).await.unwrap();
        assert_eq!(l1, b"ab");
        assert_eq!(l2, b"cd");
        escriptor.await.unwrap();
    }

    /// Un missatge més gran que MAX_MSG sense nova línia ha de petar net.
    #[tokio::test]
    async fn lector_rebutja_missatges_massa_grans() {
        let (mut a, mut b) = tokio::io::duplex(1 << 20);
        let escriptor = tokio::spawn(async move {
            let buf = vec![b'x'; MAX_MSG + 1];
            // Duplex d'1 MiB < 2 MiB: escriure-ho tot blocaria fins que
            // algú llegeixi; en trossos no.
            for chunk in buf.chunks(1 << 16) {
                a.write_all(chunk).await.unwrap();
            }
        });
        let mut lector = LectorLinies::nou();
        let r = lector.linia(&mut b).await;
        assert!(r.is_err(), "hauria de rebutjar el missatge gegant");
        escriptor.await.unwrap();
    }

    /// `envia` + `rep` fan el viatge complet d'un sobre.
    #[tokio::test]
    async fn envia_i_rep_sobre() {
        let (mut a, mut b) = tokio::io::duplex(1 << 20);
        let msg = Msg::Welcome {
            projecte: "el meu joc".into(),
        };
        envia(&mut a, &msg).await.unwrap();
        let mut lector = LectorLinies::nou();
        let tornat = rep(&mut b, &mut lector).await.unwrap();
        match tornat {
            Msg::Welcome { projecte } => assert_eq!(projecte, "el meu joc"),
            _ => panic!("hauria de ser un Welcome"),
        }
    }
}
