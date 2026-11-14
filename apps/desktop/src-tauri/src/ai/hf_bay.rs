//! Hugging Bay (huggingbay.xyz): cerca i descàrrega de models GGUF.
//!
//! Hugging Bay és un **mirror verificat** dels fitxers de Hugging Face (amb
//! SHA-256 i enllaços signats de 5 minuts). Té la seva pròpia API (JSON-RPC
//! «api/mcp» i REST «api/v1»). Aquí fem tres coses:
//!
//! 1. **Cercar** models GGUF (amb el filtre `format=gguf`).
//! 2. **Saber-ne els detalls**: fitxer recomanat, mida, SHA-256, llicència i
//!    l'enllaç de descàrrega signat.
//! 3. **Descarregar-lo** (amb progrés) i registrar-lo a Ollama com a model
//!    (Modelfile «FROM /ruta/al/fitxer.gguf»), de manera que aparegui al
//!    gestor de models i es pugui fer servir al xat com qualsevol altre.

use crate::ai::CloudModel;
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Base de l'API de Hugging Bay.
const API: &str = "https://huggingbay.xyz/api/mcp";

/// Model de Hugging Bay ja cercat: el que es pot baixar i com.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BayModel {
    /// Identificador interne del mirror (per a `/api/v1/artifacts/{id}`).
    pub id: String,
    /// «owner/model» del repositori d'Hugging Face.
    pub repo: String,
    pub owner: String,
    pub name: String,
    /// Descripció breu.
    pub description: String,
    /// Llicència (etiqueta; no és un permís).
    pub license: String,
    /// Mida total del repositori, en bytes.
    pub size: Option<u64>,
    /// Nombre de descàrregues (per ordenar).
    pub downloads: Option<u64>,
    /// Enllaç de la fitxa al mirror.
    pub url: Option<String>,
    /// Tipus del model segons el mirror («llm», «embedding», «vision»…).
    pub kind: Option<String>,
    /// Tasca del model («text-generation», «feature-extraction»…).
    pub task: Option<String>,
}

impl BayModel {
    /// El model serveix per XATAR? Els d'EMBEDDINGS (o de tasques
    /// especialitzades) NO: el motor local els engega igual, però tota
    /// petició de xat acaba amb «does not support logits computation» i
    /// l'usuari acaba pensant que NoOrbit està trencat.
    pub fn is_chat_model(&self) -> bool {
        let k = self.kind.as_deref().unwrap_or("").trim().to_lowercase();
        let t = self.task.as_deref().unwrap_or("").trim().to_lowercase();
        if k == "embedding" || k == "embeddings" {
            return false;
        }
        if t == "feature-extraction" || t == "embedding" || t == "embeddings" {
            return false;
        }
        if k == "llm" || k == "text-generation" || t == "text-generation" {
            return true;
        }
        // Sense dada del mirror: assumeix que sí (millor}x  no bloquejar).
        true
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BayFile {
    /// Nom del fitxer dins del repositori (p. ex. «Qwen3-4B-Q4_K_M.gguf»).
    pub path: String,
    pub size: Option<u64>,
    pub sha256: Option<String>,
    /// Enllaç signat de descàrrega (caduca a 5 minuts).
    pub download_url: Option<String>,
    /// Quantització detectada (Q4_K_M, Q5_K_M…).
    pub quant: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BayDetails {
    pub model: BayModel,
    /// Fitxer recomanat (el millor per a la màquina).
    pub recommended: Option<BayFile>,
    /// Tots els GGUF disponibles (per triar quantització).
    pub files: Vec<BayFile>,
    /// Estat de la descàrrega segons el mirror.
    pub status: Option<String>,
    pub downloadable: bool,
    /// RAM estimada (GB) que diu el mirror.
    pub ram_gb: Option<u64>,
    /// Quantització que s'ha demanat (Q4_K_M…).
    pub requested_quant: Option<String>,
}

/// Crida una eina de l'MCP de Hugging Bay i retorna el `structuredContent`.
async fn call_tool(
    http: &reqwest::Client,
    tool: &str,
    args: Value,
) -> Result<Value> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": tool, "arguments": args }
    });
    let resp = http
        .post(API)
        .header("Content-Type", "application/json")
        .header("User-Agent", "NoOrbit")
        .timeout(std::time::Duration::from_secs(30))
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("No s'ha pogut contactar amb huggingbay.xyz: {}", e))?;
    if !resp.status().is_success() {
        return Err(anyhow!("huggingbay.xyz ha respost {}", resp.status()));
    }
    let v: Value = resp.json().await?;
    if let Some(err) = v.get("error") {
        return Err(anyhow!(
            "huggingbay.xyz ha donat error: {}",
            err.get("message").and_then(|m| m.as_str()).unwrap_or("desconegut")
        ));
    }
    let sc = v
        .get("result")
        .and_then(|r| r.get("structuredContent"))
        .cloned()
        .ok_or_else(|| anyhow!("huggingbay.xyz no ha retornat dades"))?;
    Ok(sc)
}

