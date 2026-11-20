//! Proveïdors d'IA en línia alternatius (APIs basades en token).
//!
//! Cada integració és un punt final compatible amb l'API d'OpenAI
//! (`POST {base_url}/chat/completions`), de manera que funciona amb proveïdors
//! d'avui (OpenAI, Groq, Together, Mistral, OpenRouter, LM Studio, llama.cpp
//! servidor…) i amb els que apareguin demà, sense canviar codi: només cal
//! registrar-ne la URL, el model i el token.
//!
//! Els tokens es desen a la carpeta de dades de l'aplicació i NO surten mai al
//! frontend en text pla: la llista els emmascara.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Com s'adjunta el token a les peticions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthScheme {
    /// Capçalera  Authorization: Bearer <token>  (estàndard OpenAI i compatibles)
    #[default]
    Bearer,
    /// Capçalera amb nom custom (p.ex. "x-api-key")
    #[serde(rename = "header")]
    ApiKeyHeader,
    /// Sense autenticació
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProvider {
    pub id: String,
    pub name: String,
    /// Per exemple https://api.openai.com/v1
    pub base_url: String,
    /// Model a usar (p.ex. gpt-4o-mini, llama-3.3-70b-versatile…)
    #[serde(default)]
    pub model: String,
    /// Format de l'API: "openai" (compatible OpenAI, per defecte) o
    /// "anthropic" (Claude). Determina com es construeix la petició.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub auth: AuthScheme,
    /// Nom de la capçalera quan auth = api_key_header
    #[serde(default)]
    pub header_name: String,
    #[serde(default)]
    pub token: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Versió de l'API d'Anthropic que declarem a les peticions: és la versió GA
/// que tots els serveis compatibles accepten des de 2023. El que canvia entre
/// generacions de models no és la versió sinó el mode de raonament (vegeu
/// `anthropic_shape`).
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Versió (majors, menors) escrita dins del nom d'un model:
/// · `claude-sonnet-5-5` → (5, 5)
/// · `claude-opus-4-5-20251101` → (4, 5)
/// · `claude-3-7-sonnet-20250219` → (3, 7)
/// · `qwen3.5:0.8b` → (3, 5) — els dígits enganxats a lletres també compten,
///   perquè un servei compatible pot escriure «sonnet4.5» sense guionets.
/// Els números grans (dates, mides de paràmetres) s'ignoren; si el nom no en
/// porta, retorna (0, 0) i ho tractem com a «desconegut».
pub fn model_version(model: &str) -> (u32, u32) {
    let mut found: Vec<u32> = Vec::new();
    let mut cur = String::new();
    // Es llegeix el nom com un fil de dígits tallats per qualsevol altra cosa:
    // el punt NO talla («4.5» és una versió, no dues), la resta sí.
    for c in model.to_lowercase().chars() {
        if c.is_ascii_digit() {
            cur.push(c);
            continue;
        }
        if !cur.is_empty() {
            if let Ok(v) = cur.parse::<u32>() {
                // Un component de versió raonable: 20251101 és una data.
                if v <= 100 {
                    found.push(v);
                }
            }
            cur.clear();
        }
        if found.len() >= 2 {
            break;
        }
    }
    // L'últim número pot quedar sense tallar («claude-sonnet-5-5»).
    if found.len() < 2 && !cur.is_empty() {
        if let Ok(v) = cur.parse::<u32>() {
            if v <= 100 {
                found.push(v);
            }
        }
    }
    let maj = found.first().copied().unwrap_or(0);
    let min = found.get(1).copied().unwrap_or(0);
    (maj, min)
}

/// Com demana NoOrbit el RAONAMENT i quants tokens de sortida admet, segons el
/// model d'Anthropic. Regles de la documentació oficial (nov 2026):
/// · 4.6, 4.7, 5.x i posteriors → `adaptive`; el mode manual amb
///   `budget_tokens` REBUTA amb un 400 (per això abans Claude «no funcionava»).
/// · 4.5, 4.x i 3.7 → mode manual `enabled` amb pressupost (< `max_tokens`).
/// · 3.5 i anteriors → no se'n demana cap.
/// · noms sense números (serveis compatibles d'un tercer, com el format
///   Anthropic de DeepSeek) → mode manual amb un sostre ample.
/// Retorna (mode, max_tokens); «none» vol dir «no enviïs el bloc thinking».
pub fn anthropic_shape(model: &str) -> (&'static str, u64) {
    let (maj, min) = model_version(model);
    if maj == 0 {
        return ("enabled", 32_000);
    }
    if maj > 4 || (maj == 4 && min >= 6) {
        return ("adaptive", 64_000);
    }
    if maj == 4 && min == 5 {
        return ("enabled", 32_000);
    }
    if maj == 4 || (maj == 3 && min >= 7) {
        return ("enabled", 16_000);
    }
    ("none", 8_192)
}

/// Sostre de tokens de sortida per als serveis compatibles amb OpenAI que en
/// tenen un de prou curt per defecte. DeepSeek aplica un límit propi de ~4K si
/// no se li indica el contrari: els fitxers que la IA escrivien es tallaven a
/// mitja i semblava «la clau no va». La resta de serveis no reben res —
/// alguns models (sèrie o d'OpenAI) rebutgen el paràmetre.
pub fn openai_max_tokens(model: &str) -> Option<u64> {
    if model.to_lowercase().contains("deepseek") {
        Some(32_000)
    } else {
        None
    }
}

/// Una imatge adjunta, ja codificada per a qualsevol dels dos formats d'API.
struct ImagePart {
    mime: &'static str,
    data: String,
}

/// Tipus d'imatge que accepten Anthropic i els serveis compatibles: la resta
/// s'omet, perquè una imatge que el servei rebutja no pot fer fracassar tot el
/// missatge.
fn image_mime(path: &str) -> Option<&'static str> {
    let ext = PathBuf::from(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// Carrega les imatges adjuntes (rutes de fitxer) en base64. Més de 4 MB en
/// brut es deixa anar: Anthropic posa el límit a ~5 MB per imatge codificada,
/// i superar-lo dóna un 400 que s'interpreta com «la clau no funciona».
fn load_images(paths: &[String]) -> Vec<ImagePart> {
    use base64::Engine as _;
    let mut out = Vec::new();
    for p in paths {
        let Some(mime) = image_mime(p) else { continue };
        let Ok(bytes) = std::fs::read(p) else { continue };
        if bytes.is_empty() || bytes.len() > 4_000_000 {
            continue;
        }
        out.push(ImagePart {
            mime,
            data: base64::engine::general_purpose::STANDARD.encode(&bytes),
        });
    }
    out
}

/// Normalitza una URL base d'API compatible amb OpenAI. L'usuari sol copiar
/// la URL de la documentació del servei, i aquí és on moren les claus que
/// «no funcionen»: barra final, la ruta «/chat/completions» ja inclosa, o
/// el prefix de versió oblidat. Amb Venice: «https://api.venice.ai» (sense
/// «/api/v1») donaria 404 i pareceria que la clau és dolenta.
pub fn normalize_base(url: &str) -> String {
    let mut b = url.trim().trim_end_matches('/').to_string();
    // Si l'usuari ha enganxat l'endpoint sencer, en quedem la base. «/messages»
    // és el cas d'Anthropic: copiar `https://api.anthropic.com/v1/messages`
    // (la línia de la documentació) sense netejar-ho donava un 404 incomprensible.
    for suffix in [
        "/chat/completions",
        "/completions",
        "/responses",
        "/messages",
    ] {
        if b.to_lowercase().ends_with(suffix) {
            b.truncate(b.len() - suffix.len());
            b = b.trim_end_matches('/').to_string();
        }
    }
    let low = b.to_lowercase();
    if low.contains("api.venice.ai") && !low.contains("/api/v1") {
        b.push_str("/api/v1");
    } else if low.contains("api.openai.com") && !b.split("//").nth(1).map(|r| r.contains('/')).unwrap_or(true) {
        b.push_str("/v1");
    }
    b
}

/// El servei és LOCAL (la mateixa màquina)? Aquests serveis — LM Studio,
/// llama.cpp, vLLM, DeepInfer a 127.0.0.1… — no demanen cap clau: hi ha
/// d'enviar la petició SENSE capçalera d'autenticació (o amb una de buida,
/// que alguns rebutjen amb 401).
pub fn is_local_url(url: &str) -> bool {
    let low = url.trim().to_lowercase();
    low.contains("localhost")
        || low.contains("127.0.0.1")
        || low.contains("0.0.0.0")
        || low.contains("::1")
        || low.starts_with("http://[::1]")
}

/// Versió sense el secret, per enviar a la UI (token emmascarat).
#[derive(Debug, Clone, Serialize)]
pub struct AiProviderPublic {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub kind: String,
    pub auth: AuthScheme,
    pub header_name: String,
    pub has_token: bool,
    pub token_masked: String,
    pub enabled: bool,
}

impl AiProvider {
    fn to_public(&self) -> AiProviderPublic {
        AiProviderPublic {
            id: self.id.clone(),
            name: self.name.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            kind: self.kind.clone(),
            auth: self.auth,
            header_name: self.header_name.clone(),
            has_token: self.has_token(),
            token_masked: mask(&self.token),
            enabled: self.enabled,
        }
    }

    /// Base normalitzada per a qualsevol endpoint d'aquest proveïdor.
    pub fn base(&self) -> String {
        normalize_base(&self.base_url)
    }

    /// Clau neta d'espais (els quals solen vindre d'un copiat de la web).
    pub fn token_clean(&self) -> &str {
        self.token.trim()
    }

    /// Té una credencial utilitzable? (amb espais en blanc NO compta).
    pub fn has_token(&self) -> bool {
        matches!(self.auth, AuthScheme::None) || !self.token_clean().is_empty()
    }

    /// Aplica l'esquema d'autenticació del proveïdor a una petició. Públic
    /// perquè el mòdul `imgen` pugui usar la mateixa credencial per a l'API
    /// d'imatges (OpenAI-compatible) sense reimplementar l'autenticació.
    pub fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.auth {
            AuthScheme::Bearer => req.header("Authorization", format!("Bearer {}", self.token_clean())),
            AuthScheme::ApiKeyHeader => {
                let name = if self.header_name.is_empty() {
                    "x-api-key".to_string()
                } else {
                    self.header_name.clone()
                };
                req.header(name, self.token_clean().to_string())
            }
            AuthScheme::None => req,
        }
    }

    /// Cert si aquest proveïdor usa l'API d'Anthropic (Claude) en comptes
    /// de la compatible amb OpenAI.
    ///
    /// Es detecta TAMBÉ per la URL, no només pel camp «tipus»: l'API oficial
    /// d'Anthropic i les que en són compatibles (DeepSeek n'ofereix una a
    /// `https://api.deepseek.com/anthropic`) moren amb un 404 si els enviem
    /// `/chat/completions`, i un 404 sempre s'ha llegit com «la meva clau no
    /// funciona». Així, encara que l'entrada vingui amb el tipus mal posat
    /// (o creada per una versió antiga), la petició va ben formada.
    pub fn is_anthropic(&self) -> bool {
        if self.is_native() {
            return false;
        }
        if self.kind.eq_ignore_ascii_case("anthropic") {
            return true;
        }
        let low = self.base_url.to_lowercase();
        low.contains("anthropic.com") || low.ends_with("/anthropic")
    }

    /// Capçaleres d'una petició d'Anthropic. La clau d'API va a `x-api-key`
    /// (el que exigeix l'oficial); si l'usuari va deixar «Bearer» al formulari
    /// s'envia igualment com a `x-api-key`, perquè l'API d'Anthropic no accepta
    /// claus amb Bearer. Els serveis compatibles admeten les dues formes.
    pub fn apply_anthropic(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let req = match self.auth {
            AuthScheme::None => req,
            AuthScheme::ApiKeyHeader => {
                let name = if self.header_name.is_empty() {
                    "x-api-key".to_string()
                } else {
                    self.header_name.clone()
                };
                req.header(name, self.token_clean().to_string())
            }
            AuthScheme::Bearer => req.header("x-api-key", self.token_clean().to_string()),
        };
        req.header("anthropic-version", ANTHROPIC_VERSION)
    }

    /// Cert si aquest proveïdor és la IA NATIVA integrada: un motor Rust que
    /// viu dins del binari de NoOrbit. No fa cap petició HTTP ni necessita
    /// token o descàrrega; funciona només instal·lant l'app.
    pub fn is_native(&self) -> bool {
        self.kind.eq_ignore_ascii_case("native")
    }

    /// Nom del tipus d'API (per a la interfície i la comptabilització):
    /// «native» (motor integrat), «anthropic» (Claude) o «openai».
    pub fn kind_name(&self) -> String {
        if self.is_native() {
            "native".to_string()
        } else if self.is_anthropic() {
            "anthropic".to_string()
        } else {
            "openai".to_string()
        }
    }
}

fn mask(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() <= 8 {
        return "•".repeat(chars.len());
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{}…{}", head, tail)
}

pub struct ApiManager {
    path: PathBuf,
    pub providers: Vec<AiProvider>,
    http: reqwest::Client,
    /// Client sense el tall de 120 s: per a generacions llargues
    /// (Jam autònoma, millora autopilotada). Ve a `new`.
    http_long: reqwest::Client,
}

impl ApiManager {
    pub fn new(data_dir: PathBuf) -> Self {
        let path = data_dir.join("ai_providers.json");
        let mut providers = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<AiProvider>>(&s).ok())
            .unwrap_or_default();
        // Reserva un lloc fixe per a Venice AI si encara no n'hi ha cap
        // entrada: apareix a la llista com a «Venice AI · ⚠ sense token»
        // i només cal prémer l'editat (✏) i hi enganxar el token més tard.
        // Venice exposa una API compatible amb OpenAI, així que no cal cap
        // codi addicional: rebrà `POST {base}/chat/completions`.
        // Reserva un lloc fixe per a Venice AI si encara no n'hi ha cap
        // entrada: apareix a la llista com a «Venice AI · ⚠ sense token»
        // i només cal prémer l'editat (✏) i hi enganxar el token més tard.
        // Venice exposa una API compatible amb OpenAI, així que no cal cap
        // codi addicional: rebrà `POST {base}/chat/completions`.
        let has_venice = providers.iter().any(|p| p.id == "venice");
        // PROVEÏDORS EUROPEUS: s'assequen SI SEMPRE (idempotent) perquè siguin
        // «connectar i enganxar el token». Tots exposen API compatible amb
        // OpenAI («kind: openai»), així que reutilitzen tot el codi de xat
        // existent — cap client nou cal.
        // · Mistral AI (França)        — api.mistral.ai
        // · Aleph Alpha (Alemanya)     — api.pharia-ai.aleph-alpha.com
        // · OpenRouter (Països Baixos) — passarel·la amb models europeus
        const EU_PROVIDERS: [(&str, &str, &str, &str); 3] = [
            (
                "mistral",
                "Mistral AI 🇫🇷",
                "https://api.mistral.ai/v1",
                "mistral-small-latest",
            ),
            (
                "aleph-alpha",
                "Aleph Alpha 🇩🇪",
                "https://api.pharia-ai.aleph-alpha.com/v1",
                "pharia-1-7b-control",
            ),
            (
                "openrouter-eu",
                "OpenRouter 🇪🇺",
                "https://openrouter.ai/api/v1",
                "mistralai/mistral-small-3.1-24b-instruct",
            ),
        ];
        let mut seeded = has_venice;
        for (id, name, base_url, model) in EU_PROVIDERS {
            if !providers.iter().any(|p| p.id == id) {
                providers.push(AiProvider {
                    id: id.into(),
                    name: name.into(),
                    base_url: base_url.into(),
                    model: model.into(),
                    kind: "openai".into(),
                    auth: AuthScheme::Bearer,
                    header_name: String::new(),
                    token: String::new(),
                    enabled: true,
                });
                seeded = true;
            }
        }
        // DEEPSEEK / DEEPINFER: dos llocs fixos per al motor d'inferència
        // alliberat per DeepSeek (github.com/deepseek-ai/DeepInfer, Apache 2.0).
        // · «deepseek»    — API oficial al núvol. DeepSeek exposa DUES entrades
        //   vàlides: la compatible amb OpenAI (`https://api.deepseek.com`) i una
        //   d'estil Anthropic (`https://api.deepseek.com/anthropic`) per als
        //   clients de Claude. Compartint el mateix token, la segona fa que
        //   NoOrbit es pugui connectar a l'ecosistema Claude sense canviar
        //   res: `is_anthropic()` detecta la URL i la petició surt ben formada.
        // · «deepinfer»   — el motor DEEPINFER executat en una màquina Linux
        //   amb GPU NVIDIA SM100 o NPU Ascend 950 (la versió oficial només és
        //   inferència offline: es exposa per HTTP quan es fa servir un servei
        //   vLLM/SGLang davant; per defecte porta al port 8000 de vLLM).
        //   Desactiu per defecte: només s'activa quan l'usuari té el servidor.
        const GOLD: (&str, &str, &str, &str) = (
            "deepseek",
            "DeepSeek API (núvol)",
            "https://api.deepseek.com",
            "deepseek-flash",
        );
        if !providers.iter().any(|p| p.id == GOLD.0) {
            providers.push(AiProvider {
                id: GOLD.0.into(),
                name: GOLD.1.into(),
                base_url: GOLD.2.into(),
                model: GOLD.3.into(),
                kind: "openai".into(),
                auth: AuthScheme::Bearer,
                header_name: String::new(),
                token: String::new(),
                enabled: true,
            });
            seeded = true;
        }
        if !providers.iter().any(|p| p.id == "deepinfer") {
            providers.push(AiProvider {
                id: "deepinfer".into(),
                name: "DeepInfer (servidor GPU)".into(),
                base_url: "http://127.0.0.1:8000/v1".into(),
                model: "deepseek-ai/DeepSeek-V4.1-Flash-1017".into(),
                kind: "openai".into(),
                auth: AuthScheme::Bearer,
                header_name: String::new(),
                token: String::new(),
                // Desactiu: no té sentit enviar peticions si l'usuari encara
                // no té DeepInfer executant-se en una GPU compatible.
                enabled: false,
            });
            seeded = true;
        }
        if !has_venice {
            providers.push(AiProvider {
                id: "venice".into(),
                name: "Venice AI".into(),
                base_url: "https://api.venice.ai/api/v1".into(),
                // Model oficial de la documentació de Venice («venice-
                // uncensored»): abans posàvem «qwen3-coder» i, si el servei
                // l'ha reanomenat, l'error 400 semblava «la clau no funciona».
                model: "venice-uncensored".into(),
                kind: "openai".into(),
                auth: AuthScheme::Bearer,
                header_name: String::new(),
                token: String::new(),
                enabled: true,
            });
            seeded = true;
        }
        // CLAUDE (Anthropic): l'API oficial, que NO és compatible amb OpenAI.
        // Abans «Claude no funcionava» precisament per això: necessita una
        // ruta pròpia (`/v1/messages`), una capçalera pròpia (`x-api-key`) i
        // un mode de raonament que canvia segons la generació del model. Tot
        // això ho decideix `build_request`, per tant ací només es reserva el
        // lloc perquè només calga enganxar la clau.
        if !providers.iter().any(|p| p.id == "claude") {
            providers.push(AiProvider {
                id: "claude".into(),
                name: "Claude (Anthropic)".into(),
                base_url: "https://api.anthropic.com/v1".into(),
                // Model vigent de la documentació de l'API: el més equilibrat
                // en velocitat i intel·ligència. Els antics els pot escriure
                // l'usuari a mà i segueixen funcionant.
                model: "claude-sonnet-5-5".into(),
                kind: "anthropic".into(),
                // Anthropic NO accepta «Authorization: Bearer»: la clau va a
                // la capçalera «x-api-key».
                auth: AuthScheme::ApiKeyHeader,
                header_name: "x-api-key".into(),
                token: String::new(),
                enabled: true,
            });
            seeded = true;
        }
        // IA NATIVA: el motor integrat al binari (sense descàrrega ni token).
        // Es sembra SEMPRE al capdavant de la llista perquè «només instal·lant»
        // NoOrbit ja tinga una IA que funciona en aquest Mac (també Intel).
        if !providers.iter().any(|p| p.id == crate::ai::native::PROVIDER_ID) {
            providers.insert(
                0,
                AiProvider {
                    id: crate::ai::native::PROVIDER_ID.into(),
                    name: crate::ai::native::PROVIDER_NAME.into(),
                    base_url: String::new(),
                    model: crate::ai::native::MODEL_NAME.into(),
                    kind: crate::ai::native::KIND.into(),
                    auth: AuthScheme::None,
                    header_name: String::new(),
                    token: String::new(),
                    enabled: true,
                },
            );
            seeded = true;
        }
        // MINIMAX (gratuït): API compatible amb OpenAI (api.minimax.io/v1),
        // models potents amb 1M de context. Lloc fixe per si l'usuari hi vol
        // enganxar el seu token gratuït; també serveix de teacher de la IA
        // nativa quan no hi ha model local. No queda configurat fins al token.
        if !providers.iter().any(|p| p.id == "minimax") {
            providers.push(AiProvider {
                id: "minimax".into(),
                name: "MiniMax (gratuït)".into(),
                base_url: "https://api.minimax.io/v1".into(),
                model: "MiniMax-M3.1-Flash".into(),
                kind: "openai".into(),
                auth: AuthScheme::Bearer,
                header_name: String::new(),
                token: String::new(),
                enabled: true,
            });
            seeded = true;
        }
        // Migració suau del que ja estava desenat: URLs normals (espais,
        // barres, /api/v1 que faltava) i el model Venice per defecte antic.
        // Així una configuració creationada amb la versió prèvia torna a
        // funcionar sense que l'usuari haja de tocar res.
        let mut migrated = false;
        for p in providers.iter_mut() {
            let nb = normalize_base(&p.base_url);
            if nb != p.base_url {
                p.base_url = nb;
                migrated = true;
            }
            if p.id == "venice" && p.model.trim() == "qwen3-coder" {
                p.model = "venice-uncensored".into();
                migrated = true;
            }
            // DeepSeek: la base documentada ja no porta «/v1» i «deepseek-chat»
            // /«deepseek-reasoner» eren el nom antic del mateix motor d'avui.
            // Migrar-ho evita el 404 que feia dir que «la clau no funciona».
            if p.id == "deepseek" {
                if p.base_url.trim_end_matches('/') == "https://api.deepseek.com/v1" {
                    p.base_url = "https://api.deepseek.com".into();
                    migrated = true;
                }
                if matches!(p.model.trim(), "deepseek-chat" | "deepseek-reasoner") {
                    p.model = "deepseek-flash".into();
                    migrated = true;
                }
            }
            // Un Claude desat per una versió anterior (amb Bearer o amb tipus
            // «openai») es corregeix: l'oficial rebutja totes dues coses.
            if p.id == "claude" && (p.kind != "anthropic" || !matches!(p.auth, AuthScheme::ApiKeyHeader)) {
                p.kind = "anthropic".into();
                p.auth = AuthScheme::ApiKeyHeader;
                p.header_name = "x-api-key".into();
                migrated = true;
            }
            let tk = p.token.trim().to_string();
            if tk != p.token {
                p.token = tk;
                migrated = true;
            }
        }
        let mgr = Self {
            path,
            providers,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
            // Client per a GENERACIONS LLARGUES (Jam autònoma, millora
            // autopilotada): un model local en CPU pot tardar més de mitja
            // hora a escriure un fitxer sencer. El client de 120 s tallava
            // totes les passades i la Jam acabava sense haver escrit codi.
            http_long: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(20))
                .timeout(std::time::Duration::from_secs(60 * 60))
                .build()
                .unwrap_or_default(),
        };
        // només desem al disc si hem canviat alguna cosa (lloc nou per a
        // Venice o migració d'entrades existents).
        if seeded || migrated {
            let _ = mgr.save();
        }
        mgr
    }

    fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.providers)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<AiProviderPublic> {
        self.providers.iter().map(|p| p.to_public()).collect()
    }

    fn find(&mut self, id: &str) -> Result<&mut AiProvider> {
        self.providers
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow!("Proveïdor no trobat: {}", id))
    }

    pub fn add(&mut self, mut item: AiProvider) -> Result<AiProviderPublic> {
        if item.id.is_empty() {
            item.id = format!("prov_{}", std::time::UNIX_EPOCH.elapsed()?.as_millis());
        }
        // Neteja la configuració abans de desenar-la: una URL o una clau
        // copiades de la documentació porten espais i barres de sobra.
        item.name = item.name.trim().to_string();
        item.base_url = normalize_base(&item.base_url);
        item.model = item.model.trim().to_string();
        item.token = item.token.trim().to_string();
        if item.kind.trim().is_empty() {
            item.kind = "openai".into();
        }
        if matches!(item.auth, AuthScheme::None) && item.token.is_empty() {
            // Per defecte, Bearer si no s'indica el contrari — PERÒ un servei
            // LOCAL (llama.cpp, LM Studio, vLLM…) no demana cap clau: enviar-li
            // «Authorization: Bearer » (buit) l'amoïria i alguns serveis hi
            // responen amb 401. Aquestes es queden sense capçalera.
            if !is_local_url(&item.base_url) {
                item.auth = AuthScheme::Bearer;
            }
        }
        let pub_item = item.to_public();
        self.providers.push(item);
        self.save()?;
        Ok(pub_item)
    }

    /// Actualitza camps opcionals. Si `token` és None, conserva l'existent.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        id: &str,
        name: Option<String>,
        base_url: Option<String>,
        model: Option<String>,
        kind: Option<String>,
        auth: Option<AuthScheme>,
        header_name: Option<String>,
        token: Option<String>,
        enabled: Option<bool>,
    ) -> Result<AiProviderPublic> {
        let item = self.find(id)?;
        // La IA nativa és integrada: es conserva el seu tipus, auth i URL
        // (buida) encara que la UI envie una patch amb valors d'un altre tipus.
        let native = item.is_native();
        if let Some(v) = name {
            item.name = v.trim().to_string();
        }
        if let Some(v) = base_url {
            if !native {
                item.base_url = normalize_base(&v);
            }
        }
        if let Some(v) = model {
            item.model = v.trim().to_string();
        }
        if let Some(v) = kind {
            if !native {
                item.kind = v;
            }
        }
        if let Some(v) = auth {
            if !native {
                item.auth = v;
            }
        }
        if let Some(v) = header_name {
            item.header_name = v.trim().to_string();
        }
        if let Some(v) = token {
            item.token = v.trim().to_string();
        }
        if let Some(v) = enabled {
            item.enabled = v;
        }
        let pub_item = item.to_public();
        self.save()?;
        Ok(pub_item)
    }

    pub fn remove(&mut self, id: &str) -> Result<()> {
        // La IA nativa està integrada al programa: no es pot esborrar (només
        // activar/desactivar). Esborrar-la deixaria l'app sense IA de sèrie.
        if id == crate::ai::native::PROVIDER_ID {
            return Err(anyhow!(
                "La IA nativa està integrada al programa i no es pot esborrar; pots desactivar-la."
            ));
        }
        self.providers.retain(|p| p.id != id);
        self.save()?;
        Ok(())
    }

    /// Prepara una crida: retorna una còpia del proveïdor i un client HTTP.
    /// Així no cal retenir el mutex a través de l'`await` (el guard no és `Send`).
    /// Amb `long_run=true` el client és el de GENERACIONS LLARGUES (feines
    /// autònomes: sense el timeout de 120 s de la interacció humana).
    pub fn prepare(&self, id: &str, long_run: bool) -> Result<(AiProvider, reqwest::Client)> {
        let item = self
            .providers
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| anyhow!("Proveïdor no trobat: {}", id))?;
        if !item.enabled {
            return Err(anyhow!("El proveïdor està desactivat"));
        }
        // Millor un missatge clar ací que un 401 incomprensible del servei:
        // «Bearer » amb la clau buida és precisament el que fa que sembla
        // que «les claus no funcionen». Els servidors LOCALS (LM Studio,
        // llama.cpp…) no demanen cap clau, així que ells sí que passen.
        if !item.has_token() {
            let low = item.base().to_lowercase();
            let local = is_local_url(&low);
            if !local {
                return Err(anyhow!(
                    "Aquest proveïdor no té cap clau API guardada. Obre el menú \
                     «Proveïdors d'IA», fes clic a llapis (edita) sobre \"{}\" i \
                     enganxa la clau que t'ha donat el servei (a Venice comença \
                     per «ven-»). La clau no surt mai del teu ordinador.",
                    item.name
                ));
            }
        }
        Ok((
            item,
            if long_run { self.http_long.clone() } else { self.http.clone() },
        ))
    }
}

