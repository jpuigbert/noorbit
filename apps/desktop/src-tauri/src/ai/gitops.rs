//! Ordres de GIT (branques) per a la IA NATIVA: el sisè component del motor
//! simbòlic, al costat de l'edició de fitxers.
//!
//! Fins ara, demanar-li a la IA «esborra la carpeta del joc» esborrava la
//! carpeta però deixava al repositori la branca i el worktree d'aquell joc
//! penjats (branques orfes que no tenien res a veure amb el projecte). I
//! demanar-li «canvia a la branca feature/x» no feia res: NoOrbit no tenia
//! cap manera de tocar branques.
//!
//! Aquest mòdul entén les quatre ordres habituals en català (i en castellà
//! i anglès, que la IA les pot reescriure):
//!
//!  • «llista les branques»  → resposta DIRECTA amb la llista (no fa res).
//!  • «crea la branca X»      → «@branch: create X» (la crea i hi canvia).
//!  • «canvia a la branca X»  → «@branch: checkout X».
//!  • «esborra la branca X»   → «@branch: delete X».
//!
//! Les directrius es materialitzen al frontend (mateix mecanisme que
//! «@delete:» i «@dir:»): el backend no esborra branques per la banda
//! fosca, sinó que deixa la intenció escrita i el materialitzador l'executa
//! amb els comandaments de Git, que ja saben protegir la feina que es podria
//! perdre.

use crate::commands::workspace::current_root;
use crate::git_ops;

/// Acció que demana una directriu de branca.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchAction {
    /// Crea la branca i hi canvia.
    Create,
    /// Canvia a una branca que ja existeix.
    Checkout,
    /// Esborra la branca.
    Delete,
}

impl BranchAction {
    /// El nom curt de l'acció (el que va després de «@branch:»).
    pub fn as_str(&self) -> &'static str {
        match self {
            BranchAction::Create => "create",
            BranchAction::Checkout => "checkout",
            BranchAction::Delete => "delete",
        }
    }
}

/// Resposta del mòdul: o una acció de branca amb el seu nom, o None si la
/// petició no parla de branques.
#[derive(Debug, Clone)]
pub struct BranchIntent {
    pub action: BranchAction,
    /// Nom de la branca (buit per a «list»).
    pub name: String,
}

/// Resposta d'una ordre de branques, o None si la petició no n'és cap.
pub fn handle(prompt: &str) -> Option<String> {
    let p = prompt.trim();
    if p.is_empty() {
        return None;
    }
    let low = pla(p);
    // Sense cap menció de branca no hi ha res a fer («branques» inclòs: el
    // plural català és el que fan les frases reals «llista les branques»).
    if !conte_paraula(&low, &["branca", "branques", "branches", "branch"]) {
        return None;
    }
    // Primer les ACCIONS (crear, canviar, esborrar): si n'hi ha, mana. Després
    // el llistat, que no fa res: només respon.
    if let Some(i) = intent(p) {
        return Some(match i.action {
            BranchAction::Create => format!(
                "Creo la branca **{}** i hi canvio.\n\n@branch: create {}\n\n\
                 Si el nom no és el que volies, digues-me el correcte.",
                i.name, i.name
            ),
            BranchAction::Checkout => format!(
                "Canvio a la branca **{}**.\n\n@branch: checkout {}\n\n\
                 Si el nom no és el que volies, digues-me el correcte.",
                i.name, i.name
            ),
            BranchAction::Delete => format!(
                "Esborro la branca **{}**.\n\n@branch: delete {}\n\n\
                 Si la branca té commits que no s'han fusionat, no l'esborraré: \
                 primer cal fusionar-la o dir-me que la vols esborrar igual.",
                i.name, i.name
            ),
        });
    }
    if es_llistar(&low) {
        return Some(llista());
    }
    None
}

/// La petició demana VEURE les branques (no fer res)?
fn es_llistar(low: &str) -> bool {
    conte_paraula(
        low,
        &[
            "llista", "llistar", "mostra", "mostrar", "quines", "quin", "queda", "queden", "veio",
            "veure", "list", "show",
        ],
    )
}

