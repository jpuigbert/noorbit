//! Generació i manipulació d'imatges per a NoOrbit.
//!
//! Dos camins, triats segons la potència real de la màquina (el requisit de
//! l'usuari: «si la IA és suficientment potent»):
//! - **Local**: ComfyUI en marxa a localhost:8188, amb text→imatge
//!   (txt2img) i image→image (img2img) mitjançant els seus grafs estàndard.
//!   Només s'usa si hi ha RAM lliure suficient per al model (≥2 GB a 512 px,
//!   ≥4 GB a resolucions majors): si no n'hi ha, no saturim l'ordinador.
//! - **En línia**: proveïdors oficials registrats per l'usuari amb token
//!   (OpenAI «dall-e-3», Venice, etc.) amb `/images/generations` i
//!   `/images/edits`. Sense credencials explícites, aquest camin NO s'intenta.
//!
//! Si cap dels dos és disponible, l'error explica com activar l'un o l'altre
//! (`start_comfyui`/`install_comfyui` o el menú de Proveïdors d'IA).
//! La manipulació determinista (redimensionar, retallar, girar…) usa el
//! crate `image`, sense IA.

use crate::api::ApiManager;
use crate::config::AppConfig;
use crate::external_orchestrator::agent::OrchestratorAgent;
use anyhow::{anyhow, Result};
use base64::Engine as _;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Una imatge ja generada i desada a disc.
#[derive(Debug, Clone, Serialize)]
pub struct GeneratedImage {
    /// Ruta on s'ha desat el PNG.
    pub path: String,
    /// Backend que la ha generada: «comfyui (local)» o «<proveïdor> (online)».
    pub backend: String,
    /// Model concret usat.
    pub model: String,
}

/// Opcions de generació/manipulació amb valors per defecte raonables.
#[derive(Debug, Clone)]
pub struct GenOpts {
    pub prompt: String,
    pub negative: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    /// 0 = llavor aleatòria cada vegada.
    pub seed: u64,
    /// Model del backen (ckpt de ComfyUI o model d'imatges remot).
    pub model: Option<String>,
}

impl Default for GenOpts {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            negative: "blurry, low quality, distorted".into(),
            width: 512,
            height: 512,
            steps: 24,
            seed: 0,
            model: None,
        }
    }
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        // Una generació pot tardar minuts (model carregant a RAM): temps
        // llarg, com el del xat principal.
        .timeout(Duration::from_secs(600))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default()
}

/// RAM lliure mínima per generar en local segons la amplada triada.
fn enough_ram(width: u32) -> bool {
    let need: u64 = if width <= 512 { 2 } else { 4 } * 1024 * 1024 * 1024;
    match OrchestratorAgent::free_memory_bytes() {
        Some(free) => free >= need,
        None => true,
    }
}

fn data_generated_dir() -> Result<PathBuf> {
    let dir = AppConfig::data_dir()
        .ok_or_else(|| anyhow!("No es pot determinar la carpeta de dades de NoOrbit"))?
        .join("generated");
    std::fs::create_dir_all(&dir).map_err(|e| anyhow!("No es pot crear la carpeta de sortida: {}", e))?;
    Ok(dir)
}