/// Tradueix un error HTTP del servei a una explicació accions: la majoria de
/// «la meua clau no funciona» són en realitat un 401 (clau mal enganxada o
/// sense crèdit) o un 404 (URL base / model incorrectes).
fn http_error(status: reqwest::StatusCode, body: &str) -> anyhow::Error {
    let hint = match status.as_u16() {
        401 | 403 => " La clau no s'ha acceptat (401/403): comprova que l'has \
             enganxada SENCERA i sense espais (les de Venice comencen per \
             «ven-»), que és del mateix servei i que el compte té crèdit o \
             pla actiu.".to_string(),
        404 => " No s'ha trobat res a eixa URL (404): la URL base ha d'incloure \
             la versió (a Venice, https://api.venice.ai/api/v1) i el model \
             ha d'existir al servei (p. ex. «venice-uncensored»).".to_string(),
        429 => " El servei respon 429: massa peticions seguides o saldo \
             esgotat.".to_string(),
        _ => String::new(),
    };
    anyhow!("Error {}: {}{}", status, body, hint)
}

/// Xat contra un proveïdor remot. Suporta dos formats:
/// - compatible amb OpenAI (`POST {base}/chat/completions`)
/// - Anthropic / Claude (`POST {base}/messages`)
///
/// `conversation` és el FIL COMPLE del xat (historial + missatge actual, roles
/// alternats i acabat en «user»): si només hi ha un torn, és un xat nou.
/// Retorna el text de la resposta i, si el model n'emitix, el seu
/// *pensament* (raonament) per separat.
pub async fn chat(
    http: &reqwest::Client,
    provider: &AiProvider,
    conversation: &[(String, String)],
    system: Option<&str>,
) -> Result<(String, Option<String>)> {
    chat_with_images(http, provider, conversation, system, &[]).await
}

