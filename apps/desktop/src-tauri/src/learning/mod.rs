//! Aprenentatge local de NoOrbit — la IA que apren, en segon pla.
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
//!
//! DES DE 0.71.12 el registre també es pot abocar en els formats que documenta
//! Soup (Apache-2.0): línies `conversation` i `feedback` de `soup loop` i un
//! `soup.yaml` per a `soup train`. Així el que l'usuari ensenya al xat pot
//! acabar convertint-se en un model GGUF propi. Les regles de Soup que
//! apliquem de veritat: un identificador UUID per resposta (el feedback hi
//! apunta), fitxers només-afegits, temps ISO 8601 en UTC, cap número inventat
//! (un recompte no mesurat és `null`, no `0`) i la cua del fitxer com a
//! validació (`data.val_split`), cosa que obliga a dir si hi ha fugues.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Intercanvis conservats al registre (els més antics es descarten).
pub const MAX_EXCHANGES: usize = 250;
/// Lliçons conservades (les més antigues es descarten).
pub const MAX_LESSONS: usize = 200;
/// Caràcters màxims d'una pregunta desada al registre.
const MAX_QUESTION: usize = 1200;
/// Caràcters màxims d'una resposta desada al registre (per entrenar, com més
/// context millor, però el fitxer no ha de fer alguns megabytes).
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
    /// Vot de l'usuari sobre AQUESTA resposta: `1` = 👍 (cal entrenar amb
    /// ella), `-1` = 👎 (no s'aprèn res i serveix de rebuig per a DPO),
    /// `None` = sense votar.
    #[serde(default)]
    pub vote: Option<i8>,
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
    /// Carpeta de dades de NoOrbit: aquí dins es genera el paquet que entén
    /// Soup (`.soup-loops/`).
    data_dir: PathBuf,
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
            data_dir,
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
            question: clip(question, MAX_QUESTION),
            answer: clip(answer, MAX_TEXT),
            provider: provider.to_string(),
            model: model.to_string(),
            at: now(),
            vote: None,
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

    /// Pendents de destil·lar deixant-ne fora els que l'usuari ha rebutjat:
    /// una resposta 👎 no ensenya res i, destil·lada, ensenyaria l'error.
    pub fn pending_distillable(&self) -> Vec<Exchange> {
        self.pending()
            .into_iter()
            .filter(|e| e.vote != Some(-1))
            .collect()
    }

    // ── Vots de l'usuari (el que fa que el dataset valgui alguna cosa) ──────

    /// Vota la resposta d'un intercanvi pel seu `id`: `1` = 👍, `-1` = 👎,
    /// `0` = treu el vot. Retorna l'intercanvi ja votat (`None` si l'`id` no
    /// és al registre). Escriure el vot NO genera cap línia de Soup: això ho
    /// fa `soup_sync`, que és l'única cosa que sap què ja s'ha escrit.
    pub fn vote(&self, id: &str, vote: i8) -> Option<Exchange> {
        let v = if vote > 0 {
            Some(1)
        } else if vote < 0 {
            Some(-1)
        } else {
            None
        };
        let (found, changed) = {
            let mut d = self.data.lock().unwrap();
            match d.exchanges.iter_mut().find(|e| e.id == id) {
                Some(e) => {
                    let changed = e.vote != v;
                    e.vote = v;
                    (Some(e.clone()), changed)
                }
                None => (None, false),
            }
        };
        if changed {
            self.save();
        }
        found
    }

    /// Vota buscant la pregunta (el xat només guarda el text, no l'id del
    /// registre). Coincideix amb l'intercanvi MÉS RECENT que hi encaixa.
    pub fn vote_by_question(&self, question: &str, vote: i8) -> Option<Exchange> {
        let key = normalize_key(question);
        let id = {
            let d = self.data.lock().unwrap();
            d.exchanges
                .iter()
                .rev()
                .find(|e| same_question(&normalize_key(&e.question), &key))
                .map(|e| e.id.clone())
        };
        match id {
            Some(id) => self.vote(&id, vote),
            None => None,
        }
    }

    /// Quants 👍 i quants 👎 hi ha al registre.
    pub fn votes(&self) -> (usize, usize) {
        let d = self.data.lock().unwrap();
        (
            d.exchanges.iter().filter(|e| e.vote == Some(1)).count(),
            d.exchanges.iter().filter(|e| e.vote == Some(-1)).count(),
        )
    }

    /// Intercanvis acceptats amb 👍 (els més antics primer). Són la REFERÈNCIA
    /// amb què s'avalua qualsevol cervell nou: si una resposta no l'ha
    /// acceptada ningú, no pot servir de prova.
    pub fn accepted(&self, max: usize) -> Vec<Exchange> {
        let d = self.data.lock().unwrap();
        let mut v: Vec<Exchange> = d
            .exchanges
            .iter()
            .filter(|e| e.vote == Some(1) && !e.question.trim().is_empty())
            .cloned()
            .collect();
        if max > 0 && v.len() > max {
            v.drain(0..v.len() - max);
        }
        v
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
        let (voted_up, voted_down) = (
            d.exchanges.iter().filter(|e| e.vote == Some(1)).count(),
            d.exchanges.iter().filter(|e| e.vote == Some(-1)).count(),
        );
        LearningStats {
            exchanges: d.exchanges.len(),
            lessons: d.lessons.len(),
            pending: d.exchanges.iter().filter(|e| e.at > d.distilled_at).count(),
            distilling: self.distilling.load(Ordering::SeqCst),
            voted_up,
            voted_down,
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
        let mut d = self.data.lock().unwrap();
        let lessons = d.lessons.clone();
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
        let escollides: Vec<&Lesson> = scored.iter().take(max.max(1)).map(|(_, l)| *l).collect();
        // Marca les lliçons que han servit (es desen amb el proper registre:
        // no fa falta escriure el fitxer aquí).
        for l in &escollides {
            if let Some(x) = d.lessons.iter_mut().find(|x| x.id == l.id) {
                x.hits += 1;
            }
        }
        let mut out = String::from(
            "[APRENENATGE LOCAL] Lliçons que l'usuari ja t'ha ensenyat (o que has \
             après de les altres IAs). Tinge-les en compte: no tornis a preguntar \
             allò que ja saps, i aplica-les a la feina que toca.",
        );
        for l in &escollides {
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

// ── Magatzem compartit ─────────────────────────────────────────────────────
// El registre viu a l'`AppState` de Tauri. La porta d'avaluació
// (`ai::brain_gate`) també el necessita i corre en un fil de fons on no
// arriba l'estat: en lloc de duplicar el magatzem (dos objectes que escriuen
// al mateix fitxer es trepitjarien), en guardem una referència quan
// s'organitza l'aplicació.

static SHARED: OnceLock<Arc<LearningStore>> = OnceLock::new();

/// Desa la instància que faran servir els mòduls sense accés a l'estat de
/// Tauri. Només compta la primera vegada; la resta s'ignora.
pub fn set_shared(store: Arc<LearningStore>) {
    let _ = SHARED.set(store);
}

/// El magatzem compartit, o `None` si encara no s'ha registrat (abans
/// d'arrencar l'app i als tests, que no en necessiten cap).
pub fn shared() -> Option<&'static Arc<LearningStore>> {
    SHARED.get()
}

/// Estat de l'aprenentatge (per a la interfície).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningStats {
    pub exchanges: usize,
    pub lessons: usize,
    pub pending: usize,
    pub distilling: bool,
    /// Respostes que l'usuari ha aprovat amb 👍.
    pub voted_up: usize,
    /// Respostes que l'usuari ha rebutjat amb 👎 (no s'aprèn res d'aquestes).
    pub voted_down: usize,
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

/// Munta el text que es passa al destil·lador. Les respostes que l'usuari ha
/// rebutjat (👎) NO hi van: s'ensenyaria l'error. Si el lot sencer és
/// rebutjat es diu clar, perquè el model respongui CAP en lloc d'inventar.
pub fn build_distill_prompt(exchanges: &[Exchange]) -> String {
    let mut out = String::from(
        "Aquestes són les converses recents entre l'usuari i les IAs. Extreu les \
lliçons que cal recordar.\n\n",
    );
    let mut posades = 0usize;
    for (i, ex) in exchanges.iter().enumerate() {
        if ex.vote == Some(-1) {
            continue;
        }
        posades += 1;
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
    if posades == 0 {
        out.push_str(
            "Tots els intercanvis d'aquest lot han estat rebutjats per l'usuari: no hi ha \nres que valga la pena recordar.\n\n",
        );
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
        // Una resposta rebutjada no pot generar una lliçó, ni encara que
        // l'error sigui el que l'usuari va escriure a la pregunta.
        if ex.vote == Some(-1) {
            continue;
        }
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

/// Dues preguntes són «la mateixa» als efectes del vot: idèntiques, o una
/// començament de l'altra amb prou caràcter per no confondre-les (el registre
/// retalla a `MAX_QUESTION`, així que la versió curta és la normal).
fn same_question(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let ca = a.chars().count();
    let cb = b.chars().count();
    // Cap de les dues massa curta: «sí» coincidiria amb qualsevol cosa.
    if ca < 24 || cb < 24 {
        return false;
    }
    let (curt, llarg) = if ca <= cb { (a, b) } else { (b, a) };
    let pref: String = curt.chars().take(60).collect();
    llarg.starts_with(&pref)
}

// ── PAQUET SOUP: el registre en els formats que entén un entrenador real ────
//
// NoOrbit ja guardava les converses; el que aporta Soup (Apache-2.0) és que
// ho guardi en un format que un programa d'entrenament de debò puga llegir
// sense traduccions: línies `conversation` i `feedback` del bucle continu, un
// `train.jsonl` ChatML i un `soup.yaml` amb NOMÉS claus documentades.

/// Carpeta del bucle continu tal com la documenta Soup: `soup loop` llegeix
/// `.soup-loops/traces/*.jsonl` i `.soup-loops/feedback.jsonl`.
pub const SOUP_DIR: &str = ".soup-loops";
/// Què posem a `source`. Els noms coneguts de Soup són `soup-serve`,
/// `playground`… i en accepta qualsevol en minúscules: així es veu qui va
/// escriure cada línia si el fitxer acaba tenint més d'un autor.
pub const SOUP_SOURCE: &str = "noorbit";
/// Fracció de files que Soup reserva per validar prenent-les de la CUA del
/// fitxer (`data.val_split`). No ho decidim nosaltres: és com carrega Soup.
pub const VAL_SPLIT: f64 = 0.1;
/// Files per sota de les quals un LoRA no aprèn pràcticament res. No és un
/// llindar de Soup: és el que diu el scorecard per no vendre fum.
const MIN_FILS: usize = 16;

/// El que `soup_sync` ha escrit, per ensenyar-ho a la interfície (i, sobretot,
/// per dir el que NO s'ha pogut fer).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoupPack {
    /// Carpeta amb tot (`.soup-loops`).
    pub dir: String,
    pub traces_path: String,
    pub feedback_path: String,
    pub train_path: String,
    pub preference_path: String,
    pub config_path: String,
    pub readme_path: String,
    /// Files del dataset i línies noves dels regs només-afegits.
    pub rows_train: usize,
    pub rows_preference: usize,
    pub new_traces: usize,
    pub new_feedback: usize,
    pub total_traces: usize,
    pub votes_up: usize,
    pub votes_down: usize,
    /// Files repetides que hem deixat fora (mateixa pregunta i resposta).
    pub duplicates: usize,
    /// Respostes amb un `id` que no és un UUID vàlid i que per tant no es
    /// poden referenciar des del feedback (Soup exigeix UUID canònic).
    pub skipped_ids: usize,
    /// Puntuació de qualitat 0..1 i què li falta, raó per raó.
    pub quality: f32,
    pub scorecard: Vec<String>,
    /// Preguntes que surten alhora a la part de trenament i a la de
    /// validació: fuita de dades, perquè la validació deixaria de ser cega.
    pub leakage: usize,
    pub base_model: String,
    pub provenance_path: String,
    /// Pot aquest equip entrenar-hi? Es comprova, no es suposa.
    pub can_train_here: bool,
    pub train_check: String,
    pub note: String,
}

/// Què ja s'ha abocat als fitxers només-afegits. Cal un cursor perquè Soup
/// diu que un escriptor no edita ni esborra línies anteriors, mentre que el
/// registre de NoOrbit sí que en descarta (MAX_EXCHANGES).
#[derive(Default, Serialize, Deserialize)]
struct SoupState {
    /// Només s'abocen els torns posteriors a aquesta marca (el mateix cursor
    /// `until` que usa `soup loop`).
    #[serde(default)]
    last_at: i64,
    /// Torns escrits amb `at == last_at`: segon a segon, si no es guarden
    /// es podrien perdre per sempre.
    #[serde(default)]
    last_ids: Vec<String>,
    /// `id` de la resposta -> vot ja escrit a `feedback.jsonl`.
    #[serde(default)]
    votes: BTreeMap<String, i8>,
}

/// Aboca el registre al format de Soup i torna què n'ha sortit.
impl LearningStore {
    /// Escriu `.soup-loops/` dins la carpeta de dades. Es pot cridar tantes
    /// vegades com es vulgui: les línies de conversa i de feedback
    /// S'AFEGEIXEN (mai es reescriuen ni es borren, com mana Soup) i els
    /// datasets, la configuració i el llegiu-me es regeneren sencers.
    pub fn soup_sync(&self, base_model: &str) -> Result<SoupPack, String> {
        let dir = self.data_dir.join(SOUP_DIR);
        let traces_dir = dir.join("traces");
        fs::create_dir_all(&traces_dir)
            .map_err(|e| format!("no es pot crear {}: {e}", traces_dir.display()))?;
        let traces = traces_dir.join(format!("{SOUP_SOURCE}.jsonl"));
        let feedback = dir.join("feedback.jsonl");
        let train = dir.join("train.jsonl");
        let provenance = dir.join("provenance.jsonl");
        let preference = dir.join("preference.jsonl");
        let cfg = dir.join("soup.yaml");
        let cfg_dpo = dir.join("soup-preference.yaml");
        let readme = dir.join("LLEGEIX-ME.md");
        let state_path = dir.join("noorbit-cursor.json");

        let all = self.exchanges(MAX_EXCHANGES);
        let st = read_state(&state_path);

        // 1) Línies de conversa noves: només els torns que encara no s'han
        //    abocat (el cursor evita reescriure el que ja és al fitxer).
        let mut buf = String::new();
        let mut new_traces = 0usize;
        let mut skipped_ids = 0usize;
        let mut last_at = st.last_at;
        let mut last_ids: Vec<String> = Vec::new();
        for e in &all {
            let nou = e.at > st.last_at || (e.at == st.last_at && !st.last_ids.contains(&e.id));
            if !nou {
                continue;
            }
            if e.question.trim().is_empty() || e.answer.trim().is_empty() {
                continue;
            }
            if uuid_lowercase(&e.id).is_none() {
                skipped_ids += 1;
                continue;
            }
            if let Some(line) = conversation_line(e) {
                buf.push_str(&line);
                buf.push('\n');
                new_traces += 1;
                if e.at > last_at {
                    last_at = e.at;
                    last_ids.clear();
                }
                if e.at == last_at {
                    last_ids.push(e.id.clone());
                }
            }
        }
        if new_traces > 0 {
            use std::io::Write;
            let mut f = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&traces)
                .map_err(|e| format!("no puc escriure {}: {e}", traces.display()))?;
            f.write_all(buf.as_bytes())
                .map_err(|e| format!("no puc escriure {}: {e}", traces.display()))?;
        }

        // 2) Línies de feedback: només els vots que han CANVIAT des de
        //    l'últim sínc. Soup fa valer l'última línia per (response, kind),
        //    així que un 👍 que es retira s'escriu amb `value: null`.
        let mut fbuf = String::new();
        let mut new_feedback = 0usize;
        let mut votes_written = st.votes.clone();
        for e in &all {
            let Some(id) = uuid_lowercase(&e.id) else {
                continue;
            };
            let abans = votes_written.get(&id).copied();
            let cal = match (abans, e.vote) {
                (None, None) => false,
                (Some(a), Some(b)) => a != b,
                (_, Some(_)) => true,
                (Some(_), None) => true,
            };
            if !cal {
                continue;
            }
            if let Some(line) = feedback_line(&id, e.vote, now()) {
                fbuf.push_str(&line);
                fbuf.push('\n');
                new_feedback += 1;
            }
            match e.vote {
                Some(v) => {
                    votes_written.insert(id, v);
                }
                None => {
                    votes_written.remove(&id);
                }
            }
        }
        // Els ids que ja no són al registre (MAX_EXCHANGES els va descartant)
        // no ocupen lloc al cursor.
        let vius: HashSet<String> = all.iter().map(|e| e.id.clone()).collect();
        votes_written.retain(|k, _| vius.contains(k));
        if new_feedback > 0 {
            use std::io::Write;
            let mut f = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&feedback)
                .map_err(|e| format!("no puc escriure {}: {e}", feedback.display()))?;
            f.write_all(fbuf.as_bytes())
                .map_err(|e| format!("no puc escriure {}: {e}", feedback.display()))?;
        }

        // 3) Datasets i configuració: es regeneren (són derivats, no un reg).
        let (rows, duplicates) = train_rows(&all);
        let xerr = |e: std::io::Error, p: &std::path::Path| {
            format!("no puc escriure {}: {e}", p.display())
        };
        fs::write(&train, jsonl_chatml(&rows)).map_err(|e| xerr(e, &train))?;
        fs::write(&provenance, jsonl_provenance(&rows)).map_err(|e| xerr(e, &provenance))?;
        let (pref, rows_preference) = jsonl_preference(&all);
        fs::write(&preference, pref).map_err(|e| xerr(e, &preference))?;
        fs::write(&cfg, yaml_sft(base_model, rows.len())).map_err(|e| xerr(e, &cfg))?;
        fs::write(&cfg_dpo, yaml_dpo(base_model, rows_preference))
            .map_err(|e| xerr(e, &cfg_dpo))?;

        // 4) El que es pot dir del que s'hi ha posat (i el que no).
        let (votes_up, votes_down) = self.votes();
        let (quality, scorecard) = scorecard(&rows, votes_up, votes_down);
        let leakage = fuita_val(&rows, VAL_SPLIT);
        let (can_train_here, train_check) = soup_pot_entrenar_aqui();
        let total_traces = fs::read_to_string(&traces)
            .map(|raw| raw.lines().count())
            .unwrap_or(0);

        let pack = SoupPack {
            dir: dir.to_string_lossy().to_string(),
            traces_path: traces.to_string_lossy().to_string(),
            feedback_path: feedback.to_string_lossy().to_string(),
            train_path: train.to_string_lossy().to_string(),
            provenance_path: provenance.to_string_lossy().to_string(),
            preference_path: preference.to_string_lossy().to_string(),
            config_path: cfg.to_string_lossy().to_string(),
            readme_path: readme.to_string_lossy().to_string(),
            rows_train: rows.len(),
            rows_preference,
            new_traces,
            new_feedback,
            total_traces,
            votes_up,
            votes_down,
            duplicates,
            skipped_ids,
            quality,
            scorecard,
            leakage,
            base_model: base_model.to_string(),
            can_train_here,
            train_check,
            note: "NoOrbit escriu les dades en el format de Soup; NO instal·la Soup ni \
                   entrena res sol. Cal un equip amb Python 3.11+ i una GPU per fer el \
                   pas següent.".to_string(),
        };
        fs::write(&readme, readme_soup(&pack)).map_err(|e| xerr(e, &readme))?;

        let nou = SoupState {
            last_at,
            last_ids,
            votes: votes_written,
        };
        let raw = serde_json::to_string_pretty(&nou).unwrap_or_default();
        fs::write(&state_path, raw).map_err(|e| xerr(e, &state_path))?;
        Ok(pack)
    }
}

fn read_state(path: &std::path::Path) -> SoupState {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<SoupState>(&raw).ok())
        .unwrap_or_default()
}

/// Temps ISO 8601 en UTC amb sufix `Z`: Soup rebutja qualsevol altra forma.
fn iso_utc(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

/// `id` canònic en minúscules, o `None` si no ho és (llavors la línia no
/// s'escriu: un identificador inventat trencaria el vincle amb el feedback).
fn uuid_lowercase(id: &str) -> Option<String> {
    uuid::Uuid::parse_str(id).ok().map(|u| u.to_string())
}

/// Una línia `conversation` (schema_version 1): el que el model va veure i el
/// que va respondre. El `id` és l'identificador del registre, que és on
/// apuntarà el feedback.
pub fn conversation_line(ex: &Exchange) -> Option<String> {
    let id = uuid_lowercase(&ex.id)?;
    // `model` és obligatori i no s'inventa: si el motor no n'ha reportat cap,
    // es posa el proveïdor, que és el que de veritat ha respost.
    let model = if !ex.model.trim().is_empty() {
        ex.model.trim().to_string()
    } else if !ex.provider.trim().is_empty() {
        ex.provider.trim().to_string()
    } else {
        "desconegut".to_string()
    };
    let line = serde_json::json!({
        "schema_version": 1,
        "id": id,
        "conversation_id": null,
        "ts": iso_utc(ex.at),
        "model": model,
        "version": null,
        "adapter": null,
        "messages": [{ "role": "user", "content": ex.question }],
        "response": ex.answer,
        // No mesurat ací: la regla de Soup és `null` abans que un 0 bonic.
        "latency_ms": null,
        "finish_reason": null,
        "tokens": { "prompt": null, "completion": null },
        "options": {},
        "source": SOUP_SOURCE,
    });
    Some(line.to_string())
}

/// Una línia `feedback`: el vot de l'usuari sobre una resposta concreta.
/// `None` vol dir «vot retirat», que Soup també expressa amb `value: null`.
pub fn feedback_line(response_id: &str, vote: Option<i8>, at: i64) -> Option<String> {
    let rid = uuid_lowercase(response_id)?;
    let value = match vote {
        Some(v) if v > 0 => serde_json::json!(1),
        Some(v) if v < 0 => serde_json::json!(-1),
        _ => serde_json::Value::Null,
    };
    Some(
        serde_json::json!({
            "schema_version": 1,
            "id": uuid::Uuid::new_v4().to_string(),
            "response_id": rid,
            "ts": iso_utc(at),
            "kind": "thumb",
            "value": value,
            "user_ref": "local",
            "source": SOUP_SOURCE,
        })
        .to_string(),
    )
}

/// Files del dataset: fora el buit, fora els 👎 i fora les repeticions
/// exactes (la darrera vegada guanya, que és la que l'usuari va tolerar).
/// Retorna les files (ordre cronològic) i quantes s'han descartat.
pub fn train_rows(exs: &[Exchange]) -> (Vec<Exchange>, usize) {
    let mut keep: Vec<Exchange> = Vec::new();
    let mut dup = 0usize;
    for ex in exs {
        if ex.question.trim().is_empty() || ex.answer.trim().is_empty() {
            continue;
        }
        if ex.vote == Some(-1) {
            continue;
        }
        let key = format!(
            "{}\u{1}{}",
            normalize_key(&ex.question),
            normalize_key(&ex.answer)
        );
        if let Some(i) = keep.iter().position(|e| {
            format!(
                "{}\u{1}{}",
                normalize_key(&e.question),
                normalize_key(&e.answer)
            ) == key
        }) {
            // La versió més nova substitueix l'anterior i es posa al final:
            // l'ordre importa, perquè la cua és el que Soup valida.
            let mut newest = ex.clone();
            newest.at = newest.at.max(keep[i].at);
            keep.remove(i);
            dup += 1;
            keep.push(newest);
            continue;
        }
        keep.push(ex.clone());
    }
    (keep, dup)
}

/// ChatML pur: UNA pregunta i UNA resposta, sense claus extras. La
/// documentació de Soup diu que qualsevol forma barallada al mateix fitxer
/// es refusa, així que el `metadata` d'abans marxa al costat, a `provenance`.
pub fn jsonl_chatml(rows: &[Exchange]) -> String {
    let mut out = String::new();
    for ex in rows {
        let rec = serde_json::json!({
            "messages": [
                { "role": "user", "content": ex.question },
                { "role": "assistant", "content": ex.answer },
            ],
        });
        out.push_str(&rec.to_string());
        out.push('\n');
    }
    out
}

/// Qui ha respost què i quan, en un fitxer apart. Així `train.jsonl` es pot
/// donar a Soup tal qual i la procedència no és una clau que ningú entén.
pub fn jsonl_provenance(rows: &[Exchange]) -> String {
    let mut out = String::new();
    for (i, ex) in rows.iter().enumerate() {
        let rec = serde_json::json!({
            "row": i,
            "id": ex.id,
            "provider": ex.provider,
            "model": ex.model,
            "at": ex.at,
            "vote": ex.vote,
        });
        out.push_str(&rec.to_string());
        out.push('\n');
    }
    out
}

/// Parelles de preferència (DPO): la mateixa pregunta amb una resposta 👍 i
/// una de 👎. Sense les dues no hi ha res a comparar, i no n'inventem.
pub fn jsonl_preference(exs: &[Exchange]) -> (String, usize) {
    let mut out = String::new();
    let mut n = 0usize;
    let mut seen: HashSet<String> = HashSet::new();
    for q in exs {
        let key = normalize_key(&q.question);
        if key.is_empty() || !seen.insert(key.clone()) {
            continue;
        }
        if q.vote != Some(1) {
            continue;
        }
        // El rebuig més recent de la mateixa pregunta.
        let bad = exs
            .iter()
            .rev()
            .find(|e| e.vote == Some(-1) && same_question(&normalize_key(&e.question), &key));
        let Some(bad) = bad else { continue };
        if bad.answer == q.answer {
            continue;
        }
        let rec = serde_json::json!({
            "prompt": q.question,
            "chosen": q.answer,
            "rejected": bad.answer,
        });
        out.push_str(&rec.to_string());
        out.push('\n');
        n += 1;
    }
    (out, n)
}

/// Preguntes que cauen a banda i banda de la divisió de validació: per a
/// Soup la cua del fitxer és la validació, així que una pregunta repetida
/// que toca les dues parts fa que «validar» sigui mirar la resposta.
pub fn fuita_val(rows: &[Exchange], split: f64) -> usize {
    if rows.len() < 2 || split <= 0.0 {
        return 0;
    }
    let nval = ((rows.len() as f64) * split).round() as usize;
    let nval = nval.clamp(1, rows.len() - 1);
    let cut = rows.len() - nval;
    let mut vals: HashSet<String> = HashSet::new();
    for e in &rows[cut..] {
        vals.insert(normalize_key(&e.question));
    }
    let mut hits = 0usize;
    for e in &rows[..cut] {
        let k = normalize_key(&e.question);
        if !k.is_empty() && vals.iter().any(|v| same_question(v, &k)) {
            hits += 1;
        }
    }
    hits
}

/// Nota de qualitat del dataset: què té i què li falta, sense maquillar-ho.
/// Cada línia diu el que es pot fer, no només que «és dolent».
pub fn scorecard(rows: &[Exchange], up: usize, down: usize) -> (f32, Vec<String>) {
    let mut lines = Vec::new();
    let mut score = 1.0f32;
    if rows.is_empty() {
        lines.push(
            "0 files: primer cal conversar amb la IA i després sincronitzar.".to_string(),
        );
        return (0.0, lines);
    }
    if rows.len() < MIN_FILS {
        lines.push(format!(
            "{} files: són poques per a un LoRA (amb {} ja es comença a notar). \
             Segueix xatejant i torna a sincronitzar quan n'hi hagi més.",
            rows.len(),
            MIN_FILS
        ));
        score -= 0.35;
    } else {
        lines.push(format!("{} files: sufficients per començar un LoRA.", rows.len()));
    }
    if up == 0 {
        lines.push(
            "cap resposta aprovada amb 👍: el dataset encara no diu què és BO, només què \
             s'ha respòs. Vota al xat abans d'entrenar.".to_string(),
        );
        score -= 0.35;
    } else {
        lines.push(format!("{} respostes aprovades amb 👍." , up));
    }
    if down == 0 {
        lines.push(
            "cap 👎: no es pot fer entrenament de preferències (DPO), que necessita el \
             que has rebutjat.".to_string(),
        );
        score -= 0.15;
    } else {
        lines.push(format!(
            "{} respostes rebutjades: van a `preference.jsonl` per a `task: dpo`.",
            down
        ));
    }
    let uniq: HashSet<String> = rows.iter().map(|e| normalize_key(&e.question)).collect();
    if uniq.len() * 2 < rows.len() {
        lines.push(format!(
            "molta repetició: {} preguntes distintes en {} files.",
            uniq.len(),
            rows.len()
        ));
        score -= 0.15;
    }
    let curtes = rows
        .iter()
        .filter(|e| e.answer.chars().count() < 40)
        .count();
    if curtes * 2 > rows.len() {
        lines.push(format!(
            "{} respostes de menys de 40 caràcters: un model entrenat amb això aprèn a \
             respondre curt.",
            curtes
        ));
        score -= 0.1;
    }
    (score.clamp(0.0, 1.0), lines)
}

/// `soup.yaml` per a `soup train`. NOMÉS claus documentades a
/// `docs/config-reference.md`: des de la v0.75 de Soup una clausa que no
/// hi siga REFUSA el fitxer sencer, així que cap «suggeriment» inventat.
pub fn yaml_sft(base: &str, rows: usize) -> String {
    let batch = if rows >= 8 { 4 } else { 1 };
    format!(
        "# Generat per NoOrbit (aprenentatge local) a {SOUP_DIR}/.\n\
         # soup train --config soup.yaml --dry-run   # valida config i dades sense entrenar\n\
         # soup train --config soup.yaml\n\n\
         base: {base}\n\
         task: sft\n\n\
         data:\n\
           train: train.jsonl\n\
           format: chatml\n\
           val_split: {val}\n\n\
         training:\n\
           epochs: 2\n\
           lr: 2.0e-4\n\
           batch_size: {batch}\n\
           quantization: \"4bit\"\n\
           lora:\n\
             r: 16\n\
             alpha: 32\n\n\
         output: ./output\n",
        base = yaml_str(base),
        val = VAL_SPLIT,
    )
}

/// El mateix per a DPO: la feina dels 👎. `task: dpo` és un dels valors
/// permesos i `data.format: dpo` és el que llegeix les files
/// `{prompt, chosen, rejected}`.
pub fn yaml_dpo(base: &str, rows: usize) -> String {
    let batch = if rows >= 8 { 2 } else { 1 };
    format!(
        "# Generat per NoOrbit: preferències (les respostes 👍 contra les 👎).\n\
         # soup train --config soup-preference.yaml\n\n\
         base: {base}\n\
         task: dpo\n\n\
         data:\n\
           train: preference.jsonl\n\
           format: dpo\n\
           val_split: {val}\n\n\
         training:\n\
           epochs: 1\n\
           lr: 5.0e-6\n\
           batch_size: {batch}\n\
           quantization: \"4bit\"\n\
           lora:\n\
             r: 16\n\
             alpha: 32\n\n\
         output: ./output-preference\n",
        base = yaml_str(base),
        val = VAL_SPLIT,
    )
}

/// Una cadena YAML entre cometes si cal (els noms de model porten dos punts,
/// que YAML llegiria com a mapa si van sols).
fn yaml_str(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        return "\"unknown\"".to_string();
    }
    if t.contains(':') || t.contains('#') || t.starts_with(' ') || t != t.to_lowercase() {
        format!("\"{}\"", t.replace('\\', "\\\\").replace('\"', "\\\""))
    } else {
        t.to_string()
    }
}

/// El que pot fer aquest equip i el que no, comprovat i no suposat: Soup
/// necessita Python 3.11+ i un `soup` instal·lat. Si falta alguna de les
/// dues coses ho diu amb la raó (la mateixa honestedat del `soup doctor`).
pub fn soup_pot_entrenar_aqui() -> (bool, String) {
    let soup = match std::process::Command::new("soup").arg("--version").output() {
        Ok(o) if o.status.success() => true,
        Ok(_) => return (false, "`soup` hi és però no respon a --version.".to_string()),
        Err(_) => return (
            false,
            "`soup` no està instal·lat en aquest equip (cap `pipx install soup-cli`).".to_string(),
        ),
    };
    let py = match std::process::Command::new("python3").arg("--version").output() {
        Ok(o) => {
            let txt = String::from_utf8_lossy(&o.stdout).to_string()
                + &String::from_utf8_lossy(&o.stderr).to_string();
            txt
        }
        Err(_) => return (
            soup,
            "no s'ha pogut executar `python3 --version`, així que no es pot confirmar la \
             versió de Python."
                .to_string(),
        ),
    };
    // «Python 3.9.6» → (3, 9). Si no s'entén, es diu que no es pot comprovar.
    let vers = py
        .split_whitespace()
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .and_then(|w| {
            let mut it = w.split('.');
            Some((it.next()?.parse::<u32>().ok()?, it.next()?.parse::<u32>().ok()?))
        });
    match vers {
        Some((major, minor)) if major == 3 && (11..=13).contains(&minor) => (
            true,
            format!("Python {major}.{minor} i `soup` instal·lat: es pot provar. {py}"),
        ),
        Some((major, minor)) if major == 3 && minor >= 14 => (
            true,
            format!(
                "Python {major}.{minor} és més nou del que Soup documenta (3.11-3.13): \
                 pot funcionar; confirma-ho amb `soup doctor`. {py}"
            ),
        ),
        Some((major, minor)) => (
            false,
            format!("Python {major}.{minor}: Soup demana 3.11, 3.12 o 3.13. {py}"),
        ),
        None => (
            true,
            format!(
                "`soup` instal·lat, però no s'ha entès la sortida de Python ({py}); \
                 comprova-ho amb `soup doctor`."
            ),
        ),
    }
}

/// Text que acompanya la carpeta: com entrenar, on, i què NO s'ha fet aquí.
pub fn readme_soup(p: &SoupPack) -> String {
    format!(
        "# El que has ensenyat a NoOrbit, en format entrenable\n\n\
         Aquesta carpeta (`.soup-loops/`) segueix els formats que documenta Soup\n\
         (https://github.com/MakazhanAlpamys/Soup, Apache-2.0). NoOrbit només escriu\n\
         dades: mai no entrena res per si sol, i menys encara si l'equip no hi cap.\n\n\
         ## Què hi ha\n\n\
         - `train.jsonl` — {rows} files ChatML pur (`messages`: user + assistant).\n\
         - `provenance.jsonl` — qui ha respost cada fila i amb quina marca de temps.\n\
         - `preference.jsonl` — {rowsdpo} parelles `{{prompt, chosen, rejected}}` (👍 vs 👎).\n\
         - `soup.yaml` / `soup-preference.yaml` — configuracions per a `soup train`.\n\
         - `traces/{SOUP_SOURCE}.jsonl` — línies `conversation` (schema_version 1).\n\
         - `feedback.jsonl` — línies `feedback` amb els teus vots.\n\
         - `noorbit-cursor.json` — què ja s'ha escrit (els fitxers són només-afegits).\n\n\
         ## Per entrenar-ho (cal un equip que hi cap)\n\n\
         ```\n\
         pipx install soup-cli            # o: pip install soup-cli\n\
         soup doctor                      # confirma GPU/Python abans de res\n\
         soup train --config soup.yaml --dry-run\n\
         soup train --config soup.yaml\n\
         soup export --model ./output --format gguf --quant q4_k_m --base {base}\n\
         ```\n\n\
         El GGUF que en surt torna a NoOrbit per la mateixa porta que els altres\n\
         models: Hugging Bay o el directori d'Ollama.\n\n\
         ## On som ara mateix\n\n\
         {check}\n\n\
         ## Qualitat del que s'hi ha guardat\n\n\
         Vots: {up} 👍 i {down} 👎. Files: {rows} ({dup} repeticions fora).\n\
         Fugues de validació: {leak} preguntes que surten a la vegada a l'entrenament i a\n\
         la validació (Soup reserva la cua del fitxer, així que repetir una pregunta la\n\
         fa sortir a banda i banda).\n\n\
         {score}\n\
         "
        ,
        rows = p.rows_train,
        rowsdpo = p.rows_preference,
        base = p.base_model,
        check = if p.can_train_here {
            format!("Sí que es pot entrenar ací: {}", p.train_check)
        } else {
            format!(
                "AQUEST EQUIP NO HI CAP: {}. Porta aquesta carpeta a un equip amb una GPU\n\
                 (CUDA o Apple Silicon) i Python 3.11+, i allà executa les ordes de dalt.\n\
                 NoOrbit NO baixa ni instal·la res per fer aquest camí.",
                p.train_check
            )
        },
        up = p.votes_up,
        down = p.votes_down,
        dup = p.duplicates,
        leak = p.leakage,
        score = p
            .scorecard
            .iter()
            .map(|l| format!("- {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;


    fn ex(q: &str, a: &str) -> Exchange {
        Exchange {
            id: "x".into(),
            question: q.into(),
            answer: a.into(),
            provider: "ollama".into(),
            model: "qwen3:4b".into(),
            at: 1,
            vote: None,
        }
    }

    /// Un intercanvi amb identificador REAL (els fitxers de Soup exigeixen
    /// UUID) i vot opcional.
    fn exv(q: &str, a: &str, vote: Option<i8>) -> Exchange {
        Exchange {
            id: uuid::Uuid::new_v4().to_string(),
            question: q.into(),
            answer: a.into(),
            provider: "ollama".into(),
            model: "qwen3:4b".into(),
            at: 2,
            vote,
        }
    }

    #[test]
    fn parses_the_lesson_format_of_the_distiller() {
        let raw = "\
LLIÇÓ | etiquetes: python, proves | L'usuari treballa amb pytest
- LLIÇÓ | rust | El projecte està fet amb Rust i Tauri
- El codi ha d'estar en català
CAP
";
        let ls = parse_lessons(raw, "model local");
        assert_eq!(ls.len(), 3);
        assert_eq!(ls[0].text, "L'usuari treballa amb pytest");
        assert_eq!(ls[0].tags, vec!["python", "proves"]);
        assert_eq!(ls[1].text, "El projecte està fet amb Rust i Tauri");
        assert_eq!(ls[1].tags, vec!["rust"]);
        assert_eq!(ls[2].text, "El codi ha d'estar en català");
    }

    #[test]
    fn heuristic_finds_preferences_without_any_model() {
        let ls = heuristic_lessons(&[ex(
            "Recorda que sempre treballo amb Rust i Tauri. Fes el login.",
            "D'acord.",
        )]);
        assert!(!ls.is_empty());
        assert!(ls[0].text.contains("sempre"));
        assert!(ls[0].tags.contains(&"preferència".to_string()));
    }

    #[test]
    fn context_only_injects_relevant_lessons() {
        let dir = std::env::temp_dir().join(format!("noorbit-learning-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = LearningStore::new(dir.clone());
        let mut a = Lesson {
            id: "a".into(),
            text: "El projecte està fet amb Rust i Tauri".into(),
            tags: vec!["rust".into()],
            source: "test".into(),
            hits: 0,
            at: 1,
        };
        let mut b = Lesson {
            id: "b".into(),
            text: "El postgresiu utilitza Postgres".into(),
            tags: vec!["sql".into()],
            source: "test".into(),
            hits: 0,
            at: 2,
        };
        a.at = 1;
        b.at = 2;
        store.add_lessons(vec![a, b], 10);
        let ctx = store.context_for("fes el login amb Rust", 3);
        assert!(ctx.contains("Rust i Tauri"));
        assert!(!ctx.contains("Postgres"));
        // Una consulta sense relació noinjecta soroll.
        let ctx2 = store.context_for("quin temps fa a Oslo", 3);
        assert!(ctx2.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_lessons_are_not_stored_twice() {
        let dir = std::env::temp_dir().join(format!("noorbit-learning-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = LearningStore::new(dir.clone());
        let mk = |id: &str| Lesson {
            id: id.into(),
            text: "L'usuari treballa amb Rust".into(),
            tags: vec![],
            source: "test".into(),
            hits: 0,
            at: 1,
        };
        assert_eq!(store.add_lessons(vec![mk("a")], 10), 1);
        assert_eq!(store.add_lessons(vec![mk("b")], 10), 0);
        assert_eq!(store.lessons().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// El cicle COMPLET sense cap IA: registre → lliçons (heurística) →
    /// injecció al context. És el que fa que l'aprenentatge funcioni
    /// encara que no tingui cap model instal·lat.
    #[test]
    fn full_cycle_without_any_model() {
        let dir = std::env::temp_dir().join(format!("noorbit-learning-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = LearningStore::new(dir.clone());
        store.record(
            "Recorda que sempre treballo amb Rust i Tauri, no amb Python.",
            "Entès, faré la feina amb Rust.",
            "ollama",
            "qwen3:4b",
        );
        assert_eq!(store.stats().exchanges, 1);
        assert_eq!(store.pending().len(), 1);
        // Destil·lació heurística (el que fa la reserva sense model).
        let lessons = heuristic_lessons(&store.pending());
        assert_eq!(store.add_lessons(lessons, 10), 1);
        store.mark_distilled_upto(store.pending().last().unwrap().at);
        assert!(store.pending().is_empty());
        // La lliçó troba el seu lloc al context d'una pregunta relacionada.
        let ctx = store.context_for("fes el projecte amb Rust", 3);
        assert!(ctx.contains("APRENENATGE LOCAL"));
        assert!(ctx.contains("Rust"));
        // El registre es pot exportar per entrenar un model local.
        let (path, n) = store.export_jsonl().unwrap();
        assert_eq!(n, 1);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"role\":\"user\"") && raw.contains("\"role\":\"assistant\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── el que ve de Soup: vots, formats i el que NO s'ha de callar ────────

    /// Un 👎 no pot ensenyar res: fora de la destil·lació i fora del dataset.
    #[test]
    fn a_thumbs_down_teach_nothing() {
        let dir = std::env::temp_dir().join(format!("noorbit-vote-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = LearningStore::new(dir.clone());
        let bo = store.record(
            "com faig una funcio de sum in rust",
            "pub fn sum(v: &[i32]) -> i32 { v.iter().sum() }",
            "ollama",
            "qwen3:4b",
        );
        let dolent = store.record("quants anys te", "Trenta-vuit.", "ollama", "qwen3:4b");
        assert!(store.vote(&bo.id, 1).is_some());
        assert!(store.vote(&dolent.id, -1).is_some());
        assert_eq!(store.votes(), (1, 1));
        assert_eq!(store.stats().voted_up, 1);
        assert_eq!(store.stats().voted_down, 1);
        assert_eq!(store.accepted(10).len(), 1);
        let pendents = store.pending_distillable();
        assert_eq!(pendents.len(), 1, "el 👎 no s'ha de destil·lar");
        assert_eq!(pendents[0].id, bo.id);
        // Tornar a votar el mateix no duplica res (ja esta escrit, i el vot
        // es pot retirar amb 0).
        assert!(store.vote(&dolent.id, 0).is_some());
        assert_eq!(store.votes(), (1, 0));
        assert!(store.vote("no-existeix", 1).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// El xat no guarda l'`id` del registre, nomes el text de la pregunta:
    /// votar per pregunta ha de trobar l'intercanvi mes recent, tot i que el
    /// registre el guardi retallat.
    #[test]
    fn voting_by_question_finds_the_latest_turn() {
        let dir = std::env::temp_dir().join(format!("noorbit-voteq-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = LearningStore::new(dir.clone());
        let q = "explica'm per que un Mac Intel amb vuit gigabytes de RAM no pot carregar un model de vuit mil milions de parametres sense quedar-se sense memoria";
        store.record(q, "la primera resposta.", "ollama", "qwen3:4b");
        let segon = store.record(q, "la segona, la bona.", "ollama", "qwen3:4b");
        let got = store.vote_by_question(q, 1).expect("ha de trobar la pregunta");
        assert_eq!(got.id, segon.id, "guanya la mes recent");
        assert_eq!(store.votes().0, 1);
        // Una pregunta diferent no pot ser votada per error.
        assert!(store
            .vote_by_question("i el temps a Oslo", 1)
            .is_none());
        // ni dues preguntes massa curtes per estar segurs.
        assert!(!same_question("si", "sip"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// El paquet sencer: fitxers de Soup que es poden rellegir, i una
    /// segona sincronitzacio que NO reescriu el que ja es al reg.
    #[test]
    fn soup_pack_is_append_only_and_readable() {
        let dir = std::env::temp_dir().join(format!("noorbit-soup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = LearningStore::new(dir.clone());
        let q = "com faig que el meu servidor de python escolti en un port lliure";
        let bo = store.record(q, "usa un port de 49152 a 65535 i comprova que sigui lliure.", "ollama", "qwen3:4b");
        let dolent = store.record(q, "usa sempre el port 80.", "ollama", "qwen3:4b");
        store.vote(&bo.id, 1);
        store.vote(&dolent.id, -1);

        let pack = store.soup_sync("qwen3:4b").expect("escriure");
        assert_eq!(pack.new_traces, 2, "dos torns nous");
        assert_eq!(pack.total_traces, 2);
        assert_eq!(pack.new_feedback, 2, "dos vots nous");
        assert_eq!(pack.rows_train, 1, "el 👎 no va al dataset");
        assert_eq!(pack.rows_preference, 1, "la mateixa pregunta amb un si i un no");
        assert!(pack.duplicates == 0);

        // Les línies de conversa: els camps que Soup fa obligatoris.
        let raw = std::fs::read_to_string(&pack.traces_path).unwrap();
        let línies: Vec<serde_json::Value> = raw
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(línies.len(), 2);
        for l in &línies {
            assert_eq!(l["schema_version"], 1);
            assert_eq!(l["source"], "noorbit");
            assert_eq!(l["model"], "qwen3:4b");
            assert!(l["ts"].as_str().unwrap().ends_with('Z'));
            assert_eq!(l["messages"][0]["role"], "user");
            // Cap numero inventat: el que no s'ha mesurat es null.
            assert!(l["tokens"]["prompt"].is_null());
            assert!(l["latency_ms"].is_null());
        }
        // El feedback apunta a la resposta amb el seu identificador.
        let fb = std::fs::read_to_string(&pack.feedback_path).unwrap();
        let ids: Vec<String> = fb
            .lines()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["response_id"]
                .as_str()
                .unwrap()
                .to_string())
            .collect();
        assert!(ids.contains(&bo.id) && ids.contains(&dolent.id));
        let valors: Vec<i64> = fb
            .lines()
            .filter_map(|l| {
                serde_json::from_str::<serde_json::Value>(l).ok()
                    .and_then(|v| v["value"].as_i64())
            })
            .collect();
        assert!(valors.contains(&1) && valors.contains(&-1));

        // ChatML PUR: cap clau estranya que Soup refusaria.
        let train = std::fs::read_to_string(&pack.train_path).unwrap();
        let fila: serde_json::Value =
            serde_json::from_str(train.lines().next().unwrap()).unwrap();
        assert_eq!(fila.as_object().unwrap().len(), 1, "nomes messages");
        assert_eq!(fila["messages"][1]["role"], "assistant");
        let parella: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&pack.preference_path).unwrap())
                .unwrap();
        assert!(parella["prompt"].is_string() && parella["chosen"].is_string());
        assert_ne!(parella["chosen"], parella["rejected"]);

        // Segona passada: el reg no creix, els datasets es regeneren igual.
        let altre = store.soup_sync("qwen3:4b").unwrap();
        assert_eq!(altre.new_traces, 0, "res nou que afegir");
        assert_eq!(altre.new_feedback, 0, "els vots no han canviat");
        assert_eq!(altre.total_traces, 2);
        assert_eq!(altre.rows_train, 1);

        // I ara un vot RETIRAT: aixo si que es una novetat per al reg.
        store.vote(&dolent.id, 0);
        let tercera = store.soup_sync("qwen3:4b").unwrap();
        assert_eq!(tercera.new_feedback, 1, "el retirament tambe es una linia");
        assert_eq!(tercera.rows_train, 2, "sense 👎 ja no es descarta");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// La configuracio nomes pot usar claus documentades: des de la v0.75
    /// Soup refusa el fitxer sencer si n'hi ha una que no coneix.
    #[test]
    fn the_yaml_only_uses_documented_keys() {
        let y = yaml_sft("qwen3:4b", 40);
        for k in [
            "base:",
            "task: sft",
            "train: train.jsonl",
            "format: chatml",
            "val_split: 0.1",
            "epochs: 2",
            "lr:",
            "batch_size: 4",
            "quantization:",
            "lora:",
            "r: 16",
            "alpha: 32",
            "output:",
        ] {
            assert!(y.contains(k), "falta {k}:\n{y}");
        }
        for inventada in ["learning_rate", "lora_rank", "lora_alpha", "dataset", "model:"] {
            assert!(!y.contains(inventada), "clau no documentada: {inventada}");
        }
        // Amb poques files el lot no pot ser mes gran que les dades.
        let petit = yaml_sft("qwen2.5:1.5b", 3);
        assert!(petit.contains("batch_size: 1"), "{petit}");
        // Els noms amb dos punts van entre cometes (si no, YAML els llegeix
        // com un mapa i Soup refusaria la configuracio).
        assert!(petit.contains("\"qwen2.5:1.5b\""), "{petit}");
        let d = yaml_dpo("qwen3:4b", 12);
        assert!(d.contains("task: dpo") && d.contains("format: dpo"));
    }

    /// Dos avisos que la interficie no pot amagar: les fugues de validacio i
    /// un dataset sense cap vot.
    #[test]
    fn leakage_and_scorecard_say_what_is_missing() {
        let mateixa = "explica'm com funciona la cua de claus d'un transformer quan el context es llarg";
        let files = vec![
            exv(mateixa, "resposta un", Some(1)),
            exv("pregunta dos sobre el port de blender", "resposta dos amb prou text per valer", None),
            exv(mateixa, "resposta tres", Some(1)),
            exv("pregunta tres sobre proves unitaries en rust", "resposta quatre llarga i util", None),
            exv(mateixa, "resposta cinc", Some(1)),
        ];
        // La cua (10% d'aixo) cau a les ultimes files: si la mateixa pregunta
        // ja hi era abans, la validacio no es cega.
        assert!(fuita_val(&files, VAL_SPLIT) >= 1, "fugues detectades");
        let (q, card) = scorecard(&files, 3, 0);
        assert!(q <= 1.0);
        assert!(card.iter().any(|l| l.contains("👎")), "{:?}", card);
        // Sense cap vot la nota diu que el dataset encara no distingeix el
        // que es bo del que nomes s'ha dit.
        let (qn, cn) = scorecard(&files, 0, 0);
        assert!(qn < q, "sense vots la nota es mes baixa");
        assert!(cn.iter().any(|l| l.contains("👍")), "{:?}", cn);
        let (_, c0) = scorecard(&[], 0, 0);
        assert!(c0[0].contains("0 files"));
    }

    /// Un identificador que no es UUID no pot anar al reg de Soup (el
    /// feedback no hi sabria apuntar): es compta, no s'inventa.
    #[test]
    fn non_uuid_ids_are_reported_not_invented() {
        let dir = std::env::temp_dir().join(format!("noorbit-suupid-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = LearningStore::new(dir.clone());
        let mut fals = ex("pregunta amb identificador fals", "resposta");
        fals.at = 1;
        { // el registre s'omple per `record`, així que aquí s'escriu a mà
            let mut d = store.data.lock().unwrap();
            d.exchanges.push(fals);
        }
        store.record("una pregunta real", "una resposta real", "ollama", "qwen3:4b");
        let pack = store.soup_sync("qwen3:4b").unwrap();
        assert_eq!(pack.skipped_ids, 1, "el fals es compta");
        assert_eq!(pack.new_traces, 1, "i no s'escriu");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
