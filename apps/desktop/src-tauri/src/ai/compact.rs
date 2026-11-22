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

// ── COMPACTACIÓ PER CONTEXT (Fase 2) ─────────────────────────────────────
// Quan el context ESTIMAT supera el 90% de la finestra del model, els torns
// ANTICS es substitueixen per un resum (via el mateix proveïdor, o extractiu
// local si no n'hi ha cap de disponible) i els DARRERS torns es conserven
// sencers, amb els fitxers i decisions clau. La selecció de torns és PURA.

/// Llindar d'ocupació del context a partir del qual es compacta (percentatge).
pub const COMPACT_THRESHOLD_PCT: u64 = 90;

/// Torns recents que es conserven SENCERS en compactar.
pub const KEEP_RECENT: usize = 4;

/// Estimació barata de tokens: ~4 caràcters per token (heurística habitual
/// per a text barrejat català/codi). No és el compte real del proveïdor, però
/// és prou bona per decidir si cal compactar ABANS d'enviar la petició.
pub fn estimate_tokens(text: &str) -> u64 {
    (text.chars().count() as u64 + 3) / 4
}

/// Context estimat d'un torn: sistema + historial + prompt actual.
pub fn context_used(history: &[HistoryTurn], prompt: &str, system: Option<&str>) -> u64 {
    let mut total = estimate_tokens(prompt) + estimate_tokens(system.unwrap_or(""));
    for t in history {
        total += estimate_tokens(&t.content) + 4; // rol + embolcall del missatge
    }
    total
}

/// Cal compactar? Quan l'ús estimat arriba al 90% de la finestra del model.
/// Amb finestra desconeguda (0) no es compacta mai per aquest criteri.
pub fn needs_compaction(used: u64, limit: u64) -> bool {
    limit > 0 && used * 100 >= limit * COMPACT_THRESHOLD_PCT
}

/// Resum EXTRACTIU (determinista, sense model) dels torns antics: conserva
/// l'objectiu, els fets/decisions, les rutes de fitxer i l'esquelet del codi.
/// És la reserva quan no hi ha cap proveïdor que pugui resumir.
pub fn extractive_summary(old: &[HistoryTurn]) -> String {
    let objectiu: String = old
        .iter()
        .find(|t| t.role == "user")
        .map(|t| t.content.trim())
        .unwrap_or("")
        .chars()
        .take(800)
        .collect();
    let mut fets: Vec<String> = Vec::new();
    let mut esquelet: Vec<String> = Vec::new();
    let mut vistes: Vec<String> = Vec::new();
    for t in old {
        for raw in t.content.lines() {
            let l = raw.trim();
            if l.is_empty() || vistes.iter().any(|v| v == l) {
                continue;
            }
            if es_esquelet(l) && esquelet.len() < 60 {
                esquelet.push(l.to_string());
                vistes.push(l.to_string());
            } else if es_fet_o_decisio(l) && fets.len() < 120 {
                fets.push(l.to_string());
                vistes.push(l.to_string());
            }
        }
    }
    let mut out = String::from(
        "RESUM DE LA SESSIÓ (compactació automàtica de NoOrbit: el context \
         superava el 90% de la finestra del model; els torns antics s'han \
         condensat i els darrers es conserven sencers).\n",
    );
    if !objectiu.is_empty() {
        out.push_str("\nOBJECTIU:\n");
        out.push_str(&objectiu);
        out.push('\n');
    }
    let fets = join_amb_pressupost(fets, 2_400);
    if !fets.is_empty() {
        out.push_str("\nFETS, DECISIONS I RUTES:\n");
        out.push_str(&fets);
        out.push('\n');
    }
    let esquelet = join_amb_pressupost(esquelet, 1_200);
    if !esquelet.is_empty() {
        out.push_str("\nESQUELET DEL CODI:\n");
        out.push_str(&esquelet);
        out.push('\n');
    }
    out
}

/// Substitueix els torns ANTICS per un torn de resum, conservant els
/// `KEEP_RECENT` darrers SENCERS. Si el resum és un torn «user» i el primer
/// dels recents també, es fusionen (algunes APIs exigeixen rols alternats).
/// Amb pocs torns (≤ KEEP_RECENT) no es toca res.
pub fn compact_history(history: &[HistoryTurn], summary: &str) -> Vec<HistoryTurn> {
    if history.len() <= KEEP_RECENT || summary.trim().is_empty() {
        return history.to_vec();
    }
    let recents = &history[history.len() - KEEP_RECENT..];
    let mut out: Vec<HistoryTurn> = Vec::new();
    if recents[0].role == "user" {
        out.push(HistoryTurn {
            role: "user".into(),
            content: format!("{}\n\n{}", summary.trim(), recents[0].content),
        });
        out.extend_from_slice(&recents[1..]);
    } else {
        out.push(HistoryTurn {
            role: "user".into(),
            content: summary.trim().to_string(),
        });
        out.extend_from_slice(recents);
    }
    out
}

/// System prompt del resumidor: el resum ha de conservar el fil de treball.
const SUMMARY_SYSTEM: &str = "Ets el resumidor de sessions de NoOrbit. Resumeix \
EN CATALÀ la sessió de treball que se t'adjunta: conserva l'OBJECTIU, les \
DECISIONS preses, les RUTES de fitxers tocats, l'estat de les tasques i els \
errors pendents. Màxim ~400 paraules. No inventes res que no sigui al text.";

