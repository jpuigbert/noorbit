//! Motor NEURONAL de la IA NATIVA: el cervell de veritat de `noorbit_native`.
//!
//! Decisió de l'usuari (que revoca l'antic «motor simbòlic sense descàrregues»
//! com a única nativa): la «IA nativa» ha de ser una IA de ple dret que
//! raciocina i genera llenguatge natural, corrent TOT en local i cabent en
//! 4 GB de RAM. Com Soup demostra que es pot entrenar un 8B en 4 GB amb
//! *layer streaming*, nosaltres demostrem que es pot SERVIR un LLM real en 4
//! GB amb un model GGUF petit quantitzat (Q4) sobre `llama.cpp`.
//!
//! Arquitectura:
//! - **Model**: un GGUF petit (p. ex. Qwen2.5-0.5B-Instruct Q4_K_M, ~350 MB)
//!   que ve EMPAQUETAT a l'instal·lador (`native-model/` en els recursos de
//!   Tauri) i es copia a la carpeta de dades. Així, un cop instal·lat, NO cal
//!   internet ni descàrregues per parlar amb un LLM autèntic.
//! - **Motor**: el mateix `llama-server` (API compatible OpenAI) que ja
//!   gestiona [`crate::ai::llamacpp`]. El servei va per un port propi perquè
//!   no xoc amb el motor local que l'usuari engegui a mà.
//! - **Reserva**: si no hi ha model, ni motor, o la RAM no hi cab, aquest
//!   mòdul retorna `None` i la nativa cau al motor SIMBÒLIC (native.rs) que
//!   ja funcionava «sense baixar res». Cap pregunta queda sense resposta.
//! - **Internet sota demanda**: si la pregunta demana informació fresca o
//!   externa, es fa una recerca headless (DuckDuckGo + lectura de pàgines) i
//!   se n'injecta el resultat al context del model (RAG). L'usuari ho va
//!   demanar expressament: «que tingui accés a internet si ho necessita».
//! - **Aprenentatge**: les lliçons locals (crate::learning) i el que la nativa
//!   ha après vaguejant s'injecten al sistema perquè el model sàpiga el que
//!   ja li hem ensenyat.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Port propi del servidor neuronal de la nativa (no xoca amb Ollama 11434 ni
/// amb el motor local 11435 que l'usuari pugui engegar a mà).
pub const NATIVE_PORT: u16 = 11436;

/// Ruta absoluta del model GGUF empaquetat (resolta a `init`). `None` = no se
/// n'ha trobat cap als recursos.
static BUNDLED_MODEL: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Noms (continguts, sense accents ni majúscules) dels models que SÍ caben en
/// 4 GB i que la nativa accepta com a propis. Si l'usuari té un GGUF que no
/// encaixa aquí, NO l'usen com a natiu (podria ser un 7B que rebentaria la
/// RAM); la nativa només agafa models petits coneguts o l'empaquetat.
const SMALL_HINTS: &[&str] = &[
    "0.5b", "05b", "360m", "135m", "1.7b", "17b-instruct", "1b", "falcon-1b", "tiny", "mini", "nano",
    "smollm", "qwen2.5-0", "qwen2-0", "tinyllama", "llama-3.2-1b", "llama3.2-1b",
];

