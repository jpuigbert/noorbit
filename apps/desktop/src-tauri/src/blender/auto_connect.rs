//! Vigilant de connexió automàtica a Blender.
//!
//! S'executa en segon pla (un fil dedicat, com el bucle live de
//! `commands::blender_live_start`) i fa la feina manual que fins ara feia
//! l'usuari: quan detecta que el port 9876 obre perquè Blender ha arrencat
//! amb l'add-on (NoOrbit Bridge o blender-mcp), connecta sol i avisa la UI
//! amb l'event «blender://auto-connected»; quan el port tanca, emet
//! «blender://auto-disconnected». Zero intervenció.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

static AUTO_CONNECT_RUNNING: AtomicBool = AtomicBool::new(false);

/// Arrenca el bucle vigilant (idempotent: si ja corre, no en duplica un
/// altre). Comprova el port cada 2 s fins que s'atura amb
/// `blender_auto_connect_stop`.
pub fn spawn_auto_connect(app: AppHandle) {
    if AUTO_CONNECT_RUNNING.swap(true, Ordering::SeqCst) {
        return; // ja està corrent
    }
    // La sonda `super::port_open` és blocant (TCP connect amb timeout): va
    // en un propi fil, com els altres bucles de fons de NoOrbit.
    std::thread::spawn(move || {
        let mut was_connected = false;
        loop {
            // Permet aturar el bucle des de la UI (botó «Atura»).
            if !AUTO_CONNECT_RUNNING.load(Ordering::Relaxed) {
                break;
            }
            let now_connected = super::port_open(super::DEFAULT_SOCKET_PORT);
            if now_connected && !was_connected {
                let _ = app.emit(
                    "blender://auto-connected",
                    serde_json::json!({ "port": super::DEFAULT_SOCKET_PORT }),
                );
            } else if !now_connected && was_connected {
                let _ = app.emit("blender://auto-disconnected", serde_json::json!({}));
            }
            was_connected = now_connected;
            std::thread::sleep(Duration::from_secs(2));
        }
        AUTO_CONNECT_RUNNING.store(false, Ordering::SeqCst);
    });
}

pub fn stop() {
    AUTO_CONNECT_RUNNING.store(false, Ordering::SeqCst);
}

pub fn is_running() -> bool {
    AUTO_CONNECT_RUNNING.load(Ordering::Relaxed)
}
