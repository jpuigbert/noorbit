/// Estat del muntatge de vídeo (menú «Vídeo»): eines instal·lades, orígens del
/// projecte, EDL desat, director autopilotat i render en curs.
///
/// Viu FORA del panell: els escoltadors es registren a l'arrencada del
/// programa (App.tsx), així que el render i el director poden continuar en
/// segon pla amb el menú tancat, i els avisos (`video://note`) arriben igualment.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { readFile } from "@tauri-apps/plugin-fs";
import { create } from "zustand";
import { t } from "../i18n";
import { toastError, toastInfo, toastSuccess } from "./toastStore";

// ─── Tipus (espeullen els structs Rust de `src-tauri/src/video`) ─────────────

export interface VideoTool {
  name: string;
  path: string | null;
  version: string | null;
  ok: boolean;
}

export interface VideoTools {
  ffmpeg: VideoTool;
  ffprobe: VideoTool;
  melt: VideoTool;
  kdenlive: VideoTool;
  ready: boolean;
  bundle: string | null;
}

export interface SourceInfo {
  path: string;
  name: string;
  width: number;
  height: number;
  fps: number;
  duration: number;
  hasAudio: boolean;
  vcodec: string;
  acodec: string;
  sampleRate: number;
  channels: number;
  sizeBytes: number;
  readable: boolean;
  sheet: string | null;
  luma: number;
  saturation: number;
  motion: number;
  black: number;
  sceneChanges: number;
  silenceBlocks: number;
  totalSilenceS: number;
}

export interface Grade {
  brightness: number;
  contrast: number;
  saturation: number;
  warmth: number;
  stabilize: boolean;
}

export interface Transition {
  kind: string;
  durS: number;
}

export interface Clip {
  source: string;
  label: string;
  inS: number;
  outS: number;
  speed: number;
  gain: number;
  fadeInS: number;
  fadeOutS: number;
  mute: boolean;
  transition: Transition | null;
  grade: Grade | null;
}

export interface TextItem {
  text: string;
  atS: number;
  durS: number;
  size: number;
  pos: string;
  kind: string;
}

export interface EdlProfile {
  width: number;
  height: number;
  fps: number;
  crf: number;
  preset: string;
}

export interface Edl {
  name: string;
  profile: EdlProfile;
  clips: Clip[];
  texts: TextItem[];
  targetLufs: number;
  normalizeAudio: boolean;
  note: string;
}

export interface SanitizeReport {
  dropped: number;
  notes: string[];
  edl: Edl;
  durationS: number;
  path: string;
}

export interface ScanReport {
  toolsReady: boolean;
  bundle: string | null;
  sources: SourceInfo[];
  saved: string[];
  edl: Edl | null;
  lastRender: string | null;
  workDir: string;
}

export interface DirectorState {
  running: boolean;
  stage: string;
  pass: number;
  critique: number;
  elapsedMs: number;
  budgetMs: number;
  progress: number;
  detail: string;
  sourcesFound: number;
  clips: number;
  targetS: number;
  outPath: string | null;
  projectPath: string | null;
  edl: Edl | null;
  report: unknown;
  log: string[];
  startedAt: number;
  finishedAt: number | null;
  error: string | null;
}

export interface RenderState {
  running: boolean;
  stage: string;
  progress: number;
  detail: string;
  preview: boolean;
  outPath: string | null;
  commandLine: string | null;
  startedAt: number;
  elapsedMs: number;
  error: string | null;
  report: unknown;
}

export interface PlanInfo {
  command: string;
  out: string;
  durationS: number;
  note: string;
}

/// Opcions del director (el que decideix la IA sola).
export interface DirectorOptions {
  projectPath: string;
  name: string;
  brief: string;
  sources: string[];
  budgetSecs: number;
  targetS: number;
  provider?: string;
  model?: string;
  critiques: number;
  width?: number;
  height?: number;
  fps?: number;
  /// Qualitat final (CRF 10..32) i velocitat de codificació.
  crf?: number;
  preset?: string;
  container: string;
  silent: boolean;
}

