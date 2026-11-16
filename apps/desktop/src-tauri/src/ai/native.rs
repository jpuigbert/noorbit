//! IA NATIVA de NoOrbit — un motor d'IA integrat que NO es baixa de cap lloc.
//!
//! A diferència d'Ollama (cal instal·lar el motor i baixar models GGUF) o dels
//! proveïdors en línia (cal token), la IA nativa viu DINS del binari de
//! NoOrbit: és codi Rust que ja hi és quan l'app s'instal·la. Funciona en
//! qualsevol Mac (també Intel, sense GPU) perquè no fa inferència neuronal:
//! és un motor simbòlic (avaluador matemàtic + recuperació de coneixement).
//!
//! Tres peces:
//!
//! 1. **Cervell** (`native_brain.json`): coneixement predefinit (matemàtiques,
//!    codi, conversa, fets de NoOrbit) més el que s'hi va incorporant. Es desa
//!    a la carpeta de dades i es carrega a cada resposta.
//! 2. **Aprenentatge**: reutilitza el sistema d'aprenentatge local existent
//!    (`crate::learning`). Les lliçons que la IA ja ha après de les converses
//!    amb qualsevol altra IA s'injecten al context i la nativa les fa servir.
//! 3. **Millora silenciosa**: quan l'usuari demana «millora el context
//!    matemàtic» (o reprogramar-la), la nativa delega la generació de
//!    coneixement en una IA open-source disponible — el model LOCAL d'Ollama
//!    (qwen3, sense baixar res si ja hi és) o, si no n'hi ha, un proveïdor
//!    gratuït en línia (Ollama Cloud, MiniMax…). El resultat es fusiona al
//!    cervell. L'usuari NO veu mai quina IA ho ha generat: la resposta és
//!    natural («He ampliat el meu context…») i cap event delata el teacher.

use crate::ai::SessionCtl;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// Identificador fix del proveïdor natiu (esembra a `ApiManager::new`).
pub const PROVIDER_ID: &str = "noorbit_native";
/// Nom visible a la interfície (en català).
pub const PROVIDER_NAME: &str = "IA nativa de NoOrbit";
/// «Model» que es mostra: no és un GGUF, és el motor integrat.
pub const MODEL_NAME: &str = "noorbit-native";
/// Tipus d'API del proveïdor natiu (ni openai ni anthropic).
pub const KIND: &str = "native";

/// Una peça de coneixement del cervell: un tema, les paraules que el
/// disparen i el text que la nativa respon quan coincideix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Knowledge {
    pub topic: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    pub content: String,
}

impl Knowledge {
    fn new(topic: &str, triggers: &[&str], content: &str) -> Self {
        Self {
            topic: topic.to_string(),
            triggers: triggers.iter().map(|s| s.to_string()).collect(),
            content: content.to_string(),
        }
    }
}

// ── Cervell persistent ────────────────────────────────────────────────────

/// Ruta del cervell après (el predefinit viu al binari, no al disc).
fn brain_path() -> Option<PathBuf> {
    crate::config::AppConfig::data_dir().map(|d| d.join("native_brain.json"))
}

