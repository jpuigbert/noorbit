/// Sessió de «Ves a referències» (⌥F12, Fase 3c): llista navegable de
/// referències reals del servidor LSP. L'acció `goto.references` omple el
/// magatzem i el component ReferencesOverlay el pinta i hi navega.
import { create } from "zustand";
import type { LspRef } from "../editor/lspBridge";

interface ReferencesState {
  open: boolean;
  /// Símbol consultat (per al títol de la llista).
  symbol: string;
  items: LspRef[];
  show: (symbol: string, items: LspRef[]) => void;
  close: () => void;
}

export const useReferencesStore = create<ReferencesState>((set) => ({
  open: false,
  symbol: "",
  items: [],
  show: (symbol, items) => set({ open: true, symbol, items }),
  close: () => set({ open: false, items: [] }),
}));