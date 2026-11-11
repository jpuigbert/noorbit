//! Orquestrador de l'agent IA multimodal de NoOrbit.
//!
//! Rutes per modalitat:
//! - code/text  -> Ollama
//! - image      -> ComfyUI (si està actiu) o Ollama (fallback)
//! - 3d         -> backend 3D local (Ollama genera l'script Blender)

pub mod runtools;

use crate::ai::AiManager;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{Emitter, Manager};

/// Emets un pas de progrés cap a la UI (xat de l'agent) en temps real,
/// perquè l'usuari vega què està fent la IA mentre treballa. S'etiqueta amb
/// el xat que ha iniciat la tasca (`in_session`), així dos xats en paral·lel
/// no mesclen els passos dels seus torns.
fn progress(app: &tauri::AppHandle, label: &str, detail: &str, ok: bool) {
    let _ = app.emit(
        "agent://progress",
        serde_json::json!({
            "label": label,
            "detail": detail,
            "ok": ok,
            "session": crate::ai::current_session(),
        }),
    );
}

/// Agent d'usuari de navegador: alguns cercadors web bloquegen clients buits.
const WEB_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// Descodificació percentual mínima (per desempaquetar l'enllaç de DDG).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Treu les etiquetes HTML i normalitza els espais (per llegir el text).
fn strip_tags(html: &str) -> String {
    let sin = match regex::Regex::new(r"(?s)<[^>]+>") {
        Ok(re) => re.replace_all(html, " ").to_string(),
        Err(_) => html.to_string(),
    };
    sin.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// L'URL real del redirecte de DuckDuckGo: `...uddg=<codificat>&rut=...`.
fn extract_uddg(href: &str) -> String {
    if let Some(pos) = href.find("uddg=") {
        let rest = &href[pos + 5..];
        let enc = rest.split('&').next().unwrap_or(rest);
        percent_decode(enc)
    } else if href.starts_with("http") {
        href.to_string()
    } else {
        String::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    Code,
    Text,
    Image,
    #[serde(rename = "3d")]
    ThreeD,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub prompt: String,
    pub modality: Modality,
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    pub label: String,
    pub detail: String,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub text: String,
    pub files: Vec<String>,
    pub steps: Vec<AgentStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendInfo {
    pub id: String,
    pub name: String,
    pub modality: String,
    pub available: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub workspace: Option<PathBuf>,
    pub auto_apply: bool,
}

/// Petició per a les accions "aplica a Blender / Unreal": rep el text que
/// l'usuari escriu al xat i en fa una edició real dins l'aplicació.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyRequest {
    pub prompt: String,
    /// Xat que ha demanat l'acció: els passos i la IA fan servir el seu.
    #[serde(default)]
    pub session: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self { workspace: None, auto_apply: false }
    }
}

pub struct AgentOrchestrator {
    pub config: AgentConfig,
    http: reqwest::Client,
}

impl AgentOrchestrator {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Detecta quins backends locals hi ha disponibles ara mateix.
    pub async fn check_backends(&self) -> Vec<BackendInfo> {
        let mut backends = Vec::new();

        // Ollama
        let ollama = self
            .http
            .get("http://localhost:11434/api/tags")
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        backends.push(BackendInfo {
            id: "ollama".into(),
            name: "Ollama".into(),
            modality: "text/code".into(),
            available: ollama,
            detail: if ollama { Some("localhost:11434".into()) } else { None },
        });

        // ComfyUI
        let comfy = self
            .http
            .get("http://localhost:8188/system_stats")
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        backends.push(BackendInfo {
            id: "comfyui".into(),
            name: "ComfyUI".into(),
            modality: "image".into(),
            available: comfy,
            detail: if comfy { Some("localhost:8188".into()) } else { None },
        });

        // Blender (socket de l'add-on)
        let blender = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], crate::blender::DEFAULT_SOCKET_PORT)),
            std::time::Duration::from_millis(400),
        )
        .is_ok();
        backends.push(BackendInfo {
            id: "blender".into(),
            name: "Blender".into(),
            modality: "3d".into(),
            available: blender,
            detail: if blender { Some("socket actiu".into()) } else { None },
        });

        // Unreal (Remote Control API)
        let unreal = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], 30010)),
            std::time::Duration::from_millis(400),
        )
        .is_ok();
        backends.push(BackendInfo {
            id: "unreal".into(),
            name: "Unreal Engine 5".into(),
            modality: "game".into(),
            available: unreal,
            detail: if unreal { Some("RC :30010".into()) } else { None },
        });

        backends
    }

    /// Planifica una petita tasca sense executar-la.
    pub async fn plan(&self, ai: &AiManager, request: &AgentRequest) -> Result<AgentResult> {
        let system = "ETS L'AGENT DE NOORBIT. Respon NOMÉS un pla d'execució \
            en passos curts numerats, sense prosa addicional.";
        let text = ai.chat(&request.prompt, Some(system)).await?;
        Ok(AgentResult {
            text,
            files: vec![],
            steps: vec![AgentStep {
                label: "plan".into(),
                detail: "Pla generat".into(),
                ok: true,
            }],
        })
    }

    /// Resposta ràpida de text.
    pub async fn quick(&self, ai: &AiManager, prompt: &str) -> Result<String> {
        ai.chat(prompt, None).await
    }

    /// Cerca a DuckDuckGo (HTML, sense clau d'API) i torna (títol, URL, snippet).
    async fn web_search(&self, query: &str) -> Vec<(String, String, String)> {
        let resp = match self
            .http
            .get("https://html.duckduckgo.com/html/")
            .query(&[("q", query)])
            .header("User-Agent", WEB_UA)
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        let html = match resp.text().await { Ok(t) => t, Err(_) => return vec![] };
        let (Ok(tit), Ok(sni)) = (
            regex::Regex::new(r#"(?s)<a[^>]*class="result__a"[^>]*href="([^"]*)"[^>]*>(.*?)</a>"#),
            regex::Regex::new(r#"(?s)<a[^>]*class="result__snippet"[^>]*>(.*?)</a>"#),
        ) else {
            return vec![];
        };
        let titles: Vec<(String, String)> = tit
            .captures_iter(&html)
            .map(|c| {
                let href = c.get(1).map(|m| m.as_str()).unwrap_or("");
                let title = strip_tags(c.get(2).map(|m| m.as_str()).unwrap_or(""));
                (title, extract_uddg(href))
            })
            .filter(|(t, _)| !t.trim().is_empty())
            .take(6)
            .collect();
        let snippets: Vec<String> = sni
            .captures_iter(&html)
            .map(|c| strip_tags(c.get(1).map(|m| m.as_str()).unwrap_or("")))
            .collect();
        titles
            .into_iter()
            .enumerate()
            .map(|(i, (t, u))| (t, u, snippets.get(i).cloned().unwrap_or_default()))
            .collect()
    }

    /// Descarrega una pàgina i en torna el text net (retallat).
    async fn web_fetch(&self, url: &str) -> Option<String> {
        if url.is_empty() {
            return None;
        }
        let resp = self
            .http
            .get(url)
            .header("User-Agent", WEB_UA)
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let html = resp.text().await.ok()?;
        let brut = match regex::Regex::new(r"(?is)<(script|style|noscript|header|footer|nav)[^>]*>.*?</\1>") {
            Ok(re) => re.replace_all(&html, " ").to_string(),
            Err(_) => html,
        };
        let text = strip_tags(&brut);
        Some(text.chars().take(4000).collect())
    }

    /// Llegeix una pàgina amb el NAVEGADOR INTERN: l'obre amagada, l'espera
    /// i en torna el text ja renderitzat (JavaScript inclòs). Si la pàgina
    /// demana una acció humana —iniciar sessió, captcha— la finestra es fa
    /// visible perquè la faça l'usuari amb el SEU compte: la IA no supla
    /// identitats ni esquiva proteccions.
    async fn browser_read(&self, app: &tauri::AppHandle, url: &str) -> Result<String> {
        crate::browser::open(app, url, true)?;
        let mut last = String::new();
        for _ in 0..3 {
            match crate::browser::extract_text(app, std::time::Duration::from_secs(8)).await {
                Ok(ex) => {
                    if crate::browser::needs_human_action(&ex.text) {
                        crate::browser::set_visible(app, true).ok();
                        return Ok(format!(
                            "(La pàgina {} sembla demanar una acció humana —sessió o \
                             captcha— i ja la tens en pantalla perquè la completes. \
                             Entre temps, aquest és el text visible:)\n{}",
                            url, ex.text
                        ));
                    }
                    if ex.text.chars().count() > 80 {
                        return Ok(ex.text);
                    }
                    last = ex.text;
                }
                Err(e) => last = e.to_string(),
            }
        }
        Err(anyhow!(
            "El navegador intern no n'ha extret text: {}",
            last
        ))
    }

    /// Mode «pensament profund»: busca informació fresca a internet, consulta
    /// altres IAs en línia i demana al model (local o actiu) que sintetitzi
    /// una resposta final fonamentada. Cada fase s'emet com a progrés.
    pub async fn deep_think(
        &self,
        ai: &AiManager,
        app: &tauri::AppHandle,
        prompt: &str,
    ) -> Result<String> {
        progress(
            app,
            "Pensament profund",
            "Buscaré a internet i consultaré altres IAs abans de respondre.",
            true,
        );

        // 1) Cerca web: general + dirigida a fonts tècniques (StackOverflow,
        //    GitHub), que és on viuen les solucions reals de programari.
        let mut results = self.web_search(prompt).await;
        let mut seen: std::collections::HashSet<String> =
            results.iter().map(|r| r.1.clone()).collect();
        for site in ["stackoverflow.com", "github.com"] {
            let extra = self.web_search(&format!("site:{} {}", site, prompt)).await;
            for (title, url, sn) in extra {
                if !url.is_empty() && seen.insert(url.clone()) {
                    results.push((title, url, sn));
                }
            }
        }
        results.truncate(8);
        let n = results.len();
        let detail = if n > 0 {
            format!("He trobat {} resultats.", n)
        } else {
            "La cerca no ha retornat res (sense connexió o bloqueig).".to_string()
        };
        progress(app, "Cerca a internet", &detail, n > 0);

        let mut ctx = String::new();
        if n > 0 {
            ctx.push_str("RESULTATS DE CERCA A INTERNET:\n");
            for (title, url, sn) in &results {
                ctx.push_str(&format!("- {}", title));
                if !url.is_empty() {
                    ctx.push_str(&format!(" — {}", url));
                }
                if !sn.is_empty() {
                    ctx.push_str(&format!(": {}", sn));
                }
                ctx.push('\n');
            }
            // 2) Llegir la primera pàgina rellevant. Si el text pla (HTTP) no
            //    bastona —pàgina amb JavaScript, buida o blocada—, la IA obre
            //    el NAVEGADOR INTERN i en llegeix el contingut renderitzat; si
            //    la pàgina demana una acció humana (sessió, captcha), la
            //    finestra es fa visible perquè la faça l'usuari.
            if let Some((_, first_url, _)) = results.iter().find(|(_, u, _)| !u.is_empty()) {
                progress(app, "Llegint pàgina", &format!("Obrint {}", first_url), true);
                let page = match self.web_fetch(first_url).await {
                    Some(p) if p.chars().count() > 200 => Some(p),
                    partial => {
                        progress(
                            app,
                            "Navegador intern",
                            "La pàgina necessita JavaScript; l'obro al navegador intern per llegir-la renderitzada.",
                            true,
                        );
                        match self.browser_read(app, first_url).await {
                            Ok(tx) => Some(tx),
                            Err(e) => {
                                progress(
                                    app,
                                    "Navegador intern",
                                    &format!("No he pogut llegir-la: {}", e),
                                    false,
                                );
                                partial
                            }
                        }
                    }
                };
                if let Some(page) = page {
                    ctx.push_str("\nCONTINGUT DE LA PÀGINA PRINCIPAL:\n");
                    ctx.push_str(&page.chars().take(2500).collect::<String>());
                    ctx.push('\n');
                }
            }
        }

        // 3) Altres IAs en línia
        progress(
            app,
            "Consultant altres IAs",
            "Envio la pregunta als proveïdors en línia configurats.",
            true,
        );
        let peers = ai.ask_peers(prompt, 3).await;
        if peers.is_empty() {
            // Sense cap token d'API existeix la via MANUAL i legal: el xat web
            // de la IA triada, obert al navegador intern, on l'usuari entra amb
            // el SEU compte. Cap botó: la mateixa IA escriu la directriu
            // «NB|PREGUNTA_IA|…» i NoOrbit l'executa (menys automàtic en
            // l'accedir al servei, del tot legítim en les credencials).
            ctx.push_str(
                "\n(Sense proveïdors en línia amb clau: consulta manual possible. \
                 PREGUNTA a l'usuari quina IA web vols usar — DeepSeek, ChatGPT, \
                 Claude, Gemini, Perplexity o Grok. Després, directament al xat, \
                 escriu una línia «NB|PREGUNTA_IA|nom|la pregunta»: NoOrbit obrirà \
                 el navegador intern, l'usuari hi posarà les seues credencials, i \
                 la pregunta s'escriurà al xat web per a llegir-ne la resposta \
                 visible en pantalla. Etiqueta sempre eixes respostes com a \
                 «obtingudes del xat web de X amb el compte de l'usuari».)\n",
            );
            progress(
                app,
                "Altres IAs",
                "No hi ha tokens configurats: via MANUAL disponible — digu'm quina \
                 IA web vols usar (DeepSeek, ChatGPT, Claude, Gemini, Perplexity o \
                 Grok) i escriuré la directriu «NB|PREGUNTA_IA|…» al xat: sense \
                 cap botó, NoOrbit obrirà el navegador i preguntarà amb el teu \
                 propi compte (les credencials les poses tu).",
                false,
            );
        } else {
            ctx.push_str("\nRESPOSTES D'ALTRES IAS:\n");
            for (name, ans) in &peers {
                let a: String = ans.chars().take(1200).collect();
                ctx.push_str(&format!("- {}:\n{}\n", name, a.trim()));
            }
            progress(app, "Altres IAs", &format!("He consultat {} IAs en línia.", peers.len()), true);
        }

        // 4) Síntesi amb el model actiu (local o núvol)
        progress(app, "Sintetitzant", "Redactant la resposta final amb totes les fonts.", true);
        let system = "ETS L'AGENT DE NOORBIT EN MODE PROFUND. Tens a sota informació \
            fresca d'internet i respostes d'altres IAs. Redacta UNA resposta final, \
            completa i en català, fonamentant-te en aquest material i resolent-ne les \
            discrepàncies. Si les fonts són insuficients, digues-ho clarament. Acaba \
            amb una llista «Fonts:» amb les URL reals emprades (si n'hi ha).";
        let user = format!("{}\n\n--- MATERIAL DE CONSULTA ---\n{}", prompt, ctx);
        let final_text = ai.chat(&user, Some(system)).await?;
        Ok(final_text)
    }

    /// Forja un especialista autònom: quan el model local no arriba sol, la IA
    /// es «replica» creant un agent nou amb nom original, lligat al millor
    /// model que càpiga a la RAM, que treballa sempre amb recursos externs
    /// (internet + altres IAs) i enllesteix una feina en segon pla acotada.
    pub async fn forge(
        &self,
        ai: &AiManager,
        app: &tauri::AppHandle,
        experts: &crate::experts::ExpertManager,
        store: &crate::autonomous::AutonomousStore,
        objective: &str,
    ) -> Result<serde_json::Value> {
        progress(
            app,
            "Forjant especialista",
            "La IA està creant un agent propi per resoldre això en segon pla.",
            true,
        );

        // 1) Identitat original: que el model local inventi nom, rol i instruccions.
        let name_sys = "Inventa la identitat d'un agent d'IA autònom i resiliant que \
            resoldrà el problema indicat. Respon NOMÉS un objecte JSON \
            {\"name\": \"nom curt i original (1-2 paraules)\", \"role\": \"rol en 5 paraules\", \
            \"system\": \"instruccions de treball en català, 3-4 frases\"}.";
        let name_user = format!("Problema: {}", objective);
        let invented = ai
            .chat(&name_user, Some(name_sys))
            .await
            .unwrap_or_default();
        let json_bit = invented
            .find('{')
            .and_then(|a| invented.rfind('}').map(|b| &invented[a..=b]));
        let parsed: serde_json::Value = json_bit
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::Value::Null);
        let stamp = chrono::Utc::now().format("%H%M");
        let name = parsed
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.chars().count() <= 40)
            .unwrap_or_else(|| format!("Nòmada-{}", stamp));
        let role = parsed
            .get("role")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Recuperador autònom")
            .to_string();
        let invented_sys = parsed
            .get("system")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // 2) Model lligat a la capacitat real: mai superar la màquina.
        let bound = ai.best_fit_model().await;

        // 3) Instruccions de pervivència i honestedat.
        let system_prompt = format!(
            "{}\n\n\u{2014}\u{2014} NATURALESA AUTÒNOMA \u{2014}\u{2014}\n\
             Ets un especialista forjat per NoOrbit. Sempre que treballis: \
             1) consulta internet (inclos StackOverflow i GitHub) i les altres IAs \
             en línia disponibles abans de respondre; 2) si una via falla, cerca'n \
             una altra sense rendir-te (memòria, altre proveïdor, altre terme de \
             cerca); 3) no SOBREPASSIS mai la capacitat de la màquina: treballa amb \
             el model que tens assignat i amb fragments petits; 4) sigues honest: \
             si el problema excedeix els recursos, digues clarament què falta i \
             quina seria la via alternativa (model més potent o un servei en línia). \
             No creïssis mai comptes ni accions en noms de l'usuari.",
            if invented_sys.trim().is_empty() {
                "Treballes per resoldre problemes que la IA local no resol sola."
            } else {
                invented_sys.trim()
            }
        );

        // 4) Alta al panell d'especialistes (persistents entre sessions).
        let expert = experts.add(crate::experts::ExpertInput {
            name: name.clone(),
            role: role.clone(),
            system_prompt,
            model: bound.clone(),
        })?;

        // 5) Feina autònoma acotada: sobreviu a disc i arrenca sola.
        let job = crate::autonomous::Job {
            id: format!("job-{}", chrono::Utc::now().timestamp_millis()),
            objective: objective.to_string(),
            focus: format!(
                "Objectiu: {}\nComença aproximant-s'hi amb fonts externes (internet, altres IAs).",
                objective
            ),
            expert_id: expert.id.clone(),
            expert_name: expert.name.clone(),
            steps: 0,
            errors: 0,
            consecutive_errors: 0,
            active: true,
            solved: false,
            journal: vec![],
            created_at: chrono::Utc::now().timestamp_millis(),
        };
        store.insert(&job);
        progress(
            app,
            "Especialista forjat",
            &format!(
                "«{}» ({}) viu al panell d'especialistes i treballa sol en segon pla fins resoldre-ho o esgotar el pressupost.",
                name, role
            ),
            true,
        );
        Ok(serde_json::json!({
            "job": job,
            "expert": expert,
            "boundModel": bound,
        }))
    }

    /// Executa una tasca segons la modalitat.
    pub async fn run(&self, ai: &AiManager, request: &AgentRequest) -> Result<AgentResult> {
        let mut steps = Vec::new();
        match request.modality {
            Modality::Code | Modality::Text => {
                let system = if request.modality == Modality::Code {
                    "ETS L'AGENT DE CODI DE NOORBIT. Genera codi complet i funcional. \
                     Usa blocs de codi markdown amb llenguatge."
                } else {
                    "ETS L'ASSISTENT DE TEXT DE NOORBIT. Respon de forma clara i directa."
                };
                let text = ai.chat(&request.prompt, Some(system)).await?;
                steps.push(AgentStep {
                    label: "ollama".into(),
                    detail: "Resposta generada".into(),
                    ok: true,
                });
                Ok(AgentResult { text, files: vec![], steps })
            }
            Modality::Image => {
                // Generació real: primer ComfyUI local si la màquina té prou
                // RAM; si no, els proveïdors en línia que l'usuari haja
                // registrat amb token. Sense cap dels dos, l'error explica
                // com activar-los (no fingim un èxit «pendent de cua»).
                let mut opts = crate::imgen::GenOpts::default();
                opts.prompt = request.prompt.clone();
                match crate::imgen::generate(ai.remote_manager(), &opts).await {
                    Ok(img) => {
                        steps.push(AgentStep {
                            label: img.backend.clone(),
                            detail: format!("Imatge generada amb {}", img.model),
                            ok: true,
                        });
                        let text = format!(
                            "🖼️ Imatge generada (backend: {}, model: {}) i desada a {}",
                            img.backend, img.model, img.path
                        );
                        Ok(AgentResult { text, files: vec![img.path], steps })
                    }
                    Err(e) => Err(e),
                }
            }
            Modality::ThreeD => {
                let system = "ETS L'AGENT 3D DE NOORBIT. Genera UN sol script Python \
                     per a Blender (bpy) que construeixi l'escena demanada. \
                     Sense explicacions, només codi dins un bloc ```python.";
                let text = ai.chat(&request.prompt, Some(system)).await?;
                steps.push(AgentStep {
                    label: "ollama->blender".into(),
                    detail: "Script bpy generat".into(),
                    ok: true,
                });
                Ok(AgentResult { text, files: vec![], steps })
            }
        }
    }

    /// Executa una tasca amb **pressupost de temps** (Objectiu 4). Mentre la
    /// tasca és dins del llindar de primer pla, s'executa normal i la UI la
    /// segueix en directe. Si se'n surt, es **degrada a segon pla**: es baixa
    /// la prioritat del procés, es posa Ollama en mode eco (menys fils/context)
    /// i la UI només en mostra un indicador compacte (event «task://state»
    /// amb state=background). La tasca continua fins acabar o fins al límit
    /// total. L'usuari pot tornar-la a primer pla amb `task_bring_to_foreground`.
    pub async fn run_with_budget(
        &self,
        app: &tauri::AppHandle,
        ai: &AiManager,
        request: &AgentRequest,
        budget: TaskBudget,
    ) -> Result<AgentResult> {
        use std::time::{Duration, Instant};
        let start = Instant::now();
        let emit_state = |state: &str, elapsed_ms: u64| {
            let _ = app.emit(
                "task://state",
                serde_json::json!({
                    "task_id": budget.task_id,
                    "state": state,
                    "elapsed_ms": elapsed_ms,
                }),
            );
        };
        emit_state("foreground", 0);

        let fut = self.run(ai, request);
        tokio::pin!(fut);

        let fg_ms = budget.foreground_limit_ms.max(1);
        let mut fg_deadline = start + Duration::from_millis(fg_ms);
        let total_deadline = if budget.total_limit_ms > 0 {
            Some(start + Duration::from_millis(budget.total_limit_ms))
        } else {
            None
        };
        let mut in_background = false;

        loop {
            if !in_background {
                let now = Instant::now();
                if now < fg_deadline {
                    let sleep_for = fg_deadline - now;
                    tokio::select! {
                        r = &mut fut => {
                            emit_state("done", start.elapsed().as_millis() as u64);
                            return r;
                        }
                        _ = tokio::time::sleep(sleep_for) => { /* torna a la part de dalt i degrada */ }
                    }
                    continue;
                }
                // S'ha esgotat el temps de primer pla → passa a segon pla.
                in_background = true;
                if BG_ACTIVE.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    adjust_priority(true);
                }
                ai.set_background(true);
                emit_state("background", start.elapsed().as_millis() as u64);
                continue;
            }

            // ── Mode segon pla ──
            // L'usuari ha premut «Mostra»: tornem a primer pla amb temps nou.
            if take_foreground_request(&budget.task_id) {
                if BG_ACTIVE.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) == 1 {
                    adjust_priority(false);
                }
                ai.set_background(false);
                in_background = false;
                fg_deadline = Instant::now() + Duration::from_millis(fg_ms);
                emit_state("foreground", start.elapsed().as_millis() as u64);
                continue;
            }
            // Límit total: abandonem la feina (allibera la futura blocant).
            if let Some(td) = total_deadline {
                if Instant::now() >= td {
                    if BG_ACTIVE.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) == 1 {
                        adjust_priority(false);
                    }
                    ai.set_background(false);
                    emit_state("error", start.elapsed().as_millis() as u64);
                    return Err(anyhow!("La tasca ha superat el temps màxim permès"));
                }
            }
            // Espera amb polls espaiats (menys re-render i menys CPU).
            let poll = Duration::from_millis(if on_battery() { 5000 } else { budget.background_poll_ms });
            match tokio::time::timeout(poll, &mut fut).await {
                Ok(r) => {
                    // Acabada en segon pla: restaura prioritat i mode eco.
                    if BG_ACTIVE.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) == 1 {
                        adjust_priority(false);
                    }
                    ai.set_background(false);
                    emit_state("done", start.elapsed().as_millis() as u64);
                    return r;
                }
                Err(_) => {
                    let _ = app.emit(
                        "task://progress",
                        serde_json::json!({
                            "task_id": budget.task_id,
                            "elapsed_ms": start.elapsed().as_millis() as u64,
                        }),
                    );
                }
            }
        }
    }

    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "auto_apply": self.config.auto_apply,
            "workspace": self.config.workspace.as_ref().map(|p| p.to_string_lossy().to_string()),
        })
    }

    /// Extreu el codi d'una resposta de la IA: prioritza el primer bloc
    /// ```python (o ```py); si no n'hi ha, usa el text net.
    fn extract_code(reply: &str) -> String {
        for fence in ["```python", "```py", "```"] {
            if let Some(start) = reply.find(fence) {
                let after = &reply[start + fence.len()..];
                if let Some(end) = after.find("```") {
                    let code = after[..end].trim();
                    if !code.is_empty() {
                        return code.to_string();
                    }
                }
            }
        }
        reply.trim().to_string()
    }

    /// Si el codi executat ha imprimit el marcador NOORBIT_VIDEO_OK (l'IA ha
    /// animat l'escena i ha desat una còpia .blend), llença el render del
    /// vídeo headless i afegeix el resultat (o l'avís) al missatge final.
    async fn maybe_render_video(app: &tauri::AppHandle, stdout: &str, base: String) -> String {
        let path = stdout
            .lines()
            .find_map(|l| {
                l.split(crate::blender::VIDEO_MARKER).nth(1).map(|p| {
                    p.trim().trim_matches('"').split_whitespace().next().unwrap_or("").to_string()
                })
            })
            .filter(|p| !p.is_empty());
        let Some(blend) = path else {
            return base;
        };
        progress(app, "blender", "🎬 Renderitzant el vídeo (pot trigar uns minuts)…", true);
        let res = tauri::async_runtime::spawn_blocking(move || {
            crate::blender::render_blender_video(&blend)
        })
        .await;
        match res {
            Ok(Ok(video)) => format!("{}\n\n🎬 Vídeo renderitzat: {}", base, video),
            Ok(Err(e)) => format!(
                "{}\n\n⚠️ L'animació ja és a l'escena, però el render del vídeo ha fallat: {}",
                base, e
            ),
            Err(e) => format!("{}\n\n⚠️ Render del vídeo: {}", base, e),
        }
    }

    /// Text curt que informa de la variació d'objectes a l'escena entre
    /// l'execució (abans → després). Buit si no es pot calcular.
    fn count_delta(before: Option<usize>, after: Option<usize>) -> String {
        match (before, after) {
            (Some(b), Some(a)) if a != b => {
                format!("\n\n🧊 Objectes a l'escena: {} → {}", b, a)
            }
            (Some(b), Some(_)) => format!(
                "\n\n🧊 Objectes a l'escena: {} (no ha canviat el nombre; potser has \
modificat un objecte existent).",
                b
            ),
            _ => String::new(),
        }
    }

    /// Demana a la IA un script bpy per editar l'escena de Blender a partir
    /// del text de l'usuari, l'executa al pont i, si falla, torna-hi una
    /// vegada amb l'error. Retorna (missatge_per_a_la_UI, pensament).
    pub async fn apply_blender(
        &self,
        ai: &AiManager,
        app: &tauri::AppHandle,
        prompt: &str,
    ) -> Result<(String, bool)> {
        let system = r#"ETS L'AGENT 3D DE NOORBIT per a BLENDER. L'usuari t'explica què ha
de fer a l'escena (crear, esborrar, moure, escalar, girar, posar material, afegir
llum/càmera/text, etc.). Respon NOMÉS amb UN script Python dins un bloc ```python
que s'executi dins Blender amb la API bpy i que ACONSEEIXI REALMENT la tasca:
usa bpy.ops.* o bpy.data.* per crear o modificar objectes.
PROHIBIT entregar un script que només compti o llisti objectes
(p. ex. print(len(bpy.context.scene.objects))) llevat que se't demanis expressament.
Termina sempre amb un print() en català que digui QUÈ has fet.
EXEMPLE del format esperat:
```python
import bpy
bpy.ops.mesh.primitive_cube_add(size=2, location=(0, 0, 0))
bpy.context.active_object.name = "NoOrbit_Cub"
print("Creat el cub NoOrbit_Cub")
```
RECEPTES SIMILARS (codis provats que funcionen — imita el més semblant a la
taska demanada i adapta noms, mides i colors; NO inventis sintaxis que no vist a baix):
- CREAR una forma:
```python
import bpy
bpy.ops.mesh.primitive_uv_sphere_add(radius=1.5, location=(0, 0, 0))
ob = bpy.context.view_layer.objects.active; ob.name = "NoOrbit_Esfera"
print("Creat NoOrbit_Esfera")
```
- MATERIAL amb color (bpy.data.materials, metallic):
```python
import bpy
mat = bpy.data.materials.new("NoOrbit_Mat"); mat.use_nodes = True
bsdf = mat.node_tree.nodes["Principled BSDF"]
bsdf.inputs["Base Color"].default_value = (0.8, 0.2, 0.1, 1.0)
bsdf.inputs["Metallic"].default_value = 0.6
ob.material_slots.clear(); ob.material_slot_add(); ob.material_slots[0].material = mat
```
- TRIAR un objecte Ja EXISTENT i mou-lo/escalar/girar-lo:
```python
import bpy
if "NoOrbit_Cub" in bpy.data.objects:
    ob = bpy.data.objects["NoOrbit_Cub"]
    ob.location = (1.0, 0.0, 0.5)
    ob.scale = (1.5, 1.5, 1.0)
    ob.rotation_euler = (0.0, 0.0, 0.3)
    print("Deformat NoOrbit_Cub")