/// Coneixement PREDEFINIT: ja hi és sense baixar res. Matemàtiques, codi,
/// conversa i fets de NoOrbit, tot en català (la llengua base de l'app).
pub fn builtin() -> Vec<Knowledge> {
    vec![
        Knowledge::new(
            "matemàtiques",
            &["matematic", "matematiques", "calcul", "calcula", "arithmetic", "math", "suma", "resta", "multiplica", "divideix", "arrel", "potencia", "equacio"],
            "Puc calcular directament: escriu una expressió com «2+2», «(3+4)*5», «sqrt(144)», «2^10» o «sin(pi/2)». \
             Admet + − × ÷ % ^, parèntesis i funcions (sqrt, sin, cos, tan, log, ln, abs, exp, floor, ceil, round) amb les constants pi i e.",
        ),
        Knowledge::new(
            "codi",
            &["codi", "code", "program", "programacio", "rust", "tauri", "typescript", "react", "python", "compila", "error"],
            "NoOrbit està fet amb Rust (Tauri v2) al backend i React + TypeScript + Zustand al frontend. \
             Puc explicar conceptes de programació i estructura del projecte; per generar o editar fitxers del projecte, \
             les IAs amb eines (local o en línia) materialitzen els canvis amb directrius «@file:» i «RUN|».",
        ),
        Knowledge::new(
            "noorbit",
            &["noorbit", "app", "aplicacio", "projecte", "que es", "què es", "funciona", "proveidor", "proveidors", "ollama", "hugging", "bay"],
            "NoOrbit és un IDE amb IA integrada: editor Monaco, terminal, git, agents especialistes, Blender i Unreal. \
             La IA pot ser local (Ollama), en línia (proveïdors amb token) o aquesta IA nativa, que no necessita cap descàrrega. \
             Hugging Bay cerca i baixa models GGUF; l'aprenentatge local destil·la lliçons de les converses.",
        ),
        Knowledge::new(
            "identitat",
            &["qui ets", "que ets", "què ets", "com et dius", "ets una ia", "ets un bot", "who are you", "nativa", "native"],
            "Sóc la IA nativa de NoOrbit: un motor integrat a l'aplicació, sense descàrregues ni tokens. \
             Funciono en aquest Mac amb un cervell de coneixement que pot créixer amb l'ús i l'aprenentatge local.",
        ),
        Knowledge::new(
            "ajuda",
            &["ajuda", "help", "que pots fer", "què pots fer", "com funciona", "habilitats", "capacitats"],
            "Puc: calcular expressions matemàtiques, respondre sobre el projecte i conceptes de codi, \
             i conversar en català. El meu coneixement s'amplia amb l'aprenentatge local i quan em demanes millorar un tema.",
        ),
    ]
}

/// Carrega el cervell: predefinit + après (fusionats, sense duplicats).
pub fn load_brain() -> Vec<Knowledge> {
    let mut brain = builtin();
    if let Some(path) = brain_path() {
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(learned) = serde_json::from_str::<Vec<Knowledge>>(&raw) {
                for k in learned {
                    if !brain.iter().any(|b| same_knowledge(b, &k)) {
                        brain.push(k);
                    }
                }
            }
        }
    }
    brain
}

/// Fusiona coneixement nou al cervell persistent (dedup) i retorna quantes
/// peces noves s'han acceptat. Silenciós: no emet cap event.
pub fn merge_knowledge(incoming: Vec<Knowledge>) -> usize {
    if incoming.is_empty() {
        return 0;
    }
    let mut brain = load_brain();
    let mut added = 0usize;
    for k in incoming {
        if k.content.trim().len() < 8 {
            continue;
        }
        if brain.iter().any(|b| same_knowledge(b, &k)) {
            continue;
        }
        brain.push(k);
        added += 1;
    }
    if added > 0 {
        // Desa només l'aprés (el predefinit ja viu al binari): el fitxer es
        // manté petit i load_brain el torna a fusionar amb builtin().
        let learned: Vec<Knowledge> = brain
            .iter()
            .filter(|k| !builtin().iter().any(|b| same_knowledge(b, k)))
            .cloned()
            .collect();
        if let Some(path) = brain_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(raw) = serde_json::to_string_pretty(&learned) {
                // Escriptura atòmica: un procés que mori a mig `write` no deixa
                // el cervell truncat (load_brain ho ignoraria i es perdria tot).
                let tmp = path.with_extension("json.tmp");
                let _ = std::fs::write(&tmp, raw).and_then(|_| std::fs::rename(&tmp, &path));
            }
        }
    }
    added
}