/// Cerca models GGUF a Hugging Bay.
pub async fn search(http: &reqwest::Client, query: &str, limit: usize) -> Result<Vec<BayModel>> {
    Ok(search_page(http, query, "best", None, limit).await?.0)
}

/// Pàgina de resultats amb ORDENACIÓ i CURSOR (paginació):
/// - `sort`: «best» (relevància), «downloads» (els més descarregats), «updated»…
/// - `cursor`: el `nextCursor` de la pàgina anterior (per als 5 més
///   descarregats de cada pàgina).
///
/// Retorna (models, cursor següent, total de resultats).
pub async fn search_page(
    http: &reqwest::Client,
    query: &str,
    sort: &str,
    cursor: Option<&str>,
    limit: usize,
) -> Result<(Vec<BayModel>, Option<String>, u64)> {
    let q = query.trim();
    let limit = limit.clamp(1, 40);
    let mut args = serde_json::json!({
        "format": "gguf",
        "limit": limit,
        "sort": sort,
        "detail": "full"
    });
    // Sense text: es demana el catàleg complet ordenat (els més descarregats).
    if !q.is_empty() {
        args["q"] = serde_json::Value::String(q.to_string());
    }
    let mut cur = cursor.filter(|c| !c.is_empty()).map(|c| c.to_string());
    if let Some(c) = &cur {
        args["cursor"] = serde_json::Value::String(c.clone());
    }
    let _ = cur.take();

    // El mirror, en mode ordenat (sense text), retorna UN model per crida i un
    // cursor numèric. A més, marca «downloadable» repositoris que NO tenen cap
    // fitxer GGUF hostatjat (p. ex. Qwen3.8-…-NVFP4-MTP): abans apareixien al
    // catàleg i fallaven en baixar-los («no té cap fitxer descargable»). Aquí
    // es demanen candidats i NOMÉS es mostren els que `get_model` confirma que
    // tenen un fitxer recomanat descarregable — la MATEIXA comprovació que fa
    // la descàrrega. Es fan diverses crides seguides per omplir la pàgina.
    let mut models: Vec<BayModel> = Vec::new();
    let mut next: Option<String> = None;
    let mut total = 0u64;
    let mut calls = 0usize;
    while models.len() < limit && calls < 48 {
        // Recull un lot de candidats (els que falten per omplir la pàgina).
        let want = limit - models.len();
        let mut batch: Vec<BayModel> = Vec::new();
        while batch.len() < want && calls < 48 {
            calls += 1;
            let sc = call_tool(http, "search_artifacts", args.clone()).await?;
            total = sc.get("total").and_then(|v| v.as_u64()).unwrap_or(total);
            batch.extend(parse_rows(&sc));
            next = sc
                .get("nextCursor")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            match &next {
                Some(c) => args["cursor"] = serde_json::Value::String(c.clone()),
                None => break,
            }
        }
        if batch.is_empty() {
            break;
        }
        // Comprova EN PARAL·LEL quins candidats tenen fitxer descarregable.
        let checks =
            futures::future::join_all(batch.iter().map(|m| details(http, &m.id, None))).await;
        for (m, d) in batch.iter().zip(checks) {
            // Només s'amaga quan el mirror confirma que NO hi ha fitxer; un
            // error transitori de xarxa no ha de buidar el catàleg.
            let keep = match d {
                Ok(det) => det.recommended.is_some(),
                Err(_) => true,
            };
            if keep {
                models.push(m.clone());
                if models.len() >= limit {
                    break;
                }
            }
        }
        if next.is_none() {
            break;
        }
    }
    models.truncate(limit);
    Ok((models, next, total))
}