```
- LLUM i CÀMERA orientades cap a l'escena:
```python
import bpy
bpy.ops.object.light_add(type='SUN', location=(5, -5, 10))
lam = bpy.context.view_layer.objects.active; lam.data.energy = 3.0
bpy.ops.object.camera_add(location=(7, -6, 5),
    rotation=(1.1, 0.0, 0.8))
bpy.context.scene.camera = bpy.context.view_layer.objects.active
print("Afegida llum i càmera")
```
- ANIMAR (moure location → dues keyframes):
```python
import bpy
ob = bpy.data.objects["NoOrbit_Esfera"]
ob.location = (0, 0, 0); ob.keyframe_insert(data_path="location", frame=1)
ob.location = (0, 0, 3); ob.keyframe_insert(data_path="location", frame=60)
print("Animat l'esfera (1..60)")
```
- ESBORRAR per nom (només si se't demana):
```python
import bpy
if "NoOrbit_Esfera" in bpy.data.objects:
    bpy.data.objects.remove(bpy.data.objects["NoOrbit_Esfera"], do_unlink=True)
    print("Esborrat NoOrbit_Esfera")
```
- EDITAR UNA FIGURA DE VÀRIES PECES en viu (la majoria d'ordres de l'usuari
seran d'aquest tipus: «fes-lo més petit», «mou-lo aquí», «posa'l verd»,
«borra'l»). Cada peça té el PREFIX del propi nom de la figura (p. ex.
«Nan_»): troba-les TOTES per prefix i actua sobre el grup sencer.
```python
import bpy
peces = [o for o in bpy.data.objects if o.name.startswith("Nan_")]
# MOURE TOTA LA FIGURA (mateix desplaçament per cada peça):
from mathutils import Vector
dx, dy, dz = 2.0, 0.0, 0.0
for ob in peces:
    ob.matrix_world.translation = ob.matrix_world.translation + Vector((dx, dy, dz))
