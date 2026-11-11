//! Aprenentatge local de NoOrbit — la IA que aprèn, en segon pla.
//!
//! La idea: NoOrbit observa les converses de l'usuari amb TOTES les IAs (la
//! local d'Ollama, les remotes amb token, els especialistes i les IAs web) i
//! en treu dues coses:
//!
//! 1. un **registre** (pregunta + resposta + qui ha respost) que es pot
//!    exportar en JSONL per entrenar un model local (LoRA) quan l'usuari
//!    ho vulgui; i
//! 2. unes **lliçons** curtes (preferències, decisions, fets del projecte) que
//!    s'injecten al context dels xats futures: la IA local «sap» el que
//!    l'usuari ja li ha ensenyat, sense cap servei al núvol.
//!
//! Tot és LOCAL: el registre i les lliçons viuen a la carpeta de dades de
//! NoOrbit i la destil·lació la fa el model d'Ollama local. Res surt de
//! l'ordinador tret que l'usuari exporti el dataset ell mateix.
//!
//! L'aprenentatge és SEMPRE en segon pla: el xat no espera mai. La
//! destil·lació es fa en una tasca de fons i, si no hi ha model local, es
//! aplica una heurística senzilla (preferències detectades al text) que no
//! necessita cap IA.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Intercanvis conservats al registre (els mésantics es descarten).
pub const MAX_EXCHANGES: usize = 400;
/// Lliçons conservades (les més antigues es descarten).
pub const MAX_LESSONS: usize = 200;
/// Caràcters màxims d'una pregunta/ resposta desada al registre.
const MAX_TEXT: usize = 4000;
/// Intercanvis pendents per destil·lar que fan disparar la destil·lació
/// automàtica en segon pla.
pub const AUTO_DISTILL_MIN: usize = 4;
/// Temps màxim de la destil·lació (segons): passat aquest temps, es
/// renuncia i es queda amb el que hi hagi.
pub const DISTILL_TIMEOUT_S: u64 = 90;

/// Un torn complet: el que va preguntar l'usuari i el que va respondre una IA.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Exchange {
    pub id: String,
    pub question: String,
    pub answer: String,
    /// Proveïdor (ollama, el nom d'un proveïdor en línia, «web:ChatGPT»…).
    pub provider: String,
    /// Model que ha respost.
    pub model: String,
    /// Marca de temps (segons des de l'època).
    pub at: i64,
}

/// Una lliçó reutilitzable: allò que l'IA ha après i que li serveix al
/// futur («l'usuari treballa amb Rust i Tauri», «prefereix codi en català»).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lesson {
    pub id: String,
    pub text: String,
    /// Etiquetes curtes (python, proves, preferència, projecte…).
    pub tags: Vec<String>,
    /// D'on ha sortit (model local, heurística, especialista…).
    pub source: String,
    /// Quantes vegades ha servit a un xat.
    pub hits: u32,
    pub at: i64,
}

#[derive(Default, Serialize, Deserialize)]
struct FileData {
    #[serde(default)]
    lessons: Vec<Lesson>,
    #[serde(default)]
    exchanges: Vec<Exchange>,
    /// Marca de temps de l'última destil·lació: els intercanvis posteriors
    /// són els «pendents».
    #[serde(default)]
    distilled_at: i64,
}

/// Magatzem persistent de l'aprenentatge (registre + lliçons).
pub struct LearningStore {
    path: PathBuf,
    dataset_path: PathBuf,
    data: Mutex<FileData>,
    /// Hi ha una destil·lació en marxa? (evita dues destil·lacions alhora).
    pub distilling: AtomicBool,
}

