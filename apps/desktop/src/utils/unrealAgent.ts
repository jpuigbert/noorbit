//! Utilitats pures del flux d'agent d'Unreal (fases 1-3).
//!
//! Són funcions sense efectes laterals a poder provar amb Vitest: normalitzen
//! les fases que emet el backend, filtren i agrupen el catàleg d'eines i
//! validen els arguments que es passen a una eina.

import type { UnrealMcpTool } from "../core/unreal/client";

/// Fases del bucle d'eines que emet el backend (`unreal://agent-task`).
export type UnrealAgentPhase =
  | "eines"
  | "model"
  | "context"
  | "ronda"
  | "eina"
  | "eina_ok"
  | "eina_err"
  | "answer"
  | "error"
  | "info";

export interface UnrealAgentEvent {
  id: number;
  phase: UnrealAgentPhase;
  text: string;
  tool?: string;
  turn?: number;
  ts: number;
}

/// Fases conegudes del backend: una fase nova no ha de fer caure la UI.
const KNOWN_PHASES: UnrealAgentPhase[] = [
  "eines",
  "model",
  "context",
  "ronda",
  "eina",
  "eina_ok",
  "eina_err",
  "answer",
  "error",
];

/// Normalitza la fase que ve del backend. Una fase desconeguda esdevé
/// «info»: es mostra, però no es confon amb un error.
export function normalizeAgentPhase(phase: string): UnrealAgentPhase {
  return (KNOWN_PHASES as string[]).includes(phase)
    ? (phase as UnrealAgentPhase)
    : "info";
}

/// Com es pinteja cada fase a la interfície.
export type AgentPhaseKind = "info" | "tool" | "ok" | "error" | "answer";

export function agentPhaseKind(phase: UnrealAgentPhase): AgentPhaseKind {
  switch (phase) {
    case "eina":
      return "tool";
    case "eina_ok":
      return "ok";
    case "eina_err":
    case "error":
      return "error";
    case "answer":
      return "answer";
    default:
      return "info";
  }
}

/// Prefixos d'eines que PODEN modificar l'editor (heurística, no garantia):
/// serveix per avisar l'usuari abans de la crida.
const MUTATING_PREFIXES = [
  "delete",
  "remove",
  "destroy",
  "spawn",
  "create",
  "set",
  "add",
  "apply",
  "edit",
  "modify",
  "build",
  "import",
  "run",
];

export function isMutatingTool(name: string): boolean {
  const n = (name ?? "").toLowerCase();
  return MUTATING_PREFIXES.some((p) => n.startsWith(p));
}

/// Filtra el catàleg per nom o descripció (cerca sense accents ni
/// majúscules).
export function filterTools<T extends { name: string; description?: string | null }>(
  tools: T[],
  query: string
): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return tools;
  return tools.filter(
    (t) =>
      t.name.toLowerCase().includes(q) ||
      (t.description ?? "").toLowerCase().includes(q)
  );
}

/// Agrupa les eines pel seu prefixe (`spawn_actor` → `spawn`).
export function groupToolsByPrefix<T extends { name: string }>(
  tools: T[]
): [string, T[]][] {
  const map = new Map<string, T[]>();
  for (const t of tools) {
    const prefix = t.name.split("_")[0] || "_";
    const bucket = map.get(prefix);
    if (bucket) bucket.push(t);
    else map.set(prefix, [t]);
  }
  return [...map.entries()].sort(([a], [b]) => a.localeCompare(b));
}

/// Retalla un resultat massa gran per pintar-lo sencer (el text complet
/// queda disponible per copiar-lo).
export function truncateToolResult(
  text: string,
  max: number
): { text: string; truncated: boolean } {
  if (text.length <= max) return { text, truncated: false };
  return { text: text.slice(0, max), truncated: true };
}

/// Valida els arguments JSON d'una eina: massa grans (DoS local) o syntaxi
/// incorrecta. Retorna el text d'error o `null` si tot és correcte.
export function validateToolArgs(
  raw: string,
  maxBytes: number
): { error: string | null; parsed: Record<string, unknown> | null } {
  if (raw.length > maxBytes) {
    return {
      error: `Arguments massa grans (${(raw.length / 1024).toFixed(0)} KB, màxim ${Math.round(maxBytes / 1024)} KB).`,
      parsed: null,
    };
  }
  if (!raw.trim()) return { error: null, parsed: {} };
  try {
    const parsed = JSON.parse(raw);
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
      return {
        error: "Els arguments han de ser un objecte JSON (p. ex. {\"name\": \"Cube\"}).",
        parsed: null,
      };
    }
    return { error: null, parsed: parsed as Record<string, unknown> };
  } catch {
    return { error: "Els arguments no són JSON vàlid.", parsed: null };
  }
}

/// Text curt d'un resultat d'eina per a la línia de temps (sense truncar
/// el que es pot copiar després).
export function shortResult(text: string, max = 160): string {
  const one = text.replace(/\s+/g, " ").trim();
  if (one.length <= max) return one;
  return `${one.slice(0, max)}…`;
}

/// Tipus còmodes per al catàleg d'eines.
export type { UnrealMcpTool };
