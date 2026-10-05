//! Navegador web intern de NoOrbit, controlable per la IA.
//!
//! Quan una pàgina necessita JavaScript, sessió visual o una acció humana
//! (iniciar sessió amb el SEU compte, acceptar galetes, resoldre un captcha),
//! la IA obre una finestra webview real de l'aplicació: l'usuari veu i
//! controla la pàgina, i la IA pot llegir-ne el text ja renderitzat i
//! executar-hi JavaScript. Tot amb contingut públic o amb els credencials
//! que l'usuari introdueix ell mateix — mai se supla identitat ni s'evadeix
//! cap protecció: si la pàgina demana una acció humana, la finestra es fa
//! visible perquè la faci la persona.
//!
//! Mecànica d'extracció (vàlida per a qualsevol origen): la finestra és una
//! `WebviewWindow` de Tauri; per llegir-ne el contingut s'hi injecta amb
//! `eval()` un script que copia `document.body.innerText` al TÍTOL del
//! document (propietat accessible des de qualsevol pàgina) amb un codi únic
//! de seguretat, i el backend llegeix el títol amb `window.title()` fins que
//! apareix el marcador. No cal cap permís del lloc web ni cookie compartida.

pub mod tools;

use anyhow::{anyhow, Result};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Etiqueta única de la finestra del navegador.
pub const LABEL: &str = "browser";

const BASE_TITLE: &str = "NoOrbit — Navegador";

/// Força que només es carreguin llocs http/https: cap pàgina remota pot
/// obrir fitxers locals ni esquemes especials a través d'aquesta finestra.
fn safe_http(url: &str) -> Result<tauri::Url> {
    let parsed: tauri::Url = url
        .trim()
        .parse()
        .map_err(|_| anyhow!("URL no vàlida: {}", url))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        other => Err(anyhow!("Només es permeten URLs http/https (rebut «{}:»)", other)),
    }
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

/// Obre (o reutilitza) la finestra del navegador amb la URL donada.
/// `hidden = true` l'obre en segon pla: la IA la pot llegir sense que
/// la persona la veja fins que calgui una acció.
pub fn open(app: &AppHandle, url: &str, hidden: bool) -> Result<()> {
    let target = safe_http(url)?;
    if let Some(win) = window(app) {
        win.navigate(target)?;
        if !hidden {
            win.set_focus().ok();
        }
        return Ok(());
    }
    let _win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(target))
        .title(BASE_TITLE)
        .inner_size(1080.0, 760.0)
        .visible(!hidden)
        .focused(!hidden)
        // Bloqueja qualsevol navegació sortida que no siga http/https.
        .on_navigation(|u| matches!(u.scheme(), "http" | "https"))
        .build()
        .map_err(|e| anyhow!("No s'ha pogut obrir el navegador intern: {}", e))?;
    Ok(())
}

/// Navega la finestra existent (o l'obre si encara no n'hi ha).
pub fn navigate(app: &AppHandle, url: &str) -> Result<()> {
    let target = safe_http(url)?;
    match window(app) {
        Some(win) => win.navigate(target).map_err(|e| anyhow!("{}", e)),
        None => {
            let _win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(target))
                .title(BASE_TITLE)
                .inner_size(1080.0, 760.0)
                .on_navigation(|u| matches!(u.scheme(), "http" | "https"))
                .build()
                .map_err(|e| anyhow!("{}", e))?;
            Ok(())
        }
    }
}

pub fn set_visible(app: &AppHandle, show: bool) -> Result<()> {
    let win = window(app).ok_or_else(|| anyhow!("El navegador intern no està obert"))?;
    if show {
        win.show().ok();
        win.set_focus().ok();
    } else {
        win.hide().ok();
    }
    Ok(())
}

