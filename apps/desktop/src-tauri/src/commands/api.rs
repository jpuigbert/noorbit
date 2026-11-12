//! Comandes Tauri per a proveïdors d'IA en línia (tokens).

use crate::api::{self, AiProvider, AiProviderPublic, AuthScheme};
use crate::AppState;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{command, State};

/// Peticions d'AUTOCOMPLETAT EN VOL: permet cancel·lar-les des de la
/// interfície. Sense això, quan l'usuari seguia escrivint (o es cancel·lava el
/// suggeriment), la inferència vella continuava al servidor consumint CPU/GPU
/// i RAM fins que expirava el temps límit.
static INLINE_PENDING: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

fn inline_pending() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    INLINE_PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Espera que es cancel·li la petició (es comprova cada 80 ms).
async fn wait_cancelled(flag: Arc<AtomicBool>) {
    loop {
        if flag.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewProvider {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    /// Format d'API: "openai" (per defecte) o "anthropic" (Claude).
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub auth: Option<AuthScheme>,
    #[serde(default)]
    pub header_name: Option<String>,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPatch {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub auth: Option<AuthScheme>,
    #[serde(default)]
    pub header_name: Option<String>,
    /// Si és None, conserva el token existent.
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[command]
pub fn ai_provider_list(state: State<'_, AppState>) -> Result<Vec<AiProviderPublic>, String> {
    Ok(state.api_manager.lock().unwrap().list())
}

#[command]
pub fn ai_provider_add(
    state: State<'_, AppState>,
    provider: NewProvider,
) -> Result<AiProviderPublic, String> {
    let item = AiProvider {
        id: provider.id,
        name: provider.name,
        base_url: provider.base_url,
        model: provider.model,
        kind: provider.kind,
        auth: provider.auth.unwrap_or(AuthScheme::Bearer),
        header_name: provider.header_name.unwrap_or_default(),
        token: provider.token,
        enabled: provider.enabled.unwrap_or(true),
    };
    state
        .api_manager
        .lock()
        .unwrap()
        .add(item)
        .map_err(|e| e.to_string())
}

#[command]
pub fn ai_provider_update(
    state: State<'_, AppState>,
    patch: ProviderPatch,
) -> Result<AiProviderPublic, String> {
    state
        .api_manager
        .lock()
        .unwrap()
        .update(
            &patch.id,
            patch.name,
            patch.base_url,
            patch.model,
            patch.kind,
            patch.auth,
            patch.header_name,
            patch.token,
            patch.enabled,
        )
        .map_err(|e| e.to_string())
}

#[command]
pub fn ai_provider_remove(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .api_manager
        .lock()
        .unwrap()
        .remove(&id)
        .map_err(|e| e.to_string())
}

/// Prova un proveïdor enviant-hi una petició de xat senzilla.
#[command]
pub async fn ai_provider_test(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let mgr: Arc<Mutex<api::ApiManager>> = state.api_manager.clone();
    let (provider, client) = {
        let guard = mgr.lock().unwrap();
        guard.prepare(&id).map_err(|e| e.to_string())?
    };
    api::chat(
        &client,
        &provider,
        &[("user".to_string(), "Respon només amb la paraula: OK".to_string())],
        Some("Ets una prova de connexió. Respon molt breu."),
    )
    .await
    .map(|(content, _)| content)
    .map_err(|e| e.to_string())
}

/// Resultat d'una verificació prèvia d'un servei d'IA (DeepInfer, LM Studio,
/// vLLM, OpenAI…): no sona la paraula OK al model, sinó que fa un diagnòstic
/// pas a pas de la connexió. L'objectiu és que si FALLA, la UI el marqui
/// amb ✓/✗ i digui EL MOTIU exacte — i què es pot fer per arreglar-ho.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointCheck {
    pub ok: bool,
    /// Fase on s'ha aturat: "connexio" | "auth" | "url" | "model" | "llest".
    pub stage: String,
    pub title: String,
    pub detail: String,
    /// Models que el servidor ha ofert (si n'hem pogut obtenir la llista).
    pub models: Vec<String>,
}

#[command]
pub async fn ai_endpoint_check(
    base_url: String,
    model: String,
    token: Option<String>,
    kind: Option<String>,
) -> Result<EndpointCheck, String> {
    endpoint_check_inner(base_url, model, token, kind).await
}

/// Cos real del diagnòstic (compartit amb `ai_remote_verify`).
pub async fn endpoint_check_inner(
    base_url: String,
    model: String,
    token: Option<String>,
    kind: Option<String>,
) -> Result<EndpointCheck, String> {
    use std::time::Duration;

    let base = crate::api::normalize_base(&base_url);
    let is_anthropic = kind.as_deref() == Some("anthropic");
    // /models (OpenAI-compatible) o /v1/models (Anthropic).
    let url = if is_anthropic {
        if base.to_lowercase().ends_with("/v1") {
            format!("{}/models", base)
        } else {
            format!("{}/v1/models", base)
        }
    } else {
        format!("{}/models", base)
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();
    let mut req = client.get(&url);
    let tok = token.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    if let Some(t) = &tok {
        if is_anthropic {
            req = req
                .header("x-api-key", t.as_str())
                .header("anthropic-version", "2023-06-01");
        } else {
            req = req.header("Authorization", format!("Bearer {}", t));
        }
    }

    // Pista per plataforma (DeepInfer oficial només corre a Linux).
    let is_local = base.contains("127.0.0.1")
        || base.contains("localhost")
        || base.contains("[::1]");
    #[allow(clippy::vec_init_then_push)]
    let platform_note: String = if is_local && !cfg!(target_os = "linux") {
        if cfg!(target_os = "macos") {
            "\n⚠️ En macOS: la versió oficial de DeepInfer només corre a Linux (GPU NVIDIA SM100 amb CUDA 13 o NPU Ascend 950). Aquest servidor ha de viure en una altra màquina: escriu la URL amb la seva IP (p. ex. http://192.168.1.20:8000/v1).".to_string()
        } else if cfg!(target_os = "windows") {
            "\n⚠️ En Windows: la versió oficial de DeepInfer només corre a Linux (GPU NVIDIA SM100 amb CUDA 13 o NPU Ascend 950). Aquest servidor ha de viure en una altra màquina: escriu la URL amb la seva IP (p. ex. http://192.168.1.20:8000/v1).".to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let mut detail = if e.is_timeout() {
                format!("El servidor ha trigat massa a respondre ({}).\nPot ser que estigui carregant el model o col·locat darrere d'una xarxa lenta; torna-ho a provar en uns segons.", url)
            } else {
                format!("Cap servidor contesta a {}.\nPossibles motius: 1) el servei no està en marxa (engega'l i torna-ho a provar); 2) la URL/port són erronis; 3) tallafocs o antivírus que bloqueja el port; 4) el servidor i NoOrbit no estan a la mateixa xarxa.", url)
            };
            if !platform_note.is_empty() {
                detail.push_str(&platform_note);
            }
            return Ok(EndpointCheck {
                ok: false,
                stage: "connexio".into(),
                title: "El servidor no respon".into(),
                detail,
                models: Vec::new(),
            });
        }
    };

    let status = resp.status();
    // El servidor d'IA remot de NoOrbit (wrapper Python) respon 503 quan
    // és en marxa però el motor encara està carregant el model: aquest cas
    // mereix el seu propi missatge, no un «404/500» genèric.
    if status.as_u16() == 503 {
        let body = resp.text().await.unwrap_or_default();
        return Ok(EndpointCheck {
            ok: false,
            stage: "model".into(),
            title: "El servidor és en marxa, però el model encara no pot respondre".into(),
            detail: format!(
                "La connexió amb {} funciona (el servei hi és), però el motor d'inferència encara està carregant el model — pot trigar minuts en GPU grans.\nTorna a prémer «Comprova» d'aquí a uns minuts.\nResposta del servidor: {}",
                url,
                if body.is_empty() { "—".to_string() } else { body.chars().take(400).collect() }
            ),
            models: Vec::new(),
        });
    }
    if status.as_u16() == 401 || status.as_u16() == 403 {
        let body = resp.text().await.unwrap_or_default();
        let why = if tok.is_none() {
            "el servidor demana autenticació i no n'has enganxat cap clau".to_string()
        } else {
            "la clau no és vàlida o ha expirat (torna-la a generar al servei)".to_string()
        };
        return Ok(EndpointCheck {
            ok: false,
            stage: "auth".into(),
            title: format!("Autenticació rebutjada ({})", status),
            detail: format!(
                "{}. Motiu probable: {}{}\nResposta del servidor: {}",
                url,
                why,
                if platform_note.is_empty() { String::new() } else { format!("{}", platform_note) },
                if body.is_empty() { "—".to_string() } else { body.chars().take(300).collect() }
            ),
            models: Vec::new(),
        });
    }
    if status.as_u16() == 404 {
        let mut hint = if is_anthropic {
            format!("La URL {} no té l'endpoint de models d'Anthropic: la base ha de ser https://api.anthropic.com (NoOrbit hi afegeix /v1/models).", url)
        } else {
            format!("La URL {} no existeix (404). Motius habituals: falta el sufix «/v1» a la base (hauria de ser …/v1 i demanar …/v1/models), el port és erroni, o el servei és una versió sense l'API HTTP habilitada.", url)
        };
        if !platform_note.is_empty() {
            hint.push_str(&platform_note);
        }
        return Ok(EndpointCheck {
            ok: false,
            stage: "url".into(),
            title: format!("URL no trobada (404)"),
            detail: hint,
            models: Vec::new(),
        });
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Ok(EndpointCheck {
            ok: false,
            stage: "url".into(),
            title: format!("El servidor ha respost {} ", status),
            detail: format!(
                "{}\nResposta del servidor: {}{}",
                url,
                if body.is_empty() { "—".to_string() } else { body.chars().take(300).collect() },
                if platform_note.is_empty() { String::new() } else { format!("{}{}", "\n", platform_note) }
            ),
            models: Vec::new(),
        });
    }

    // 200 OK: parseem la llista de models que ofereix el servei.
    let models: Vec<String> = resp
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| {
            v.get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| {
                            m.get("id")
                                .and_then(|i| i.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect::<Vec<_>>()
                })
                .or_else(|| v.get("models").and_then(|d| d.as_array()).map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.as_str().map(|s| s.to_string()))
                        .collect()
                }))
        })
        .unwrap_or_default();

    let wanted = model.trim();
    if wanted.is_empty() {
        return Ok(EndpointCheck {
            ok: true,
            stage: "model".into(),
            title: "El servidor respon, però no hi has triat cap model".into(),
            detail: format!(
                "Tria'n un de la llista que t'ha ofert ({} trobats).",
                models.len()
            ),
            models,
        });
    }
    if !models.is_empty() && !models.iter().any(|m| m == wanted) {
        let listed: Vec<&str> = models.iter().map(|s| s.as_str()).take(12).collect();
        return Ok(EndpointCheck {
            ok: false,
            stage: "model".into(),
            title: format!("El model «{}» no és al servidor", wanted),
            detail: format!(
                "El servidor està en marxa i contesta bé, però no té aquest model. Aquests són els que ofereix (clica'n un per triar-lo): {}{}",
                listed.join(", "),
                if models.len() > listed.len() { format!(" … i {} més.", models.len() - listed.len()) } else { String::new() }
            ),
            models,
        });
    }

    Ok(EndpointCheck {
        ok: true,
        stage: "llest".into(),
        title: "Tot a punt".to_string(),
        detail: format!("El servidor contesta bé a {} i el model «{}» hi és.", url, wanted),
        models,
    })
}