/// Com `chat`, però adjuntant IMATGES (visió) a l'últim torn d'usuari. Abans
/// les imatges només arribaven a Ollama: amb Claude o DeepSeek la IA veia una
/// RUTA de fitxer i no la imatge, i l'usuari ho llegia com «no funciona».
pub async fn chat_with_images(
    http: &reqwest::Client,
    provider: &AiProvider,
    conversation: &[(String, String)],
    system: Option<&str>,
    images: &[String],
) -> Result<(String, Option<String>)> {
    chat_impl(http, provider, conversation, system, images, false).await
}

/// Xat CURT, per a l'autocompletat de l'editor: sense raonament i amb poca
/// sortida. Claude pensant abans d'escriure el fragment no arribava dins dels
/// 8 segons que el guard de l'editor tolera, i el suggeriment senzillament no
/// apareixia (semblava que «la IA no funciona amb l'editor»).
pub async fn chat_quick(
    http: &reqwest::Client,
    provider: &AiProvider,
    conversation: &[(String, String)],
    system: Option<&str>,
) -> Result<(String, Option<String>)> {
    chat_impl(http, provider, conversation, system, &[], true).await
}

async fn chat_impl(
    http: &reqwest::Client,
    provider: &AiProvider,
    conversation: &[(String, String)],
    system: Option<&str>,
    images: &[String],
    quick: bool,
) -> Result<(String, Option<String>)> {
    // IA NATIVA: resposta local del motor integrat (sense HTTP ni token).
    if provider.is_native() {
        let prompt = native_prompt(conversation);
        let brain = crate::ai::native::load_brain();
        let lessons = crate::ai::native::lessons_from_system(system);
        return Ok((crate::ai::native::answer(&prompt, &brain, &lessons), None));
    }
    let parts = load_images(images);
    let req = build_request(provider, conversation, system, &parts, false, quick);
    send_chat(http, provider, req).await
}

