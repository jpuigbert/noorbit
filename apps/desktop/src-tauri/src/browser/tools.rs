//! Directrius «NB|…»: el pont entre QUALSEVOL IA del xat i el navegador
//! intern, sense cap botó.
//!
//! El model (Ollama local, DeepSeek sense token, ChatGPT, un expert…) escriu una línia de directriu a la seua resposta i s'atura.
//! NoOrbit l'executa de veritat —obre la finestra, en llegeix el text
//! renderitzat o pregunta a una IA web amb el compte de l'usuari— i li'n
//! torna el resultat en un torn nou, perquè continue i responga amb ell.
//! És el mateix mecanisme d'un agent amb eines, però negociat per text:
//! funciona fins i tot amb models xicotets que no tenen tool-calling natiu.
//!
//! Rècord de legitimitat: les dades que obtenim són les que la pàgina
//! MOSTRA EN PANTALLA després que l'usuari hi haja entrat amb el SEU compte.
//! Mai galetes, mai tokens, mai sessions alienes; si cal una acció humana,
//! la finestra es fa visible perquè la faça la persona.

use super::{self as br};
use serde_json::json;
use tauri::{AppHandle, Emitter};
use std::time::Duration;

/// Nombre màxim de torns de directrius dins d'UNA mateixa resposta del xat
/// (model → directriu → resultat → model…). Passat això, es retorna l'últim
/// text generat tal com està.
pub const MAX_ROUNDS: usize = 3;

/// Text que s'afegeix al system prompt de TOTS els xats (cap model, proveïdor
/// ni expert): ensenya les directrius al model. Breu i explícit perquè fins i
/// tot els models locals menuts les entenguen.
pub const DIRECTIVE_PROMPT: &str = "— NAVEGADOR INTERN (eines de NoOrbit) —\n\
Tens un navegador web intern. Quan et calga informació que no tens (dades \
actuals, una pàgina concreta, o opinar d'una IA més potent que tu), escriu \
UNA directriu en una línia a part i ATURA la resposta eixe torn. NoOrbit \
l'executarà i et tornarà el resultat en un missatge nou; aleshores \
respondràs a la pregunta original amb eixe contingut.\n\
  NB|OBRIR|https://url-de-la-pagina\n\
  NB|LLEGIR|\n\
  NB|PREGUNTA_IA|nom-de-la-ia|la pregunta completa en una linia\n\
  NB|ESTAT|\n\
  NB|TANCAR|\n\
A «PREGUNTA_IA» només hi ha: deepseek, dsh (DeepSeek Harness local, \
port 3080), chatgpt, claude, gemini, perplexity, grok. La directriu va SOLA \
en el seu torn (no la baralles amb la resposta). \
ABANS de fer servir PREGUNTA_IA, demana PERMÍS a l'usuari en un torn a \
part: digue-li a quina IA web vols preguntar i què li vols demanar; \
executa la directriu NOMÉS quan l'usuari ho accepte (si ja t'ho ha demanat \
ell mateix, no cal que ho tornes a preguntar). La finestra del navegador \
es mostrarà sempre a la pantalla perquè l'usuari veja la pregunta i hi \
faiga login amb el seu compte si cal. \
Quan rebes línies «NB|RESULTAT|…», NO repetiscales: usa'n el contingut, \
cita la font i no inventes res que no hi siga. Si un resultat porta blocs \
de «CODI» i és útil per a la tasca, INTEGRAR-LO al projecte: revisa'l, \
adapta'l i emet cada fitxer amb una línia «@file: ruta» seguida del \
contingut complet (sense tanques): NoOrbit els escriu SOLS al workspace.";

/// Una directriu reconeguda a la resposta del model.
#[derive(Debug, Clone, PartialEq)]
pub enum NBAction {
    Open(String),
    Read,
    AskIa(String, String),
    Status,
    Close,
}

/// Extreu les directrius «NB|OP|arg|arg2» en línies pròpies (tolera
/// majúscules/minúscules i sers del model amb espais davant).
pub fn parse(text: &str) -> Vec<NBAction> {
    let Ok(re) = regex::Regex::new(r"(?im)^[ \t>*-]*nb\|([a-z_]+)\|([^|\r\n]*)(?:\|([^\r\n]*))?[ \t]*$") else {
        return vec![];
    };
    let mut out = Vec::new();
    for c in re.captures_iter(text) {
        let g = |i: usize| c.get(i).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
        let op = g(1).to_uppercase();
        let act = match op.as_str() {
            "OBRIR" | "OBRI" | "OPEN" => NBAction::Open(g(2)),
            "LLEGIR" | "READ" => NBAction::Read,
            "PREGUNTA_IA" | "IAWEB" | "ASKIA" | "CONSULTA" => NBAction::AskIa(g(2), g(3)),
            "ESTAT" | "STATUS" => NBAction::Status,
            "TANCAR" | "CLOSE" => NBAction::Close,
            _ => continue,
        };
        out.push(act);
    }
    out
}

