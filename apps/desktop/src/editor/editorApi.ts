/// Pont d'acció sobre l'editor Monaco (versió autònoma).
/// L'EditorPane registra la instància aquí i els menús / dreceres
/// criden aquestes funcions, com fa VS Code amb els seus comandaments.
import type { editor as MonacoEditor } from "monaco-editor";
import { invoke } from "@tauri-apps/api/core";

type Monaco = typeof import("monaco-editor");

let _editor: MonacoEditor.IStandaloneCodeEditor | null = null;
let _monaco: Monaco | null = null;
let _themesDefined = false;

/// Defineja temes de Monaco que copien els tons de color de NoOrbit.
function defineNoOrbitThemes(mon: Monaco) {
  if (_themesDefined) return;
  const base = "vs-dark" as const;
  const mk = (bg: string) => ({
    base,
    inherit: true,
    rules: [],
    colors: { "editor.background": bg },
  });
  mon.editor.defineTheme("noorbit-gray", mk("#2b2d31"));
  mon.editor.defineTheme("noorbit-green", mk("#0c1a12"));
  mon.editor.defineTheme("noorbit-yellow", mk("#1a1606"));
  _themesDefined = true;
}

export function bindEditor(
  ed: MonacoEditor.IStandaloneCodeEditor | null,
  mon: Monaco | null
) {
  _editor = ed;
  _monaco = mon;
  if (mon) defineNoOrbitThemes(mon);
}

/// Tradueix el tema de la UI al nom de tema de Monaco.
export function monacoThemeFor(ui: string): string {
  switch (ui) {
    case "light":
      return "light";
    case "gray":
      return "noorbit-gray";
    case "green":
      return "noorbit-green";
    case "yellow":
      return "noorbit-yellow";
    default:
      return "vs-dark";
  }
}

export const hasEditor = () => _editor !== null;

function runAction(id: string) {
  const a = _editor?.getAction(id);
  if (a) a.run();
}

// --- Edició bàsica -----------------------------------------------------------
export const undo = () => runAction("undo");
export const redo = () => runAction("redo");
export const cut = () => runAction("editor.action.clipboardCutAction");
export const copy = () => runAction("editor.action.clipboardCopyAction");
export const paste = () => runAction("editor.action.clipboardPasteAction");
export const selectAll = () => runAction("editor.action.selectAll");

// --- Cerca i substitució -----------------------------------------------------
export const find = () => runAction("actions.find");
export const replace = () => runAction("editor.action.startFindReplaceAction");

// --- Navegació ---------------------------------------------------------------
export const gotoLine = () => runAction("editor.action.gotoLine");
export const goToSymbol = () => runAction("editor.action.goToSymbols");

/// Porta l'editor a una línia/columna i selecciona el text trobat
/// (ho usa la cerca del projecte ⇧⌘F en obrir un resultat).
export function revealAt(line: number, column = 1, matchLen = 0) {
  if (!_editor) return;
  _editor.revealLineInCenter(line);
  _editor.setSelection({
    startLineNumber: line,
    startColumn: column,
    endLineNumber: line,
    endColumn: column + Math.max(0, matchLen),
  });
  _editor.focus();
}

// --- Selecció ---------------------------------------------------------------
export const selectNextOccurrence = () => runAction("editor.action.addSelectionToNextFindMatch");
export const toggleColumnSelection = () => runAction("editor.action.toggleColumnSelection");

// --- Visualització de l'editor ----------------------------------------------
export const toggleWordWrap = () => runAction("editor.action.toggleWordWrap");
export const formatDocument = () => runAction("editor.action.formatDocument");

export function toggleMinimap() {
  if (!_editor || !_monaco) return;
  const current = _editor.getOption(_monaco.editor.EditorOption.minimap)?.enabled ?? false;
  _editor.updateOptions({ minimap: { enabled: !current } });
}

export function changeFontDelta(delta: number) {
  if (!_editor || !_monaco) return;
  const size = _editor.getOption(_monaco.editor.EditorOption.fontSize) || 13;
  _editor.updateOptions({ fontSize: Math.max(8, Math.min(32, size + delta)) });
}