interface VideoStudioState {
  tools: VideoTools | null;
  scan: ScanReport | null;
  edl: Edl | null;
  director: DirectorState | null;
  render: RenderState | null;
  plan: PlanInfo | null;
  notes: string[];
  /// Transicions vàlides (les del mateix `xfade` d'ffmpeg), per omplir els selectors.
  transitions: string[];
  scanning: boolean;
  busy: string | null;
  ready: boolean;
  init: () => void;
  /// Omple el panell amb el que JA està passant: eines, estat del director i del
  /// render (poden treballar en segon pla amb el menú tancat) i les transicions.
  hydrate: () => Promise<void>;
  loadTools: () => Promise<void>;
  scanProject: (projectPath: string, sheets?: boolean) => Promise<void>;
  startDirector: (o: DirectorOptions) => Promise<void>;
  stopDirector: () => Promise<void>;
  renderEdl: (projectPath: string, preview: boolean, container: string, openAfter: boolean) => Promise<void>;
  stopRender: () => Promise<void>;
  buildPlan: (projectPath: string, preview: boolean, container: string) => Promise<void>;
  saveEdl: (projectPath: string, e: Edl) => Promise<SanitizeReport | null>;
  parseEdl: (projectPath: string, raw: string) => Promise<SanitizeReport | null>;
  makeAutoEdl: (projectPath: string, name: string, targetS: number) => Promise<SanitizeReport | null>;
  addSource: (projectPath: string, source: string) => Promise<void>;
  exportProject: (projectPath: string) => Promise<void>;
  openFile: (path: string) => Promise<void>;
  openKdenlive: (path: string) => Promise<void>;
  installEditor: (editor: string) => Promise<void>;
  verifyLast: (outPath: string, e: Edl) => Promise<unknown>;
}

