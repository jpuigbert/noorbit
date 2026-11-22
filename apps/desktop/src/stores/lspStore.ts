/// Estat del servidor LSP per a l'indicador discret de la StatusBar (Fase 3c).
/// `refresh` consulta `lsp_status` (barat: NO arrenca cap servidor) i degrada
/// a «cap servidor» si el backend no respon.
import { create } from "zustand";
import { lspStatus } from "../editor/lspBridge";

interface LspState {
  server: string | null;
  language: string | null;
  ready: boolean;
  refresh: (root: string | null) => Promise<void>;
}

export const useLspStore = create<LspState>((set) => ({
  server: null,
  language: null,
  ready: false,
  refresh: async (root) => {
    if (!root) {
      set({ server: null, language: null, ready: false });
      return;
    }
    set(await lspStatus(root));
  },
}));