/// L'últim torn d'usuari: és la pregunta que rep la IA nativa.
fn native_prompt(conversation: &[(String, String)]) -> String {
    conversation
        .iter()
        .rev()
        .find(|(r, _)| r == "user")
        .map(|(_, c)| c.clone())
        .unwrap_or_default()
}

/// Una petició de xat ja muntada: a quina URL va, quin cos porta i si és
/// d'Anthropic (cal saber-ho per posar-hi les capçaleres correctes i per
/// saber com s'ha de llegir la resposta).
struct ChatRequest {
    url: String,
    body: serde_json::Value,
    anthropic: bool,
}

/// Endpoint d'Anthropic des de la base: va bé amb `https://api.anthropic.com`,
/// amb `https://api.anthropic.com/v1` i amb qualsevol servei compatible, com
/// ara l'entrada d'Anthropic de DeepSeek (`https://api.deepseek.com/anthropic`).
fn anthropic_endpoint(base: &str) -> String {
    if base.ends_with("/v1") {
        format!("{}/messages", base)
    } else {
        format!("{}/v1/messages", base)
    }
}

/// Contingut d'un missatge: una CADENA simple si no hi ha imatges (alguns
/// servidors propis rebutgen la forma de llista), i una llista de BLOCS si
/// n'hi ha. Anthropic vol `{type:"image",source:{type:"base64",…}}`; el
/// format OpenAI, `{type:"image_url",image_url:{url:"data:…"}}`.
fn content_value(text: &str, images: &[ImagePart], anthropic: bool) -> serde_json::Value {
    if images.is_empty() {
        return serde_json::Value::String(text.to_string());
    }
    let mut parts = vec![serde_json::json!({ "type": "text", "text": text })];
    for img in images {
        if anthropic {
            parts.push(serde_json::json!({
                "type": "image",
                "source": { "type": "base64", "media_type": img.mime, "data": img.data }
            }));
        } else {
            parts.push(serde_json::json!({
                "type": "image_url",
                "image_url": { "url": format!("data:{};base64,{}", img.mime, img.data) }
            }));
        }
    }
    serde_json::Value::Array(parts)
}

