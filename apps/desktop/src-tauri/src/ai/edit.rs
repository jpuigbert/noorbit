//! Edició de fitxers de la IA nativa: cinquè component del motor simbòlic
//! (després de matemàtiques, cervell, aprenentatge i plantilles de codi).
//!
//! El motor natiu no inventa programari arbitrari, però SÍ que manipula els
//! fitxers del projecte quan la petició ho diu EXPLÍCIT: esborrar-los,
//! reemplaçar-hi text, afegir-hi contingut al final o crear-ne un de nou amb
//! el contingut que l'usuari enganxa (entre cometes o en una tanca de codi).
//!
//! Respon amb les directrius que el xat ja sap materialitzar («@edit:»,
//! «@delete:», «@file:» → `parseEditBlocks`/`parseDeleteDirectives`/
//! `parseFileBlocks` del frontend) i, quan pot, verifica CONTRA EL FITXER
//! REAL del projecte: si el text a canviar no hi és, ho diu i mostra les
//! primeres línies en lloc d'emetre una edició impossible. Si la petició no
//! és cap edició reconeguda, retorna None i el xat prova les plantilles i,
//! si cal, una IA amb generació lliure.

use crate::commands::workspace::resolve_in_root;

/// Resposta d'edició materialitzable, o None si la petició no és una edició
/// reconeguda.
pub fn handle(prompt: &str) -> Option<String> {
    let p = prompt.trim();
    if p.is_empty() {
        return None;
    }
    if let Some(r) = handle_delete(p) {
        return Some(r);
    }
    if let Some(r) = handle_replace(p) {
        return Some(r);
    }
    if let Some(r) = handle_append(p) {
        return Some(r);
    }
    if let Some(r) = handle_create(p) {
        return Some(r);
    }
    None
}

// ── Ajudes de text ────────────────────────────────────────────────────────

/// Text en minúscules i sense accents, per comparar paraules d'una petició.
fn pla(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' => 'a',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'í' | 'ï' | 'î' | 'ì' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            _ => c,
        })
        .collect()
}

/// El text conté alguna de les paraules com a mot sencer?
fn conte_paraula(text: &str, paraules: &[&str]) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .any(|w| paraules.contains(&w))
}

/// Busca `needle` a `haystack` sense distingir majúscules i retorna la
/// posició de BYTE dins del text ORIGINAL (segur per retallar-lo després,
/// encara que hi hagi accents).
fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    let h: Vec<char> = haystack.chars().collect();
    let n: Vec<char> = needle.chars().collect();
    if n.is_empty() || n.len() > h.len() {
        return None;
    }
    for i in 0..=(h.len() - n.len()) {
        let mut ok = true;
        for k in 0..n.len() {
            if !h[i + k].eq_ignore_ascii_case(&n[k]) {
                ok = false;
                break;
            }
        }
        if ok {
            return haystack.char_indices().nth(i).map(|(b, _)| b);
        }
    }
    None
}

/// Els segments entre cometes («"…"» o ««…»»), en ordre, sense retallar-ne
/// l'interior més que els extrems. Del text ORIGINAL: es conserva el cas.
fn quoted_segments(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Option<String> = None;
    for c in text.chars() {
        match c {
            '"' | '«' | '»' => match cur.take() {
                Some(s) => {
                    let s = s.trim();
                    if !s.is_empty() {
                        out.push(s.to_string());
                    }
                }
                None => cur = Some(String::new()),
            },
            _ => {
                if let Some(s) = &mut cur {
                    s.push(c);
                }
            }
        }
    }
    if let Some(s) = cur {
        let s = s.trim();
        if !s.is_empty() {
            out.push(s.to_string());
        }
    }
    out
}