/// Resol el model i (opcionalment) copia el motor empaquetat a la carpeta de
/// dades. S'anomena una sola vegada des del `setup` de Tauri, on hi ha
/// `AppHandle` per resoldre la ruta de recursos.
pub fn init(app: &tauri::AppHandle) {
    use tauri::Manager as _;
    let resource = match app.path().resource_dir() {
        Ok(r) => r,
        Err(_) => {
            let _ = BUNDLED_MODEL.set(None);
            return;
        }
    };

    // 1) Motor empaquetat (opcional): si l'instal·lador inclou un
    //    `native-engine/`, el copiem a la carpeta on `llamacpp::binary()` el
    //    trobarà, perquè la nativa funcioni offline sense Homebrew ni
    //    descàrregues. Si ja hi ha un llama-server (brew, instal·lat, o
    //    copiat abans), no cal res.
    let engine_src = resource.join("native-engine");
    if engine_src.is_dir() && crate::ai::llamacpp::binary().is_none() {
        if let Some(dst) = crate::ai::llamacpp::install_dir() {
            let _ = copy_tree(&engine_src, &dst);
            // Marca executable el(s) binari(s) que hagi pogut copiar.
            #[cfg(unix)]
            if let Some(bin) = crate::ai::llamacpp::binary() {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755));
            }
        }
    }

    // 2) Model empaquetat: el primer .gguf dins `native-model/`.
    let model = find_gguf_in(&resource.join("native-model"))
        // En desenvolupament (pnpm tauri dev) els recursos no són al .app:
        // mirem també la carpeta del crat immediatament superior.
        .or_else(|| find_gguf_in(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("native-model")));
    let _ = BUNDLED_MODEL.set(model);
}

/// El model GGUF que la nativa ha de servir, o `None` si no n'hi ha cap de
/// petit disponible.
pub fn model_path() -> Option<PathBuf> {
    // 1) El empaquetat (sempre prioritari: és el que l'usuari va triar).
    if let Some(m) = BUNDLED_MODEL.get().and_then(|o| o.as_ref()) {
        if m.exists() {
            return Some(m.clone());
        }
    }
    // 2) Un model petit JA descarregat per l'usuari a la carpeta de GGUF, que
    //    encaixi amb la llista de models lleugers.
    for dir in gguf_dirs() {
        if let Some(m) = find_small_gguf_in(&dir) {
            return Some(m);
        }
    }
    None
}

/// Carpeta(s) on poden haver-hi GGUF (la de dades i la externa configurada).
fn gguf_dirs() -> Vec<PathBuf> {
    let mut v = vec![crate::ai::hf_bay::models_dir()];
    if let Ok(cfg) = crate::config::AppConfig::load() {
        let d = cfg.ai.gguf_dir.trim();
        if !d.is_empty() {
            let p = PathBuf::from(d);
            if !v.contains(&p) {
                v.push(p);
            }
        }
    }
    v
}

fn find_gguf_in(dir: &PathBuf) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("gguf")).unwrap_or(false) {
            return Some(p);
        }
    }
    None
}

/// El primer GGUF el nom del qual contingui un marcador de model lleuger.
fn find_small_gguf_in(dir: &PathBuf) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut cands: Vec<PathBuf> = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if !p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("gguf")).unwrap_or(false) {
            continue;
        }
        let name = crate::ai::codegen::pla(&p.file_name().unwrap_or_default().to_string_lossy());
        if SMALL_HINTS.iter().any(|h| name.contains(h)) {
            cands.push(p);
        }
    }
    // El més petit (per RAM) dels candidats.
    cands.sort_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(u64::MAX));
    cands.into_iter().next()
}

/// Hi ha prou RAM lliure per carregar un model petit (~0.5–1 GB)? Si no, la
/// nativa no s'arrisca a saturar el sistema i deixa la feina al simbòlic.
fn ram_ok() -> bool {
    match crate::ai::wander::ram_disponible() {
        // S'aixeca amb un llindar conservador de 900 MB lliures: un 0.5B Q4
        // ocupa ~350 MB de pesos + KV; amb 900 MB lliures hi cab còmode.
        Some(bytes) => bytes >= 900_000_000,
        // Si no sabem la RAM (sistema estrany), intentem-ho: l'arrencada del
        // servidor ja fallarà si no abasta i caurem al simbòlic.
        None => true,
    }
}

