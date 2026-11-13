//! Separació del **raonament** (pensament en veu alta) de la resposta quan un
//! model escriu les dues coses pel mateix canal. Lògica PURA de text: sense
//! Tauri ni estat, es pot testejar sola.

/// Etiquetes que alguns models empren per a escriure el raonament DINS del seu
/// propi text (formats antics de DeepSeek, models a què es demana «pensament en
/// veu alta»…). La primera component OBRI el pensament i la segona el TANCA.
/// S'escriuen per parts amb `concat!` perquè aquest fitxer no continga mai la
/// marca literal sencera (confongria cerques i resums).
const REASON_TAGS: [(&str, &str); 2] = [
    (concat!("<", "think", ">"), concat!("<", "/think", ">")),
    (
        concat!("<", "reasoning", ">"),
        concat!("<", "/reasoning", ">"),
    ),
];

/// Paraules que obren/tanquen el pensament quan el model no usa etiquetes.
const REASON_OPEN_WORDS: [&str; 5] = ["reason:", "thinking:", "think:", "pensament:", "pensa:"];
const REASON_CLOSE_WORDS: [&str; 3] = ["answer:", "respon:", "respuesta:"];

/// Cerca `needle` (ASCII) dins de `haystack` ignorant majúscules i sense
/// alterar els índexs. `to_lowercase()` NO serveix aquí: pot canviar la
/// longitud en bytes (p. ex. «İ» → «i̇»), i els índexs trobats a la versió
/// en minúscules no serien vàlids per tallar l'original (pànic de char
/// boundary o tall al lloc equivocat).
pub(crate) fn find_ignore_ascii_case(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.len() > h.len() {
        return None;
    }
    // Els bytes ASCII no poden ser part d'un caràcter UTF-8 multi-byte: la
    // cerca a nivell de byte mai talla un caràcter pel mig.
    (0..=(h.len() - n.len())).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// Separa el raonament que un model escriu MESCLAT amb la seua resposta.
/// Retorna (pensament, resposta); si no hi ha cap senyal, el text sencer és la
/// resposta i el pensament queda buit.
pub(crate) fn split_inline_reasoning(text: &str) -> (String, String) {
    // 1) hi ha una ETIQUETA DE TANCAMENT? tot el que precedeix és pensament.
    let closes: Vec<&str> = REASON_TAGS
        .iter()
        .map(|(_, c)| *c)
        .chain(REASON_CLOSE_WORDS.iter().copied())
        .collect();
    let mut best: Option<(usize, usize)> = None; // (índex, longitud de l'etiqueta)
    for c in closes {
        if let Some(i) = find_ignore_ascii_case(text, c) {
            if best.map(|(bi, _)| i < bi).unwrap_or(true) {
                best = Some((i, c.len()));
            }
        }
    }
    if let Some((i, len)) = best {
        let head = &text[..i];
        // Si l'etiqueta d'obertura era una paraula, el pensament comença després.
        let opens: Vec<&str> = REASON_TAGS
            .iter()
            .map(|(o, _)| *o)
            .chain(REASON_OPEN_WORDS.iter().copied())
            .collect();
        let mut thought = head.to_string();
        for o in opens {
            if let Some(s) = find_ignore_ascii_case(head, o) {
                thought = head[s + o.len()..].to_string();
                break;
            }
        }
        let answer = text[i + len..].trim_start().to_string();
        return (thought.trim().to_string(), answer);
    }
    // 2) encara no hi ha tancament: si el fragment COMENÇA per una obertura, tot
    //    el que porta fins ara és raonament i el model no ha contestat.
    let t = text.trim_start();
    for (o, _) in REASON_TAGS {
        if find_ignore_ascii_case(t, o) == Some(0) {
            return (t[o.len()..].to_string(), String::new());
        }
    }
    for p in REASON_OPEN_WORDS {
        if find_ignore_ascii_case(t, p) == Some(0) {
            return (t[p.len()..].trim_start().to_string(), String::new());
        }
    }
    (String::new(), text.to_string())
}

/// Tallem un tram del text acumulat, respectant els límits de caràcter (els
/// fragments d'un model poden partir un accent per la meitat).
fn take(s: &str, a: usize, b: usize) -> String {
    let (a, b) = (a.min(s.len()), b.min(s.len()));
    let mut i = a;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    let mut j = b;
    while j < s.len() && !s.is_char_boundary(j) {
        j += 1;
    }
    s[i..j].to_string()
}

/// Cerca la TANCAMENT del pensament dins d'un text (ignora majúscules).
/// Retorna (índex, longitud de l'etiqueta) — índexs VÀLIDS sobre el text
/// original (no sobre una còpia en minúscules, que podria tenir una longitud
/// diferent en bytes).
fn reasoning_close_at(text: &str) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for c in REASON_TAGS
        .iter()
        .map(|(_, c)| *c)
        .chain(REASON_CLOSE_WORDS.iter().copied())
    {
        if let Some(i) = find_ignore_ascii_case(text, c) {
            if best.map(|(bi, _)| i < bi).unwrap_or(true) {
                best = Some((i, c.len()));
            }
        }
    }
    best
}