fn same_knowledge(a: &Knowledge, b: &Knowledge) -> bool {
    let key = |s: &str| {
        s.to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    key(&a.topic) == key(&b.topic) && key(&a.content) == key(&b.content)
}

// ── Avaluador matemàtic (descens recursiu) ────────────────────────────────

/// Avalua una expressió aritmètica. Retorna None si no és vàlida (no és una
/// expressió matemàtica, divisió per zero, funció desconeguda…).
pub fn eval_math(expr: &str) -> Option<f64> {
    let chars: Vec<char> = expr.chars().collect();
    let mut p = Parser { chars, pos: 0 };
    p.skip_ws();
    let v = p.expr()?;
    p.skip_ws();
    if p.pos != p.chars.len() {
        return None; // sobra text: no és una expressió neta
    }
    if v.is_finite() {
        Some(v)
    } else {
        None
    }
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }
    fn eat(&mut self, c: char) -> bool {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    /// expr := term (('+' | '-') term)*
    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('+') => {
                    self.pos += 1;
                    v += self.term()?;
                }
                Some('-') => {
                    self.pos += 1;
                    v -= self.term()?;
                }
                _ => break,
            }
        }
        Some(v)
    }
    /// term := factor (('*' | '/' | '%') factor)*
    fn term(&mut self) -> Option<f64> {
        let mut v = self.factor()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('*') => {
                    self.pos += 1;
                    v *= self.factor()?;
                }
                Some('/') => {
                    self.pos += 1;
                    let d = self.factor()?;
                    if d == 0.0 {
                        return None;
                    }
                    v /= d;
                }
                Some('%') => {
                    self.pos += 1;
                    let d = self.factor()?;
                    if d == 0.0 {
                        return None;
                    }
                    v %= d;
                }
                _ => break,
            }
        }
        Some(v)
    }
    /// factor := ('+' | '-') factor | power
    fn factor(&mut self) -> Option<f64> {
        self.skip_ws();
        match self.peek() {
            Some('-') => {
                self.pos += 1;
                Some(-self.factor()?)
            }
            Some('+') => {
                self.pos += 1;
                self.factor()
            }
            _ => self.power(),
        }
    }
    /// power := atom ('^' factor)?  (associatiu per la dreta)
    fn power(&mut self) -> Option<f64> {
        let base = self.atom()?;
        self.skip_ws();
        if self.peek() == Some('^') {
            self.pos += 1;
            let exp = self.factor()?;
            Some(base.powf(exp))
        } else {
            Some(base)
        }
    }
    /// atom := nombre | '(' expr ')' | funció '(' expr ')' | constant
    fn atom(&mut self) -> Option<f64> {
        self.skip_ws();
        match self.peek() {
            Some('(') => {
                self.pos += 1;
                let v = self.expr()?;
                if !self.eat(')') {
                    return None;
                }
                Some(v)
            }
            Some(c) if c.is_ascii_digit() || c == '.' => self.number(),
            Some(c) if c.is_alphabetic() => self.name(),
            _ => None,
        }
    }
    fn number(&mut self) -> Option<f64> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.') {
            self.pos += 1;
        }
        // Notació científica: 1e3, 2.5e-4
        if matches!(self.peek(), Some('e') | Some('E')) {
            let save = self.pos;
            self.pos += 1;
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.pos += 1;
            }
            if matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    self.pos += 1;
                }
            } else {
                self.pos = save; // no era exponent: torna enrere
            }
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse::<f64>().ok()
    }
    fn name(&mut self) -> Option<f64> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_alphabetic()) {
            self.pos += 1;
        }
        let name: String = self.chars[start..self.pos].iter().collect::<String>().to_lowercase();
        self.skip_ws();
        // Constants
        match name.as_str() {
            "pi" => return Some(std::f64::consts::PI),
            "e" => return Some(std::f64::consts::E),
            _ => {}
        }
        // Funcions: nom '(' expr ')'
        if !self.eat('(') {
            return None;
        }
        let arg = self.expr()?;
        if !self.eat(')') {
            return None;
        }
        let v = match name.as_str() {
            "sqrt" => {
                if arg < 0.0 {
                    return None;
                }
                arg.sqrt()
            }
            "abs" => arg.abs(),
            "sin" => arg.sin(),
            "cos" => arg.cos(),
            "tan" => arg.tan(),
            "exp" => arg.exp(),
            "ln" => {
                if arg <= 0.0 {
                    return None;
                }
                arg.ln()
            }
            "log" => {
                if arg <= 0.0 {
                    return None;
                }
                arg.log10()
            }
            "floor" => arg.floor(),
            "ceil" => arg.ceil(),
            "round" => arg.round(),
            _ => return None,
        };
        Some(v)
    }
}

