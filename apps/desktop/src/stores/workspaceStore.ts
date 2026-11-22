import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { lintOffline, schedulePhpLint } from "../editor/diagnostics";
import { scheduleLspDiagnostics } from "../editor/lspBridge";
import { dropMarkOnManualSave } from "./editReviewStore";
import { toastError } from "./toastStore";

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
  /// Ruta absoluta si el fitxer obert és un VÍDEO: l'editor el reprodueix
  /// dins del programa (no el llegeix com a text).
  videoPath?: string;
}

/// Grup d'editor amb el focus (principal o dividit). Les dreceres globals
/// (⌘W, edició IA…) actuen sobre el grup que l'usuari està tocant.
export type EditorGroup = "main" | "split";

interface WorkspaceState {
  root: string | null;
  tree: FileNode[];
  expanded: Record<string, boolean>;
  /// PESTANYES obertes (font de veritat). `openFile` es deriva del fitxer
  /// actiu per compatibilitat amb tots els consumidors antics.
  openFiles: OpenFile[];
  activePath: string | null;
  /// Segon grup d'editor (divisió vertical): quin fitxer mostra. Null = tancat.
  splitPath: string | null;
  focusedGroup: EditorGroup;
  /// COMPATIBILITAT: fitxer actiu derivat (openFiles + activePath). Cap
  /// consumidor antic (autosave, revisió IA, snapshots…) no s'ha de trencar.
  openFile: OpenFile | null;
  recents: string[];