pub fn close(app: &AppHandle) -> Result<()> {
    match window(app) {
        Some(win) => win.close().map_err(|e| anyhow!("{}", e)),
        None => Ok(()),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserStatus {
    pub open: bool,
    pub visible: bool,
    pub url: String,
    pub title: String,
}

/// Estat de la finestra: oberta, visible, URL i títol actuals.
pub fn status(app: &AppHandle) -> BrowserStatus {
    match window(app) {
        Some(win) => BrowserStatus {
            open: true,
            visible: win.is_visible().unwrap_or(false),
            url: win.url().map(|u| u.to_string()).unwrap_or_default(),
            title: win.title().unwrap_or_default(),
        },
        None => BrowserStatus {
            open: false,
            visible: false,
            url: String::new(),
            title: String::new(),
        },
    }
}

/// Executa JavaScript a la pàgina carregada (la IA l'usa per interactuar:
/// omplir formularis, fer clic, desplaçar…). S'executa DINS de la finestra
/// del navegador, que no té cap permís IPC sobre l'aplicació (les seues
/// capacitats només cobreixen la finestra «main») i només carrega llocs
/// http/https: l'script no pot tocar fitxers ni comandes de NoOrbit.
pub fn eval_js(app: &AppHandle, js: &str) -> Result<()> {
    let win = window(app).ok_or_else(|| anyhow!("El navegador intern no està obert"))?;
    win.eval(js).map_err(|e| anyhow!("JavaScript rebutjat pel navegador: {}", e))
}

/// Text ja renderitzat d'una pàgina, llegit a través del títol del document.
#[derive(Debug, Clone, Serialize)]
pub struct PageExtract {
    pub url: String,
    pub text: String,
}

/// Detecta contingut que requereix una acció HUMANA visible (sessió,
/// captcha, consentiment). En aquests casos la finestra es fa visible perquè
/// la persona actuï: la IA no intenta esquivar-ho ni fingir la sessió.
pub fn needs_human_action(text: &str) -> bool {
    let low = text.to_lowercase();
    [
        "captcha",
        "verify you are human",
        "are you a robot",
        "confirm you are human",
        "security check",
        "log in to continue",
        "sign in to continue",
        "please sign in",
        "please log in",
    ]
    .iter()
    .any(|m| low.contains(m))
}

// ── Consulta manual a IAs web (el camí sense token) ────────────────────────────────
//
// Quan la IA local no té prou potència i l'usuari NO cap token d'API, el
// servei es fa com el faria qualsevol persona: s'obre el xat WEB oficial de
// la IA triada, l'usuari hi inicia sessió amb el SEU compte, la IA escriu la
// pregunta a la caixa d'entrada d'aquell xat (davant de l'usuari, que ho veu
// i pot corregir-ho) i després LLEGEIX la resposta que la pàgina ja mostra
// en pantalla. Cap cookie, cap token, cap sessió suplantada: només el que
// la persona veuria llegint el seu propi navegador.

/// IA web consultable amb el compte de l'usuari (URLs oficials dels xats).
#[derive(Debug, Clone, Serialize)]
pub struct WebIa {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
}

pub const WEB_IAS: &[WebIa] = &[
    WebIa { id: "deepseek", name: "DeepSeek", url: "https://chat.deepseek.com/" },
    WebIa { id: "chatgpt", name: "ChatGPT (OpenAI)", url: "https://chatgpt.com/" },
    WebIa { id: "claude", name: "Claude", url: "https://claude.ai/new" },
    WebIa { id: "gemini", name: "Gemini (Google)", url: "https://gemini.google.com/app" },
    WebIa { id: "perplexity", name: "Perplexity", url: "https://www.perplexity.ai/" },
    WebIa { id: "grok", name: "Grok (xAI)", url: "https://grok.com/" },
];

/// Troba la IA pel nom o id («deepseek», «ChatGPT», «openai», «google»…).
pub fn find_ia(name: &str) -> Option<&'static WebIa> {
    let n = name.to_lowercase();
    WEB_IAS.iter().find(|w| {
        w.id == n
            || w.name.to_lowercase().contains(&n)
            || n.contains(w.id)
            || match w.id {
                "chatgpt" => n.contains("openai") || n == "gpt",
                "gemini" => n.contains("google"),
                "grok" => n.contains("xai") || n.contains("x.com"),
                _ => false,
            }
    })
}

/// Estat de la pàgina del xat web: hi ha caixa d'escriptura i cal sessió?
#[derive(Debug, Clone, Serialize)]
pub struct ChatProbe {
    /// Hi ha una caixa d'entrada visible (pàgina preparada per preguntar).
    pub has_input: bool,
    /// La pàgina reclama iniciar sessió / crear compte / captcha.
    pub login_needed: bool,
    pub url: String,
}

/// Consulta l'estat de la pàgina actual amb un script auxiliar que escriu el
/// resultat al títol del document (mateix mecanisme que `extract_text`).
pub async fn chat_probe(app: &AppHandle) -> Result<ChatProbe> {
    let win = window(app).ok_or_else(|| anyhow!("El navegador intern no està obert"))?;
    let nonce = chrono::Utc::now().timestamp_millis();
    let marker = format!("NBCB#{}#", nonce);
    let js = format!(
        r#"(function(){{try{{var t=(document.body&&document.body.innerText)?document.body.innerText.slice(0,9000):'';var need=/(captcha|verify you are human|are you a robot|log in|sign in|create an account|get started|continue with|inicia sess|inicia sesi|registrat|regístrate)/i.test(t);var ok=false;try{{var els=document.querySelectorAll('textarea,[contenteditable="true"],[role="textbox"]');for(var i=0;i<els.length;i++){{var x=els[i];if(!x.disabled&&(x.offsetWidth>0||x.offsetHeight>0)){{ok=true;break;}}}}}}catch(e){{}}document.title='{marker}'+(ok?'I':'-')+(need?'L':'-');}}catch(e){{document.title='{marker}ERR';}}}})();"#,
        marker = marker
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        win.eval(&js)
            .map_err(|e| anyhow!("No s'ha pogut sondejar la pàgina: {}", e))?;
        tokio::time::sleep(Duration::from_millis(350)).await;
        let title = win.title().unwrap_or_default();
        if let Some(rest) = title.strip_prefix(&marker) {
            win.set_title(BASE_TITLE).ok();
            let url = win.url().map(|u| u.to_string()).unwrap_or_default();
            if rest.starts_with("ERR") {
                return Err(anyhow!("La pàgina ha bloquejat el sondeig"));
            }
            return Ok(ChatProbe {
                has_input: rest.starts_with('I'),
                login_needed: rest.ends_with('L'),
                url,
            });
        }
        if tokio::time::Instant::now() >= deadline {
            win.set_title(BASE_TITLE).ok();
            return Err(anyhow!(
                "La pàgina encara no ha carregat (sondeig sense resposta)"
            ));
        }
    }
}

/// Part final d'un text, per comparar l'estabilitat de la resposta sense
/// dependre de tota la pàgina (capçaleres, barra lateral, suggeriments…).
fn tail_of(text: &str, chars: usize) -> String {
    let v: Vec<char> = text.chars().collect();
    let start = v.len().saturating_sub(chars);
    v[start..].iter().collect()
}

/// Bloc de codi llegit d'un xat web: els llocs pinten el codi en elements
/// propis (amb el llenguatge a la classe), i «innerText» sol eixugar-ne la
/// estructura. Ací es recol·lecten SEPARATS perquè la IA local els puga
/// integrar al projecte, no només llegir-los en prosa.
#[derive(Debug, Clone, Serialize)]
pub struct CodeBlock {
    pub lang: String,
    pub code: String,
}

/// Arrenca de la pàgina actual els blocs de codi visible (fins a 12, els de
/// la resposta més recent). Mateix mecanisme que
/// «extract_text»: l'script escriu el resultat (JSON) al TÍTOL del document
/// rere un marcador únic, i el backend llegix el títol fins que apareix.
pub async fn grab_code_blocks(app: &AppHandle) -> Result<Vec<CodeBlock>> {
    let win = window(app).ok_or_else(|| anyhow!("El navegador intern no està obert"))?;
    let nonce = chrono::Utc::now().timestamp_millis();
    let marker = format!("NBCODE#{}#", nonce);
    let js = format!(
        r#"(function(){{try{{
var els=document.querySelectorAll('pre code');
if(!els.length)els=document.querySelectorAll('pre');
var out=[];var seen={{}};
for(var i=0;i<els.length&&out.length<12;i++){{
  var c=els[i];
  var t=(c.innerText||'').replace(/\s+$/,'');
  if(t.length<4)continue;
  var key=t.slice(0,180);
  if(seen[key])continue;seen[key]=1;
  var lang='';
  try{{
    var chain=[c,c.parentElement,c.firstElementChild];
    for(var k=0;k<chain.length&&!lang;k++){{
      var el=chain[k];if(!el)continue;
      var m=String(el.className||'').match(/language-([\w+#.-]+)/i);if(m){{lang=m[1];continue;}}
      var d=el.getAttribute&&el.getAttribute('data-lang');if(d){{lang=d;continue;}}
      var dl=el.closest&&el.closest('[data-lang]');if(dl){{lang=dl.getAttribute('data-lang');}}
    }}
  }}catch(e){{}}
  out.push(lang+':::'+t.slice(0,14000));
}}
document.title='{marker}'+encodeURIComponent(JSON.stringify(out));
}}catch(e){{document.title='{marker}ERR';}}}})();"#,
        marker = marker
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        win.eval(&js)
            .map_err(|e| anyhow!("No s'han pogut arreplegar els blocs de codi: {}", e))?;
        tokio::time::sleep(Duration::from_millis(350)).await;
        let title = win.title().unwrap_or_default();
        if let Some(rest) = title.strip_prefix(&marker) {
            win.set_title(BASE_TITLE).ok();
            if rest.starts_with("ERR") {
                return Err(anyhow!("La pàgina ha bloquejat l'extracció de codi"));
            }
            let json = urlencoding::decode(rest).map(|c| c.to_string()).unwrap_or(rest.to_string());
            let raw: Vec<String> = serde_json::from_str(&json).unwrap_or_default();
            return Ok(raw
                .iter()
                .map(|s| match s.split_once(":::") {
                    Some((lang, code)) => CodeBlock {
                        lang: lang.trim().to_string(),
                        code: code.to_string(),
                    },
                    None => CodeBlock {
                        lang: String::new(),
                        code: s.clone(),
                    },
                })
                .collect());
        }
        if tokio::time::Instant::now() >= deadline {
            win.set_title(BASE_TITLE).ok();
            return Err(anyhow!(
                "La pàgina no ha respost a l'extracció de codi (potser encara carrega)"
            ));
        }
    }
}

