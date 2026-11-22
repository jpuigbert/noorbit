/// Store del TERMINAL PTY REAL multi-sessió (Fase 3b): metadata de les
/// sessions (pestanyes) i pont amb el backend (`term_start`, `term_write`,
/// `term_resize`, `term_kill`, `term_list`, `term_scrollback`). La sortida
/// en streaming arriba per l'event `term://out` { id, data } i la mort del
/// procés per `term://exit` { id, code }. Els consumidors de la sortida
/// (instàncies xterm) es subscriuen amb `onSessionOutput`: el store NO
/// guarda línies, el scrollback viu a l'xterm (5000 línies).
///
/// COMPATIBILITAT: els events legacy de `run_command` porten `call_id`
/// (no `id`); els ignorem aquí. Els comandaments antics segueixen
/// registrats al backend per a qualsevol consumidor one-shot.
///
/// Els listeners es registren ACÍ (idempotent, amb reintent si fallaven),
/// mai a App.tsx: cada finestra (principal o separada) té el seu propi
/// store i rep els events globals de Tauri.
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n";
import { toastError } from "./toastStore";

export interface TermSession {
  id: string;
  title: string;
  alive: boolean;
  code: number | null;
}

interface TerminalState {
  sessions: TermSession[];
  activeId: string | null;
  /// Connecta els events i sincronitza les sessions del backend (idempotent).
  init: () => Promise<void>;
  /// Rellegeix `term_list` i fusiona pestanyes (sessions creades des d'una
  /// altra finestra, sessions mortes mentre esta finestra no mirava…).
  sync: () => Promise<void>;
  /// Crea la primera sessió si no hi ha cap (en obrir el panell).
  bootstrap: (cwd: string) => Promise<void>;
  createSession: (cwd: string, cols?: number, rows?: number) => Promise<string | null>;
  closeSession: (id: string) => Promise<void>;
  setActive: (id: string) => void;
  /// stdin interactiu: tecles de l'xterm cap al PTY.
  write: (id: string, data: string) => void;
  resize: (id: string, cols: number, rows: number) => void;
}

/// Subscriptors de sortida per sessió (els hosts xterm). Viu fora de
/// l'estat React: escriptures d'alta freqüència no han de re-renderitzar res.
const outputSubs = new Map<string, Set<(data: string) => void>>();

/// Subscriu un consumidor a la sortida (i a la línia de finalització) d'una
/// sessió. Retorna la funció de desubscripció.
export function onSessionOutput(id: string, cb: (data: string) => void): () => void {
  let set = outputSubs.get(id);
  if (!set) {
    set = new Set();
    outputSubs.set(id, set);
  }
  set.add(cb);
  return () => {
    const s = outputSubs.get(id);
    if (!s) return;
    s.delete(cb);
    if (s.size === 0) outputSubs.delete(id);
  };
}

function dispatch(id: string, data: string) {
  const set = outputSubs.get(id);
  if (!set) return;
  for (const cb of set) {
    try {
      cb(data);
    } catch (e) {
      console.error("Error escrivint al terminal", e);
    }
  }
}

let listenersReady = false;
let booting = false;
const unlisteners: Array<() => void> = [];

/// Payloads barrejats: el PTY nou porta `id`; el legacy `call_id`.
interface OutPayload {
  id?: string;
  data?: string;
  call_id?: string;
}
interface ExitPayload {
  id?: string;
  code?: number | null;
  call_id?: string;
}