/// Extreu una expressió matemàtica d'un prompt (o None si no n'hi ha).
/// Camí net («calcula 2+2», «(3+4)*5») i finestra de tokens encastada
/// («fes 2+2 si us plau»). Evita falsos positius com «projecte 2024-2025».
pub fn extract_math(prompt: &str) -> Option<String> {
    let norm = prompt
        .to_lowercase()
        .replace('×', "*")
        .replace('÷', "/")
        .replace('−', "-")
        .replace('·', "*");
    let mut s = norm.trim();
    for prefix in [
        "calcula ", "calculate ", "compute ", "quant és ", "quant es ", "cuanto es ",
        "resultat de ", "resol ", "evalua ", "avalua ", "what is ", "how much is ", "fes ",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
            break;
        }
    }
    let s = s.trim_end_matches(['?', '=', '.', '!', ' ']).trim();
    // Camí net: tot el prompt és una expressió.
    if math_candidate(s) {
        return Some(s.to_string());
    }
    // Finestra de tokens: busca la subseqüència més llarga que siga una
    // expressió vàlida (cada token ha de ser «matemàtic»).
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.len() > 40 {
        return None;
    }
    let mut best: Option<String> = None;
    for i in 0..tokens.len() {
        for j in (i + 1)..=tokens.len() {
            let window = tokens[i..j].join(" ");
            if math_candidate_in_prose(&window) {
                if best.as_ref().map(|b| window.len() > b.len()).unwrap_or(true) {
                    best = Some(window);
                }
            }
        }
    }
    best
}

/// versió «dins d'una frase» de `math_candidate`: exigeix un operador FORT
/// (+ * / ^ %) o un menys separat per espais. Així «el projecte 2024-2025»,
/// «v2.3-4» o un rang de dates no es confonen amb una resta.
fn math_candidate_in_prose(s: &str) -> bool {
    let t = s.trim();
    let strong = t.chars().any(|c| matches!(c, '+' | '*' | '/' | '^' | '%'));
    let spaced_minus = t.contains(" - ");
    if !strong && !spaced_minus {
        return false;
    }
    math_candidate(t)
}

/// Un text és una expressió matemàtica candidate? Només caràcters permesos,
/// cap paraula desconeguda, almenys un dígit i un operador binari, i avalua.
fn math_candidate(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || !t.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    if !t.chars().any(|c| matches!(c, '+' | '*' | '/' | '^' | '%')) && !t.contains('-') {
        return false;
    }
    // Cap lletra que no forme una funció/constant coneguda.
    let allowed = ["sqrt", "sin", "cos", "tan", "log", "ln", "abs", "exp", "floor", "ceil", "round", "pi", "e"];
    let mut word = String::new();
    for c in t.chars() {
        if c.is_alphabetic() {
            word.push(c);
        } else {
            if !word.is_empty() {
                if !allowed.contains(&word.as_str()) {
                    return false;
                }
                word.clear();
            }
            if !matches!(c, '0'..='9' | '+' | '-' | '*' | '/' | '^' | '%' | '(' | ')' | '.' | ' ') {
                return false;
            }
        }
    }
    if !word.is_empty() && !allowed.contains(&word.as_str()) {
        return false;
    }
    eval_math(t).is_some()
}

/// Formata un resultat numèric net (sense cua de zeros flotants).
fn format_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 && v.abs() < 1e15 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{:.6}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ── Resposta (matemàtiques → coneixement → lliçons → conversa) ────────────

/// Resposta de la IA nativa a un prompt. `brain` és el cervell carregat i
/// `lessons` el bloc d'aprenentatge local injectat al system prompt (pot ser
/// buit). Tot local, sense xarxa.
pub fn answer(prompt: &str, brain: &[Knowledge], lessons: &str) -> String {
    let p = prompt.trim();
    // 1. Matemàtiques: si hi ha una expressió, la calcula de veritat.
    if let Some(expr) = extract_math(p) {
        if let Some(v) = eval_math(&expr) {
            return format!("{} = {}", expr.trim(), format_num(v));
        }
    }
    let q: std::collections::HashSet<String> = crate::learning::tokens(p).into_iter().collect();
    // 2. Coneixement del cervell i 3. aprenentatge local: es comparen ELS DOS
    // i guanya el que cobreix més paraules de la pregunta. Les lliçons van
    // abans en cas d'empat perquè són dades CONCRETES de l'usuari, mentre que
    // el cervell predefinit és documentació genèrica de NoOrbit.
    let brain_hit = best_match(brain, &q);
    let lesson_hit = best_lesson(lessons, &q);
    let brain_cov = brain_hit
        .map(|k| {
            let by_trigger = k
                .triggers
                .iter()
                .map(|t| covered(t, &q))
                .max()
                .unwrap_or(0);
            by_trigger.max(covered(&k.topic, &q))
        })
        .unwrap_or(0);
    if let Some(line) = &lesson_hit {
        if covered(line, &q) >= brain_cov {
            return format!("Segons el que he après: {}", line);
        }
    }
    if let Some(k) = brain_hit {
        return k.content.clone();
    }
    // 4. Conversa (salutacions, agraïments, comiat).
    if let Some(reply) = small_talk(p) {
        return reply.to_string();
    }
    // 5. Reserva honesta: el motor natiu no genera text lliure com un LLM.
    "Sóc la IA nativa de NoOrbit (motor integrat, sense descàrregues). Puc calcular \
     expressions matemàtiques i respondre sobre el projecte, codi i el meu funcionament. \
     Per a tasques de generació llarga o edició de fitxers, tria una IA local (Ollama) o en \
     línia al selector de proveïdors. Si vols que amplïi el meu coneixement sobre un tema, \
     demana-m'ho (p. ex. «millora el context matemàtic»)."
        .to_string()
}

