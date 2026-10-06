/// Magatzem de PROBLEMES (diagnòstics) de tot el projecte, a l'estil del panell
/// «Problems» de VS Code / Sublime. Recull dos tipus de marques:
///   • Les NATIVES de Monaco (TypeScript/JS/JSON/CSS/HTML), que el worker del
///     propi editor calcula en escriure.
///   • Les del nostre MOTOR LLEUGER (python, rust, c…), vege editor/diagnostics.
/// Totes es listenen via «onDidChangeMarkers», així el panell i l'esquiggle del
/// editor queden sempre sincronitzats sense duplicar lògica.
import { create } from "zustand";

export type ProblemSeverity = "error" | "warning" | "info";

export interface Problem {
  /// Ruta ABSOLUTA del fitxer (URI de Monaco sense l'esquema «file://»).
  path: string;
  name: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  message: string;
  severity: ProblemSeverity;
  /// Qui ho ha detectat: «typescript», «json», «noorbit-lint»…
  source: string;
}

interface ProblemsState {
  /// Problemes agrupats per ruta de fitxer.
  byFile: Record<string, Problem[]>;
  setFileProblems: (path: string, problems: Problem[]) => void;
  clearAll: () => void;
  /// Total d'errors i avisos arreu del projecte (per al comptador de la barra).
  counts: () => { errors: number; warnings: number };
}

export const useProblemsStore = create<ProblemsState>((set, get) => ({
  byFile: {},

  setFileProblems: (path, problems) =>
    set((s) => {
      const next = { ...s.byFile };
      if (problems.length === 0) delete next[path];
      else next[path] = problems;
      return { byFile: next };
    }),

  clearAll: () => set({ byFile: {} }),

  counts: () => {
    let errors = 0;
    let warnings = 0;
    for (const list of Object.values(get().byFile)) {
      for (const p of list) {
        if (p.severity === "error") errors++;
        else if (p.severity === "warning") warnings++;
      }
    }
    return { errors, warnings };
  },
}));

/// Ordre per gravetat: primer els errors, després les línies.
export function sortProblems(list: Problem[]): Problem[] {
  const rank = { error: 0, warning: 1, info: 2 } as const;
  return [...list].sort(
    (a, b) =>
      rank[a.severity] - rank[b.severity] ||
      a.line - b.line ||
      a.column - b.column
  );
}
