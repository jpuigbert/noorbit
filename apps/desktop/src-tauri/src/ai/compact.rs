//! COMPACTACIÓ Densa del context: en lloc de RETALLAR (i perdre l'esperit del
//! programa), es CONDENSA. Quan la memòria no dona per tot el context, NoOrbit
//! no es queda sense feina: condensa el fil en un «fitxer de memòria» que
//! conserva l'OBJECTIU, el PLA, l'ESQUELET DEL CODI (funcions, classes,
//! signatures), les rutes de fitxers i l'estat de les tasques, i la IA CONTINUA
//! programant on s'havia quedat, a trossos.
//!
//! Tot aquí és DETERMINISTA (cap crida al model): la compactació ha de poder
//! fer-se JUSTAMENT quan el model ja no hi cab — demanar-li un resum seria
//! demanar-li el que ja no pot processar.

use super::HistoryTurn;

/// Continuació a trossos: la IA no torna a començar, acaba la feina.
pub const CONTINUA_A_TROÇOS: &str = "CONTINUA on t'has quedat: NO tornis a planificar ni \
a explicar des de zero. Fes el següent TROS de la feina (un pas o un fitxer) i, \
si encara queda pendent, digue clarament QUÈ queda per fer. Conserva el \
criteri i l'estil del codi que ja has escrit.";

/// Presupost de caràcters per secció (perquè la memòria compactada cap).
const OBJECTIU_MAX: usize = 1_400;
const FETS_MAX: usize = 2_600;
const ESQUELET_MAX: usize = 2_000;
const RECENT_TURNS: usize = 3;
const RECENT_MAX: usize = 800;
const TASCA_MAX: usize = 2_000;

/// Una línia que és part del PLA o d'una DECISIÓ (es conserva textualment).
fn es_fet_o_decisio(linea: &str) -> bool {
    let l = linea.trim();
    if l.is_empty() || l.chars().count() > 240 {
        return false;
    }
    // Llistes i passos numerats: el PLA.
    if l.starts_with("- ")
        || l.starts_with("* ")
        || l.starts_with("• ")
        || l.starts_with("‣ ")
        || l.starts_with("> ")
    {
        return true;
    }
    let caps = l.chars().take(3);
    let numerat = caps.clone().all(|c| c.is_ascii_digit()) && l.contains(". ");
    if numerat {
        return true;
    }
    // Directrius de tasques: l'estat real de la feina.
    let low = l.to_lowercase();
    if low.contains("@todo:") || low.contains("@done:") || low.contains("@doing:") {
        return true;
    }
    // Rutes de fitxer: on viu el programa.
    if l.contains("/src/")
        || l.contains("./")
        || l.contains(".tsx")
        || l.contains(".ts:")
        || l.contains(".rs:")
        || l.contains(".py:")
        || l.contains(".json:")
        || l.contains(".css:")
        || l.contains(".md:")
        || l.contains(".html:")
    {
        return true;
    }
    // Decisions i errors: el que NO es pot perdre.
    const DECISIONS: [&str; 14] = [
        "creat", "creada", "creat:", "modificat", "desat", "decidim", "decidit", "ara el",
        "cal ", "ha de", "error", "fall", "pendent", "to-do",
    ];
    DECISIONS.iter().any(|k| low.contains(k))
}

/// Una línia que és ESQUELET (declaració de funció, classe, tipus…): la
/// «forma» del programa sense el cos. Això és el que conserva l'esperit del
/// codi quan el context es condensa.
fn es_esquelet(linea: &str) -> bool {
    let l = linea.trim();
    if l.is_empty() || l.chars().count() > 160 {
        return false;
    }
    const DECL: [&str; 14] = [
        "fn ", "fn(", "class ", "def ", "struct ", "enum ", "impl ", "trait ", "interface ",
        "function ", "const ", "pub ", "type ", "export ",
    ];
    if DECL.iter().any(|k| l.starts_with(k)) {
        return true;
    }
    // Línies d'obertura d'un bloc de codi: la declaració sense el cos.
    l.ends_with("{") || l.ends_with(") {") || l.ends_with("=> {")
}

/// Text de la TASCA actual: el que hi ha després de «PREGUNTA / TASCA:» (o el
/// text sencer si no hi ha marcador).
pub fn extract_task(prompt: &str) -> String {
    let t = match prompt.find("PREGUNTA / TASCA:") {
        Some(pos) => prompt[pos + "PREGUNTA / TASCA:".len()..].trim(),
        None => prompt.trim(),
    };
    t.chars().take(TASCA_MAX).collect()
}