fn save_png(bytes: &[u8], prefix: &str) -> Result<String> {
    let path = data_generated_dir()?
        .join(format!("{}-{}.png", prefix, chrono::Utc::now().timestamp_millis()));
    std::fs::write(&path, bytes).map_err(|e| anyhow!("No s'ha pogut desar la imatge: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}

/// Importa una imatge codificada en base64 (amb o sense capçalera «data: …
/// base64,») a la carpeta de dades i retorna'n la ruta absoluta. Ho fa
/// servir el xat quan l'usuari ENGANXA una imatge al portatextils: la IA
/// local amb visió (llava, gemma3…) la pot veure i la UI la mostra.
pub fn import_base64(data_b64: &str, name: Option<&str>) -> Result<String> {
    // Llevar el prefix «data:image/png;base64,» si ve com a data-URI.
    let payload = match data_b64.find("base64,") {
        Some(i) => &data_b64[i + "base64,".len()..],
        None => data_b64.trim(),
    };
    let cleaned: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&cleaned)
        .map_err(|_| anyhow!("La imatge enganxada no s'ha pogut descodificar"))?;
    if bytes.len() < 32 {
        return Err(anyhow!("La imatge enganxada és buida o massa xicoteta"));
    }
    // Extensió segons els magic bytes (no cal reencodificar).
    let ext = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "png"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "jpg"
    } else if bytes.get(0..4) == Some(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "webp"
    } else if bytes.starts_with(b"GIF8") {
        "gif"
    } else {
        // Format desconegut: si el crate `image` el llegeix, reconvertim a PNG.
        let img = image::load_from_memory(&bytes)
            .map_err(|_| anyhow!("Format d'imatge no reconegut"))?;
        let dyn_img = image::DynamicImage::ImageRgba8(img.to_rgba8());
        let path = data_generated_dir()?
            .join(format!("enganxat-{}.png", chrono::Utc::now().timestamp_millis()));
        dyn_img.save(&path).map_err(|e| anyhow!("No s'ha pogut desar: {}", e))?;
        return Ok(path.to_string_lossy().to_string());
    };
    let stamp = chrono::Utc::now().timestamp_millis();
    let path = data_generated_dir()?.join(format!("enganxat-{}.{}", stamp, ext));
    std::fs::write(&path, bytes).map_err(|e| anyhow!("No s'ha pogut desar la imatge: {}", e))?;
    let _ = name; // el nom original només és orientatiu; el fitxer ja té extensió
    Ok(path.to_string_lossy().to_string())
}

/// Carpeta de la zona de dades on van els fitxers adjuntats al xat (codi,
/// documents, dades…). Separada de «generated» perquè no es barregi amb
/// imatges creades per la IA.
fn data_adjunts_dir() -> Result<PathBuf> {
    let dir = AppConfig::data_dir()
        .ok_or_else(|| anyhow!("No es pot determinar la carpeta de dades de NoOrbit"))?
        .join("adjunts");
    std::fs::create_dir_all(&dir)
        .map_err(|e| anyhow!("No es pot crear la carpeta d'adjunts: {}", e))?;
    Ok(dir)
}

/// Desa QUALSEVOL fitxer adjuntat al xat (enganxat del porta-retalls o triat
/// al disc, que no arriba amb ruta real) a la carpeta de dades i en retorna
/// la ruta absoluta. El nom s'hanitza: no pot eixir de la carpeta.
pub fn attach_base64(data_b64: &str, name: &str) -> Result<String> {
    use base64::Engine as _;
    let payload = match data_b64.find("base64,") {
        Some(i) => &data_b64[i + "base64,".len()..],
        None => data_b64.trim(),
    };
    let cleaned: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&cleaned)
        .map_err(|_| anyhow!("El fitxer adjunt no s'ha pogut descodificar"))?;
    if bytes.is_empty() {
        return Err(anyhow!("El fitxer adjunt és buit"));
    }
    if bytes.len() > 40_000_000 {
        return Err(anyhow!("Fitxer adjunt massa gran (màx. 40 MB)"));
    }
    let raw = name.rsplit(['/', '\\']).next().unwrap_or("fitxer").trim();
    let mut safe: String = raw
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' '))
        .take(80)
        .collect();
    let trimmed = safe.trim_matches('.');
    if trimmed.is_empty() {
        safe = "fitxer".to_string();
    }
    let stamp = chrono::Utc::now().timestamp_millis();
    let path = data_adjunts_dir()?.join(format!("adjunt-{}-{}", stamp, safe));
    std::fs::write(&path, &bytes)
        .map_err(|e| anyhow!("No s'ha pogut desar l'adjunt: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}

// ── ComfyUI (local) ─────────────────────────────────────────────────────────

/// URL base si el servidor ComfyUI respon al port 8188.
fn comfy_base() -> Option<String> {
    if crate::ai::comfyui_running() {
        Some("http://127.0.0.1:8188".into())
    } else {
        None
    }
}

/// Primer checkpoint instal·lat a ComfyUI (per generar amb el que hi haja).
async fn first_checkpoint(http: &reqwest::Client, base: &str) -> Result<String> {
    let v: Value = http
        .get(format!("{}/object_info/CheckpointLoaderSimple", base))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| anyhow!("ComfyUI no respon — {}", e))?
        .json()
        .await
        .map_err(|e| anyhow!("Resposta inesperada de ComfyUI: {}", e))?;
    let req = &v["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"];
    // Format habitual: [["model_a.safetensors", "model_b…"]] o ["a","b"].
    let first = req
        .get(0)
        .and_then(|a| a.get(0))
        .or_else(|| req.get(0).filter(|s| s.is_string()))
        .and_then(|s| s.as_str())
        .ok_or_else(|| {
            anyhow!("ComfyUI no té cap model (checkpoint) instal·lat: baixa'n un des de la seva interfície")
        })?;
    Ok(first.to_string())
}

/// Envia un graf i espera la imatge resultant (cua de ComfyUI).
async fn submit_and_wait(http: &reqwest::Client, base: &str, graph: Value) -> Result<Vec<u8>> {
    let resp = http
        .post(format!("{}/prompt", base))
        .json(&json!({ "prompt": graph, "client_id": "noorbit" }))
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| anyhow!("ComfyUI no accepta la petició — {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("ComfyUI error {}: {}", status, text));
    }
    let v: Value = resp.json().await?;
    let prompt_id = v
        .get("prompt_id")
        .and_then(|p| p.as_str())
        .ok_or_else(|| anyhow!("ComfyUI no ha retornat prompt_id"))?
        .to_string();
    // Vigilància de la cua: /history/{id} fins que eixiga amb outputs.
    let started = std::time::Instant::now();
    loop {
        if started.elapsed() > Duration::from_secs(600) {
            return Err(anyhow!(
                "ComfyUI no ha acabat la generació en 10 minuts (cua saturada o model massa gran)"
            ));
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let h: Result<Value> = http
            .get(format!("{}/history/{}", base, prompt_id))
            .timeout(Duration::from_secs(20))
            .send()
            .await?
            .json()
            .await
            .map_err(anyhow::Error::from);
        let Ok(hist) = h else { continue };
        let outputs = hist
            .get(&prompt_id)
            .and_then(|e| e.get("outputs"))
            .and_then(|o| o.as_object());
        if let Some(map) = outputs {
            if let Some(img) = map
                .values()
                .find_map(|node| node.get("images").and_then(|i| i.get(0)))
            {
                let filename = img.get("filename").and_then(|f| f.as_str()).unwrap_or("");
                let subfolder = img.get("subfolder").and_then(|f| f.as_str()).unwrap_or("");
                let itype = img.get("type").and_then(|f| f.as_str()).unwrap_or("output");
                if !filename.is_empty() {
                    let bytes = http
                        .get(format!(
                            "{}/view?filename={}&subfolder={}&type={}",
                            base,
                            urlencoding::encode(filename),
                            urlencoding::encode(subfolder),
                            urlencoding::encode(itype)
                        ))
                        .timeout(Duration::from_secs(60))
                        .send()
                        .await?
                        .bytes()
                        .await?;
                    return Ok(bytes.to_vec());
                }
            }
        }
    }
}

/// Graf estàndard de text→imatge (checkpoint + CLIP + KSampler + SaveImage).
fn txt2img_graph(model: &str, o: &GenOpts, seed: u64) -> Value {
    json!({
        "1": { "class_type": "CheckpointLoaderSimple", "inputs": { "ckpt_name": model } },
        "2": { "class_type": "CLIPTextEncode", "inputs": { "text": o.prompt, "clip": ["1", 1] } },
        "3": { "class_type": "CLIPTextEncode", "inputs": { "text": o.negative, "clip": ["1", 1] } },
        "4": { "class_type": "EmptyLatentImage", "inputs": { "width": o.width, "height": o.height, "batch_size": 1 } },
        "5": { "class_type": "KSampler", "inputs": {
            "seed": seed, "steps": o.steps, "cfg": 7.0,
            "sampler_name": "euler", "scheduler": "normal", "denoise": 1.0,
            "model": ["1", 0], "positive": ["2", 0], "negative": ["3", 0], "latent_image": ["4", 0]
        } },
        "6": { "class_type": "VAEDecode", "inputs": { "samples": ["5", 0], "vae": ["1", 2] } },
        "7": { "class_type": "SaveImage", "inputs": { "filename_prefix": "noorbit", "images": ["6", 0] } }
    })
}

/// Graf img2img: carrega la imatge, la codifica i la regenera amb `denoise`.
fn img2img_graph(model: &str, uploaded: &str, o: &GenOpts, seed: u64, strength: f64) -> Value {
    json!({
        "1": { "class_type": "CheckpointLoaderSimple", "inputs": { "ckpt_name": model } },
        "2": { "class_type": "CLIPTextEncode", "inputs": { "text": o.prompt, "clip": ["1", 1] } },
        "3": { "class_type": "CLIPTextEncode", "inputs": { "text": o.negative, "clip": ["1", 1] } },
        "4": { "class_type": "LoadImage", "inputs": { "image": uploaded } },
        "8": { "class_type": "VAEEncode", "inputs": { "pixels": ["4", 0], "vae": ["1", 2] } },
        "5": { "class_type": "KSampler", "inputs": {
            "seed": seed, "steps": o.steps, "cfg": 7.0,
            "sampler_name": "euler", "scheduler": "normal", "denoise": strength,
            "model": ["1", 0], "positive": ["2", 0], "negative": ["3", 0], "latent_image": ["8", 0]
        } },
        "6": { "class_type": "VAEDecode", "inputs": { "samples": ["5", 0], "vae": ["1", 2] } },
        "7": { "class_type": "SaveImage", "inputs": { "filename_prefix": "noorbit-edit", "images": ["6", 0] } }
    })
}

/// Puja una imatge a ComfyUI i retorna el nom amb què la guarda.
async fn comfy_upload(
    http: &reqwest::Client,
    base: &str,
    bytes: &[u8],
    filename: &str,
) -> Result<String> {
    let part = reqwest::multipart::Part::bytes(bytes.to_vec())
        .file_name(filename.to_string())
        .mime_str("image/png")
        .map_err(|e| anyhow!("Capçalera d'imatge invàlida: {}", e))?;
    let form = reqwest::multipart::Form::new()
        .part("image", part)
        .text("overwrite", "true");
    let resp = http
        .post(format!("{}/upload/image", base))
        .multipart(form)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| anyhow!("ComfyUI no accepta la pujada — {}", e))?;
    let v: Value = resp.json().await.map_err(|e| anyhow!("Resposta inesperada: {}", e))?;
    v.get("name")
        .and_then(|n| n.as_str())
        .map(|n| n.to_string())
        .ok_or_else(|| anyhow!("ComfyUI no ha confirmat la pujada de la imatge"))
}

// ── Proveïdors en línia oficials (amb token de l'usuari) ────────────────────

/// Generació remota: `/images/generations` compatible amb OpenAI. Prova els
/// proveïdors activats amb token, en ordre, i retorna (bytes, etiqueta).
async fn generate_online(
    remote: &Arc<Mutex<ApiManager>>,
    o: &GenOpts,
) -> Result<(Vec<u8>, String, String)> {
    let ids: Vec<String> = {
        let guard = remote.lock().unwrap();
        guard
            .providers
            .iter()
            .filter(|p| p.enabled && !p.token.trim().is_empty() && !p.is_anthropic())
            .map(|p| p.id.clone())
            .collect()
    };
    if ids.is_empty() {
        return Err(anyhow!("Cap proveïdor en línia amb token registrat (menú \
                          «Proveïdors d'IA»: Venice, OpenAI… amb la teua clau)"));
    }
    let mut last_err = String::new();
    for id in ids {
        let prepared = remote.lock().unwrap().prepare(&id);
        let (provider, client) = match prepared {
            Ok(v) => v,
            Err(e) => {
                last_err = e.to_string();
                continue;
            }
        };
        // Model d'IMATGES: l'usuari l'ha d'indicar (p. ex. «dall-e-3»,
        // «venice-sd15»); el model de xat del proveïdor no serveix ací.
        let model = o
            .model
            .clone()
            .unwrap_or_else(|| "dall-e-3".to_string());
        // Venice permet contingut SENSE CENSURA si demanem explícitament el
        // que acceptem rebre. Sense «acceptable_content», Venice respon amb
        // una imatge filtrada/censurada encara que el prompt siga lliure.
        // Així doncs, quan el proveïdor és Venice, declarant que acceptem
        // contingut per a adults, l'API ens el retorna tal com es demana.
        let is_venice = provider.base().to_lowercase().contains("venice")
            || provider.name.to_lowercase().contains("venice");
        let mut body = json!({
            "model": model,
            "prompt": o.prompt,
            "n": 1,
            "size": format!("{}x{}", o.width.max(256).min(1024) - (o.width.max(256).min(1024) % 16),
                                     o.height.max(256).min(1024) - (o.height.max(256).min(1024) % 16)),
            "response_format": "b64_json",
        });
        if is_venice {
            // Venice no filtra: acceptem qualsevol contingut legítim (adults,
            // violència artística, etc.) i el privacitat és total.
            body["acceptable_content"] = json!(["sexual", "violence", "pii", "harassment", "hate"]);
            body["privacy_level"] = json!("private");
        }
        let resp = {
            let req = client
                .post(format!("{}/images/generations", provider.base()))
                .json(&body);
            provider.apply_auth(req).send().await
        };
        match resp {
            Ok(r) if r.status().is_success() => {
                let v: Value = r.json().await.unwrap_or_default();
                if let Some(b64) = v["data"][0]["b64_json"].as_str() {
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(b64)
                        .map_err(|e| anyhow!("Base64 invàlid: {}", e))?;
                    return Ok((bytes, format!("{} (online)", provider.name), model));
                }
                if let Some(url) = v["data"][0]["url"].as_str() {
                    let bytes = client
                        .get(url)
                        .timeout(Duration::from_secs(120))
                        .send()
                        .await?
                        .bytes()
                        .await?;
                    return Ok((bytes.to_vec(), format!("{} (online)", provider.name), model));
                }
                last_err = format!("{}: resposta sense imatge", provider.name);
            }
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                last_err = format!("{} error {}: {}", provider.name, status, text);
            }
            Err(e) => last_err = format!("{}: {}", provider.name, e),
        }
    }
    Err(anyhow!("Cap proveïdor en línia ha generat la imatge ({})", last_err))
}