/// Escriu la pregunta a la caixa d'entrada del xat web obert (com si
/// l'usuari la teclés: inserir text + esdeveniment «input» + clic al botó
/// d'enviar o Intro) i espera que la resposta de la pàgina s'estabilitzi,
/// llegint-la pel text visible en pantalla. No toca galetes ni tokens.
pub async fn chat_ask(app: &AppHandle, question: &str, timeout: Duration) -> Result<String> {
    let win = window(app).ok_or_else(|| anyhow!("El navegador intern no està obert"))?;
    // 1) Fotografia de referencia: la cua del text actual (abans de preguntar).
    let base = extract_text(app, Duration::from_secs(8))
        .await
        .map(|e| tail_of(&e.text, 2000))
        .unwrap_or_default();

    // 2) Injecció de la pregunta. La string va escapada per serde_json, així
    //    que qualsevol cometa o salt de línia queda fora de l'abast de l'script.
    let q = serde_json::to_string(question).unwrap_or_else(|_| "\"\"".into());
    let js = format!(
        r#"(function(){{try{{
var p={q};
var els=[].slice.call(document.querySelectorAll('textarea,[contenteditable="true"],[role="textbox"],input[type="text"]));
els=els.filter(function(x){{return !x.disabled&&(x.offsetWidth>0||x.offsetHeight>0);}});
if(!els.length){{document.title='NBQ#NOINPUT';return;}}
var e=els[els.length-1];
e.focus();
var ok=false;try{{ok=document.execCommand('insertText',false,p);}}catch(x){{}}
var val=(e.value!==undefined?e.value:e.textContent)||'';
if(!ok||val.indexOf(p)<0){{
  if(typeof e.value==='string'){{
    var proto=Object.getPrototypeOf(e);
    var d=Object.getOwnPropertyDescriptor(proto,'value')||Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype,'value');
    if(d&&d.set){{d.set.call(e,p);}}else{{e.value=p;}}
  }}else{{e.textContent=p;}}
  e.dispatchEvent(new InputEvent('input',{{bubbles:true,cancelable:true,inputType:'insertText',data:p}}));
}}
setTimeout(function(){{try{{
  var btns=[].slice.call(document.querySelectorAll('button,[role="button"]')).filter(function(x){{
    var l=((x.getAttribute('aria-label')||'')+' '+(x.title||'')+' '+(x.textContent||'')).toLowerCase();
    return /(send|enviar|submit|pregunta|ask)/.test(l)&&!/stop|atura|cancel|cancela/.test(l)&&!x.disabled;
  }});
  if(btns.length){{btns[btns.length-1].click();}}
  else{{['keydown','keypress','keyup'].forEach(function(tp){{e.dispatchEvent(new KeyboardEvent(tp,{{bubbles:true,cancelable:true,key:'Enter',code:'Enter',keyCode:13,which:13}}));}});}}
  document.title='NBQ#SENT';
}}catch(x){{document.title='NBQ#SENDFAIL';}}}},250);
}}catch(x){{document.title='NBQ#ERR';}}}})();"#,
        q = q
    );
    win.eval(&js)
        .map_err(|e| anyhow!("L'entrada del xat web no ha acceptat la pregunta: {}", e))?;
    // Comprova el marcador: si no hi ha caixa d'entrada, la pàgina encara no
    // està preparada (potser falta acabar d'iniciar sessió).
    let inject_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let title = win.title().unwrap_or_default();
        if title.contains("NBQ#NOINPUT") || title.contains("NBQ#ERR") {
            return Err(anyhow!(
                "La pàgina no mostra (encara) la caixa d'escriure del xat: potser \
                 cal iniciar sessió o carregar-la. Fes-la visible i acaba-ho tu."
            ));
        }
        if title.contains("NBQ#SENT") || title.contains("NBQ#SENDFAIL") {
            break;
        }
        if tokio::time::Instant::now() >= inject_deadline {
            break; // algunes pàgines bloquegen el títol: igualment esperarem la resposta
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    // 3) Espera la resposta: el text de la pàgina deixa de créixer durant uns
    //    segons i és diferent del de partida (generar triga; això és el més
    //    honest que es pot fer sense accedir a cap mecanisme intern del lloc).
    let start = tokio::time::Instant::now();
    let mut last = base.clone();
    let mut stable_from: Option<tokio::time::Instant> = None;
    loop {
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let now = tokio::time::Instant::now();
        if now - start > timeout {
            return Err(anyhow!(
                "El xat web no ha arribat a una resposta estable en {} s. El text \
                 de la pantalla és ara: {}",
                timeout.as_secs(),
                tail_of(&last, 800)
            ));
        }
        let ex = match extract_text(app, Duration::from_secs(6)).await {
            Ok(e) => e,
            Err(_) => continue,
        };
        let cur = tail_of(&ex.text, 2000);
        if cur == last {
            // Estable respecte a l'última lectura: si a més és nou respecte a
            // la fotografia inicial i fa estona que no canvia, és la resposta.
            let since = *stable_from.get_or_insert(now);
            if cur != base && cur.chars().count() > 40 && now - since >= Duration::from_secs(5) {
                return Ok(ex.text);
            }
        } else {
            stable_from = None;
            last = cur;
        }
    }
}

