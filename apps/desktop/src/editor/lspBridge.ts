/// PONT LSP del frontend (Fase 3c): connecta Monaco amb els comandaments
/// `lsp_*` del backend (sessió persistent, degradació honesta).
///
///  • Completion/hover: NOMÉS per a llenguatges amb servidor DETECTAT
///    (rust, php, python…). TS/JS NO es dupliquen: Monaco ja fa IntelliSense
///    amb els seus workers; per a ells només aportem els diagnòstics de
///    PROJECTE del tsserver (geterr), que els workers no calculen.
///  • Diagnòstics: `scheduleLspDiagnostics` amb debounce + generació
///    (cancel·lació de consultes en vol) → markers amb owner «noorbit-lsp»,
///    que conviuen amb els workers natius i amb el «php -l» (noorbit-lint).
///  • Referències: `lspReferences` per a l'acció «Ves a referències» (⌥F12).
///
/// Tots els wrappers degraden a llista buida / null / estat buit: MAI un
/// error que trenque l'editor (el backend ja ho fa; ací, doble xarxa).
import { invoke } from "@tauri-apps/api/core";
import type { editor as E, languages as L, IRange, Position, CancellationToken } from "monaco-editor";
import { applyLspMarkers } from "./diagnostics";

// ---- Contracte EXACTE amb el backend (lsp/commands.rs) ----------------------

export interface LspCompletion {
  label: string;
  insertText: string;
  detail: string;
  /// CompletionItemKind numèric LSP (1-based; Monaco té un altre ordre).
  kind: number;
}

export interface LspHover {
  contents: string;
}

export interface LspRef {
  /// Ruta RELATIVA a l'arrel del projecte (o absoluta si és fora).
  path: string;
  line: number;
  col: number;
}

/// Definició real del servidor LSP (`lsp_definition`, Fase 3): coordenades
/// 1-based com la resta del contracte; `preview` és la línia del destí.
export interface LspDef {
  path: string;
  line: number;
  column: number;
  preview: string;
}

/// Resultat d'un renombrament aplicat (`lsp_rename`): quants fitxers s'han
/// tocat i quines rutes (relatives a l'arrel) perquè la UI els rellegesqui.
export interface LspRenameResp {
  files: number;
  edits: number;
  paths: string[];
}

export interface LspDiag {
  line: number;
  col: number;
  severity: "error" | "warning" | "info";
  message: string;
  source: string;
}

export interface LspStatus {
  server: string | null;
  language: string | null;
  ready: boolean;
}

export interface LspServerInfo {
  language: string;
  binary: string;
  installed: boolean;
}

export async function lspCompletion(
  projectPath: string,
  path: string,
  line: number,
  col: number
): Promise<LspCompletion[]> {
  try {
    return (await invoke<LspCompletion[]>("lsp_completion", { projectPath, path, line, col })) ?? [];
  } catch {
    return [];
  }
}

export async function lspHover(
  projectPath: string,
  path: string,
  line: number,
  col: number
): Promise<LspHover | null> {
  try {
    return (await invoke<LspHover | null>("lsp_hover", { projectPath, path, line, col })) ?? null;
  } catch {
    return null;
  }
}

export async function lspReferences(
  projectPath: string,
  path: string,
  line: number,
  col: number
): Promise<LspRef[]> {
  try {
    return (await invoke<LspRef[]>("lsp_references", { projectPath, path, line, col })) ?? [];
  } catch {
    return [];
  }
}

/// «Ves a la definició» amb el servidor LSP real (F12). Null = sense
/// servidor, sense definició o timeout: qui crida decideix el fallback
/// (l'heurístic `find_definition`). Mai un error cap a l'editor.
export async function lspDefinition(
  projectPath: string,
  path: string,
  line: number,
  col: number
): Promise<LspDef | null> {
  try {
    return await withTimeout(
      invoke<LspDef | null>("lsp_definition", { projectPath, path, line, col }).then(
        (d) => d ?? null
      ),
      PROVIDER_TIMEOUT_MS,
      null as LspDef | null
    );
  } catch {
    return null;
  }
}

/// Renombrament del símbol sota el cursor (`lsp_rename`): el backend demana
/// les edicions al servidor i les aplica al disc. Null = sense servidor,
/// sense resposta o res a canviar: mai un error cap a l'editor.
export async function lspRename(
  projectPath: string,
  path: string,
  line: number,
  col: number,
  newName: string
): Promise<LspRenameResp | null> {
  try {
    return await withTimeout(
      invoke<LspRenameResp | null>("lsp_rename", { projectPath, path, line, col, newName }).then(
        (d) => d ?? null
      ),
      PROVIDER_TIMEOUT_MS,
      null as LspRenameResp | null
    );
  } catch {
    return null;
  }
}

