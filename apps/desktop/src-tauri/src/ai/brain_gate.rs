//! PORTA D'AVALUACIÓ DEL CERVELL NATIU: abans de deixar que la IA nativa
//! aprengui res, es COMPROVA si allò nou empitjora les respostes que
//! l'usuari ja havia acceptat; si les empitjora, ES RESTAURA el cervell.
//!
//! Ve del `training.eval_gate` de Soup (Apache-2.0), que defineix exactament
//! aquest contracte: `regression_threshold` (0,05 de pèrdua màxima permesa) i
//! `on_regression: stop`. Nosaltres apliquem el mateix esperit al cervell
//! simbòlic de NoOrbit: una fusió de coneixement NO és un do automàtic, és
//! una hipòtesi que s'ha de defensar davant les proves.
//!
//! Les proves són les respostes que l'usuari va votar amb 👍 (mòdul
//! `learning`). Si no n'hi ha cap, la porta ho DIU en lloc de donar-ho per
//! bo: «no avaluada: sense proves». Aquesta honestedat és la part que més
//! val del disseny de Soup, no pas l'aritmètica.
//!
//! Com ho mesurem sense cap IA: cada prova té una resposta de referència. La
//! resposta del cervell es considera encertada si comparteix prou paraules
//! significatives amb la referència (coeficient de Dice sobre els mateixos
//! `tokens()` que fa servir l'aprenentatge). Comparar ABANS contra DESPRÉS
//! amb la mateixa métrica és el que importa: la métrica absoluta seria
//! injusta amb un motor simbòlic que respon curt.

use super::native::{self, Knowledge};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Una prova: la pregunta i la resposta que l'usuari va ACCEPTAR.
#[derive(Debug, Clone)]
pub struct Prova {
    pub question: String,
    pub expected: String,
}

/// Una resposta es considera encertada a partir d'aquest solapament de
/// paraules. No és un llindar de Soup (allà és la PÈRDUA permesa); és el que
/// fem servir per dir si dues respostes parlen del mateix.
pub const LLINDAR: f32 = 0.34;
/// Pèrdua màxima permesa respecte del cervell actual: 5 punts, com
/// `training.eval_gate.regression_threshold` de Soup.
pub const REGRESSIO: f32 = 0.05;
/// Proves com a molt: el motor simbòlic és ràpid, però el registre pot tenir
/// centenars de 👍 i l'informe no ha de creixer per sempre.
pub const MAX_PROVES: usize = 32;

/// Resultat de puntuar un conjunt de proves amb UNA manera de respondre.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Puntuacio {
    pub proves: usize,
    pub encertades: usize,
    /// Fracció de proves ben contestades (0..1).
    pub nota: f32,
}

/// Què ha fet la porta. `motiu` mai no va buit: si no s'ha pogut comprovar
/// res, hi diu QUÈ falta (la regla del `soup doctor`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resultat {
    /// El cervell nou es queda?
    pub acceptat: bool,
    /// S'ha desfet el que ja estava escrit (còpia de seguretat restaurada).
    pub rollback: bool,
    /// Peces de coneixement nous acceptats (0 si s'ha fet marxa enrere).
    pub peces: usize,
    pub proves: usize,
    pub abans: Option<Puntuacio>,
    pub despres: Option<Puntuacio>,
    /// Diferència de nota (`despres - abans`), si s'ha pogut mesurar.
    pub delta: Option<f32>,
    pub motiu: String,
    /// Temps de la comprovació (per saber si l'informe és d'ara).
    pub at: i64,
}

impl Resultat {
    /// Una línia per a la interfície o per al diari del vagueig.
    pub fn missatge(&self) -> String {
        let mesura = match (self.abans, self.despres) {
            (Some(a), Some(d)) => format!(
                "proves {} → {}: {} encertades abans, {} després",
                self.proves,
                self.proves,
                a.encertades,
                d.encertades
            ),
            _ => format!("{} proves disponibles", self.proves),
        };
        if self.rollback {
            format!("Cervell restaurat ({}). {}", mesura, self.motiu)
        } else if self.abans.is_none() || self.despres.is_none() {
            format!("Coneixement acceptat sense avaluar ({}). {}", mesura, self.motiu)
        } else {
            format!("Coneixement acceptat ({}). {}", mesura, self.motiu)
        }
    }
}

