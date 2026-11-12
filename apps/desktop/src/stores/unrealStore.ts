import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  unrealAutoConnect,
  unrealBuildLighting,
  unrealCreateBlueprint,
  unrealDetectInstallations,
  unrealFindUproject,
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
import { useWorkspaceStore } from "./workspaceStore";

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
  /// Rellotge del watchdog de connexió (ping cada 5 s); null si no corre.
  watchdogTimer: number | null;

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
  watchdogTimer: null,

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
    // WATCHDOG DE CONNEXIÓ: cada 5 s comprova si l'editor està en marxa.
    // Així la UI detecta SOLA quan l'usuari obre Unreal (o el RC el connecta)
    // i marca «desconnectat» quan el tanca, sense que ningú cli res.
    if (get().watchdogTimer) return;
    const timer = window.setInterval(async () => {
      if (get().capturing || get().building) return;
      try {
        const ok = await unrealPing();
        if (ok !== get().connected) {
          if (ok) {
            const info = await unrealInfo();
            set({
              connected: true,
              status: "connected",
              statusMessage: "",
              engineVersion: info.engine_version,
              projectName: info.project_name,
            });
          } else {
            set({
              connected: false,
              status: "idle",
              statusMessage: "",
              engineVersion: null,
              projectName: null,
            });
          }
        } else if (ok) {
          // Connectat: refresca metadades (l'usuari pot canviar de nivell).
          const info = await unrealInfo();
          if (
            info.engine_version !== get().engineVersion ||
            info.project_name !== get().projectName
          ) {
            set({ engineVersion: info.engine_version, projectName: info.project_name });
          }
        }
      } catch {
        /* ping fallat: el següent tick ho repetirà */
      }
    }, 5_000);
    set({ watchdogTimer: timer });
  },

  detectInstallations: async () => {
    try {
      // El backend pot tornar null (versió antiga o error): la interfície
      // ha de poder dibuixar-se igual — mai un null aquí, que tombava tota
      // la finestra del panell d'Unreal.
      const installations = (await unrealDetectInstallations()) ?? [];
      set({ installations: Array.isArray(installations) ? installations : [] });
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

      // AUTO-DETECCIÓ del projecte: si l'usuari no n'ha triat cap, mirem si
      // la carpeta de treball oberta conté un .uproject (arrel + un nivell).
      let project = get().projectPath;
      if (!project) {
        const root = useWorkspaceStore.getState().root;
        if (root) {
          try {
            const found = await unrealFindUproject(root);
            if (found) {
              project = found;
              set({ projectPath: found });
            }
          } catch {
            /* sense .uproject: mantén el error d'abans */
          }
        }
      }
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
  ///
  /// Mètode ROBUST: la comanda de consola «HighResShot» (funciona a totes les
  /// versions de UE5, mentre que les APIs de captura per Python canvien), i
  /// després copiem el PNG MÉS NOU de Saved/Screenshots a una ruta temporal
  /// real del sistema (no «/tmp»: a Windows no existeix).
  captureViewport: async () => {
    if (get().capturing || !get().connected) return;
    set({ capturing: true });
    try {
      const tmp = `noorbit-ue-capture-${Date.now()}.png`;
      const code = `
import unreal, glob, os, shutil, time
logdir = unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_log_dir())
out = os.path.join(logdir, "noorbit-captures", r"${tmp}")
if os.path.exists(out):
    os.remove(out)
os.makedirs(os.path.dirname(out), exist_ok=True)
before = set()
saved = unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_saved_dir())
for f in glob.glob(os.path.join(saved, "Screenshots", "**", "*.png"), recursive=True):
    before.add(f)
unreal.SystemLibrary.execute_console_command(None, "HighResShot 1920x1080")
time.sleep(2.5)
newest = None
for _ in range(10):
    files = [f for f in glob.glob(os.path.join(saved, "Screenshots", "**", "*.png"), recursive=True) if f not in before]
    if files:
        newest = max(files, key=os.path.getmtime)
        break
    time.sleep(0.5)
if newest:
    shutil.copy(newest, out)
    print("NOORBIT_CAPTURE:" + out)
else:
    print("NOORBIT_CAPTURE:fail")
`;
      const res = await unrealRunPython(code);
      const outLine = res?.output.find((l) => l.includes("NOORBIT_CAPTURE:")) ?? "";
      const captured = outLine.split("NOORBIT_CAPTURE:")[1]?.trim();
      if (res?.success && captured && captured !== "fail") {
        // llegim el fitxer i el convertim a data URL
        const bytes = await readFile(captured);
        let binary = "";
        bytes.forEach((b) => (binary += String.fromCharCode(b)));
        const dataUrl = "data:image/png;base64," + btoa(binary);
        set({ liveCapture: dataUrl });
      } else {
        set({
          lastError:
            "La captura del viewport ha fallat. Obre el viewport de l'editor (no pot estar minimitzat) i torna-ho a provar.",
        });
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
