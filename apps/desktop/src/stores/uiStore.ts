import { emit } from "@tauri-apps/api/event";
import { create } from "zustand";

const LS_AUTOSAVE = "noorbit.autosave";
const LS_AUTOSAVE_DELAY = "noorbit.autosaveDelay";
const LS_THEME = "noorbit.theme";
const LS_SIDEBAR_W = "noorbit.ui.sidebarW";
const LS_RIGHT_W = "noorbit.ui.rightW";
const LS_TERM_H = "noorbit.ui.termH";
const LS_FUN_CURSOR = "noorbit.fun.cursor";
const LS_FUN_SOUND = "noorbit.fun.sound";

export type Theme = "dark" | "light" | "gray" | "green" | "yellow" | "gold";

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
  /** Menú «Hugging Bay»: cerca i baixada de models GGUF al mirror. */
  showHuggingBay: boolean;
  /** Menú «Pujar»: publicació de temes WordPress al marketplace. */
  showPublishTheme: boolean;
  /** Gestor d'extensions de VS Code instal·lades. */
  showVsCodeExtensions: boolean;
  /** Menú «Game Studios»: flux de creació de jocs de navegador + dreceres per IA. */
  showGameStudios: boolean;
  /** Menú «Crear tema de WordPress»: la IA en crea els fitxers inicials. */
  showWpTheme: boolean;
  /** Menú «Treball autopilotat»: la IA millora qualsevol projecte un temps triat. */
  showProjectWork: boolean;
  /** Menú «Vídeo»: muntatge professional Blender → Kdenlive/ffmpeg dirigit per IA. */
  showVideoStudio: boolean;
  showIaCatalog: boolean;
  showRemoteServer: boolean;
  showSettings: boolean;
  showHelp: boolean;
  /** Informe de fallida («crash»): què ha anat malament i com enviar-ho. */
  showCrashReport: boolean;
  showPlugins: boolean;
  showCommandPalette: boolean;
  showQuickOpen: boolean;
  showSearch: boolean;
  showNewProject: boolean;
  showStartup: boolean;
  showAccount: boolean;
  /** Assistent de primera arrencada (3 passos). */
  showOnboarding: boolean;
  /** Assistent d'entorn («Posant NoOrbit en marxa»): s'obre sol si falta
   *  alguna cosa i es pot obrir manualment des de la barra d'estat. */
  showSetupCheck: boolean;
  terminalOpen: boolean;

  // Mides dels panells (arrossegant les nanses de separació): es recorden
  // entre sessions perquè cada pantalla té la seva millor disposició.
  sidebarWidth: number;
  rightPanelWidth: number;
  terminalHeight: number;

  // Panells oberts en FINESTRA SEPARADA (per posar-los a una altra pantalla):
  // mentre són separats, el programa mostra un marc amb el botó de reenganxar.
  chatDetached: boolean;
  terminalDetached: boolean;

  // Autodesat (com VS Code)
  autoSave: boolean;
  autoSaveDelay: number; // ms

  // Zoom de la interfície (com ⌘+ / ⌘- de VS Code)
  zoom: number;

  // Tema de colors de tota la pantalla
  theme: Theme;

  // Diversió: cursor disfressat per zones (SVG propis) i toc alegre en
  // acabar una tasca. Es recorden entre sessions; commutadors ràpids a
  // la barra d'estat i ajustos a Configuració → General.
  funCursor: boolean;
  funSound: boolean;

  toggleSidebar: () => void;
  toggleRightPanel: () => void;
  setRightPanelOpen: (v: boolean) => void;
  toggleChat: () => void;
  setChatOpen: (open: boolean) => void;
  setShowModelManager: (v: boolean) => void;
  setShowHuggingBay: (v: boolean) => void;
  setShowPublishTheme: (v: boolean) => void;
  setShowVsCodeExtensions: (v: boolean) => void;
  setShowGameStudios: (v: boolean) => void;
  setShowWpTheme: (v: boolean) => void;
  setShowProjectWork: (v: boolean) => void;
  setShowVideoStudio: (v: boolean) => void;
  setShowIaCatalog: (v: boolean) => void;
  setShowRemoteServer: (v: boolean) => void;
  setShowSettings: (v: boolean) => void;
  setShowPlugins: (v: boolean) => void;
  setShowHelp: (v: boolean) => void;
  setShowCrashReport: (v: boolean) => void;
  setShowCommandPalette: (v: boolean) => void;
  setShowQuickOpen: (v: boolean) => void;
  setShowSearch: (v: boolean) => void;
  setShowNewProject: (v: boolean) => void;
  setShowStartup: (v: boolean) => void;
  setShowAccount: (v: boolean) => void;
  setShowOnboarding: (v: boolean) => void;
  setShowSetupCheck: (v: boolean) => void;
  setTerminalOpen: (v: boolean) => void;
  toggleTerminal: () => void;
  setSidebarWidth: (px: number) => void;
  addSidebarWidth: (d: number) => void;
  setRightPanelWidth: (px: number) => void;
  addRightPanelWidth: (d: number) => void;
  setTerminalHeight: (px: number) => void;
  addTerminalHeight: (d: number) => void;
  setChatDetached: (v: boolean) => void;
  setTerminalDetached: (v: boolean) => void;
  setAutoSave: (v: boolean) => void;
  setAutoSaveDelay: (ms: number) => void;
  setZoom: (z: number) => void;
  zoomIn: () => void;
  zoomOut: () => void;
  zoomReset: () => void;
  setTheme: (t: Theme) => void;
  setFunCursor: (v: boolean) => void;
  setFunSound: (v: boolean) => void;
}