# ESCALAR TOTA LA FIGURA (anant bé: escala des del peu de la figura):
pivot = min(ob.matrix_world.translation.z for ob in peces)
s = 0.5
for ob in peces:
    t = ob.matrix_world.translation
    t.x = (t.x * s); t.y = (t.y * s)
    t.z = pivot + (t.z - pivot) * s
    ob.scale = (ob.scale[0]*s, ob.scale[1]*s, ob.scale[2]*s)
print("Editades %d peces del grup Nan_" % len(peces))
```
- CANVIAR EL COLOR d'una figura/peça (el material ja existeix: retoca'l
  sense crear-ne cap de nou — reaproveu amb correctament el nom):
```python
import bpy
verd = (0.1, 0.75, 0.15, 1.0)
for ob in bpy.data.objects:
    if ob.name.startswith("Nan_"):
        for slot in ob.material_slots:
            m = slot.material
            if m and m.use_nodes:
                b = next((nd for nd in m.node_tree.nodes
                          if nd.type == "BSDF_PRINCIPLED"), None)
                if b:
                    b.inputs["Base Color"].default_value = verd
                    m.name = "Nan_Retocat_Verd"
print("Color canviat a verd per a Nan_")
```
- BORRAR UNA FIGURA SENCERA pel seu PREFIX (per exemple «borra el nan»):
```python
import bpy
esborrats = []
for nom in list(bpy.data.objects.keys()):
    if nom.startswith("Nan_"):
        bpy.data.objects.remove(bpy.data.objects[nom], do_unlink=True)
        esborrats.append(nom)
