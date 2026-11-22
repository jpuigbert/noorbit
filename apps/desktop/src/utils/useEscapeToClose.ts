/// Tancament amb Esc per als modals amb patró «X + overlay» (Configuració,
/// Nou projecte, Compte): mentre el modal és OBERT, un listener de keydown
/// a window crida `onClose` en prémer Escape. En tancar-se (o desmuntar-se)
/// el listener es retira: amb el modal tancat, Esc no fa res.
///
/// Notes de convivència:
///  · El handler global d'App.tsx ignora les tecles sense modifier (excepte
///    F1/F3/F12), així que Esc no hi interfereix.
///  · Si un element intern del modal gestiona Esc pel seu compte, pot cridar
///    `preventDefault()` i el modal NO es tancarà (primer mana el de dins).
///  · El callback es guarda en un ref perquè el listener no es refà a cada
///    render (els modals passen closures noves a cada passada).
import { useEffect, useRef } from "react";

export function useEscapeToClose(open: boolean, onClose: () => void): void {
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.defaultPrevented) closeRef.current();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);
}