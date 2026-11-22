/// Estat de Git compartit per a la StatusBar i els badges del panell dret
/// (Fase 4). `refresh` consulta `git_status` (barat: només llegeix l'índex)
/// i degrada a «cap repo» si el backend no respon. El GitPanel publica el
/// seu propi resultat ací (`publish`) per no duplicar consultes.
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface GitChange {
  path: string;
  status: string;
  staged: boolean;
}
export interface GitStatusInfo {
  repo: boolean;
  branch: string;
  changes: GitChange[];
}

interface GitStatusState {
  /** Darrera resposta coneguda del backend (null = sense dades encara). */
  status: GitStatusInfo | null;
  /** Consulta lleugera per al polling de la StatusBar. */
  refresh: (root: string | null) => Promise<void>;
  /** El GitPanel comparteix el resultat del seu refresh complet. */
  publish: (st: GitStatusInfo | null) => void;
}

export const useGitStatusStore = create<GitStatusState>((set, get) => ({
  status: null,
  refresh: async (root) => {
    if (!root) {
      if (get().status !== null) set({ status: null });
      return;
    }
    try {
      const st = await invoke<GitStatusInfo>("git_status");
      set({ status: st });
    } catch {
      // Backend sense repo o degradat: la barra simplement no mostra branca.
      if (get().status !== null) set({ status: null });
    }
  },
  publish: (st) => set({ status: st }),
}));