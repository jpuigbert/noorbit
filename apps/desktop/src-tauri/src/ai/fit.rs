//! PREDICCIÓ D'ENCAIX: abans de baixar o carregar un model, NoOrbit diu si
//! HI CAP a la RAM d'aquesta màquina — i, si no ho pot saber, PER QUÈ no.
//!
//! La idea ve de `soup doctor` i del *hardware fit* de Soup (Apache-2.0): un
//! entrenament o una descàrrega només s'han de començar quan l'equip hi cap,
//! i la predicció ha de dir la veritat, inclosa quan la dada que falta és el
//! motiu que no es pot comprovar res.
//!
//! Criteri ÚNIC: tot el programa usa aquest mòdul (`WaitCtx`, el gestor de
//! models, Hugging Bay i la descàrrega d'Ollama). Abans cada lloc feia la
//! seva aritmètica i els missatges es contradeien.
//!
//! Com estimem-ho (tot aproximacions DOCUMENTADES, no promeses):
//! * El model ha de cabre a la RAM **total** amb marge: NoOrbit ja fa servir
//!   el 60 % del total com a sostre (`best_fit_model`); el mateix sostre és
//!   el que decideix ací.
//! * A més del pes del fitxer, cal RAM per a la **cua de claus** (KV cache)
//!   del context: creix amb els tokens i amb la mida del model.
//! * Si la RAM **lliure** és menor que el que cal, el model hi cap al
//!   teòric però ara mateix no: cal alliberar models carregats primer.

use super::fmt_gb;
use serde::Serialize;

/// Sostre d'ús raonable: un model (més el seu context) no hauria de passar
/// del 60 % de la RAM total de la màquina. Mateix criteri que `best_fit_model`
/// i que la nota d'espera del xat, perquè l'usuari no senta dues xifres.
pub const FRACCIO_SOSTRE: f64 = 0.6;
/// Per damunt del 80 % del sostre el model hi cap, però anirà lent: avís.
pub const FRACCIO_JUST: f64 = 0.8;
/// Marge de seguretat per a la RAM que el sistema i NoOrbit necessiten mentre
/// un model genera (si la RAM lliure és justeta, no és culpa del model).
const MARGE_SISTEMA: u64 = 512 * 1024 * 1024;
/// Context (tokens) que NoOrbit sol demanar als models locals. Per estimar la
/// cua de claus no serveix la finestra màxima del model: allò que es carrega
/// és el que s'usa realment.
pub const CONTEXT_ESTIMAT: u64 = 8192;

/// Veredicte de l'encaix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdicte {
    /// Hi cap còmodament dins el sostre.
    Cap,
    /// Hi cap, però sense marge (o sense RAM lliure ara mateix).
    Just,
    /// No hi cap: NoOrbit ho diu ABANS de baixar o carregar res.
    NoCap,
    /// No es pot comprovar: falta una dada (mida del model o RAM de la
    /// màquina). Es diu QUÈ falta, no es fingeix un veredicte.
    Desconegut,
}

/// Resultat complet de la comprovació: el veredicte i els números que el
/// fonamenten, perquè la interfície els puga mostrar (no només «no hi cap»).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Encaix {
    pub verdicte: Verdicte,
    /// Mida del model al disc (bytes) o `None` si el mirror no la dona.
    pub mida: Option<u64>,
    pub ram_total: Option<u64>,
    /// RAM lliure en fer la comprovació (`None` si el SO no la reporta).
    pub ram_lliure: Option<u64>,
    /// El que necessitaria: pes del model + cua de claus del context.
    pub cal: u64,
    /// Sostre aplicat (bytes) segons la RAM total.
    pub sostre: u64,
    /// Per què aquest veredicte — o què falta per saber-ho.
    pub motiu: String,
    /// Què fer: alliberar RAM, triar una quantització més petita, un model
    /// més petit o fer servir el núvol. `None` quan hi cap bé.
    pub sortida: Option<String>,
}

impl Encaix {
    /// Cert si es pot continuar (hi cap o hi cap just). `NoCap` i
    /// `Desconegut` fan aturar una descàrrega.
    pub fn acceptable(&self) -> bool {
        matches!(self.verdicte, Verdicte::Cap | Verdicte::Just)
    }

    /// Línia curta per a la interfície (xips, llista de models).
    pub fn etiqueta(&self) -> String {
        match self.verdicte {
            Verdicte::Cap => "hi cap".to_string(),
            Verdicte::Just => "just d'RAM".to_string(),
            Verdicte::NoCap => "NO hi cap".to_string(),
            Verdicte::Desconegut => "encaix sense comprovar".to_string(),
        }
    }

    /// Text llarg per a un diàleg o una descàrrega refusada: què passa, els
    /// números reals i què fer.
    pub fn missatge(&self) -> String {
        let mut s = format!("{}: {}", self.etiqueta(), self.motiu);
        if let Some(d) = &self.sortida {
            s.push_str(&format!(" {d}"));
        }
        s
    }