/// Converteix la resposta de la cerca en models (funció PURA: es pot
/// provar sense xarxa).
fn parse_rows(sc: &Value) -> Vec<BayModel> {
    let rows = sc
        .get("rows")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for r in rows {
        let id = r
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if id.is_empty() {
            continue;
        }
        let repo = r
            .get("repo")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let owner = r
            .get("owner")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let name = r
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&repo)
            .to_string();
        let description = r
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let license = r
            .get("licenseLabel")
            .or_else(|| r.get("license"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let size = r.get("sizeBytes").and_then(|v| v.as_u64());
        let downloads = r.get("downloadCount").and_then(|v| v.as_u64());
        let url = r
            .get("webUrl")
            .or_else(|| r.get("sourceUrl"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        out.push(BayModel {
            id,
            repo,
            owner,
            name,
            description,
            license,
            size,
            downloads,
            url,
            kind: r
                .get("type")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string()),
            task: r
                .get("task")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string()),
        });
    }
    out
}

/// Avís quan el model NO serveix per xatejar (embeddings i similars):
/// `None` si el model és de xat.
pub fn avisa_si_no_xateja(m: &BayModel) -> Option<String> {
    if m.is_chat_model() {
        return None;
    }
    Some(format!(
        "«{}» NO és un model de xat: és d'EMBEDDINGS (serveix per calcular vectors, no per \
         generar text). No serveix per crear temes ni per xatejar. Tria un model de \
         text-generation (p. ex. «SmolLM2-135M-Instruct», «Qwen2.5-Coder-7B-Instruct» o \
         «gemma-3-4b-it»).",
        m.name
    ))
}

/// Detalls d'un model: fitxer recomanat, quantitzacions i enllaç signat.
/// `quant` permet demanar una quantització concreta (p. ex. «Q4_K_M»): si el
/// mirror l'ofereix, el fitxer recomanat és el demanat (amb la seva mida i el
/// seu SHA-256).
pub async fn details(
    http: &reqwest::Client,
    id: &str,
    quant: Option<&str>,
) -> Result<BayDetails> {
    let mut args = serde_json::json!({ "id": id, "detail": "full" });
    if let Some(q) = quant.filter(|q| !q.trim().is_empty()) {
        args["quant"] = serde_json::Value::String(q.trim().to_string());
    }
    let sc = call_tool(http, "get_model", args).await?;

    let repo = sc
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let (owner, name) = match repo.split_once('/') {
        Some((o, n)) => (o.to_string(), n.to_string()),
        None => (String::new(), repo.clone()),
    };
    let model = BayModel {
        id: sc
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(id)
            .to_string(),
        repo: repo.clone(),
        owner,
        name,
        description: sc
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        license: sc
            .get("licenseLabel")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        size: sc.get("sizeBytes").and_then(|v| v.as_u64()),
        downloads: sc.get("downloadCount").and_then(|v| v.as_u64()),
        url: sc.get("url").and_then(|v| v.as_str()).map(|s| s.to_string()),
        kind: sc
            .get("type")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string()),
        task: sc
            .get("task")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string()),
    };

    // El fitxer recomanat ve en «mainFile» (amb l'enllaç signat).
    let mut files: Vec<BayFile> = Vec::new();
    if let Some(main) = sc.get("mainFile") {
        if let Some(path) = main.get("path").and_then(|v| v.as_str()) {
            files.push(BayFile {
                path: path.to_string(),
                size: main.get("sizeBytes").and_then(|v| v.as_u64()),
                sha256: main
                    .get("sha256")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                download_url: main
                    .get("downloadUrl")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                quant: quant_de(path),
            });
        }
    }
    // Altres GGUF del repositori (per oferir altres quantitzacions).
    if let Some(list) = sc.get("files").and_then(|v| v.as_array()) {
        for f in list {
            let path = f.get("path").and_then(|v| v.as_str()).unwrap_or_default();
            if !path.to_lowercase().ends_with(".gguf") {
                continue;
            }
            if files.iter().any(|x| x.path == path) {
                continue;
            }
            files.push(BayFile {
                path: path.to_string(),
                size: f.get("sizeBytes").and_then(|v| v.as_u64()),
                sha256: f
                    .get("sha256")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                download_url: f
                    .get("downloadUrl")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                quant: quant_de(path),
            });
        }
    }

    let recommended = files.first().cloned();
    let status = sc
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let downloadable = sc
        .get("downloadable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(BayDetails {
        model,
        recommended,
        files,
        status,
        downloadable,
        ram_gb: sc.get("ramNeededGb").and_then(|v| v.as_u64()),
        requested_quant: sc
            .get("requestedQuant")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

/// Descarrega el GGUF recomanat d'un model del mirror i retorna (detalls,
/// ruta del fitxer).
///
/// El fitxer **es conserva** a la carpeta de models de NoOrbit: serveix per
/// a l'ús NATIU (llama.cpp, sense Ollama) i també per registrar-lo a Ollama.
pub async fn fetch_gguf(
    http: &reqwest::Client,
    app: Option<&tauri::AppHandle>,
    id: &str,
    quant: Option<&str>,
) -> Result<(BayDetails, std::path::PathBuf)> {
    let det = details(http, id, quant).await?;
    let file = det
        .recommended
        .clone()
        .ok_or_else(|| anyhow!("Aquest model no té cap fitxer descargable."))?;
    let url = file
        .download_url
        .clone()
        .ok_or_else(|| anyhow!("El mirror no dona enllaç de descàrrega per a aquest model."))?;
    let base = ollama_name(&det.model.repo);
    let file_name = file
        .path
        .rsplit('/')
        .next()
        .unwrap_or("model.gguf")
        .to_string();
    // Evitem «mxbai-embed-large-v1-mxbai-embed-large-v1-f16.gguf»: si el
    // fitxer ja comença pel nom del repositori, no l'hem de prefixar altre
    // cop (el nom ha de ser llegible: és el que veu l'usuari a la llista).
    let dest_name = if file_name.to_lowercase().starts_with(&base.to_lowercase()) {
        file_name
    } else {
        format!("{}-{}", base, file_name)
    };
    let path = download_file(http, app, id, &url, &dest_name, file.sha256.as_deref()).await?;
    Ok((det, path))
}

/// Detecta la quantització d'un nom de fitxer GGUF (Q4_K_M, Q5_K_M…).
fn quant_de(path: &str) -> Option<String> {
    let up = path.to_uppercase();
    const QUANTS: [&str; 10] = [
        "Q2_K", "Q3_K_S", "Q3_K_M", "Q3_K_L", "Q4_0", "Q4_K_S", "Q4_K_M", "Q5_K_M", "Q6_K", "Q8_0",
    ];
    for q in QUANTS {
        if up.contains(q) {
            return Some(q.to_string());
        }
    }
    if up.contains("Q4") {
        return Some("Q4".to_string());
    }
    if up.contains("Q5") {
        return Some("Q5".to_string());
    }
    if up.contains("F16") || up.contains("BF16") {
        return Some("F16".to_string());
    }
    None
}

/// Converteix un resultat de Hugging Bay en el model genèric de la UI.
pub fn a_cloud_model(m: &BayModel) -> CloudModel {
    CloudModel {
        // El nom que Ollama accepta per baixar des del mirror:
        // «ollama pull huggingbay.xyz/<owner>/<model>:<quant>».
        name: format!("huggingbay.xyz/{}/{}", m.owner, m.name),
        description: if m.license.is_empty() {
            m.description.clone()
        } else {
            format!("{} · {}", m.description, m.license)
        },
        size: m.size,
        // L'enllaç del mirror (per obrir la fitxa i veure els fitxers).
        url: Some(format!("https://huggingbay.xyz/api/v1/artifacts/{}", m.id)),
    }
}

/// Destí on es desen els GGUF baixats (carpeta de dades de NoOrbit).
///
/// Si l'usuari ha triat un DISC EXTERN (IA ▸ Models ▸ «Models GGUF»), els
/// models hi van: es poden fer servir des d'allà i el disc es pot endur a
/// una altra màquina.
pub fn models_dir() -> std::path::PathBuf {
    if let Ok(cfg) = crate::config::AppConfig::load() {
        let d = cfg.ai.gguf_dir.trim();
        if !d.is_empty() {
            return std::path::PathBuf::from(d);
        }
    }
    crate::config::AppConfig::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("models")
}

/// Registre de cancel·lació de descàrregues (per id de model). Cada descàrrega
/// consulta el seu flag en cada fragment: si l'usuari prem «Cancel·la», el
/// bucle s'atura, esborra el «.part» i retorna un error clar.
fn cancels() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static C: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Flag de cancel·lació d'una descàrrega (es crea si no existeix).
pub fn cancel_token(id: &str) -> Arc<AtomicBool> {
    let mut m = cancels().lock().unwrap();
    m.entry(id.to_string())
        .or_insert_with(|| Arc::new(AtomicBool::new(false)))
        .clone()
}

/// Demana cancel·lar la descàrrega en curs d'aquest model.
pub fn request_cancel(id: &str) {
    cancel_token(id).store(true, Ordering::Relaxed);
}

/// Espai (LLIURE, TOTAL) en bytes al disc on es desaran els models. El TOTAL
/// serveix per mostrar la mida del disc/USB destí; el LLIURE, per avisar ABANS
/// de baixar un model enorme (p. ex. 247 GB) que no hi cap.
pub fn disk_usage() -> Option<(u64, u64)> {
    let dir = models_dir();
    // El directori pot no existir encara: pugem fins al pare que existeix.
    let mut p: &std::path::Path = dir.as_path();
    while !p.exists() {
        match p.parent() {
            Some(pp) => p = pp,
            None => break,
        }
    }
    disk_space(p)
}

#[cfg(unix)]
fn disk_space(path: &std::path::Path) -> Option<(u64, u64)> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut st) == 0 {
            let free = st.f_bavail as u64 * st.f_frsize as u64;
            let total = st.f_blocks as u64 * st.f_frsize as u64;
            Some((free, total))
        } else {
            None
        }
    }
}

