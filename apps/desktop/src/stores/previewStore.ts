import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export type RightTab = "preview" | "agent" | "process" | "computer" | "experts" | "unreal" | "blender" | "mobile" | "git" | "problems" | "console";

interface PreviewState {
  rightTab: RightTab;
  previewUrl: string | null;
  previewActive: boolean;

  setRightTab: (tab: RightTab) => void;
  init: () => Promise<void>;
  startPreview: (root?: string) => Promise<void>;
  stopPreview: () => Promise<void>;
}

export const usePreviewStore = create<PreviewState>((set) => ({
  rightTab: "agent",
  previewUrl: null,
  previewActive: false,

  setRightTab: (tab) => set({ rightTab: tab }),

  init: async () => {
    const { listen } = await import("@tauri-apps/api/event");
    listen("preview://stopped", () => {
      set({ previewActive: false, previewUrl: null });
    });
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