/// La pregunta demana informació EXTERNA o FRESCA que un model de 4 GB no pot
/// saber de memòria? (notícies, preus, versions, dades d'avui, documentació
/// d'una API, un fet concret…). Només llavors obrim internet, per no fer
/// consultes de xarxa en preguntes locals o de codi.
pub fn needs_web(prompt: &str) -> bool {
    let q = question_of(prompt);
    let low = crate::ai::codegen::pla(&q);
    const HINTS: &[&str] = &[
        "internet", "cerca", "cerca a", "busca", "buscar", "google", "duckduckgo", "noticia", "noticies",
        "actualitat", "avui", "ahir", "aquest any", "ultim", "ultims", "darrer", "darrers", "preu",
        "preus", "cotitzacio", "accio", "bitcoin", "temps", "clima", "el temps", "versio", "llancament",
        "release", "novedat", "novedats", "documentacio", "docs de", "com s'usa", "api de",
        "que es", "qui es", "quien", "quanta gent", "capital de", "any de", "data de",
        "estats units", "unio europea", "guerra", "eleccio", "medalla", "record",
    ];
    HINTS.iter().any(|h| low.contains(h))
}

/// Extrau la pregunta final (darrere de «PREGUNTA / TASCA:») per no analitzar
/// el context del projecte sencer.
fn question_of(prompt: &str) -> String {
    match prompt.rfind("PREGUNTA / TASCA:") {
        Some(i) => prompt[i + "PREGUNTA / TASCA:".len()..].to_string(),
        None => prompt.to_string(),
    }
}

/// Omet línies EXACTAMENT repetides del fragment de context del prompt (tot
/// allò anterior a «PREGUNTA / TASCA:»), que es conserva sencer. És la
/// versió text del principi darrere del mode «reproduction» d'HyperQwen
/// («speculation that drafts out of the prompt»): allò que ja és al context
/// una vegada no cal tornar-lo a pagar en tokens d'entrada. Només es toquen
/// línies llargues (≥ 40 caràcters): claus, tanquetes i mots breus es
/// repeteixen legítimament i es conserven intactes.
fn dedup_context(prompt: &str) -> String {
    const MIN_LEN: usize = 40;
    let Some(pos) = prompt.rfind("PREGUNTA / TASCA:") else {
        // Sense el marcador estàndard del xat no distingim context de tasca:
        // millor no arriscar el contingut.
        return prompt.to_string();
    };
    let (ctx, task) = (&prompt[..pos], &prompt[pos..]);
    let mut vistes: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut omeses = 0usize;
    let mut net = String::with_capacity(ctx.len());
    for line in ctx.lines() {
        let t = line.trim();
        if t.chars().count() >= MIN_LEN && !vistes.insert(t.to_string()) {
            omeses += 1;
            continue;
        }
        net.push_str(line);
        net.push('\n');
    }
    if omeses == 0 {
        return prompt.to_string();
    }
    format!(
        "{}\n… [{} línies duplicades del context omeses per NoOrbit] …\n\n{}",
        net, omeses, task
    )
}

/// Pressupost de tokens de SORTIDA segons la tasca: l'anàleg local del «draft
/// vocabulary calibrat sobre la distribució de sortida del model»
/// d'HyperQwen. La majoria de preguntes no necessiten els 900 tokens fixos
/// d'abans (i un sostre baix evita que el model s'allargui de balons); la
/// generació de codi sí que manté marge ample, condicionat a la RAM.
fn output_budget(q: &str) -> u32 {
    output_budget_amb_ram(q, ram_ok())
}

/// Versió pura (testable) del pressupost: `ram_ample` és el gate `ram_ok()`.
/// Amb RAM suficient, la generació de codi fitxer-a-fitxer disposa de 2048
/// tokens (un fitxer sencer + explicació); amb poca RAM es manté el sostre
/// baix de sempre (1200) per no allargar la inferència ni la cache KV en una
/// màquina justa (Intel 8 GB).
fn output_budget_amb_ram(q: &str, ram_ample: bool) -> u32 {
    let low = crate::ai::codegen::pla(q);
    const CODE: &[&str] = &[
        "crea", "genera", "escriu", "redacta", "implementa", "component", "funcio", "script",
        "codi", "programa", "fitxer", "plantilla", "joc", "aplicacio", "edita", "modifica",
    ];
    const CURT: &[&str] = &[
        "calcula", "on es", "qui es", "quant", "data de", "significa", "any de", "capital de",
        "si o no", "veritat o fals", "mesura", "pes de",
    ];
    if CODE.iter().any(|h| low.contains(h)) {
        if ram_ample {
            2048
        } else {
            1200
        }
    } else if CURT.iter().any(|h| low.contains(h)) {
        256
    } else {
        512
    }
}