impl LearningStore {
    pub fn new(data_dir: PathBuf) -> Self {
        let path = data_dir.join("learning.json");
        let dataset_path = data_dir.join("learning_dataset.jsonl");
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<FileData>(&raw).ok())
            .unwrap_or_default();
        Self {
            path,
            dataset_path,
            data: Mutex::new(data),
            distilling: AtomicBool::new(false),
        }
    }

    fn save(&self) {
        let data = self.data.lock().unwrap();
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string_pretty(&*data) {
            let _ = fs::write(&self.path, raw);
        }
    }

    /// Registra un torn (pregunta + resposta) i el desa.
    pub fn record(&self, question: &str, answer: &str, provider: &str, model: &str) -> Exchange {
        let ex = Exchange {
            id: uuid::Uuid::new_v4().to_string(),
            question: clip(question, MAX_TEXT),
            answer: clip(answer, MAX_TEXT),
            provider: provider.to_string(),
            model: model.to_string(),
            at: now(),
        };
        {
            let mut d = self.data.lock().unwrap();
            d.exchanges.push(ex.clone());
            let extra = d.exchanges.len().saturating_sub(MAX_EXCHANGES);
            if extra > 0 {
                d.exchanges.drain(0..extra);
            }
        }
        self.save();
        ex
    }

    /// Intercanvis recents (els més nous primer).
    pub fn exchanges(&self, limit: usize) -> Vec<Exchange> {
        let d = self.data.lock().unwrap();
        let mut v: Vec<Exchange> = d.exchanges.iter().rev().take(limit).cloned().collect();
        v.reverse();
        v
    }

    /// Intercanvis que encara no s'han destil·lat.
    pub fn pending(&self) -> Vec<Exchange> {
        let d = self.data.lock().unwrap();
        d.exchanges
            .iter()
            .filter(|e| e.at > d.distilled_at)
            .cloned()
            .collect()
    }

    /// Marca els intercanvis pendents com a destil·lats.
    pub fn mark_distilled(&self) {
        {
            let mut d = self.data.lock().unwrap();
            d.distilled_at = now();
        }
        self.save();
    }

    /// Marca com a destil·lats els intercanvis fins a la marca de temps
    /// donada (els que hi ha al voltant, no: es tornaran a mirar).
    pub fn mark_distilled_upto(&self, at: i64) {
        {
            let mut d = self.data.lock().unwrap();
            if at > d.distilled_at {
                d.distilled_at = at;
            }
        }
        self.save();
    }

    pub fn lessons(&self) -> Vec<Lesson> {
        self.data.lock().unwrap().lessons.clone()
    }

    /// Afegeix lliçons noves evitant duplicats (text igual o un contingut
    /// dins de l'altre). Retorna quantes se n'han acceptat.
    pub fn add_lessons(&self, mut incoming: Vec<Lesson>, max: usize) -> usize {
        if incoming.is_empty() {
            return 0;
        }
        let mut added = 0usize;
        {
            let mut d = self.data.lock().unwrap();
            for l in incoming.drain(..) {
                let key = normalize_key(&l.text);
                if key.len() < 8 {
                    continue;
                }
                let dup = d.lessons.iter().any(|e| {
                    let k = normalize_key(&e.text);
                    k == key || (k.len() > 20 && key.len() > 20 && (k.contains(&key) || key.contains(&k)))
                });
                if dup {
                    continue;
                }
                d.lessons.push(l);
                added += 1;
            }
            let max = if max == 0 { MAX_LESSONS } else { max };
            let extra = d.lessons.len().saturating_sub(max);
            if extra > 0 {
                d.lessons.drain(0..extra);
            }
        }
        if added > 0 {
            self.save();
        }
        added
    }

    pub fn delete_lesson(&self, id: &str) -> bool {
        let removed = {
            let mut d = self.data.lock().unwrap();
            let before = d.lessons.len();
            d.lessons.retain(|l| l.id != id);
            before != d.lessons.len()
        };
        if removed {
            self.save();
        }
        removed
    }

    /// Buida el registre i les lliçons.
    pub fn clear(&self) {
        {
            let mut d = self.data.lock().unwrap();
            d.lessons.clear();
            d.exchanges.clear();
            d.distilled_at = 0;
        }
        self.save();
    }

    pub fn stats(&self) -> LearningStats {
        let d = self.data.lock().unwrap();
        LearningStats {
            exchanges: d.exchanges.len(),
            lessons: d.lessons.len(),
            pending: d.exchanges.iter().filter(|e| e.at > d.distilled_at).count(),
            distilling: self.distilling.load(Ordering::SeqCst),
        }
    }

    /// Ruta on s'exporta el dataset (JSONL) per entrenar un model local.
    pub fn dataset_path(&self) -> PathBuf {
        self.dataset_path.clone()
    }

    /// Exporta el registre en JSONL (format de xat, compatible amb
    /// LLaMA-Factory / Unsloth / Axolotl) i retorna la ruta.
    pub fn export_jsonl(&self) -> Result<(PathBuf, usize), String> {
        let exchanges = self.exchanges(MAX_EXCHANGES);
        let mut out = String::new();
        for ex in &exchanges {
            if ex.question.trim().is_empty() || ex.answer.trim().is_empty() {
                continue;
            }
            let rec = serde_json::json!({
                "messages": [
                    { "role": "user", "content": ex.question },
                    { "role": "assistant", "content": ex.answer },
                ],
                "metadata": { "provider": ex.provider, "model": ex.model, "at": ex.at },
            });
            out.push_str(&rec.to_string());
            out.push('\n');
        }
        let path = self.dataset_path.clone();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&path, out).map_err(|e| format!("No s'ha pogut escriure el dataset: {e}"))?;
        let n = exchanges.len();
        Ok((path, n))
    }

    /// Text de les lliçons que apliquen a aquesta consulta (per injectar-lo
    /// al system prompt). Potenci per coincidència de paraules i les
    /// preferències (que apliquen sempre).
    pub fn context_for(&self, query: &str, max: usize) -> String {
        let q: HashSet<String> = tokens(query).into_iter().collect();
        let lessons = self.lessons();
        if lessons.is_empty() {
            return String::new();
        }
        let total = lessons.len();
        let mut scored: Vec<(f32, &Lesson)> = Vec::new();
        for (i, l) in lessons.iter().enumerate() {
            let mut score = 0.0f32;
            for w in tokens(&l.text) {
                if q.contains(&w) {
                    score += 1.0;
                }
            }
            for w in tokens(&l.tags.join(" ")) {
                if q.contains(&w) {
                    score += 2.0;
                }
            }
            // Les preferències apliquen SEMPRE (són el que l'usuari vol que
            // se'l recordi, passi el que passi).
            if l.tags.iter().any(|t| is_preference(t)) {
                score += 3.0;
            }
            // Les lliçons més recents pesen una mica més.
            let age = total.saturating_sub(i);
            if age <= 5 {
                score += 0.6 - 0.1 * age as f32;
            }
            if score >= 1.0 {
                scored.push((score, l));
            }
        }
        if scored.is_empty() {
            return String::new();
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut out = String::from(
            "[APRENENATGE LOCAL] Lliçons que l'usuari ja t'ha ensenyat (o que has \
             après de les altres IAs). Tinge-les en compte: no tornis a preguntar \
             allò que ja saps, i aplica-les a la feina que toca.",
        );
        for (_, l) in scored.iter().take(max.max(1)) {
            let tags = if l.tags.is_empty() {
                String::new()
            } else {
                format!(" ({})", l.tags.join(", "))
            };
            out.push_str(&format!("\n- {}{}", l.text, tags));
        }
        out
    }
}

