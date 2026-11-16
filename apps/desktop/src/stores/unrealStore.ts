import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  unrealAutoConnect,
  unrealBuildLighting,
  unrealCreateBlueprint,
  unrealDetectInstallations,
  unrealFindUproject,
  unrealGetRcPort,
  unrealImportAsset,
  unrealInfo,
  unrealPackageGame,
  unrealPing,
  unrealRunPython,
  unrealSetRcPort,
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
  /// Port de la Remote Control que usa REALMENT el backend (config.toml). Es
  /// pot canviar des d'aquí: si l'editor n'usa un altre, sense això la
  /// connexió falla «en silenci» i sembla que «no passa res».
  rcPort: number;

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
  loadRcPort: () => Promise<void>;
  setRcPort: (port: number) => Promise<void>;
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

/// Promesa única d'inicialització: el doble muntatge de React StrictMode
/// no registra el listener de «unreal://build-output» dues vegades (si no,
/// cada línia de build s'afegiria duplicada al visor).
let initPromise: Promise<void> | null = null;

export const useUnrealStore = create<UnrealState>((set, get) => ({
  connected: false,
  engineVersion: null,
  projectName: null,
  installations: [],
  projectPath: null,
  rcPort: 30010,

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

  init: () => {
    if (!initPromise) {
      initPromise = (async () => {
        await listen<string>("unreal://build-output", (evt) => {
          set((s) => ({ buildOutput: s.buildOutput + evt.payload }));
        });
        await get().detectInstallations();
        await get().loadRcPort();
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
        // Guard de reentrada: si una crida es penja (l'editor no respon), no
        // apilem pings solapats cada 5 s.
        let polling = false;
        const timer = window.setInterval(async () => {
          if (get().capturing || get().building || polling) return;
          polling = true;
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
          } finally {
            polling = false;
          }
        }, 5_000);
        set({ watchdogTimer: timer });
      })().catch(() => {
        // Si la inicialització falla, la propera crida ho torna a intentar.
        initPromise = null;
      });
    }
    return initPromise!;
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

  /// Demana al backend el port EFFECTIU (no el 30010 fixe de la UI): si
  /// l'usuari el va canviar a config.toml, la interfície n'ha de mostrar el
  /// valor vertader.
  loadRcPort: async () => {
    try {
      const port = await unrealGetRcPort();
      if (port) set({ rcPort: port });
    } catch {
      /* versió antiga del backend: deixa el port per defecte */
    }
  },

  setRcPort: async (port) => {
    const net = Math.trunc(port);
    if (!Number.isFinite(net) || net < 1 || net > 65535) {
      set({
        lastError:
          "El port de Remote Control ha de ser un número entre 1 i 65535.",
      });
      return;
    }
    try {
      const efectiu = await unrealSetRcPort(net);
      set({ rcPort: efectiu, lastError: null });
      // El canvi s'aplica de seguida: els clients resolen el port a cada
      // crida. Si l'editor ja està obert, el proper ping el retrobarà.
      await get().loadRcPort();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ lastError: `No s'ha pogut desar el port: ${msg}` });
    }
  },

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
  ///
  /// Dos detalls que abans feien fallar SIEMPRE la captura: la comanda de
  /// consola anava amb `None` de context (UE5 no trobava el viewport i no
  /// feia res) i només es mirava la carpeta ja buida la primera vegada.
  captureViewport: async () => {
    if (get().capturing || !get().connected) return;
    set({ capturing: true });
    try {
      const tmp = `noorbit-ue-capture-${Date.now()}.png`;
      const code = `
import unreal, glob, os, shutil, time

world = None
try:
    world = unreal.get_editor_subsystem(unreal.LevelEditorSubsystem).get_editor_world()
except Exception:
    pass

logdir = unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_log_dir())
out = os.path.join(logdir, "noorbit-captures", r"${tmp}")
saved = unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_saved_dir())
dirs = [os.path.join(saved, nom) for nom in ("Screenshots", "ScreenShots")]
start = time.time()


def _pngs():
    trobats = set()
    for d in dirs:
        for f in glob.glob(os.path.join(d, "**", "*.png"), recursive=True):
            trobats.add(f)
    return trobats


before = _pngs()
# 1) Ordre de consola: la que funciona a totes les versions de UE5.
unreal.SystemLibrary.execute_console_command(world, "HighResShot 1920x1080")
newest = None
for _ in range(24):
    time.sleep(0.5)
    after = _pngs()
    nous = sorted(after - before, key=os.path.getmtime)
    if nous:
        newest = nous[-1]
        break
    # 2) Reserva: qualsevol PNG creat DES D'ARA (si la carpeta ja en tenia,
    #    la diferencia de llistats no en diu res).
    recents = [f for f in after if os.path.getmtime(f) >= start - 2]
    if recents:
        newest = max(recents, key=os.path.getmtime)
        break
if newest:
    if os.path.exists(out):
        os.remove(out)
    os.makedirs(os.path.dirname(out), exist_ok=True)
    shutil.copy(newest, out)
    print("NOORBIT_CAPTURE:" + out)
else:
    print(
        "NOORBIT_CAPTURE:fail (cap PNG nou a "
        + ", ".join(dirs)
        + "; cal que el viewport de l'editor estigui visible)"
    )
`;
      const res = await unrealRunPython(code);
      const outLine = res?.output.find((l) => l.includes("NOORBIT_CAPTURE:")) ?? "";
      const captured = outLine.split("NOORBIT_CAPTURE:")[1]?.trim() ?? "";
      if (res?.success && captured && !captured.startsWith("fail")) {
        // llegim el fitxer i el convertim a data URL
        const bytes = await readFile(captured);
        let binary = "";
        bytes.forEach((b) => (binary += String.fromCharCode(b)));
        const dataUrl = "data:image/png;base64," + btoa(binary);
        set({ liveCapture: dataUrl });
      } else {
        // Ara l'usuari veu el MOTIU real (abans sempre era el mateix text
        // genèric, perquè la sortida de l'script mai no arribava aquí).
        const motiu = !res
          ? "l'editor no ha respost"
          : res.success
            ? captured || "l'script no ha imprès res"
            : (res.errors ?? []).join(" ").trim() || "error Python a l'editor";
        set({
          lastError:
            `La captura del viewport ha fallat: ${motiu}. ` +
            "Activa el viewport de l'editor (no pot estar minimitzat) i torna-ho a provar.",
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