/// Finestra de context BASE del servidor neuronal (tokens): la mateixa que
/// s'usa en engegar-lo (RAM disponible + cache KV q8_0 si el motor la suporta).
/// La compactació de sessió la fa servir per saber quan el context del xat
/// s'acosta al límit REAL del GGUF local (no la finestra d'un model de núvol).
pub fn finestra_base() -> u64 {
    let base: u64 = if crate::ai::wander::ram_total()
        .map(|b| b >= 6_000_000_000)
        .unwrap_or(true)
    {
        2048
    } else {
        1024
    };
    if crate::ai::llamacpp::supports_flag("--cache-type-k") {
        base * 2
    } else {
        base
    }
}

/// Rutes de fitxer que la TASCA esmenta explícitament («edita main.gd»,
/// «genera src/App.tsx»…). Només extensions reals de codi/config: així un
/// número de versió («v2.3») no es confon amb un fitxer.
fn fitxers_demanats(q: &str) -> Vec<String> {
    let Ok(re) = regex::Regex::new(
        r"(?i)[a-z0-9_][a-z0-9_./-]*\.(?:rs|tsx|ts|jsx|js|py|php|css|scss|html|gd|tscn|godot|json|toml|md|lua|cs|cpp|cc|h|hpp|java|go|rb|sh|yml|yaml|vue|svelte|sql|ini|txt|collection|script|uproject|gltf|blend)\b",
    ) else {
        return Vec::new();
    };
    let mut v: Vec<String> = Vec::new();
    for m in re.find_iter(q) {
        let s = m.as_str().to_lowercase();
        if !v.contains(&s) {
            v.push(s);
        }
    }
    v
}

/// Redueix el context del projecte al NECESSARI per al fitxer demanat: si la
/// tasca anomena un fitxer concret i el context porta el projecte sencer en
/// seccions «===== ruta =====» (format de `buildContext`), es conserven només
/// les seccions d'eixos fitxers (més una capçalera breu amb l'arbre de rutes).
/// La resta del flux (dedup + compact_prompt) continua igual. Si no hi ha
/// cap coincidència, el prompt es deixa intacte: mai es perd informació que
/// el model pugui necessitar per a una pregunta general.
fn context_per_fitxer(prompt: &str) -> String {
    let Some(pos) = prompt.rfind("PREGUNTA / TASCA:") else {
        return prompt.to_string();
    };
    let (ctx, task) = (&prompt[..pos], &prompt[pos..]);
    let demanats = fitxers_demanats(task);
    if demanats.is_empty() || !ctx.contains("=====") {
        return prompt.to_string();
    }
    // Separa el context en seccions «===== ruta =====» + capçalera prèvia.
    let mut seccions: Vec<(String, String)> = Vec::new();
    let mut capcalera = String::new();
    let mut actual: Option<(String, String)> = None;
    for line in ctx.lines() {
        let t = line.trim();
        if t.starts_with("=====") && t.ends_with("=====") && t.len() > 10 {
            if let Some(sec) = actual.take() {
                seccions.push(sec);
            }
            let ruta = t.trim_matches('=').trim().to_lowercase();
            actual = Some((ruta, String::new()));
        } else if let Some((_, cos)) = actual.as_mut() {
            cos.push_str(line);
            cos.push('\n');
        } else {
            capcalera.push_str(line);
            capcalera.push('\n');
        }
    }
    if let Some(sec) = actual.take() {
        seccions.push(sec);
    }
    if seccions.is_empty() {
        return prompt.to_string();
    }
    let base = |r: &str| r.rsplit('/').next().unwrap_or(r).to_string();
    let coincideix = |ruta: &str| {
        demanats.iter().any(|d| ruta == d || base(ruta) == base(d))
    };
    let conservades: Vec<&(String, String)> =
        seccions.iter().filter(|(r, _)| coincideix(r)).collect();
    if conservades.is_empty() || conservades.len() == seccions.len() {
        return prompt.to_string();
    }
    let omeses = seccions.len() - conservades.len();
    // Capçalera breu (instruccions + arbre de rutes retallat): el model
    // continua sabent què més hi ha al projecte sense pagar-ne el contingut.
    let mut out: String = capcalera.chars().take(1600).collect();
    out.push_str(&format!(
        "\n… [context reduït per NoOrbit: {} seccions d'altres fitxers omeses; \
         es conserva el context del fitxer demanat] …\n\n",
        omeses
    ));
    for (r, c) in conservades {
        out.push_str(&format!("===== {} =====\n{}", r, c));
    }
    out.push_str(task);
    out
}