/// Edició remota: `/images/edits` (multipart) compatible amb OpenAI.
async fn edit_online(
    http: &reqwest::Client,
    remote: &Arc<Mutex<ApiManager>>,
    image_bytes: &[u8],
    o: &GenOpts,
) -> Result<(Vec<u8>, String, String)> {
    let ids: Vec<String> = {
        let guard = remote.lock().unwrap();
        guard
            .providers
            .iter()
            .filter(|p| p.enabled && !p.token.trim().is_empty() && !p.is_anthropic())
            .map(|p| p.id.clone())
            .collect()
    };
    let mut last_err = String::new();
    for id in ids {
        let prepared = remote.lock().unwrap().prepare(&id);
        let (provider, client) = match prepared {
            Ok(v) => v,
            Err(e) => {
                last_err = e.to_string();
                continue;
            }
        };
        let model = o.model.clone().unwrap_or_else(|| "dall-e-2".to_string());
        let part = reqwest::multipart::Part::bytes(image_bytes.to_vec())
            .file_name("image.png")
            .mime_str("image/png")?;
        let form = reqwest::multipart::Form::new()
            .part("image", part)
            .text("prompt", o.prompt.clone())
            .text("model", model.clone())
            .text("n", "1")
            .text("response_format", "b64_json");
        let req = client
            .post(format!("{}/images/edits", provider.base()))
            .multipart(form);
        let resp = match provider.apply_auth(req).send().await {
            Ok(r) => r,
            Err(e) => {
                last_err = format!("{}: {}", provider.name, e);
                continue;
            }
        };
        if resp.status().is_success() {
            let v: Value = resp.json().await.unwrap_or_default();
            if let Some(b64) = v["data"][0]["b64_json"].as_str() {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .map_err(|e| anyhow!("Base64 invàlid: {}", e))?;
                return Ok((bytes, format!("{} (online)", provider.name), model));
            }
            last_err = format!("{}: resposta sense imatge", provider.name);
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            last_err = format!("{} error {}: {}", provider.name, status, text);
        }
    }
    let _ = http;
    Err(anyhow!("Cap proveïdor en línia ha editat la imatge ({})", last_err))
}