#[cfg(windows)]
fn disk_space(path: &std::path::Path) -> Option<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            dir: *const u16,
            free_avail: *mut u64,
            total: *mut u64,
            free_total: *mut u64,
        ) -> i32;
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut free_avail = 0u64;
    let mut total = 0u64;
    let mut free_total = 0u64;
    let ok = unsafe {
        GetDiskFreeSpaceExW(wide.as_ptr(), &mut free_avail, &mut total, &mut free_total)
    };
    if ok != 0 {
        Some((free_avail, total))
    } else {
        None
    }
}

#[cfg(not(any(unix, windows)))]
fn disk_space(_path: &std::path::Path) -> Option<(u64, u64)> {
    None
}

/// Esborra les descàrregues PARCIALS («*.part») que hagen quedat als
/// directoris de models. Si l'app es va tancar (o va morir de cop) mentre
/// baixava un model d'una IA, el «.part» a mitges no serveix de res: no es
/// pot fer servir i ocupava disc debades. Es crida en arrencar, al costat
/// de la neteja de servidors orfes (llamacpp::cleanup_stale).
///
/// SEGURETAT: només s'esborren fitxers acabats en «.part»; els models
/// complets (.gguf/.safetensors) no es toquen MAI. Si una carpeta no
/// existeix o no es pot llegir, se salta sense fallar l'arrencada.
pub fn cleanup_partial_downloads() {
    // GGUF (Hugging Bay) i models d'imatge en CPU: els dos llocs on una
    // descàrrega interrompuda deixa un «.part».
    let mut dirs = vec![models_dir()];
    if let Ok(sd) = crate::imgen::sd_models_dir() {
        dirs.push(sd);
    }
    let mut removed = 0usize;
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_part = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("part"))
                .unwrap_or(false);
            if is_part && path.is_file() && std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    if removed > 0 {
        eprintln!(
            "NoOrbit/models: s'han esborrat {} descàrregues parcials (.part) d'una sessió anterior.",
            removed
        );
    }
}