/// Compacta la sessió SI CAL (context estimat ≥ 90% de `limit`). El resum es
/// demana al MATEIX proveïdor del xat (sessió aïllada, amb timeout); si no
/// n'hi ha cap de disponible o falla, s'usa el resum extractiu local. En
/// compactar, emet «ai://compacted» perquè el xat mostri la nota.
/// Retorna `true` si s'ha compactat.
pub async fn maybe_compact(
    app: &tauri::AppHandle,
    ai: &std::sync::Arc<super::AiManager>,
    sid: &str,
    opts: &mut super::ChatOpts,
    prompt: &str,
    system: Option<&str>,
    limit: u64,
) -> bool {
    let used = context_used(&opts.history, prompt, system);
    if !needs_compaction(used, limit) || opts.history.len() <= KEEP_RECENT {
        return false;
    }
    let (old, _) = opts.history.split_at(opts.history.len() - KEEP_RECENT);
    // Proveïdor capaç de resumir: remot amb clau, o Ollama en marxa. La
    // nativa integrada (0.5B) no resumeix: el resum extractiu és millor.
    let (provider, _) = ai.session_labels(sid).await;
    let pot_resumir = if provider == "ollama" {
        ai.is_running().await
    } else {
        provider != super::native::PROVIDER_ID
    };
    let mut summary = String::new();
    if pot_resumir {
        let old_text = old
            .iter()
            .map(|t| format!("{}: {}", t.role, t.content))
            .collect::<Vec<_>>()
            .join("\n\n");
        let old_text: String = old_text.chars().take(24_000).collect();
        let demana = format!("Resumeix aquesta sessió:\n\n{}", old_text);
        let opts_sum = super::ChatOpts {
            provider: Some(provider.clone()),
            ..Default::default()
        };
        if let Ok(Ok(text)) = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            super::in_session(
                &format!("compact_{}", sid),
                ai.chat_opts(&demana, Some(SUMMARY_SYSTEM), &opts_sum),
            ),
        )
        .await
        {
            if !text.trim().is_empty() {
                summary = text.trim().chars().take(6_000).collect();
            }
        }
    }
    if summary.is_empty() {
        summary = extractive_summary(old);
    }
    opts.history = compact_history(&opts.history, &summary);
    use tauri::Emitter;
    let _ = app.emit(
        "ai://compacted",
        serde_json::json!({ "session": sid, "note": "Sessió resumida per context" }),
    );
    true
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

    // ── Compaction per context (Fase 2) ─────────────────────────────────

    #[test]
    fn el_llindar_de_compactacio_es_el_90_percent() {
        // 89% → no; 90% → sí; finestra desconeguda → mai.
        assert!(!needs_compaction(89, 100));
        assert!(needs_compaction(90, 100));
        assert!(needs_compaction(100, 100));
        assert!(!needs_compaction(1_000_000, 0));
        // Estimació ~4 caràcters/token.
        assert_eq!(estimate_tokens("12345678"), 2);
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn la_compactacio_conserva_els_darrers_torns_sencers() {
        let hist: Vec<HistoryTurn> = (0..10)
            .map(|i| {
                torn(
                    if i % 2 == 0 { "user" } else { "assistant" },
                    &format!("torn {}", i),
                )
            })
            .collect();
        let out = compact_history(&hist, "RESUM dels antics");
        // Els 4 darrers torns hi són SENCERS i en ordre; el primer («user»)
        // porta el resum fusionat al davant (algunes APIs exigeixen rols
        // alternats) però el seu contingut es conserva íntegre.
        assert_eq!(out.len(), 4);
        let cua: Vec<&str> = out.iter().map(|t| t.content.as_str()).collect();
        assert!(cua[0].ends_with("torn 6"), "el torn 6 s'ha perdut: {}", cua[0]);
        assert_eq!(&cua[1..], &["torn 7", "torn 8", "torn 9"]);
        // El resum substitueix els antics (no queda cap «torn 0»).
        assert!(out.iter().any(|t| t.content.contains("RESUM dels antics")));
        assert!(!out.iter().any(|t| t.content == "torn 0"));
        // Històries curtes: no es toquen.
        let curta = &hist[..3];
        assert_eq!(compact_history(curta, "resum").len(), 3);
    }

    #[test]
    fn el_resum_es_fusiona_amb_el_primer_recent_si_tots_dos_son_user() {
        let hist = vec![
            torn("user", "antic"),
            torn("assistant", "resposta"),
            torn("user", "darrer user 1"),
            torn("assistant", "darrer ia 1"),
            torn("user", "darrer user 2"),
            torn("assistant", "darrer ia 2"),
        ];
        let out = compact_history(&hist, "RESUM");
        // Rols alternats: el resum s'ha fusionat amb el primer recent «user»
        // (6 torns → resum+4 recents fusionats en 4).
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].role, "user");
        assert!(out[0].content.starts_with("RESUM"));
        assert!(out[0].content.contains("darrer user 1"));
        for parell in out.windows(2) {
            assert_ne!(parell[0].role, parell[1].role);
        }
    }

    #[test]
    fn el_resum_extractiu_conserva_decisions_i_rutes() {
        let hist = vec![
            torn("user", "Construeix un joc de tres en ratlla amb Godot."),
            torn(
                "assistant",
                "- Pla: crear tauler i lògica\nHe creat src/tauler.gd\nn _ready():\nError: el node no existeix\n@todo: afegir IA",
            ),
            torn("user", "continua"),
        ];
        let out = extractive_summary(&hist);
        assert!(out.contains("RESUM DE LA SESSIÓ"));
        assert!(out.contains("tres en ratlla"));
        assert!(out.contains("src/tauler.gd"));
        assert!(out.contains("@todo:"));
        assert!(out.contains("Error"));
    }
}