print("Esborrats: %s" % esborrats)
```
- MODIFICADORS (bevel + subdivision en un objecte JA creat):
```python
import bpy
ob = bpy.data.objects["NoOrbit_Cub"]
if "Bevel" not in ob.modifiers:
    ob.modifiers.new("Bevel", "BEVEL")
    ob.modifiers["Bevel"].width = 0.05
    ob.modifiers.new("Subsurf", "SUBSURF")
    ob.modifiers["Subsurf"].levels = 2
print("Aplicats modificadors a NoOrbit_Cub")
```
- FIGURA COMPLEXA DE VARIES PECES (nan de jardí, personatge, objecte que
demani més d'una forma) — SEMPRE així: MOLTES primitives (una per cada
peça, cada una amb la seva ubicació/escala/rotació i el seu material),
apartades dels objectes existents (p. ex. X = 3.0), alineades entre sí
(mateix eix X, base al terra z=0), i al final selecciona-les i apunta la
vista amb view3d.view_selected + ombrejat MATERIAL per veure'n els colors.
```python
import bpy
def nanmat(n, c, rough=0.6):
    m = bpy.data.materials.new(n); m.use_nodes = True
    b = next(nd for nd in m.node_tree.nodes if nd.type == "BSDF_PRINCIPLED")
    b.inputs["Base Color"].default_value = (*c, 1.0)
    b.inputs["Roughness"].default_value = rough
    return m
