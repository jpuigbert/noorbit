//! Reconeixement de veu del SISTEMA (macOS): `SFSpeechRecognizer`.
//!
//! És el motor per defecte perquè no demana cap clau ni cap descàrrega: el
//! sistema ja porta els models de la llengua (i a macOS 13+ el reconeixement
//! pot ser 100 % al dispositiu, sense enviar el so enlloc).
//!
//! Es fa per FFI directa (objc2) en compte d'afegir una caixa extra: el que
//! necessitem és menut —autorització, un recognitzador, una petició amb un
//! fitxer i esperar el resultat— i així no depenem de cap crate nou.
//!
//! PERMISOS: l'app ha de dur `NSSpeechRecognitionUsageDescription` a
//! l'Info.plist; si falta, macOS mata el procés en el primer accés. El
//! `build.rs` l'incrusta al binari (per a `tauri dev`) i el paquet .app el
//! porta al seu Info.plist.

#![cfg(target_os = "macos")]

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, sel};
use objc2_foundation::NSString;

#[link(name = "Speech", kind = "framework")]
extern "C" {}

// SFSpeechRecognizerAuthorizationStatus
const NOT_DETERMINED: isize = 0;
const DENIED: isize = 1;
const RESTRICTED: isize = 2;
const AUTHORIZED: isize = 3;

/// Quant de temps esperem el resultat abans de cancel·lar (una gravació de
/// mig minut se sol resoldre en pocs segons; això és només una xarxa).
const TIMEOUT: Duration = Duration::from_secs(150);

pub fn status() -> (bool, String) {
    unsafe {
        let status: isize = msg_send![class!(SFSpeechRecognizer), authorizationStatus];
        match status {
            AUTHORIZED => (
                true,
                "Permís concedit. Si el sistema té el model del català, el so es \
                 reconeix al dispositiu."
                    .into(),
            ),
            NOT_DETERMINED => (
                true,
                "El primer dictat demanarà permís de «Reconeixement de veu».".into(),
            ),
            DENIED => (
                false,
                "Permís de «Reconeixement de veu» DENEGAT. Activa'l a Ajustos del \
                 Sistema ▸ Privadesa i seguretat ▸ Reconeixement de veu i torna a \
                 obrir NoOrbit."
                    .into(),
            ),
            RESTRICTED => (
                false,
                "El sistema restringix el reconeixement de veu en este equip.".into(),
            ),
            _ => (false, "Estat de permís desconegut.".into()),
        }
    }
}

/// Transcriu un fitxer d'àudio. Bloqueja el fil fins que hi ha resultat (es
/// crida dins de `spawn_blocking`).
pub fn transcribe(path: &Path, locale: &str) -> Result<String, String> {
    unsafe { transcribe_inner(path, locale) }
}