/// Notes i errors es tradueixen en el moment que passen (l'idioma es pot
/// canviar en calent), no quan es carrega el mòdul.
export const useVideoStudioStore = create<VideoStudioState>((set, get) => {
  const note = (line: string) =>
    set((s) => ({ notes: [...s.notes.slice(-24), line] }));

  /// Crida el backend amb el botó corresponent en estat «cargolant».
  const call = async <T>(
    key: string,
    cmd: string,
    args?: Record<string, unknown>
  ): Promise<T | null> => {
    set({ busy: key });
    try {
      return (await invoke<T>(cmd, args ?? {})) as T;
    } catch (e) {
      toastError(t("video.err") + " «" + cmd + "»", e);
      note(String(e));
      return null;
    } finally {
      set({ busy: null });
    }
  };

  return {
    tools: null,
    scan: null,
    edl: null,
    director: null,
    render: null,
    plan: null,
    notes: [],
    transitions: [],
    scanning: false,
    busy: null,
    ready: false,

    init: () => {
      if (get().ready) return;
      set({ ready: true });
      void listen<DirectorState>("video://director", (e) => {
        const d = e.payload;
        set({ director: d, edl: d.edl ?? get().edl });
        if (!d.running && d.outPath) {
          toastSuccess(t("video.doneToast") + " " + d.outPath.split("/").pop());
        } else if (!d.running && d.error) {
          toastError(t("video.err") + ": " + d.error);
        }
      }).catch(() => undefined);
      void listen<RenderState>("video://render", (e) => {
        const r = e.payload;
        set({ render: r });
        if (!r.running && r.error) toastError(t("video.err") + ": " + r.error);
        if (!r.running && r.outPath && !r.error)
          toastSuccess(t("video.renderDone") + " " + r.outPath.split("/").pop());
      }).catch(() => undefined);
      void listen<string>("video://note", (e) => note(String(e.payload))).catch(
        () => undefined
      );
      // El primer estat ja es demana quan s'obre el panell (loadTools/scanProject).
    },

    loadTools: async () => {
      const tools = await call<VideoTools>("tools", "video_tools");
      if (tools) set({ tools });
    },

    hydrate: async () => {
      const [d, r, tr] = await Promise.all([
        invoke<DirectorState>("video_director_state").catch(() => null),
        invoke<RenderState>("video_render_state").catch(() => null),
        invoke<string[]>("video_transitions").catch(() => [] as string[]),
      ]);
      set({
        director: d ?? get().director,
        render: r ?? get().render,
        transitions: tr ?? [],
      });
    },

    scanProject: async (projectPath, sheets = true) => {
      if (!projectPath) return;
      set({ scanning: true });
      try {
        const rep = await call<ScanReport>("scan", "video_scan", { projectPath, sheets });
        if (rep) {
          set({ scan: rep, edl: rep.edl ?? get().edl });
          if (rep.sources.length === 0) note(t("video.noSources"));
        }
      } finally {
        set({ scanning: false });
      }
    },

    startDirector: async (o) => {
      const err = await call<unknown>("director", "video_director_start", {
        projectPath: o.projectPath,
        name: o.name,
        brief: o.brief,
        sources: o.sources,
        budgetSecs: o.budgetSecs,
        targetS: o.targetS,
        provider: o.provider,
        model: o.model,
        critiques: o.critiques,
        width: o.width,
        height: o.height,
        fps: o.fps,
        crf: o.crf,
        preset: o.preset,
        container: o.container,
        silent: o.silent,
      });
      // `call` ja ha avisat si el backend ha rebutjat alguna cosa (p. ex. ja hi
      // ha un director treballant): no cal duplicar el missatge.
      if (err === null) return;
      set({
        director: {
          ...(get().director as DirectorState),
          running: true,
          stage: "inventari",
          detail: t("video.started"),
          budgetMs: o.budgetSecs * 1000,
          startedAt: Math.floor(Date.now() / 1000),
          finishedAt: null,
          outPath: null,
          error: null,
          progress: 0,
          targetS: o.targetS,
        } as DirectorState,
      });
      toastInfo(t("video.started"));
    },

    stopDirector: async () => {
      await invoke("video_director_stop").catch(() => undefined);
      note(t("video.stopping"));
    },

    renderEdl: async (projectPath, preview, container, openAfter) => {
      await call<null>("render", "video_render", {
        projectPath,
        edl: get().edl ?? undefined,
        preview,
        container,
        openAfter,
      });
    },

    stopRender: async () => {
      await invoke("video_render_stop").catch(() => undefined);
      note(t("video.stopping"));
    },

    buildPlan: async (projectPath, preview, container) => {
      const p = await call<PlanInfo>("plan", "video_plan", {
        projectPath,
        edl: get().edl ?? undefined,
        preview,
        container,
      });
      if (p) set({ plan: p });
    },

    saveEdl: async (projectPath, e) => {
      const rep = await call<SanitizeReport>("save", "video_edl_set", { projectPath, edl: e });
      if (rep) {
        set({ edl: rep.edl });
        rep.notes.forEach(note);
        if (rep.dropped > 0) note(t("video.dropped").replace("{n}", String(rep.dropped)));
      }
      return rep;
    },

    parseEdl: async (projectPath, raw) => {
      const rep = await call<SanitizeReport>("parse", "video_edl_parse", { projectPath, raw });
      if (rep) {
        set({ edl: rep.edl });
        rep.notes.forEach(note);
        toastSuccess(t("video.edlLoaded"));
      } else {
        toastError(t("video.edlBadJson"));
      }
      return rep;
    },

    makeAutoEdl: async (projectPath, name, targetS) => {
      const rep = await call<SanitizeReport>("auto", "video_auto_edl", {
        projectPath,
        name,
        targetS,
      });
      if (rep) {
        set({ edl: rep.edl });
        rep.notes.forEach(note);
      }
      return rep;
    },

    addSource: async (projectPath, source) => {
      const ok = await call<string>("add", "video_source_add", { projectPath, source });
      if (ok) await get().scanProject(projectPath, false);
    },

    exportProject: async (projectPath) => {
      const files = await call<string[]>("export", "video_export", {
        projectPath,
        edl: get().edl ?? undefined,
      });
      if (files && files.length) {
        toastSuccess(t("video.exported") + " " + files.map((f) => f.split("/").pop()).join(" · "));
        files.forEach(note);
      }
    },

    openFile: async (path) => {
      await invoke("video_open", { path }).catch((e) => toastError(t("video.err"), e));
    },

    openKdenlive: async (path) => {
      const msg = await call<string>("kdenlive", "video_open_kdenlive", { mltPath: path });
      if (msg) toastInfo(msg);
    },

    installEditor: async (editor) => {
      set({ busy: "install" });
      try {
        const msg = await invoke<string>("install_video_editor", { editor });
        toastInfo(msg);
        note(msg);
      } catch (e) {
        toastError(t("video.err"), e);
      } finally {
        set({ busy: null });
      }
    },

    /// Comprova l'últim render contra l'EDL (durada, sonoritat, plans morts).
    verifyLast: async (outPath, e) => {
      const rep = await call<unknown>("verify", "video_verify", { outPath, edl: e });
      if (rep) note(t("video.verified"));
      return rep;
    },
  };
});