/// Construeix la petició de xat (amb o sense streaming) en el format del
/// proveïdor. És L'ÚNIC lloc on es decideixen el sostre de sortida i el mode de
/// raonament; si un servei els rebutja, `degrade_request` els rebaixa i
/// s'ho torna a intentar. Així és com Claude i DeepSeek funcionen amb els
/// models d'avui i amb els que apareguin demà, sense tocar codi.
fn build_request(
    provider: &AiProvider,
    conversation: &[(String, String)],
    system: Option<&str>,
    images: &[ImagePart],
    stream: bool,
    quick: bool,
) -> ChatRequest {
    let base = provider.base();
    // Les imatges van a l'últim torn d'usuari: és el que s'està contestant.
    let last_user = conversation
        .iter()
        .rposition(|(r, _)| r == "user")
        .unwrap_or(usize::MAX);

    if provider.is_anthropic() {
        // Anthropic: el «system» va FORA del fil, i cap element del fil no en
        // pot ser un rol «system» (un historial amb system ficat = 400).
        let msgs: Vec<serde_json::Value> = conversation
            .iter()
            .enumerate()
            .filter(|(_, (role, _))| role != "system")
            .map(|(i, (role, content))| {
                let imgs = if i == last_user { images } else { &[] as &[ImagePart] };
                serde_json::json!({
                    "role": role,
                    "content": content_value(content, imgs, true)
                })
            })
            .collect();
        // En mode curt no es pensa res: el que cal és el fragment, de seguida.
        let (mode, max_tokens) = if quick {
            ("none", QUICK_MAX_TOKENS)
        } else {
            anthropic_shape(&provider.model)
        };
        let mut body = serde_json::json!({
            "model": provider.model.as_str(),
            "max_tokens": max_tokens,
            "system": system.unwrap_or("Ets l'assistent de NoOrbit."),
            "messages": msgs,
        });
        match mode {
            // Models nous: el model decideix quant pensar. Models antics: els
            // hi hem de dir nosaltres, amb un pressupost CLARAMENT inferior al
            // sostre de sortida (si no, l'API respon 400).
            "adaptive" => body["thinking"] = serde_json::json!({ "type": "adaptive" }),
            "enabled" => {
                let budget = (max_tokens / 2).max(1_024).min(max_tokens.saturating_sub(1_024));
                body["thinking"] =
                    serde_json::json!({ "type": "enabled", "budget_tokens": budget });
            }
            _ => {}
        }
        if stream {
            body["stream"] = serde_json::json!(true);
        }
        ChatRequest {
            url: anthropic_endpoint(&base),
            body,
            anthropic: true,
        }
    } else {
        let mut messages: Vec<serde_json::Value> = Vec::new();
        if let Some(s) = system {
            messages.push(serde_json::json!({ "role": "system", "content": s }));
        }
        for (i, (role, content)) in conversation.iter().enumerate() {
            let imgs = if i == last_user { images } else { &[] as &[ImagePart] };
            messages.push(serde_json::json!({
                "role": role,
                "content": content_value(content, imgs, false)
            }));
        }
        let mut body = serde_json::json!({
            "model": provider.model.as_str(),
            "messages": messages,
            "stream": stream,
        });
        // DeepSeek talla la sortida a ~4K si no se li demana el contrari: els
        // fitxers que escrivien els agents quedaven a mitja. La resta de
        // serveis no reben el paràmetre (alguns models el rebutgen).
        if quick {
            body["max_tokens"] = serde_json::json!(QUICK_MAX_TOKENS);
        } else if let Some(mt) = openai_max_tokens(&provider.model) {
            body["max_tokens"] = serde_json::json!(mt);
        }
        if stream {
            // Demana el compte de tokens al final del flux: sense això el
            // proveïdor no diu quant ha consumit (necessari per a saber el
            // cost real i quant del context s'ha ocupat).
            body["stream_options"] = serde_json::json!({ "include_usage": true });
        }
        ChatRequest {
            url: format!("{}/chat/completions", base),
            body,
            anthropic: false,
        }
    }
}

/// Prou tokens per a una continuació d'una o dues línies, i prou poc perquè la
/// petició es resolgui dins del temps límit de l'editor (`chat_quick`).
const QUICK_MAX_TOKENS: u64 = 1_024;

/// Rebaixa una petició que el servei ha rebutjat pel FORMAT: primer se'n treu
/// el bloc de raonament (l'acusat habitual del 400 a Anthropic), després se'n
/// baixa el sostre de sortida (els serveis amb models petits el posen a 2–4K).
fn degrade_request(req: &mut ChatRequest, attempt: usize) {
    let Some(obj) = req.body.as_object_mut() else {
        return;
    };
    if attempt == 1 {
        obj.remove("thinking");
        obj.remove("reasoning_effort");
    } else {
        // NOMÉS rebaixar: si la petició ja demanava poc (mode curt), un 4096
        // ara l'augmentaria i el següent reintent tornaria a ser rebutjat.
        let cur = obj
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(u64::MAX);
        obj.insert("max_tokens".to_string(), serde_json::json!(cur.min(4_096)));
    }
}

/// Ruta alterna per a un 404: la diferència entre «…/chat/completions» i
/// «…/v1/chat/completions» (o entre «…/messages» i «…/v1/messages») és el que
/// separa un servei que «no va» d'un que funciona, i la documentació de cada
/// proveïdor no s'hi posa d'acord.
fn alt_url(req: &ChatRequest) -> Option<String> {
    if req.anthropic {
        if req.url.ends_with("/v1/messages") {
            Some(req.url.replace("/v1/messages", "/messages"))
        } else if req.url.ends_with("/messages") {
            Some(req.url.replace("/messages", "/v1/messages"))
        } else {
            None
        }
    } else if req.url.ends_with("/v1/chat/completions") {
        Some(req.url.replace("/v1/chat/completions", "/chat/completions"))
    } else if req.url.ends_with("/chat/completions") {
        Some(req.url.replace("/chat/completions", "/v1/chat/completions"))
    } else {
        None
    }
}

/// El 400 és del FORMAT (val la pena reintentar-lo en una versió més simple)
/// o de la CONFIGURACIÓ (model inexistent, saldo)? En el segon cas reintentar
/// només perd temps i confon: l'usuari veuria tres errors pel mateix motiu.
fn is_shape_error(status: u16, text: &str) -> bool {
    if status != 400 {
        return false;
    }
    let low = text.to_lowercase();
    let missing_model = [
        "model_not_found",
        "invalid model",
        "unknown model",
        "does not exist",
        "no such model",
        "not a valid model",
    ]
    .iter()
    .any(|k| low.contains(k));
    if missing_model {
        return false;
    }
    [
        "thinking",
        "budget_tokens",
        "max_tokens",
        "temperature",
        "reasoning",
        "adaptive",
        "stream_options",
        "unexpected",
        "unknown parameter",
        "unsupported",
    ]
    .iter()
    .any(|k| low.contains(k))
}

/// Obre la connexió amb un proveïdor remot, reintentant-la quan el motiu és el
/// FORMAT de la petició (400) o la RUTA (404). Retorna la resposta ENCARA sense
/// llegir-ne el cos: el xat normal el voldrà com a JSON i el streaming com a
/// flux SSE, però els reintents i les capçaleres són idèntics.
async fn open_chat_response(
    http: &reqwest::Client,
    provider: &AiProvider,
    mut req: ChatRequest,
    accept: &str,
) -> Result<reqwest::Response> {
    let mut attempt = 0usize;
    let mut tried_alt_url = false;
    loop {
        let built = http
            .post(&req.url)
            .header("Accept", accept)
            .json(&req.body);
        // Anthropic i els seus compatibles volen «x-api-key» +
        // «anthropic-version»; si l'usuari ha deixat «Bearer» al formulari
        // s'envia igualment com a «x-api-key», que és l'única forma que
        // l'oficial accepta.
        let sent = if req.anthropic {
            provider.apply_anthropic(built)
        } else {
            provider.apply_auth(built)
        };
        let resp = sent
            .send()
            .await
            .map_err(|e| anyhow!("El proveïdor no respon: {}", e))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let text = resp.text().await.unwrap_or_default();
        if status.as_u16() == 404 && !tried_alt_url {
            if let Some(alt) = alt_url(&req) {
                tried_alt_url = true;
                req.url = alt;
                continue;
            }
        }
        attempt += 1;
        if attempt <= 2 && is_shape_error(status.as_u16(), &text) {
            degrade_request(&mut req, attempt);
            continue;
        }
        return Err(http_error(status, &text));
    }
}

/// Envia una petició sense streaming i en llegeix la resposta.
async fn send_chat(
    http: &reqwest::Client,
    provider: &AiProvider,
    req: ChatRequest,
) -> Result<(String, Option<String>)> {
    let anthropic = req.anthropic;
    let resp = open_chat_response(http, provider, req, "application/json").await?;
    let parsed: serde_json::Value = resp.json().await?;
    if anthropic {
        read_anthropic(&parsed)
    } else {
        read_openai(&parsed)
    }
}