/// Avança el pas de progrés al xat (mateix event i etiquetatge de sessió que
/// la resta de l'agent, perquè es vega en directe què està fent el navegador).
fn progress(app: &AppHandle, detail: &str, ok: bool) {
    let _ = app.emit(
        "agent://progress",
        json!({
            "label": "Navegador intern",
            "detail": detail,
            "ok": ok,
            "session": crate::ai::current_session(),
        }),
    );
}

/// Retalla text per no inflar el context del model.
fn clip(s: &str, n: usize) -> String {
    let v: Vec<char> = s.chars().collect();
    if v.len() <= n {
        s.to_string()
    } else {
        format!("{}…[retallat]", v[..n].iter().collect::<String>())
    }
}

/// Converteix blocs de codi arreplegats d'una pàgina web en Markdown amb
/// tanques i llenguatge, perquè el model els recognosca com a codi real i
/// els puga emetre amb «@file:» (DIRECTIVE_PROMPT ja li'n dona la ordre).
fn blocks_md(blocks: &[br::CodeBlock], max: usize) -> String {
    let mut out = String::new();
    for (i, b) in blocks.iter().take(max).enumerate() {
        let truncated = b.code.chars().count() > 8000;
        out.push_str(&format!(
            "// bloc {} ({})\n```{}\n{}\n```\n",
            i + 1,
            if b.lang.is_empty() { "llenguatge no indicat" } else { b.lang.as_str() },
            b.lang,
            clip(&b.code, 8000)
        ));
        if truncated {
            out.push_str(
                "// (aquest bloc ve RETALLAT: completa'l amb criteri abans \
                 d'escriure'l, o torna a preguntar)\n",
            );
        }
    }
    out
}

/// Executa les directrius en ordre i construeix l'informe («NB|RESULTAT|…»)
/// que es tornarà al model com a torn d'usuari.
pub async fn execute(app: &AppHandle, actions: &[NBAction]) -> String {
    let mut report = String::from("NB|RESULTAT|—\n");
    for a in actions {
        let line = match a {
            NBAction::Open(url) => {
                if url.is_empty() {
                    "NB|RESULTAT|OBRIR| error: falta la URL".to_string()
                } else {
                    // S'obre EN SEGON PLA: si la pàgina demana una acció humana,
                    // «LLEGIR» la farà visible. L'usuari no rep molèsties gratis.
                    match br::open(app, url, true) {
                        Ok(()) => {
                            progress(app, &format!("Obrint {}", url), true);
                            // Deixa uns segons perquè la pàgina arrenque el JS.
                            tokio::time::sleep(Duration::from_secs(4)).await;
                            format!("NB|RESULTAT|OBRIR| ok — pàgina oberta en segon pla: {}", url)
                        }
                        Err(e) => format!("NB|RESULTAT|OBRIR| error: {}", e),
                    }
                }
            }
            NBAction::Read => {
                progress(app, "Llegint la pàgina oberta…", true);
                let fut = async {
                    let mut last = String::new();
                    for _ in 0..3 {
                        match br::extract_text(app, Duration::from_secs(10)).await {
                            Ok(ex) => {
                                if br::needs_human_action(&ex.text) {
                                    br::set_visible(app, true).ok();
                                    return Ok(format!(
                                        "(requereix una acció HUMANA: la finestra ja és a \
                                         la pantalla perquè l'usuari inicie sessió o done \
                                         un captcha — NoOrbit no la suplanta) Això és el \
                                         que es veia: {}",
                                        clip(&ex.text, 4000)
                                    ));
                                }
                                if ex.text.chars().count() > 80 {
                                    return Ok(ex.text);
                                }
                                last = ex.text;
                            }
                            Err(e) => last = e.to_string(),
                        }
                    }
                    Err(anyhow::anyhow!("{}", last))
                };
                match tokio::time::timeout(Duration::from_secs(45), fut).await {
                    Ok(Ok(tx)) => {
                        // La pàgina pot ser un tutorial amb codi: si en traiem
                        // blocs, van a part perquè el model els puga escriure
                        // al projecte (@file) quan els veja aprofitables.
                        let blocks = br::grab_code_blocks(app).await.unwrap_or_default();
                        let mut r = format!("NB|RESULTAT|LLEGIR|\n{}", clip(&tx, 6000));
                        if !blocks.is_empty() {
                            r.push_str("\n\nCODI DE LA PÀGINA (aprofita'l amb @file si cal):\n");
                            r.push_str(&blocks_md(&blocks, 6));
                        }
                        r
                    }
                    Ok(Err(e)) => format!("NB|RESULTAT|LLEGIR| error: {}", e),
                    Err(_) => "NB|RESULTAT|LLEGIR| error: temps d'espera exhaurit".to_string(),
                }
            }
            NBAction::AskIa(name, question) => {
                if question.trim().is_empty() {
                    "NB|RESULTAT|PREGUNTA_IA| error: format esperat \
                     NB|PREGUNTA_IA|deepseek|la pregunta"
                        .to_string()
                } else {
                    ask_web_ia(app, name, question).await
                }
            }
            NBAction::Status => {
                let s = br::status(app);
                format!(
                    "NB|RESULTAT|ESTAT| obert={} visible={} url={}",
                    s.open, s.visible, s.url
                )
            }
            NBAction::Close => match br::close(app) {
                Ok(()) => "NB|RESULTAT|TANCAR| ok".to_string(),
                Err(e) => format!("NB|RESULTAT|TANCAR| error: {}", e),
            },
        };
        report.push_str(&line);
        report.push('\n');
    }
    report
}