/// Estat de l'aprenentatge (per a la interfície).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningStats {
    pub exchanges: usize,
    pub lessons: usize,
    pub pending: usize,
    pub distilling: bool,
}

/// Instruccions del destil·lador: llegeix les converses i en treu lliçons.
/// Tot en català (la llengua base de NoOrbit) i MOLT estrictes: el model
/// petit s'obliga a devolver només línies amb el format demanat.
pub const DISTILL_SYSTEM: &str = "Ets el destil·lador de coneixement de NoOrbit: \
un model local que llegeix les converses de l'usuari amb altres IAs i en \
extreu LLIÇONS reutilitzables per a futures tasques.

Regles estrictes:
- Escriu NOMÉS lliçons que val la pena recordar: preferències de l'usuari \
(«sempre treballa amb Rust»), decisions preses, fets tècnics del projecte, \
correccions que l'usuari ha donat al model.
- No resumis la conversa, no expliquis, no inventis res.
- Si no hi ha cap llicó útil, respongueu exactament: CAP
- Format: una llicó per línia, en català, com a mínim:
  LLIÇÓ | etiquetes: a, b | text de la llicó (una frase, com a molt 25 paraules)
- Màxim 6 lliçons. Res de codi de cerca, res de text addicional.";

/// Munta el text que es passa al destil·lador.
pub fn build_distill_prompt(exchanges: &[Exchange]) -> String {
    let mut out = String::from(
        "Aquestes són les converses recents entre l'usuari i les IAs. Extreu les \
lliçons que cal recordar.\n\n",
    );
    for (i, ex) in exchanges.iter().enumerate() {
        out.push_str(&format!(
            "INTERCANVI {} (usuari → {}{})\n",
            i + 1,
            ex.provider,
            if ex.model.is_empty() {
                String::new()
            } else {
                format!("/{}", ex.model)
            }
        ));
        out.push_str(&format!("Usuari: {}\n", ex.question.trim()));
        out.push_str(&format!("IA: {}\n\n", ex.answer.trim()));
    }
    out.push_str("Lliçons (o exactament CAP):");
    out
}

