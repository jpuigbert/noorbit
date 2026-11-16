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
  /// Identificador ESTABLE del torn: serveix de clau a React. Sense ell,
  /// la llista invertida dels torns fa que tots els torns canviïn de clau
  /// en cada torn nou (i l'estat obert/plegat es perdi).
  id: number;
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

/// Comptador d'identificadors de torn (clau estable a React).
let turnSeq = 0;

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

// ── THROTTLE DEL VISOR (un sol render per tros de text) ──────────────────
// Un model local emet «ai://process» un cop PER TOKEN: sense això, el visor
// es re-renderitzava desenes de cops per segon. Els fragments «thinking» i
// «streaming» s'acumulen per sessió i s'apliquen junts cada 80 ms; «done» i
// «error» aboquen el pendent immediatament perquè el text final no es perda.
interface PushAcc {
  thinking: string;
  text: string;
  provider: string;
  model: string;
  elapsed_ms: number;
}

const pushBuf: Record<string, PushAcc | undefined> = {};
const pushTimers: Record<string, ReturnType<typeof setTimeout> | undefined> = {};
const PUSH_FLUSH_MS = 80;

function flushPush(sid: string) {
  const t = pushTimers[sid];
  if (t) clearTimeout(t);
  delete pushTimers[sid];
  const acc = pushBuf[sid];
  delete pushBuf[sid];
  if (!acc || (!acc.thinking && !acc.text)) return;
  useProcessStore.setState((s) => {
    const prev = s.per[sid] ?? EMPTY;
    // Un torn acabat seguit de text nou = generació nova: l'anterior
    // s'arxiva (com feia el push immediat, però un sol cop per abocament).
    const starting =
      prev.phase === "done" || prev.phase === "error" || prev.phase === "idle";
    let base: ProcessSnapshot = prev;
    if (starting) {
      const turn: ProcessTurn | null =
        prev.thinking || prev.text || prev.error
          ? {
              id: ++turnSeq,
              provider: prev.provider,
              model: prev.model,
              // Els torns arxivats es retallen TAMBÉ en memòria (la conversa
              // completa ja viu al xat): 8 torns × textos llargs per sessió
              // arribaven a ocupar una bona part de la RAM del webview.
              thinking: tail(prev.thinking, TURN_CHARS),
              text: tail(prev.text, TURN_CHARS),
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
    const next: ProcessSnapshot = {
      ...base,
      thinking: (starting ? "" : prev.thinking) + acc.thinking,
      text: (starting ? "" : prev.text) + acc.text,
      provider: acc.provider,
      model: acc.model,
      phase: acc.text ? "streaming" : "thinking",
      elapsedMs: acc.elapsed_ms,
      startedAt: start,
    };
    persistSoon();
    return { per: { ...s.per, [sid]: next }, rev: s.rev + 1 };
  });
}

export const useProcessStore = create<ProcessState>((set) => ({
  per: loadAll(),
  rev: 0,

  push: (e) => {
    const sid = e.session || "main";
    // Fragments de text: s'acumulen i s'apliquen amb throttle (vegeu
    // «flushPush»). La resta de fases («done», «error»…) s'apliquen
    // immediatament, abocant abans el pendent.
    if (e.phase === "thinking" || e.phase === "streaming") {
      const prev = pushBuf[sid] ?? {
        thinking: "",
        text: "",
        provider: "",
        model: "",
        elapsed_ms: 0,
      };
      pushBuf[sid] = {
        thinking: e.phase === "thinking" ? prev.thinking + e.chunk : prev.thinking,
        text: e.phase === "streaming" ? prev.text + e.chunk : prev.text,
        provider: e.provider,
        model: e.model,
        elapsed_ms: e.elapsed_ms,
      };
      if (!pushTimers[sid]) {
        pushTimers[sid] = setTimeout(() => flushPush(sid), PUSH_FLUSH_MS);
      }
      return;
    }
    flushPush(sid);
    set((s) => {
      const prev = s.per[sid] ?? EMPTY;
      if (e.phase === "done") {
        const next: ProcessSnapshot = {
          ...prev,
          phase: "done",
          elapsedMs: e.elapsed_ms,
          startedAt: null,
        };
        persistSoon();
        return { per: { ...s.per, [sid]: next }, rev: s.rev + 1 };
      }
      if (e.phase === "error") {
        // El chunk porta el motiu (no es descarta com abans) i el temps real.
        const reason = (e.chunk || "").trim();
        const known = prev.error && reason && prev.error.includes(reason);
        const next: ProcessSnapshot = {
          ...prev,
          error: !reason || known ? prev.error : (prev.error ? prev.error + "\n\n" : "") + reason,
          phase: "error",
          elapsedMs: e.elapsed_ms,
          startedAt: null,
        };
        persistSoon();
        return { per: { ...s.per, [sid]: next }, rev: s.rev + 1 };
      }
      return {};
    });
  },

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
  // Amb catch: si el registre falla, el rebuig quedava sense gestionar (i el
  // flag ja era cert → mai es reintentava).
  void listen<ProcessEvent>("ai://process", (evt) => {
    if (!evt.payload) return;
    useProcessStore.getState().push(evt.payload);
  }).catch(() => undefined);
}