// ── Servidor d'IA remot (pujada per FTP/SFTP + API HTTP al servidor) ────────
//
// L'usuari pot tenir la GPU en un servidor extern (p. ex. un Linux amb
// NVIDIA SM100 per a DeepInfer). Aquesta comanda:
//   1) genera dos fitxers (`noorbit-ai-server.sh` + `noorbit_server_api.py`),
//   2) els puja per SFTP/FTPS/FTP amb `curl` (deps zero: el curl de l'OS),
//   3) imprimeix els passos exactes que queden per executar via «ssh»,
// i després es pot verificar amb `ai_remote_verify` (reusa el diagnòstic).
// El servidor exposa una API **compatible amb OpenAI**: NoOrbit s'hi connecta
// amb un proveïdor normal, així que les respostes tornen al local per HTTP i
// el Mac no gasta ni un mega de RAM en inferència.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDeploySpec {
    pub host: String,
    /// "sftp" | "ftps" | "ftp" (ftp = contrasenya en clar: s'avisa a la UI).
    #[serde(default = "rd_default_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub port: Option<u16>,
    pub user: String,
    #[serde(default)]
    pub password: String,
    /// Carpeta remota on pujar els fitxers (es crea si falta).
    #[serde(default = "rd_default_dir")]
    pub remote_dir: String,
    /// Port HTTP on escoltarà la API dins del servidor.
    #[serde(default = "rd_default_api_port")]
    pub api_port: u16,
    /// "deepinfer" | "vllm"
    #[serde(default = "rd_default_backend")]
    pub backend: String,
    /// Model que carregarà el servidor (buit = el de rigor del backend).
    #[serde(default)]
    pub model: Option<String>,
}