export async function lspDiagnostics(projectPath: string, path: string): Promise<LspDiag[]> {
  try {
    return (await invoke<LspDiag[]>("lsp_diagnostics", { projectPath, path })) ?? [];
  } catch {
    return [];
  }
}

export async function lspStatus(projectPath: string): Promise<LspStatus> {
  try {
    return (
      (await invoke<LspStatus>("lsp_status", { projectPath })) ?? {
        server: null,
        language: null,
        ready: false,
      }
    );
  } catch {
    return { server: null, language: null, ready: false };
  }
}

export async function listLspServers(): Promise<LspServerInfo[]> {
  try {
    return (await invoke<LspServerInfo[]>("list_lsp_servers")) ?? [];
  } catch {
    return [];
  }
}

// ---- Mapatges PURS (testables sense Monaco ni backend) ----------------------

/// CompletionItemKind LSP → CompletionItemKind de Monaco (ordres diferents!).
const LSP_TO_MONACO_KIND: Record<number, number> = {
  1: 18, // Text
  2: 0, // Method
  3: 1, // Function
  4: 2, // Constructor
  5: 3, // Field
  6: 4, // Variable
  7: 5, // Class
  8: 7, // Interface
  9: 8, // Module
  10: 9, // Property
  11: 12, // Unit
  12: 13, // Value
  13: 15, // Enum
  14: 17, // Keyword
  15: 27, // Snippet
  16: 19, // Color
  17: 20, // File
  18: 21, // Reference
  19: 23, // Folder
  20: 16, // EnumMember
  21: 14, // Constant
  22: 6, // Struct
  23: 10, // Event
  24: 11, // Operator
  25: 24, // TypeParameter
};

export function monacoKind(lspKind: number): number {
  return LSP_TO_MONACO_KIND[lspKind] ?? 18; // Text com a fallback honest
}

/// Gravetat del contracte → MarkerSeverity de Monaco (Error 8 / Warning 4 /
/// Info 2). Constant numèrica per poder-ho provar sense instanciar Monaco.
export function markerSeverity(sev: string): number {
  return sev === "error" ? 8 : sev === "warning" ? 4 : 2;
}

/// Diagnòstic del backend → marker de Monaco (1-based, com el contracte).
export function toMarker(d: LspDiag): E.IMarkerData {
  const line = Math.max(1, d.line);
  const col = Math.max(1, d.col);
  return {
    startLineNumber: line,
    startColumn: col,
    endLineNumber: line,
    endColumn: col + 1,
    message: d.message,
    severity: markerSeverity(d.severity),
    source: d.source,
  };
}

/// Entrada de completion → suggeriment de Monaco dins l'abast de la paraula.
export function toSuggestion(entry: LspCompletion, range: IRange): L.CompletionItem {
  return {
    label: entry.label,
    kind: monacoKind(entry.kind),
    insertText: entry.insertText || entry.label,
    detail: entry.detail,
    range,
  };
}

// ---- Catàleg de servidors (detecció al backend, cache d'una sola consulta) --

let catalogPromise: Promise<LspServerInfo[]> | null = null;
export function getServerCatalog(): Promise<LspServerInfo[]> {
  if (!catalogPromise) catalogPromise = listLspServers();
  return catalogPromise;
}

/// Llenguatge del catàleg → id de llenguatge de Monaco. TS/JS NO hi són:
/// Monaco ja els serveix amb workers propis (no dupliquem providers).
const MONACO_LANG: Record<string, string> = {
  rust: "rust",
  php: "php",
  python: "python",
  c: "c",
  cpp: "cpp",
  go: "go",
  java: "java",
  kotlin: "kotlin",
  csharp: "csharp",
  ruby: "ruby",
  swift: "swift",
  lua: "lua",
  bash: "shell",
  yaml: "yaml",
  sql: "sql",
  dockerfile: "dockerfile",
};

/// Extensions amb sessió LSP possible (la resta: ni consulta, ni soroll).
const LSP_EXTS = new Set([
  "ts", "tsx", "js", "jsx", "mjs", "cjs",
  "rs", "py", "pyi", "php",
  "c", "h", "cpp", "cc", "cxx", "hpp",
  "go", "java", "kt", "kts", "cs", "rb", "swift", "scala", "sc",
  "ex", "exs", "dart", "lua", "zig", "hs", "sh", "bash",
  "yaml", "yml", "tf", "tfvars", "vue", "svelte", "sql",
]);

// ---- Arrel del projecte per als providers (l'EditorPane la manté al dia) ----

let projectRoot: string | null = null;
export function setLspProjectRoot(root: string | null) {
  projectRoot = root;
}

// ---- Providers de Monaco (completion + hover) --------------------------------

const PROVIDER_DEBOUNCE_MS = 250;
/// Just per damunt dels 8 s del backend (mateix criteri que inlineComplete).
const PROVIDER_TIMEOUT_MS = 8_500;

