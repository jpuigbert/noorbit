import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { lintOffline } from "../editor/diagnostics";
import { dropMarkOnManualSave } from "./editReviewStore";

export interface FileNode {
  name: string;
  path: string;
  is_dir: boolean;
  children?: FileNode[];
}

export interface OpenFile {
  path: string;
  name: string;
  content: string;
  dirty: boolean;
  /// Ruta absoluta si el fitxer obert és una IMATGE: l'editor la mostra com a
  /// previsualització (no la llegeix com a text, eixiria basura).
  imagePath?: string;
}

interface WorkspaceState {
  root: string | null;
  tree: FileNode[];
  expanded: Record<string, boolean>;
  openFile: OpenFile | null;
  recents: string[];

  pickWorkspace: () => Promise<void>;
  openWorkspace: (path: string) => Promise<void>;
  removeRecent: (path: string) => void;
  refreshTree: (path?: string) => Promise<void>;
  toggleDir: (path: string) => Promise<void>;
  loadFile: (path: string) => Promise<void>;
  saveFile: (path: string, content: string) => Promise<void>;
  writeLive: (path: string, content: string) => Promise<void>;
  updateContent: (content: string) => void;
  /// Canvia directament el fitxer obert (el usa la revisió d'edicions de la IA
  /// per a posar el contingut definitiu o restaurar l'anterior sense escriure).
  setOpenFile: (file: OpenFile | null) => void;
  closeFile: () => void;

  // Operacions de l'arbre del projecte (barra lateral).
  createFile: (path: string) => Promise<void>;
  createFolder: (path: string) => Promise<void>;
  removeNode: (path: string) => Promise<void>;
  renameNode: (fromPath: string, newName: string) => Promise<void>;

  // Funcionalitat estil VS Code
  newFile: () => void;
  openFilePicker: () => Promise<void>;
  saveAs: () => Promise<void>;
  saveAll: () => Promise<void>;
}

const LS_RECENTS = "noorbit.recents";
const MAX_RECENTS = 8;

/// Extensions que l'editor ha de mostrar com a imatge, no com a text.
const IMAGE_EXT = /\.(png|jpe?g|webp|gif|bmp|ico|tiff?|heic|avif)$/i;
export function isImagePath(path: string): boolean {
  return IMAGE_EXT.test(path);
}