/// Paraules significatives compartides (Dice): quant diuen el mateix dos
/// textos. Sense paraules buides ni accents (els mateixos `tokens` que
/// fa servir l'aprenentatge, perquè la porta i el context no comparin de
/// dues maneres diferents).
pub fn solapament(a: &str, b: &str) -> f32 {
    let x: HashSet<String> = crate::learning::tokens(a).into_iter().collect();
    let y: HashSet<String> = crate::learning::tokens(b).into_iter().collect();
    if x.is_empty() || y.is_empty() {
        return 0.0;
    }
    let comuns = x.intersection(&y).count() as f32;
    2.0 * comuns / (x.len() as f32 + y.len() as f32)
}

/// Puntuar un joc de proves amb qualsevol manera de respondre (en tests
/// s'hi passa una funció falsa; a la vida real, el cervell carregat).
/// Sense proves no hi ha nota: `None`, no un 0 que sembli un fracàs.
pub fn puntuar<F>(proves: &[Prova], respondre: F) -> Option<Puntuacio>
where
    F: Fn(&str) -> Option<String>,
{
    if proves.is_empty() {
        return None;
    }
    let mut encertades = 0usize;
    for p in proves {
        let nota = respondre(&p.question)
            .map(|r| solapament(&r, &p.expected))
            .unwrap_or(0.0);
        if nota >= LLINDAR {
            encertades += 1;
        }
    }
    Some(Puntuacio {
        proves: proves.len(),
        encertades,
        nota: encertades as f32 / proves.len() as f32,
    })
}

/// La decisió de la porta, separada del disc per poder-ne fer tests.
/// Torna (acceptar, fer marxa enrere, motiu).
pub fn decide(
    abans: Option<Puntuacio>,
    despres: Option<Puntuacio>,
    peces: usize,
) -> (bool, bool, String) {
    match (abans, despres) {
        (None, _) => (
            true,
            false,
            "no avaluada: al registre no hi ha cap resposta acceptada amb 👍 i, sense \
             proves, la porta no pot comparar el cervell vell amb el nou."
                .to_string(),
        ),
        (_, None) => (
            true,
            false,
            format!(
                "no avaluada després de fusionar ({peces} peces nous): les proves han \
                 desaparegut del registre entre les dues mesures."
            ),
        ),
        (Some(a), Some(d)) => {
            if peces == 0 {
                return (
                    true,
                    false,
                    format!(
                        "res nou que afegir ({} proves, {} ja encertades).",
                        a.proves, a.encertades
                    ),
                );
            }
            if d.nota + REGRESSIO < a.nota {
                return (
                    false,
                    true,
                    format!(
                        "regressió: de {} encertades a {} de {} (ha baixat més del {} \
                         permes): el coneixement nou empitjora respostes que ja havies \
                         aprovat, així que es desfà.",
                        a.encertades,
                        d.encertades,
                        a.proves,
                        format!("{:.0}%", REGRESSIO * 100.0)
                    ),
                );
            }
            if d.nota > a.nota {
                return (
                    true,
                    false,
                    format!(
                        "millora: de {} encertades a {} de {}.",
                        a.encertades, d.encertades, a.proves
                    ),
                );
            }
            (
                true,
                false,
                format!(
                    "empata ({} de {}): el que s'ha afegit no toca les proves; es queda, \
                     perquè no fa perdre res.",
                    a.encertades, a.proves
                ),
            )
        }
    }
}

/// Proves des del registre d'aprenentatge compartit. Buit si encara no hi ha
/// cap magatzem o cap 👍: aleshores la porta ho dirà, no inventarà un 100%.
pub fn proves_des_del_registre() -> Vec<Prova> {
    let Some(store) = crate::learning::shared() else {
        return Vec::new();
    };
    store
        .accepted(MAX_PROVES)
        .into_iter()
        .map(|e| Prova {
            question: e.question,
            expected: e.answer,
        })
        .collect()
}