// ── Rodet de CPU (stable-diffusion.cpp) ─────────────────────────────────────
//
// Camí LOCAL alternatiu a ComfyUI: el binari `sd` de stable-diffusion.cpp
// treballa amb el processador (CPU), sense cap servei en marxa ni graf:
// un procés i un PNG. És més lent, però roda a qualsevol Mac — també en
// Mac Intel (x86_64), on ComfyUI oficial no està publicat — i a Linux i
// Windows. El model és un fitxer .gguf/.safetensors a «sd-models/».

/// Carpeta on NoOrbit deixa el binari `sd` quan el baixa ell mateix
/// (Linux i Windows: binaris oficials de GitHub).
pub fn sd_install_dir() -> Option<PathBuf> {
    AppConfig::data_dir().map(|d| d.join("sd"))
}

/// Binari `sd` (stable-diffusion.cpp) si està instal·lat: al PATH (Homebrew)
/// o a les dades de NoOrbit (quan NoOrbit el mateix el baixa).
pub fn sd_binary() -> Option<PathBuf> {
    if let Ok(p) = which::which("sd") {
        return Some(p);
    }
    let dir = sd_install_dir()?;
    let exe = if cfg!(windows) { "sd.exe" } else { "sd" };
    let p = dir.join(exe);
    if p.exists() {
        return Some(p);
    }
    // Pot ser un nivell més avall (l'arquiu traeu una carpeta).
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let p = e.path().join(exe);
        if p.exists() {
            return Some(p);
        }
        for s in std::fs::read_dir(e.path()).into_iter().flatten().flatten() {
            let p = s.path().join(exe);
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

/// Instal·la el binari `sd` (stable-diffusion.cpp) i en DONEIX el model
/// inicial: un sol botó i el rodet ja serveix.
///
/// - macOS: via Homebrew (al Mac Intel no hi ha binari oficial publicat:
///   el formulae el compila). S'executa en segon pla i se'n reporta el
///   progrés a l'usuari.
/// - Linux i Windows: binaris oficials de GitHub, directament a les dades
///   de NoOrbit (sense permisos d'administrador).
pub async fn sd_install(
    http: &reqwest::Client,
    app: Option<&tauri::AppHandle>,
) -> Result<String> {
    let _ = http;
    if let Some(b) = sd_binary() {
        return Ok(format!("El rodet de CPU ja és a {}", b.to_string_lossy()));
    }

    #[cfg(target_os = "macos")]
    {
        if which::which("brew").is_ok() {
            let mut cmd = std::process::Command::new("brew");
            cmd.args(["install", "stable-diffusion.cpp"]);
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
            let mut child = cmd
                .spawn()
                .map_err(|e| anyhow!("No s'ha pogut engegar Homebrew: {}", e))?;
            // La sortida es llegeix en un fil i es reporta com a progrés:
            // Homebrew compila el motor i pot trigar uns minuts.
            let stdout = child.stdout.take();
            let app2 = app.cloned();
            std::thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                let Some(h) = stdout else { return };
                let reader = BufReader::new(h);
                for line in reader.lines().map_while(Result::ok) {
                    let l = line.trim();
                    if l.is_empty() {
                        continue;
                    }
                    emit_sd_progress(app2.as_ref(), l);
                }
            });
            let status = tokio::task::spawn_blocking(move || child.wait())
                .await
                .map_err(|e| anyhow!("Error esperant Homebrew: {}", e))?
                .map_err(|e| anyhow!("Error esperant Homebrew: {}", e))?;
            if !status.success() {
                return Err(anyhow!(
                    "Homebrew no ha pogut instal·lar el rodet d'imatges en CPU \
                     (stable-diffusion.cpp). pots-ho provar a mà: «brew install \
                     stable-diffusion.cpp»."
                ));
            }
            return match sd_binary() {
                Some(b) => Ok(format!("Rodet d'imatges en CPU instal·lat a {}", b.to_string_lossy())),
                None => Err(anyhow!(
                    "Homebrew ha acabat, però el binari «sd» no s'ha trobat al PATH."
                )),
            };
        }
        // Sense Homebrew: s'obre la pàgina de descàrrega.
        let _ = std::process::Command::new("open")
            .arg("https://github.com/leejet/stable-diffusion.cpp/releases")
            .spawn();
        return Err(anyhow!(
            "No hi ha Homebrew en aquest Mac, i el rodet d'imatges en CPU no té \
             binari oficial per a Mac Intel. S'ha obert la pàgina de descàrrega: \
             tria l'arquiu «Darwin» (si el teu Mac és Apple Silicon) o compila'l \
             amb «brew install stable-diffusion.cpp»."
        ));
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Linux i Windows: binaris oficials de GitHub.
        let dir = sd_install_dir().ok_or_else(|| anyhow!("No puc resoldre la carpeta de dades"))?;
        std::fs::create_dir_all(&dir)?;
        let releases = http
            .get("https://api.github.com/repos/leejet/stable-diffusion.cpp/releases?per_page=8")
            .header("User-Agent", "NoOrbit")
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| anyhow!("No s'ha pogut contactar amb GitHub: {}", e))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| anyhow!("Resposta de GitHub ill·legible: {}", e))?;
        let want = if cfg!(windows) { "win-cpu-x64" } else { "Linux-Ubuntu" };
        let mut url = None;
        let mut nom = String::new();
        if let Some(arr) = releases.as_array() {
            for r in arr {
                let Some(assets) = r.get("assets").and_then(|a| a.as_array()) else {
                    continue;
                };
                for a in assets {
                    let n = a.get("name").and_then(|v| v.as_str()).unwrap_or_default();
                    if n.contains(want) && n.ends_with(".zip") {
                        url = a
                            .get("browser_download_url")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        nom = n.to_string();
                        break;
                    }
                }
                if url.is_some() {
                    break;
                }
            }
        }
        let url = url.ok_or_else(|| {
            anyhow!("No he trobat cap binari oficial del rodet d'imatges en CPU per a aquesta plataforma")
        })?;
        emit_sd_progress(app, &format!("descarregant {}…", nom));
        let resp = http
            .get(&url)
            .header("User-Agent", "NoOrbit")
            .timeout(std::time::Duration::from_secs(600))
            .send()
            .await
            .map_err(|e| anyhow!("No s'ha pogut baixar {}: {}", nom, e))?;
        let arxiu = dir.join(&nom);
        let mut out = std::fs::File::create(&arxiu)?;
        use futures::StreamExt;
        use std::io::Write;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            out.write_all(&chunk)?;
        }
        let _ = out.flush();
        drop(out);
        // Descomprimeix.
        let ok = std::process::Command::new("unzip")
            .arg("-o")
            .arg(&arxiu)
            .arg("-d")
            .arg(&dir)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let _ = std::fs::remove_file(&arxiu);
        if !ok {
            return Err(format!("No s'ha pogut descomprimir {}", nom));
        }
        // Marca el binari com a executable.
        if let Some(b) = sd_binary() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&b, std::fs::Permissions::from_mode(0o755));
            }
            return Ok(format!("Rodet d'imatges en CPU instal·lat a {}", b.to_string_lossy()));
        }
        Err("He descomprimit el binari, però no hi trobo l'executable «sd».".into())
    }
}