/// CONDENSA el fil de la conversa + la tasca en un «fitxer de memòria» que
/// conserva l'esperit del programa (objectiu, pla, esquelet del codi, rutes i
/// tasques) i permet a la IA continuar la programació a trossos.
pub fn dense_compact(history: &[HistoryTurn], task: &str) -> String {
    // 1) OBJECTIU: el primer missatge de l'usuari (què estem construint).
    let objectiu = history
        .iter()
        .find(|t| t.role == "user")
        .map(|t| t.content.trim())
        .unwrap_or("")
        .chars()
        .take(OBJECTIU_MAX)
        .collect::<String>();

    // 2) FETS I DECISIONS + 3) ESQUELET DEL CODI: tot el fil, triant el que
    //    importa i sense repetir (el model no torni a llegir el mateix).
    let mut fets: Vec<String> = Vec::new();
    let mut esquelet: Vec<String> = Vec::new();
    let mut vistes: Vec<String> = Vec::new();
    for t in history {
        for raw in t.content.lines() {
            let l = raw.trim();
            if l.is_empty() {
                continue;
            }
            if es_esquelet(l) {
                if !vistes.iter().any(|v| v == l) && esquelet.iter().count() < 120 {
                    esquelet.push(l.to_string());
                    vistes.push(l.to_string());
                }
                continue;
            }
            if es_fet_o_decisio(l) {
                if !vistes.iter().any(|v| v == l) && fets.iter().count() < 160 {
                    fets.push(l.to_string());
                    vistes.push(l.to_string());
                }
            }
        }
    }
    let fets = join_amb_pressupost(fets, FETS_MAX);
    let esquelet = join_amb_pressupost(esquelet, ESQUELET_MAX);

    // 4) ELS DARRERS TURNS: el context immediat, textualment.
    let mut recents: Vec<String> = Vec::new();
    for t in history.iter().rev().take(RECENT_TURNS).rev() {
        let qui = if t.role == "user" { "TU" } else { "IA" };
        let txt: String = t.content.trim().chars().take(RECENT_MAX).collect();
        if !txt.is_empty() {
            recents.push(format!("{}: {}", qui, txt));
        }
    }

    let mut out = String::new();
    out.push_str("MEMÒRIA DE TREBALL (COMPACTADA per NoOrbit: la memòria no donava per tot el context. ");
    out.push_str("Això NO és una retallada: s'ha conservat l'objectiu, el pla i l'esquelet del codi)\n");
    if !objectiu.is_empty() {
        out.push_str("\nOBJECTIU (què hem de fer):\n");
        out.push_str(&objectiu);
        out.push('\n');
    }
    if !fets.is_empty() {
        out.push_str("\nFETS, DECISIONS I RUTES (fins aquí):\n");
        out.push_str(&fets);
        out.push('\n');
    }
    if !esquelet.is_empty() {
        out.push_str("\nESQUELET DEL CÓDI (l'estructura que ja has escrit — conserva-la i estén-la):\n");
        out.push_str(&esquelet);
        out.push('\n');
    }
    if !recents.is_empty() {
        out.push_str("\nÚLTIMS TURNS (el fil immediat):\n");
        out.push_str(&recents.join("\n"));
        out.push('\n');
    }
    out.push_str("\nTASCA ACTUAL:\n");
    out.push_str(task.trim());
    out.push('\n');
    out.push('\n');
    out.push_str(CONTINUA_A_TROÇOS);
    out
}

/// Uneix les línies conservant l'ordre i respectant el pressupost: si s'excedeix,
/// talla pel final (les primeres són les més recents per ordre invers? no: es
/// mantenen les primeres, que són les primeres decisions).
fn join_amb_pressupost(linies: Vec<String>, max: usize) -> String {
    let mut out = String::new();
    let mut gastat = 0usize;
    for l in linies {
        if gastat + l.chars().count() + 1 > max {
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&l);
        gastat += l.chars().count() + 1;
    }
    out
}

#[cfg(test)]
mod proves {
    use super::*;

    fn torn(role: &str, content: &str) -> HistoryTurn {
        HistoryTurn {
            role: role.to_string(),
            content: content.to_string(),
        }
    }

    /// La compactació ha de CONSERVAR l'esperit del programa: objectiu, pla,
    /// esquelet del codi, rutes i tasques — i no ha de ser una retallada.
    #[test]
    fn conserva_objectiu_pla_i_esquelet() {
        let hist = vec![
            torn(
                "user",
                "Crea una botiga de WordPress a Untena basada en Gosets i un tema original.",
            ),
            torn(
                "assistant",
                "- Configurar l'entorn de WordPress\n- Instal·lar els plugins necessaris\n\
                 - Fitxer: src/config/woocommerce.ts\n\
                 ```ts\nexport function crearBotiga() {\n  return true;\n}\n```\n\
                 @todo: Dissenyar i personalitzar la botiga\n\
                 Error: la connexió a la base de dades ha fallat",
            ),
            torn("assistant", "He creat el fitxer src/config/woocommerce.ts"),
        ];
        let out = dense_compact(&hist, "Afegeix el pagament amb Stripe");
        assert!(out.contains("OBJECTIU"), "falta l'objectiu: {}", out);
        assert!(out.contains("WordPress"), "s'ha perdut l'objectiu");
        assert!(out.contains("Configurar l'entorn"), "s'ha perdut el pla");
        assert!(
            out.contains("export function crearBotiga"),
            "s'ha perdut l'esquelet del codi: {}",
            out
        );
        assert!(out.contains("src/config/woocommerce.ts"), "s'ha perdut la ruta");
        assert!(out.contains("@todo:"), "s'ha perdut l'estat de les tasques");
        assert!(out.contains("Afegeix el pagament amb Stripe"), "falta la tasca");
        assert!(out.contains("CONTINUA"), "falta la continuació a trossos");
    }

    /// El pressupost s'ha de respectar: la memòria compactada no pot tornar a
    /// ser tan gran com el fil original.
    #[test]
    fn el_compactat_es_molt_mes_breu() {
        let gran = "x".repeat(50_000);
        let hist = vec![torn("user", &gran), torn("assistant", &gran)];
        let out = dense_compact(&hist, "fes-ho");
        assert!(
            out.chars().count() < 12_000,
            "la memòria compactada és massa gran: {} caràcters",
            out.chars().count()
        );
    }
}
