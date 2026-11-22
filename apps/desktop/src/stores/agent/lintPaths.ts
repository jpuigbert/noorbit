/// Wrapper TOLERANT del comandament `lint_paths` del backend (Fase 1).
/// Contracte: invoke("lint_paths", { projectPath, paths }) →
/// Array<{ path, ok, output }>. El backend executa `php -l` per als .php i
/// `node --check` per als .js; la resta d'extensions tornen ok:true.
///
/// Si el comandament falla o ENCARA NO EXISTEIX (backend antic o registre en
/// curs), NO bloquegem el flux: tornem `available: false` i el cridaner ho
/// mostra com a nota visible (degradació honesta, mai un fals «verd»).

import { invoke } from "@tauri-apps/api/core";

export interface LintPathResult {
  path: string;
  ok: boolean;
  output: string;
}

export interface LintOutcome {
  /// Cert si el backend ha respost (encara que hi hagi errors de lint).
  available: boolean;
  results: LintPathResult[];
}

export async function lintPaths(projectPath: string, paths: string[]): Promise<LintOutcome> {
  if (paths.length === 0) return { available: true, results: [] };
  try {
    const results = await invoke<LintPathResult[]>("lint_paths", { projectPath, paths });
    if (!Array.isArray(results)) return { available: false, results: [] };
    return { available: true, results };
  } catch {
    // Comandament no registrat o error del backend: degradació amb nota.
    return { available: false, results: [] };
  }
}