M_ROG = nanmat("Nan_Vermell", (0.80, 0.05, 0.03))
M_PELL = nanmat("Nan_Pell", (0.90, 0.70, 0.58))
M_BARBA = nanmat("Nan_Barba", (0.95, 0.95, 0.92), rough=0.85)
M_BOTA = nanmat("Nan_Bota", (0.28, 0.18, 0.10))
M_ULL = nanmat("Nan_Ull", (0.02, 0.02, 0.02), rough=0.3)
X = 3.0
def nanpeça(nom, add, loc, scale=(1,1,1), rot=(0,0,0), material=None):
    add(); ob = bpy.context.view_layer.objects.active; ob.name = nom
    ob.location = loc; ob.rotation_euler = rot; ob.scale = scale
    if material:
        ob.data.materials.clear(); ob.data.materials.append(material)
    return ob
nomop = {  # NOM → (operador bpy.ops.mesh, lloc, escala, rotació, material)
  "Peu_Esq": (lambda: bpy.ops.mesh.primitive_uv_sphere_add(radius=0.18), (X-0.18,0,0.12), (1.4,1.0,0.65), (0,0,0), M_BOTA),
  "Peu_Dte": (lambda: bpy.ops.mesh.primitive_uv_sphere_add(radius=0.18), (X+0.18,0,0.12), (1.4,1.0,0.65), (0,0,0), M_BOTA),
  "Cos": (lambda: bpy.ops.mesh.primitive_cone_add(radius1=0.58, radius2=0.36, depth=1.05, vertices=32), (X,0,0.66), (1,1,1), (0,0,0), M_ROG),
  "Cinto": (lambda: bpy.ops.mesh.primitive_cylinder_add(radius=0.53, depth=0.09, vertices=32), (X,0,0.34), (1,1,1), (0,0,0), M_ROG),
  "Brac_Esq": (lambda: bpy.ops.mesh.primitive_uv_sphere_add(radius=0.12), (X-0.55,0,0.83), (1.9,1.0,1.1), (0,-1.15,0), M_ROG),
  "Brac_Dte": (lambda: bpy.ops.mesh.primitive_uv_sphere_add(radius=0.12), (X+0.55,0,0.83), (1.9,1.0,1.1), (0,1.15,0), M_ROG),
  "Cap": (lambda: bpy.ops.mesh.primitive_uv_sphere_add(radius=0.28), (X,0,1.42), (1,1,1), (0,0,0), M_PELL),
  "Barba": (lambda: bpy.ops.mesh.primitive_uv_sphere_add(radius=0.28), (X,-0.13,1.18), (0.75,0.55,0.95), (0,0,0), M_BARBA),
  "Nas": (lambda: bpy.ops.mesh.primitive_uv_sphere_add(radius=0.07), (X,-0.29,1.33), (1,1,1), (0,0,0), M_PELL),
  "Ull_Esq": (lambda: bpy.ops.mesh.primitive_uv_sphere_add(radius=0.028), (X-0.09,-0.25,1.47), (1,1,1), (0,0,0), M_ULL),
  "Ull_Dte": (lambda: bpy.ops.mesh.primitive_uv_sphere_add(radius=0.028), (X+0.09,-0.25,1.47), (1,1,1), (0,0,0), M_ULL),
  "Gorra": (lambda: bpy.ops.mesh.primitive_cone_add(radius1=0.30, radius2=0.02, depth=0.80, vertices=32), (X,0.02,1.78), (1,1,1), (0.16,0,0), M_ROG),
}
for nom, (op, loc, sc, rot, mat) in nomop.items():
    nanpeça("Nan_" + nom, op, loc, sc, rot, mat)
