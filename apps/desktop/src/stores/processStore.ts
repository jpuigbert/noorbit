import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";

/// Fragment del procés d'una generació d'IA. El backend l'emet per l'event
/// unificat «ai://process» des de QUALSEVOL base (Ollama, OpenAI-compatible,
/// Claude…), així la UI pot seguir token a token la generació siga quin siga
/// el model actiu.
export interface ProcessEvent {
  provider: string;
  model: string;
  /// "thinking" | "streaming" | "done" | "error".
  phase: string;
  /// Text incremental (buit en done/error).
  chunk: string;
  elapsed_ms: number;
  /// Xat que genera aquests tokens; amb diversos xats en marxa alhora, cada
  /// sessió té el seu propre llenç al visor de procés.
  session?: string;
}

/// Un torn JA ACABAT del visor. Abans el llenç s'esborrava quan en començava
/// un de nou i el raonament es perdia per sempre: ara el torn s'arxiva i es
/// pot tornar a obrir des del panell.
export interface ProcessTurn {
  provider: string | null;
  model: string | null;
  thinking: string;
  text: string;
  error: string | null;
  elapsedMs: number;
  endedAt: number;
}

/// Estat del visor per a UN xat. abans n'hi havia un de global; ara cada sessió
/// acumula el seu text perquè dos xats en paral·lel no es mixtugin.
export interface ProcessSnapshot {
  text: string;
  thinking: string;
  /// Motiu del faliment, EXPLICAT pel backend (abans quedava un «ERROR» mut).
  error: string | null;
  provider: string | null;
  model: string | null;
  phase: string;
  elapsedMs: number;
  startedAt: number | null;
  /// Torns anteriors del mateix xat, del més antic al més recent.
  turns: ProcessTurn[];
}

interface ProcessState {
  /// Snapshots indexats per id de sessió ("main" per al xat per defecte).
  per: Record<string, ProcessSnapshot>;
  /// Incrementa amb cada event (per a selectors reactius sans).
  rev: number;

  push: (e: ProcessEvent) => void;
  reset: (session?: string) => void;
}

const MAX_TURNS = 8;
/// Seccions que es guarden a localStorage: es conserva la cua (el més recent),
/// que és el que interessa rellegir, i no la capçalera d'un text enorme.
const LIVE_CHARS = 20000;
const TURN_CHARS = 6000;
const LS_KEY = "noorbit.process";

const EMPTY: ProcessSnapshot = {
  text: "",
  thinking: "",
  error: null,
  provider: null,
  model: null,
  phase: "idle",
  elapsedMs: 0,
  startedAt: null,
  turns: [],
};

function tail(s: string, n: number): string {
  return s.length > n ? s.slice(s.length - n) : s;
}

/// Un torn en marxa no es pot rependre després de recarregar l'app (els events
/// es van perdre), així que es presenta com a acabat: el text es conserva.
function settled(phase: string): string {
  return phase === "thinking" || phase === "streaming" ? "done" : phase;
}

function loadAll(): Record<string, ProcessSnapshot> {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (!raw) return { main: EMPTY };
    const data = JSON.parse(raw) as Record<string, Partial<ProcessSnapshot>>;
    const out: Record<string, ProcessSnapshot> = {};
    for (const [sid, v] of Object.entries(data)) {
      out[sid] = {
        ...EMPTY,
        ...v,
        thinking: v.thinking ?? "",
        text: v.text ?? "",
        error: v.error ?? null,
        turns: v.turns ?? [],
        phase: settled(v.phase ?? "idle"),
        startedAt: null,
      };
    }
    if (!out.main) out.main = EMPTY;
    return out;
  } catch {
    return { main: EMPTY };
  }
}

let saving: ReturnType<typeof setTimeout> | undefined;