/// Detecta la intenció de branca d'una petició (la pura, sense efectes).
pub fn intent(prompt: &str) -> Option<BranchIntent> {
    let p = prompt.trim();
    if p.is_empty() {
        return None;
    }
    let low = pla(p);
    // Sense cap menció de branca no hi ha res a fer (plural català inclòs).
    if !conte_paraula(&low, &["branca", "branques", "branches", "branch"]) {
        return None;
    }
    // 1) ESBORRAR: «esborra la branca X», «elimina el branch X»…
    if conte_paraula(
        &low,
        &[
            "esborra", "esborrar", "esborra'm", "borra", "elimina", "eliminar", "suprimeix",
            "suprimir", "delete", "remove",
        ],
    ) {
        let (name, font) = branch_name(p, &low)?;
        // El nom agafat ABANS de «branca» que és una paraula del discurs
        // («esborra la carpeta del joc i la seva branca» → «joc») no és fiable:
        // per a una acció destructiva com esborrar, no l'emetem.
        if font == Font::Abans && !sembla_branca(&name) && es_discurs(&name) {
            return None;
        }
        return Some(BranchIntent {
            action: BranchAction::Delete,
            name,
        });
    }
    // 2) CREAR: «crea la branca X», «nova branca X», «fes una branca X»…
    if conte_paraula(
        &low,
        &[
            "crea", "crear", "nova", "nou", "fes", "fes-me", "obre", "create", "make", "new",
        ],
    ) {
        let (name, _) = branch_name(p, &low)?;
        return Some(BranchIntent {
            action: BranchAction::Create,
            name,
        });
    }
    // 3) CANVIAR: «canvia a la branca X», «vés a la branca X»…
    if conte_paraula(
        &low,
        &[
            "canvia", "canviar", "cambia", "cambiar", "vés", "ves", "passa", "checkout", "switch",
            "utilitza", "usa",
        ],
    ) {
        let (name, _) = branch_name(p, &low)?;
        return Some(BranchIntent {
            action: BranchAction::Checkout,
            name,
        });
    }
    // 4) LLISTAR: «llista les branques», «quines branques hi ha?»…
    //    (Ho decideix `handle`, que retorna la llista DIRECTA.)
    None
}

/// Llista les branques del projecte obert (resposta directa, no fa res).
fn llista() -> String {
    let Some(root) = current_root() else {
        return "No hi ha cap projecte obert: no puc veure les branques.".to_string();
    };
    if !git_ops::is_repo(&root) {
        return "Aquesta carpeta no és un repositori Git: no hi ha branques.".to_string();
    }
    match git_ops::branches(&root) {
        Ok(bs) if bs.is_empty() => "El repositori encara no té cap branca.".to_string(),
        Ok(bs) => {
            let mut out = String::from("Branques del repositori:\n");
            for b in bs {
                let mut linha = if b.current {
                    format!("• **{}** (actual)", b.name)
                } else {
                    format!("• {}", b.name)
                };
                if let Some(w) = &b.worktree {
                    linha.push_str(&format!(" — worktree: {}", w));
                }
                if let Some(u) = &b.upstream {
                    linha.push_str(&format!(" — segueix {}", u));
                }
                if b.ahead > 0 || b.behind > 0 {
                    linha.push_str(&format!(" (↑{} ↓{})", b.ahead, b.behind));
                }
                out.push_str(&linha);
                out.push('\n');
            }
            out.trim_end().to_string()
        }
        Err(e) => format!("No puc llegir les branques: {}", e),
    }
}

// ── Ajudes de text ─────────────────────────────────────────────────────────

/// Text en minúscules i sense accents, per comparar paraules.
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

/// D'on hem tret el nom de la branca? Importa per a la confiança: un nom que
/// ve ABANS de «branca» pot ser la paraula del discurs més propera, no el
/// nom real («…la seva branca» → «joc»).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Font {
    /// Entre cometes: explícit de l'usuari.
    Cometes,
    /// Just abans de «branca».
    Abans,
    /// Just després de «branca».
    Despres,
}

/// El nom de la branca d'una petició. Es busca, per ordre:
///  1. El text entre cometes («la branca "feature/x"»).
///  2. La paraula abans de «branca» si té forma de branca («feature/x, la
///     branca del joc» → «feature/x»).
///  3. La paraula després de «branca/branch» («la branca feature/x»).
fn branch_name(prompt: &str, low: &str) -> Option<(String, Font)> {
    // 1) Cometes: són la forma explícita de l'usuari, mana. L'única que no
    // pot emetre's és una amb SALT DE LÍNIA: fragmentaria la directriu (el
    // frontend en lligiria només la primera línia i podria encertar una
    // branca equivocal). Els noms amb espais sí que passen: el backend els
    // rebutja amb un error clar que cita el nom sencer.
    for seg in quoted(prompt) {
        let s = seg.trim();
        if s.is_empty() || !s.chars().any(|c| c.is_alphanumeric()) {
            continue;
        }
        if s.contains('\n') {
            return None;
        }
        return Some((s.to_string(), Font::Cometes));
    }
    // 2) Abans de la paraula «branca»/«branch» (plural primer: «branca» és
    //    substring de «branques» i s'empassaria el marc bo).
    let mut abans: Option<String> = None;
    for marca in ["branques", "branches", "branch", "branca"] {
        if let Some(n) = word_before(low, marca) {
            abans = Some(n);
            break;
        }
    }
    // 3) Després de la paraula «branca»/«branch».
    let mut despres: Option<String> = None;
    for marca in ["branques", "branches", "branch", "branca"] {
        if let Some(n) = word_after(low, marca) {
            despres = Some(n);
            break;
        }
    }
    // El nom que «te forma de branca» (amb barra o guió) és el bo: «feature/x».
    if let Some(n) = &abans {
        if sembla_branca(n) {
            return Some((n.clone(), Font::Abans));
        }
    }
    match despres {
        Some(n) => Some((n, Font::Despres)),
        None => abans.map(|n| (n, Font::Abans)),
    }
}