/// Retalla un text a `n` caràcters (amb punt suspensiu si cal tallar).
fn cap(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n).collect();
    out.push('…');
    out
}

/// Resposta del LLM neuronal, o `None` si cap peça no és disponible (aleshores
/// la nativa usa el seu motor simbòlic). `lessons` és el bloc d'aprenentatge
/// local (pot ser buit) que s'injecta al sistema perquè el model sàpiga el que
/// ja li hem ensenyat.
pub async fn ask(
    http: &reqwest::Client,
    prompt: &str,
    system: Option<&str>,
    lessons: &str,
) -> Option<String> {
    // 1) Prerequisits: model petit + motor + RAM.
    let model = model_path()?;
    if crate::ai::llamacpp::binary().is_none() {
        return None;
    }
    if !ram_ok() {
        return None;
    }

    // 2) Assegura el servidor obert (ctx prudent per a 4 GB). Anàleg de la
    //    idea KVarN d'HyperQwen (cache KV quantitzada a pocs bits): si el
    //    motor instal·lat suporta `--cache-type-k`, la cache KV passa a q8_0
    //    → ocupa aproximadament la meitat per token i es pot doblar la
    //    finestra de context amb el mateix pressupost de RAM (menys poda i
    //    menys reenviaments). Si el binari no ho reconeix, `start_with_args`
    //    ho intenta igualment i cau sol a l'arrencada sense les opcions.
    let base_ctx = if crate::ai::wander::ram_total().map(|b| b >= 6_000_000_000).unwrap_or(true) {
        2048
    } else {
        1024
    };
    let kv8 = crate::ai::llamacpp::supports_flag("--cache-type-k");
    let mut ctx = if kv8 { base_ctx * 2 } else { base_ctx };
    // Pressupost de sortida ABANS d'engegar el servidor: si la tasca és de
    // generació de codi amb RAM suficient (2048 tokens), la finestra ha de
    // cabre el prompt compactat (≤ ~1.750 tokens) + sistema (~600) + sortida
    // + marge. Amb un 0.5B la KV extra ocupa desenes de MB, molt per sota del
    // llindar de `ram_ok` (900 MB lliures): no es trenca cap límit de RAM.
    let budget = output_budget(&question_of(prompt));
    if budget > 1200 {
        ctx = ctx.max(1750 + 600 + budget + 256);
    }
    let opts: &[&str] = if kv8 {
        &["--cache-type-k", "q8_0", "--cache-type-v", "q8_0"]
    } else {
        &[]
    };
    let model_str = model.to_string_lossy().to_string();
    let (_id, _name, base) =
        crate::ai::llamacpp::start_with_args(http, &model_str, NATIVE_PORT, ctx, opts)
            .await
            .ok()?;

    // 3) Recerca web sota demanda (només si la pregunta ho demana).
    let web = if needs_web(prompt) {
        web_context(http, &question_of(prompt)).await
    } else {
        String::new()
    };

    // 4) Muntada del sistema: instruccions + aprenentatge + web (si escau).
    //    El context del projecte ja ve dins el `prompt`; el recortem perquè un
    //    0.5B amb milers de línies de codi rebentaria la finestra i la RAM.
    let mut sys = String::from(
        "Ets la IA nativa de NoOrbit. Respon en CATALÀ, de manera clara i directa, \
         basant-te en el context del projecte i la informació que se t'adjunten. \
         Si no saps alguna cosa, digues-ho amb honestedat en lloc d'inventar. \
         Quan t'ajunten resultat d'internet, fonamenta-hi la resposta.",
    );
    if let Some(s) = system {
        let extra = cap(s.trim(), 600);
        if !extra.is_empty() {
            sys.push_str("\n\nCONSIGNA ADDICIONAL:\n");
            sys.push_str(&extra);
        }
    }
    // Cada bloc injectat té un sostre propi: el sistema NO pot superar el
    // pressupost total (≈ sistema-base + 600 + 700 + 600 + 1200 caràcters),
    // perquè amb un ctx de 2048–4096 cal deixar lloc per a la tasca i la
    // sortida. El que cau fora es retalla per rellevància decreixent.
    let les = cap(lessons.trim(), 700);
    if !les.is_empty() {
        sys.push_str("\n\n");
        sys.push_str(&les);
    }
    // Aprenentatge del vagueig: el coneixement que la nativa ha après (del
    // diari i les millores silencioses) es desa al cervell après. L'injectem
    // perquè el model neuronal també el profiti, no només la reserva simbòlica.
    let after = crate::ai::native::learned_digest(12);
    let after = cap(after.trim(), 600);
    if !after.is_empty() {
        sys.push_str("\n\n");
        sys.push_str(&after);
    }
    if !web.trim().is_empty() {
        let w = cap(web.trim(), 1200);
        sys.push_str("\n\n");
        sys.push_str(&w);
    }

    // Si la tasca demana un fitxer concret, el context del projecte sencer es
    // redueix al necessari per a eixe fitxer; després s'ometen les repeticions
    // (no es paga dues vegades el mateix fragment) i `compact_prompt` fa la
    // poda segura si continua massa gran.
    let user = crate::ai::compact_prompt(&dedup_context(&context_per_fitxer(prompt)));

    // 5) CRIDA al /chat/completions compatible OpenAI del servidor local.
    let url = format!("{}/chat/completions", base);
    let body = serde_json::json!({
        "model": model_str,
        "messages": [
            { "role": "system", "content": sys },
            { "role": "user", "content": user },
        ],
        "max_tokens": budget,
        "temperature": 0.4,
        "stream": false,
    });
    let resp = http
        .post(&url)
        .timeout(std::time::Duration::from_secs(180))
        .json(&body)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: serde_json::Value = resp.json().await.ok()?;
    let text = v
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;
    Some(text)
}