/// Separa EN DIRECTE el pensament de la resposta quan un model escriu les dues
/// coses pel mateix canal. No es pot mirar fragment per fragment: Ollama
/// parteix les etiquetes entre tokens (d'««» en feia «<th» + «ink»»), així que
/// es guarda el text brut acumulat i es busca sempre damunt del conjunt.
pub(crate) struct ThoughtSplitter {
    /// Tot el «content» rebut fins ara.
    raw: String,
    /// Bytes de `raw` ja repartits al visor.
    sent: usize,
    /// None: encara no sabem si aquest model pensa en veu alta.
    /// Some(true): estem dins del pensament. Some(false): ja respon.
    mode: Option<bool>,
}

impl ThoughtSplitter {
    pub(crate) fn new() -> Self {
        Self {
            raw: String::new(),
            sent: 0,
            mode: None,
        }
    }

    /// Afegeix un fragment i retorna (pensament nou, resposta nova) per emetre.
    pub(crate) fn push(&mut self, delta: &str) -> (String, String) {
        self.raw.push_str(delta);
        loop {
            // Encara sense decidir: es mira l'inici del text, no el fragment.
            if self.mode.is_none() {
                let skip = self.raw.len() - self.raw.trim_start().len();
                let head = take(&self.raw, skip, self.raw.len());
                // Quina etiqueta d'obertura (si n'hi ha) comença el text?
                let open_len = REASON_TAGS
                    .iter()
                    .map(|(o, _)| *o)
                    .chain(REASON_OPEN_WORDS)
                    .find(|tag| find_ignore_ascii_case(&head, tag) == Some(0))
                    .map(|t| t.len())
                    .unwrap_or(0);
                if let Some((i, len)) = reasoning_close_at(&head) {
                    // Pensament tancat abans de decidir-ne res: tot el que hi
                    // ha abans de l'etiqueta de tancament ho és.
                    let thought = take(&head, open_len, i).trim().to_string();
                    self.sent = skip + i + len;
                    self.mode = Some(false);
                    let answer = take(&self.raw, self.sent, self.raw.len());
                    self.sent = self.raw.len();
                    return (thought, answer);
                }
                if open_len > 0 {
                    self.mode = Some(true);
                    self.sent = skip + open_len;
                    continue;
                }
                // Massa text sense cap senyal: era la resposta normal i corrent.
                if head.chars().count() >= 24 || self.raw.len() >= 96 {
                    self.mode = Some(false);
                    self.sent = 0;
                    continue;
                }
                // Poquet encara: esperem el següent fragment abans de mostrar.
                return (String::new(), String::new());
            }
            if self.mode == Some(true) {
                // Dins del pensament: tot és raonament fins que aparega el
                // tancament (que pot haver arribat retallat en dues parts).
                let rest = take(&self.raw, self.sent, self.raw.len());
                if let Some((i, len)) = reasoning_close_at(&rest) {
                    let thought = take(&self.raw, self.sent, self.sent + i);
                    self.sent += i + len;
                    self.mode = Some(false);
                    let answer = take(&self.raw, self.sent, self.raw.len());
                    self.sent = self.raw.len();
                    return (thought, answer);
                }
                self.sent = self.raw.len();
                return (rest, String::new());
            }
            let answer = take(&self.raw, self.sent, self.raw.len());
            self.sent = self.raw.len();
            return (String::new(), answer);
        }
    }

    /// El que quedava per repartir quan el model ha acabat: si encara no
    /// s'havia decidit res, era una resposta curta i no cal perdre-la.
    pub(crate) fn flush(&mut self) -> (String, String) {
        let rest = take(&self.raw, self.sent, self.raw.len());
        self.sent = self.raw.len();
        match self.mode {
            Some(true) => (rest, String::new()),
            _ => (String::new(), rest),
        }
    }
}

/// Extrau el RAONAMENT d'un fragment NDJSON d'Ollama. El camp oficial és
/// `message.thinking`, però els servidors compatibles amb OpenAI (Ollama Cloud,
/// DeepSeek, OpenRouter…) l'anomenen `reasoning` o `reasoning_content`: els
/// tres es reconeixen perquè QUALSEVOL model puga mostrar el seu pensament.
pub(crate) fn chunk_thinking(msg: &serde_json::Value) -> String {
    ["thinking", "reasoning", "reasoning_content"]
        .iter()
        .filter_map(|k| msg.get(*k).and_then(|v| v.as_str()))
        .find(|s| !s.is_empty())
        .unwrap_or_default()
        .to_string()
}