interface Timer {
  t: ReturnType<typeof setTimeout> | null;
  gen: number;
}
const completionTimers = new Map<string, Timer>();

function withTimeout<T>(p: Promise<T>, ms: number, fallback: T): Promise<T> {
  return Promise.race([
    p,
    new Promise<T>((res) => setTimeout(() => res(fallback), ms)),
  ]);
}

/// Guard de registre: Monaco ACUMULA providers; l'editor es remunta
/// (split, StrictMode) i sense guard cada muntatge en duplicaria els registres.
let providersRegistered = false;

/// Registra completion/hover per als llenguatges AMB servidor detectat.
/// S'ha de cridar a cada onMount (idempotent). La detecció és asíncrona:
/// si el catàleg arriba tard, els providers es registren igualment després.
export function registerLspProviders(mon: typeof import("monaco-editor")) {
  if (providersRegistered) return;
  providersRegistered = true;
  void getServerCatalog().then((catalog) => {
    for (const srv of catalog) {
      if (!srv.installed) continue;
      const lang = MONACO_LANG[srv.language];
      if (!lang) continue; // typescript (workers natius) i desconeguts: fora
      try {
        mon.languages.registerCompletionItemProvider(lang, {
          provideCompletionItems: async (
            model: E.ITextModel,
            position: Position,
            _ctx: L.CompletionContext,
            token: CancellationToken
          ): Promise<L.CompletionList> => {
            const empty: L.CompletionList = { suggestions: [] };
            if (!projectRoot || model.isDisposed() || model.uri.scheme !== "file") return empty;
            const path = model.uri.path;
            // Debounce + generació: l'última pulsació mana (com inlineComplete).
            const key = model.uri.toString();
            let timer = completionTimers.get(key);
            if (!timer) {
              timer = { t: null, gen: 0 };
              completionTimers.set(key, timer);
            }
            timer.gen += 1;
            const myGen = timer.gen;
            if (timer.t) clearTimeout(timer.t);
            const cancelled = await new Promise<boolean>((res) => {
              timer!.t = setTimeout(() => res(false), PROVIDER_DEBOUNCE_MS);
              token.onCancellationRequested(() => res(true));
            });
            if (cancelled || myGen !== timer.gen || token.isCancellationRequested) return empty;
            const entries = await withTimeout(
              lspCompletion(projectRoot, path, position.lineNumber, position.column),
              PROVIDER_TIMEOUT_MS,
              [] as LspCompletion[]
            );
            if (myGen !== timer.gen || token.isCancellationRequested) return empty;
            const word = model.getWordUntilPosition(position);
            const range: IRange = {
              startLineNumber: position.lineNumber,
              endLineNumber: position.lineNumber,
              startColumn: word.startColumn,
              endColumn: word.endColumn,
            };
            return { suggestions: entries.slice(0, 200).map((e) => toSuggestion(e, range)) };
          },
        });
        mon.languages.registerHoverProvider(lang, {
          provideHover: async (
            model: E.ITextModel,
            position: Position,
            token: CancellationToken
          ): Promise<L.Hover | null> => {
            if (!projectRoot || model.isDisposed() || model.uri.scheme !== "file") return null;
            const res = await withTimeout(
              lspHover(projectRoot, model.uri.path, position.lineNumber, position.column),
              PROVIDER_TIMEOUT_MS,
              null as LspHover | null
            );
            if (!res || token.isCancellationRequested) return null;
            return { contents: [{ value: res.contents }] };
          },
        });
      } catch {
        /* llenguatge no registrat a Monaco: degradació silenciosa */
      }
    }
  });
}

// ---- Diagnòstics de projecte (tsserver geterr / publishDiagnostics) ----------

const DIAG_DEBOUNCE_MS = 900;
const diagTimers = new Map<string, Timer>();

/// Programa la consulta de diagnòstics reals d'un fitxer (debounce per ruta +
/// generació: una resposta vella mai aixafa una de nova). El backend llig el
/// text del DISC: en desar, la consulta és exacta; mentre s'escriu sense
/// desar, pot anar un pas per darrere (limitació honesta del contracte).
export function scheduleLspDiagnostics(projectPath: string | null, path: string) {
  if (!projectPath || !path.startsWith(projectPath)) return;
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  if (!LSP_EXTS.has(ext)) return;
  let timer = diagTimers.get(path);
  if (!timer) {
    timer = { t: null, gen: 0 };
    diagTimers.set(path, timer);
  }
  timer.gen += 1;
  const myGen = timer.gen;
  if (timer.t) clearTimeout(timer.t);
  timer.t = setTimeout(() => {
    void (async () => {
      const diags = await lspDiagnostics(projectPath, path);
      // Cancel·lació: si hi ha una consulta més nova, esta ja no mana.
      if (diagTimers.get(path)?.gen !== myGen) return;
      applyLspMarkers(path, diags);
    })();
  }, DIAG_DEBOUNCE_MS);
}