/// Recerca headless a internet i torna un bloc de context (o buit si no hi ha
/// connexió/resultats). Reaprofita els extractors d'URLs del vagueig.
async fn web_context(http: &reqwest::Client, query: &str) -> String {
    let q = query.trim();
    if q.is_empty() {
        return String::new();
    }
    let engines = crate::ai::wander::cercadors_lliures();
    let mut ctx = String::from(
        "INFORMACIÓ D'INTERNET (cercada per la IA nativa per respondre la teva \
         pregunta; pot no ser exhaustiva):\n",
    );
    let mut troballes = 0usize;
    for (_nom, base_url, param) in engines.iter().take(2) {
        let Ok(resp) = http
            .get(*base_url)
            .query(&[(*param, q)])
            .header("User-Agent", "NoOrbit/0.7 native-ai")
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
        else {
            continue;
        };
        let Ok(html) = resp.text().await else { continue };
        let urls = crate::ai::wander::urls_resultats(&html, _nom, &[]);
        for u in urls.iter().take(3) {
            let Some(page) = fetch_clean(http, u).await else { continue };
            if page.chars().count() < 120 {
                continue;
            }
            ctx.push_str(&format!("\n--- {u} ---\n"));
            ctx.push_str(&page.chars().take(1800).collect::<String>());
            ctx.push('\n');
            troballes += 1;
            if troballes >= 3 {
                return ctx;
            }
        }
        if troballes > 0 {
            return ctx;
        }
    }
    if troballes == 0 {
        String::new()
    } else {
        ctx
    }
}