/// Converteix la resposta del model en lliçons. tolerant: accepta diversos
/// formats perquè un model petit no l'encerti sempre.
pub fn parse_lessons(raw: &str, source: &str) -> Vec<Lesson> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        // Resposta buida del destil·lador: cap llicó.
        let low = l.to_lowercase();
        if low == "cap" || low == "cap llicó" || low == "cap lliço" || low == "cap." {
            continue;
        }
        let l = l
            .trim_start_matches(['-', '*', '•', '–', '—', '·', '>'])
            .trim();
        if l.is_empty() {
            continue;
        }
        let parts: Vec<&str> = l.split('|').map(|p| p.trim()).collect();
        let (text, tags) = match parts.len() {
            0 => continue,
            1 => (parts[0].to_string(), Vec::new()),
            2 => {
                let head = strip_marker(parts[0]);
                let second = parts[1];
                // «LLIÇÓ | text» → el text és la segona part.
                if head.is_empty() {
                    (second.to_string(), Vec::new())
                } else if looks_like_tags(second) {
                    // «text | etiquetes» → text + etiquetes.
                    (head, parse_tags(second))
                } else {
                    (format!("{} {}", head, second), Vec::new())
                }
            }
            _ => {
                let head = strip_marker(parts[0]);
                // «LLIÇÓ | etiquetes: a, b | text» → les etiquetes són el camp
                // del mig; la resta és el text de la llicó.
                let tags = parse_tags(parts[1]);
                let text = if head.is_empty() {
                    parts[2..].join(" | ")
                } else {
                    format!("{} {}", head, parts[2..].join(" | "))
                };
                (text.trim().to_string(), tags)
            }
        };
        let text = clean_text(&text);
        if text.len() < 8 {
            continue;
        }
        out.push(Lesson {
            id: uuid::Uuid::new_v4().to_string(),
            text,
            tags,
            source: source.to_string(),
            hits: 0,
            at: now(),
        });
    }
    out
}

/// Heurística de reserva: NO necessita cap IA. Detecta preferències i fets
/// del projecte al text de l'usuari i en fa lliçons. Serveix perquè
/// l'aprenentatge funcioni encara que no hi hagi cap model local instal·lat.
pub fn heuristic_lessons(exchanges: &[Exchange]) -> Vec<Lesson> {
    const MARKERS: &[(&str, &str)] = &[
        ("prefereixo", "preferència"),
        ("preferim", "preferència"),
        ("prefereix", "preferència"),
        ("sempre", "preferència"),
        ("mai ", "preferència"),
        ("no vull", "preferència"),
        ("no facis", "preferència"),
        ("vull que", "preferència"),
        ("necessito", "preferència"),
        ("recorda que", "preferència"),
        ("utilitza", "preferència"),
        ("tracta", "preferència"),
        ("el projecte", "projecte"),
        ("aquest projecte", "projecte"),
        ("treballem amb", "projecte"),
        ("estem fent", "projecte"),
        ("fem servir", "projecte"),
    ];
    let mut out = Vec::new();
    for ex in exchanges {
        for sentence in split_sentences(&ex.question) {
            let low = sentence.to_lowercase();
            for (marker, tag) in MARKERS {
                if low.contains(marker) {
                    let text = clean_text(&sentence);
                    if text.len() >= 12 {
                        out.push(Lesson {
                            id: uuid::Uuid::new_v4().to_string(),
                            text,
                            tags: vec![tag.to_string()],
                            source: "heurística".to_string(),
                            hits: 0,
                            at: now(),
                        });
                    }
                    break;
                }
            }
        }
    }
    out
}

// ── Utilitats ───────────────────────────────────────────────────────────

fn now() -> i64 {
    chrono::Local::now().timestamp()
}

fn clip(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= max {
        return t.to_string();
    }
    let cut: String = t.chars().take(max).collect();
    format!("{}…", cut)
}

