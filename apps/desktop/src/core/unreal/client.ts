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

export const unrealAutoConnect = (project: string, timeout = 90) =>
  invoke<LaunchReport>("unreal_auto_connect", { project, timeout });

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