fn rd_default_protocol() -> String { "sftp".to_string() }
fn rd_default_dir() -> String { format!("/opt/noorbit-ia") }
fn rd_default_api_port() -> u16 { 8000 }
fn rd_default_backend() -> String { "deepinfer".to_string() }

/// Escapa un fragment per incrustar-lo en una URL de curl (user, clau):
/// tot allò que no sigui alfanumèric segur va amb percentatge.
fn rd_url_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Tradueix els codis de sortida de curl a un motiu humà en català, per a
/// la política «si hi ha error: marca'l i digues el motiu exacte».
fn rd_explain_curl(code: i32, stderr: &str, url: &str) -> String {
    let base = match code {
        67 => "Accés denegat: usuari o contrasenya incorrectes (o el servidor no admet accés per contrasenya; hi podria caler una clau SSH).",
        28 => "Temps esgotat: la xarxa ha trigat massa (prova una altra vegada o un port més accessible).",
        7  => "Connexió refusada: cap servei escolta a aquell host/port (comprova el port i el tallafocs).",
        6  => "No s'ha pogut resoldre el nom del host: revisa que l'adreça estigui ben escrita i visible des d'aquesta xarxa.",
        3  => "La URL està mal escrita (un port amb lletres o una ruta rara).",
        9  => "Accés a un recurs sense permisos (la carpeta remota no es pot crear o no té drets d'escriptura).",
        78 => "El fitxer no s'ha pogut deixar al destí (permisos o disc ple al servidor).",
        10 => "Usuari FTP no acceptat pel servidor.",
        51 => "Error de SSL/TLS amb el servidor (mmm, prova FTPS--insecure ja ve posat; revisa que el servei sigui realment FTPS).",
        60 => "Certificat del servidor desconegut/self-signed: per a SFTP/FTPS ja posem --insecure; si encara falla, comprova que sigui realment SFTP/FTPS i no SFTP per un port d'un servei HTTP.",
        _  => "Error de transferència (veure el detall de sota).",
    };
    format!(
        "curl ha sortit amb el codi {} (~motiu: {}).\nDestí: {}.\nDetall del curl: {}",
        code,
        base,
        url,
        if stderr.trim().is_empty() { "—".to_string() } else { stderr.trim().chars().take(400).collect() }
    )
}

