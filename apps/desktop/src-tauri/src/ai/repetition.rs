//! Detecció de **bucles desbocats** en el text generat per la IA: un model
//! petit (o sense suficient RAM) pot repetir la mateixa frase sense acabar
//! mai. Lògica PURA (sense Tauri ni estat): es pot testejar sola.

/// Detecta un **bucle desbocat**: un model petit (o sense suficient RAM) pot
/// entrar a repetir la mateixa frase una i una altra sense acabar mai —la UI
/// queda a «Treballant…» indefinidament. Comprova si el tram final del text
/// generat és la repetició consecutiva d'un mateix bloc. Si és així, el
/// streaming es talla i es conserva el text obtingut fins al moment.
pub fn runaway_repetition(full: &str) -> bool {
    const MIN: usize = 24; // bloc mínim que considerem un «cicle» possible
    const MAX: usize = 240; // bloc màxim
    const REPEATS: usize = 4; // repeticions seguides que disparen el tall

    let b = full.as_bytes();
    if b.len() < MIN * REPEATS {
        return false;
    }
    // Provem mides de cicle de gran a petit; si els últims `len` bytes es
    // repeteixen `REPEATS` vegades seguides cap arrere, és un bucle.
    let max = MAX.min(b.len() / REPEATS);
    let mut len = max;
    while len >= MIN {
        if b.len() >= len * REPEATS {
            let tail = &b[b.len() - len..];
            let mut reps = 1usize;
            let mut i = b.len() - len;
            while i >= len && &b[i - len..i] == tail {
                reps += 1;
                if reps >= REPEATS {
                    return true;
                }
                i -= len;
            }
        }
        len = len.saturating_sub(1);
    }

    // Repetició en QUALSEVOL punt del text (no només al final): un model
    // petit pot reemetre el mateix fitxer 14 vegades enmig de la resposta i
    // seguir escrivint (es va veure amb temes WordPress). Comptem quantes
    // vegades apareix la mateixa finestra de 100 caràcters.
    //
    // IMPORTANT (rendiment): això s'executa a CADA tros de streaming, amb el
    // text ACUMULAT. Recalcular el hash de cada finestra des de zero fa que
    // el cost sigui O(n²) (centenars de milions d'operacions en respostes
    // llargues → CPU cremada i streaming congelat). Ara el hash és RODANT
    // (O(1) per posició) i, quan un hash coincideix TIMES vegades, es
    // VERIFIQUEN els bytes de debò (evita falsos positius per col·lisió).
    const WIN: usize = 100;
    const TIMES: usize = 5;
    if b.len() < WIN * TIMES {
        return false;
    }
    const P: u64 = 0x0000_0100_0000_01b3;
    let mut pow = 1u64;
    for _ in 0..WIN - 1 {
        pow = pow.wrapping_mul(P);
    }
    let mut h: u64 = 0;
    for &byte in &b[..WIN] {
        h = h.wrapping_mul(P).wrapping_add(byte as u64);
    }
    // hash -> posicions on l'hem vista (fins a TIMES).
    let mut counts: std::collections::HashMap<u64, Vec<usize>> =
        std::collections::HashMap::new();
    let mut i = 0usize;
    while i + WIN <= b.len() {
        let e = counts.entry(h).or_insert_with(Vec::new);
        if e.len() < TIMES {
            e.push(i);
        }
        if e.len() == TIMES {
            // Verificació real dels bytes: només és bucle si TOTES les
            // finestres són idèntiques.
            let first = e[0];
            let same = e[1..]
                .iter()
                .all(|&p| b[p..p + WIN] == b[first..first + WIN]);
            if same {
                return true;
            }
            // Col·lisió de hash: es descarta l'entrada i es continua.
            counts.remove(&h);
        }
        // Avança la finestra una posició (hash rodant).
        if i + WIN < b.len() {
            let out = b[i] as u64;
            let inn = b[i + WIN] as u64;
            h = h.wrapping_sub(out.wrapping_mul(pow));
            h = h.wrapping_mul(P).wrapping_add(inn);
        }
        i += 1;
    }
    false
}

/// resum FNV-1a de 64 bits (per comparar finestres de text ràpidament).
#[allow(dead_code)]
fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}