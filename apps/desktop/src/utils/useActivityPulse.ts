/// Pol d'activitat per a l'APRENENTATGE AUTÒNOM EN REPÒS: avisa el backend
/// quan l'usuari toca el teclat, el ratolí o la roda (com a molt un cop cada
/// 20 s, per no omplir el canal d'invocacions). Així la IA nativa mai no es
/// posa a navegar i llegir pàgines mentre algú fa servir el programa, encara
/// que l'ociositat es mesuri també al backend.
import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

/// Miles de segons entre pols (el backend marca el segon de l'últim toc).
const POL_CADA_MS = 20_000;

export function useActivityPulse() {
  useEffect(() => {
    let ultim = 0;
    const pol = () => {
      const now = Date.now();
      if (now - ultim < POL_CADA_MS) return;
      ultim = now;
      void invoke("native_note_activity").catch(() => undefined);
    };
    // Arrencar el programa compta com a activitat: el vagueig no comença
    // en el segon zero, quan l'usuari encara està mirant què fa.
    pol();
    const capturar: (keyof WindowEventMap)[] = ["keydown", "pointerdown"];
    const passius: (keyof WindowEventMap)[] = ["pointermove", "wheel"];
    for (const ev of capturar) window.addEventListener(ev, pol, true);
    for (const ev of passius) window.addEventListener(ev, pol, { passive: true });
    return () => {
      for (const ev of capturar) window.removeEventListener(ev, pol, true);
      for (const ev of passius) window.removeEventListener(ev, pol);
    };
  }, []);
}
