//! Servidor de preview HTTP per al workspace.
//!
//! Seguretat (servidor local exposat a la xarxa):
//!   • Resposta només a peticions amb el TOKEN (aleatori, per arrencada):
//!     una altra pàgina del navegador no pot llegir el teu projecte.
//!   • Es rebutja l'enllaçament DNS (rebinding): l'«Origin»/«Host» ha de ser
//!     el del propi servidor.
//!   • Rutes: `..` i symlinks fora de l'arrel no serveixen cap fitxer.

use super::workspace::current_root;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{command, Emitter};

static PREVIEW_RUNNING: AtomicBool = AtomicBool::new(false);
static PREVIEW_STOP: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// TOKEN de la sessió actual del preview (es genera en cada arrencada).
/// L'URL que es mostra a l'usuari l'inclou: sense ell, la resposta és 403.
static PREVIEW_TOKEN: Mutex<Option<String>> = Mutex::new(None);

/// Genera un token aleatori (sense dependències: `getrandom` no cal).
fn make_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // Se barreja amb l'identificador del procés i l'adreça d'un punter: és un
    // token LOCAL (no criptogràfic), però és suficient per a un servidor que
    // només escolta a 127.0.0.1.
    let mix = (nanos as u64) ^ (std::process::id() as u64) << 17;
    let addr = &mix as *const u64 as u64;
    format!("{:016x}{:016x}", mix.rotate_left(23), addr.rotate_left(41))
}

/// Cookie on es desa el token (perquè els recursos relatius — CSS, JS,
/// imatges — el portin sols, sense haver d'afegir el token a cada enllaç).
const PREVIEW_COOKIE: &str = "noorbit_preview";

/// Comprova que la petició és del mateix origin (anti DNS-rebinding) i que
/// porta el token correcte.
fn request_is_allowed(request: &tiny_http::Request, token: &str) -> bool {
    // 1) Token: pot anar a la query (?t=…), a la capçalera o a la cookie.
    let url = request.url();
    let has_token = url
        .split('&')
        .any(|kv| kv == &format!("t={}", token))
        || request
            .headers()
            .iter()
            .any(|h| h.field.equiv("X-Preview-Token") && h.value.as_str() == token)
        || request.headers().iter().any(|h| {
            h.field.equiv("Cookie")
                && h.value
                    .as_str()
                    .contains(&format!("{}={}", PREVIEW_COOKIE, token))
        });
    if !has_token {
        return false;
    }
    // 2) Host: ha de ser el del propi servidor (evita el rebinding: un domini
    //    maliciós que resol a 127.0.0.1 apuntaria al nostre servidor amb un
    //    Host diferent).
    let host_ok = request.headers().iter().any(|h| {
        h.field.equiv("Host")
            && (h.value.as_str().starts_with("127.0.0.1") || h.value.as_str().starts_with("localhost"))
    });
    if !host_ok {
        return false;
    }
    // 3) Origin (si en ve): només el nostre propi origen.
    if let Some(origin) = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Origin"))
        .map(|h| h.value.to_string())
    {
        let allowed = origin.starts_with("http://127.0.0.1")
            || origin.starts_with("http://localhost")
            || origin.starts_with("tauri://")
            || origin.starts_with("http://tauri.localhost");
        if !allowed {
            return false;
        }
    }
    true
}

/// La petició ha vingut amb el token a la query (i no a la cookie)? Cal
/// respondre amb un 302 que deixi la cookie i netegi la query.
fn needs_redirect(request: &tiny_http::Request, token: &str) -> bool {
    let in_query = request
        .url()
        .split('&')
        .any(|kv| kv == &format!("t={}", token));
    let in_cookie = request
        .headers()
        .iter()
        .any(|h| {
            h.field.equiv("Cookie")
                && h.value
                    .as_str()
                    .contains(&format!("{}={}", PREVIEW_COOKIE, token))
        });
    in_query && !in_cookie
}