    /// Serialitzat per a la interfície (els xips de la UI no fan `Debug`).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::Value::Null)
    }
}

/// Bytes de cua de claus (KV cache) per token, segons la mida del model.
/// Aproximació per escales: un 0.6B ocupa ~=16 kB per token, un 8B Q4 ~=128 kB
/// i un MoE gran més. És una xifra orientativa, però és la que fa que un model
/// de 8k de context costi REALMENT un GB més de RAM.
pub fn kv_bytes_per_token(mida: Option<u64>) -> u64 {
    const MB: u64 = 1024 * 1024;
    match mida {
        Some(s) if s < 700 * MB => 16 * MB / 1024, // 16 kB
        Some(s) if s < 1_600 * MB => 32 * MB / 1024,
        Some(s) if s < 2_800 * MB => 64 * MB / 1024,
        Some(s) if s < 6_000 * MB => 96 * MB / 1024,
        Some(_) => 128 * MB / 1024,
        // Sense mida: supòsit conservador de model mitjà (96 kB/token).
        None => 96 * MB / 1024,
    }
}

/// Bytes que necessita un model d'aquest pes amb aquest context.
pub fn cal_necessari(mida: Option<u64>, context: u64) -> u64 {
    let model = mida.unwrap_or_default();
    let cua = kv_bytes_per_token(mida).saturating_mul(context.max(1));
    model.saturating_add(cua)
}

/// Comprova l'encaix amb dades que el cridador ja té (per a tests i per a
/// quan la RAM lliure ve d'un altre lloc).
pub fn comprova(
    mida: Option<u64>,
    ram_total: Option<u64>,
    ram_lliure: Option<u64>,
    context: u64,
) -> Encaix {
    let context = if context == 0 { CONTEXT_ESTIMAT } else { context };
    let cal = cal_necessari(mida, context);
    let sostre = ram_total
        .map(|r| (r as f64 * FRACCIO_SOSTRE) as u64)
        .unwrap_or(0);

    // Falta alguna de les dues dades: NO fingim un veredicte (això és el que
    // Soup fa al «pre-flight»: dir per què no s'ha pogut comprovar).
    let (verdicte, motiu, sortida) = match (mida, ram_total) {
        (None, _) => (
            Verdicte::Desconegut,
            format!(
                "la font no reporta la mida d'aquest model (se n'assumeix un context de {} \
                 tokens), així que no es pot comprovar si hi cap",
                context
            ),
            Some(
                "Baixa'l igualment si saps que el teu equip hi cap, o tria un model que sí que \
                 reporta la mida.".to_string(),
            ),
        ),
        (Some(_), None) => (
            Verdicte::Desconegut,
            "no es pot llegir la RAM total d'aquest equip, així que no hi ha sostre de compare".to_string(),
            Some("Compara'l amb la RAM de la màquina abans de baixar-lo.".to_string()),
        ),
        (Some(s), Some(r)) => {
            let just_lim = (sostre as f64 * FRACCIO_JUST) as u64;
            let lliure_real = ram_lliure.map(|f| f.saturating_sub(MARGE_SISTEMA));
            if cal > sostre {
                let pot = (sostre.saturating_sub(cal_necessari(None, context)) as f64 / 1e9)
                    .max(0.0);
                (
                    Verdicte::NoCap,
                    format!(
                        "{} de model + ~{} de context = {} i l'equip en té {} de RAM (sostre \
                         {}): NO hi cap",
                        fmt_gb(s),
                        fmt_gb(cal.saturating_sub(s)),
                        fmt_gb(cal),
                        fmt_gb(r),
                        fmt_gb(sostre)
                    ),
                    Some(format!(
                        "Tria'n un que no passe de ~{} GB (o una quantització més petita, \
                         p. ex. Q4_K_M); amb un proveïdor del núvol no cal baixar-ne res.",
                        format!("{:.1}", pot)
                    )),
                )
            } else if cal > just_lim {
                (
                    Verdicte::Just,
                    format!(
                        "{} (comptant-hi el context) sobre un sostre de {}: hi cap, però sense \
                         marge",
                        fmt_gb(cal),
                        fmt_gb(sostre)
                    ),
                    Some(
                        "Anirà lent: tanca aplicacions grans i allibera els models carregats \
                         (IA ▸ Models ▸ «Allibera RAM»)."
                            .to_string(),
                    ),
                )
            } else if let Some(ll) = lliure_real {
                if cal > ll {
                    (
                        Verdicte::Just,
                        format!(
                            "hi cap al teòric ({} de sostre), però ara mateix només hi ha {} \
                             lliures",
                            fmt_gb(sostre),
                            fmt_gb(ll)
                        ),
                        Some(
                            "Allibera la RAM dels models carregats (IA ▸ Models ▸ «Allibera \
                             RAM») abans de fer-lo servir."
                                .to_string(),
                        ),
                    )
                } else {
                    (
                        Verdicte::Cap,
                        format!("{} necessaris (model + context) en un sostre de {}", fmt_gb(cal), fmt_gb(sostre)),
                        None,
                    )
                }
            } else {
                (
                    Verdicte::Cap,
                    format!("{} necessaris (model + context) en un sostre de {}", fmt_gb(cal), fmt_gb(sostre)),
                    None,
                )
            }
        }
    };

    Encaix {
        verdicte,
        mida,
        ram_total,
        ram_lliure,
        cal,
        sostre,
        motiu,
        sortida,
    }
}