/// Tema desat (el llegim directament a les finestres de panells separats,
/// que no passen per l'arrencada completa de l'app).
export function loadTheme(): Theme {
  try {
    const v = localStorage.getItem(LS_THEME);
    if (v === "light" || v === "gray" || v === "green" || v === "yellow" || v === "gold" || v === "dark")
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

/// Cursor divertit desat: el rellegeixen les finestres de panells separats
/// quan l'usuari el canvia des del menú Vista del programa principal.
export function loadFunCursor(): boolean {
  return loadBool(LS_FUN_CURSOR, true);
}

export const useUIStore = create<UIState>((set, get) => ({
  sidebarOpen: true,
  rightPanelOpen: true,
  chatOpen: true,
  showModelManager: false,
  showHuggingBay: false,
  showPublishTheme: false,
  showVsCodeExtensions: false,
  showGameStudios: false,
  showWpTheme: false,
  showProjectWork: false,
  showVideoStudio: false,
  showIaCatalog: false,
  showRemoteServer: false,
  showSettings: false,
  showPlugins: false,
  showHelp: false,
  showCrashReport: false,
  showCommandPalette: false,
  showQuickOpen: false,
  showSearch: false,
  showNewProject: false,
  showStartup: false,
  showAccount: false,
  /** Assistent de primera arrencada (3 passos): només si no s'ha completat. */
  showOnboarding: false,
  showSetupCheck: false,
  terminalOpen: false,

  sidebarWidth: loadNum(LS_SIDEBAR_W, 240),
  rightPanelWidth: loadNum(LS_RIGHT_W, 380),
  terminalHeight: loadNum(LS_TERM_H, 220),

  chatDetached: false,
  terminalDetached: false,

  autoSave: loadBool(LS_AUTOSAVE, true),
  autoSaveDelay: loadNum(LS_AUTOSAVE_DELAY, 1000),
  zoom: 1,
  theme: loadTheme(),
  funCursor: loadFunCursor(),
  funSound: loadBool(LS_FUN_SOUND, true),

  toggleSidebar: () => set({ sidebarOpen: !get().sidebarOpen }),
  toggleRightPanel: () => set({ rightPanelOpen: !get().rightPanelOpen }),
  setRightPanelOpen: (v) => set({ rightPanelOpen: v }),
  toggleChat: () => set({ chatOpen: !get().chatOpen }),
  setChatOpen: (open) => set({ chatOpen: open }),
  setShowModelManager: (v) => set({ showModelManager: v }),
  setShowHuggingBay: (v) => set({ showHuggingBay: v }),
  setShowPublishTheme: (v) => set({ showPublishTheme: v }),
  setShowVsCodeExtensions: (v) => set({ showVsCodeExtensions: v }),
  setShowGameStudios: (v) => set({ showGameStudios: v }),
  setShowWpTheme: (v) => set({ showWpTheme: v }),
  setShowProjectWork: (v) => set({ showProjectWork: v }),
  setShowVideoStudio: (v) => set({ showVideoStudio: v }),
  setShowIaCatalog: (v) => set({ showIaCatalog: v }),
  setShowRemoteServer: (v) => set({ showRemoteServer: v }),
  setShowSettings: (v) => set({ showSettings: v }),
  setShowPlugins: (v) => set({ showPlugins: v }),
  setShowHelp: (v) => set({ showHelp: v }),
  setShowCrashReport: (v) => set({ showCrashReport: v }),
  setShowCommandPalette: (v) => set({ showCommandPalette: v }),
  setShowQuickOpen: (v) => set({ showQuickOpen: v }),
  setShowSearch: (v) => set({ showSearch: v }),
  setShowNewProject: (v) => set({ showNewProject: v }),
  setShowStartup: (v) => set({ showStartup: v }),
  setShowAccount: (v) => set({ showAccount: v }),
  setShowOnboarding: (v) => set({ showOnboarding: v }),
  setShowSetupCheck: (v) => set({ showSetupCheck: v }),
  setTerminalOpen: (v) => set({ terminalOpen: v }),
  toggleTerminal: () => set({ terminalOpen: !get().terminalOpen }),

  // Mides dels panells: amb clamps perquè cap panell no desaparegui ni
  // engolpí tota la finestra; es desen a localStorage (el doble clic sobre
  // la nansa torna als valors per defecte).
  setSidebarWidth: (px) => {
    const v = Math.min(560, Math.max(170, Math.round(px)));
    try {
      localStorage.setItem(LS_SIDEBAR_W, String(v));
    } catch {
      /* sense persistència */
    }
    set({ sidebarWidth: v });
  },
  addSidebarWidth: (d) => get().setSidebarWidth(get().sidebarWidth + d),
  setRightPanelWidth: (px) => {
    const v = Math.min(700, Math.max(280, Math.round(px)));
    try {
      localStorage.setItem(LS_RIGHT_W, String(v));
    } catch {
      /* sense persistència */
    }
    set({ rightPanelWidth: v });
  },
  addRightPanelWidth: (d) => get().setRightPanelWidth(get().rightPanelWidth + d),
  setTerminalHeight: (px) => {
    const v = Math.min(700, Math.max(120, Math.round(px)));
    try {
      localStorage.setItem(LS_TERM_H, String(v));
    } catch {
      /* sense persistència */
    }
    set({ terminalHeight: v });
  },
  addTerminalHeight: (d) => get().setTerminalHeight(get().terminalHeight + d),

  // Els flags de separació NO es persistenten: si l'app es tanca de cop,
  // els panells tornen enganxats a la finestra principal.
  setChatDetached: (v) => set({ chatDetached: v }),
  setTerminalDetached: (v) => set({ terminalDetached: v }),

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
    // Les finestres de panells separats són webviews independents i no
    // veuen aquest canvi: els l'enviem perquè s'actualitzin en directe.
    void emit("theme://changed", { theme: t }).catch(() => undefined);
  },

  setFunCursor: (v) => {
    try {
      localStorage.setItem(LS_FUN_CURSOR, v ? "1" : "0");
    } catch {
      /* sense persistència */
    }
    set({ funCursor: v });
    // Com amb el tema: les finestres separades són webviews independents i
    // tenen el seu propi estat. «Cursor clàssic» ha de valdre a TOT arreu.
    void emit("fun-cursor://changed", { on: v }).catch(() => undefined);
  },
  setFunSound: (v) => {
    try {
      localStorage.setItem(LS_FUN_SOUND, v ? "1" : "0");
    } catch {
      /* sense persistència */
    }
    set({ funSound: v });
  },
}));