/// Camins CANDIDATS de la petició: tokens amb extensió («app.py»,
/// «src/x.tsx»…) i paraules darrere de «carpeta/directori/folder…». Ordre
/// d'aparició, sense duplicats.
pub fn targets(prompt: &str) -> Vec<String> {
    let mut out = file_like(prompt);
    let low = pla(prompt);
    for marca in ["carpeta", "directori", "directorio", "folder", "directory"] {
        let mut rest: &str = &low;
        let mut off = 0usize; // desplaçament de BYTES dins `low`
        while let Some(pos) = rest.find(marca) {
            let abs = off + pos + marca.len();
            if abs < low.len() {
                // Salta l'espai o els dos punts després de la marca
                // («carpeta dist», «carpeta: dist»): si no, la paraula
                // comença en blanc i es perd.
                let tail = &low[abs..];
                let inici = tail
                    .char_indices()
                    .find(|(_, c)| !c.is_whitespace() && *c != ':')
                    .map(|(i, _)| i)
                    .unwrap_or(tail.len());
                let word: String = low[abs + inici..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/' | '\\'))
                    .collect();
                let n = word.len();
                if (2..=48).contains(&n) && !out.contains(&word) {
                    out.push(word);
                }
                // Avança SEMPRE: una paraula buida («carpeta ·dist») salta
                // un CARÀCTER sencer, mai un byte a mitges (retallar un
                // multi-byte per la meitat faria un panic).
                let salt = if n == 0 {
                    let c_len = low[abs + inici..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(1);
                    inici + c_len
                } else {
                    inici + n
                };
                off = abs + salt;
            } else {
                off = low.len();
            }
            if off >= low.len() {
                break;
            }
            rest = &low[off..];
        }
    }
    out
}

/// Tokens «fitxer.ext» del text (extensió de 2+ lletres, nom amb lletres).
/// Descarta decimals («2.5»), URLs i sentències que acaben en punt.
fn file_like(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if !(c.is_alphanumeric() || matches!(c, '.' | '/' | '\\' | '-' | '_' | ':')) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len()
            && (chars[i].is_alphanumeric() || matches!(chars[i], '.' | '/' | '\\' | '-' | '_' | ':'))
        {
            i += 1;
        }
        let t: String = chars[start..i].iter().collect();
        // Sense punt ni dos punts finals: «demo.rs:» («guarda demo.rs:») és el
        // mateix fitxer que «demo.rs», i «C:» sol no és cap fitxer.
        let t = t.trim_end_matches(|c| c == '.' || c == ':').to_string();
        if t.contains("://") || t.starts_with("www.") {
            continue;
        }
        if let Some(dot) = t.rfind('.') {
            let nom = &t[..dot];
            let ext = &t[dot + 1..];
            let ext_ok = ext.len() >= 2 && ext.chars().all(|c| c.is_ascii_alphabetic());
            let nom_ok = !nom.is_empty() && nom.chars().any(|c| c.is_ascii_alphabetic());
            if ext_ok && nom_ok && !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

/// Tanca de codi ```…``` de la petició (el cos, sense la tanca), si n'hi ha.
fn extract_fence(text: &str) -> Option<String> {
    let t = text.trim();
    let start = t.find("```")?;
    let rest = &t[start + 3..];
    let nl = rest.find('\n').map(|i| i + 1).unwrap_or(0);
    let body = &rest[nl..];
    let end = body.find("```")?;
    let code = body[..end].trim();
    if code.is_empty() {
        None
    } else {
        Some(code.to_string())
    }
}

/// Llegeix el fitxer del projecte (resolent la ruta contra l'arrel oberta).
fn read_file(target: &str) -> Option<String> {
    std::fs::read_to_string(resolve_in_root(target)).ok()
}

/// Primers n línies del contingut, per mostrar un tast a l'usuari.
fn peek(content: &str, n: usize) -> String {
    content
        .lines()
        .take(n)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Treu del davant una col·lecció de paraules comuns de la frase del fitxer
/// («al fitxer», «en el fitxer», «dins de»…) i retorna el text net.
fn strip_tail_connectors(text: &str) -> String {
    let mut t = text.trim().to_string();
    for _ in 0..2 {
        let low = pla(&t);
        let mut stripped = false;
        for w in [
            "al fitxer", "a l'arxiu", "del fitxer", "de l'arxiu", "en el fitxer",
            "dins de", "dins del fitxer", "in file", "in the file", "of the file",
            "a ", "en ", "de ", "al ", "del ", "in ", "of ", "the ",
        ] {
            if low.ends_with(w) {
                t = t[..t.len() - w.len()].trim().to_string();
                stripped = true;
                break;
            }
        }
        if !stripped {
            break;
        }
    }
    t
}

/// Troba `needle` al contingut: exacte o sense distingir majúscules. Retorna
/// (inici, final) en bytes del contingut amb la coincidència REAL (el text
/// exacte del fitxer, perquè la directriu «@edit:» encaixi a la primera).
fn locate_in(content: &str, needle: &str) -> Option<(usize, usize)> {
    let n = needle.trim();
    if n.is_empty() {
        return None;
    }
    if let Some(pos) = content.find(n) {
        return Some((pos, pos + n.len()));
    }
    if let Some(pos) = find_ci(content, n) {
        let h: Vec<char> = content[pos..].chars().take(n.chars().count()).collect();
        return Some((pos, pos + h.iter().map(|c| c.len_utf8()).sum::<usize>()));
    }
    None
}

// ── Esborrat ──────────────────────────────────────────────────────────────

/// «esborra src/vell.py», «elimina la carpeta dist», «treu el README.md»…
/// Emet «@delete: ruta» (verificant que la ruta existeix al projecte).
fn handle_delete(prompt: &str) -> Option<String> {
    let low = pla(prompt);
    if !conte_paraula(
        &low,
        &[
            "esborra", "esborrar", "esborra'm", "borra", "elimina", "eliminar", "suprimeix",
            "suprimir", "treu", "treure", "delete", "remove", "rm",
        ],
    ) {
        return None;
    }
    let t = targets(prompt);
    let first = t.first()?;
    if resolve_in_root(first).exists() {
        Some(format!(
            "Esborro **{first}** del projecte.\n\n@delete: {first}\n\n\
             Si no era el que volies, digues-me el camí exacte."
        ))
    } else {
        Some(format!(
            "No trobo **{first}** dins el projecte (o no hi ha cap carpeta oberta). \
             Digues-me el camí exacte del que vols esborrar."
        ))
    }
}

// ── Reemplaçament ─────────────────────────────────────────────────────────

/// «canvia "A" per "B" a app.py», «reemplaça A amb B a src/x.ts»…
/// Emet «@edit: ruta» amb CERCA/NOU, verificant que CERCA és al fitxer REAL.
fn handle_replace(prompt: &str) -> Option<String> {
    let low = pla(prompt);
    if !conte_paraula(
        &low,
        &[
            "canvia", "canviar", "cambia", "cambiar", "reemplaça", "reemplaca", "reemplaçar",
            "substitueix", "substitueir", "replace", "change",
        ],
    ) {
        return None;
    }
    // 1. Cometes: la forma més fiable («canvia "A" per "B" a app.py»).
    let quotes = quoted_segments(prompt);
    let (trobar, posar, fitxer): (String, String, String) = if quotes.len() >= 2 {
        let f = targets(prompt).first()?.clone();
        (quotes[0].clone(), quotes[1].clone(), f)
    } else {
        // 2. Separador «X per Y», «X amb Y», «X → Y»… sobre el text ORIGINAL
        //    (es conserva el cas del que es vol canviar).
        let seps = [" per ", " amb ", "→", " to ", " with ", " for ", " a "];
        let mut tria: Option<(String, String, String)> = None;
        for s in seps {
            let Some(pos) = find_ci(prompt, s) else {
                continue;
            };
            let abans = prompt[..pos].trim();
            let despres = prompt[pos + s.len()..].trim();
            if abans.is_empty() || despres.is_empty() {
                continue;
            }
            // L'«abans» ha de contenir el verb d'edició (o ser molt breu).
            let abans_low = pla(abans);
            let te_verb = conte_paraula(
                &abans_low,
                &[
                    "canvia", "canviar", "cambia", "reemplaça", "reemplaca", "substitueix",
                    "replace", "change", "posa", "posar", "posa'm", "deixa",
                ],
            );
            if !te_verb && abans_low.split_whitespace().count() > 4 {
                continue;
            }
            // El «després» pot acabar amb la ruta del fitxer: la separem.
            let (nou, ruta) = if let Some(f) = targets(prompt).into_iter().find(|c| {
                find_ci(despres, c).is_some()
            }) {
                let ipos = find_ci(despres, &f)?;
                let cap = despres[..ipos].trim();
                let cap = strip_tail_connectors(cap);
                (cap, f)
            } else {
                return None; // sense fitxer de destí no és un reemplaçament útil
            };
            if nou.is_empty() {
                continue;
            }
            tria = Some((abans.to_string(), nou.to_string(), ruta));
            break;
        }
        tria?
    };

    let content = match read_file(&fitxer) {
        Some(c) => c,
        None => {
            return Some(format!(
                "No trobo **{fitxer}** dins el projecte (o no hi ha cap carpeta oberta). \
                 Digues-me el camí exacte del fitxer a editar."
            ));
        }
    };
    // Verifiquem que el text CERCA hi és (exacte o amb majúscules diferents).
    let (real_find, nova) = match locate_in(&content, &trobar) {
        Some((a, b)) => (content[a..b].to_string(), posar),
        None => {
            return Some(format!(
                "No trobo «{trobar}» dins **{fitxer}**. Les primeres línies del fitxer són:\n\n\
                 ```\n{}\n```\n\n\
                 Copia'm el text EXACTE que vols canviar (entre cometes) i el text nou.",
                peek(&content, 12)
            ));
        }
    };
    Some(format!(
        "Edito **{fitxer}**: canvio «{real_find}» per «{nova}». Es mostrarà la revisió verd/roig.\n\n\
         @edit: {fitxer}\n\
         <<<<<<< CERCA
{real_find}
=======
{nova}
>>>>>>>

\
         Si el canvi no és el que volies, prem «Rebutja» a la revisió."
    ))
}

// ── Afegit al final ───────────────────────────────────────────────────────

/// «afegeix print('hola') al final de app.py»… Emet «@edit: ruta» amb
/// CERCA = última línia no buida del fitxer i NOU = aquella línia + el text.
fn handle_append(prompt: &str) -> Option<String> {
    let low = pla(prompt);
    if !conte_paraula(
        &low,
        &["afegeix", "afegir", "append", "afegeix-hi", "afegeixhi", "posa", "posar", "escriu", "escriure"],
    ) || !low.contains("final") {
        return None;
    }
    let Some(pos) = find_ci(prompt, "al final") else {
        return None;
    };
    let abans = prompt[..pos].trim();
    let despres = prompt[pos + "al final".len()..].trim();
    // El «després» porta la ruta («de app.py», «del fitxer app.py»…).
    let fitxer = targets(prompt)
        .into_iter()
        .find(|c| find_ci(despres, c).is_some())
        .or_else(|| targets(prompt).first().cloned())?;
    // El text a afegir és el «davant» sense el verb inicial.
    let afegit = {
        let verbs = [
            "afegeix-hi", "afegeix", "afegir", "append", "posa-hi", "posa", "escriu-hi", "escriu",
            "escriure",
        ];
        let mut t = abans.to_string();
        for v in verbs {
            if let Some(i) = find_ci(&t, v) {
                let end = i + v.len();
                if end < t.len() && pla(&t[end..]).trim_start().is_empty() {
                    // el verb és TOT el davant (sense text): res a afegir
                    return None;
                }
                t = t[end..].trim_start().to_string();
                break;
            }
        }
        strip_tail_connectors(&t)
    };
    if afegit.is_empty() {
        return None;
    }
    let content = match read_file(&fitxer) {
        Some(c) => c,
        None => {
            return Some(format!(
                "No trobo **{fitxer}** dins el projecte (o no hi ha cap carpeta oberta). \
                 Digues-me el camí exacte del fitxer."
            ));
        }
    };
    if content.trim().is_empty() {
        // Fitxer buit: un «@edit:» amb CERCA buit no s'aplicaria mai; millor
        // crear-lo directament amb el contingut (equivalent, i revisable).
        return Some(format!(
            "**{fitxer}** és buit: l'omplo amb el text que m'has donat.\n\n\
             @file: {fitxer}\n```\n{afegit}\n```"
        ));
    }
    let last = content
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    let (cerca, nova) = if last.is_empty() {
        (String::new(), afegit.clone())
    } else {
        (last.to_string(), format!("{}\n{}", last, afegit))
    };
    Some(format!(
        "Afegeixo al final de **{fitxer}**: «{afegit}».\n\n\
         @edit: {fitxer}\n\
         <<<<<<< CERCA
{cerca}
=======
{nova}
>>>>>>>"
    ))
}

// ── Creació amb contingut ─────────────────────────────────────────────────

/// «crea app.py amb print('hola')», «escriu notes.txt que digui "…"» o un
/// verb de creació amb una tanca de codi. Emet «@file: ruta» amb el
/// contingut ENGANXAT per l'usuari (no inventat).
fn handle_create(prompt: &str) -> Option<String> {
    let low = pla(prompt);
    let verb = conte_paraula(
        &low,
        &[
            "crea", "crear", "escriu", "escriure", "guarda", "guardar", "desa", "desar",
            "create", "make",
        ],
    );
    if !verb {
        return None;
    }
    // Contingut: tanca de codi preferida; si no, darrera cometa del prompt.
    let contingut = extract_fence(prompt).or_else(|| quoted_segments(prompt).pop())?;
    let fitxer = targets(prompt).first()?.clone();
    let existeix = resolve_in_root(&fitxer).exists();
    let avis = if existeix {
        "⚠️ Aquest fitxer JA EXISTEIX: es mostrarà la revisió verd/roig per confirmar."
    } else {
        ""
    };
    Some(format!(
        "Et creo **{fitxer}** amb el contingut que m'has donat. {avis}\n\n\
         @file: {fitxer}\n```\n{contingut}\n```"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecta_tokens_de_fitxer_i_descarta_brossa() {
        assert_eq!(file_like("esborra src/app.py"), vec!["src/app.py"]);
        assert_eq!(file_like("canvia X per Y a C:/coses/x.tsx"), vec!["C:/coses/x.tsx"]);
        assert!(file_like("calcula 2.5 + 3").is_empty());
        assert!(file_like("mira https://x.com").is_empty());
    }

    #[test]
    fn detecta_carpetes_amb_marca() {
        let t = targets("esborra la carpeta dist i el fitxer vell.js");
        assert!(t.contains(&"vell.js".to_string()));
        assert!(t.contains(&"dist".to_string()));
    }

    #[test]
    fn segments_entre_cometes_conserven_el_cas() {
        assert_eq!(quoted_segments("canvia \"Color\" per \"color\""), vec!["Color", "color"]);
        assert_eq!(quoted_segments("canvia «a» per «b» a x.py"), vec!["a", "b"]);
    }

    #[test]
    fn extrau_la_tanca_de_codi() {
        let r = extract_fence("crea app.py\n```python\nprint('hola')\n```");
        assert_eq!(r.as_deref(), Some("print('hola')"));
    }

    #[test]
    fn cerca_insensible_preserva_l_index_original() {
        let text = "canvia Hola per Adeu";
        let pos = find_ci(text, "hola").unwrap();
        assert_eq!(&text[pos..pos + 4], "Hola");
    }

    #[test]
    fn localitza_al_contingut_real() {
        let c = "const Color = '#2563eb';\n";
        let (a, b) = locate_in(c, "color = '#2563eb'").unwrap();
        assert_eq!(&c[a..b], "Color = '#2563eb'");
        assert!(locate_in(c, "no hi soc").is_none());
    }

    #[test]
    fn esborrat_sense_projecte_obert_respon_honestament() {
        // Sense arrel de workspace, la ruta relativa no existeix: missatge net.
        let r = handle("esborra src/no_existeix_3f9.py").unwrap();
        assert!(r.contains("No trobo"), "{}", r);
    }

    #[test]
    fn crea_amb_contingut_enganxat() {
        let r = handle("crea hola.py amb \"print('hola')\"").unwrap();
        assert!(r.contains("@file: hola.py"), "{}", r);
        assert!(r.contains("print('hola')"), "{}", r);
    }

    #[test]
    fn crea_amb_tanca_de_codi() {
        let r = handle("guarda demo.rs:\n```rust\nfn main() {}\n```").unwrap();
        assert!(r.contains("@file: demo.rs"), "{}", r);
        assert!(r.contains("fn main() {}"), "{}", r);
    }

    #[test]
    fn les_converses_no_son_edicions() {
        assert!(handle("hola com estàs").is_none());
        assert!(handle("calcula 2+2").is_none());
        assert!(handle("què és rust").is_none());
        // Parla de canviar però sense fitxer ni cometes: no és edició clara.
        assert!(handle("canvia això per allò").is_none());
    }
}
