/// AVÍSOS VISIBLES (toasts) — la capa que fa que un error no sigui silenciós.
///
/// Abans, moltes operacions (desar un fitxer, afegir un proveïdor IA,
/// descarregar un model) fallaven en silenci: el `invoke` rebutjava, ningú
/// capturava el rebuig i la captura global de consola l'empassava. L'usuari
/// veia com si tot hagués anat bé.
///
/// Aquest magatzem és el punt únic on els errors es mostren a l'usuari.
import { create } from "zustand";

export type ToastLevel = "info" | "success" | "error";

export interface Toast {
  id: number;
  message: string;
  level: ToastLevel;
}

interface ToastState {
  toasts: Toast[];
  push: (message: string, level?: ToastLevel) => void;
  dismiss: (id: number) => void;
}

let seq = 0;
const MAX_TOASTS = 4;

export const useToastStore = create<ToastState>((set) => ({
  toasts: [],
  push: (message, level = "info") => {
    seq += 1;
    const id = seq;
    set((s) => {
      const next = [...s.toasts, { id, message, level }];
      // No acumulem avisos sense fi: els últims es fallen els primers.
      return { toasts: next.length > MAX_TOASTS ? next.slice(next.length - MAX_TOASTS) : next };
    });
    // El missatge es marca perquè el component el tregui sol (5 s).
    window.setTimeout(() => {
      useToastStore.getState().dismiss(id);
    }, level === "error" ? 8000 : 4500);
  },
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));

/// Mostra un error (amb el text de l'error si en té).
export function toastError(prefix: string, e?: unknown): void {
  const detail = e === undefined ? "" : ": " + String(e);
  useToastStore.getState().push(prefix + detail, "error");
}

/// Mostra un avís informatiu.
export function toastInfo(message: string): void {
  useToastStore.getState().push(message, "info");
}

/// Mostra un missatge d'èxit.
export function toastSuccess(message: string): void {
  useToastStore.getState().push(message, "success");
}
