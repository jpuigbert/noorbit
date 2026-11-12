import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export type RightTab = "preview" | "agent" | "web" | "dev";

/// Les "Integracions" viuen DINS de l'agent (decisió v0.71): l'usuari no ha
/// de plegar pestanyes per veure l'equip treballant — són sub-pestanyes de
/// la mateixa vista de l'Agent.
export type AgentSub =
  | "chat"
  | "experts"
  | "learning"
  | "computer"
  | "unreal"
  | "blender"
  | "mobile";

/// Els esdeveniments del preview es registren UNA sola vegada.
let previewEventsReady = false;

interface PreviewState {  rightTab: RightTab;
  agentSub: AgentSub;
  previewUrl: string | null;
  previewActive: boolean;

  setRightTab: (tab: RightTab) => void;
  setAgentSub: (sub: AgentSub) => void;
  init: () => Promise<void>;
  startPreview: (root?: string) => Promise<void>;
  stopPreview: () => Promise<void>;
}

export const usePreviewStore = create<PreviewState>((set) => ({
  rightTab: "agent",
  agentSub: "chat",
  previewUrl: null,
  previewActive: false,

  setRightTab: (tab) => set({ rightTab: tab }),
  setAgentSub: (sub) => set({ agentSub: sub }),

  init: async () => {
    // Guard: el registre es fa UNA vegada (RightPanel crida init() en cada
    // muntatge) i amb catch (un registre fallat quedava sense gestionar).
    if (previewEventsReady) return;
    try {
      const { listen } = await import("@tauri-apps/api/event");
      await listen("preview://stopped", () => {
        set({ previewActive: false, previewUrl: null });
      });
      previewEventsReady = true;
    } catch (e) {
      console.error("Preview: no s'han pogut connectar els esdeveniments", e);
    }
  },

  startPreview: async (root) => {
    try {
      const url = await invoke<string>("start_preview_server", {
        root: root ?? null,
        port: null,
      });
      set({ previewUrl: url, previewActive: true, rightTab: "preview" });
    } catch (e) {
      console.error("Preview:", e);
      set({ previewActive: false });
    }
  },

  stopPreview: async () => {
    await invoke("stop_preview_server").catch(() => undefined);
    set({ previewActive: false, previewUrl: null });
  },
}));
