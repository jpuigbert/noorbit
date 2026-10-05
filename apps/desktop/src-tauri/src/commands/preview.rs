//! Servidor de preview HTTP per al workspace.

use super::workspace::current_root;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{command, Emitter};

static PREVIEW_RUNNING: AtomicBool = AtomicBool::new(false);
static PREVIEW_STOP: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

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
    let url = format!("http://{}", addr);

    let stop = Arc::new(AtomicBool::new(false));
    *PREVIEW_STOP.lock().unwrap() = Some(stop.clone());
    PREVIEW_RUNNING.store(true, Ordering::SeqCst);

    std::thread::spawn(move || {
        let base: PathBuf = base;
        while !stop.load(Ordering::SeqCst) {
            let request = match server.recv_timeout(std::time::Duration::from_millis(500)) {
                Ok(Some(r)) => r,
                Ok(None) => continue,
                Err(_) => break,
            };
            let path = request.url().trim_start_matches('/');
            let file = if path.is_empty() { "index.html" } else { path };
            let mut p = base.join(file);
            // prevenció d'evasió de ruta
            if !p.starts_with(&base) {
                p = base.join("index.html");
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
    Ok(())
}