/// Avís de progrés del rodet de CPU (el mateix canal que la baixada del
/// model: «ai://sd-model»).
pub fn emit_sd_progress(app: Option<&tauri::AppHandle>, message: &str) {
    use tauri::Emitter;
    if let Some(app) = app {
        let _ = app.emit(
            "ai://sd-model",
            serde_json::json!({ "message": message }),
        );
    }
}

/// Carpeta on viuen els models del rodet de CPU («sd-models/» a les dades).
pub fn sd_models_dir() -> Result<PathBuf> {
    let dir = AppConfig::data_dir()
        .ok_or_else(|| anyhow!("No es pot determinar la carpeta de dades de NoOrbit"))?
        .join("sd-models");
    std::fs::create_dir_all(&dir)
        .map_err(|e| anyhow!("No es pot crear la carpeta sd-models: {}", e))?;
    Ok(dir)
}

/// Primer model .gguf/.safetensors complet (no «.part») del rodet de CPU.
pub fn sd_model_path() -> Option<PathBuf> {
    let dir = sd_models_dir().ok()?;
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("")
                .to_lowercase();
            name.ends_with(".gguf") || name.ends_with(".safetensors")
        })
        .collect();
    candidates.sort();
    candidates.into_iter().find(|p| {
        !p.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("part"))
            .unwrap_or(false)
    })
}