// ── Streaming (event unificat «ai://process») ─────────────────────────────
//
// Versió amb streaming token a token de `chat`. Fa `stream: true`, parseja el
// SSE del proveïdor i, per a cada fragment, emet «ai://chunk» (per al xat, com
// fa Ollama) i «ai://process» (amb la fase). Així qualsevol proveïdor remot
// (OpenAI, Groq, Claude…) mostra el procés en temps real igual que Ollama.

/// Com `chat` però amb streaming SSE. `conversation` porta tot el fil del xat;
/// `ctl` i `start` els passa la cap crida (`AiManager::chat_stream`) perquè la
/// cancel·lació, el cronòmetre i els events «ai://chunk»/«ai://process»
/// siguen del XAT que ha enviat el missatge (paral·lel entre xats).
pub async fn chat_stream(
    app: &tauri::AppHandle,
    http: &reqwest::Client,
    provider: &AiProvider,
    conversation: &[(String, String)],
    system: Option<&str>,
    ctl: &std::sync::Arc<crate::ai::SessionCtl>,
    start: std::time::Instant,
) -> Result<(String, Option<String>)> {
    chat_stream_with_images(app, http, provider, conversation, system, &[], ctl, start).await
}

/// Com `chat_stream` amb imatges adjuntes (visió) per als proveïdors remots.
pub async fn chat_stream_with_images(
    app: &tauri::AppHandle,
    http: &reqwest::Client,
    provider: &AiProvider,
    conversation: &[(String, String)],
    system: Option<&str>,
    images: &[String],
    ctl: &std::sync::Arc<crate::ai::SessionCtl>,
    start: std::time::Instant,
) -> Result<(String, Option<String>)> {
    // IA NATIVA: streaming local del motor integrat (mateixos events que la
    // resta de proveïdors). La millora silenciosa es fa a AiManager, que té
    // accés al teacher; ací només es respon amb el cervell carregat.
    if provider.is_native() {
        let prompt = native_prompt(conversation);
        let brain = crate::ai::native::load_brain();
        let lessons = crate::ai::native::lessons_from_system(system);
        let text = crate::ai::native::answer(&prompt, &brain, &lessons);
        crate::ai::native::emit_stream(app, ctl, &prompt, &text, start).await;
        return Ok((text, None));
    }
    let parts = load_images(images);
    if provider.is_anthropic() {
        stream_sse(app, http, provider, conversation, system, &parts, ctl, start, anthropic_delta).await
    } else {
        stream_sse(app, http, provider, conversation, system, &parts, ctl, start, openai_delta).await
    }
}

/// Extreu (text, raonament) d'un fragment JSON SSE d'OpenAI-compatible.
fn openai_delta(v: &serde_json::Value) -> (String, String) {
    let delta = v.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("delta"));
    let text = delta
        .and_then(|d| d.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    let thinking = delta
        .and_then(|d| d.get("reasoning_content").or_else(|| d.get("reasoning")))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    (text, thinking)
}

/// Extreu (text, raonament) d'un fragment JSON SSE d'Anthropic. Només els
/// events `content_block_delta` porten text: `text_delta` → resposta,
/// `thinking_delta` → raonament.
fn anthropic_delta(v: &serde_json::Value) -> (String, String) {
    if v.get("type").and_then(|t| t.as_str()) != Some("content_block_delta") {
        return (String::new(), String::new());
    }
    let delta = v.get("delta");
    let kind = delta.and_then(|d| d.get("type")).and_then(|t| t.as_str()).unwrap_or("");
    match kind {
        "text_delta" => (
            delta.and_then(|d| d.get("text")).and_then(|t| t.as_str()).unwrap_or("").to_string(),
            String::new(),
        ),
        "thinking_delta" => (
            String::new(),
            delta
                .and_then(|d| d.get("thinking"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
        ),
        _ => (String::new(), String::new()),
    }
}

/// Cos comú de streaming: obre la petició (stream:true), llegeix el flux byte
/// a byte, part per línies i, per a cada `data: {json}`, aplica `parse_delta`
/// per obtenir (text, thinking) i emetre'ls cap al XAT `ctl`.
#[allow(clippy::too_many_arguments)]
async fn stream_sse(
    app: &tauri::AppHandle,
    http: &reqwest::Client,
    provider: &AiProvider,
    conversation: &[(String, String)],
    system: Option<&str>,
    images: &[ImagePart],
    ctl: &std::sync::Arc<crate::ai::SessionCtl>,
    start: std::time::Instant,
    parse_delta: fn(&serde_json::Value) -> (String, String),
) -> Result<(String, Option<String>)> {
    // Mateixa construcció i mateixos reintents que el xat sense streaming: si
    // el servei rebutja el mode de raonament o la ruta, se'n rebaixa la
    // petició i s'ho torna a intentar abans de mostrar res.
    let req = build_request(provider, conversation, system, images, true, false);
    let resp = open_chat_response(http, provider, req, "text/event-stream").await?;

    use futures::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    let mut full = String::new();
    let mut thinking = String::new();
    let label = provider.name.clone();
    let model = provider.model.clone();
    // Comptabilització de tokens: l'API de núvol l'envia al final del flux
    // («usage»), i Anthropic la distribueix entre «message_start» i
    // «message_delta». Sense això no es pot saber quant s'ha gastat.
    let mut usage_input: u64 = 0;
    let mut usage_output: u64 = 0;
    // El flux també pot acabar TALLAT pel sostre de sortida; callar-ho era el
    // que feia que un fitxer escrit per la IA semblés «fet» sent incomplet.
    let mut cut = false;

    'stream: loop {
        if ctl.is_cancelled() {
            return Err(anyhow!(crate::ai::CANCELLED_MSG));
        }
        let chunk = match stream.next().await {
            Some(Ok(c)) => c,
            Some(Err(e)) => return Err(anyhow!("Stream tallat: {}", e)),
            None => break,
        };
        buf.extend_from_slice(&chunk);
        // Cursor en lloc de «drain» per línia: desplaçar el buffer restant a
        // cada línia feia que el cost creiés amb el quadrat del flux.
        let mut cursor = 0;
        while let Some(pos) = buf[cursor..].iter().position(|b| *b == b'\n') {
            let mut line = &buf[cursor..cursor + pos];
            cursor += pos + 1;
            while matches!(line.last(), Some(&b'\n') | Some(&b'\r')) {
                line = &line[..line.len() - 1];
            }
            let text = String::from_utf8_lossy(line);
            let text = text.trim();
            // SSE: només les línies «data: …» porten càrrega; la resta
            // («event:», comentaris «:», buit) s'ignoren.
            let Some(payload) = text.strip_prefix("data:") else {
                continue;
            };
            let payload = payload.trim();
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
                continue;
            };
            let (tok, th) = parse_delta(&v);
            // «usage» (OpenAI-compatible) o «message_start/message_delta»
            // (Anthropic): el compte de tokens del torn.
            if let Some(u) = v.get("usage").and_then(|u| u.as_object()) {
                let get = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
                let p = get("prompt_tokens").max(get("input_tokens"));
                let c = get("completion_tokens").max(get("output_tokens"));
                if p > 0 {
                    usage_input = p;
                }
                if c > 0 {
                    usage_output = c;
                }
            }
            if v.get("type").and_then(|t| t.as_str()) == Some("message_start") {
                if let Some(u) = v.get("message").and_then(|m| m.get("usage")) {
                    usage_input = u
                        .get("input_tokens")
                        .and_then(|x| x.as_u64())
                        .unwrap_or(usage_input);
                }
            }
            if v.get("type").and_then(|t| t.as_str()) == Some("message_delta") {
                if let Some(u) = v.get("usage") {
                    usage_output = u
                        .get("output_tokens")
                        .and_then(|x| x.as_u64())
                        .unwrap_or(usage_output);
                }
                // Anthropic avisa del tall amb «stop_reason», no amb un error.
                if v.get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(|s| s.as_str())
                    == Some("max_tokens")
                {
                    cut = true;
                }
            }
            // OpenAI-compatible: «finish_reason: length» = sortida tallada.
            if v.get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("finish_reason"))
                .and_then(|f| f.as_str())
                == Some("length")
            {
                cut = true;
            }
            // Anthropic també emet errors DINS del flux (el servei sobrecarregat
            // o un bloc «thinking» mal format): sense llegir-los, el torn
            // acabava amb una resposta a mitja i un «resposta buida» llunyà.
            if v.get("type").and_then(|t| t.as_str()) == Some("error") && full.is_empty() {
                let msg = v
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("error desconegut durant el flux");
                return Err(anyhow!("El proveïdor ha tallat la generació: {}", msg));
            }
            let ms = start.elapsed().as_millis() as u64;
            if !th.is_empty() {
                thinking.push_str(&th);
                ctl.process(app, &label, &model, "thinking", &th, ms);
            }
            if !tok.is_empty() {
                full.push_str(&tok);
                ctl.chunk(app, &tok);
                ctl.process(app, &label, &model, "streaming", &tok, ms);
                // Bucle desbocat (model petit repetint la mateixa frase): tallar.
                if crate::ai::runaway_repetition(&full) {
                    ctl.process(
                        app,
                        &label,
                        &model,
                        "thinking",
                        "S'ha detectat un bucle de repetició: la generació s'atura per a no bloquejar-se.",
                        ms,
                    );
                    break 'stream;
                }
            }
        }
        if cursor > 0 {
            buf.drain(..cursor);
        }
    }
    let ms = start.elapsed().as_millis() as u64;
    if cut {
        ctl.process(app, &label, &model, "thinking", CUT_NOTICE, ms);
    }
    ctl.process(app, &label, &model, "done", "", ms);
    // Comptabilització de tokens del torn (entrada/sortida): la interfície
    // la mostra sota la resposta. Si el proveïdor no l'ha enviada, s'estima
    // pel nombre de caràcters (~4 caràcters per token) per no deixar-ho en
    // blanc.
    if usage_input > 0 || usage_output > 0 {
        crate::ai::emit_usage(
            app,
            &ctl.id,
            &provider.kind_name(),
            &model,
            usage_input,
            usage_output,
            None,
        );
    }
    if full.trim().is_empty() {
        return Err(anyhow!("Resposta buida del proveïdor"));
    }
    Ok((full, (!thinking.trim().is_empty()).then_some(thinking)))
}