/// Connecta els events del backend un sol cop (idempotent). El flag només
/// es posa quan TOTS els escoltadors s'han registrat: si `listen` falla,
/// el proper `init`/`createSession` ho reintenta (mateix patró que l'store
/// one-shot anterior, que es quedava encallat si fallava a mitges).
async function ensureListeners() {
  if (listenersReady) return;
  try {
    const { listen } = await import("@tauri-apps/api/event");
    const [u1, u2] = await Promise.all([
      listen<OutPayload>("term://out", (evt) => {
        const p = evt.payload;
        if (typeof p?.id !== "string" || typeof p.data !== "string") return; // legacy
        dispatch(p.id, p.data);
      }),
      listen<ExitPayload>("term://exit", (evt) => {
        const p = evt.payload;
        if (typeof p?.id !== "string") return; // legacy
        const id = p.id;
        const code = typeof p.code === "number" ? p.code : null;
        useTerminalStore.setState((s) => ({
          sessions: s.sessions.map((sess) =>
            sess.id === id ? { ...sess, alive: false, code } : sess
          ),
        }));
        // Línia de comiat dins l'xterm (text tènue), en català.
        const text =
          code === null ? t("terminal.exitedLine") : t("terminal.exitedCode", { n: code });
        dispatch(id, `\r\n\x1b[2m${text}\x1b[0m\r\n`);
      }),
    ]);
    unlisteners.push(u1, u2);
    listenersReady = true;
  } catch (e) {
    console.error("No s'han pogut connectar els escoltadors del terminal", e);
  }
}

function basename(p: string): string {
  const parts = p.replace(/\/+$/, "").split("/");
  return parts[parts.length - 1] || p;
}

export const useTerminalStore = create<TerminalState>((set, get) => ({
  sessions: [],
  activeId: null,

  init: async () => {
    await ensureListeners();
    await get().sync();
  },

  sync: async () => {
    try {
      const list = await invoke<Array<{ id: string; title: string }>>("term_list");
      set((s) => {
        const byId = new Map(list.map((i) => [i.id, i]));
        const sessions: TermSession[] = s.sessions.map((sess) => {
          const info = byId.get(sess.id);
          if (info) return { ...sess, title: info.title, alive: true };
          // El backend ja no la té: si la crèiem viva, ha mort sense event
          // (p. ex. finestra tancada de cop). La conservem com a pestanya
          // morta per no perdre'n el scrollback.
          return sess.alive ? { ...sess, alive: false } : sess;
        });
        for (const info of list) {
          if (!sessions.some((x) => x.id === info.id)) {
            sessions.push({ id: info.id, title: info.title, alive: true, code: null });
          }
        }
        const activeId =
          s.activeId && sessions.some((x) => x.id === s.activeId)
            ? s.activeId
            : sessions.find((x) => x.alive)?.id ?? sessions[0]?.id ?? null;
        return { sessions, activeId };
      });
    } catch (e) {
      console.error("No s'han pogut sincronitzar les sessions de terminal", e);
    }
  },

  bootstrap: async (cwd) => {
    if (booting || get().sessions.length > 0) return;
    booting = true;
    try {
      await get().createSession(cwd);
    } finally {
      booting = false;
    }
  },

  createSession: async (cwd, cols = 80, rows = 24) => {
    await ensureListeners();
    try {
      const id = await invoke<string>("term_start", { cwd, cols, rows });
      set((s) => ({
        sessions: s.sessions.concat({
          id,
          // Títol provisional (shell · carpeta arriba amb el sync).
          title: basename(cwd) || "terminal",
          alive: true,
          code: null,
        }),
        activeId: id,
      }));
      void get().sync();
      return id;
    } catch (e) {
      toastError(t("terminal.errStart"), e);
      return null;
    }
  },

  closeSession: async (id) => {
    try {
      await invoke("term_kill", { id });
    } catch (e) {
      toastError(t("terminal.errKill"), e);
    }
    outputSubs.delete(id);
    set((s) => {
      const idx = s.sessions.findIndex((x) => x.id === id);
      const sessions = s.sessions.filter((x) => x.id !== id);
      const activeId =
        s.activeId === id
          ? sessions[Math.min(idx, sessions.length - 1)]?.id ?? null
          : s.activeId;
      return { sessions, activeId };
    });
  },

  setActive: (id) => set({ activeId: id }),

  write: (id, data) => {
    // Fire-and-forget: si la sessió acaba de morir, el tecleig es perd
    // sense molestar l'usuari (la pestanya ja mostra «finalitzat»).
    void invoke("term_write", { id, data }).catch(() => undefined);
  },

  resize: (id, cols, rows) => {
    void invoke("term_resize", { id, cols, rows }).catch(() => undefined);
  },
}));