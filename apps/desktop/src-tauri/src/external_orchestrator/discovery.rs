//! Descobriment d'altres IAs locals presents al sistema, fora de NoOrbit
//! (Ollama, LM Studio, Jan, llama.cpp, vLLM…). S'usen tres mètodes com el pla
//! original: processos en execució, binaris al PATH i endpoints HTTP que
//! responen. Els duplicats es resolden: cada IA només es reporta una vegada,
//! amb l'estat més favorable (en marxa > instal·lat).

use serde::Serialize;
use std::time::Duration;

/// Una IA local externa trobada al sistema.
#[derive(Debug, Clone, Serialize)]
pub struct ExternalIa {
    pub name: String,
    /// Com l'hem trobada: "process" | "executable" | "endpoint".
    pub kind: String,
    /// "running" (hi pot respondre ara) | "installed" (només és al sistema).
    pub status: String,
    /// URL base de la seva API local (buida si només està instal·lada).
    pub base_url: String,
    /// Estil de comunicació: "ollama" (nativa) | "openai" (compatible /v1).
    pub api: String,
}

impl ExternalIa {
    pub fn is_running(&self) -> bool {
        self.status == "running"
    }
}

/// Candidat conegut: claus del seu procés, binaris al PATH i port local.
struct Candidate {
    name: &'static str,
    process_keys: &'static [&'static str],
    binaries: &'static [&'static str],
    port: u16,
    api: &'static str,
}

/// Taula d'IAs locals suportades (ampliable afegint-hi files).
const CANDIDATES: &[Candidate] = &[
    Candidate {
        name: "ollama",
        process_keys: &["ollama"],
        binaries: &["ollama"],
        port: 11434,
        api: "ollama",
    },
    Candidate {
        name: "lm-studio",
        process_keys: &["lm studio", "lm-studio", "lmstudio"],
        binaries: &["lms"],
        port: 1234,
        api: "openai",
    },
    Candidate {
        name: "jan",
        // Claus prou específiques per no confondre el nom «Jan» amb qualsevol
        // altre procés de l'ordinador.
        process_keys: &["jan.app/contents/macos", "jan.exe", "/jan ", "jan-server"],
        binaries: &["jan"],
        port: 1337,
        api: "openai",
    },
    Candidate {
        name: "llama.cpp",
        process_keys: &["llama-server"],
        binaries: &["llama-server"],
        port: 8080,
        api: "openai",
    },
    Candidate {
        name: "vllm",
        process_keys: &["vllm"],
        binaries: &["vllm"],
        port: 8000,
        api: "openai",
    },
];

/// Bolcat de la taula de processos en minúscules, per buscar-hi les claus.
fn process_table() -> String {
    #[cfg(unix)]
    {
        std::process::Command::new("ps")
            .arg("aux")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
            .unwrap_or_default()
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("tasklist")
            .creation_flags(0x00000008) // sense finestra de consola
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
            .unwrap_or_default()
    }
    #[cfg(not(any(unix, windows)))]
    {
        String::new()
    }
}

/// Comprova si algun procés escolta al port local indicat.
fn port_open(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(400),
    )
    .is_ok()
}

/// Executa tots els mètodes de descobriment i retorna la llista unificada.
/// `http` s'usa per confirmar que l'endpoint realment respon abans de
/// marcar una IA com a «running» (un port obert no sempre és la seva API).
pub async fn discover_all(http: &reqwest::Client) -> Vec<ExternalIa> {
    let table = process_table();
    let mut out = Vec::new();
    for c in CANDIDATES {
        let base_url = format!("http://127.0.0.1:{}", c.port);
        let process_running = c
            .process_keys
            .iter()
            .any(|k| table.contains(&k.to_lowercase()));
        // El port escolta i l'API respon (processos i rutes dels binaris).
        let endpoint_alive = port_open(c.port) && probe_api(http, &base_url, c.api).await;
        let installed = c.binaries.iter().any(|b| which::which(b).is_ok());

        if endpoint_alive {
            out.push(ExternalIa {
                name: c.name.into(),
                kind: "endpoint".into(),
                status: "running".into(),
                base_url,
                api: c.api.into(),
            });
        } else if process_running {
            out.push(ExternalIa {
                name: c.name.into(),
                kind: "process".into(),
                status: "running".into(),
                base_url,
                api: c.api.into(),
            });
        } else if installed {
            out.push(ExternalIa {
                name: c.name.into(),
                kind: "executable".into(),
                status: "installed".into(),
                base_url,
                api: c.api.into(),
            });
        }
    }
    out
}

/// Prova ràpida que l'endpoint és realment l'API esperada: Ollama amb
/// /api/tags, les compatibles amb OpenAI amb /v1/models.
async fn probe_api(http: &reqwest::Client, base_url: &str, api: &str) -> bool {
    let path = if api == "ollama" { "/api/tags" } else { "/v1/models" };
    http.get(format!("{}{}", base_url, path))
        .timeout(Duration::from_millis(1500))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}