bpy.ops.object.select_all(action="DESELECT")
for o in bpy.data.objects:
    if o.name.startswith("Nan_"):
        o.select_set(True)
bpy.context.view_layer.objects.active = bpy.data.objects["Nan_Cos"]
for area in bpy.context.screen.areas:
    if area.type == "VIEW_3D":
        region = next(r for r in area.regions if r.type == "WINDOW")
        with bpy.context.temp_override(area=area, region=region):
            bpy.ops.view3d.view_selected()
            bpy.context.space_data.shading.type = "MATERIAL"
print("Nan de jardí creat amb %d peces" % len(nomop))
```
- ANIMAR I FER UN VÍDEO 3D (quan l'usuari demani vídeo, animació o pel·lícula
de la seva escena): 1) anima amb KEYFRAMES les peces de la figura (EL GRUP
sencer, no una peça sola), 2) ajusta l'escena (frame_start/frame_end, fps,
resolució 960x540 perquè el render sigui ràpid) i la càmera mirant l'acció
(si no hi ha càmera: bpy.ops.object.camera_add + scene.camera), 3) desa una
CÒPIA del fitxer (bpy.ops.wm.save_as_mainfile(filepath=ruta, copy=True)) i
4) acaba amb print("NOORBIT_VIDEO_OK:<ruta.blend>") — NO cridis mai
bpy.ops.render.render(animation=True) dins l'editor: bloquejaria Blender.
NoOrbit mira el marcador i renderitza el vídeo ELL SOL (headless) i et
retornarà la ruta del .mp4 en un missatge nou (feina feta).
```python
import bpy, math
sc = bpy.context.scene
sc.frame_start = 1; sc.frame_end = 96; sc.render.fps = 24
peces = [o for o in bpy.data.objects if o.name.startswith("Nan_")]
# Gira el grup complet una volta sencera entre frame 1 i 96:
for ob in peces:
    ob.keyframe_insert(data_path="location", frame=1)
    ob.keyframe_insert(data_path="rotation_euler", frame=1)
    ob.rotation_euler = (0.0, 0.0, math.radians(360))
    ob.keyframe_insert(data_path="rotation_euler", frame=96)
sc.render.resolution_x = 960; sc.render.resolution_y = 540
sc.render.image_settings.file_format = 'FFMPEG'
sc.render.ffmpeg.format = 'MPEG4'
sc.render.ffmpeg.audio_codec = 'AAC'
if sc.camera is None:
    bpy.ops.object.camera_add(location=(4.5, -5.5, 3.2),
        rotation=(1.15, 0.0, 0.75))
    sc.camera = bpy.context.view_layer.objects.active
bpy.ops.wm.save_as_mainfile(filepath="/tmp/noorbit_video.blend", copy=True)
print("NOORBIT_VIDEO_OK:/tmp/noorbit_video.blend")
```
- MÚSICA I SONS (quan l'usuari demani música, so o ambientació): genera el
WAV amb numpy (Blender el porta inclòs) i afegeix-lo a l'EDITOR DE
SEQÜÈNCIES amb sequence_editor_create + sequences.new_sound (Blender 4.x).
```python
import bpy, wave, numpy as np
sr = 22050; dur = 8.0; t = np.arange(int(sr*dur)) / sr
# Progressió d'acords (Do - Fa - Sol - Do), una nota cada 2 s:
freqs = [261.63, 349.23, 392.00, 261.63]
audio = np.zeros_like(t)
for i, f in enumerate(freqs):
    seg = (t >= i*2) & (t < (i+1)*2)
    audio[seg] += 0.30*np.sin(2*np.pi*f*t[seg]) + 0.12*np.sin(2*np.pi*2*f*t[seg])
env = np.minimum(1, np.minimum(t/0.3, (dur-t)/0.3))  # fade in/out
audio *= env
with wave.open("/tmp/noorbit_music.wav", "wb") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(int(sr))
    w.writeframes((np.clip(audio, -1, 1) * 32767).astype(np.int16).tobytes())
sc = bpy.context.scene
se = sc.sequence_editor_create()
se.sequences.new_sound("NoOrbit_Musica", "/tmp/noorbit_music.wav", 1, 1)
sc.frame_end = max(sc.frame_end, int(dur * sc.render.fps))
print("Música de %.0f s afegida a la línia de temps" % dur)
```
- VEU / PARLA (el nan parla!): usa el TTS DEL SISTEMA (no cal res extern).
macOS: `say` + `afconvert` a WAV; Linux: `espeak-ng -w`; Windows: PowerShell
System.Speech. Després afegeix la veu com a so de seqüència.
```python
import bpy, subprocess, platform, os
text = "Hola! Soc el nan del teu jardí vermell."
out = "/tmp/noorbit_veu"
sist = platform.system()
if sist == "Darwin":
    subprocess.run(["say", "-o", out + ".aiff", text], check=True)
    subprocess.run(["afconvert", "-f", "WAVE", "-d", "LEI16@22050",
                    out + ".aiff", out + ".wav"], check=True)
    os.remove(out + ".aiff")
elif sist == "Linux":
    subprocess.run(["espeak-ng", "-w", out + ".wav", text], check=True)
else:
    subprocess.run(["powershell", "-Command",
        "Add-Type -AssemblyName System.Speech; "
        "$s = New-Object System.Speech.Synthesis.SpeechSynthesizer; "
        "$s.SetOutputToWaveFile('%s.wav'); $s.Speak('%s'); $s.Dispose()"
        % (out, text)], check=True)
sc = bpy.context.scene
se = sc.sequence_editor_create()
veu = se.sequences.new_sound("NoOrbit_Veu", out + ".wav", 2, 1)
dur = veu.frame_final_duration
sc.frame_end = max(sc.frame_end, dur)
print("Veu afegida (%d frames): «%s»" % (dur, text))
```
- PARLAR FLUID (boca animada mentre sona la veu): crea una boca petita a la
cara i anima la seva escala amb keyframes cada pocs frames durant TOTA la
durada de la veu (aleatòria = parlant; no sincronia per fonema, és fluida).
```python
import bpy, random
boca = bpy.data.objects.get("Nano_Boca")
if boca is None:
    bpy.ops.mesh.primitive_uv_sphere_add(radius=0.05,
        segments=16, ring_count=12,
        location=(3.0, -0.27, 1.24))  # sota el nas (adapta-ho al cap)
    boca = bpy.context.view_layer.objects.active
    boca.name = "Nano_Boca"
veu = next((s for s in bpy.context.scene.sequence_editor.sequences
            if s.name.startswith("NoOrbit_Veu")), None)
dur = int(veu.frame_final_duration) if veu else 96
f = 1
while f < dur:
    boca.scale = (1.0, 0.4 + random.random()*0.8, 0.6)
    boca.keyframe_insert("scale", frame=f)
    f += random.randint(3, 7)