/// Estat del rodet de CPU per al Gestor de models i el manual del model.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdInfo {
    /// Binari `sd` present.
    pub installed: bool,
    /// Carpeta de models en format llegible.
    pub models_dir: String,
    /// Nom del model disponible (o None si encara no n'hi ha cap).
    pub model: Option<String>,
}

pub fn sd_info() -> Result<SdInfo, String> {
    Ok(SdInfo {
        installed: sd_binary().is_some(),
        models_dir: sd_models_dir()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .to_string(),
        model: sd_model_path()
            .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string()),
    })
}

/// Línia d'estat curt per al manual del model: Some(«sd (CPU) amb el model
/// X») si Hi ha binari i model; Some amb missatge sense model si només hi ha
/// binari; None si no hi ha rodet.
pub fn sd_status_text() -> Option<String> {
    if sd_binary().is_none() {
        return None;
    }
    Some(match sd_model_path() {
        Some(p) => format!(
            "model {}",
            p.file_name().unwrap_or_default().to_string_lossy()
        ),
        None => "instal·lat però sense cap model a la carpeta «sd-models»".to_string(),
    })
}

/// Genera una imatge amb el binari `sd` en CPU mateix fil de treball que
/// ComfyUI però sense servidor: una comanda, un PNG.
async fn sd_txt2img(o: &GenOpts, seed: u64) -> Result<(Vec<u8>, String)> {
    let bin = sd_binary().ok_or_else(|| anyhow!("El rodet de CPU no està instal·lat"))?;
    let model = sd_model_path().ok_or_else(|| {
        anyhow!(
            "El rodet de CPU no té cap model: baixa'n un al Gestor de models \
             (botó «Baixa el model inicial» de la secció «Rodet d'imatges en CPU»)"
        )
    })?;
    let stamp = chrono::Utc::now().timestamp_millis();
    let out = sd_models_dir()?.join(format!("sd-{}-{}.png", seed, stamp));
    let steps = o.steps.clamp(1, 60);
    let mut cmd = tokio::process::Command::new(&bin);
    cmd.arg("-m")
        .arg(&model)
        .arg("-p")
        .arg(&o.prompt)
        .arg("-o")
        .arg(&out)
        .arg("--steps")
        .arg(steps.to_string())
        .arg("-W")
        .arg(o.width.to_string())
        .arg("-H")
        .arg(o.height.to_string())
        .arg("--seed")
        .arg(seed.to_string())
        .arg("--threads")
        .arg("4");
    if !o.negative.trim().is_empty() {
        cmd.arg("-n").arg(&o.negative);
    }
    let outp = cmd
        .output()
        .await
        .map_err(|e| anyhow!("No s'ha pogut executar «sd»: {}", e))?;
    if outp.status.success() {
        let bytes = std::fs::read(&out)
            .map_err(|e| anyhow!("«sd» no ha deixat PNG llegible: {}", e))?;
        let _ = std::fs::remove_file(&out);
        return Ok((
            bytes,
            model
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        ));
    }
    // En error, ens quedem amb l'ÚLTIMA línia no buida de stderr: és on el
    // binari explica la causa real (model corrupt, OOM, format no suportat…).
    let tail = String::from_utf8_lossy(&outp.stderr)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .last()
        .unwrap_or_default()
        .to_string();
    if tail.is_empty() {
        return Err(anyhow!("«sd» ha fallat (exit {:?}) sense detall", outp.status.code()));
    }
    Err(anyhow!("«sd» ha fallat (exit {:?}): {}", outp.status.code(), tail))
}