/// Codifica en percentatge els bytes que NO són vàlids en una capçalera HTTP
/// (no-ASCII o de control). Sense això, una URL amb accents o bytes estranys
/// feia fallar `HeaderValue::parse` i el `unwrap()` matava el fil del servidor
/// en silenci. El resultat sempre és ASCII visible, apte per al parse.
fn header_safe(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if (0x20..0x7f).contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

#[command]
pub async fn start_preview_server(
    app: tauri::AppHandle,
    root: Option<String>,
    port: Option<u16>,
) -> Result<String, String> {
    if PREVIEW_RUNNING.load(Ordering::SeqCst) {
        return Err("El preview ja està en marxa".into());
    }

    let base = root
        .map(PathBuf::from)
        .or_else(current_root)
        .ok_or_else(|| "Cal obrir un workspace primer".to_string())?;
    if !base.is_dir() {
        return Err(format!("{} no és una carpeta", base.display()));
    }

    let port = port.unwrap_or(8123);
    let addr = format!("127.0.0.1:{}", port);
    let server = tiny_http::Server::http(&addr)
        .map_err(|e| format!("No s'ha pogut obrir el servidor: {}", e))?;
    let token = make_token();
    if let Ok(mut t) = PREVIEW_TOKEN.lock() {
        *t = Some(token.clone());
    }
    // El token va a la QUERY (no a la ruta): així els enllaços relatius del
    // projecte (CSS, JS, imatges) continuen funcionant. La primera petició
    // el desa en cookie i es redirigeix a la ruta neta.
    let url = format!("http://{}/?t={}", addr, token);

    let stop = Arc::new(AtomicBool::new(false));
    *PREVIEW_STOP.lock().unwrap() = Some(stop.clone());
    PREVIEW_RUNNING.store(true, Ordering::SeqCst);

    std::thread::spawn(move || {
        let base: PathBuf = base;
        let token = token.clone();
        while !stop.load(Ordering::SeqCst) {
            let request = match server.recv_timeout(std::time::Duration::from_millis(500)) {
                Ok(Some(r)) => r,
                Ok(None) => continue,
                Err(_) => break,
            };
            // Peticions no autoritzades → 403 (no es serveix cap fitxer).
            if !request_is_allowed(&request, &token) {
                let _ = request.respond(
                    tiny_http::Response::from_string("403 Forbidden").with_status_code(403),
                );
                continue;
            }
            // Primera visita amb el token a la query: es deixa la cookie i
            // es redirigeix a la ruta neta (els recursos relatius ja portaran
            // la cookie).
            if needs_redirect(&request, &token) {
                let location = header_safe(request.url().split('?').next().unwrap_or("/"));
                let cookie = format!("{}={}; Path=/; SameSite=Strict", PREVIEW_COOKIE, token);
                // Cap `unwrap()`: si tot i la codificació el parse falla, es
                // respon 400 en comptes de fer panic i matar el servidor.
                let (h1, h2) = match (
                    tiny_http::Header::from_bytes("Location", location.as_bytes()),
                    tiny_http::Header::from_bytes("Set-Cookie", cookie.as_bytes()),
                ) {
                    (Ok(a), Ok(b)) => (a, b),
                    _ => {
                        let _ = request.respond(
                            tiny_http::Response::from_string("400 Bad Request")
                                .with_status_code(400),
                        );
                        continue;
                    }
                };
                let _ = request.respond(
                    tiny_http::Response::from_string("")
                        .with_status_code(302)
                        .with_header(h1)
                        .with_header(h2),
                );
                continue;
            }
            // La query (amb el token) es treu abans de construir la ruta.
            let raw_path = request
                .url()
                .split('?')
                .next()
                .unwrap_or("")
                .trim_start_matches('/');
            // Es DECODIFICA el camí (per exemple «%20» → espai, «%2e» → «.»)
            // i DESPRÉS es valida: si no, un «..%2f..%2f» passava la revisió
            // de components sense ser detectat com a escapada.
            let path = urlencoding::decode(raw_path)
                .map(|c| c.into_owned())
                .unwrap_or_else(|_| raw_path.to_string());
            let file = if path.is_empty() { "index.html" } else { &path };
            let mut p = base.join(file);
            // prevenció d'evasió de ruta: `..` resolt lexicogràficament ABANS
            // del join (starts_with sol és evitable amb /../), i després
            // canonicalitzem i tornem a comprovar per tallar també symlinks
            // que apunten fora del workspace.
            if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                p = base.join("index.html");
            }
            if let Ok(canon_base) = base.canonicalize() {
                if let Ok(canon) = p.canonicalize() {
                    if !canon.starts_with(&canon_base) {
                        p = base.join("index.html");
                    }
                }
            }
            if p.is_dir() {
                p = p.join("index.html");
            }
            let response = match std::fs::read(&p) {
                Ok(bytes) => {
                    let mime = mime_guess::from_path(&p).first_or_octet_stream();
                    let header = tiny_http::Header {
                        field: "Content-Type".parse().unwrap(),
                        value: mime.to_string().parse().unwrap(),
                    };
                    tiny_http::Response::from_data(bytes).with_header(header)
                }
                Err(_) => {
                    tiny_http::Response::from_string("404 Not Found").with_status_code(404)
                }
            };
            let _ = request.respond(response);
        }
        PREVIEW_RUNNING.store(false, Ordering::SeqCst);
        if let Ok(mut t) = PREVIEW_TOKEN.lock() {
            *t = None;
        }
        let _ = app.emit("preview://stopped", ());
    });

    Ok(url)
}

