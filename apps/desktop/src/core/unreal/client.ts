import { invoke } from "@tauri-apps/api/core";

export interface UnrealConfig {
  host: string;
  port: number;
}

export const DEFAULT_UNREAL_CONFIG: UnrealConfig = {
  host: "127.0.0.1",
  port: 30010,
};

export interface UnrealStatus {
  connected: boolean;
  engine_version: string | null;
  project_name: string | null;
}

export interface LaunchReport {
  engine_path: string;
  project_path: string;
  launched: boolean;
  message: string;
}

export interface PythonResult {
  success: boolean;
  output: string[];
  errors: string[];
}

export interface PackageOptions {
  project: string;
  platform: string;
  configuration: string;
  output_dir?: string;
}

export interface BuildResult {
  success: boolean;
  output: string;
  output_path: string | null;
}

export const unrealPing = () =>
  invoke<boolean>("unreal_ping", { config: DEFAULT_UNREAL_CONFIG });

export const unrealInfo = () =>
  invoke<UnrealStatus>("unreal_info", { config: DEFAULT_UNREAL_CONFIG });

export const unrealDetectInstallations = () =>
  invoke<string[]>("unreal_detect_installations");

/// Instal·lacions A MITGES (l'Epic Launcher encara baixant el motor): no
/// serveixen per connectar, però l'UI les mostra perquè l'usuari sap on
/// les té i per què NoOrbit diu que no ha trobat cap motor.
export const unrealDetectPendingInstallations = () =>
  invoke<string[]>("unreal_detect_pending_installations");

/// Connecta (i arrenca si cal) l'editor amb un projecte.
///
/// `engineRoot` és opcional: carpeta del motor triada a mà (p. ex. un Unreal
/// instal·lat en un disc extern). Si no es passa, el backend ho detecta sol.
export const unrealAutoConnect = (
  project: string,
  timeout = 90,
  engineRoot?: string | null
) =>
  invoke<LaunchReport>("unreal_auto_connect", {
    project,
    timeout,
    engineRoot: engineRoot ?? null,
  });

export const unrealRunPython = (code: string) =>
  invoke<PythonResult>("unreal_run_python", {
    config: DEFAULT_UNREAL_CONFIG,
    code,
  });

export const unrealCreateBlueprint = (name: string, parentClass: string) =>
  invoke<PythonResult>("unreal_create_blueprint", {
    config: DEFAULT_UNREAL_CONFIG,
    name,
    parentClass,
  });

export const unrealSpawnActor = (
  classPath: string,
  location: [number, number, number]
) =>
  invoke<PythonResult>("unreal_spawn_actor", {
    config: DEFAULT_UNREAL_CONFIG,
    classPath,
    location,
  });

export const unrealImportAsset = (filePath: string, destPath: string) =>
  invoke<PythonResult>("unreal_import_asset", {
    config: DEFAULT_UNREAL_CONFIG,
    filePath,
    destPath,
  });

export const unrealBuildLighting = (quality: string) =>
  invoke<PythonResult>("unreal_build_lighting", {
    config: DEFAULT_UNREAL_CONFIG,
    quality,
  });

export const unrealPackageGame = (
  engineRoot: string,
  options: PackageOptions
) => invoke<BuildResult>("unreal_package_game", { engineRoot, options });

/// Cerca un .uproject dins la carpeta del projecte obert (arrel + un nivell):
/// serveix per auto-omplir el projecte d'Unreal sense haver de seleccionar-lo.
export const unrealFindUproject = (root: string) =>
  invoke<string | null>("unreal_find_uproject", { root });

/// Port REAL de la Remote Control (el de la configuració, no sempre 30010):
/// la UI el mostra i el deixa canviar, perquè un editor amb un altre port
/// connecta en cas contrari «en silenci» sense que ningú sàpiga contra què
/// s'està intentant.
export const unrealGetRcPort = () => invoke<number>("unreal_get_rc_port");

export const unrealSetRcPort = (port: number) =>
  invoke<number>("unreal_set_rc_port", { port });

// ── Agent MCP (plugin oficial d'Unreal 5.8+) ──────────────────────────────

export interface UnrealSetupStep {
  name: string;
  ok: boolean;
  message: string;
}

export interface UnrealSetupReport {
  steps: UnrealSetupStep[];
  ok: boolean;
  mcp_connected: boolean;
  tools: number;
  message: string;
}

export interface UnrealMcpTool {
  name: string;
  description: string | null;
  input_schema?: unknown;
}

export interface UnrealMcpCallResult {
  text: string;
  is_error: boolean;
}

/// ORQUESTRADOR «Activar Agent Unreal»: prepara l'entorn en segon pla
/// (Ollama, plugins MCP, auto-arrencada del servidor, handshake MCP).
export const unrealAgentSetup = (project: string, launch: boolean) =>
  invoke<UnrealSetupReport>("unreal_agent_setup", { project, launch });

/// Executa una tasca d'alt nivell contra l'editor (el model local tria les
/// eines MCP rellevants i les crida en un bucle fins a la resposta final).
export const unrealAgentTask = (prompt: string) =>
  invoke<string>("unreal_agent_task", { prompt });

/// Catàleg d'eines MCP de l'editor.
export const unrealMcpTools = () =>
  invoke<UnrealMcpTool[]>("unreal_mcp_tools");

/// Executa una eina MCP concreta (depuració / ordres puntuals).
export const unrealMcpCall = (name: string, args: Record<string, unknown>) =>
  invoke<UnrealMcpCallResult>("unreal_mcp_call", { name, args });
