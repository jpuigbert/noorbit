//! Preus per milió de tokens i estimació del COST REAL d'un torn.
//!
//! El compte de tokens que arriba del proveïdor NO és sempre el que es paga:
//! els tokens servits des de la memòria cau del proveïdor (el «prompt
//! caching») es consumeixen igual, però es paguen a una fracció del preu
//! (≈10 %). Aquest mòdul tradueix el compte en diners perquè la interfície
//! pugui dir quant ha costat DE VERDAT cada resposta i no només quant ha
//! ocupat el context.
//!
//! Els preus són els de llista habituals, en USD per milió de tokens, i són
//! orientatius (els proveïdors els canvien). Per això viuen tots en aquest
//! fitxer: és l'únic lloc a tocar quan un model canvia de preu.

use super::Usage;

/// Preu d'un model (USD per MILió de tokens).
#[derive(Debug, Clone, Copy)]
pub struct Prices {
    /// Tokens d'entrada pagats al preu sencer.
    pub input: f64,
    /// Tokens generats.
    pub output: f64,
    /// Tokens servits des de la memòria cau (una fracció del preu d'entrada).
    pub cache_read: f64,
    /// Tokens escrits a la memòria cau (normalment 1,25× el preu d'entrada).
    pub cache_write: f64,
}

/// Preus coneguts. L'ORDRE IMPORTA: es busca el primer que coincideixi amb el
/// nom del model, així que els noms més concrets han d'anar ABANS dels
/// generals («gpt-5-mini» abans de «gpt-5»).
const TAULA: &[(&str, Prices)] = &[
    // ── OpenAI ────────────────────────────────────────────────────────────
    ("gpt-5-nano", preu(0.05, 0.40, 0.005, 0.0)),
    ("gpt-5-mini", preu(0.25, 2.00, 0.025, 0.0)),
    ("gpt-5", preu(1.25, 10.00, 0.125, 0.0)),
    ("gpt-4.1-nano", preu(0.10, 0.40, 0.025, 0.0)),
    ("gpt-4.1-mini", preu(0.40, 1.60, 0.10, 0.0)),
    ("gpt-4.1", preu(2.00, 8.00, 0.50, 0.0)),
    ("gpt-4o-mini", preu(0.15, 0.60, 0.075, 0.0)),
    ("gpt-4o", preu(2.50, 10.00, 1.25, 0.0)),
    ("o4-mini", preu(1.10, 4.40, 0.275, 0.0)),
    ("o3-mini", preu(1.10, 4.40, 0.55, 0.0)),
    ("o3", preu(2.00, 8.00, 0.50, 0.0)),
    // ── Anthropic (Claude) ────────────────────────────────────────────────
    ("claude-opus-4", preu(15.00, 75.00, 1.50, 18.75)),
    ("claude-sonnet-4", preu(3.00, 15.00, 0.30, 3.75)),
    ("claude-3-7-sonnet", preu(3.00, 15.00, 0.30, 3.75)),
    ("claude-haiku-4", preu(1.00, 5.00, 0.10, 1.25)),
    ("claude-3-5-haiku", preu(0.80, 4.00, 0.08, 1.00)),
    ("claude-3-5-sonnet", preu(3.00, 15.00, 0.30, 3.75)),
    // ── DeepSeek ──────────────────────────────────────────────────────────
    ("deepseek-reasoner", preu(0.55, 2.19, 0.14, 0.0)),
    ("deepseek-chat", preu(0.27, 1.10, 0.07, 0.0)),
    // ── Google (Gemini) ────────────────────────────────────────────────────
    ("gemini-2.5-pro", preu(1.25, 10.00, 0.31, 0.0)),
    ("gemini-2.5-flash", preu(0.30, 2.50, 0.075, 0.0)),
];

/// Abreviatura per construir un [`Prices`] sense repetir el nom del camp.
const fn preu(input: f64, output: f64, cache_read: f64, cache_write: f64) -> Prices {
    Prices {
        input,
        output,
        cache_read,
        cache_write,
    }
}

/// Preus d'un model, si el coneixem. Es busca pel nom COMPLET del model
/// (pot ser «gpt-5-mini-2025-08-07» o «claude-sonnet-4-5-20250929»).
pub fn prices_for(model: &str) -> Option<Prices> {
    let m = model.to_ascii_lowercase();
    TAULA.iter().find(|(k, _)| m.contains(k)).map(|(_, p)| *p)
}

/// Cost estimat (USD) d'un torn amb aquests preus. Els tokens de la memòria cau
/// es cobren al seu preu (reduït), no al preu sencer d'entrada: aquest és
/// precisament el punt que fa que «s'ha consumit» i «s'ha pagat» no siguin
/// el mateix número.
pub fn estimate_cost(prices: &Prices, usage: &Usage) -> f64 {
    const M: f64 = 1_000_000.0;
    usage.paid_input() as f64 / M * prices.input
        + usage.cache_write as f64 / M * prices.cache_write
        + usage.cache_read as f64 / M * prices.cache_read
        + usage.output as f64 / M * prices.output
}

/// Cost estimat (USD) del torn amb el model indicat. `None` si no es coneix el
/// model: millor no ensenyar cap xifra que pugui ser falsa.
pub fn cost_for(model: &str, usage: &Usage) -> Option<f64> {
    prices_for(model).map(|p| estimate_cost(&p, usage))
}

#[cfg(test)]
mod proves {
    use super::*;

    fn u(input: u64, output: u64, read: u64, write: u64) -> Usage {
        Usage {
            input,
            output,
            cache_read: read,
            cache_write: write,
        }
    }

    #[test]
    fn el_model_mes_concret_guanya_al_general() {
        // «gpt-5-mini» no ha de caure a la fila de «gpt-5».
        let mini = prices_for("gpt-5-mini-2025-08-07").unwrap();
        assert!((mini.input - 0.25).abs() < f64::EPSILON);
        let base = prices_for("gpt-5").unwrap();
        assert!((base.input - 1.25).abs() < f64::EPSILON);
    }

    #[test]
    fn model_desconegut_no_inventa_preu() {
        assert!(prices_for("el-meu-model-local").is_none());
        assert!(cost_for("el-meu-model-local", &u(10, 10, 0, 0)).is_none());
    }

    #[test]
    fn la_memoria_cau_costa_menys_que_entrar_a_ple() {
        let p = prices_for("claude-sonnet-4-5").unwrap();
        // Mateixa entrada, una vegada normal i una vegada des de la memòria cau.
        let ple = estimate_cost(&p, &u(1000, 100, 0, 0));
        let amb_cau = estimate_cost(&p, &u(1000, 100, 1000, 0));
        assert!(amb_cau < ple);
        // El que es paga de veritat són els 100 tokens que NO venen de la cau.
        let esperat = 100.0 / 1_000_000.0 * p.input + 100.0 / 1_000_000.0 * p.output;
        assert!((amb_cau - esperat).abs() < 1e-12);
    }

    #[test]
    fn el_cost_es_zero_si_no_hi_ha_tokens() {
        let p = prices_for("gpt-4o").unwrap();
        assert!(estimate_cost(&p, &u(0, 0, 0, 0)).abs() < f64::EPSILON);
    }
}