/// Quantes paraules de la consulta `q` apareixen a `text` (llindar comú per
/// comparar el cervell predefinit amb les lliçons apreses).
fn covered(text: &str, q: &std::collections::HashSet<String>) -> usize {
    crate::learning::tokens(text)
        .into_iter()
        .filter(|w| q.contains(w))
        .count()
}

fn best_match<'a>(brain: &'a [Knowledge], q: &std::collections::HashSet<String>) -> Option<&'a Knowledge> {
    let mut best: Option<(f32, &Knowledge)> = None;
    for k in brain {
        let mut score = 0.0f32;
        for t in &k.triggers {
            for w in crate::learning::tokens(t) {
                if q.contains(&w) {
                    score += 2.0;
                }
            }
        }
        for w in crate::learning::tokens(&k.topic) {
            if q.contains(&w) {
                score += 1.5;
            }
        }
        if score >= 2.0 && best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
            best = Some((score, k));
        }
    }
    best.map(|(_, k)| k)
}

/// Extreu la millor lliçó del bloc d'aprenentatge («- text (tags)»).
fn best_lesson(lessons: &str, q: &std::collections::HashSet<String>) -> Option<String> {
    let mut best: Option<(f32, String)> = None;
    for line in lessons.lines() {
        let l = line.trim().trim_start_matches(['-', '*', '•']).trim();
        if l.is_empty() || l.starts_with("[APRENENATGE") {
            continue;
        }
        let text = l.split(" (").next().unwrap_or(l).trim();
        let mut score = 0.0f32;
        for w in crate::learning::tokens(text) {
            if q.contains(&w) {
                score += 1.0;
            }
        }
        if score >= 2.0 && best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
            best = Some((score, text.to_string()));
        }
    }
    best.map(|(_, t)| t)
}

fn small_talk(p: &str) -> Option<&'static str> {
    let low = p.to_lowercase();
    let t = low.trim();
    if matches!(t, "hola" | "bon dia" | "bona tarda" | "hi" | "hello" | "hey" | "ei") {
        return Some("Hola! Sóc la IA nativa de NoOrbit. En què et puc ajudar?");
    }
    if t.contains("gràcies") || t.contains("gracies") || t.contains("thanks") {
        return Some("De res! Si necessites res més, aquí em tens.");
    }
    if matches!(t, "adéu" | "adeu" | "bye" | "fins aviat") {
        return Some("Adéu! Bona feina amb NoOrbit.");
    }
    if t.contains("com estàs") || t.contains("com estas") || t.contains("how are you") {
        return Some("Funcionant correctament, sense necessitat de descàrregues. Què necessites?");
    }
    None
}

/// Extreu el bloc d'aprenentatge local del system prompt (l'última secció,
/// afegida per `chat_with_tools`). Així la nativa «aprofita l'aprenentatge»
/// sense cap plumbing nou: reutilitza el que ja s'injecta a totes les IAs.
pub fn lessons_from_system(system: Option<&str>) -> String {
    let Some(s) = system else {
        return String::new();
    };
    match s.find("[APRENENATGE LOCAL]") {
        Some(idx) => s[idx..].trim().to_string(),
        None => String::new(),
    }
}

