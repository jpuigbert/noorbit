/// ESTALVI AUTOMÀTIC EN INACTIVITAT.
///
/// Mentre la Jam o el treball autopilotat corre i l'usuari no toca res, el
/// SO apagaria la pantalla o faria el seu propi estalvi i ningú no veuria la
/// feina. Aquest hook obre sol l'estalvi de NoOrbit (la pantalla de feina)
/// al cap de X minuts sense tocar res — només si hi ha feina autònoma en
/// marxa, perquè un escriptori quiet sense feina no ha d'obrir res.
///
/// Mateix patró que `useActivityPulse`: escoltem teclat/ratolí/roda per
/// marcar l'últim toc i, com a molt un cop cada 20 s, mirem si cal obrir-la.
/// L'ajust («obre l'estalvi sol» + minuts) viu a `useUIStore` i es llegeix
/// en cada pol, així que els canvis d'Ajustos s'apliquen sense reiniciar.
import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useUIStore } from "../stores/uiStore";
import { obrirEstalvi, SAVER_CLOSED_EVENT, SAVER_LABEL } from "./window";

/// Mil·lisegons entre comprovacions (el mateix ritme que el pol d'activitat).
const POL_CADA_MS = 20_000;

/// Estat mínim del backend: només ens cal saber si corre.
interface RunningState {
  running: boolean;
}

export function useAutoSaver() {
  useEffect(() => {
    let ultimToc = Date.now();

    const comprova = () => {
      const { saverAuto, saverAutoMinutes } = useUIStore.getState();
      if (!saverAuto) return;
      const idle = Date.now() - ultimToc;
      if (idle < saverAutoMinutes * 60_000) return;
      void (async () => {
        try {
          // Ja està oberta? (a pantalla completa no rep els nostres tocs,
          // així que l'ociositat segueix creixent mentre es veu: no passa
          // res, però no l'hem de tornar a obrir).
          if (await WebviewWindow.getByLabel(SAVER_LABEL)) return;
          const [jam, feina] = await Promise.all([
            invoke<RunningState>("jam_state").catch(() => null),
            invoke<RunningState>("project_work_state").catch(() => null),
          ]);
          if (jam?.running || feina?.running) void obrirEstalvi();
        } catch {
          /* sense estalvi: la feina continua igualment */
        }
      })();
    };

    // Qualsevol toc reinicia el rellotge (el mateix joc d'esdeveniments que
    // `useActivityPulse`; el moviment del ratolí ja no cal: sense clic ni
    // roda la pantalla de feina ha d'aparèixer igualment).
    const tocat = () => {
      ultimToc = Date.now();
    };
    const capturar: (keyof WindowEventMap)[] = ["keydown", "pointerdown"];
    const passius: (keyof WindowEventMap)[] = ["wheel"];
    for (const ev of capturar) window.addEventListener(ev, tocat, true);
    for (const ev of passius) window.addEventListener(ev, tocat, { passive: true });

    // Quan l'estalvi es tanca des de DINS (clic/tecla a la seva finestra),
    // la finestra principal no ha rebut cap toc: el tancament compta com a
    // activitat perquè no es torni a obrir tot seguit.
    let un: (() => void) | undefined;
    void listen(SAVER_CLOSED_EVENT, () => {
      ultimToc = Date.now();
    }).then((u) => {
      un = u;
    });

    // Primera comprovació ara mateix (si l'usuari va obrir el programa i el
    // va deixar quiet amb una Jam en marxa, l'estalvi arriba sol al cap dels
    // minuts triats, no al cap de 20 s + minuts).
    comprova();
    const id = setInterval(comprova, POL_CADA_MS);

    return () => {
      clearInterval(id);
      if (un) un();
      for (const ev of capturar) window.removeEventListener(ev, tocat, true);
      for (const ev of passius) window.removeEventListener(ev, tocat);
    };
  }, []);
}