/// Descarrega un fitxer del mirror amb PROGRÉS (esdeveniment «ai://pull»,
/// el mateix que usa la descàrrega d'Ollama) i el desa a la carpeta de models.
///
/// INTEGRITAT: es baixa a un fitxer «.part» i es verifica el SHA-256 que
/// dona el mirror (si en dona). Només quan tot és correcte es renombra al
/// nom final: una descàrrega interrompuda MAI deixa un fitxer parcial que
/// després es donaria per bo (abans passava: el model quedava corrupte i
/// no es tornava a baixar mai).
///
/// L'enllaç signat caduca a 5 minuts: si arriba exhaurit, es torna a demanar
/// el model (amb un enllaç nou) i es reintenta una vegada.
pub async fn download_file(
    http: &reqwest::Client,
    app: Option<&tauri::AppHandle>,
    id: &str,
    url: &str,
    dest_name: &str,
    sha256: Option<&str>,
) -> Result<std::path::PathBuf> {
    use sha2::{Digest, Sha256};
    use tauri::Emitter;
    let dir = models_dir();
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(dest_name);

    // Si ja hi és (desada anterior, verificada en baixar-la), no tornem a
    // baixar-la.
    if let Ok(meta) = std::fs::metadata(&dest) {
        if meta.len() > 0 {
            return Ok(dest);
        }
    }
    // Restes d'una descàrrega interrompuda: es comença de zero (el «.part»
    // mai es dóna per bo).
    let part = dir.join(format!("{}.part", dest_name));
    let _ = std::fs::remove_file(&part);

    // Cancel·lació: nova descàrrega → netegem qualsevol cancel·lació prèvia.
    let cancel = cancel_token(id);
    cancel.store(false, Ordering::Relaxed);

    // Tot el cos de la descàrrega dins d'un bloc: si falla a MITGES (xarxa,
    // disc o rename) el «.part» s'esborra igualment, igual que en la
    // descàrrega de models d'imatge. Una descàrrega interrompuda mai deixa
    // restes que després es donarien per bones.
    let result = async {
        let mut attempts = 0;
        let mut last_err = String::new();
        // L'enllaç signat CADUCA (5 min): si arriba exhaurit es demana un de nou
        // i es REINTENTA amb ell (abans es demanava i es descartava: el bucle
        // repetia la mateixa URL caducada i no passava mai).
        let mut url = url.to_string();
        while attempts < 3 {
            attempts += 1;
            let resp = http
                .get(&url)
                .header("User-Agent", "NoOrbit")
                .timeout(std::time::Duration::from_secs(60 * 30))
                .send()
                .await
                .map_err(|e| anyhow!("No s'ha pogut descarregar: {}", e))?;
            if !resp.status().is_success() {
                last_err = format!("el mirror ha respost {}", resp.status());
                // Enllaç caducat (403/404): en demanem un de nou.
                let nou = match details(http, id, None).await {
                    Ok(d) => d.recommended.and_then(|f| f.download_url),
                    Err(e) => {
                        last_err = format!("{} (i no s'ha pogut demanar un enllaç nou: {})", last_err, e);
                        None
                    }
                };
                match nou {
                    Some(u) => {
                        if u == url {
                            // Mateix enllaç: no hi ha res a reintentar.
                            break;
                        }
                        url = u;
                        continue;
                    }
                    None => break,
                }
            }
            let total = resp.content_length().unwrap_or(0);
            let mut out = std::fs::File::create(&part)?;
            let mut stream = resp.bytes_stream();
            use futures::StreamExt;
            let mut written: u64 = 0;
            let mut last_pct: i64 = -1;
            let mut hasher = Sha256::new();
            while let Some(chunk) = stream.next().await {
                // Cancel·lació de l'usuari: aturem, esborrem el parcial i sortim.
                if cancel.load(Ordering::Relaxed) {
                    drop(out);
                    let _ = std::fs::remove_file(&part);
                    return Err(anyhow!("Descàrrega cancel·lada."));
                }
                let chunk = chunk?;
                use std::io::Write;
                out.write_all(&chunk)?;
                hasher.update(&chunk);
                written += chunk.len() as u64;
                if total > 0 {
                    let pct = (written * 100 / total) as i64;
                    if pct != last_pct {
                        last_pct = pct;
                        if let Some(app) = app {
                            let _ = app.emit(
                                "ai://pull",
                                serde_json::json!({
                                    "model": format!("huggingbay:{}", id),
                                    "message": format!("descarregant {} — {}%", dest_name, pct)
                                }),
                            );
                        }
                    }
                }
            }
            let _ = std::io::Write::flush(&mut out);
            drop(out);
            if total > 0 && written != total {
                last_err = format!("descàrrega incompleta ({} de {} bytes)", written, total);
                continue;
            }
            // Verificació SHA-256: el mirror el dona per a cada fitxer. Si no
            // coincideix, la descàrrega està corrupta: es descarta (mai es dóna
            // per bona) i es diu clarament a l'usuari.
            if let Some(expected) = sha256.filter(|s| !s.trim().is_empty()) {
                if let Some(app) = app {
                    let _ = app.emit(
                        "ai://pull",
                        serde_json::json!({
                            "model": format!("huggingbay:{}", id),
                            "message": format!("verificant SHA-256 de {}…", dest_name)
                        }),
                    );
                }
                let digest = hasher.finalize();
                let actual = hex_digest(&digest);
                if !actual.eq_ignore_ascii_case(expected.trim()) {
                    return Err(anyhow!(
                        "El SHA-256 de «{}» no coincideix amb el del mirror: la descàrrega \
                         està corrupta i s'ha descartat. Torna-ho a provar.",
                        dest_name
                    ));
                }
            }
            // Tot correcte: el nom final NOMÉS existeix quan el fitxer és bo
            // (rename atòmic dins del mateix directori).
            std::fs::rename(&part, &dest)?;
            return Ok(dest);
        }
        Err(anyhow!(
            "No s'ha pogut baixar el model de Hugging Bay: {}",
            if last_err.is_empty() { "error desconegut" } else { &last_err }
        ))
    }
    .await;
    // Qualsevol error (xarxa, disc, SHA, rename o reintents esgotats) deixa
    // el «.part» net: mai queda una descàrrega parcial al disc.
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

/// SHA-256 en hex minúscules (sense dependència «hex»).
fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Registra un GGUF baixat a Ollama perquè aparegui al gestor de models i es
/// pugui fer servir al xat.
///
/// Es fa amb el CLI («ollama create <nom> -f Modelfile»): l'API /api/create
/// d'Ollama no accepta una RUTA LOCAL com a «from» (dona «invalid model
/// name»), però el Modelfile «FROM /ruta/al/fitxer.gguf» sí que funciona i
/// Ollama en crea la seva pròpia còpia (la podem esborrar després).
pub async fn register_in_ollama(
    model_name: &str,
    gguf: &std::path::Path,
) -> Result<String> {
    let bin = crate::ai::ollama_binary()
        .ok_or_else(|| anyhow!("No trobo el binari d'Ollama al sistema"))?;

    // Modelfile temporal: «FROM /ruta/al/fitxer.gguf».
    let modelfile = gguf.with_extension("Modelfile");
    std::fs::write(
        &modelfile,
        format!("FROM {}\n", gguf.to_string_lossy()),
    )?;

    let nom = model_name.to_string();
    let modelfile_path = modelfile.clone();
    let sortida = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&bin)
            .arg("create")
            .arg(&nom)
            .arg("-f")
            .arg(&modelfile_path)
            .output()
    })
    .await
    .map_err(|e| anyhow!("No s'ha pogut executar Ollama: {}", e))?;

    let out = sortida.map_err(|e| anyhow!("No s'ha pogut executar Ollama: {}", e))?;
    // Netegem el Modelfile temporal (el GGUF el borra el cridaner després).
    let _ = std::fs::remove_file(&modelfile);

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let ok = String::from_utf8_lossy(&out.stdout);
        return Err(anyhow!(
            "Ollama no ha creat el model: {}",
            if err.trim().is_empty() { ok.trim() } else { err.trim() }
        ));
    }
    Ok(model_name.to_string())
}