// --- Estat ------------------------------------------------------------------
export function editorHasFocus(): boolean {
  return _editor?.hasTextFocus() ?? false;
}

// --- Sessió de cerca del projecte (⇧⌘F → F3/⇧F3) ---------------------------
// Quan la UI obre un resultat de la cerca global, registra aquí totes les
// posicions de la paraula dins el fitxer: les ressaltem i F3 hi navega.
interface Range {
  startLineNumber: number;
  startColumn: number;
  endLineNumber: number;
  endColumn: number;
}

let _matchRanges: Range[] = [];
let _matchIdx = -1;
let _decoIds: string[] = [];

function paintMatches() {
  if (!_editor || !_monaco) return;
  const deco = _monaco.editor;
  _decoIds = _editor.deltaDecorations(_decoIds, []);
  if (_matchRanges.length === 0) return;
  const model = _editor.getModel();
  if (!model) return;
  _decoIds = _editor.deltaDecorations(
    _decoIds,
    _matchRanges.map((r, i) => ({
      range: r,
      options: {
        className: i === _matchIdx ? "noorbit-match-active" : "noorbit-match",
        overviewRuler: {
          color: i === _matchIdx ? "#f85149" : "#8b5cf6",
          darkColor: i === _matchIdx ? "#f85149" : "#8b5cf6",
          position: deco.OverviewRulerLane.Center,
        },
        stickiness: deco.TrackedRangeStickiness.NeverGrowsWhenTypingAtEdges,
      },
    }))
  );
}

/// Defineix la sessió de cerca: llista de posicions (ja ordenades) i
/// ressaltat immediat. Retorna quantes coincidències hi ha.
export function setSearchMatches(ranges: Range[]): number {
  _matchRanges = ranges;
  _matchIdx = ranges.length ? 0 : -1;
  paintMatches();
  if (_matchIdx >= 0) {
    _editor?.revealLineInCenter(ranges[0].startLineNumber);
  }
  return ranges.length;
}

/// F3: salta a la següent coincidència de la sessió de cerca.
/// Si no n'hi ha sessió, usa el cercador intern de Monaco (find widget).
export function goNextMatch() {
  if (_matchRanges.length === 0) {
    runAction("actions.find");
    return;
  }
  _matchIdx = (_matchIdx + 1) % _matchRanges.length;
  paintMatches();
  const r = _matchRanges[_matchIdx];
  _editor?.revealLineInCenter(r.startLineNumber);
  _editor?.setSelection(r);
}

/// ⇧F3: anterior coincidència (volta).
export function goPrevMatch() {
  if (_matchRanges.length === 0) {
    runAction("actions.find");
    return;
  }
  _matchIdx = (_matchIdx - 1 + _matchRanges.length) % _matchRanges.length;
  paintMatches();
  const r = _matchRanges[_matchIdx];
  _editor?.revealLineInCenter(r.startLineNumber);
  _editor?.setSelection(r);
}

/// Neteja el ressaltat (en tancar la cerca o canviar de fitxer).
export function clearSearchMatches() {
  _matchRanges = [];
  _matchIdx = -1;
  paintMatches();
}

// --- Ves a la definició (F12) ------------------------------------------------
export interface DefResult {
  ok: boolean;
  path?: string;
  line?: number;
  column?: number;
  error?: string;
}

/// Demana al backend la definició del símbol sota el cursor. No fa el salt
/// ell mateix: retorna la pos perquè l'acció carregue el fitxer i hi salte.
export async function findDefinitionHere(path: string): Promise<DefResult> {
  if (!_editor) return { ok: false, error: "No hi ha editor." };
  const pos = _editor.getPosition();
  if (!pos) return { ok: false, error: "Sense cursor." };
  try {
    const def = await invoke<{ path: string; line: number; column: number }>(
      "find_definition",
      { path, line: pos.lineNumber, column: pos.column, symbol: null }
    );
    return { ok: true, path: def.path, line: def.line, column: def.column };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}
