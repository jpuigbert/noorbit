/// Magatzem de la CONSOLA DE DEPURACIÓ. Recull en un sol lloc:
///   • console.log/info/warn/error de la interfície (React/i18n…).
///   • Errors no captats del webview (window.onerror / unhandledrejection).
///   • Esdeveniments rellevants del backend (errors d'IA, terminal, previsualització).
/// Així, quan alguna cosa va malament, l'usuari té un registre on mirar-ho, com
/// la consola de desenvolupador d'un navegador o la de Sublime/VS Code.
import { create } from "zustand";

export type LogLevel = "log" | "info" | "warn" | "error" | "system";

export interface ConsoleEntry {
  at: number;
  level: LogLevel;
  source: string; // «interfície», «IA», «Terminal», «sistema»…
  text: string;
}

const MAX = 800;

interface ConsoleState {
  entries: ConsoleEntry[];
  paused: boolean;
  add: (level: LogLevel, source: string, text: string) => void;
  clear: () => void;
  setPaused: (p: boolean) => void;
}

function fmt(x: unknown): string {
  if (typeof x === "string") return x;
  try {
    return JSON.stringify(x);
  } catch {
    return String(x);
  }
}

export const useConsoleStore = create<ConsoleState>((set) => ({
  entries: [],
  paused: false,
  add: (level, source, text) =>
    set((s) => {
      if (s.paused && level !== "error") return s;
      const next = s.entries.length >= MAX ? s.entries.slice(s.entries.length - MAX + 1) : s.entries.slice();
      next.push({ at: Date.now(), level, source, text });
      return { entries: next };
    }),
  clear: () => set({ entries: [] }),
  setPaused: (p) => set({ paused: p }),
}));

let installed = false;

/// Instala els paranys globals. Es crida UNA vegada des de App.tsx.
export function installConsoleCapture() {
  if (installed) return;
  installed = true;
  const push = useConsoleStore.getState().add;

  // 1) console.* → consola (sense matar l'original).
  const orig = {
    log: console.log.bind(console),
    info: console.info.bind(console),
    warn: console.warn.bind(console),
    error: console.error.bind(console),
  };
  const hook =
    (level: LogLevel, fn: (...a: unknown[]) => void) =>
    (...args: unknown[]) => {
      fn(...args);
      push(level, "interfície", args.map(fmt).join(" "));
    };
  console.log = hook("log", orig.log);
  console.info = hook("info", orig.info);
  console.warn = hook("warn", orig.warn);
  console.error = hook("error", orig.error);

  // 2) Errors no capturats del webview.
  window.addEventListener("error", (e) => {
    push("error", "sistema", `${e.message} (${e.filename ?? "?"}:${e.lineno ?? 0})`);
  });
  window.addEventListener("unhandledrejection", (e) => {
    push("error", "sistema", "Promesa rebutjada: " + fmt(e.reason));
  });

  push("system", "sistema", "Consola de depuració activa.");
}

/// Registra un esdeveniment del backend (cridat des dels listeners a App.tsx).
export function logBackend(source: string, text: string, level: LogLevel = "log") {
  useConsoleStore.getState().add(level, source, text);
}
