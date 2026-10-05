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

/// Estat del visor per a UN xat. abans n'hi havia un de global; ara cada sessió
/// acumula el seu text perquè dos xats en paral·lel no es mixtugin.
export interface ProcessSnapshot {
  text: string;
  thinking: string;
  provider: string | null;
  model: string | null;
  phase: string;
  elapsedMs: number;
  startedAt: number | null;
}

interface ProcessState {
  /// Snapshots indexats per id de sessió ("main" per al xat per defecte).
  per: Record<string, ProcessSnapshot>;
  /// Incrementa amb cada event (per a selectors reactius sans).
  rev: number;

  push: (e: ProcessEvent) => void;
  reset: (session?: string) => void;
}

const EMPTY: ProcessSnapshot = {
  text: "",
  thinking: "",
  provider: null,
  model: null,
  phase: "idle",
  elapsedMs: 0,
  startedAt: null,
};

export const useProcessStore = create<ProcessState>((set) => ({
  per: { main: EMPTY },
  rev: 0,

  push: (e) =>
    set((s) => {
      const sid = e.session || "main";
      const prev = s.per[sid] ?? EMPTY;

      // Una fase «thinking»/«streaming» després d'una generació acabada
      // (done/error/idle) significa que ENCOMENÇA una de nova: netegem el
      // llenç perquè el visor no mezcle respostes diferents del mateix xat.
      const starting =
        (e.phase === "thinking" || e.phase === "streaming") &&
        (prev.phase === "done" || prev.phase === "error" || prev.phase === "idle");
      const base: ProcessSnapshot = starting
        ? { ...EMPTY, provider: prev.provider, model: prev.model }
        : prev;
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
        next = { ...prev, phase: "error", elapsedMs: e.elapsed_ms, startedAt: null };
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
      return { per: { ...s.per, [sid]: next }, rev: s.rev + 1 };
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