/// El mot té forma de nom de branca de Git? (amb barra «feature/x» o guió).
fn sembla_branca(mot: &str) -> bool {
    mot.contains('/') || mot.contains('-')
}

/// Connectors que NO són mai un nom de branca («la», «del», «seva»…).
/// Els possessius hi són inclosos: «esborra la branca SEVA» no vol dir que la
/// branca es digui «seva».
const CONNECTORS: &[&str] = &[
    "la", "el", "les", "els", "un", "una", "uns", "unes", "de", "del", "a", "al", "en", "per",
    "i", "o", "que", "amb", "sense", "d", "l", "es", "hi", "son", "aquest", "aquesta", "aquell",
    "aquella", "se", "me", "nos", "seu", "seva", "seus", "seves", "meu", "meva", "meus", "meves",
    "teu", "teva", "teus", "teves", "nostre", "nostra", "nostres", "vostre", "vostra", "vostres",
    "llur", "llurs", "propi", "propia", "propis", "mateix", "mateixa", "mateixos", "mateixes",
];

/// Paraules del discurs que difícilment són el nom real d'una branca: si
/// apareixen just ABANS de «branca» («la carpeta del joc i la seva branca» →
/// «joc»), no en traem una directriu destructiva.
fn es_discurs(word: &str) -> bool {
    conte_paraula(
        &pla(word),
        &[
            "joc", "jocs", "carpeta", "carpetes", "directori", "directoris", "fitxer", "fitxers",
            "projecte", "projectes", "versi", "historia", "feina", "codi", "branca", "branches",
            "branch", "rama",
        ],
    )
}

/// Connectors que no són cap nom de branca?
fn es_connector(word: &str) -> bool {
    CONNECTORS.contains(&word)
}

/// Paraula (amb barres i guions) que ve JUST DESPRÉS de `marca`: el primer
/// «mot» que no sigui un connector («la branca DEL JOC» → «joc»).
fn word_after(low: &str, marca: &str) -> Option<String> {
    let mut off = 0usize;
    while let Some(pos) = low[off..].find(marca) {
        let abs = off + pos + marca.len();
        for tok in low[abs..].split_whitespace() {
            let w = net_nom(tok);
            if w.is_empty() {
                continue;
            }
            if es_connector(&w) {
                continue; // «del», «a», «la»…: no són el nom
            }
            if w.chars().any(|c| c.is_ascii_alphabetic()) {
                return Some(w);
            }
            // Un mot sense lletres («123») no pot ser una branca: atura't.
            return None;
        }
        off = abs;
        if off >= low.len() {
            break;
        }
    }
    None
}

/// Neteja un «mot» perquè pugui ser un nom de branca: sense cometes, sense
/// puntuació final.
fn net_nom(tok: &str) -> String {
    let mut w: String = tok
        .trim_matches(|c: char| {
            matches!(c, ',' | ';' | ':' | '»' | '«' | '"' | '\'' | '!' | '?' | '(' | ')')
        })
        .to_string();
    while w.ends_with('.') {
        w.pop();
    }
    w
}

/// Paraula (amb barres i guions) que ve JUST ABANS de `marca`: l'última
/// paraula del text anterior que no sigui un connector («feature/x, la
/// branca del joc» → «feature/x»).
fn word_before(low: &str, marca: &str) -> Option<String> {
    let mut off = 0usize;
    while let Some(pos) = low[off..].find(marca) {
        let abs = off + pos;
        let head = &low[..abs];
        for tok in head.split_whitespace().rev() {
            let w = net_nom(tok);
            if w.is_empty() {
                continue;
            }
            if es_connector(&w) {
                continue;
            }
            if w.len() >= 2 && w.chars().any(|c| c.is_ascii_alphabetic()) {
                return Some(w);
            }
        }
        off = abs + marca.len();
        if off >= low.len() {
            break;
        }
    }
    None
}