unsafe fn transcribe_inner(path: &Path, locale: &str) -> Result<String, String> {
    let cls = class!(SFSpeechRecognizer);

    // 1. Permís. Sense això, la petició ni arranca.
    let status: isize = msg_send![cls, authorizationStatus];
    match status {
        AUTHORIZED => {}
        NOT_DETERMINED => {
            if !request_authorization() {
                return Err(
                    "no s'ha concedit el permís de «Reconeixement de veu»".into(),
                );
            }
        }
        DENIED => {
            return Err(
                "permís de «Reconeixement de veu» denegat (Ajustos del Sistema ▸ \
                 Privadesa i seguretat ▸ Reconeixement de veu)"
                    .into(),
            )
        }
        RESTRICTED => return Err("reconeixement de veu restringit pel sistema".into()),
        _ => {}
    }

    // 2. El recognitzador de la llengua. Si «ca-ES» no hi és, provem «ca» i,
    //    finalment, la llengua del sistema: val més una transcripció en la
    //    llengua del sistema que cap.
    let recognizer = create_recognizer(locale).ok_or_else(|| {
        format!("el sistema no té cap model de reconeixement per a «{locale}»")
    })?;

    let available: bool = msg_send![&*recognizer, isAvailable];
    if !available {
        return Err(
            "el reconeixement del sistema no està disponible ara (cal connexió, o \
             el model de la llengua no s'ha baixat)"
                .into(),
        );
    }

    // 3. La petició: el fitxer gravat, sense resultats parcials (volem la
    //    frase sencera, no un degoteig de paraules).
    let ns_path = NSString::from_str(&path.to_string_lossy());
    let url = objc2_foundation::NSURL::fileURLWithPath(&ns_path);
    let alloc: *mut AnyObject = msg_send![class!(SFSpeechURLRecognitionRequest), alloc];
    let req: *mut AnyObject = msg_send![alloc, initWithURL: &*url];
    let req = Retained::from_raw(req)
        .ok_or_else(|| "no s'ha pogut preparar la petició de veu".to_string())?;
    let _: () = msg_send![&*req, setShouldReportPartialResults: false];

    // Al dispositiu si el sistema ho suporta: el so no ix de l'ordinador.
    // (setRequiresOnDeviceRecognition: existeix des de macOS 13.)
    let knows_on_device: bool =
        msg_send![&*recognizer, respondsToSelector: sel!(supportsOnDeviceRecognition)];
    if knows_on_device {
        let on_device: bool = msg_send![&*recognizer, supportsOnDeviceRecognition];
        let can_require: bool =
            msg_send![&*req, respondsToSelector: sel!(setRequiresOnDeviceRecognition:)];
        if on_device && can_require {
            let _: () = msg_send![&*req, setRequiresOnDeviceRecognition: true];
        }
    }

    // 4. La tasca i l'espera. El «resultHandler» pot cridar-se diverses voltes
    //    (parcials) i des de qualsevol cua: només ens interessa la final.
    let (tx, rx) = mpsc::channel::<Result<String, String>>();
    let handler = RcBlock::new(
        move |result: Option<&AnyObject>, error: Option<&AnyObject>| {
            unsafe {
                if let Some(err) = error {
                    let desc: *mut NSString = msg_send![err, localizedDescription];
                    let text = if desc.is_null() {
                        "error desconegut del reconeixement".to_string()
                    } else {
                        (*desc).to_string()
                    };
                    let _ = tx.send(Err(text));
                    return;
                }
                let Some(result) = result else { return };
                let is_final: bool = msg_send![result, isFinal];
                if !is_final {
                    return;
                }
                let best: *mut AnyObject = msg_send![result, bestTranscription];
                if best.is_null() {
                    let _ = tx.send(Err("no s'ha entès cap paraula".into()));
                    return;
                }
                let text: *mut NSString = msg_send![best, formattedString];
                let value = if text.is_null() {
                    String::new()
                } else {
                    (*text).to_string()
                };
                let _ = tx.send(Ok(value));
            }
        },
    );

    let task: *mut AnyObject = msg_send![
        &*recognizer,
        recognitionTaskWithRequest: &*req,
        resultHandler: &*handler
    ];
    let task = Retained::from_raw(task)
        .ok_or_else(|| "no s'ha pogut iniciar el reconeixement de veu".to_string())?;

    match rx.recv_timeout(TIMEOUT) {
        Ok(Ok(text)) if !text.trim().is_empty() => Ok(text.trim().to_string()),
        Ok(Ok(_)) => Err("no s'ha entès cap paraula".into()),
        Ok(Err(e)) => Err(e),
        Err(_) => {
            let _: () = msg_send![&*task, cancel];
            Err("el reconeixement ha tardat massa (temps esgotat)".into())
        }
    }
}

/// `+[SFSpeechRecognizer requestAuthorization:]` amb espera: el permís es
/// demana una sola volta i el sistema pot trigar el que l'usuari tarde a
/// contestar el diàleg.
unsafe fn request_authorization() -> bool {
    let (tx, rx) = mpsc::channel::<isize>();
    let block = RcBlock::new(move |status: isize| {
        let _ = tx.send(status);
    });
    let _: () = msg_send![class!(SFSpeechRecognizer), requestAuthorization: &*block];
    rx.recv_timeout(Duration::from_secs(180))
        .map(|s| s == AUTHORIZED)
        .unwrap_or(false)
}

/// Crea el recognitzador provant la llengua demanada, la seua variant curta i
/// la llengua del sistema, en este ordre.
unsafe fn create_recognizer(locale: &str) -> Option<Retained<AnyObject>> {
    let mut candidates: Vec<String> = vec![locale.to_string()];
    let short = locale.split(['-', '_']).next().unwrap_or("").to_string();
    if !short.is_empty() && short != locale {
        candidates.push(short);
    }
    if let Some(system) = system_locale() {
        candidates.push(system);
    }

    for candidate in candidates {
        let ns = NSString::from_str(&candidate);
        let alloc: *mut AnyObject = msg_send![class!(SFSpeechRecognizer), alloc];
        let ptr: *mut AnyObject = msg_send![alloc, initWithLocale: &*ns];
        if let Some(rec) = Retained::from_raw(ptr) {
            return Some(rec);
        }
    }
    None
}

unsafe fn system_locale() -> Option<String> {
    let current: *mut AnyObject = msg_send![class!(NSLocale), currentLocale];
    if current.is_null() {
        return None;
    }
    let id: *mut NSString = msg_send![current, localeIdentifier];
    if id.is_null() {
        None
    } else {
        Some((*id).to_string())
    }
}