/// Descarrega una pàgina i en torna el text net (sense scripts/estils),
/// retallat. Si falla, `None`.
async fn fetch_clean(http: &reqwest::Client, url: &str) -> Option<String> {
    let resp = http
        .get(url)
        .header("User-Agent", "NoOrbit/0.7 native-ai")
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let html = resp.text().await.ok()?;
    let re = regex::Regex::new(
        r"(?is)<(script|style|noscript|header|footer|nav)[^>]*>.*?</(script|style|noscript|header|footer|nav)>",
    )
    .ok()?;
    let brut = re.replace_all(&html, " ").to_string();
    Some(strip_tags(&brut).chars().take(3000).collect())
}

/// Treu les etiquetes HTML i col·lapsa espais (per llegir pàgines com a text).
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag => {}
            c if c == '\n' || c == '\t' || c == ' ' => {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            c => out.push(c),
        }
    }
    // Decodifica les entitats HTML bàsiques i torna a col·lapsar espais
    // («Hola&nbsp; món» → «Hola món», sense doble espai).
    let decoded = out
        .trim()
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let mut net = String::with_capacity(decoded.len());
    for c in decoded.chars() {
        if c == ' ' {
            if !net.ends_with(' ') {
                net.push(' ');
            }
        } else {
            net.push(c);
        }
    }
    net.trim().to_string()
}

