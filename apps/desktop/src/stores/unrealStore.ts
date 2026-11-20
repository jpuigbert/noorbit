import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  unrealAgentSetup,
  unrealAgentTask,
  unrealAutoConnect,
  unrealBuildLighting,
  unrealCreateBlueprint,
  unrealDetectInstallations,
  unrealDetectPendingInstallations,
  unrealFindUproject,
  unrealGetRcPort,
  unrealImportAsset,
  unrealInfo,
  unrealMcpCall,
  unrealMcpTools,
  unrealPackageGame,
  unrealPing,
  unrealRunPython,
  unrealSetRcPort,
  unrealSpawnActor,
  PythonResult,
  PackageOptions,
  BuildResult,
  UnrealSetupStep,
  UnrealSetupReport,
  UnrealMcpTool,
  UnrealMcpCallResult,
} from "../core/unreal/client";
import { useWorkspaceStore } from "./workspaceStore";
import { t } from "../i18n";
import {
  normalizeAgentPhase,
  type UnrealAgentEvent,
} from "../utils/unrealAgent";

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
  /// Instal·lacions A MITGES (l'Epic Launcher encara baixant el motor). No
  /// es poden fer servir, però es mostren perquè l'usuari sap que el motor
  /// hi és i que la descàrrega no s'ha acabat.
  pendingInstallations: string[];
  /// Carpeta del motor triada a mà. Vacua = detecció automàtica (que ja
  /// escaneja els discs externs i les unitats montades).
  engineRoot: string | null;
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

  /// Agent MCP (Unreal 5.8+): les tres fases del flux d'un sol clic.
  ///
  /// FASE 1 (setup): passos del preparati, informe final i bloqueig de
  /// reintent després d'un timeout (evita arrencar DOS editors).
  setupSteps: UnrealSetupStep[];
  setupRunning: boolean;
  setupReport: UnrealSetupReport | null;
  setupError: string | null;
  setupProject: string | null;
  setupStartedAt: number | null;
  setupFinishedAt: number | null;
  setupAbandonedAt: number | null;

  /// FASE 2 (descobriment): catàleg d'eines MCP i execució puntual.
  tools: UnrealMcpTool[];
  toolsLoading: boolean;
  toolsError: string | null;
  toolsLoadedAt: number | null;

  /// FASE 3 (execució): el bucle d'eines de l'agent, en directe.
  taskEvents: UnrealAgentEvent[];
  taskRunning: boolean;
  taskAnswer: string | null;
  taskError: string | null;

  init: () => Promise<void>;
  detectInstallations: () => Promise<void>;
  loadRcPort: () => Promise<void>;
  setRcPort: (port: number) => Promise<void>;
  setProjectPath: (path: string | null) => void;
  /// Tria la carpeta del motor d'Unreal a mà (disc extern o lloc rar).
  setEngineRoot: (path: string | null) => void;
  connect: (timeout?: number) => Promise<void>;
  disconnect: () => void;

  /// FASE 1: prepara TOT l'entorn d'un sol clic (Ollama, plugins MCP,
  /// auto-arrencada del servidor i handshake). Amb timeout i bloqueig de
  /// reintent: si es quedat penjat, un altre intent obriria un segon
  /// editor d'Unreal (doble RAM i dos ports MCP).
  runOneClickSetup: (opts?: {
    launch?: boolean;
    project?: string;
  }) => Promise<void>;
  clearSetup: () => void;

  /// FASE 2: catàleg d'eines MCP (amb càch d'un minut) i crida puntual.
  loadTools: (force?: boolean) => Promise<void>;
  callTool: (name: string, args: unknown) => Promise<UnrealMcpCallResult>;

  /// FASE 3: xat lliure amb l'agent (bucle d'eines del model local).
  runTask: (prompt: string) => Promise<void>;
  clearTask: () => void;

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

/// El setup pot trigar minuts (arrencada de l'editor, compilació de shaders):
/// sis minuts de marge abans de declarar-lo penjat.
const SETUP_TIMEOUT_MS = 360_000;
/// Després d'un timeout NO es pot reintentar immediatament: el backend
/// arrenca l'editor i un segon intent obriria un SEGON editor (doble RAM i
/// dos ports MCP al mateix temps).
const RETRY_BLOCK_MS = 60_000;
/// El catàleg d'eines canva poc: es torna a demanar com a molt d'un minut.
const TOOLS_CACHE_MS = 60_000;
/// Límit dels arguments d'una eina: un JSON de 50 MB bloqueja el fil
/// principal del navegador (DoS local).
const MAX_ARGS_BYTES = 100_000;
/// Esdeveniments del bucle d'eines conservats en memòria.
const MAX_TASK_EVENTS = 300;