  pickWorkspace: () => Promise<void>;
  openWorkspace: (path: string) => Promise<void>;
  /// Reobri l'últim projecte actiu desat a localStorage («noorbit.lastRoot»).
  /// Retorna `false` si no n'hi ha o si no s'ha pogut obrir (la UI cau llavors
  /// a la StartupScreen amb els recents). La clau NOMÉS s'esborra quan el root
  /// realment no existeix; en fallades transitòries (permís TCC pendent…)
  /// es conserva perquè la pròxima arrencada ho torne a provar.
  restoreLastRoot: () => Promise<boolean>;
  removeRecent: (path: string) => void;
  refreshTree: (path?: string) => Promise<void>;
  toggleDir: (path: string) => Promise<void>;
  /// Obre (o activa) una pestanya. `force` rellegeix del disc encara que la
  /// pestanya existisca; `background` no canvia el fitxer actiu. Retorna
  /// `false` NOMÉS si la lectura del disc ha fallat (p. ex. fitxer esborrat).
  loadFile: (path: string, opts?: { force?: boolean; background?: boolean }) => Promise<boolean>;
  saveFile: (path: string, content: string) => Promise<void>;
  writeLive: (path: string, content: string) => Promise<void>;
  updateContent: (content: string, path?: string) => void;
  /// Canvia directament un fitxer obert (el usa la revisió d'edicions de la IA
  /// per a posar el contingut definitiu o restaurar l'anterior sense escriure).
  /// Si la pestanya existeix es reemplaça AL SEU LLOC (sense activar-la); si
  /// no existeix s'obri com a pestanya nova (semàntica antiga d'«obrir»).
  setOpenFile: (file: OpenFile | null) => void;
  /// Tanca una pestanya (per defecte l'activa) i activa la veïna.
  closeFile: (path?: string) => void;
  setActive: (path: string) => void;
  closeOthers: (path?: string) => void;
  closeAll: () => void;
  /// ⌘\: alterna la divisió vertical (segon grup d'editor).
  toggleSplit: () => void;
  /// «Obre al costat»: mostra el fitxer al grup dividit (l'obri si cal).
  openSide: (path: string) => Promise<void>;
  setFocusedGroup: (g: EditorGroup) => void;

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
const LS_LAST_ROOT = "noorbit.lastRoot";
const MAX_RECENTS = 8;

/// Comptador de lectures en curs: descarta la resposta d'un `loadFile` que
/// arriba després d'un altre (evita mostrar i desar el fitxer equivocat).
let loadFileSeq = 0;

/// Extensions que l'editor ha de mostrar com a imatge, no com a text.
const IMAGE_EXT = /\.(png|jpe?g|webp|gif|bmp|ico|tiff?|heic|avif)$/i;
export function isImagePath(path: string): boolean {
  return IMAGE_EXT.test(path);
}

/// Extensions de vídeo que l'editor ha de reproduir dins del programa, no
/// llegir-les com a text (eixiria basura binària).
const VIDEO_EXT = /\.(mp4|mov|mkv|webm|avi|m4v|mts|m2ts|f4v)$/i;
export function isVideoPath(path: string): boolean {
  return VIDEO_EXT.test(path);
}

function baseName(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
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

/// Error DEFINITIU d'`open_workspace`: la carpeta realment no existeix (el
/// backend reserva el missatge «… no existeix» per a ErrorKind::NotFound).
/// La resta d'errors (accés denegat pel TCC, backend ocupat, desconeguts)
/// són transitòries i NO han de fer perdre el lastRoot. `invoke` rebutja amb
/// un string pla del backend, no necessàriament amb un Error.
function isMissingRootError(e: unknown): boolean {
  const msg = typeof e === "string" ? e : e instanceof Error ? e.message : String(e ?? "");
  return /no existeix|not found|no such file/i.test(msg);
}

/// Estat de pestanyes coherent: `openFile` SEMPRE es deriva d'openFiles +
/// activePath, i el split mai apunta a una pestanya inexistent.
interface TabPatch {
  openFiles?: OpenFile[];
  activePath?: string | null;
  splitPath?: string | null;
  focusedGroup?: EditorGroup;
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => {
  /// Únic punt d'escriptura de l'estat de pestanyes: deriva `openFile` i
  /// saneja activePath/splitPath/focusedGroup perquè no queden penjats.
  const setTabs = (patch: TabPatch) => {
    const cur = get();
    const openFiles = patch.openFiles ?? cur.openFiles;
    let activePath = patch.activePath !== undefined ? patch.activePath : cur.activePath;
    let splitPath = patch.splitPath !== undefined ? patch.splitPath : cur.splitPath;
    let focusedGroup = patch.focusedGroup ?? cur.focusedGroup;
    if (activePath && !openFiles.some((f) => f.path === activePath)) {
      activePath = openFiles[0]?.path ?? null;
    }
    if (splitPath && !openFiles.some((f) => f.path === splitPath)) {
      splitPath = null;
    }
    if (focusedGroup === "split" && !splitPath) focusedGroup = "main";
    const openFile = openFiles.find((f) => f.path === activePath) ?? null;
    set({ openFiles, activePath, splitPath, focusedGroup, openFile });
  };

  /// Insereix o reemplaça una pestanya i (si no és background) l'activa.
  const upsertTab = (file: OpenFile, background?: boolean) => {
    const files = get().openFiles;
    const exists = files.some((f) => f.path === file.path);
    const openFiles = exists
      ? files.map((f) => (f.path === file.path ? file : f))
      : [...files, file];
    setTabs({ openFiles, activePath: background ? get().activePath : file.path });
  };

  /// Després d'obrir/desar/activar un .php del projecte: lint real (php -l)
  /// amb debounce propi. La resta d'extensions no fan res.
  const maybePhpLint = (path: string) => {
    const root = get().root;
    if (root && /\.php$/i.test(path) && path.startsWith(root)) {
      schedulePhpLint(root, path);
    }
  };

  /// Fase 3c: diagnòstics LSP reals en obrir/desar/activar (el backend llig
  /// el DISC: en estos moments el text és exacte). Debounce i cancel·lació
  /// dins del pont; extensions sense servidor no fan res.
  const maybeLspDiagnostics = (path: string) => {
    scheduleLspDiagnostics(get().root, path);
  };

  return {
    root: null,
    tree: [],
    expanded: {},
    openFiles: [],
    activePath: null,
    splitPath: null,
    focusedGroup: "main",
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
      // Últim projecte actiu: si la finestra es recarrega (p. ex. després d'un
      // error), l'arrencada el reobri sol en lloc de perdre l'estat de treball.
      try {
        localStorage.setItem(LS_LAST_ROOT, resolved);
      } catch {
        /* sense persistència */
      }
      // Registra el projecte a la llista de recents (sense duplicats).
      const recents = [resolved, ...get().recents.filter((p) => p !== resolved)].slice(
        0,
        MAX_RECENTS
      );
      storeRecents(recents);
      set({ recents });
      await get().refreshTree(resolved);
    },

    restoreLastRoot: async () => {
      let last: string | null = null;
      try {
        last = localStorage.getItem(LS_LAST_ROOT);
      } catch {
        /* sense persistència */
      }
      if (!last) return false;
      try {
        await get().openWorkspace(last);
        return true;
      } catch (e) {
        // NOMÉS esborrem la clau si el root realment no existeix: en una
        // fallada transitòria (permís TCC de l'Escriptori pendent després
        // d'una re-firma, backend ocupat…) esborrar-la perdria la
        // restauració automàtica PER SEMPRE. La conservem i retornem false
        // perquè la UI caiga als recents i la pròxima arrencada ho reprovi.
        if (isMissingRootError(e)) {
          try {
            localStorage.removeItem(LS_LAST_ROOT);
          } catch {
            /* sense persistència */
          }
        }
        return false;
      }
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
        // Refresc AUTOMÀTIC (polling/focus): les carpetes obertes han de
        // conservar els seus fills — si només rellegírem l'arrel, cada 5 s
        // es buidarien visualment tots els directoris expandits.
        const expanded = get().expanded;
        const fill = async (nodes: FileNode[], depth: number): Promise<FileNode[]> => {
          if (depth > 12) return nodes;
          return Promise.all(
            nodes.map(async (n) => {
              if (n.is_dir && expanded[n.path]) {
                try {
                  const children = await invoke<FileNode[]>("list_dir", { path: n.path });
                  return { ...n, children: await fill(children, depth + 1) };
                } catch {
                  return { ...n, children: [] };
                }
              }
              return n;
            })
          );
        };
        const full = await fill(tree, 0);
        // Polling cada 5 s: si res no ha canviat, no fem cap set (l'arbre
        // virtualitzat no es re-renderitza sense motiu).
        if (JSON.stringify(full) !== JSON.stringify(get().tree)) {
          set({ tree: full });
        }
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

    loadFile: async (path, opts) => {
      // Pestanya ja oberta i sense `force`: només s'activa — MAI es rellegeix
      // del disc, que aixafaria les edicions sense desar de l'usuari.
      const existing = get().openFiles.find((f) => f.path === path);
      if (existing && !opts?.force) {
        if (!opts?.background) setTabs({ activePath: path, focusedGroup: "main" });
        maybePhpLint(path);
        maybeLspDiagnostics(path);
        return true;
      }
      // Guard anti-cursa: si l'usuari clica dos fitxers ràpid (el primer gran o
      // en disc lent), la resposta del PRIMER pot arribar DESPRÉS de la del
      // segon i sobrescriure l'editor → si es desa, les edicions van al fitxer
      // EQUIVOCAT al disc. El comptador descarta respostes tardanes.
      const myCall = ++loadFileSeq;
      // Les imatges NO es llegeixen com a text: es marquen perquè l'editor en
      // faces una previsualització (Monaco mostraria bytes binaris sense sentit).
      if (isImagePath(path)) {
        if (myCall !== loadFileSeq) return true;
        upsertTab(
          { path, name: baseName(path), content: "", dirty: false, imagePath: path },
          opts?.background
        );
        return true;
      }
      // Els vídeos NO es llegeixen com a text: es marquen perquè l'editor els
      // reprodueixi dins del programa (Monaco mostraria bytes binaris sense sentit).
      if (isVideoPath(path)) {
        if (myCall !== loadFileSeq) return true;
        upsertTab(
          { path, name: baseName(path), content: "", dirty: false, videoPath: path },
          opts?.background
        );
        return true;
      }
      try {
        const content = await invoke<string>("read_file", { path });
        if (myCall !== loadFileSeq) return true; // ha arribat tard: descarta
        upsertTab({ path, name: baseName(path), content, dirty: false }, opts?.background);
        maybePhpLint(path);
        maybeLspDiagnostics(path);
        return true;
      } catch (e) {
        console.error("No s'ha pogut obrir el fitxer", e);
        return false;
      }
    },

    saveFile: async (path, content) => {
      try {
        await invoke("write_file", { path, content });
      } catch (e) {
        // Sense això, un desat fallit (disc ple, permisos, fitxer esborrat)
        // era invisible: el flag `dirty` era l'únic indici i l'usuari creia
        // que ho tenia desat.
        toastError("No s'ha pogut desar " + baseName(path), e);
        throw e;
      }
      // Si la IA tenia un canvi pendent en este fitxer, el guardat manual de
      // l'usuari l'aixafa: la revisió es tanca (com fa VS Code).
      dropMarkOnManualSave(path);
      const files = get().openFiles;
      if (files.some((f) => f.path === path)) {
        setTabs({
          openFiles: files.map((f) => (f.path === path ? { ...f, content, dirty: false } : f)),
        });
      }
      maybePhpLint(path);
      maybeLspDiagnostics(path);
    },

    updateContent: (content, path) => {
      const target = path ?? get().activePath;
      if (!target) return;
      const files = get().openFiles;
      if (!files.some((f) => f.path === target)) return;
      setTabs({
        openFiles: files.map((f) => (f.path === target ? { ...f, content, dirty: true } : f)),
      });
    },

    // ---- Escriptura «en viu» d'un fitxer que la IA està generant -------------
    // Escriu el contingut al disc I, si aquest fitxer és en una pestanya (o
    // no n'hi ha cap), el refresca perquè l'usuari VEUA créixer el codi
    // mentre la IA l'escriu. dirty=false: el disc i l'editor coincideixen.
    writeLive: async (path, content) => {
      try {
        await invoke("write_file", { path, content });
      } catch (e) {
        // L'escriptura «en viu» pot fallar (fitxer bloquejat, permisos): sense
        // tractar-ho, el rebuig pujava a la cadena de l'agent sense gestor i
        // l'estat d'escriptura quedava inconsistent.
        toastError("No s'ha pogut escriure " + baseName(path), e);
        throw e;
      }
      const s = get();
      const file: OpenFile = { path, name: baseName(path), content, dirty: false };
      if (s.openFiles.some((f) => f.path === path)) {
        // Pestanya oberta (activa o no): es refresca AL SEU LLOC.
        setTabs({ openFiles: s.openFiles.map((f) => (f.path === path ? { ...f, content, dirty: false } : f)) });
      } else if (s.openFiles.length === 0 || s.activePath?.startsWith("untitled:")) {
        // Sense pestanyes (o només un sense-títol): la IA ocupa l'editor.
        const openFiles = s.activePath?.startsWith("untitled:")
          ? s.openFiles.map((f) => (f.path === s.activePath ? file : f))
          : [file];
        setTabs({ openFiles, activePath: path });
      } else {
        // La IA escriu un fitxer DISTINT dels oberts: Monaco no el veu (no en
        // té model), així que fem-hi la revisió lleugera per registrar-ne els
        // errors al panell «Problemes» igualment, sense obrir-lo.
        lintOffline(path, baseName(path), content);
      }
    },

    setOpenFile: (file) => {
      if (!file) {
        get().closeFile();
        return;
      }
      const files = get().openFiles;
      if (files.some((f) => f.path === file.path)) {
        // Reemplaç al mateix lloc: la revisió IA no ha de canviar la pestanya
        // activa ni el focus de l'usuari.
        setTabs({ openFiles: files.map((f) => (f.path === file.path ? file : f)) });
      } else {
        upsertTab(file);
      }
    },

    closeFile: (path) => {
      // Tanca la pestanya; NO descarta una revisió pendent: el compte enrere
      // de l'acceptació automàtica (35 s) segueix corrent.
      const s = get();
      const target = path ?? s.activePath;
      if (!target) return;
      const idx = s.openFiles.findIndex((f) => f.path === target);
      if (idx === -1) return;
      const openFiles = s.openFiles.filter((f) => f.path !== target);
      let activePath = s.activePath;
      if (s.activePath === target) {
        // Veïna: primer la de la dreta, després la de l'esquerra (com VS Code).
        activePath = openFiles[idx]?.path ?? openFiles[idx - 1]?.path ?? null;
      }
      const splitPath = s.splitPath === target ? activePath : s.splitPath;
      setTabs({ openFiles, activePath, splitPath });
    },

    setActive: (path) => {
      if (!get().openFiles.some((f) => f.path === path)) return;
      setTabs({ activePath: path, focusedGroup: "main" });
      maybePhpLint(path);
      maybeLspDiagnostics(path);
    },

    closeOthers: (path) => {
      const s = get();
      const keep = path ?? s.activePath;
      if (!keep) return;
      const kept = s.openFiles.find((f) => f.path === keep);
      if (!kept) return;
      setTabs({
        openFiles: [kept],
        activePath: keep,
        splitPath: s.splitPath === keep ? keep : null,
      });
    },

    closeAll: () => {
      setTabs({ openFiles: [], activePath: null, splitPath: null, focusedGroup: "main" });
    },

    toggleSplit: () => {
      const s = get();
      if (s.splitPath) {
        setTabs({ splitPath: null, focusedGroup: "main" });
      } else if (s.activePath) {
        setTabs({ splitPath: s.activePath, focusedGroup: "split" });
      }
    },

    openSide: async (path) => {
      if (!get().openFiles.some((f) => f.path === path)) {
        await get().loadFile(path, { background: true });
      }
      if (get().openFiles.some((f) => f.path === path)) {
        setTabs({ splitPath: path, focusedGroup: "split" });
      }
    },

    setFocusedGroup: (g) => {
      setTabs({ focusedGroup: g });
    },

    // ---- Fitxer nou sense títol (com ⌘N de VS Code) ---------------------------
    newFile: () => {
      let i = 1;
      for (const f of get().openFiles) {
        const m = f.path.match(/^untitled:(\d+)/);
        if (m) i = Math.max(i, Number(m[1]) + 1);
      }
      upsertTab({
        path: `untitled:${i}`,
        name: `sense-títol-${i}`,
        content: "",
        dirty: true,
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
      const s = get();
      const renamed: OpenFile = {
        path: target,
        name: baseName(target),
        content: current.content,
        dirty: false,
      };
      setTabs({
        openFiles: s.openFiles.map((f) => (f.path === current.path ? renamed : f)),
        activePath: s.activePath === current.path ? target : s.activePath,
        splitPath: s.splitPath === current.path ? target : s.splitPath,
      });
      get().refreshTree();
    },

    // ---- Desa tot: totes les pestanyes brutes; el sense-títol actiu demana on --
    saveAll: async () => {
      const s = get();
      for (const f of s.openFiles) {
        if (f.dirty && !f.path.startsWith("untitled:")) {
          await get().saveFile(f.path, f.content);
        }
      }
      const cur = get().openFile;
      if (cur && cur.path.startsWith("untitled:")) await get().saveAs();
    },

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

    // Esborra un fitxer o carpeta; les pestanyes que hi eren dins es tanquen.
    removeNode: async (path) => {
      await invoke("delete_path", { path });
      const s = get();
      const inside = (p: string) => p === path || p.startsWith(path + "/");
      if (s.openFiles.some((f) => inside(f.path))) {
        const openFiles = s.openFiles.filter((f) => !inside(f.path));
        setTabs({
          openFiles,
          // Si l'activa era dins del node esborrat, activa la primera resta
          // (no deixem l'editor buit amb pestanyes encara obertes).
          activePath:
            s.activePath && inside(s.activePath)
              ? (openFiles[0]?.path ?? null)
              : s.activePath,
          splitPath: s.splitPath && inside(s.splitPath) ? null : s.splitPath,
        });
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
      const s = get();
      // El node pot ser una carpeta: les pestanyes de dins també es reanomenen.
      const remap = (p: string) =>
        p === fromPath ? to : p.startsWith(fromPath + "/") ? to + p.slice(fromPath.length) : p;
      setTabs({
        openFiles: s.openFiles.map((f) => {
          const p = remap(f.path);
          if (p === f.path) return f;
          const next: OpenFile = { ...f, path: p, name: baseName(p) };
          // Els visors d'imatge/vídeo guarden la ruta absoluta a part: si no
          // es remapeja, després de renombrar apunten al fitxer antic.
          if (f.imagePath) next.imagePath = remap(f.imagePath);
          if (f.videoPath) next.videoPath = remap(f.videoPath);
          return next;
        }),
        activePath: s.activePath ? remap(s.activePath) : null,
        splitPath: s.splitPath ? remap(s.splitPath) : null,
      });
      await get().refreshTree();
    },
  };
});

/// Desa el fitxer actiu; si és «untitled», obre el diàleg Desa com a…
export async function saveCurrentFile(): Promise<void> {
  const s = useWorkspaceStore.getState();
  const current = s.openFile;
  if (!current) return;
  if (current.path.startsWith("untitled:")) await s.saveAs();
  else await s.saveFile(current.path, current.content);
}