/// Consulta manual (legal) a una IA web: navegador visible, sessió de
/// l'usuari, pregunta escrita a la seua caixa i resposta llegida de pantalla.
async fn ask_web_ia(app: &AppHandle, name: &str, question: &str) -> String {
    let Some(ia) = br::find_ia(name) else {
        return format!(
            "NB|RESULTAT|PREGUNTA_IA| error: no conec cap IA web anomenada «{}». \
             Opcions: deepseek, dsh (DeepSeek Harness local), chatgpt, claude, \
             gemini, perplexity, grok.",
            name
        );
    };
    let st = br::status(app);
    let host = ia
        .url
        .split("//")
        .nth(1)
        .unwrap_or(ia.url)
        .split('/')
        .next()
        .unwrap_or(ia.url);
    if !st.open || !st.url.contains(host) {
        // Visible: potser caldra iniciar sessió i l'usuari ha de veure-ho tot.
        if br::open(app, ia.url, false).is_err() {
            return format!("NB|RESULTAT|PREGUNTA_IA| error: no s'ha pogut obrir {}", ia.url);
        }
        tokio::time::sleep(Duration::from_secs(6)).await;
    } else {
        br::set_visible(app, true).ok();
    }
    progress(app, &format!("Preguntant a {} (xat web, amb el teu compte)…", ia.name), true);
    match br::chat_probe(app).await {
        Ok(probe) if probe.login_needed && !probe.has_input => {
            return format!(
                "NB|RESULTAT|PREGUNTA_IA| {} demana iniciar sessió i la finestra ja \
                 és a la pantalla: l'usuari ha d'entrar amb el SEU compte i tornar a \
                 preguntar. NoOrbit no toca credencials ni sessions.",
                ia.name
            );
        }
        Ok(_) => {}
        Err(e) => return format!("NB|RESULTAT|PREGUNTA_IA| error en sondejar la pàgina: {}", e),
    }
    let fut = br::chat_ask(app, question, Duration::from_secs(170));
    match tokio::time::timeout(Duration::from_secs(185), fut).await {
        Ok(Ok(ans)) => {
            // El xat web sovint dona CODI en blocs propis. Els arrepleguem
            // separats (amb el llenguatge) perquè la IA local els puga
            // INTEGRAR de veritat: si els emet amb «@file:», NoOrbit els
            // escriu al projecte automaticament (autoMaterialize).
            let blocks = br::grab_code_blocks(app).await.unwrap_or_default();
            let mut body = format!(
                "NB|RESULTAT|PREGUNTA_IA| resposta del xat web de {} (compte de \
                 l'usuari, text visible en pantalla):\n{}",
                ia.name,
                clip(&ans, 6000)
            );
            if !blocks.is_empty() {
                body.push_str("\n\nCODI OBTINGUT DEL XAT WEB — aprofita'l:\n");
                body.push_str(&blocks_md(&blocks, 8));
                body.push_str(
                    "INSTRUCCIÓ: si aquest codi és útil per a la tasca de l'usuari, \
                     NO el mostres només en prosa: INTEGRA-LO al projecte. Abans \
                     d'escriure un fitxer, revisa el codi rebut (no el creguis \
                     cec) i adapta'l al llenguatge i l'estructura del workspace \
                     obert; després emet cada fitxer amb una \
                     línia «@file: ruta/relativa» seguida del contingut complet \
                     (sense tanques). NoOrbit escriurà eixos fitxers AUTOMATICAMENT \
                     en acabar eixe torn. Si el workspace està buit, inventa tu la \
                     estructura mínima (README.md inclòs) i digues a l'usuari què \
                     has creat i d'on ve el codi.",
                );
            }
            body
        }
        Ok(Err(e)) => format!("NB|RESULTAT|PREGUNTA_IA| error: {}", e),
        Err(_) => "NB|RESULTAT|PREGUNTA_IA| error: la pàgina no ha estabilizat la \
                   resposta en temps"
            .to_string(),
    }
}