/// Script d'instal·lació/arrencada del servidor (escrit a la carpeta remota).
pub const RD_SHELL_TEMPLATE: &str = r#"#!/usr/bin/env bash
set -euo pipefail

# ══════════════════════════════════════════════════════════════════════
#  NoOrbit · servidor d'IA remot  (generat per NoOrbit)
#  Exposa una API compatible amb OpenAI a:  http://(aquesta máquina):{{API_PORT}}/v1
#  Motor: {{BACKEND}}  ·  Model: {{MODEL}}
# ══════════════════════════════════════════════════════════════════════

DIR="$(cd "$(dirname "$0")" && pwd)"
PORT="{{API_PORT}}"
BACKEND="{{BACKEND}}"
MODEL="{{MODEL}}"
VENV="$DIR/noorbit-venv"
LOG="$DIR/noorbit-ia-server.log"
PID="$DIR/noorbit-ia-server.pid"

echo "[NoOrbit] · Servidor d'IA · (mmotor: $BACKEND, model: $MODEL, port: $PORT)"

# 1) Python + entorn virtual
if ! command -v python3 >/dev/null 2>&1; then
  echo "[NoOrbit][ERROR] Hi falta python3: instal·la'l amb el gestor del teu sistema (apt install python3-venv …) i torna a executar." >&2
  exit 1
fi
if [ ! -x "$VENV/bin/python" ]; then
  echo "[NoOrbit][1/4] Creo l'entorn virtual a $VENV …"
  python3 -m venv "$VENV"
fi
PIP="$VENV/bin/pip"

echo "[NoOrbit][2/4] Instal·lo les dependències (fastapi, uvicorn) …"
"$PIP" install --quiet --upgrade pip
"$PIP" install --quiet fastapi "uvicorn[standard]" requests

echo "[NoOrbit][3/4] Preparo el motor «$BACKEND» …"
if [ "$BACKEND" = "deepinfer" ]; then
  # Motor oficial de DeepSeek (github.com/deepseek-ai/DeepInfer): cal Linux
  # + GPU NVIDIA SM100 (CUDA 13) o Ascend 950 (CANN 9.2). Si aquest servidor
  # no el compleix l'instal·lació FALLARÀ AQUÍ amb el motiu exacte.
  if ! "$PIP" install --quiet deepinfer 2>/dev/null; then
    echo "[NoOrbit][ERROR] No s'ha pogut instal·lar «deepinfer»." >&2
    echo "  Motius habituals: (a) no és Linux, (b) no hi ha GPU SM100/Ascend," >&2
    echo "  (c) falta CUDA 13 / CANN 9.2, (d) Python no és 3.12." >&2
    echo "  Alternativa fàcil: NOORBIT_BACKEND=vllm (o simplement vllm serve)." >&2
    exit 3
  fi
else
  # vLLM fa el servei OpenAI natiu i és la via més provada per a servidors.
  "$PIP" install --quiet vllm || {
    echo "[NoOrbit][ERROR] No s'ha pogut instal·lar vllm (revisa GPU/CUDA, pip a l'entorn virtual)." >&2
    exit 3
  }
fi

# 4) Arrenca el servei: el wrapper noorbit_server_api.py (pujat amb el script)
#    exposa /v1/models i /v1/chat/completions encara que el motor trigui a
#    carregar el model (mentrestant respon 503 amb el motiu).
if [ -f "$PID" ] && kill -0 "$(cat "$PID")" 2>/dev/null; then
  echo "[NoOrbit] Ja hi ha un servei en marxa (pid $(cat "$PID")); l'ature per a tornar a arrencar."
  kill "$(cat "$PID")" 2>/dev/null || true
  sleep 2
fi
echo "[NoOrbit][4/4] Engego el servei al port $PORT (registre: $LOG) …"
NOORBIT_PORT="$PORT" NOORBIT_ENGINE="$BACKEND" NOORBIT_MODEL="$MODEL" \
  nohup "$VENV/bin/uvicorn" noorbit_server_api:app --host 0.0.0.0 --port "$PORT" \
  --app-dir "$DIR" >>"$LOG" 2>&1 &
echo $! >"$PID"
sleep 4
if curl -s --max-time 5 "http://127.0.0.1:$PORT/v1/models" >/dev/null 2>&1; then
  echo "[NoOrbit] ✔ Servidor EN MARXA:  http://$(hostname -i 2>/dev/null | awk '{print $1}'):$PORT/v1"
else
  echo "[NoOrbit] ⏳ Aixecat; el model encara està carregant (mira $LOG). Torna-ho a provar amb el botó «Comprova» de NoOrbit."
fi
"#;

