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
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let sc = call_tool(
        http,
        "search_artifacts",
        serde_json::json!({
            "q": q,
            "format": "gguf",
            "limit": limit.clamp(1, 40),
            "detail": "full"
        }),
    )
    .await?;
    Ok(parse_rows(&sc))
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
        });
    }
    out
}

/// Detalls d'un model: fitxer recomanat, quantitzacions i enllaç signat.
pub async fn details(http: &reqwest::Client, id: &str) -> Result<BayDetails> {
    let sc = call_tool(http, "get_model", serde_json::json!({ "id": id, "detail": "full" })).await?;

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
    })
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
pub fn models_dir() -> std::path::PathBuf {
    crate::config::AppConfig::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("models")
}

/// Descarrega un fitxer del mirror amb PROGRÉS (esdeveniment «ai://pull»,
/// el mateix que usa la descàrrega d'Ollama) i el desa a la carpeta de models.
///
/// L'enllaç signat caduca a 5 minuts: si arriba exhaurit, es torna a demanar
/// el model (amb un enllaç nou) i es reintenta una vegada.
pub async fn download_file(
    http: &reqwest::Client,
    app: Option<&tauri::AppHandle>,
    id: &str,
    url: &str,
    dest_name: &str,
) -> Result<std::path::PathBuf> {
    use tauri::Emitter;
    let dir = models_dir();
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(dest_name);

    // Si ja hi és (desada anterior), no tornem a baixar-la.
    if let Ok(meta) = std::fs::metadata(&dest) {
        if meta.len() > 0 {
            return Ok(dest);
        }
    }

    let mut attempts = 0;
    let mut last_err = String::new();
    while attempts < 2 {
        attempts += 1;
        let resp = http
            .get(url)
            .header("User-Agent", "NoOrbit")
            .timeout(std::time::Duration::from_secs(60 * 30))
            .send()
            .await
            .map_err(|e| anyhow!("No s'ha pogut descarregar: {}", e))?;
        if !resp.status().is_success() {
            last_err = format!("el mirror ha respost {}", resp.status());
            // Enllaç caducat: en demanem un de nou.
            let nou = match details(http, id).await {
                Ok(d) => d.recommended.and_then(|f| f.download_url),
                Err(e) => {
                    last_err = format!("{} (i no s'ha pogut demanar un enllaç nou: {})", last_err, e);
                    None
                }
            };
            match nou {
                Some(u) => {
                    let _ = u;
                    continue;
                }
                None => break,
            }
        }
        let total = resp.content_length().unwrap_or(0);
        let mut out = std::fs::File::create(&dest)?;
        let mut stream = resp.bytes_stream();
        use futures::StreamExt;
        let mut written: u64 = 0;
        let mut last_pct: i64 = -1;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            use std::io::Write;
            out.write_all(&chunk)?;
            written += chunk.len() as u64;
            if total > 0 {
                let pct = (written * 100 / total) as i64;
                if pct != last_pct {
                    last_pct = pct;
                    if let Some(app) = app {
                        let _ = app.emit(
                            "ai://pull",
                            serde_json::json!({
                                "model": dest_name,
                                "message": format!("descarregant {} — {}%", dest_name, pct)
                            }),
                        );
                    }
                }
            }
        }
        let _ = std::io::Write::flush(&mut out);
        if total > 0 && written != total {
            last_err = format!("descàrrega incompleta ({} de {} bytes)", written, total);
            continue;
        }
        return Ok(dest);
    }
    Err(anyhow!(
        "No s'ha pogut baixar el model de Hugging Bay: {}",
        if last_err.is_empty() { "error desconegut" } else { &last_err }
    ))
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
}
