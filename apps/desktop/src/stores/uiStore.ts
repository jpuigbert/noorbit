import { create } from "zustand";

const LS_AUTOSAVE = "noorbit.autosave";
const LS_AUTOSAVE_DELAY = "noorbit.autosaveDelay";
const LS_THEME = "noorbit.theme";

export type Theme = "dark" | "light" | "gray" | "green" | "yellow";

function loadBool(key: string, fallback: boolean): boolean {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : v === "1";
  } catch {
    return fallback;
  }
}

function loadNum(key: string, fallback: number): number {
  try {
    const v = localStorage.getItem(key);
    const n = v === null ? NaN : Number(v);
    return Number.isFinite(n) ? n : fallback;
  } catch {
    return fallback;
  }
}

interface UIState {
  sidebarOpen: boolean;
  rightPanelOpen: boolean;
  chatOpen: boolean;
  showModelManager: boolean;
  showSettings: boolean;
  showHelp: boolean;
  showPlugins: boolean;
  showCommandPalette: boolean;
  showQuickOpen: boolean;
  showSearch: boolean;
  showNewProject: boolean;
  showStartup: boolean;
  terminalOpen: boolean;

  // Autodesat (com VS Code)
  autoSave: boolean;
  autoSaveDelay: number; // ms

  // Zoom de la interfície (com ⌘+ / ⌘- de VS Code)
  zoom: number;

  // Tema de colors de tota la pantalla
  theme: Theme;

  toggleSidebar: () => void;
  toggleRightPanel: () => void;
  setRightPanelOpen: (v: boolean) => void;
  toggleChat: () => void;
  setChatOpen: (open: boolean) => void;
  setShowModelManager: (v: boolean) => void;
  setShowSettings: (v: boolean) => void;
  setShowPlugins: (v: boolean) => void;
  setShowHelp: (v: boolean) => void;
  setShowCommandPalette: (v: boolean) => void;
  setShowQuickOpen: (v: boolean) => void;
  setShowSearch: (v: boolean) => void;
  setShowNewProject: (v: boolean) => void;
  setShowStartup: (v: boolean) => void;
  setTerminalOpen: (v: boolean) => void;
  toggleTerminal: () => void;
  setAutoSave: (v: boolean) => void;
  setAutoSaveDelay: (ms: number) => void;
  setZoom: (z: number) => void;
  zoomIn: () => void;
  zoomOut: () => void;
  zoomReset: () => void;
  setTheme: (t: Theme) => void;
}

function loadTheme(): Theme {
  try {
    const v = localStorage.getItem(LS_THEME);
    if (v === "light" || v === "gray" || v === "green" || v === "yellow" || v === "dark")
      return v;
  } catch {
    /* sense persistència */
  }
  return "dark";
}

/// Aplica el tema a l'arrel del document (les variables CSS en depenen).
export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === "dark") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", theme);
}

export const useUIStore = create<UIState>((set, get) => ({
  sidebarOpen: true,
  rightPanelOpen: true,
  chatOpen: true,
  showModelManager: false,
  showSettings: false,
  showPlugins: false,
  showHelp: false,
  showCommandPalette: false,
  showQuickOpen: false,
  showSearch: false,
  showNewProject: false,
  showStartup: false,
  terminalOpen: false,

  autoSave: loadBool(LS_AUTOSAVE, true),
  autoSaveDelay: loadNum(LS_AUTOSAVE_DELAY, 1000),
  zoom: 1,
  theme: loadTheme(),

  toggleSidebar: () => set({ sidebarOpen: !get().sidebarOpen }),
  toggleRightPanel: () => set({ rightPanelOpen: !get().rightPanelOpen }),
  setRightPanelOpen: (v) => set({ rightPanelOpen: v }),
  toggleChat: () => set({ chatOpen: !get().chatOpen }),
  setChatOpen: (open) => set({ chatOpen: open }),
  setShowModelManager: (v) => set({ showModelManager: v }),
  setShowSettings: (v) => set({ showSettings: v }),
  setShowPlugins: (v) => set({ showPlugins: v }),
  setShowHelp: (v) => set({ showHelp: v }),
  setShowCommandPalette: (v) => set({ showCommandPalette: v }),
  setShowQuickOpen: (v) => set({ showQuickOpen: v }),
  setShowSearch: (v) => set({ showSearch: v }),
  setShowNewProject: (v) => set({ showNewProject: v }),
  setShowStartup: (v) => set({ showStartup: v }),
  setTerminalOpen: (v) => set({ terminalOpen: v }),
  toggleTerminal: () => set({ terminalOpen: !get().terminalOpen }),

  setAutoSave: (v) => {
    try {
      localStorage.setItem(LS_AUTOSAVE, v ? "1" : "0");
    } catch {
      /* sense persistència */
    }
    set({ autoSave: v });
  },
  setAutoSaveDelay: (ms) => {
    try {
      localStorage.setItem(LS_AUTOSAVE_DELAY, String(ms));
    } catch {
      /* sense persistència */
    }
    set({ autoSaveDelay: ms });
  },

  setZoom: (z) => set({ zoom: Math.min(2, Math.max(0.6, Math.round(z * 100) / 100)) }),
  zoomIn: () => get().setZoom(get().zoom + 0.1),
  zoomOut: () => get().setZoom(get().zoom - 0.1),
  zoomReset: () => get().setZoom(1),

  setTheme: (t) => {
    try {
      localStorage.setItem(LS_THEME, t);
    } catch {
      /* sense persistència */
    }
    applyTheme(t);
    set({ theme: t });
  },
}));