/// Wrapper FastAPI que exposa la API OpenAI i parla amb el motor d'inferència
/// (deepinfer o vLLM) en aquest servidor. Les respostes tornen per HTTP a
/// NoOrbit, que les mostra al xat localment.
pub const RD_PY_TEMPLATE: &str = r#"#!/usr/bin/env python3
"""NoOrbit · servidor d'IA remot — API compatible amb OpenAI.

Exposa  GET /v1/models  i  POST /v1/chat/completions  (amb opcions de
streaming SSE i NO-streaming). Si el motor encara està carregant el model
respon 503 amb el motiu, i quan està llest genera les respostes.

Variables d'entorn (les posa `noorbit-ai-server.sh`):
  NOORBIT_PORT    port d'escolta (per defecte 8000)
  NOORBIT_ENGINE  "deepinfer" | "vllm"
  NOORBIT_MODEL   nom exacte del model (Hugging Face)
"""
import json
import os
import threading
import time
from typing import Optional

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse, StreamingResponse

try:
    import uvicorn
except Exception:  # pragma: no cover
    raise SystemExit("Hi falta uvicorn: executa `noorbit-ai-server.sh` primer.")

PORT = int(os.environ.get("NOORBIT_PORT", "8000"))
ENGINE = os.environ.get("NOORBIT_ENGINE", "deepinfer").lower()
MODEL = os.environ.get("NOORBIT_MODEL",
                       "deepseek-ai/DeepSeek-V4.1-Flash-1017")

app = FastAPI(title="NoOrbit AI Server", version="0.67.0")
STATE = {"llm": None, "error": None, "started": time.time()}


def _load_engine() -> None:
    """Carrega el motor en un fil de fons: mentrestant el servei respon
    503 «el motor encara carrega» per què la UI del local ho expliqui."""
    try:
        # deepinfer reflecteix l'API offline de vLLM (LLM + SamplingParams);
        # si pel servidor volem vLLM natiu, l'import canvia de paquet.
        if ENGINE == "deepinfer":
            from deepinfer import LLM, SamplingParams  # type: ignore
        else:
            from vllm import LLM, SamplingParams  # type: ignore
        STATE["llm"] = LLM(model=MODEL)
        STATE["error"] = None
        print(f"[NoOrbit] ✔ model «{MODEL}» carregat i a punt ({ENGINE}).", flush=True)
    except BaseException as exc:  # noqa: BLE001 — volem capturar-ho TOT
        STATE["error"] = f"{type(exc).__name__}: {exc}"
        print(f"[NoOrbit][ERROR] al carregar el motor: {STATE['error']}", flush=True)


threading.Thread(target=_load_engine, daemon=True).start()


def _reply_content(args):
    messages, sampling = args
    if ENGINE == "deepinfer":
        from deepinfer import SamplingParams  # type: ignore
    else:
        from vllm import SamplingParams  # type: ignore
    sp = SamplingParams(
        max_tokens=sampling.get("max_tokens", 1024),
        temperature=sampling.get("temperature", 0.7),
        top_p=sampling.get("top_p", 0.95),
    )
    outs = STATE["llm"].chat(messages=messages, sampling_params=sp)
    # vLLM/deepinfer: la resposta surt a .outputs[0].text
    for o in (outs if isinstance(outs, list) else [outs]):
        if hasattr(o, "outputs") and getattr(o, "outputs", None):
            return o.outputs[0].text
        if isinstance(o, dict) and "text" in o:
            return o["text"]
    return str(outs)


@app.get("/v1/models")
async def models():
    return {"object": "list", "data": [{"id": MODEL, "object": "model", "owned_by": "noorbit"}]}


@app.get("/v1/health")
async def health():
    return {"ok": STATE["llm"] is not None, "error": STATE["error"], "model": MODEL}


@app.get("/health")
async def health2():
    return await health()


