import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

/// Catàleg d'IAs de WEB (DeepSeek, ChatGPT, Claude, Gemini, Perplexity, Grok…)
/// que NoOrbit pot consultar pel navegador intern, amb connexió a Internet i
/// la sessió de l'USUARI (mai tokens, mai sessions suplantades).
///
/// AMB ELS XATS UNIFICATS: aquest store NO guarda converses. Quan l'usuari
/// vol xatejar amb una d'elles, s'obre un xat normal (agentStore.newChat) amb
/// proveïdor «web:<id>», i la conversa viu al mateix lloc que la de la IA
/// local i la dels especialistes. Aquest store només fa de catàleg.

export interface WebAiInfo {
  id: string;
  name: string;
  url: string;
}

interface WebAiState {
  ias: WebAiInfo[];
  init: () => Promise<void>;
  /** obrir la finestra del navegador al xat web (per iniciar sessió) */
  openWebChat: (id: string) => Promise<void>;
}

export const useWebAiStore = create<WebAiState>((set) => ({
  ias: [],

  init: async () => {
    try {
      const list = await invoke<{ id: string; name: string; url: string }[]>("web_ia_list");
      set({ ias: list });
    } catch {
      /* backend no disponible (p. ex. navegador de dev sense Tauri): llista buida */
    }
  },

  openWebChat: async (id) => {
    try {
      await invoke<string>("web_ia_open", { name: id });
    } catch {
      /* la finestra no s'ha pogut obrir: el xat informat quan tracti d'enviar */
    }
  },
}));
