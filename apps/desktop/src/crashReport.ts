/// INFORME DE FALLIDA («crash»): el text que l'usuari copia i envia a
/// l'autor. Reuneix dues coses:
///
///   1. el que diu el BACKEND (versió, sistema, darreres fallides del
///      procés i el final del registre `crash.log`), i
///   2. el que ha passat a la INTERFÍCIE (l'error que ha fet fallar la
///      finestra i els darrers errors de la consola de depuració).
///
/// Tot plegat en un sol text Markdown: l'usuari el pot enganxar tal qual
/// en un issue de GitHub o en un correu, sense haver de buscar res pel
/// sistema ni saber on són els fitxers.
import { invoke } from "@tauri-apps/api/core";
import { copyText } from "./utils/clipboard";
import { useConsoleStore } from "./stores/consoleStore";

/// Resum de l'informe que arriba del backend.
export interface CrashReportDto {
  appVersion: string;
  os: string;
  arch: string;
  startedAt: string;
  dataDir: string | null;
  logPath: string | null;
  sessionPanics: number;
  previousPanics: number;
  lastCrashAt: string | null;
  lastCrashAgeMinutes: number | null;
  markdown: string;
}

/// El que va passar a la interfície (opcional).
export interface CrashContext {
  /// D'on ve l'error («renderitzat», «consola»…).
  source?: string;
  /// El missatge de l'error.
  message?: string;
  /// La pila de la crida (stack).
  stack?: string;
}

/// On es pot enviar l'informe.
export const CRASH_REPORT_DESTINATION = "https://github.com/jpuigbert/noorbit/issues";

/// Quants errors de la consola s'afegeixen a l'informe.
const MAX_CONSOLE_LINES = 40;

/// Llegeix l'informe del backend (null si el backend no respon).
export async function fetchCrashReport(): Promise<CrashReportDto | null> {
  try {
    return await invoke<CrashReportDto>("crash_report");
  } catch {
    return null;
  }
}

/// Una línia dels registres de la consola en una sola línia (perquè el
/// Markdown no es treni).
function flat(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

/// Secció de la interfície: l'error que ha fet fallar la finestra + els
/// darrers errors i avisos de la consola de depuració.
export function frontendSection(ctx?: CrashContext): string {
  const lines: string[] = ["## Errors de la interfície", ""];
  if (ctx?.source || ctx?.message || ctx?.stack) {
    if (ctx.source) lines.push(`**${ctx.source}**`, "");
    if (ctx.message) lines.push("```", flat(ctx.message), "```", "");
    if (ctx.stack) lines.push("```", ctx.stack.trim(), "```", "");
  }
  const errors = useConsoleStore
    .getState()
    .entries.filter((e) => e.level === "error" || e.level === "warn");
  const recent = errors.slice(-MAX_CONSOLE_LINES);
  if (recent.length > 0) {
    lines.push("### Darrers errors de la consola de depuració", "");
    for (const e of recent) {
      const at = new Date(e.at).toLocaleString("ca");
      lines.push(`- ${at} · [${e.source}] ${flat(e.text)}`);
    }
    lines.push("");
  } else {
    lines.push("(Cap error registrat a la consola de depuració.)", "");
  }
  return lines.join("\n");
}

/// Text complet de l'informe (backend + interfície + on enviar-lo).
export async function buildCrashReport(ctx?: CrashContext): Promise<string> {
  const report = await fetchCrashReport();
  const head =
    report?.markdown ??
    "# Informe de fallida de NoOrbit\n\n(no s'ha pogut llegir el registre del backend: " +
      "l'aplicació potser s'ha tancat de cop)";
  return [
    head,
    "",
    frontendSection(ctx),
    "---",
    "",
    `Envia aquest informe a: ${CRASH_REPORT_DESTINATION}`,
    "",
  ].join("\n");
}

/// Còpia l'informe al porta-retalls. Retorna true si s'ha copiat.
export async function copyCrashReport(ctx?: CrashContext): Promise<boolean> {
  try {
    return await copyText(await buildCrashReport(ctx));
  } catch {
    return false;
  }
}