/// Llegeix el text renderitzat de la pàgina actual. Reinjecta l'script cada
/// mig segon fins que el títol torna amb el marcador, o fins que expira
/// `wait` (les pàgines amb JavaScript triguen a pintar). És asíncron: no
/// bloqueja el runtime de Tauri mentre espera.
pub async fn extract_text(app: &AppHandle, wait: Duration) -> Result<PageExtract> {
    let win = window(app).ok_or_else(|| anyhow!("El navegador intern no està obert"))?;
    let nonce = chrono::Utc::now().timestamp_millis();
    let marker = format!("NBEXT#{}#", nonce);
    // L'script copia al títol (accessible des de qualsevol origen) la URL i
    // el text visible, codificats per no trencar el marcador.
    let js = format!(
        r#"(function(){{try{{var t=(document.body&&document.body.innerText)?document.body.innerText.slice(0,12000):'';document.title='{marker}'+encodeURIComponent(location.href)+'#'+encodeURIComponent(t);}}catch(e){{document.title='{marker}ERR#'+String(e);}}}})();"#,
        marker = marker
    );
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let _ = win.set_title(&format!("{} · {}", BASE_TITLE, "llegint…"));
        win.eval(&js)
            .map_err(|e| anyhow!("No s'ha pogut llegir la pàgina: {}", e))?;
        // Espera curta perquè el DOM acabe de pintar i el títol canvie.
        tokio::time::sleep(Duration::from_millis(400)).await;
        let title = win.title().unwrap_or_default();
        if let Some(rest) = title.strip_prefix(&marker) {
            let mut parts = rest.splitn(2, '#');
            let url = parts.next().unwrap_or("");
            let text = parts.next().unwrap_or("");
            if url == "ERR" {
                win.set_title(BASE_TITLE).ok();
                return Err(anyhow!("La pàgina ha bloquejat la lectura: {}", text));
            }
            win.set_title(BASE_TITLE).ok();
            let decode = |s: &str| urlencoding::decode(s).map(|c| c.to_string()).unwrap_or_else(|_| s.to_string());
            return Ok(PageExtract {
                url: decode(url),
                text: decode(text),
            });
        }
        if tokio::time::Instant::now() >= deadline {
            win.set_title(BASE_TITLE).ok();
            return Err(anyhow!("La pàgina no ha respost a la lectura (potser encara carrega)"));
        }
    }
}