@app.post("/v1/chat/completions")
async def chat_completions(req: Request):
    ask = await req.json()
    messages = ask.get("messages") or []
    stream = bool(ask.get("stream", False))
    sampling = {"max_tokens": ask.get("max_tokens") or 1024,
                "temperature": ask.get("temperature") if ask.get("temperature") is not None else 0.7,
                "top_p": ask.get("top_p") or 0.95}

    if STATE["llm"] is None:
        # El client (NoOrbit) entén el 503 com a «servidor viu, model pendent».
        why = STATE["error"] or "el motor encara està carregant el model"
        return JSONResponse(status_code=503, content={
            "error": {"message": f"[NoOrbit] El servidor és en marxa però el model encara no pot respondre ({why}). Torna-ho a provar en uns minuts."},
            "model": MODEL,
        })

    try:
        content = await _reply(messages, sampling)
    except BaseException as exc:  # noqa: BLE001
        return JSONResponse(status_code=500, content={
            "error": {"message": f"[NoOrbit][ERROR] {type(exc).__name__}: {exc}"}
        })

    now = int(time.time())
    ident = f"chatcmpl-{int(time.time() * 1000)}"
    if not stream:
        return {
            "id": ident, "object": "chat.completion", "created": now, "model": MODEL,
            "choices": [{"index": 0,
                         "message": {"role": "assistant", "content": content},
                         "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
        }

    async def sse():
        chunk = {
            "id": ident, "object": "chat.completion.chunk", "created": now, "model": MODEL,
            "choices": [{"index": 0, "delta": {"content": content}, "finish_reason": None}],
        }
        yield f"data: {json.dumps(chunk, ensure_ascii=False)}\n\n"
        end = {
            "id": ident, "object": "chat.completion.chunk", "created": now, "model": MODEL,
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
        }
        yield f"data: {json.dumps(end)}\n\ndata: [DONE]\n\n"

    return StreamingResponse(sse(), media_type="text/event-stream")


async def _reply(messages: list, sampling: dict) -> str:
    # La crida del motor és blocant (pot trigar minuts): la treiem del bucle
    # d'events de FastAPI amb un fil d'execució.
    import asyncio
    loop = asyncio.get_event_loop()
    return await loop.run_in_executor(None, _reply_content, (messages, sampling))


if __name__ == "__main__":
    uvicorn.run(app, host="0.0.0.0", port=PORT)
"#;

#[command]
pub async fn ai_remote_deploy(spec: RemoteDeploySpec) -> Result<String, String> {
    let host = spec.host.trim().trim_end_matches('/').to_string();
    if host.is_empty() {
        return Err("[NoOrbit][ERROR] Hi falta el host (adreça o IP del servidor).".to_string());
    }
    let proto = match spec.protocol.to_lowercase().as_str() {
        "sftp" | "ftps" | "ftp" => spec.protocol.to_lowercase(),
        other => {
            return Err(format!(
                "[NoOrbit][ERROR] Protocol desconegut «{}»: només sftp, ftps o ftp.",
                other
            ))
        }
    };
    let api_port = if spec.api_port == 0 { 8000 } else { spec.api_port };
    let backend = if spec.backend.eq_ignore_ascii_case("vllm") {
        "vllm".to_string()
    } else {
        "deepinfer".to_string()
    };
    let model = spec
        .model
        .as_deref()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| match backend.as_str() {
            "vllm" => "meta-llama/Llama-3.1-8B-Instruct".to_string(),
            _ => "deepseek-ai/DeepSeek-V4.1-Flash-1017".to_string(),
        });
    // La carpeta remota sempre amb barra inicial (ruta completa per sftp;
    // per ftp/ftps és relativa al directori d'entrada de l'usuari).
    let mut remote_dir = spec.remote_dir.trim().to_string();
    if !remote_dir.starts_with('/') {
        remote_dir = format!("/{}", remote_dir);
    }
    if remote_dir == "/" || remote_dir.len() < 2 {
        return Err("[NoOrbit][ERROR] Cap carpeta remota vàlida: tria una ruta tipus /opt/noorbit-ia.".to_string());
    }

    // ── 1) Generar els fitxers a la carpeta temporanèa del sistema ──────────
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("noorbit-remote-{}-{}", std::process::id(), stamp));
    let sh = tmp.join("noorbit-ai-server.sh");
    let py = tmp.join("noorbit_server_api.py");
    {
        let sh_content = RD_SHELL_TEMPLATE
            .replace("{{API_PORT}}", &api_port.to_string())
            .replace("{{BACKEND}}", &backend)
            .replace("{{MODEL}}", &model);
        // El wrapper només cal substituir-hi la versió informativa.
        let py_content = RD_PY_TEMPLATE.to_string();
        std::fs::create_dir_all(&tmp).map_err(|e| {
            format!("[NoOrbit][ERROR] No puc crear la carpeta temporal {}: {}", tmp.display(), e)
        })?;
        std::fs::write(&sh, sh_content).map_err(|e| {
            format!("[NoOrbit][ERROR] No puc escriure {}: {}", sh.display(), e)
        })?;
        std::fs::write(&py, py_content).map_err(|e| {
            format!("[NoOrbit][ERROR] No puc escriure {}: {}", py.display(), e)
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755));
        }
    }

    // ── 2) Pujar els dos fitxers amb curl (spawn_blocking: el bloc no és async)
    let url_user = rd_url_escape(&spec.user);
    let url_pass = rd_url_escape(&spec.password);
    let auth = if spec.user.is_empty() {
        String::new()
    } else if url_pass.is_empty() {
        format!("{}@", url_user)
    } else {
        format!("{}:{}@", url_user, url_pass)
    };
    let port_txt = spec.port.map(|p| format!(":{}", p)).unwrap_or_default();
    let url_base = format!("{}://{}{}{}", proto, auth, host, port_txt);
    let target_sh = format!("{}/noorbit-ai-server.sh", remote_dir.trim_end_matches('/'));
    let target_py = format!("{}/noorbit_server_api.py", remote_dir.trim_end_matches('/'));

    let sh_path = sh.display().to_string();
    let py_path = py.display().to_string();
    let base = url_base.clone();
    let wget = tokio::task::spawn_blocking(move || -> (Vec<(i32, String, String)>, String) {
        let mut results = Vec::new();
        for (target, local) in [(target_sh, sh_path), (target_py, py_path)] {
            let url = format!("{}{}", base, target);
            let mut args: Vec<String> = vec![
                "--upload-file".into(),
                local.clone(),
                "--ftp-create-dirs".into(),
                "--connect-timeout".into(),
                "20".into(),
                "--max-time".into(),
                "600".into(),
                "--silent".into(),
                "--show-error".into(),
            ];
            // SFTP: sense TTY no podem confirmar clau de host; FTPS: certs
            // propis normalment són locals. Sense això, no hi ha pujada.
            args.push("--insecure".into());
            args.push(url.clone());
            let out = std::process::Command::new("curl")
                .args(&args)
                .output();
            match out {
                Ok(o) => {
                    let code = o.status.code().unwrap_or(-1);
                    let err = String::from_utf8_lossy(&o.stderr).to_string();
                    results.push((code, url, err));
                    if code != 0 {
                        break; // el segon no té sentit si el primer ha fallat
                    }
                }
                Err(e) => {
                    results.push((-1, url, format!("No puc executar curl: {}", e)));
                    break;
                }
            }
        }
        (results, String::new())
    })
    .await
    .map_err(|e| format!("[NoOrbit][ERROR] fil de pujada: {}", e))?;

    let (uploads, _) = wget;
    let mut report = String::new();
    report.push_str(&format!("[NoOrbit][1/2] Fitxers generats a {} (script + servei API · motor {} · model {})\n", tmp.display(), backend, model));

    for (i, (code, url, err)) in uploads.iter().enumerate() {
        let fitxer = if i == 0 { "noorbit-ai-server.sh" } else { "noorbit_server_api.py" };
        if *code == 0 {
            report.push_str(&format!("[NoOrbit][2/2] ✔ «{}» pujat correctament per {}\n", fitxer, proto.to_uppercase()));
        } else {
            let motiu = rd_explain_curl(*code, err, url);
            let mut full = format!(
                "[NoOrbit][ERROR] La pujada de «{}» ha fallat.\n{}\n\n{}",
                fitxer, motiu, report
            );
            full.push_str(&format!(
                "\n— Proves ràpides: tens la contrasenya bé? Veus el servidor amb «ping {}»? El servei {}
és activat i el port obert?\n— Passos al PC (manuals): ssh {}@{} \"cd {}\" i a dins publica els dos fitxers a mà.\n",
                fitxer, proto, if spec.user.is_empty() { "<usuari>" } else { spec.user.trim() }, host, remote_dir
            ));
            return Err(full);
        }
    }

    // ── 3) Passos que queden al SERVIDOR (per SSH) ─────────────────────────
    let cmd_ssh = format!(
        "ssh {user}@{host} 'cd {dir} && chmod +x noorbit-ai-server.sh && ./noorbit-ai-server.sh'",
        user = if spec.user.is_empty() { "<usuari>" } else { spec.user.trim() },
        host = host,
        dir = remote_dir
    );
    report.push_str(&format!(
        "\n>>> ÚLTIM PAS (al servidor, via SSH):\n  {}\n\nEl script instal·la (python3-venv, fastapi, uvicorn, {}), arrenca el servei\nal port {} i no toca ni res més. Quan digui «Servidor EN MARXA», prem el botó «Comprova»\nd'aquest panell (o «Connecta») per verificar la API des de NoOrbit.\n",
        cmd_ssh,
        if backend == "deepinfer" { "deepinfer" } else { "vllm" },
        api_port
    ));
    report.push_str(&format!(
        "\nPer aturar el servei al servidor: ssh {user}@{host} 'kill $(cat {dir}/noorbit-ia-server.pid)'\n",
        user = if spec.user.is_empty() { "<usuari>" } else { spec.user.trim() },
        host = host,
        dir = remote_dir
    ));
    Ok(report)
}

