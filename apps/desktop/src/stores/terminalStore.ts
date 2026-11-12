/// Store del terminal integrat: manté el simulacre de consola (línies de
/// comanda i sortida) i executa les comandes al backend (`run_command`),
/// que va emetent `term://out` per trossos i `term://exit` en acabar.
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface TermLine {
  id: number;
  kind: "cmd" | "out" | "err" | "exit" | "info";
  text: string;
}

interface TerminalState {
  lines: TermLine[];
  running: boolean;
  history: string[];
  run: (cmd: string) => Promise<void>;
  clear: () => void;
}

let nextId = 1;
let currentCall = "";
let listenersReady = false;
let pending = ""; // sortida acumulada entre emissió i emissió
const unlisteners: Array<() => void> = [];

const MAX_LINES = 600;

function push(lines: TermLine[], line: Omit<TermLine, "id">): TermLine[] {
  const out = lines.concat({ id: nextId++, ...line });
  return out.length > MAX_LINES ? out.slice(out.length - MAX_LINES) : out;
}

/// Connecta els events del backend un sol cop (idempotent).
///
/// Abans es marcava `listenersReady = true` ABANS de registrar els escoltadors
/// i sense capturar errors: si el `listen` fallava, el flag ja era cert (mai
/// es reintentava) i, sense l'event `term://exit`, el camp `running` quedava
/// encallat a true per sempre → el terminal bloquejava tota execució futura
/// fins a reiniciar l'app. Ara el flag només es posa quan TOTS els
/// escoltadors s'han registrat correctament.
async function ensureListeners() {
  if (listenersReady) return;
  try {
    const { listen } = await import("@tauri-apps/api/event");
    const [u1, u2] = await Promise.all([
      listen<{ call_id: string; kind: string; text: string }>("term://out", (evt) => {
        if (evt.payload.call_id !== currentCall) return;
        pending += evt.payload.text;
        // Dividim en línies; el tros incomplet queda pendent per al següent chunk.
        const parts = pending.split("\n");
        pending = parts.pop() ?? "";
        if (parts.length === 0) return;
        useTerminalStore.setState((s) => {
          let lines = s.lines;
          for (const p of parts) {
            if (p.trim() === "" && lines.length > 0) continue;
            lines = push(lines, { kind: evt.payload.kind === "err" ? "err" : "out", text: p });
          }
          return { lines };
        });
      }),
      listen<{ call_id: string; code: number }>("term://exit", (evt) => {
        if (evt.payload.call_id !== currentCall) return;
        if (pending !== "") {
          useTerminalStore.setState((s) => ({ lines: push(s.lines, { kind: "out", text: pending }) }));
          pending = "";
        }
        useTerminalStore.setState((s) => ({
          running: false,
          lines: push(s.lines, {
            kind: "exit",
            text: evt.payload.code === 0 ? "✓" : `✗ exit ${evt.payload.code}`,
          }),
        }));
      }),
    ]);
    unlisteners.push(u1, u2);
    listenersReady = true;
  } catch (e) {
    // Es deixa el flag a false: el proper `run` ho reintenta.
    console.error("No s'han pogut connectar els escoltadors del terminal", e);
  }
}

/// Identificador de la comanda EN EXECUCIÓ (per aturar-la).
export function currentCallId(): string {
  return currentCall;
}

export const useTerminalStore = create<TerminalState>((set, get) => ({
  lines: [],
  running: false,
  history: [],

  run: async (cmd) => {
    const c = cmd.trim();
    if (!c || get().running) return;
    await ensureListeners();
    // Identificador únic: dos «run» en el mateix mil·lisegon es
    // confonien i la sortida del primer es perdia.
    currentCall = `t${Date.now()}-${nextId++}`;
    pending = "";
    set((s) => ({
      running: true,
      history: s.history.filter((h) => h !== c).concat(c).slice(-50),
      lines: push(s.lines, { kind: "cmd", text: c }),
    }));
    try {
      // invoke resol quan la comanda HA ACABAT; la sortida ja va arribant
      // pels events. El status final també el porta term://exit.
      // ATENCIÓ al nom de l'argument: Tauri 2 l' espera en camelCase
      // (`callId`). Amb `call_id` la comanda fallava amb «missing required
      // key callId» i el terminal NOexecutava res.
      await invoke("run_command", { cmd: c, callId: currentCall });
    } catch (e) {
      set((s) => ({
        running: false,
        lines: push(s.lines, { kind: "err", text: String(e) }),
      }));
    }
  },

  clear: () => set({ lines: [] }),
}));