/// Converteix un PNG del projecte (graella de fotogrames) en `data:` URL per
/// mostrar-lo al panell. El mateix camí que fa servir el viewport d'Unreal:
/// el plugin `fs` té permís de lectura sobre tot el disc.
export async function imageAsDataUrl(path: string): Promise<string | null> {
  try {
    const bytes = await readFile(path);
    let binary = "";
    bytes.forEach((b) => (binary += String.fromCharCode(b)));
    return "data:image/png;base64," + btoa(binary);
  } catch {
    return null;
  }
}

/// Drecera per a les accions del menú: obre el panell i (opcional) demana una
/// feina concreta al store. El `show` viu a `uiStore`, que és el que munta el
/// diàleg a `App.tsx`. Les mateixes feines són les que fa la IA des del xat amb
/// `@video:` — un sol motor, dues portes d'entrada.
export async function openVideoStudio(
  setShow: (v: boolean) => void,
  job?: "scan" | "render" | "preview" | "export" | "director" | "auto" | "kdenlive"
): Promise<void> {
  const { useWorkspaceStore } = await import("./workspaceStore");
  const root = useWorkspaceStore.getState().root;
  setShow(true);
  const v = useVideoStudioStore.getState();
  v.init();
  void v.loadTools();
  void v.hydrate();
  if (!root) {
    toastInfo(t("video.needProject"));
    return;
  }
  if (job === "export") {
    await v.exportProject(root);
    return;
  }
  if (job === "render" || job === "preview") {
    await useVideoStudioStore.getState().renderEdl(root, job === "preview", "mp4", job === "render");
    return;
  }
  if (job === "director") {
    // La IA ho fa tot sola: 40 minuts de criteri (el preset per defecte) i la
    // durada que decideixi ella. Es pot seguir i aturar des del panell.
    await useVideoStudioStore.getState().startDirector({
      projectPath: root,
      name: root.split("/").filter(Boolean).pop() ?? "Muntatge",
      brief: "",
      sources: [],
      budgetSecs: 2400,
      targetS: 0,
      critiques: 2,
      container: "mp4",
      silent: false,
    });
    return;
  }
  if (job === "auto") {
    await v.scanProject(root, false);
    await useVideoStudioStore
      .getState()
      .makeAutoEdl(root, root.split("/").filter(Boolean).pop() ?? "Muntatge", 0);
    return;
  }
  if (job === "kdenlive") {
    const st = useVideoStudioStore.getState();
    await st.exportProject(root);
    const last = useVideoStudioStore.getState().scan?.lastRender ?? null;
    if (last) {
      await st.openKdenlive(last.replace(/\.(mp4|mov|webm)$/i, ".kdenlive"));
    } else {
      toastInfo(t("video.noKdenliveProject"));
    }
    return;
  }
  await v.scanProject(root, true);
}