/// Verificació del servidor d'IA remot ja desplegat: mateix diagnòstic que
/// el botó «Comprova» del gestor de proveïdors (connexió → /v1/models → model).
#[command]
pub async fn ai_remote_verify(
    host: String,
    api_port: Option<u16>,
    model: Option<String>,
) -> Result<EndpointCheck, String> {
    let host = host.trim().trim_end_matches('/').trim();
    if host.is_empty() {
        return Err("[NoOrbit][ERROR] Hi falta el host del servidor per verificar-ho.".to_string());
    }
    let port = api_port.filter(|p| *p != 0).unwrap_or(8000);
    let base = format!("http://{}:{}/v1", host, port);
    let model = model.unwrap_or_default();
    endpoint_check_inner(base, model, None, Some("openai".into())).await
}

/// Xat directe contra un proveïdor (independent del proveïdor actiu).
/// Autocomplete en línia (estil Copilot però amb la IA pròpia de l'usuari):
/// crida CRUA i CURTA — sense eines, sense events, timeout breu — perquè
/// pugui encaixar dins d'un tipeig sense molestar. Retorna només el fragment
/// a inserir (buit si el model no té res a dir).
#[command]
pub async fn ai_inline_complete(
    state: State<'_, AppState>,
    prefix: String,
    suffix: String,
    language: Option<String>,
    request_id: Option<String>,
) -> Result<String, String> {
    use std::time::Duration;

    let ai: Arc<crate::ai::AiManager> = state.ai_manager.clone();
    let id = ai.active_provider().await;
    let mgr: Arc<Mutex<api::ApiManager>> = state.api_manager.clone();
    let (provider, client) = {
        let guard = mgr.lock().unwrap();
        match guard.prepare(&id) {
            Ok(v) => v,
            Err(e) => return Err(e.to_string()),
        }
    };

    let lang = language.unwrap_or_default();
    let system = "Ets un motor d'autocompleció de codi integrat en un editor. Respon NOMÉS amb el fragment exacte que completa el lloc del cursor (sense repetir text previ, sense explicacions, sense claus markdown, sense fer res que ja hi hagi). Si no tens res útil a escriure, respon una cadena buida.";
    let prompt = format!(
        "Llenguatge: {}\n\nCODI ABANS del cursor (des del principi):\n{}\n\n⟦⟧ lloc del cursor\n\nCODI DESPRÉS del cursor:\n{}\n\nEscriu la continuació que aniria just al cursor.",
        lang,
        prefix.chars().rev().take(3000).collect::<String>().chars().rev().collect::<String>(),
        suffix.chars().take(600).collect::<String>(),
    );

    // Petició amb temps límit curt: s'anula sola si el model pesa massa.
    let dur = Duration::from_secs(8);
    let turns = [("user".to_string(), prompt)];

    // Senyal de cancel·lació (opcional): la interfície pot dir «ja no el vull»
    // quan l'usuari segueix escrivint o cancel·la el suggeriment.
    let cancel = request_id
        .as_ref()
        .map(|rid| rid.trim())
        .filter(|rid| !rid.is_empty())
        .map(|rid| {
            let flag = Arc::new(AtomicBool::new(false));
            if let Ok(mut m) = inline_pending().lock() {
                // Es netegen les entrades velles (el registre no ha de créixer).
                if m.len() > 64 {
                    let old: Vec<String> = m.keys().take(m.len() - 64).cloned().collect();
                    for k in old {
                        m.remove(&k);
                    }
                }
                m.insert(rid.to_string(), flag.clone());
            }
            (rid.to_string(), flag)
        });

    let chat_fut = api::chat(&client, &provider, &turns, Some(system));
    tokio::pin!(chat_fut);

    let outcome: Result<(String, Option<String>), String> = match cancel {
        Some((rid, flag)) => {
            let res = tokio::select! {
                r = tokio::time::timeout(dur, &mut chat_fut) => match r {
                    Ok(Ok(v)) => Ok(v),
                    Ok(Err(e)) => Err(e.to_string()),
                    Err(_) => Err("timeout".to_string()),
                },
                _ = wait_cancelled(flag) => Err("cancelled".to_string()),
            };
            // La petició ja no és «en vol»: es treu del registre.
            if let Ok(mut m) = inline_pending().lock() {
                m.remove(&rid);
            }
            res
        }
        None => match tokio::time::timeout(dur, chat_fut).await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => Err("timeout".to_string()),
        },
    };

    match outcome {
        Ok((content, _)) => {
            let mut out = content.trim_start().to_string();
            // Retall: sense blocs ```…``` ni capçaleres de conversa.
            if let Some(fence) = out.find("```") {
                out.truncate(fence);
                out = out.trim_end().to_string();
            }
            let out = out.trim().to_string();
            Ok(out)
        }
        Err(e) => Err(e),
    }
}