function loadRecents(): string[] {
  try {
    const raw = localStorage.getItem(LS_RECENTS);
    const arr = raw ? (JSON.parse(raw) as unknown) : [];
    return Array.isArray(arr) ? arr.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

function storeRecents(list: string[]) {
  try {
    localStorage.setItem(LS_RECENTS, JSON.stringify(list));
  } catch {
    /* sense persistència */
  }
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  root: null,
  tree: [],
  expanded: {},
  openFile: null,
  recents: loadRecents(),

  pickWorkspace: async () => {
    const selected = await open({ directory: true });
    if (typeof selected === "string") {
      await get().openWorkspace(selected);
    }
  },

  openWorkspace: async (path) => {
    const resolved = await invoke<string>("open_workspace", { path });
    set({ root: resolved });
    // Registra el projecte a la llista de recents (sense duplicats).
    const recents = [resolved, ...get().recents.filter((p) => p !== resolved)].slice(
      0,
      MAX_RECENTS
    );
    storeRecents(recents);
    set({ recents });
    await get().refreshTree(resolved);
  },

  removeRecent: (path) => {
    const recents = get().recents.filter((p) => p !== path);
    storeRecents(recents);
    set({ recents });
  },

  refreshTree: async (path) => {
    const target = path ?? get().root;
    if (!target) return;
    try {
      const tree = await invoke<FileNode[]>("list_dir", { path: target });
      set({ tree });
    } catch {
      set({ tree: [] });
    }
  },

  toggleDir: async (path) => {
    const expanded = { ...get().expanded };
    if (expanded[path]) {
      delete expanded[path];
      set({ expanded });
      return;
    }
    expanded[path] = true;
    set({ expanded });
    // omplir fills si cal (per nivell)
    const fill = async (nodes: FileNode[]): Promise<FileNode[]> =>
      Promise.all(
        nodes.map(async (n) => {
          if (n.is_dir && n.path === path && !n.children) {
            const children = await invoke<FileNode[]>("list_dir", { path: n.path });
            return { ...n, children };
          }
          return n;
        })
      );
    set({ tree: await fill(get().tree) });
  },

  loadFile: async (path) => {
    // Les imatges NO es llegeixen com a text: es marquen perquè l'editor en
    // faces una previsualització (Monaco mostraria bytes binaris sense sentit).
    if (isImagePath(path)) {
      set({
        openFile: {
          path,
          name: path.split(/[/\\]/).pop() ?? path,
          content: "",
          dirty: false,
          imagePath: path,
        },
      });
      return;
    }
    try {
      const content = await invoke<string>("read_file", { path });
      set({
        openFile: { path, name: path.split(/[/\\]/).pop() ?? path, content, dirty: false },
      });
    } catch (e) {
      console.error("No s'ha pogut obrir el fitxer", e);
    }
  },

  saveFile: async (path, content) => {
    await invoke("write_file", { path, content });
    // Si la IA tenia un canvi pendent en este fitxer, el guardat manual de
    // l'usuari l'aixafa: la revisió es tanca (com fa VS Code).
    dropMarkOnManualSave(path);
    const current = get().openFile;
    if (current && current.path === path) {
      set({ openFile: { ...current, content, dirty: false } });
    }
  },

  updateContent: (content) => {
    const current = get().openFile;
    if (current) set({ openFile: { ...current, content, dirty: true } });
  },

  // ---- Escriptura «en viu» d'un fitxer que la IA està generant -------------
  // Escriu el contingut al disc I, si aquest fitxer és l'obert a l'editor (o
  // no n'hi ha cap altre), el refresca perquè l'usuari VEUA créixer el codi
  // mentre la IA l'escriu. dirty=false: el disc i l'editor coincideixen.
  writeLive: async (path, content) => {
    await invoke("write_file", { path, content });
    const cur = get().openFile;
    if (!cur || cur.path === path || cur.path.startsWith("untitled:")) {
      set({
        openFile: { path, name: path.split(/[/\\]/).pop() ?? path, content, dirty: false },
      });
    } else {
      // La IA escriu un fitxer DISTINT de l'obert: Monaco no el veu (no en té
      // model), així que fem-hi la revisió lleugera per registrar-ne els errors
      // al panell «Problemes» igualment, sense obrir-lo.
      lintOffline(path, path.split(/[/\\]/).pop() ?? path, content);
    }
  },

  setOpenFile: (file) => set({ openFile: file }),

  closeFile: () => {
    // Tanca la pestanya NO descarta una revisió pendent: el compte enrere de
    // l'acceptació automàtica (35 s) segueix corrent.
    set({ openFile: null });
  },

  // ---- Fitxer nou sense títol (com ⌘N de VS Code) ---------------------------
  newFile: () => {
    const n = get().openFile;
    let i = 1;
    while (n && n.path.startsWith("untitled:")) {
      const m = n.path.match(/^untitled:(\d+)/);
      if (m) i = Number(m[1]) + 1;
      break;
    }
    set({
      openFile: {
        path: `untitled:${i}`,
        name: `sense-títol-${i}`,
        content: "",
        dirty: true,
      },
    });
  },

  // ---- Obre un fitxer amb el diàleg del sistema -----------------------------
  openFilePicker: async () => {
    const selected = await open({
      multiple: false,
      directory: false,
      defaultPath: get().root ?? undefined,
    });
    if (typeof selected === "string") await get().loadFile(selected);
  },

  // ---- Desa com a… (obligatori per als fitxers sense títol) -----------------
  saveAs: async () => {
    const current = get().openFile;
    if (!current) return;
    const target = await save({
      defaultPath: current.path.startsWith("untitled:") ? current.name : current.path,
    });
    if (typeof target !== "string") return;
    await invoke("write_file", { path: target, content: current.content });
    set({
      openFile: {
        path: target,
        name: target.split(/[/\\]/).pop() ?? target,
        content: current.content,
        dirty: false,
      },
    });
    get().refreshTree();
  },

  // ---- Desa tot (amb un sol fitxer obert, desa'l si és net o sense títol) ---
  saveAll: async () => {
    const current = get().openFile;
    if (!current) return;
    if (current.path.startsWith("untitled:")) {
      await get().saveAs();
      return;
    }
    if (current.dirty) await get().saveFile(current.path, current.content);
  },

  // ---- Desa el fitxer actiu (⌘S): si és sense títol, demana on --------------

  // Crea un fitxer buit al camí absolut indicat i refresca l'arbre.
  createFile: async (path) => {
    await invoke("write_file", { path, content: "" });
    await get().refreshTree();
  },

  // Crea una carpeta (amb els pares que calguin) dins del projecte.
  createFolder: async (path) => {
    await invoke("create_dir", { path });
    await get().refreshTree();
  },

  // Esborra un fitxer o carpeta; si era l'obert (o hi era dins), el tanca.
  removeNode: async (path) => {
    await invoke("delete_path", { path });
    const cur = get().openFile;
    if (cur && (cur.path === path || cur.path.startsWith(path + "/"))) {
      set({ openFile: null });
    }
    await get().refreshTree();
  },

  // Reanomena un node dins del mateix pare (nom sense bars).
  renameNode: async (fromPath, newName) => {
    const clean = newName.trim();
    if (!clean || clean.includes("/") || clean.includes("\\")) return;
    const cut = fromPath.lastIndexOf("/");
    if (cut <= 0) return; // l'arrel no es reanomena
    const to = `${fromPath.slice(0, cut)}/${clean}`;
    await invoke("rename_path", { from: fromPath, to });
    const cur = get().openFile;
    if (cur && cur.path === fromPath) {
      set({ openFile: { ...cur, path: to, name: clean } });
    }
    await get().refreshTree();
  },
}));

/// Desa el fitxer obert; si és «untitled», obre el diàleg Desa com a…
export async function saveCurrentFile(): Promise<void> {
  const s = useWorkspaceStore.getState();
  const current = s.openFile;
  if (!current) return;
  if (current.path.startsWith("untitled:")) await s.saveAs();
  else await s.saveFile(current.path, current.content);
}