/// Edita una imatge amb el rodet de CPU: image→image amb el binari `sd`
/// amb la imatge original com a «-i» i el «-strength» de la força.
async fn sd_img2img(
    init_bytes: &[u8],
    init_name: &str,
    o: &GenOpts,
    seed: u64,
    strength: f64,
) -> Result<(Vec<u8>, String)> {
    let bin = sd_binary().ok_or_else(|| anyhow!("El rodet de CPU no està instal·lat"))?;
    let model = sd_model_path().ok_or_else(|| {
        anyhow!(
            "El rodet de CPU no té cap model: baixa'n un al Gestor de models \
             (secció «Rodet d'imatges en CPU»)"
        )
    })?;
    let dir = sd_models_dir()?;
    let stamp = chrono::Utc::now().timestamp_millis();
    // Nom sanejat del fitxer origen (mai no deixem caràcters rars a l'shell
    // del binari; un path simple és suficient).
    let safe_init: String = init_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
        .collect();
    let init_path = dir.join(format!("sd-in-{}-{}", stamp, safe_init));
    std::fs::write(&init_path, init_bytes)
        .map_err(|e| anyhow!("No s'ha pogut preparar la imatge origen: {}", e))?;
    let out = dir.join(format!("sd-edit-{}-{}.png", seed, stamp));
    let steps = o.steps.clamp(1, 60);
    let mut cmd = tokio::process::Command::new(&bin);
    cmd.arg("-m")
        .arg(&model)
        .arg("-i")
        .arg(&init_path)
        .arg("--strength")
        .arg(strength.clamp(0.05, 0.95).to_string())
        .arg("-p")
        .arg(&o.prompt)
        .arg("-o")
        .arg(&out)
        .arg("--steps")
        .arg(steps.to_string())
        .arg("-W")
        .arg(o.width.to_string())
        .arg("-H")
        .arg(o.height.to_string())
        .arg("--seed")
        .arg(seed.to_string())
        .arg("--threads")
        .arg("4");
    if !o.negative.trim().is_empty() {
        cmd.arg("-n").arg(&o.negative);
    }
    let outp = cmd
        .output()
        .await
        .map_err(|e| anyhow!("No s'ha pogut executar «sd»: {}", e))?;
    let _ = std::fs::remove_file(&init_path);
    if let Ok(bytes) = std::fs::read(&out) {
        if bytes.len() > 100 {
            let _ = std::fs::remove_file(&out);
            return Ok((
                bytes,
                model
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
            ));
        }
    }
    let tail = String::from_utf8_lossy(&outp.stderr)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .last()
        .unwrap_or_default()
        .to_string();
    if tail.is_empty() {
        return Err(anyhow!("«sd» ha fallat (exit {:?}) sense detall", outp.status.code()));
    }
    Err(anyhow!("«sd» ha fallat (exit {:?}): {}", outp.status.code(), tail))
}

// ── API pública ─────────────────────────────────────────────────────────────
/// ComfyUI local si hi ha servidor + RAM, si no, proveïdors amb token.
pub async fn generate(
    remote: Option<Arc<Mutex<ApiManager>>>,
    o: &GenOpts,
) -> Result<GeneratedImage> {
    if o.prompt.trim().is_empty() {
        return Err(anyhow!("Cal descriure la imatge que vols generar"));
    }
    let http = http_client();
    let seed = if o.seed == 0 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(42)
    } else {
        o.seed
    };

    // 1) Local (ComfyUI), només si la màquina aguanta la resolució.
    let mut note: String;
    if let Some(base) = comfy_base() {
        if enough_ram(o.width) {
            match local_txt2img(&http, &base, o, seed).await {
                Ok((bytes, model)) => {
                    let path = save_png(&bytes, "imatge")?;
                    return Ok(GeneratedImage {
                        path,
                        backend: "comfyui (local)".into(),
                        model,
                    });
                }
                Err(e) => note = e.to_string(),
            }
        } else {
            note = format!(
                "RAM lliure insuficient per generar {}x{} en local; s'intenta en línia",
                o.width, o.height
            );
        }
    } else {
        note = if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
            // ComfyUI oficial només es publica per a Apple Silicon: en un Mac 
            // Intel el camí local no existeix, no té sentit recomanar-lo.
            "ComfyUI local no és compatible amb Mac Intel (només Apple Silicon)".into()
        } else {
            "ComfyUI no està en marxa".into()
        };
    }

    // 1b) Rodet de CPU (stable-diffusion.cpp): l'única via LOCAL en Mac Intel,
    // però útil a QUALSEVOL ordinador quan ComfyUI no està en marxa.
    if sd_binary().is_some() {
        match sd_txt2img(o, seed).await {
            Ok((bytes, model)) => {
                let path = save_png(&bytes, "imatge")?;
                return Ok(GeneratedImage {
                    path,
                    backend: "sd (CPU)".into(),
                    model,
                });
            }
            Err(e) => note = format!("{} / rodet de CPU: {}", note, e),
        }
    }

    // 2) En línia amb les credencials explícites de l'usuari.
    if let Some(remote) = remote {
        match generate_online(&remote, o).await {
            Ok((bytes, backend, model)) => {
                let path = save_png(&bytes, "imatge")?;
                return Ok(GeneratedImage { path, backend, model });
            }
            Err(e) => note = format!("{} / online: {}", note, e),
        }
    } else {
        note = format!("{} / sense proveïdors en línia registrats", note);
    }
    let local_hint = if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "En aquest Mac Intel, ComfyUI local NO és possible, però hi ha un rodet \
         d'imatges en CPU que SÍ funciona: instal·la'l amb el botó «Instal·la» \
         de la secció «Rodet d'imatges en CPU» del Gestor de models i baixa el \
         model inicial («Baixa el model inicial», ≈2 GB). Aleshores «IMG|» \
         funciona en local acabat d'engegar el xat. També pots registrar un \
         proveïdor en línia amb token al menú «Proveïdors d'IA» (p. ex. OpenAI \
         amb «dall-e-3», o Venice)."
    } else {
        "Arrenca ComfyUI amb «start_comfyui» (o instal·la'l amb \
         «install_comfyui») per fer-ho en local, o registra un proveïdor en \
         línia amb token al menú de Proveïdors d'IA (p. ex. OpenAI amb \
         «dall-e-3») indicant el model d'imatges."
    };
    Err(anyhow!("No puc generar la imatge ({}). {}", note, local_hint))
}

