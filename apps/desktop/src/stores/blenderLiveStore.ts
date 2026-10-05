import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface BlenderInfo {
  version: string | null;
  file: string | null;
  objects: number | null;
}

interface BlenderStatus {
  installed: boolean;
  running: boolean;
  binary: string | null;
}

interface BlenderLiveState {
  connected: boolean;
  installed: boolean;
  binary: string | null;
  live: boolean;
  busy: boolean;
  /// Cert si el vigilant de connexió automàtica està actiu (Objectiu 1).
  autoConnect: boolean;
  info: BlenderInfo | null;
  frame: string | null; // data URL
  lastError: string | null;
  lastMessage: string | null;

  init: () => Promise<void>;
  checkConnection: () => Promise<void>;
  refreshStatus: () => Promise<void>;
  launch: () => Promise<void>;
  installAddon: () => Promise<void>;
  installMcpAddon: () => Promise<void>;
  toggleLive: () => Promise<void>;
  /// Arrenca/atura el vigilant que connecta Blender sol quan detecta el port.
  setAutoConnect: (on: boolean) => Promise<void>;
  execute: (code: string) => Promise<string | null>;
}

export const useBlenderLiveStore = create<BlenderLiveState>((set, get) => ({
  connected: false,
  installed: false,
  binary: null,
  live: false,
  busy: false,
  autoConnect: false,
  info: null,
  frame: null,
  lastError: null,
  lastMessage: null,

  init: async () => {
    listen<{ image: string }>("blender://frame", (evt) => {
      set({ frame: "data:image/png;base64," + evt.payload.image });
    });
    // Objectiu 1: el vigilant del backend avisa quan Blender connecta o
    // es desconnecta sol. Actualitzem l'estat sense intervenció de l'usuari.
    listen("blender://auto-connected", () => {
      set({ connected: true });
      void get().checkConnection();
    });
    listen("blender://auto-disconnected", () => {
      set({ connected: false, info: null });
    });
    // Recuperem si el vigilant ja estava actiu (config per defecte: sí).
    try {
      const running = await invoke<boolean>("blender_auto_connect_status");
      set({ autoConnect: running });
    } catch {
      /* ignore */
    }
    await get().refreshStatus();
    await get().checkConnection();
  },

  // Estat del binari + si el port del pont està obert.
  refreshStatus: async () => {
    try {
      const st = await invoke<BlenderStatus>("blender_status");
      set({ installed: st.installed, binary: st.binary, connected: st.running });
    } catch {
      /* ignore */
    }
  },

  checkConnection: async () => {
    try {
      const ok = await invoke<boolean>("blender_ping");
      set({ connected: ok });
      if (ok) {
        const info = await invoke<BlenderInfo>("blender_info");
        set({ info });
      } else {
        set({ info: null });
      }
    } catch {
      set({ connected: false, info: null });
    }
  },

  // Arrenca Blender amb el pont integrat (no cal instal·lar res).
  launch: async () => {
    set({ busy: true, lastError: null, lastMessage: null });
    try {
      const msg = await invoke<string>("blender_launch");
      set({ lastMessage: msg });
      await get().refreshStatus();
      await get().checkConnection();
    } catch (e) {
      set({ lastError: String(e) });
    } finally {
      set({ busy: false });
    }
  },

  // Instal·la i activa l'add-on perquè s'inicie sol en obrir Blender.
  installAddon: async () => {
    set({ busy: true, lastError: null, lastMessage: null });
    try {
      const msg = await invoke<string>("blender_install_addon");
      set({ lastMessage: msg });
    } catch (e) {
      set({ lastError: String(e) });
    } finally {
      set({ busy: false });
    }
  },

  // Descarrega i instal·la automàticament l'add-on comunitari blender-mcp.
  installMcpAddon: async () => {
    set({ busy: true, lastError: null, lastMessage: null });
    try {
      const msg = await invoke<string>("blender_install_mcp_addon");
      set({ lastMessage: msg });
    } catch (e) {
      set({ lastError: String(e) });
    } finally {
      set({ busy: false });
    }
  },

  toggleLive: async () => {
    if (get().live) {
      await invoke("blender_live_stop");
      set({ live: false });
    } else {
      await invoke("blender_live_start");
      set({ live: true });
    }
  },

  setAutoConnect: async (on) => {
    try {
      if (on) {
        await invoke("blender_auto_connect_start");
        set({ autoConnect: true });
      } else {
        await invoke("blender_auto_connect_stop");
        set({ autoConnect: false });
      }
    } catch (e) {
      set({ lastError: String(e) });
    }
  },

  execute: async (code) => {
    try {
      const out = await invoke<string>("blender_execute", { code });
      set({ lastError: null });
      return out;
    } catch (e) {
      set({ lastError: String(e) });
      return null;
    }
  },
}));
