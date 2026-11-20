//! Impedeix que el SO S'ADORMI o suspengui la feina autònoma (Jam, millora
//! autopilotada) per «inactivitat»: la IA treballa en segon pla sense teclat
//! ni ratolí, i sense això el Mac/PC es para a mitja generació.
//!
//! Disseny: `activar(nom)` retorna un GUARD. Mentre hi hagi un guard viu, el
//! son de sistema i la suspensió per idle estan blocats; quan tots els guards
//! moren (Jam acabada, aturada o fallida) el SO torna al seu comportament
//! normal. Cada crida és idempotent i mai paniqueja: si el mecanisme natiu no
//! està disponible (p. ex. `caffeinate` esborrat), la Jam continua igualment.
//!
//! Mecanismes (tots del propi SO — cap dependència externa):
//! - macOS: `caffeinate -i -s -w <pid>` — blocat d'idle; sobreviu a la
//!   pantalla apagada i, quan el pare mor (encara que sigui per SIGKILL), el
//!   `caffeinate` es tanca sol perquè vetlla el pid del pare.
//! - Linux: `systemd-inhibit --what=idle:sleep --mode=block` (falla net si no
//!   hi ha systemd: aleshores no fem res, com abans).
//! - Windows: `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED |
//!   ES_DISPLAY_REQUIRED)` reafirmat periòdicament des d'un fil lleuger.

use std::sync::atomic::{AtomicUsize, Ordering};

static ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// Bloqueig viu del son del sistema. En caure'l (Jam acabada o aturada), si
/// era l'últim, es allibera el son i el monitor torna al seu estalvi normal.
pub struct KeepAwakeGuard {
    stop: Option<std::sync::mpsc::Sender<()>>,
    child: Option<std::process::Child>,
    what: &'static str,
}

impl Drop for KeepAwakeGuard {
    fn drop(&mut self) {
        if ACTIVE.fetch_sub(1, Ordering::SeqCst) > 1 {
            // Encara queda feina autònoma en marxa: no toquem res.
            return;
        }
        #[cfg(windows)]
        unsafe {
            es_clear();
        }
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        eprintln!("keep_awake: alliberat ({})", self.what);
    }
}

/// Activa el bloqueig de suspensió/idle per a una feina autònoma (`who` és
/// una etiqueta per al log: «Jam», «treball autopilotat»…).
pub fn activar(who: &'static str) -> KeepAwakeGuard {
    ACTIVE.fetch_add(1, Ordering::SeqCst);
    let out = if cfg!(target_os = "macos") {
        macos(who)
    } else if cfg!(target_os = "linux") {
        linux(who)
    } else if cfg!(target_os = "windows") {
        windows(who)
    } else {
        Ok((None, None))
    };
    match out {
        Ok((stop, child)) => {
            eprintln!("keep_awake: el son del sistema està blocat ({who})");
            KeepAwakeGuard { stop, child, what: who }
        }
        Err(e) => {
            // No és fatal: la Jam continua; simplement el SO podria adormir-se.
            eprintln!("keep_awake: no s'ha pogut blocar el son ({who}): {e}");
            KeepAwakeGuard { stop: None, child: None, what: who }
        }
    }
}

#[cfg(target_os = "macos")]
fn macos(_who: &str) -> Result<(Option<std::sync::mpsc::Sender<()>>, Option<std::process::Child>), String> {
    use std::process::Stdio;
    let me = std::process::id();
    // `-w <pid>`: caffeinate mor quan mor AQUEST procés, encara que el matin
    // sense Don't-Ask (el guardian fa killpg al fill, no al pare).
    let child = std::process::Command::new("caffeinate")
        .args(["-i", "-s", "-w", &me.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("caffeinate: {e}"))?;
    Ok((None, Some(child)))
}

#[cfg(not(target_os = "macos"))]
fn macos(_who: &str) -> Result<(Option<std::sync::mpsc::Sender<()>>, Option<std::process::Child>), String> {
    Ok((None, None))
}

#[cfg(target_os = "linux")]
fn linux(who: &str) -> Result<(Option<std::sync::mpsc::Sender<()>>, Option<std::process::Child>), String> {
    use std::process::Stdio;
    let child = std::process::Command::new("systemd-inhibit")
        .args([
            "--what=idle:sleep",
            "--mode=block",
            "--who",
            "NoOrbit",
            "--why",
            who,
            "--",
            "sleep",
            "infinity",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("systemd-inhibit: {e}"))?;
    Ok((None, Some(child)))
}

#[cfg(not(target_os = "linux"))]
fn linux(_who: &str) -> Result<(Option<std::sync::mpsc::Sender<()>>, Option<std::process::Child>), String> {
    Ok((None, None))
}

#[cfg(target_os = "windows")]
mod win {
    use std::ffi::c_void;

    pub type ExecStateFn = unsafe extern "system" fn(u32) -> u32;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const i8) -> Option<ExecStateFn>;
    }

    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
    const ES_DISPLAY_REQUIRED: u32 = 0x0000_0002;

    fn proc() -> Option<ExecStateFn> {
        unsafe {
            let name: Vec<u16> = "kernel32.dll".encode_utf16().chain(std::iter::once(0)).collect();
            let module = GetModuleHandleW(name.as_ptr());
            if module.is_null() {
                return None;
            }
            GetProcAddress(module, b"SetThreadExecutionState\0".as_ptr() as *const i8)
        }
    }

    pub fn set(flags: u32) -> bool {
        match proc() {
            Some(f) => unsafe { f(flags) != 0 },
            None => false,
        }
    }

    pub fn available() -> bool {
        proc().is_some()
    }
}

/// Windows: `SetThreadExecutionState` és per FIL i pot decaure — el reafirmem
/// cada 50 s des d'un fil lleuger mentre el guard visqui.
#[cfg(target_os = "windows")]
fn windows(_who: &str) -> Result<(Option<std::sync::mpsc::Sender<()>>, Option<std::process::Child>), String> {
    if !win::available() {
        return Err("SetThreadExecutionState no disponible".into());
    }
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || loop {
        win::set(
            0x8000_0000 | 0x0000_0001 | 0x0000_0002, // CONTINUOUS | SYSTEM | DISPLAY
        );
        if rx.recv_timeout(std::time::Duration::from_secs(50)).is_ok() {
            break;
        }
    });
    Ok((Some(tx), None))
}

#[cfg(not(target_os = "windows"))]
fn windows(_who: &str) -> Result<(Option<std::sync::mpsc::Sender<()>>, Option<std::process::Child>), String> {
    Ok((None, None))
}

#[cfg(target_os = "windows")]
unsafe fn es_clear() {
    win::set(0x8000_0000); // ES_CONTINUOUS sol = alliberar el bloqueig
}