/// Cancel·la una petició d'autocompletat EN VOL: la inferència s'atura al
/// servidor (no consumeix CPU/GPU ni RAM fins a expirar).
#[command]
pub async fn ai_inline_complete_cancel(request_id: String) -> Result<(), String> {
    let rid = request_id.trim().to_string();
    if rid.is_empty() {
        return Ok(());
    }
    if let Ok(m) = inline_pending().lock() {
        if let Some(flag) = m.get(&rid) {
            flag.store(true, Ordering::Relaxed);
        }
    }
    Ok(())
}

#[command]
pub async fn ai_provider_chat(
    state: State<'_, AppState>,
    id: String,
    prompt: String,
    system: Option<String>,
) -> Result<String, String> {
    let mgr: Arc<Mutex<api::ApiManager>> = state.api_manager.clone();
    let (provider, client) = {
        let guard = mgr.lock().unwrap();
        guard.prepare(&id).map_err(|e| e.to_string())?
    };
    api::chat(
        &client,
        &provider,
        &[("user".to_string(), prompt.clone())],
        system.as_deref(),
    )
        .await
        .map(|(content, _)| content)
        .map_err(|e| e.to_string())
}

/// Activa un proveïdor (id "ollama" o el d'una integració remota) per a tota l'IA.
#[command]
pub async fn ai_select_provider(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let ai = state.ai_manager.clone();
    ai.set_active_provider(&id).await;
    Ok(())
}

#[command]
pub async fn ai_current_provider(state: State<'_, AppState>) -> Result<String, String> {
    let ai = state.ai_manager.clone();
    Ok(ai.active_provider().await)
}