/// Neteja el text d'una llicó: espais sobrants, prefixos tipus «Lliçó:».
fn clean_text(s: &str) -> String {
    let mut t = s.trim().to_string();
    for prefix in ["lliçó:", "lliço:", "licó:", "llicó:", "llió:", "text:"] {
        if t.to_lowercase().starts_with(prefix) {
            t = t[prefix.len()..].trim().to_string();
        }
    }
    t = t
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if t.chars().count() > 300 {
        let cut: String = t.chars().take(300).collect();
        t = format!("{cut}…");
    }
    t
}

/// Treu el marcador «LLIÇÓ» del principi d'un camp.
fn strip_marker(s: &str) -> String {
    let low = s.to_lowercase();
    for marker in ["lliçó", "lliço", "llicó", "licó", "llicó"] {
        if low.starts_with(marker) {
            let rest = s[marker.len()..].trim_start_matches([' ', ':', '-']);
            if rest.is_empty() {
                return String::new();
            }
            // «LLIÇÓ: text» o «LLIÇÓ text» → el text.
            if rest.chars().count() > 6 {
                return rest.to_string();
            }
            return String::new();
        }
    }
    s.trim().to_string()
}

/// «a, b, c» → vec!["a","b","c"] (net, sense «etiquetes:»).
fn parse_tags(s: &str) -> Vec<String> {
    let body = s
        .trim()
        .trim_start_matches("etiquetes:")
        .trim_start_matches("etiquetes")
        .trim_start_matches(':')
        .trim();
    let mut out = Vec::new();
    for raw in body.split(',') {
        let t = raw
            .trim()
            .trim_matches(|c: char| c == '"' || c == '\'')
            .trim()
            .to_lowercase();
        if t.is_empty() || t.len() > 24 {
            continue;
        }
        if !out.contains(&t) {
            out.push(t);
        }
        if out.len() >= 5 {
            break;
        }
    }
    out
}

/// Un camp sembla una llista d'etiquetes? (curt i sense puntuació final)
fn looks_like_tags(s: &str) -> bool {
    let words = s.split_whitespace().count();
    words <= 4 && !s.ends_with('.') && !s.contains('?')
}

/// Divideix un text en frases (per a la heurística).
fn split_sentences(s: &str) -> Vec<String> {
    s.split(['.', '!', '?', '\n', ';'])
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect()
}

fn is_preference(tag: &str) -> bool {
    let t = tag.to_lowercase();
    t.contains("prefer")
}

/// Paraules que no serveixen per trobar relacions (català + anglès).
const STOPWORDS: &[&str] = &[
    "que", "què", "com", "per", "però", "perque", "perquè", "amb", "aquest", "aquesta", "aquests",
    "aquestes", "aquells", "aquelles", "tots", "totes", "tota", "tot", "cada", "cap", "també",
    "tambe", "molt", "més", "mes", "molt", "poc", "tampoc", "ja", "encara", "ara", "després",
    "despres", "abans", "aquí", "aqui", "allà", "alla", "allí", "alli", "seu", "seva", "meu",
    "meva", "teu", "teva", "nostre", "nostra", "seu", "cosa", "coses", "fer", "fes", "fes",
    "pots", "podeu", "podem", "vull", "vols", "volem", "tinc", "tens", "tenim", "ha", "han",
    "he", "has", "hi", "ho", "ne", "se", "li", "els", "les", "del", "dels", "de", "la", "el",
    "un", "una", "uns", "unes", "i", "o", "si", "no", "es", "son", "són", "ets", "ets",
    "està", "esta", "estaven", "havia", "haver", "ser", "quan", "on", "què", "perquè",
    "the", "and", "for", "with", "that", "this", "you", "your", "not", "but", "can", "will",
    "from", "have", "has", "was", "were", "are", "all", "any", "into", "out", "about",
];

/// Paraules significatives d'un text (minúscules, sense accents, sense
/// paraules buides).
pub fn tokens(s: &str) -> Vec<String> {
    normalize(s)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3 && !STOPWORDS.contains(w))
        .map(|w| w.to_string())
        .collect()
}

/// Text normalitzat per comparar (minúscules, sense accents ni puntuació).
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' => 'a',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'í' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            _ => c,
        })
        .collect()
}

/// Clau de comparació per detectar lliçons duplicades.
fn normalize_key(s: &str) -> String {
    normalize(s)
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
