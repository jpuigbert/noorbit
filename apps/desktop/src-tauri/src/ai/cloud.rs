//! Parseig de la pàgina HTML d'ollama.com (biblioteca o cerca) per extreure'n
//! els models cloud. Lògica PURA de text: sense Tauri ni estat, testeable sola.

use super::CloudModel;

/// Extreu els models de la pàgina HTML d'ollama.com (biblioteca o cerca).
/// Ambdues pàgines comparteixen l'estructura: enllaç `href="/library/NOM"`
/// seguit d'un paràgraf de descripció.
pub(crate) fn parse_cloud_models(html: &str) -> Vec<CloudModel> {
    let mut out: Vec<CloudModel> = Vec::new();
    let mut seen: std::collections::HashSet<String> = Default::default();
    for chunk in html.split("href=\"/library/").skip(1) {
        // El nom acaba a la següent cometes que tanca l'atribut href.
        let name: String = chunk.chars().take_while(|&c| c != '"').collect();
        let name = name.trim().to_string();
        if name.is_empty() || name.contains('/') || !seen.insert(name.clone()) {
            continue;
        }
        let description = extract_desc(chunk);
        // La mida que ollama.com mostra a la fitxa («7.6GB», «2.1TB», «404MB»):
        // el que pesa la descàrrega ABANS de fer-la.
        let size = extract_size(chunk);
        out.push(CloudModel { name, description, size, url: None });
        if out.len() >= 80 {
            break;
        }
    }
    out
}

/// Mida de descàrrega que ollama.com mostra a la fitxa d'un model: el primer
/// text del tipus «7.6GB», «2.1TB» o «404MB» dins del fragment. En bytes;
/// `None` si la fitxa no porta mida. Busca primer TB i després GB/MB perquè
/// el número que hi ha just ABANS de la unitat sempre sigui el seu.
fn extract_size(chunk: &str) -> Option<u64> {
    let up = chunk.to_ascii_uppercase();
    for (unit, mult) in [
        ("TB", 1_000_000_000_000u64),
        ("GB", 1_000_000_000u64),
        ("MB", 1_000_000u64),
    ] {
        let mut from = 0;
        while let Some(p) = up[from..].find(unit) {
            let p = from + p;
            let before = &up[..p];
            // El número just abans de la unitat: retrocedeix sobre dígits,
            // punt decimal i espais (p. ex. «>7.6GB<» → 7.6).
            let ns = before
                .char_indices()
                .rev()
                .take_while(|(_, c)| c.is_ascii_digit() || *c == '.' || *c == ' ')
                .map(|(i, _)| i)
                .last();
            if let Some(ns) = ns {
                if let Ok(v) = before[ns..].trim().parse::<f64>() {
                    if v > 0.0 {
                        return Some((v * mult as f64) as u64);
                    }
                }
            }
            from = p + unit.len();
        }
    }
    None
}

/// Dins d'un fragment d'un model, retorna el text del primer <p> descriptiu.
fn extract_desc(chunk: &str) -> String {
    // El primer <p ...>text...</p> del fragment és la descripció.
    let Some(p_idx) = chunk.find("<p ") else {
        return String::new();
    };
    let after_open = &chunk[p_idx..];
    let Some(gt) = after_open.find('>') else {
        return String::new();
    };
    let body = &after_open[gt + 1..];
    let desc = match body.find("</p>") {
        Some(end) => &body[..end],
        None => body,
    };
    decode_entities(&strip_tags(desc)).trim().to_string()
}

/// Elimina qualsevol etiqueta HTML restant.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Decodifica les entitats HTML més habituals que apareixen a les descripcions.
fn decode_entities(s: &str) -> String {
    s.replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}