/// Avís que es mostra quan la sortida s'ha acabat pel sostre de tokens. Va al
/// panell de RAONAMENT (o al procés del xat), mai al text de la resposta: els
/// equips llegeixen directives (`NB|`, `RUN|`…) línia a línia i afegir-hi una
/// línia més les hauria fet malbé.
const CUT_NOTICE: &str = "⚠ La sortida s'ha acabat al límit de tokens: la resposta ve truncada. \
Demana-ho en parts més curtes o apuja el sostre de sortida del proveïdor.";

/// Resposta compatible amb OpenAI: el text és a `choices[0].message.content`
/// (cadena o llista de blocs, segons el servidor) i el raonament, si en porta,
/// a `reasoning_content` (DeepSeek, OpenRouter, Ollama Cloud) o `reasoning`.
fn read_openai(parsed: &serde_json::Value) -> Result<(String, Option<String>)> {
    let choice = parsed.get("choices").and_then(|c| c.get(0));
    let message = choice.and_then(|c| c.get("message"));
    let content = content_to_text(message.and_then(|m| m.get("content")));
    let thinking = message
        .and_then(|m| m.get("reasoning_content").or_else(|| m.get("reasoning")))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty());
    // «length» = el model encara tenia coses a dir i el servei l'ha aturat.
    let cut = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(|f| f.as_str())
        == Some("length");
    if content.trim().is_empty() {
        return Err(anyhow!("Resposta buida del proveïdor"));
    }
    Ok((content, note_cut(thinking, cut)))
}