async fn local_txt2img(
    http: &reqwest::Client,
    base: &str,
    o: &GenOpts,
    seed: u64,
) -> Result<(Vec<u8>, String)> {
    let model = match &o.model {
        Some(m) => m.clone(),
        None => first_checkpoint(http, base).await?,
    };
    let graph = txt2img_graph(&model, o, seed);
    let bytes = submit_and_wait(http, base, graph).await?;
    Ok((bytes, model))
}

/// Edita una imatge existent (image→image) amb una instrucció de text.
/// `strength` (0..1) = com de fort es reinterpreta la imatge original.
pub async fn edit(
    remote: Option<Arc<Mutex<ApiManager>>>,
    path: &Path,
    o: &GenOpts,
    strength: f64,
) -> Result<GeneratedImage> {
    let bytes = std::fs::read(path).map_err(|e| anyhow!("No puc llegir la imatge: {}", e))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "source.png".into());
    let http = http_client();
    let seed = if o.seed == 0 { 7 } else { o.seed };

    // 1) Local: img2img a ComfyUI.
    let mut note: String;
    if let Some(base) = comfy_base() {
        if enough_ram(o.width) {
            let local = async {
                let model = match &o.model {
                    Some(m) => m.clone(),
                    None => first_checkpoint(&http, &base).await?,
                };
                let uploaded = comfy_upload(&http, &base, &bytes, &name).await?;
                let graph = img2img_graph(&model, &uploaded, o, seed, strength.clamp(0.1, 1.0));
                let out = submit_and_wait(&http, &base, graph).await?;
                Result::<(Vec<u8>, String)>::Ok((out, model))
            }
            .await;
            match local {
                Ok((out, model)) => {
                    let path = save_png(&out, "edicio")?;
                    return Ok(GeneratedImage {
                        path,
                        backend: "comfyui (local)".into(),
                        model,
                    });
                }
                Err(e) => note = e.to_string(),
            }
        } else {
            note = "RAM insuficient per editar en local".into();
        }
    } else {
        note = "ComfyUI no està en marxa".into();
    }

    // 1b) Rodet de CPU: l'única via LOCAL en Mac Intel (i alternativa a
    // ComfyUI a qualsevol màquina sense servidor encès).
    if sd_binary().is_some() {
        match sd_img2img(&bytes, &name, o, seed, strength).await {
            Ok((out, model)) => {
                let path = save_png(&out, "edicio")?;
                return Ok(GeneratedImage {
                    path,
                    backend: "sd (CPU)".into(),
                    model,
                });
            }
            Err(e) => note = format!("{} / rodet de CPU: {}", note, e),
        }
    }

    // 2) En línia (OpenAI-compatible /images/edits).
    if let Some(remote) = remote {
        match edit_online(&http, &remote, &bytes, o).await {
            Ok((out, backend, model)) => {
                let path = save_png(&out, "edicio")?;
                return Ok(GeneratedImage { path, backend, model });
            }
            Err(e) => note = format!("{} / online: {}", note, e),
        }
    }
    Err(anyhow!(
        "No puc editar la imatge ({}). Cal ComfyUI en marxa o un proveïdor \
         en línia amb token i model d'imatges compatible amb /images/edits.",
        note
    ))
}

/// Operacions deterministes de píxel (sense IA): redimensionar, retallar,
/// girar, voltejar i pasar a gris. Ràpides i fiables per a qualsevol màquina.
pub fn transform(
    path: &Path,
    op: &str,
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    angle: u32,
) -> Result<GeneratedImage> {
    let img = image::open(path).map_err(|e| anyhow!("No puc obrir la imatge: {}", e))?;
    let out = match op {
        "resize" => {
            if width == 0 || height == 0 {
                return Err(anyhow!("resize necessita width i height"));
            }
            img.resize_exact(width, height, image::imageops::FilterType::Lanczos3)
        }
        "crop" => {
            if width == 0 || height == 0 {
                return Err(anyhow!("crop necessita width i height (x,y opcionals)"));
            }
            img.crop_imm(x, y, width, height)
        }
        "rotate" => match angle % 360 {
            0 => img,
            90 => img.rotate90(),
            180 => img.rotate180(),
            270 => img.rotate270(),
            a => return Err(anyhow!("rotate només accepta 90/180/270 graus (rebut {})", a)),
        },
        "flip_h" => img.fliph(),
        "flip_v" => img.flipv(),
        "grayscale" => img.grayscale(),
        other => return Err(anyhow!("Operació desconeguda: {} (resize/crop/rotate/flip_h/flip_v/grayscale)", other)),
    };
    let dest = data_generated_dir()?.join(format!(
        "transform-{}-{}.png",
        op,
        chrono::Utc::now().timestamp_millis()
    ));
    out.save(&dest).map_err(|e| anyhow!("No s'ha pogut desar: {}", e))?;
    Ok(GeneratedImage {
        path: dest.to_string_lossy().to_string(),
        backend: format!("image ({} pixel)", op),
        model: "noorbit-crate-image".into(),
    })
}