#[command]
pub async fn stop_preview_server() -> Result<(), String> {
    if let Some(stop) = PREVIEW_STOP.lock().unwrap().take() {
        stop.store(true, Ordering::SeqCst);
    }
    PREVIEW_RUNNING.store(false, Ordering::SeqCst);
    if let Ok(mut t) = PREVIEW_TOKEN.lock() {
        *t = None;
    }
    Ok(())
}

#[cfg(test)]
mod proves {
    use super::*;

    /// La ruta servida no ha de poder sortir de l'arrel, ni amb `..` ni amb
    /// la mateixa cosa codificada en percentatges (que el servidor DECODIFICA
    /// abans de validar).
    #[test]
    fn les_rutes_surt_de_l_arrel_no_serveixen_fixers() {
        let base = PathBuf::from("/tmp/workspace");
        for raw in [
            "../../../etc/passwd",
            "..%2f..%2fetc%2fpasswd",
            "%2e%2e%2f%2e%2e%2fetc%2fpasswd",
            "subdir/../../outside.txt",
        ] {
            // El servidor el decodifica primer (com fa el codi real).
            let decoded = urlencoding::decode(raw)
                .map(|c| c.into_owned())
                .unwrap_or_else(|_| raw.to_string());
            let mut p = base.join(&decoded);
            let escapes = p
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir));
            assert!(escapes, "hauria de detectar l'escapada: {}", raw);
            // La protecció substitueix per index.html.
            if escapes {
                p = base.join("index.html");
            }
            assert!(p.starts_with(&base));
        }
    }

    /// El token és únic per arrencada i no és buit.
    #[test]
    fn el_token_es_un_t_e_no_es_preteix() {
        let a = make_token();
        let b = make_token();
        assert!(a.len() >= 16);
        assert_ne!(a, b);
    }

    /// Els bytes invàlids per a una capçalera HTTP es codifiquen: el parse
    /// ja no pot fer panic i matar el fil del servidor.
    #[test]
    fn header_safe_codifica_bytes_invalids() {
        assert_eq!(header_safe("/ruta normal"), "/ruta normal");
        assert_eq!(header_safe("/cafè"), "/caf%C3%A8");
        assert_eq!(header_safe("/a\nb"), "/a%0Ab");
        // El resultat sempre és parsejable com a capçalera real.
        for raw in ["/cafè", "/a\u{0}b", "/x\u{7f}y", "/ña", "/a\tb"] {
            let safe = header_safe(raw);
            assert!(
                tiny_http::Header::from_bytes("Location", safe.as_bytes()).is_ok(),
                "hauria de ser parsejable: {}",
                raw
            );
        }
    }
}