/// Nom del model a Ollama, derivat del nom del repositori (sense espais).
pub fn ollama_name(repo: &str) -> String {
    let n = repo
        .rsplit('/')
        .next()
        .unwrap_or(repo)
        .to_lowercase()
        .replace(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_', "-");
    if n.is_empty() {
        "model-hf-bay".to_string()
    } else {
        n
    }
}

#[cfg(test)]
mod proves {
    use super::*;

    /// Test de XARXA (s'omet per defecte): comprova que el mirror respon amb
    /// la forma que esperem. Per executar-lo:
    ///   cargo test hf_bay -- --ignored --nocapture
    #[test]
    #[ignore]
    fn xarxa_omple_la_pagina() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let http = reqwest::Client::builder()
                .user_agent("NoOrbit")
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap();
            let (models, next, total) =
                search_page(&http, "", "downloads", None, 5).await.unwrap();
            println!("models: {} next={:?} total={}", models.len(), next, total);
            for m in &models {
                println!(
                    "  - {:40} ⬇{:>9} {:.1} GB",
                    m.id,
                    m.downloads.unwrap_or(0),
                    m.size.unwrap_or(0) as f64 / 1e9
                );
            }
            assert!(models.len() >= 1, "el mirror hauria de retornar almenys un model");
            assert!(total > 0);
            // Detalls amb quantització demanada.
            let d = details(&http, &models[0].id, Some("Q4_K_M")).await.unwrap();
            println!("detall: {:?} ram={:?}", d.model.name, d.ram_gb);
            assert!(!d.model.name.is_empty());
        });
    }

    /// La cerca ha de llegir els camps que retorna el mirror (id, repo,
    /// owner, mida, descàrregues…) i el nom ha de ser el que Ollama accepta.
    #[test]
    fn parseja_la_cerca_del_mirror() {
        let sc = serde_json::json!({
            "rows": [{
                "id": "hf-model-qwen-qwen3-4b-gguf",
                "repo": "Qwen/Qwen3-4B-GGUF",
                "owner": "Qwen",
                "name": "Qwen3-4B-GGUF",
                "summary": "Model from Hugging Face for text generation",
                "licenseLabel": "apache-2.0",
                "sizeBytes": 15797191012u64,
                "downloadCount": 397110u64,
                "webUrl": "https://huggingbay.xyz/m/hf-model-qwen-qwen3-4b-gguf"
            }]
        });
        let models = parse_rows(&sc);
        assert_eq!(models.len(), 1);
        let m = &models[0];
        assert_eq!(m.id, "hf-model-qwen-qwen3-4b-gguf");
        assert_eq!(m.repo, "Qwen/Qwen3-4B-GGUF");
        assert_eq!(m.owner, "Qwen");
        assert_eq!(m.size, Some(15797191012u64));
        assert_eq!(m.license, "apache-2.0");
        // El nom que es mostra ha de ser l'acceptat per Ollama/mirror.
        let cloud = a_cloud_model(m);
        assert_eq!(cloud.name, "huggingbay.xyz/Qwen/Qwen3-4B-GGUF");
        assert!(cloud.url.unwrap().contains("hf-model-qwen-qwen3-4b-gguf"));
    }

    /// Detecta la quantització pel nom del fitxer.
    #[test]
    fn detecta_la_quantitzacio() {
        assert_eq!(quant_de("Qwen3-4B-Q4_K_M.gguf").as_deref(), Some("Q4_K_M"));
        assert_eq!(quant_de("model-Q5_K_M.gguf").as_deref(), Some("Q5_K_M"));
        assert_eq!(quant_de("model-Q8_0.gguf").as_deref(), Some("Q8_0"));
        assert_eq!(quant_de("model.bin"), None);
    }

    /// El nom del model a Ollama ha de ser net (ni espais ni barres).
    #[test]
    fn normalitza_el_nom_a_ollama() {
        assert_eq!(ollama_name("unsloth/SmolLM2-135M-Instruct-GGUF"), "smollm2-135m-instruct-gguf");
        assert_eq!(ollama_name("Qwen/Qwen3 4B"), "qwen3-4b");
    }

    /// El SHA-256 es calcula en hex minúscules (vector conegut: cadena buida).
    #[test]
    fn sha256_hex_del_vector_conegut() {
        use sha2::{Digest, Sha256};
        let d = Sha256::digest(b"");
        assert_eq!(
            hex_digest(&d),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