/// El text d'un `content` que ve o bé com a cadena, o bé com a llista de blocs
/// (alguns servidors i els models amb visió ho fan sempre).
fn content_to_text(v: Option<&serde_json::Value>) -> String {
    match v {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Posa l'avís de tall dins del raonament, que només es mostra a la interfície.
fn note_cut(thinking: Option<String>, cut: bool) -> Option<String> {
    if !cut {
        return thinking;
    }
    Some(match thinking {
        Some(t) if !t.trim().is_empty() => format!("{}\n{}", t, CUT_NOTICE),
        _ => CUT_NOTICE.to_string(),
    })
}

/// Anthropic (Claude): el text i el pensament són BLOCS dins de `content`
/// (`text` / `thinking`), i el tall es declara amb `stop_reason` en lloc
/// d'amb un error. Les capçaleres i el sostre de sortida ja els porta la
/// petició (`build_request` + `apply_anthropic`).
fn read_anthropic(parsed: &serde_json::Value) -> Result<(String, Option<String>)> {
    let mut answer = String::new();
    let mut thinking = String::new();
    if let Some(arr) = parsed.get("content").and_then(|c| c.as_array()) {
        for b in arr {
            match b.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                "text" => answer.push_str(b.get("text").and_then(|t| t.as_str()).unwrap_or("")),
                "thinking" => thinking
                    .push_str(b.get("thinking").and_then(|t| t.as_str()).unwrap_or("")),
                _ => {}
            }
        }
    }
    let cut = parsed
        .get("stop_reason")
        .and_then(|s| s.as_str())
        == Some("max_tokens");
    if answer.trim().is_empty() {
        // Un cos buit amb «error» a dalt mereix el missatge del servei, no un
        // «resposta buida» que no diu res de la causa real.
        let detail = parsed
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .map(|e| format!(": {}", e))
            .unwrap_or_default();
        return Err(anyhow!("Resposta buida del proveïdor{}", detail));
    }
    Ok((answer, note_cut((!thinking.trim().is_empty()).then_some(thinking), cut)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proveïdor de prova. `kind` obert a propòsit: molts errors reals venien
    /// d'una configuració desada amb el tipus que NO tocava.
    fn provider(kind: &str, base: &str, model: &str) -> AiProvider {
        let anthropic = kind == "anthropic";
        AiProvider {
            id: "prova".into(),
            name: "Prova".into(),
            base_url: base.into(),
            model: model.into(),
            kind: kind.into(),
            auth: if anthropic {
                AuthScheme::ApiKeyHeader
            } else {
                AuthScheme::Bearer
            },
            header_name: if anthropic { "x-api-key".into() } else { String::new() },
            token: "clau-de-prova".into(),
            enabled: true,
        }
    }

    fn turns() -> Vec<(String, String)> {
        vec![
            ("system".to_string(), "system dins del fil".to_string()),
            ("user".to_string(), "hola".to_string()),
        ]
    }

    #[test]
    fn la_versió_d_un_model_es_llegeix_dins_del_nom() {
        assert_eq!(model_version("claude-sonnet-5-5"), (5, 5));
        assert_eq!(model_version("claude-opus-4-6-20251101"), (4, 6));
        assert_eq!(model_version("claude-3-7-sonnet-20250219"), (3, 7));
        // Les dates no són versions i no ocupen el lloc de la versió.
        assert_eq!(model_version("claude-20251101-sonnet"), (0, 0));
        // Dígits enganxats a lletres: en llegim la part numèrica.
        assert_eq!(model_version("qwen3.5:0.8b"), (3, 5));
        assert_eq!(model_version("gpt-4o-mini"), (4, 0));
        // Sense cap número: «desconegut», que és el que fa anar el mode manual.
        assert_eq!(model_version("deepseek-flash"), (0, 0));
    }

    #[test]
    fn claude_pensa_en_adaptive_des_del_4_6_i_amb_pressupost_abans() {
        // Demanar «enabled» + pressupost a un model nou torna un 400: era la
        // causa que Claude «no funcionés» amb una clau bona.
        assert_eq!(anthropic_shape("claude-sonnet-5-5"), ("adaptive", 64_000));
        assert_eq!(anthropic_shape("claude-haiku-5-5"), ("adaptive", 64_000));
        assert_eq!(anthropic_shape("claude-opus-4-8"), ("adaptive", 64_000));
        assert_eq!(anthropic_shape("claude-opus-4-6-20251101"), ("adaptive", 64_000));
        assert_eq!(anthropic_shape("claude-sonnet-4-5"), ("enabled", 32_000));
        assert_eq!(anthropic_shape("claude-3-7-sonnet"), ("enabled", 16_000));
        assert_eq!(anthropic_shape("claude-3-5-sonnet"), ("none", 8_192));
        // Serveis compatibles sense números al nom (DeepSeek en format Claude).
        assert_eq!(anthropic_shape("deepseek-flash"), ("enabled", 32_000));
    }

    #[test]
    fn deepseek_necessita_el_sostre_de_sortida_explicit() {
        assert_eq!(openai_max_tokens("deepseek-flash"), Some(32_000));
        assert_eq!(openai_max_tokens("deepseek-v4-pro"), Some(32_000));
        // La sèrie «o» d'OpenAI rebutja el paràmetre: no s'hi posa.
        assert_eq!(openai_max_tokens("gpt-4o-mini"), None);
    }

    #[test]
    fn el_format_anthropic_es_reconeix_per_la_url_encara_que_el_tipus_minti() {
        assert!(provider("openai", "https://api.anthropic.com/v1", "claude-sonnet-5-5").is_anthropic());
        assert!(provider("openai", "https://api.deepseek.com/anthropic", "deepseek-flash").is_anthropic());
        assert!(provider("anthropic", "https://altre.example.com", "x").is_anthropic());
        // DeepSeek en format OpenAI-compatible NO és Anthropic.
        assert!(!provider("openai", "https://api.deepseek.com", "deepseek-flash").is_anthropic());
        // La IA nativa mai: no parla HTTP.
        assert!(!provider("native", "https://api.anthropic.com/v1", "natiu").is_anthropic());
    }

    #[test]
    fn la_base_es_normalitza_i_l_endpoint_d_anthropic_porta_v1_messages() {
        // L'usuari sol enganxar la línia de la documentació sencera.
        assert_eq!(
            normalize_base("https://api.anthropic.com/v1/messages/"),
            "https://api.anthropic.com/v1"
        );
        assert_eq!(
            normalize_base("https://api.deepseek.com/anthropic/v1/chat/completions"),
            "https://api.deepseek.com/anthropic/v1"
        );
        assert_eq!(
            anthropic_endpoint("https://api.anthropic.com/v1"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            anthropic_endpoint("https://api.deepseek.com/anthropic"),
            "https://api.deepseek.com/anthropic/v1/messages"
        );
    }

    #[test]
    fn la_ruta_alterna_canvia_entre_v1_i_sense_v1() {
        let req = |url: &str, anthropic: bool| ChatRequest {
            url: url.into(),
            body: serde_json::json!({}),
            anthropic,
        };
        assert_eq!(
            alt_url(&req("https://api.anthropic.com/v1/messages", true)).as_deref(),
            Some("https://api.anthropic.com/messages")
        );
        assert_eq!(
            alt_url(&req("https://api.anthropic.com/messages", true)).as_deref(),
            Some("https://api.anthropic.com/v1/messages")
        );
        assert_eq!(
            alt_url(&req("https://api.deepseek.com/chat/completions", false)).as_deref(),
            Some("https://api.deepseek.com/v1/chat/completions")
        );
    }

    #[test]
    fn claude_reben_el_system_fora_del_fil_i_en_adaptive() {
        let p = provider("anthropic", "https://api.anthropic.com/v1", "claude-sonnet-5-5");
        let req = build_request(&p, &turns(), Some("El sistema"), &[], false, false);
        assert!(req.anthropic);
        assert_eq!(req.url, "https://api.anthropic.com/v1/messages");
        assert_eq!(req.body["system"], "El sistema");
        assert_eq!(req.body["thinking"]["type"], "adaptive");
        assert_eq!(req.body["max_tokens"], 64_000);
        // Cap torn amb rol «system»: l'API d'Anthropic els rebutja amb un 400.
        let msgs = req.body["messages"].as_array().expect("messages");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "hola");
        assert!(req.body.get("stream").is_none());
    }

    #[test]
    fn deepseek_reben_max_tokens_ampli_i_usage_només_en_flux() {
        let p = provider("openai", "https://api.deepseek.com", "deepseek-flash");
        let req = build_request(&p, &turns(), Some("El sistema"), &[], true, false);
        assert!(!req.anthropic);
        assert_eq!(req.url, "https://api.deepseek.com/chat/completions");
        assert_eq!(req.body["max_tokens"], 32_000);
        assert_eq!(req.body["stream"], true);
        assert_eq!(req.body["stream_options"]["include_usage"], true);
        let msgs = req.body["messages"].as_array().expect("messages");
        assert_eq!(msgs[0]["role"], "system");

        // Sense streaming no es demana «stream_options» (alguns servidors el
        // rebutgen) i el flux no es marca.
        let plain = build_request(&p, &turns(), Some("El sistema"), &[], false, false);
        assert_eq!(plain.body["stream"], false);
        assert!(plain.body.get("stream_options").is_none());
    }

    #[test]
    fn el_mode_curt_de_l_autocompletat_no_pensa_i_demana_poc() {
        let claude = provider("anthropic", "https://api.anthropic.com/v1", "claude-sonnet-5-5");
        let req = build_request(&claude, &turns(), Some("sys"), &[], false, true);
        assert!(req.body.get("thinking").is_none());
        assert_eq!(req.body["max_tokens"], QUICK_MAX_TOKENS);

        let deepseek = provider("openai", "https://api.deepseek.com", "deepseek-flash");
        let req = build_request(&deepseek, &turns(), Some("sys"), &[], false, true);
        assert_eq!(req.body["max_tokens"], QUICK_MAX_TOKENS);
    }

    #[test]
    fn les_imatges_van_en_base64_amb_la_forma_de_cada_api() {
        let img = ImagePart {
            mime: "image/png",
            data: "QUJD".into(),
        };
        // Sense imatges, una cadena simple: hi ha servidors que la forma de
        // llista la rebutgen.
        assert_eq!(content_value("hola", &[], true), serde_json::json!("hola"));

        let ant = content_value("què veus?", &[ImagePart {
            mime: img.mime,
            data: img.data.clone(),
        }], true);
        let parts = ant.as_array().expect("blocs");
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[1]["type"], "image");
        assert_eq!(parts[1]["source"]["type"], "base64");
        assert_eq!(parts[1]["source"]["media_type"], "image/png");
        assert_eq!(parts[1]["source"]["data"], "QUJD");

        let oai = content_value("què veus?", &[img], false);
        let parts = oai.as_array().expect("blocs");
        assert_eq!(parts[1]["type"], "image_url");
        assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,QUJD");
    }

    #[test]
    fn el_reintent_degradat_treu_el_raonament_i_no_apuja_mai_el_sostre() {
        let mut req = ChatRequest {
            url: "https://api.anthropic.com/v1/messages".into(),
            body: serde_json::json!({ "thinking": { "type": "adaptive" }, "max_tokens": 64_000 }),
            anthropic: true,
        };
        degrade_request(&mut req, 1);
        assert!(req.body.get("thinking").is_none());
        assert_eq!(req.body["max_tokens"], 64_000);
        degrade_request(&mut req, 2);
        assert_eq!(req.body["max_tokens"], 4_096);

        // Una petició curta no pot acabar demanant MÉS tokens al reintent.
        let mut curt = ChatRequest {
            url: "https://x/v1/messages".into(),
            body: serde_json::json!({ "max_tokens": QUICK_MAX_TOKENS }),
            anthropic: true,
        };
        degrade_request(&mut curt, 2);
        assert_eq!(curt.body["max_tokens"], QUICK_MAX_TOKENS);
    }

    #[test]
    fn un_400_de_format_es_reintenta_però_un_model_inexistent_no() {
        assert!(is_shape_error(400, "`thinking` is not supported for this model"));
        assert!(is_shape_error(400, "max_tokens: field required"));
        // Reintentar-ho només faria perdre temps i ensenyar tres errors iguals.
        assert!(!is_shape_error(400, "model_not_found: claude-3-5-sonnet"));
        assert!(!is_shape_error(400, "unknown model 'claude-9'"));
        assert!(!is_shape_error(401, "thinking"));
    }

    #[test]
    fn la_resposta_d_anthropic_separa_text_i_pensament_i_avisa_del_tall() {
        let ok = serde_json::json!({
            "content": [
                { "type": "thinking", "thinking": "mirant-ho" },
                { "type": "text", "text": "fet" }
            ],
            "stop_reason": "end_turn"
        });
        assert_eq!(read_anthropic(&ok).unwrap(), ("fet".to_string(), Some("mirant-ho".to_string())));

        let tall = serde_json::json!({
            "content": [{ "type": "text", "text": "a mitja" }],
            "stop_reason": "max_tokens"
        });
        let (_, nota) = read_anthropic(&tall).unwrap();
        assert!(nota.unwrap_or_default().contains(CUT_NOTICE));

        // Cos buit amb «error»: el que cal ensenyar és el motiu del servei.
        let err = serde_json::json!({ "content": [], "error": { "message": "model inexistent" } });
        let msg = read_anthropic(&err).unwrap_err().to_string();
        assert!(msg.contains("model inexistent"), "{}", msg);
    }

    #[test]
    fn la_resposta_openai_llegeix_blocs_i_avisa_quan_es_talla() {
        let tall = serde_json::json!({
            "choices": [{
                "message": { "content": "codi", "reasoning_content": "pensant" },
                "finish_reason": "length"
            }]
        });
        let (text, nota) = read_openai(&tall).unwrap();
        assert_eq!(text, "codi");
        let nota = nota.unwrap_or_default();
        assert!(nota.starts_with("pensant"), "{}", nota);
        assert!(nota.contains(CUT_NOTICE), "{}", nota);

        // Alguns servidors tornen el contingut com a llista de blocs.
        let blocs = serde_json::json!({
            "choices": [{
                "message": { "content": [{ "type": "text", "text": "a" }, { "type": "text", "text": "b" }] },
                "finish_reason": "stop"
            }]
        });
        assert_eq!(read_openai(&blocs).unwrap(), ("ab".to_string(), None));

        let buit = serde_json::json!({ "choices": [{ "message": { "content": "" } }] });
        assert!(read_openai(&buit).is_err());
    }
}