/// Comprova l'encaix d'un model llegint la RAM REAL d'aquesta màquina.
pub fn comprova_aqui(mida: Option<u64>, context: u64) -> Encaix {
    comprova(mida, super::wander::ram_total(), super::wander::ram_disponible(), context)
}

/// RAM total del sistema (bytes) — embolcall per al cridador, que així no
/// obliga a cap altre mòdul a dependre de `wander`.
pub fn ram_total_aqui() -> Option<u64> {
    super::wander::ram_total()
}

/// RAM lliure del sistema (bytes, amb la inactiva recuperable). `None` en els
/// sistemes on no es pot llegir: aleshores l'encaix només usa el sostre.
pub fn ram_lliure_aqui() -> Option<u64> {
    super::wander::ram_disponible()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;
    const GIB: u64 = 1024 * 1024 * 1024;

    /// Un Mac Intel de 8 GB amb un 8B de 5,2 GB: NO hi cap (model + context
    /// passen del sostre de 4,8 GB). És el cas que abans es descobria després
    /// de la descàrrega i de cinc minuts d'espera.
    #[test]
    fn vuit_gigabytes_no_hi_cap_un_de_vuit_b() {
        let e = comprova(Some(5_200_000_000), Some(8 * GIB), None, 8192);
        assert_eq!(e.verdicte, Verdicte::NoCap);
        assert!(!e.acceptable());
        assert!(e.motiu.contains("NO hi cap"), "{}", e.motiu);
        assert!(e.missatge().contains("Q4_K_M") || e.missatge().contains("GB"));
    }

    /// El model inicial d'un equip de 8 GB ha de donar «hi cap»: si tot
    /// semblés impossible, l'usuari nou no tindria per on començar.
    #[test]
    fn el_model_inicial_dels_vuit_gigabytes_hi_cap() {
        let e = comprova(Some(2_600_000_000), Some(8 * GIB), None, 8192);
        assert_eq!(e.verdicte, Verdicte::Cap, "{}", e.motiu);
        assert!(e.acceptable());
        assert!(e.sortida.is_none());
    }

    /// Per damunt del 80 % del sostre: hi cap, però ho diu com un avís.
    #[test]
    fn a_prop_del_sostre_es_just() {
        let e = comprova(Some(4_000_000_000), Some(8 * GIB), None, 8192);
        assert_eq!(e.verdicte, Verdicte::Just, "{}", e.motiu);
        assert!(e.acceptable());
    }

    /// Sense la mida del model NO s'inventa un veredicte: diu què falta.
    #[test]
    fn sense_mida_no_fingeix_res() {
        let e = comprova(None, Some(8 * GIB), None, 8192);
        assert_eq!(e.verdicte, Verdicte::Desconegut);
        assert!(!e.acceptable());
        assert!(e.motiu.contains("no es pot comprovar"), "{}", e.motiu);
    }

    /// Sense RAM total igualment: «Desconegut», no «Cap».
    #[test]
    fn sense_ram_no_fingeix_res() {
        let e = comprova(Some(1_900_000_000), None, None, 8192);
        assert_eq!(e.verdicte, Verdicte::Desconegut);
    }

    /// La RAM lliure mana ara mateix: un model que cabria, amb la memòria
    /// ocupada per un altre, s'ha de marcar com a «just» i dir com solutionar.
    #[test]
    fn amb_lliure_poc_es_just() {
        let e = comprova(
            Some(2_600_000_000),
            Some(8 * GIB),
            Some(1_500_000_000),
            8192,
        );
        assert_eq!(e.verdicte, Verdicte::Just, "{}", e.motiu);
        assert!(e.motiu.contains("lliures"), "{}", e.motiu);
        assert!(e.missatge().contains("Allibera"));
    }

    /// La cua de claus mai no és zero: un model de context llarg costa més
    /// RAM que el seu pes al disc.
    #[test]
    fn el_context_suma_ram() {
        let curt = cal_necessari(Some(2_600_000_000), 1);
        let llarg = cal_necessari(Some(2_600_000_000), 32768);
        assert!(llarg > curt + GB, "{} vs {}", curt, llarg);
    }

    #[test]
    fn un_equip_gran_hi_cap_de_molt_b() {
        let e = comprova(Some(5_200_000_000), Some(16 * GIB), None, 8192);
        assert_eq!(e.verdicte, Verdicte::Cap, "{}", e.motiu);
    }
}