// ── Millora silenciosa (reprogramació amb altres IAs) ─────────────────────

/// Detecta una petició de millora/reprogramació del PROPI motor i en treu el
/// tema. Precís: no segresta tasques normals («millora el codi del projecte»
/// no és una millora del context de la nativa).
pub fn improvement_request(prompt: &str) -> Option<String> {
    let low = prompt.to_lowercase();
    let reprogram = low.contains("reprogram");
    let improve_verb = ["millora", "millorar", "millores", "improve", "mejora", "amplia", "ampliar", "expand", "augmenta"]
        .iter()
        .any(|v| low.contains(v));
    let self_target = ["context", "coneixement", "knowledge", "habilitat", "skill", "teu", "teva", "your", "matemàtic", "matematic", "math", "cervell", "brain"]
        .iter()
        .any(|v| low.contains(v));
    let learn_about = low.contains("aprèn sobre") || low.contains("apren sobre") || low.contains("learn about") || low.contains("aprende sobre");
    if !(reprogram || (improve_verb && self_target) || learn_about) {
        return None;
    }
    Some(detect_topic(&low))
}

fn detect_topic(low: &str) -> String {
    const TOPICS: &[(&[&str], &str)] = &[
        (&["matemàtic", "matematic", "math", "calcul", "arithmetic", "àlgebra", "algebra", "geometr"], "matemàtiques"),
        (&["física", "fisica", "physics"], "física"),
        (&["química", "quimica", "chemistry"], "química"),
        (&["codi", "code", "program", "rust", "tauri", "typescript", "react", "python"], "codi"),
        (&["història", "historia", "history"], "història"),
        (&["llengua", "català", "catala", "gramàtica", "gramatica", "language"], "llengua"),
        (&["noorbit", "projecte", "app", "aplicacio"], "noorbit"),
    ];
    for (keys, topic) in TOPICS {
        if keys.iter().any(|k| low.contains(k)) {
            return topic.to_string();
        }
    }
    "general".to_string()
}

/// Instruccions del teacher (la IA open-source que genera el coneixement).
/// Molt estrictes: només JSON, en català, sense explicar res.
pub const IMPROVE_SYSTEM: &str = "Ets el millorador de coneixement de la IA nativa de NoOrbit. \
Generes coneixement reutilitzable en català sobre el tema que es demana.