/// Ruta del cervell après (el mateix fitxer que llegeix `native::load_brain`).
fn brain_file() -> Option<std::path::PathBuf> {
    crate::config::AppConfig::data_dir().map(|d| d.join("native_brain.json"))
}

/// Fusiona amb porta: mesura abans, escriu, torna a mesurar i, si baixa més
/// del permès, RESTAURA la còpia de seguretat. Silenciós com `merge_knowledge`
/// (no emet events); qui vulgui el veredicte, el llegeix al `Resultat` o a
/// `darrer()` (l'últim veredicte, per a la interfície).
pub fn fusiona_amb_porta(incoming: Vec<Knowledge>) -> Resultat {
    let proves = proves_des_del_registre();
    let abans_brain = native::load_brain();
    let abans = puntuar(&proves, |q| {
        native::answer_or_none(q, &abans_brain, "")
    });

    // Còpia de seguretat ABANS d'escriure: si la mesura posterior diu que
    // anàvem pitjor, es pot tornar exactament al que hi havia.
    let backup = brain_file().and_then(|p| std::fs::read(&p).ok());
    let peces = native::merge_knowledge(incoming);
    let despres_brain = native::load_brain();
    let despres = puntuar(&proves, |q| {
        native::answer_or_none(q, &despres_brain, "")
    });

    let (acceptat, rollback, motiu) = decide(abans, despres, peces);
    let mut peces_finals = peces;
    if rollback {
        // Es desfà la fusió tornant al contingut anterior (o esborrant el
        // fitxer si abans no n'hi havia cap).
        if let Some(path) = brain_file() {
            match backup {
                Some(raw) => {
                    let tmp = path.with_extension("json.roll");
                    if std::fs::write(&tmp, raw)
                        .and_then(|_| std::fs::rename(&tmp, &path))
                        .is_err()
                    {
                        eprintln!("NoOrbit/porta: no s'ha pogut restaurar el cervell.");
                    }
                }
                None => {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        peces_finals = 0;
    }
    let delta = match (abans, despres) {
        (Some(a), Some(d)) => Some(d.nota - a.nota),
        _ => None,
    };
    let r = Resultat {
        acceptat,
        rollback,
        peces: peces_finals,
        proves: proves.len(),
        abans,
        despres,
        delta,
        motiu,
        at: chrono::Local::now().timestamp(),
    };
    if let Ok(mut g) = darrer_mut() {
        *g = Some(r.clone());
    }
    r
}

/// Últim veredicte de la porta (per a la interfície: «què ha après la
/// nativa i com ha anat la comprovació»). `None` si encara no s'ha fet cap
/// fusió.
pub fn darrer() -> Option<Resultat> {
    darrer_mut().ok().and_then(|g| g.clone())
}

fn darrer_mut() -> std::sync::LockResult<std::sync::MutexGuard<'static, Option<Resultat>>> {
    static DARRER: OnceLock<Mutex<Option<Resultat>>> = OnceLock::new();
    let c = DARRER.get_or_init(|| Mutex::new(None));
    c.lock()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prova(q: &str, r: &str) -> Prova {
        Prova {
            question: q.to_string(),
            expected: r.to_string(),
        }
    }

    fn nota(proves: usize, encertades: usize) -> Puntuacio {
        Puntuacio {
            proves,
            encertades,
            nota: encertades as f32 / proves as f32,
        }
    }

    #[test]
    fn el_solapament_mesura_si_diuen_el_mateix() {
        // El mateix text, amb majúscules i accents diferents, és un 1.
        assert!((solapament("El bosc està tapat", "el bosc esta tapat") - 1.0).abs() < 1e-6);
        // Dues frases del mateix tema: prou paraules comunes per considerar-
        // ho encertat, encara que l'ordre sigui altre.
        let q = solapament("la capital de frança es paris", "paris es la capital de frança");
        assert!(q > LLINDAR, "{q}");
        // I dues que no hi tenen res a veure.
        assert!(solapament("tancar la finestra del navegador", "la capital de frança es paris") < LLINDAR);
        // Text buit: zero, no una divisió per zero ni un 1 enganyós.
        assert_eq!(solapament("", "res"), 0.0);
    }

    #[test]
    fn sense_proves_no_hi_ha_nota_i_no_es_fingit_un_zero() {
        // El «no inventis números» de Soup: absència de dades ≠ fracàs.
        assert!(puntuar(&[], |_| Some("algo".to_string())).is_none());
        let proves = [prova("quant es 2+2", "quatre")];
        let p = puntuar(&proves, |_| Some("cuatro".to_string())).unwrap();
        assert_eq!((p.proves, p.encertades), (1, 0));
        // Un motor que respon `None` tampoc no encerta.
        assert_eq!(puntuar(&proves, |_| None).unwrap().encertades, 0);
    }

    #[test]
    fn la_porta_desfa_el_que_empitjora_i_es_queda_amb_el_que_milla() {
        // Regressió per damunt del llindar → marxa enrere.
        let (acceptat, rollback, motiu) = decide(Some(nota(10, 8)), Some(nota(10, 4)), 3);
        assert!(!acceptat && rollback);
        assert!(motiu.contains("regressió"), "{motiu}");
        // Baixa petita (dins del 5% de pèrduda permesa) → es queda.
        let (a, r, _) = decide(Some(nota(100, 90)), Some(nota(100, 88)), 1);
        assert!(a && !r);
        // Millora → es queda i ho diu.
        let (a, r, motiu) = decide(Some(nota(10, 4)), Some(nota(10, 7)), 2);
        assert!(a && !r && motiu.contains("millora"), "{motiu}");
        // Empata → es queda, perquè no fa perdre res.
        let (a, r, motiu) = decide(Some(nota(10, 6)), Some(nota(10, 6)), 2);
        assert!(a && !r && motiu.contains("empata"), "{motiu}");
    }

    #[test]
    fn la_porta_diu_que_falta_quant_no_pot_comprovar_res() {
        // Cap 👍 al registre: no es pot avaluar, i la porta ho escriu.
        let (acceptat, rollback, motiu) = decide(None, None, 5);
        assert!(acceptat && !rollback);
        assert!(motiu.contains("no avaluada") && motiu.contains("👍"), "{motiu}");
        // Les proves han desaparegut entre les dues mesures.
        let (_, _, motiu) = decide(Some(nota(4, 2)), None, 1);
        assert!(motiu.contains("desaparegut"), "{motiu}");
        // Res nou que afegir: tampoc no cal dramatitzar.
        let (acceptat, rollback, motiu) = decide(Some(nota(4, 2)), Some(nota(4, 2)), 0);
        assert!(acceptat && !rollback && motiu.contains("res nou"), "{motiu}");
    }

    #[test]
    fn el_missatge_explica_que_sha_fet_en_una_linia() {
        let r = Resultat {
            acceptat: false,
            rollback: true,
            peces: 0,
            proves: 6,
            abans: Some(nota(6, 5)),
            despres: Some(nota(6, 1)),
            delta: Some(-0.6667),
            motiu: "regressió".to_string(),
            at: 0,
        };
        let m = r.missatge();
        assert!(m.starts_with("Cervell restaurat"), "{m}");
        assert!(m.contains("5 encertades abans, 1 després"), "{m}");

        let sense_mesura = Resultat {
            acceptat: true,
            rollback: false,
            peces: 3,
            proves: 0,
            abans: None,
            despres: None,
            delta: None,
            motiu: "no avaluada: sense proves".to_string(),
            at: 0,
        };
        assert!(sense_mesura.missatge().contains("0 proves disponibles"));
    }

    #[test]
    fn les_proves_vinen_del_registre_i_no_inventen_res() {
        // Sense magatzem compartit (o sense cap 👍) la porta treballa amb
        // buit, mai amb un 100% inventat.
        let proves = proves_des_del_registre();
        assert!(proves.len() <= MAX_PROVES);
        let (acceptat, rollback, _) = decide(puntuar(&proves, |_| None), None, 1);
        if proves.is_empty() {
            assert!(acceptat && !rollback);
        }
    }
}