/// Desa el visor de tots els xats, acotat i amb una mica de retard: amb
/// streaming ràpid serien centenars d'escriptures per segon.
function persistSoon() {
  if (saving) return;
  saving = setTimeout(() => {
    saving = undefined;
    try {
      const slim: Record<string, unknown> = {};
      for (const [sid, s] of Object.entries(useProcessStore.getState().per)) {
        slim[sid] = {
          provider: s.provider,
          model: s.model,
          phase: settled(s.phase),
          elapsedMs: s.elapsedMs,
          thinking: tail(s.thinking, LIVE_CHARS),
          text: tail(s.text, LIVE_CHARS),
          error: s.error,
          turns: s.turns.slice(-MAX_TURNS).map((t) => ({
            ...t,
            thinking: tail(t.thinking, TURN_CHARS),
            text: tail(t.text, TURN_CHARS),
          })),
        };
      }
      localStorage.setItem(LS_KEY, JSON.stringify(slim));
    } catch {
      /* sense espai al magatzem local: el visor funciona igual */
    }
  }, 1200);
}

export const useProcessStore = create<ProcessState>((set) => ({
  per: loadAll(),
  rev: 0,

  push: (e) =>
    set((s) => {
      const sid = e.session || "main";
      const prev = s.per[sid] ?? EMPTY;

      // Una fase «thinking»/«streaming» després d'una generació acabada
      // (done/error/idle) significa que ENCOMENÇA una de nova: el llenç
      // anterior NO s'elimina, s'arxiva com a torn històric per poder-lo obrir.
      const starting =
        (e.phase === "thinking" || e.phase === "streaming") &&
        (prev.phase === "done" || prev.phase === "error" || prev.phase === "idle");
      let base: ProcessSnapshot = prev;
      if (starting) {
        const turn: ProcessTurn | null =
          prev.thinking || prev.text || prev.error
            ? {
                provider: prev.provider,
                model: prev.model,
                thinking: prev.thinking,
                text: prev.text,
                error: prev.error,
                elapsedMs: prev.elapsedMs,
                endedAt: Date.now(),
              }
            : null;
        base = {
          ...EMPTY,
          provider: prev.provider,
          model: prev.model,
          turns: turn ? [...prev.turns, turn].slice(-MAX_TURNS) : prev.turns,
        };
      }
      const start = (starting ? null : prev.startedAt) ?? Date.now();

      let next: ProcessSnapshot;
      if (e.phase === "thinking") {
        next = {
          ...base,
          thinking: (starting ? "" : prev.thinking) + e.chunk,
          provider: e.provider,
          model: e.model,
          phase: "thinking",
          elapsedMs: e.elapsed_ms,
          startedAt: start,
        };
      } else if (e.phase === "done") {
        next = { ...prev, phase: "done", elapsedMs: e.elapsed_ms, startedAt: null };
      } else if (e.phase === "error") {
        // El chunk porta el motiu (no es descarta com abans) i el temps real.
        const reason = (e.chunk || "").trim();
        const known = prev.error && reason && prev.error.includes(reason);
        next = {
          ...prev,
          error: !reason || known ? prev.error : (prev.error ? prev.error + "\n\n" : "") + reason,
          phase: "error",
          elapsedMs: e.elapsed_ms,
          startedAt: null,
        };
      } else {
        // streaming: acumula el text de la resposta.
        next = {
          ...base,
          text: (starting ? "" : prev.text) + e.chunk,
          provider: e.provider,
          model: e.model,
          phase: "streaming",
          elapsedMs: e.elapsed_ms,
          startedAt: start,
        };
      }
      const per = { ...s.per, [sid]: next };
      persistSoon();
      return { per, rev: s.rev + 1 };
    }),

  reset: (session) =>
    set((s) => {
      const sid = session || "main";
      return { per: { ...s.per, [sid]: EMPTY } };
    }),
}));

/// Snapshot d'un xat concret (per defecte el primer argument és "main").
export function selectProcessSession(sid: string): ProcessSnapshot {
  return useProcessStore.getState().per[sid] ?? EMPTY;
}

let registered = false;

/// Registra una sola vegada l'escoltador global de «ai://process» (des
/// d'App.tsx), perquè el panell funcione encara que no estiga obert.
export function registerProcessListener() {
  if (registered) return;
  registered = true;
  void listen<ProcessEvent>("ai://process", (evt) => {
    useProcessStore.getState().push(evt.payload);
  });
}