/// Segments entre cometes («"…"» o ««…»»), en ordre.
fn quoted(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Option<String> = None;
    for c in text.chars() {
        match c {
            '"' | '«' | '»' => match cur.take() {
                Some(s) => {
                    let s = s.trim().to_string();
                    if !s.is_empty() {
                        out.push(s);
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
        let s = s.trim().to_string();
        if !s.is_empty() {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecta_crear_una_branca() {
        let i = intent("crea la branca feature/joc").unwrap();
        assert_eq!(i.action, BranchAction::Create);
        assert_eq!(i.name, "feature/joc");
    }

    #[test]
    fn detecta_canviar_de_branca() {
        let i = intent("canvia a la branca develop").unwrap();
        assert_eq!(i.action, BranchAction::Checkout);
        assert_eq!(i.name, "develop");
    }

    #[test]
    fn detecta_esborrar_una_branca() {
        let i = intent("esborra la branca vella").unwrap();
        assert_eq!(i.action, BranchAction::Delete);
        assert_eq!(i.name, "vella");
    }

    #[test]
    fn el_nom_entre_cometes_guanya() {
        let i = intent("vés a la branca \"feature/x\"").unwrap();
        assert_eq!(i.name, "feature/x");
    }

    #[test]
    fn el_nom_abans_de_la_petita_tambe_val() {
        // «feature/x, la branca del joc»: el nom va ABANS de «branca».
        let low = pla("feature/x, la branca del joc");
        assert_eq!(
            branch_name("feature/x, la branca del joc", &low),
            Some(("feature/x".to_string(), Font::Abans))
        );
    }

    #[test]
    fn els_connectors_no_son_noms_de_branca() {
        let low = pla("canvia a la branca del joc");
        // «del» és un connector: el nom que en sortirà és «joc», no «del».
        assert_eq!(
            branch_name("canvia a la branca del joc", &low),
            Some(("joc".to_string(), Font::Despres))
        );
    }

    #[test]
    fn els_possessius_no_son_noms_de_branca() {
        // «esborra la carpeta del joc i la seva branca»: «seva» és connector, i
        // la paraula que queda abans de «branca» («joc») és discurs: per a una
        // acció destructiva no s'emet cap directriu (l'esborrat de la carpeta
        // el fa el mòdul d'edició, que és el que realment es demanava).
        assert!(intent("esborra la carpeta del joc i la seva branca").is_none());
        assert!(intent("elimina la meva branca").is_none() || {
            // «meva» és connector: si no queda cap altre nom, no hi ha intenció.
            intent("elimina la meva branca").map(|i| i.name != "meva").unwrap_or(true)
        });
    }

    #[test]
    fn un_nom_explicit_despres_de_branca_siempre_compta() {
        // «esborra la branca joc»: el nom va DESPRÉS de «branca»: és explícit
        // i s'emet, encara que «joc» també sigui una paraula del discurs.
        let i = intent("esborra la branca joc").unwrap();
        assert_eq!(i.action, BranchAction::Delete);
        assert_eq!(i.name, "joc");
    }

    #[test]
    fn el_plural_branques_tambe_activa_el_guard() {
        // «llista les branques» (plural català, el documentat al cap de
        // fitxer): abans el guard el deixava fora i `handle` no responia.
        let r = handle("llista les branques");
        assert!(r.is_some(), "el plural ha de reconèixer-se");
        let i = intent("esborra les branques velles").unwrap();
        assert_eq!(i.action, BranchAction::Delete);
        assert_eq!(i.name, "velles");
    }

    #[test]
    fn les_cometes_que_no_poden_ser_branca_no_endevinen_res() {
        // Nom amb espais: les cometes són explícites, s'emet; el backend el
        // rebutja amb un error clar (una branca no pot portar espais).
        let i = intent("esborra la branca \"cosa amb espai\"").unwrap();
        assert_eq!(i.name, "cosa amb espai");
        // Nom amb salt de línia: fragmentaria la directriu (el frontend en
        // lligiria «a»): no se'nemet cap, res d'actuar sobre la branca
        // equivocal per un copiar-enganyar.
        assert!(intent("esborra la branca \"a\nb\"").is_none());
    }

    #[test]
    fn sense_brisques_no_hi_ha_intencio() {
        assert!(intent("hola com estàs").is_none());
        assert!(intent("esborra la carpeta dist").is_none());
        assert!(intent("llista els fitxers").is_none());
    }

    #[test]
    fn l_accio_es_el_primer_que_mana() {
        // «esborra» guanya sobre «crea» (no es pot crear i esborrar alhora).
        let i = intent("esborra la branca temporal").unwrap();
        assert_eq!(i.action, BranchAction::Delete);
    }

    #[test]
    fn la_resposta_porta_la_directriu() {
        let r = handle("canvia a la branca develop").unwrap();
        assert!(r.contains("@branch: checkout develop"), "{}", r);
        let r = handle("crea la branca nova").unwrap();
        assert!(r.contains("@branch: create nova"), "{}", r);
        let r = handle("esborra la branca vella").unwrap();
        assert!(r.contains("@branch: delete vella"), "{}", r);
    }
}
