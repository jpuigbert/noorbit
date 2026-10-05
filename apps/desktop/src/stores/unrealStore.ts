import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  unrealAutoConnect,
  unrealBuildLighting,
  unrealCreateBlueprint,
  unrealDetectInstallations,
  unrealImportAsset,
  unrealInfo,
  unrealPackageGame,
  unrealPing,
  unrealRunPython,
  unrealSpawnActor,
  PythonResult,
  PackageOptions,
  BuildResult,
} from "../core/unreal/client";

export type UnrealConnectStatus =
  | "idle"
  | "detecting"
  | "launching"
  | "waiting"
  | "connecting"
  | "connected"
  | "error";

interface UnrealState {
  connected: boolean;
  engineVersion: string | null;
  projectName: string | null;
  installations: string[];
  projectPath: string | null;

  status: UnrealConnectStatus;
  statusMessage: string;
  lastError: string | null;

  lastPythonResult: PythonResult | null;
  lastBuildResult: BuildResult | null;
  buildOutput: string;
  building: boolean;

  liveCapture: string | null; // data URL de la última captura del viewport
  capturing: boolean;

  init: () => Promise<void>;
  detectInstallations: () => Promise<void>;
  setProjectPath: (path: string | null) => void;
  connect: (timeout?: number) => Promise<void>;
  disconnect: () => void;

  runPython: (code: string) => Promise<PythonResult | null>;
  createBlueprint: (
    name: string,
    parent: string
  ) => Promise<PythonResult | null>;
  spawnActor: (
    classPath: string,
    location: [number, number, number]
  ) => Promise<PythonResult | null>;
  importAsset: (file: string, dest: string) => Promise<PythonResult | null>;
  buildLighting: (quality: string) => Promise<PythonResult | null>;
  captureViewport: () => Promise<void>;

  packageGame: (engineRoot: string, opts: PackageOptions) => Promise<void>;
  clearBuildOutput: () => void;
}

export const useUnrealStore = create<UnrealState>((set, get) => ({
  connected: false,
  engineVersion: null,
  projectName: null,
  installations: [],
  projectPath: null,

  status: "idle",
  statusMessage: "",
  lastError: null,

  lastPythonResult: null,
  lastBuildResult: null,
  buildOutput: "",
  building: false,

  liveCapture: null,
  capturing: false,

  init: async () => {
    listen<string>("unreal://build-output", (evt) => {
      set((s) => ({ buildOutput: s.buildOutput + evt.payload }));
    });
    await get().detectInstallations();
    // Prova de detectar connexió existent
    try {
      const ok = await unrealPing();
      if (ok) {
        const info = await unrealInfo();
        set({
          connected: true,
          status: "connected",
          engineVersion: info.engine_version,
          projectName: info.project_name,
        });
      }
    } catch {
      /* no connectat */
    }
  },

  detectInstallations: async () => {
    try {
      const installations = await unrealDetectInstallations();
      set({ installations });
    } catch {
      set({ installations: [] });
    }
  },

  setProjectPath: (path) => set({ projectPath: path }),

  connect: async (timeout = 90) => {
    const setStatus = (s: UnrealConnectStatus, msg: string) =>
      set({ status: s, statusMessage: msg, lastError: null });

    try {
      setStatus("detecting", "Detectant Unreal Engine…");

      const alreadyOpen = await unrealPing();
      if (alreadyOpen) {
        setStatus("connecting", "Connectant…");
        const info = await unrealInfo();
        set({
          connected: true,
          status: "connected",
          statusMessage: "Connectat",
          engineVersion: info.engine_version,
          projectName: info.project_name,
        });
        return;
      }

      const project = get().projectPath;
      if (!project) {
        throw new Error(
          "Selecciona un fitxer .uproject primer.\n" +
            "Usa el botó 'Seleccionar projecte'."
        );
      }

      setStatus("waiting", "Arrencant editor i esperant Remote Control API…");
      const report = await unrealAutoConnect(project, timeout);

      const info = await unrealInfo();
      set({
        connected: true,
        status: "connected",
        statusMessage: report.message,
        engineVersion: info.engine_version,
        projectName: info.project_name,
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({
        connected: false,
        status: "error",
        statusMessage: msg,
        lastError: msg,
      });
    }
  },

  disconnect: () =>
    set({
      connected: false,
      status: "idle",
      statusMessage: "",
      lastError: null,
    }),

  runPython: async (code) => {
    try {
      const res = await unrealRunPython(code);
      set({ lastPythonResult: res });
      return res;
    } catch (e) {
      set({ lastError: String(e) });
      return null;
    }
  },

  createBlueprint: async (name, parent) => {
    try {
      const res = await unrealCreateBlueprint(name, parent);
      set({ lastPythonResult: res });
      return res;
    } catch (e) {
      set({ lastError: String(e) });
      return null;
    }
  },

  spawnActor: async (classPath, location) => {
    try {
      const res = await unrealSpawnActor(classPath, location);
      set({ lastPythonResult: res });
      return res;
    } catch (e) {
      set({ lastError: String(e) });
      return null;
    }
  },

  importAsset: async (file, dest) => {
    try {
      const res = await unrealImportAsset(file, dest);
      set({ lastPythonResult: res });
      return res;
    } catch (e) {
      set({ lastError: String(e) });
      return null;
    }
  },

  buildLighting: async (quality) => {
    try {
      const res = await unrealBuildLighting(quality);
      set({ lastPythonResult: res });
      return res;
    } catch (e) {
      set({ lastError: String(e) });
      return null;
    }
  },

  /// Captura el viewport de l'editor i la mostra com a imatge.
  captureViewport: async () => {
    if (get().capturing || !get().connected) return;
    set({ capturing: true });
    try {
      const tmp = `/tmp/noorbit_ue_capture_${Date.now()}.png`;
      const code = `
import unreal
ok = unreal.GameEditorLibrary.high_res_screenshot(r"${tmp}", 1920, 1080)
print("capture:" + ("ok" if ok else "fail"))
`;
      const res = await unrealRunPython(code);
      if (res && res.success) {
        // llegim el fitxer i el convertim a data URL
        const bytes = await readFile(tmp);
        let binary = "";
        bytes.forEach((b) => (binary += String.fromCharCode(b)));
        const dataUrl = "data:image/png;base64," + btoa(binary);
        set({ liveCapture: dataUrl });
      } else {
        set({ lastError: "La captura del viewport ha fallat" });
      }
    } catch (e) {
      set({ lastError: String(e) });
    } finally {
      set({ capturing: false });
    }
  },

  packageGame: async (engineRoot, opts) => {
    set({ building: true, buildOutput: "", lastBuildResult: null });
    try {
      const res = await unrealPackageGame(engineRoot, opts);
      set({ lastBuildResult: res });
    } finally {
      set({ building: false });
    }
  },

  clearBuildOutput: () => set({ buildOutput: "", lastBuildResult: null }),
}));