/// El setup ja està en marxa (evita dues crides simultànies al backend).
let setupInFlight = false;
/// Comptador de ids dels esdeveniments de l'agent.
let eventSeq = 0;
const nextEventId = () => ++eventSeq;

/// Resol el `.uproject`: el triat a mà o el detectat dins del workspace.
async function resolveUproject(get: () => UnrealState): Promise<string | null> {
  const chosen = get().projectPath;
  if (chosen) return chosen;
  const root = useWorkspaceStore.getState().root;
  if (!root) return null;
  try {
    const found = await unrealFindUproject(root);
    if (found) {
      get().setProjectPath(found);
      return found;
    }
  } catch {
    /* sense .uproject: el caller avisa */
  }
  return null;
}

export const useUnrealStore = create<UnrealState>((set, get) => ({
  connected: false,
  engineVersion: null,
  projectName: null,
  installations: [],
  pendingInstallations: [],
  engineRoot: null,
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

  setupSteps: [],
  setupRunning: false,
  setupReport: null,
  setupError: null,
  setupProject: null,
  setupStartedAt: null,
  setupFinishedAt: null,
  setupAbandonedAt: null,

  tools: [],
  toolsLoading: false,
  toolsError: null,
  toolsLoadedAt: null,

  taskEvents: [],
  taskRunning: false,
  taskAnswer: null,
  taskError: null,

  init: () => {
    if (!initPromise) {
      initPromise = (async () => {
        await listen<string>("unreal://build-output", (evt) => {
          set((s) => ({ buildOutput: s.buildOutput + evt.payload }));
        });
        // Passos de l'orquestrador «Activar Agent Unreal» (✓/✗ per pas).
        await listen<UnrealSetupStep>("unreal://agent-setup", (evt) => {
          const s = evt.payload;
          set((st) => ({
            setupSteps: [...st.setupSteps, s].slice(-20),
          }));
        });
        // Progrés del bucle d'eines de l'agent (fase + text + eina + ronda).
        await listen<{
          phase: string;
          text: string;
          tool?: string;
          turn?: number;
        }>("unreal://agent-task", (evt) => {
          const p = evt.payload;
          const ev: UnrealAgentEvent = {
            id: nextEventId(),
            phase: normalizeAgentPhase(p.phase),
            text: p.text ?? "",
            tool: p.tool,
            turn: p.turn,
            ts: Date.now(),
          };
          if (ev.phase === "answer") {
            set((st) => ({
              taskEvents: [...st.taskEvents, ev].slice(-MAX_TASK_EVENTS),
              taskAnswer: ev.text,
            }));
            return;
          }
          if (ev.phase === "error") {
            set((st) => ({
              taskEvents: [...st.taskEvents, ev].slice(-MAX_TASK_EVENTS),
              taskError: ev.text,
            }));
            return;
          }
          set((st) => ({
            taskEvents: [...st.taskEvents, ev].slice(-MAX_TASK_EVENTS),
          }));
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

    // Les instal·lacions a mitges van a part: el backend pot ser vell i no
    // conèixer la comanda, i en aquest cas simplement no se'n mostra cap.
    try {
      const pending = (await unrealDetectPendingInstallations()) ?? [];
      set({
        pendingInstallations: Array.isArray(pending) ? pending : [],
      });
    } catch {
      set({ pendingInstallations: [] });
    }
  },

  setProjectPath: (path) => set({ projectPath: path }),

  setEngineRoot: (path) => set({ engineRoot: path }),

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
      // El motor triat a mà té prioritat (disc extern); si no hi ha, el
      // backend el detecta sol (també a les unitats montades).
      const report = await unrealAutoConnect(project, timeout, get().engineRoot);

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

  /// FASE 1 · SETUP — un sol clic prepara TOT l'entorn (Ollama, plugins MCP
  /// al .uproject, auto-arrencada del servidor i handshake). Idempotent.
  ///
  /// Amb timeout propi: si es queda penjat, es bloqueja el reintent (un altre
  /// intent obriria un SEGON editor d'Unreal: doble RAM i dos ports MCP).
  runOneClickSetup: async (opts) => {
    if (setupInFlight) return;

    const { setupAbandonedAt } = get();
    if (setupAbandonedAt && Date.now() - setupAbandonedAt < RETRY_BLOCK_MS) {
      const remaining = Math.ceil(
        (RETRY_BLOCK_MS - (Date.now() - setupAbandonedAt)) / 1000
      );
      set({ setupError: t("unreal.agent.setup.retryBlocked", { s: remaining }) });
      return;
    }

    setupInFlight = true;
    set({
      setupRunning: true,
      setupSteps: [],
      setupReport: null,
      setupError: null,
      setupStartedAt: Date.now(),
      setupFinishedAt: null,
      setupAbandonedAt: null,
    });

    let timer: ReturnType<typeof setTimeout> | null = null;
    let timedOut = false;
    try {
      const project = opts?.project ?? (await resolveUproject(get));
      if (!project) {
        throw new Error(t("unreal.agent.setup.noProject"));
      }
      set({ setupProject: project });

      const invokePromise = unrealAgentSetup(project, opts?.launch ?? true);
      // El rebuig tardà es captura aquí: si no, saltaria com a
      // unhandledRejection quan el timeout guanyi la carrera.
      invokePromise.catch(() => {
        /* l'error real el mostra el Promise.race */
      });
      const timeoutPromise = new Promise<never>((_, reject) => {
        timer = setTimeout(() => {
          timedOut = true;
          reject(
            new Error(
              t("unreal.agent.setup.timeout", {
                min: Math.round(SETUP_TIMEOUT_MS / 60_000),
              })
            )
          );
        }, SETUP_TIMEOUT_MS);
      });

      const report = await Promise.race([invokePromise, timeoutPromise]);
      set({ setupReport: report });
      if (!report.ok) set({ setupError: report.message });
      // Setup correcte: el catàleg d'eines ja és disponible.
      if (report.ok && report.mcp_connected) {
        void get().loadTools(true);
      }
    } catch (e) {
      set({
        setupError: e instanceof Error ? e.message : String(e),
        // Només un timeout bloqueja el reintent: un error normal es pot
        // corregir i tornar a provar de seguida.
        setupAbandonedAt: timedOut ? Date.now() : null,
      });
    } finally {
      if (timer) clearTimeout(timer);
      setupInFlight = false;
      set({ setupRunning: false, setupFinishedAt: Date.now() });
    }
  },

  clearSetup: () => {
    if (setupInFlight) return;
    set({
      setupSteps: [],
      setupReport: null,
      setupError: null,
      setupProject: null,
      setupStartedAt: null,
      setupFinishedAt: null,
      setupAbandonedAt: null,
    });
  },

  /// FASE 2 · DESCOBERIMENT — catàleg d'eines MCP de l'editor (amb càch
  /// d'un minut: no canvia entre crides).
  loadTools: async (force = false) => {
    const { tools, toolsLoadedAt, toolsLoading } = get();
    if (toolsLoading) return;
    if (
      !force &&
      tools.length > 0 &&
      toolsLoadedAt &&
      Date.now() - toolsLoadedAt < TOOLS_CACHE_MS
    ) {
      return;
    }
    set({ toolsLoading: true, toolsError: null });
    try {
      const list = await unrealMcpTools();
      set({
        tools: list ?? [],
        toolsLoading: false,
        toolsLoadedAt: Date.now(),
      });
    } catch (e) {
      set({
        toolsLoading: false,
        toolsError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  /// Crida puntual d'una eina: primer es valida que existeix al catàleg
  /// (no es crida res que el servidor no hagi anunciat) i que els arguments
  /// no siguin un JSON gegant.
  callTool: async (name, args) => {
    const known = get().tools.some((tool) => tool.name === name);
    if (!known) {
      throw new Error(t("unreal.agent.tools.unknownTool", { name }));
    }
    const json = JSON.stringify(args ?? {});
    if (json.length > MAX_ARGS_BYTES) {
      throw new Error(
        t("unreal.agent.tools.argsTooBig", {
          kb: Math.round(json.length / 1024),
          max: Math.round(MAX_ARGS_BYTES / 1024),
        })
      );
    }
    return unrealMcpCall(name, (args ?? {}) as Record<string, unknown>);
  },

  /// FASE 3 · EXECUCIÓ — una tasca d'alt nivell («crea 5 llums en fila…»):
  /// el model local tria les eines MCP i les crida fins a la resposta final.
  /// El bucle es veu en directe (esdeveniment `unreal://agent-task`).
  runTask: async (prompt) => {
    if (get().taskRunning) return;
    set({
      taskRunning: true,
      taskAnswer: null,
      taskError: null,
      taskEvents: [],
    });
    try {
      const answer = await unrealAgentTask(prompt);
      set({ taskAnswer: answer });
    } catch (e) {
      set({ taskError: e instanceof Error ? e.message : String(e) });
    } finally {
      set({ taskRunning: false });
    }
  },

  clearTask: () =>
    set({ taskEvents: [], taskAnswer: null, taskError: null }),

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