/// Copia recursivament `from` dins `to` (per portar el motor empaquetat a la
/// carpeta de dades on el busca `llamacpp::binary()`).
fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let ft = e.file_type()?;
        let dest = to.join(e.file_name());
        if ft.is_dir() {
            copy_tree(&e.path(), &dest)?;
        } else {
            std::fs::copy(e.path(), &dest)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod proves {
    use super::*;

    #[test]
    fn needs_web_dona_preguntes_externes() {
        assert!(needs_web("què és el canvi climàtic avui"));
        assert!(needs_web("cerca la darrera versió de react"));
        assert!(needs_web("quin temps farà demà"));
    }

    #[test]
    fn needs_web_no_toca_preguntes_locals() {
        assert!(!needs_web("què fa aquest projecte"));
        assert!(!needs_web("crea un component React"));
        assert!(!needs_web("calcula 2+2"));
        assert!(!needs_web("explica'm l'estructura del codi"));
    }

    #[test]
    fn strip_tags_neteja_html() {
        assert_eq!(strip_tags("<p>Hola&nbsp; món</p>").trim(), "Hola món");
        assert_eq!(strip_tags("<b>rust</b> code").trim(), "rust code");
    }

    #[test]
    fn question_of_isola_la_tasca() {
        let p = format!("context molt llarg\n\n---\nPREGUNTA / TASCA:\n{}", "què és X?");
        assert_eq!(question_of(&p).trim(), "què és X?");
    }

    #[test]
    fn dedup_context_omet_duplicats_i_conserva_tasca() {
        let llarg = "aquest fragment de fitxer és prou llarg per considerar-se únic";
        let p = format!(
            "{}\ncapa curta sense repetir\n{}\n\nPREGUNTA / TASCA:\nquè fa?",
            llarg, llarg
        );
        let out = dedup_context(&p);
        assert_eq!(out.matches(llarg).count(), 1);
        assert!(out.contains("1 línies duplicades"));
        assert!(out.ends_with("què fa?"));
    }

    #[test]
    fn dedup_context_sense_marcador_no_toca_res() {
        let p = "text lliure sense el marcador estàndard que munta el xat de NoOrbit\nrepetit\nrepetit";
        assert_eq!(dedup_context(p), p);
    }

    #[test]
    fn dedup_context_conserva_línies_curtes() {
        let p = "}\n}\n}\ncap\n\nPREGUNTA / TASCA:\ndigue'm";
        let out = dedup_context(&p);
        assert!(!out.contains("duplicades"));
        assert_eq!(out.matches('}').count(), 3);
    }

    #[test]
    fn output_budget_segons_la_tasca() {
        // Amb RAM suficient: marge ample per a generació fitxer-a-fitxer.
        assert_eq!(output_budget_amb_ram("crea un component react", true), 2048);
        assert_eq!(output_budget_amb_ram("calcula 12*34", true), 256);
        assert_eq!(output_budget_amb_ram("explica'm com es forma la pluja", true), 512);
    }

    #[test]
    fn output_budget_condicionat_a_la_ram() {
        // Poca RAM (gate ram_ok fals): el sostre de codi es manté baix, com
        // abans; la resta de pressupostos no depenen de la RAM.
        assert_eq!(output_budget_amb_ram("crea un component react", false), 1200);
        assert_eq!(output_budget_amb_ram("edita el fitxer main.gd", false), 1200);
        assert_eq!(output_budget_amb_ram("edita el fitxer main.gd", true), 2048);
        assert_eq!(output_budget_amb_ram("calcula 12*34", false), 256);
        assert_eq!(output_budget_amb_ram("explica'm com es forma la pluja", false), 512);
        // El wrapper públic delega al gate real (sigui quin sigui l'estat de
        // la màquina, el valor és un dels dos sostres de codi).
        let b = output_budget("crea un component react");
        assert!(b == 1200 || b == 2048, "budget inesperat: {}", b);
    }

    #[test]
    fn fitxers_demanats_detecta_rutes_reals() {
        assert_eq!(fitxers_demanats("edita el fitxer main.gd"), vec!["main.gd"]);
        assert_eq!(fitxers_demanats("genera src/App.tsx i prou"), vec!["src/app.tsx"]);
        // Un número de versió o una extensió inventada no són fitxers.
        assert!(fitxers_demanats("puja a la versió v2.3").is_empty());
        assert!(fitxers_demanats("explica'm la pluja").is_empty());
    }

    #[test]
    fn context_per_fitxer_redueix_al_demanat() {
        let p = format!(
            "RUTES:\nmain.gd\naltre.rs\n\nCONTINGUT:\n===== main.gd =====\nextends Node2D\n\n===== altre.rs =====\nfn main() {{}}\n\n---\nPREGUNTA / TASCA:\nedita el fitxer main.gd per fer que la pilota boti"
        );
        let out = context_per_fitxer(&p);
        assert!(out.contains("===== main.gd ====="), "{}", out);
        assert!(out.contains("extends Node2D"), "{}", out);
        assert!(!out.contains("fn main()"), "{}", out);
        assert!(out.contains("1 seccions d'altres fitxers omeses"), "{}", out);
        // La tasca es conserva SENCERA al final.
        assert!(out.ends_with("edita el fitxer main.gd per fer que la pilota boti"), "{}", out);
    }

    #[test]
    fn context_per_fitxer_no_toca_preguntes_generals() {
        // Sense fitxer demanat: el context es deixa intacte.
        let p = "===== main.gd =====\nextends Node2D\n\n===== altre.rs =====\nfn main() {}\n\n---\nPREGUNTA / TASCA:\nquè fa aquest projecte?";
        assert_eq!(context_per_fitxer(p), p);
        // Sense seccions de fitxers o sense marcador: intacte.
        let p2 = "context pla sense seccions\n\nPREGUNTA / TASCA:\nedita main.gd";
        assert_eq!(context_per_fitxer(p2), p2);
        let p3 = "text lliure sense marcador que esmenta main.gd";
        assert_eq!(context_per_fitxer(p3), p3);
        // Si el fitxer demanat no és al context, tampoc es retalla res.
        let p4 = "===== altre.rs =====\nfn main() {}\n\n---\nPREGUNTA / TASCA:\nedita main.gd";
        assert_eq!(context_per_fitxer(p4), p4);
    }

    #[test]
    fn cap_retalla_amb_elipsis() {
        assert_eq!(cap("hola", 10), "hola");
        assert_eq!(cap("0123456789x", 5), "01234…");
    }
}