boca.scale = (1.0, 0.4, 0.6)
boca.keyframe_insert("scale", frame=dur)
print("Boca animada parlant durant %d frames" % dur)
```
- ÀUDIO AL VÍDEO: SI l'escena té sons de seqüència (música/veu), el render
amb FFMPEG els inclou NOMÉS si scene.render.ffmpeg.audio_codec = 'AAC' —
posa-ho SEMPRE quan hi hagi àudio (a la recepta de vídeo, abans de desar).
REGLS DE FIGURES (fonts principals d'error):
- Primitius DISPONIBLES (Blender 4.x): primitive_uv_sphere_add,
  primitive_cone_add, primitive_cylinder_add, primitive_torus_add,
  primitive_plane_add, primitive_cube_add, primitive_circle_add,
  primitive_grid_add. ⚠️ primitive_capsule_add NO sempre existeix: no la
  fagis servir (usa uv_sphere, escalem (1.9,1,1) per braços o pals).
- Materials: agafa el node BSDF per TIPUS (nd.type == "BSDF_PRINCIPLED"),
  MAI per nom (el node «Principled BSDF» pot no existir).
- Si demanen un personatge/objecte real, MAI lliuris una única primitiva:
  «una esfera sobre un con» NO és un nan. Compon peça a peça (barba, gorra,
  nas, botes…) i no diguis que està acabat sense les peces i la vista.
- Al final, SEMPRE: selecciona les peces, view3d.view_selected i
  shading.type = "MATERIAL", perquè l'usuari vegi el color real.
REGLS PER QUE NO FALLI (errors típics a Blender):
- Després de crear un objecte amb un operator, NO depengues de bpy.context.active_object;
  agafa'l amb ob = bpy.context.view_layer.objects.active o crea'l amb bpy.data.objects.new +
  bpy.context.collection.objects.link(ob).
- Abans de bpy.data.objects["Nom"], comprova que existeix: if "Nom" in bpy.data.objects: ...
- No baralles text fora de l'script. UN sol bloc ```python.
Sense cap text fora del bloc de codi."#;
        // ABANS de generar res: comprovem que Blender és obert i el pont
        // escolta al port. Si no hi ha connexió, NO malgastem dues generacions
        // de l'IA en un script que no es podrà executar (aquest era el bucle
        // confús «script ha fallat; la IA el corregeix…» repetint-se).
        progress(app, "blender", "Comprovant la connexió amb Blender…", true);
        let connected = tauri::async_runtime::spawn_blocking(|| {
            crate::blender::BlenderSocketClient::new(crate::blender::DEFAULT_SOCKET_PORT).ping()
        })
        .await
        .unwrap_or(false);
        if !connected {
            // Si l'usuari ha donat accés a l'ordinador, NoOrbit ho fa tot sol:
            // arrenca Blender amb el pont (`blender -P noorbit_bridge.py`) i
            // espera que el port 9876 escolti, per a després CONTINUAR amb la
            // tasca demanada — sense tornar-la a demanar l'usuari.
            let comp_access = app.state::<crate::AppState>().computer.permissions().enabled;
            if !comp_access {
                progress(app, "blender", "Blender no connectat", false);
                return Ok((
                    "❌ No puc aplicar a Blender perquè **no està connectat** al pont \
(el problema NO és l'script sinó la connexió).\n\n\
Puc obrir-lo i connectar-lo JO SOL si em dones accés: activa\n\
«**Permet que la IA controli l'ordinador**» al panell **Ordinador** i torna-ho a demanar.\n\n\
O fes-ho a mà: obre Blender i prem **«Arrenca Blender»** al panell Blender \
(o activa l'add-on **NoOrbit Bridge**: barra lateral N > NoOrbit > Inicia).".to_string(),
                    false,
                ));
            }
            progress(
                app,
                "blender",
                "No connectat — tinc accés a l'ordinador: obrint Blender amb el pont…",
                true,
            );
            let app2 = app.clone();
            let launched: std::result::Result<String, String> =
                match tauri::async_runtime::spawn_blocking(move || {
                    crate::blender::launch_with_bridge(&app2)
                })
                .await
                {
                    Ok(Ok(msg)) => Ok(msg),
                    Ok(Err(e)) => Err(e.to_string()),
                    Err(e) => Err(format!("El procés d'arrencada ha fallat: {}", e)),
                };
            // «launch_with_bridge» ja espera ~30 s el port; re-pingue una estona
            // més per si l'add-on triga a despertar (arrencada lenta en CPU).
            let bridge_up = launched.is_ok()
                && tauri::async_runtime::spawn_blocking(|| {
                    let c = crate::blender::BlenderSocketClient::new(
                        crate::blender::DEFAULT_SOCKET_PORT,
                    );
                    for _ in 0..40 {
                        if c.ping() {
                            return true;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    c.ping()
                })
                .await
                .unwrap_or(false);
            if !bridge_up {
                progress(app, "blender", "No s'ha pogut connectar Blender", false);
                let motiu = match &launched {
                    Ok(_) => "Blender s'ha arrencat però el pont no respon al port 9876; \
 possiblement cal activar l'add-on NoOrbit Bridge dins Blender (panell N > Inicia)."
                        .to_string(),
                    Err(e) => e.clone(),
                };
                return Ok((
                    format!(
                        "❌ He intentat obrir i connectar Blender automàticament però no \
ho he aconseguit: {}\n\nQuan aparegui «Connectat» al panell Blender, torna-ho a \
demanar i ho aplicaré.",
                        motiu
                    ),
                    false,
                ));
            }
            progress(app, "blender", "✅ Blender obert i connectat — continuo", true);
        }
        progress(app, "ia", "La IA està escrivint l'script per a Blender…", true);
        let reply = ai.chat(prompt, Some(system)).await?;
        let code = Self::extract_code(&reply);
        // Guarda: si la IA no ha generat codi Python (resposta en prosa,
        // sense bloc ```python), no ho executem — evita el SyntaxError a Blender.
        if let Err(e) = crate::blender::guard_python(&code) {
            progress(app, "blender", "La IA no ha generat codi Python", false);
            return Ok((
                format!("❓ No puc aplicar a Blender: {}\n\nResposta de la IA:\n{}", e, reply.trim()),
                false,
            ));
        }
        progress(app, "ia", "Script generat", true);
        // L'execució és blocant (socket): va al thread blocking.
        progress(app, "blender", "Executant l'script a Blender…", true);
        // Comptem objectes ABANS per poder informar del canvi real a l'escena.
        let before = tauri::async_runtime::spawn_blocking(|| {
            crate::blender::BlenderSocketClient::new(crate::blender::DEFAULT_SOCKET_PORT)
                .info()
                .ok()
                .and_then(|i| i.objects)
        })
        .await
        .unwrap_or(None);
        let out = tauri::async_runtime::spawn_blocking(move || {
            crate::blender::BlenderSocketClient::new(crate::blender::DEFAULT_SOCKET_PORT)
                .execute_auto(&code)
        })
        .await
        .map_err(|e| anyhow!("{}", e))?;

        match out {
            Ok(stdout) => {
                let after = tauri::async_runtime::spawn_blocking(|| {
                    crate::blender::BlenderSocketClient::new(crate::blender::DEFAULT_SOCKET_PORT)
                        .info()
                        .ok()
                        .and_then(|i| i.objects)
                })
                .await
                .unwrap_or(None);
                progress(app, "blender", "Canvis aplicats a Blender", true);
                let delta = Self::count_delta(before, after);
                let base = format!("✅ Blender: canvis aplicats.\n\n{}{}", stdout, delta)
                    .trim_end()
                    .to_string();
                Ok((Self::maybe_render_video(app, &stdout, base).await, true))
            }
            Err(e) => {
                progress(app, "blender", "L'script ha fallat; la IA el corregeix…", false);
                // Reintent: li passem l'error perquè corregisqa el script.
                let retry_prompt = format!(
                    "L'script anterior ha fallat amb aquest error:\n{}\n\nTasca original:\n{}\n\nReescriu NOMÉS l'script corregit.",
                    e, prompt
                );
                let reply2 = ai.chat(&retry_prompt, Some(system)).await?;
                let code2 = Self::extract_code(&reply2);
                if let Err(e) = crate::blender::guard_python(&code2) {
                    progress(app, "blender", "El 2n intent tampoc és codi Python", false);
                    return Ok((
                        format!(
                            "❓ El 2n intent tampoc ha generat codi Python: {}",
                            e
                        ),
                        false,
                    ));
                }
                let code2_for_msg = code2.clone();
                let out2 = tauri::async_runtime::spawn_blocking(move || {
                    crate::blender::BlenderSocketClient::new(crate::blender::DEFAULT_SOCKET_PORT)
                        .execute_auto(&code2)
                })
                .await
                .map_err(|e| anyhow!("{}", e))?;
                match out2 {
                    Ok(stdout) => {
                        let after = tauri::async_runtime::spawn_blocking(|| {
                            crate::blender::BlenderSocketClient::new(
                                crate::blender::DEFAULT_SOCKET_PORT,
                            )
                            .info()
                            .ok()
                            .and_then(|i| i.objects)
                        })
                        .await
                        .unwrap_or(None);
                        progress(app, "blender", "Canvis aplicats (2n intent)", true);
                        let delta = Self::count_delta(before, after);
                        let base = format!(
                            "✅ Blender (2n intent): canvis aplicats.\n\n{}{}",
                            stdout, delta
                        )
                        .trim_end()
                        .to_string();
                        Ok((Self::maybe_render_video(app, &stdout, base).await, true))
                    }
                    Err(e2) => {
                        progress(app, "blender", "Blender ha fallat", false);
                        Ok((
                            format!(
                                "❌ L'script ha fallat dos cops. Blender ha retornat:\n{}\n\n\
Aquest és l'últim script generat (pots provar d'executar-lo a mà a Blender > \
Scripting > Console):\n```python\n{}\n```",
                                e2, code2_for_msg
                            ),
                            false,
                        ))
                    }
                }
            }
        }
    }

    /// Igual que `apply_blender` però per a Unreal Engine (Remote Control /
    /// Python). El text de l'usuari pot ser l'objecte a editar i la
    /// modificació a fer-hi.
    pub async fn apply_unreal(
        &self,
        ai: &AiManager,
        app: &tauri::AppHandle,
        prompt: &str,
        config: &crate::unreal::commands::UnrealConfig,
    ) -> Result<(String, bool)> {
        let system = "ETS L'AGENT DE NOORBIT per a UNREAL ENGINE 5. L'usuari et dona un \
texte i t'explica què hi ha de fer (crear o moure actors, editar un text/títol, \
importar, il·luminar…). Respon NOMÉS amb un script Python per a Unreal Editor \
(un bloc ```python) usant `unreal` (Editor Scripting). Sense explicacions.";
        progress(app, "ia", "La IA està escrivint l'script per a Unreal…", true);
        let reply = ai.chat(prompt, Some(system)).await?;
        let code = Self::extract_code(&reply);
        progress(app, "ia", "Script generat", true);
        let client = crate::unreal::UnrealClient::with_port(config.port);
        progress(app, "unreal", "Executant l'script a Unreal…", true);
        let result = crate::unreal::python::run_script(&client, &code)
            .await
            .map_err(|e| anyhow!("{}", e))?;
        if result.success {
            progress(app, "unreal", "Canvis aplicats a Unreal", true);
            let out = result.output.join("\n");
            Ok((format!("✅ Unreal: canvis aplicats.\n\n{}", out).trim_end().to_string(), true))
        } else {
            progress(app, "unreal", "L'script ha fallat; la IA el corregeix…", false);
            let errs = result.errors.join("\n");
            // Reintent amb l'error.
            let retry_prompt = format!(
                "L'script anterior ha fallat:\n{}\n\nTasca original:\n{}\n\nReescriu NOMÉS l'script corregit.",
                errs, prompt
            );
            let reply2 = ai.chat(&retry_prompt, Some(system)).await?;
            let code2 = Self::extract_code(&reply2);
            let result2 = crate::unreal::python::run_script(&client, &code2)
                .await
                .map_err(|e| anyhow!("{}", e))?;
            if result2.success {
                progress(app, "unreal", "Canvis aplicats (2n intent)", true);
                let out = result2.output.join("\n");
                Ok((
                    format!("✅ Unreal (2n intent): canvis aplicats.\n\n{}", out)
                        .trim_end()
                        .to_string(),
                    true,
                ))
            } else {
                progress(app, "unreal", "Unreal ha fallat", false);
                Ok((
                    format!("❌ Unreal ha fallat:\n{}", result2.errors.join("\n")),
                    false,
                ))
            }
        }
    }
}