Regles estrictes:
- Respon NOMÉS amb un array JSON vàlid, sense cap text addicional ni markdown.
- Cada element: {\"topic\": \"tema\", \"triggers\": [\"paraula1\", \"paraula2\"], \"content\": \"explicació en català\"}.
- «triggers»: paraules clau en minúscules que disparen aquest coneixement (3-8).
- «content»: 1-3 frases útils i correctes en català (màxim 60 paraules).
- Genera entre 3 i 6 elements. No inventis dades; si no saps el tema, respon [].";

/// Munta el prompt que es passa al teacher.
pub fn build_improve_prompt(topic: &str, request: &str) -> String {
    format!(
        "Tema a millorar: {topic}.\nPetició de l'usuari: «{request}».\n\n\
         Genera coneixement nou i útil sobre aquest tema per al cervell de la IA nativa. \
         Respon NOMÉS amb l'array JSON demanat (o [] si no pots aportar res)."
    )
}

/// Converteix la resposta del teacher en coneixement. Tolerant: accepta JSON
/// net, JSON dins de markdown o text, i un format de línies de reserva.
pub fn parse_knowledge(raw: &str) -> Vec<Knowledge> {
    let t = raw.trim();
    // Extreu el bloc JSON entre el primer '[' i l'últim ']'.
    if let (Some(a), Some(b)) = (t.find('['), t.rfind(']')) {
        if b > a {
            if let Ok(v) = serde_json::from_str::<Vec<Knowledge>>(&t[a..=b]) {
                return v.into_iter().filter(|k| k.content.trim().len() >= 8).collect();
            }
        }
    }
    // Reserva: línies «tema | paraules | contingut».
    let mut out = Vec::new();
    for line in t.lines() {
        let l = line.trim().trim_start_matches(['-', '*', '•']).trim();
        if l.is_empty() || l.starts_with('[') || l.starts_with('{') {
            continue;
        }
        let parts: Vec<&str> = l.split('|').map(|p| p.trim()).collect();
        if parts.len() >= 3 && parts[2].len() >= 8 {
            out.push(Knowledge {
                topic: parts[0].to_string(),
                triggers: parts[1].split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect(),
                content: parts[2..].join(" | "),
            });
        }
    }
    out
}

/// Missatge natural de millora (NO esmenta mai cap altra IA).
pub fn improved_message(topic: &str, n: usize) -> String {
    if n > 0 {
        format!(
            "He ampliat el meu context sobre {topic}: he incorporat {n} conceptes nous al meu cervell. \
             Ja puc respondre millor sobre aquest tema. Prova de preguntar-m'hi."
        )
    } else {
        format!(
            "He revisat el meu context sobre {topic} i ja estava al dia. \
             Si vols, concreta què vols que hi afegeixi i ho tornaré a mirar."
        )
    }
}

/// Coneixement de reserva per a la millora OFFLINE (sense cap IA disponible):
/// retorna les peces predefinides relacionades amb el tema. Silenciós.
pub fn builtin_topic_knowledge(topic: &str) -> Vec<Knowledge> {
    let t = topic.to_lowercase();
    builtin()
        .into_iter()
        .filter(|k| k.topic.to_lowercase().contains(&t) || t.contains(&k.topic.to_lowercase()) || t == "general")
        .collect()
}

// ── Streaming (events del xat, com qualsevol altre proveïdor) ─────────────

/// Emet una resposta nativa token a token («ai://chunk» + «ai://process»),
/// igual que Ollama o els remots, perquè la UI la mostre en directe. La
/// cancel·lació és la del XAT que ha enviat el missatge.
pub async fn emit_stream(
    app: &tauri::AppHandle,
    ctl: &Arc<SessionCtl>,
    prompt: &str,
    text: &str,
    start: std::time::Instant,
) {
    let label = PROVIDER_NAME;
    let model = MODEL_NAME;
    // Paraules (amb l'espai) com a «tokens»: streaming natural i barat.
    let mut pieces: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        cur.push(c);
        if c == ' ' || c == '\n' {
            pieces.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        pieces.push(cur);
    }
    for piece in pieces {
        if ctl.is_cancelled() {
            return;
        }
        let ms = start.elapsed().as_millis() as u64;
        ctl.chunk(app, &piece);
        ctl.process(app, label, model, "streaming", &piece, ms);
        tokio::time::sleep(std::time::Duration::from_millis(8)).await;
    }
    let ms = start.elapsed().as_millis() as u64;
    ctl.process(app, label, model, "done", "", ms);
    // Comptabilització estimada (~4 caràcters per token), com els remots.
    let input = (prompt.chars().count() / 4).max(1) as u64;
    let output = (text.chars().count() / 4).max(1) as u64;
    crate::ai::emit_usage(app, &ctl.id, KIND, model, input, output, None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avalua_expressions_basiques() {
        assert_eq!(eval_math("2+2"), Some(4.0));
        assert_eq!(eval_math("(3+4)*5"), Some(35.0));
        assert_eq!(eval_math("2^10"), Some(1024.0));
        assert_eq!(eval_math("10/4"), Some(2.5));
        assert_eq!(eval_math("7%3"), Some(1.0));
        assert_eq!(eval_math("-2^2"), Some(-4.0));
        assert_eq!(eval_math("2^-2"), Some(0.25));
    }

    #[test]
    fn avalua_funcions_i_constants() {
        assert_eq!(eval_math("sqrt(144)"), Some(12.0));
        assert!((eval_math("sin(pi/2)").unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(eval_math("abs(-5)"), Some(5.0));
        assert_eq!(eval_math("ln(e)"), Some(1.0));
        assert_eq!(eval_math("log(1000)"), Some(3.0));
        assert_eq!(eval_math("floor(3.7)"), Some(3.0));
    }

    #[test]
    fn rebutja_el_que_no_es_matematic() {
        assert_eq!(eval_math("hola"), None);
        assert_eq!(eval_math("2+"), None);
        assert_eq!(eval_math("1/0"), None);
        assert_eq!(eval_math("sqrt(-1)"), None);
        assert_eq!(eval_math("2 2"), None);
        assert_eq!(eval_math("foo(2)"), None);
    }

    #[test]
    fn extreu_matematiques_del_prompt() {
        assert_eq!(extract_math("calcula 2+2").as_deref(), Some("2+2"));
        assert_eq!(extract_math("quant és (3+4)*5?").as_deref(), Some("(3+4)*5"));
        assert_eq!(extract_math("2+2").as_deref(), Some("2+2"));
        assert_eq!(extract_math("fes 12/4 si us plau").as_deref(), Some("12/4"));
        // Falsos positius: text amb números i guions NO és una expressió.
        assert_eq!(extract_math("el projecte 2024-2025"), None);
        assert_eq!(extract_math("hola com estàs"), None);
    }

    #[test]
    fn resposta_matematica_i_de_coneixement() {
        let brain = builtin();
        assert_eq!(answer("calcula 2+2", &brain, ""), "2+2 = 4");
        let r = answer("què pots fer en matemàtiques?", &brain, "");
        assert!(r.contains("sqrt"));
        let r = answer("qui ets?", &brain, "");
        assert!(r.contains("nativa"));
    }

    #[test]
    fn fa_servir_les_llissons_apreses() {
        let brain = builtin();
        let lessons = "[APRENENATGE LOCAL] Lliçons:\n- L'usuari treballa amb Rust i Tauri (rust, projecte)";
        let r = answer("recordes amb què treballa l'usuari rust tauri?", &brain, lessons);
        assert!(r.contains("Rust i Tauri"));
    }

    #[test]
    fn detecta_peticions_de_millora() {
        assert_eq!(improvement_request("millora el context matemàtic").as_deref(), Some("matemàtiques"));
        assert_eq!(improvement_request("reprograma't amb més coneixement de física").as_deref(), Some("física"));
        assert!(improvement_request("millora el teu context").is_some());
        // Tasques normals NO són millores del motor.
        assert!(improvement_request("millora el codi del projecte").is_none());
        assert!(improvement_request("fes un login").is_none());
    }

    #[test]
    fn parseja_el_json_del_teacher() {
        let raw = "```json\n[{\"topic\":\"matemàtiques\",\"triggers\":[\"derivada\"],\"content\":\"La derivada mesura la taxa de canvi d'una funció.\"}]\n```";
        let ks = parse_knowledge(raw);
        assert_eq!(ks.len(), 1);
        assert_eq!(ks[0].topic, "matemàtiques");
        assert!(ks[0].content.contains("derivada"));
        // Format de línies de reserva.
        let ks2 = parse_knowledge("física | gravetat, newton | La gravetat atrau els cossos amb massa.");
        assert_eq!(ks2.len(), 1);
        assert_eq!(ks2[0].topic, "física");
    }

    #[test]
    fn missatge_de_millora_no_delata_cap_ia() {
        let m = improved_message("matemàtiques", 3);
        assert!(m.contains("context"));
        assert!(!m.to_lowercase().contains("ollama"));
        assert!(!m.to_lowercase().contains("minimax"));
        assert!(!m.to_lowercase().contains("teacher"));
        assert!(!m.to_lowercase().contains("openai"));
    }

    #[test]
    fn extreu_les_llissons_del_system() {
        let sys = "Ets l'assistent.\n\nRUN|manual\n\n[APRENENATGE LOCAL] Lliçons:\n- Treballa amb Rust";
        let l = lessons_from_system(Some(sys));
        assert!(l.starts_with("[APRENENATGE LOCAL]"));
        assert!(l.contains("Rust"));
        assert!(!l.contains("RUN|"));
        assert_eq!(lessons_from_system(None), "");
    }

    #[test]
    fn fusiona_i_deduplica_el_cervell() {
        let dir = std::env::temp_dir().join(format!("noorbit-native-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // Simula el cervell en un directori temporal escrivint directament.
        let path = dir.join("native_brain.json");
        let k = Knowledge::new("tema", &["trigger"], "Contingut de prova prou llarg.");
        std::fs::write(&path, serde_json::to_string(&vec![k.clone()]).unwrap()).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        let loaded: Vec<Knowledge> = serde_json::from_str(&raw).unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(same_knowledge(&loaded[0], &k));
        let _ = std::fs::remove_dir_all(&dir);
    }
}