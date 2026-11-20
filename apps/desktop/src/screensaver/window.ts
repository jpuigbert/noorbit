/// ESTALVI DE PANTALLA DE LA FEINA AUTONOMA (pantalla completa multi-monitor).
/// La Jam i el treball autopilotat poden durar dies escrivint codi sense que
/// l'usuari toqui res; sense pantalla, el Mac/PC fa el seu estalvi (apaga o
/// fa «showscreen») i ningú no veu QUÈ s'està fent ni en QUIN projecte.
///
/// Aquesta finestra pròpia (label «noorbit-screensaver») pinta el progrés en
/// gran; es tanca amb un clic o qualsevol tecla i torna a la feina normal.
/// El so del SO ja està blocat des del backend (keep_awake.rs), així que la
/// pantalla es pot deixar encesa tota la nit sense que el machine s'adormi.
///
/// Ús:
/// - `obrirEstalvi()` — el botó del panell Jam/modal de treball (i l'usuari
///   el pot obrir quantes vegades vulgui).
/// - `tancaEstalvi()` — la mateixa finestra en clic/tecla.
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

export const SAVER_LABEL = "noorbit-screensaver";

/// Crida en curs (doble clic ràpid): sense això, dues crides simultànies
/// intentarien crear la mateixa etiqueta i la segona petaria.
let pendent: Promise<void> | null = null;

/// Obre l'estalvi a PANTALLA COMPLETA (a la pantalla on ja hi ha una finestra
/// de NoOrbit — l'usuari l'obre des d'aquell monitor). Si ja era obert, només
/// el porta al davant.
export async function obrirEstalvi(): Promise<void> {
  if (pendent) return pendent;
  pendent = (async () => {
    try {
      const existing = await WebviewWindow.getByLabel(SAVER_LABEL);
      if (existing) {
        void existing.setFocus().catch(() => undefined);
        return;
      }
      const win = new WebviewWindow(SAVER_LABEL, {
        title: "NoOrbit \u2014 feina en curs",
        fullscreen: true,
        decorations: false,
        alwaysOnTop: true,
        skipTaskbar: true,
        resizable: false,
        theme: "dark",
        backgroundColor: "#07070c",
      });
      win.once("tauri://error", (e) => {
        // Creació fallida (p. ex. capacitat ACL absent): ho avisem però no
        // bloquem res; la Jam continua igualment en segon pla.
        console.error("noorbit-screensaver:", e.payload);
      });
    } finally {
      pendent = null;
    }
  })();
  return pendent;
}

/// Tanca l'estalvi des de DINS de la seva propia finestra (clic/tecla).
export async function tancaEstalvi(): Promise<void> {
  try {
    const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    await getCurrentWebviewWindow().destroy();
  } catch {
    /* si no es pot destruir, l'usuari pot fer força la sortida (cmd+Q no: la
       Jam continua en segon pla, que és el que vol) */
  }
}