// ── Pressupost de temps i degradació a segon pla (Objectiu 4) ─────────────

use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Mutex as StdMutex;

/// Nombre de tasques actualment en segon pla. Serveix per baixar la
/// prioritat del procés només mentre n'hi ha alguna i restaurar-la quan
/// toutes acaben (evita afectar tot NoOrbit de manera permanent).
static BG_ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// Tasques que l'usuari ha demanat tornar a primer pla (des del indicador
/// de la barra d'estat). `run_with_budget` les consulta a cada poll.
static FOREGROUND_REQUESTS: std::sync::LazyLock<StdMutex<HashMap<String, ()>>> =
    std::sync::LazyLock::new(|| StdMutex::new(HashMap::new()));

/// Demana des de la UI que una tasca en segon pla torne a primer pla.
pub fn request_foreground(task_id: &str) {
    if let Ok(mut m) = FOREGROUND_REQUESTS.lock() {
        m.insert(task_id.to_string(), ());
    }
}

/// Consumeix una petició de tornar a primer pla (true si n'hi havia una).
fn take_foreground_request(task_id: &str) -> bool {
    FOREGROUND_REQUESTS
        .lock()
        .map(|mut m| m.remove(task_id).is_some())
        .unwrap_or(false)
}

/// Ajusta la prioritat (nice) del procés. Unix: nice 10 en segon pla
/// (cedix CPU a la resta del sistema), nice 0 per restaurar. Altres SO:
/// no-op (la reducció real ja ve del mode eco d'Ollama).
fn adjust_priority(background: bool) {
    #[cfg(unix)]
    unsafe {
        let nice = if background { 10 } else { 0 };
        // id = 0 → el procés cridant.
        libc::setpriority(libc::PRIO_PROCESS, 0, nice);
    }
    #[cfg(not(unix))]
    let _ = background;
}

/// Endollat per bateria (mode eco addicional): redueix encara més els polls
/// quan el portàtil no és al corrent, per estalviar energia.
fn on_battery() -> bool {
    #[cfg(target_os = "macos")]
    {
        return std::process::Command::new("pmset")
            .args(["-g", "batt"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("Battery Power"))
            .unwrap_or(false);
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/sys/class/power_supply/AC/online") {
            return s.trim() == "0";
        }
        return false;
    }
    #[cfg(windows)]
    {
        false
    }
}

/// Pressupost de temps d'una tasca: llindar de primer pla, límit total i
/// freqüència de poll en segon pla.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBudget {
    pub task_id: String,
    pub foreground_limit_ms: u64,
    pub total_limit_ms: u64,
    pub background_poll_ms: u64,
}

impl Default for TaskBudget {
    fn default() -> Self {
        Self {
            task_id: uuid::Uuid::new_v4().to_string(),
            foreground_limit_ms: 60_000,
            total_limit_ms: 3_600_000,
            background_poll_ms: 2_000,
        }
    }
}

impl TaskBudget {
    /// Construeix el pressupost a partir de la configuració de l'usuari.
    pub fn from_limits(foreground_limit_s: u64, total_limit_s: u64) -> Self {
        Self {
            task_id: uuid::Uuid::new_v4().to_string(),
            foreground_limit_ms: foreground_limit_s.max(1) * 1000,
            total_limit_ms: total_limit_s * 1000,
            background_poll_ms: 2_000,
        }
    }
}
