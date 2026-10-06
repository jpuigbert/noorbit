/// Motor de DIAGNÒSTIC del editor. Dos nivells:
///
///  1. NATIU (Monaco). Per a TypeScript/JS/JSON/CSS/HTML, el worker del propi
///     Monaco calcula errors de sintaxi i semàntica en escriure i ja pinta
///     l'«esquiggle» vermell. Nosaltres només ENSURTEM aquestes marques via
///     «onDidChangeMarkers» i les bolquem al panell «Problemes».
///
///  2. LLEUGER PROPI. Per a llenguaments que Monaco NO valida (Python, Rust,
///     C/C++…), fem una comprovació determinista i sense dependènciesExternes:
///     balanceig de claudàtors/parèntesis/acolkes ignorant cadenes i comentaris,
///     i cadenes de text sense tancar. Suficient per agafar l'error clàssic de
///     «m'he deixat una clau oberta» mentre s'escriu a mà, com fa Sublime.
///
/// Totes dues acaben sent «markers» de Monaco: el marcat a la línia i el panell
/// compartixen la mateixa font de veritat.
import { getMonaco, getEditor } from "./editorApi";
import type { Uri } from "monaco-editor";
import { useProblemsStore, type Problem } from "../stores/problemsStore";

/// Llenguatges que Monaco ja valid nativament (no cal duplicar-los).
const NATIVE = new Set([
  "ts",
  "tsx",
  "js",
  "jsx",
  "mjs",
  "cjs",
  "json",
  "css",
  "scss",
  "less",
  "html",
]);

interface LangCfg {
  lineComment: string[];
  blockComment: [string, string] | null;
  tripleQuotes: string[]; // cadenes multilínia (Python)
  charLiteral: boolean; // com l'apòstrof de Rust/C per a un caràcter
  stringsMultiline: boolean; // una cadena normal pot travessar línies?
}

const CFG: Record<string, LangCfg> = {
  py: { lineComment: ["#"], blockComment: null, tripleQuotes: ['"""', "'''"], charLiteral: false, stringsMultiline: false },
  rs: { lineComment: ["//"], blockComment: ["/*", "*/"], tripleQuotes: [], charLiteral: true, stringsMultiline: false },
  c: { lineComment: ["//"], blockComment: ["/*", "*/"], tripleQuotes: [], charLiteral: true, stringsMultiline: false },
  h: { lineComment: ["//"], blockComment: ["/*", "*/"], tripleQuotes: [], charLiteral: true, stringsMultiline: false },
  cpp: { lineComment: ["//"], blockComment: ["/*", "*/"], tripleQuotes: [], charLiteral: true, stringsMultiline: false },
};

/// Config per a la família JS/TS usada NOMÉS en revisió «offline» (fitxers que
/// la IA ha escrit i no són oberts). «charLiteral: true» fa ignorar l'apòstrof
/// com a cadena: evita falsos positius amb textos en català a dins JSX
/// («No s'ha pogut…»), que amb la nostra interfície serien contínus.
const JS_CFG: LangCfg = {
  lineComment: ["//"],
  blockComment: ["/*", "*/"],
  tripleQuotes: ["`"], // plantilles literal `…` (multilínia permesa)
  charLiteral: true,
  stringsMultiline: false,
};
for (const k of ["ts", "tsx", "js", "jsx", "mjs", "cjs"]) CFG[k] = JS_CFG;

function extOf(name: string): string {
  return name.split(".").pop()?.toLowerCase() ?? "";
}

interface RawIssue {
  line: number; // 1-based
  column: number; // 1-based
  endColumn: number;
  message: string;
  error: boolean;
}

/// Emparellament de delimiters conscient de cadenes i comentaris.
function lintSource(ext: string, text: string): RawIssue[] {
  const cfg = CFG[ext];
  if (!cfg) return []; // md, txt…: massa soroll, no cal

  const issues: RawIssue[] = [];
  const lines = text.split("\n");
  const openers: Record<string, string> = { ")": "(", "]": "[", "}": "{" };
  const closers = new Set([")", "]", "}"]);
  const stack: { ch: string; line: number; col: number }[] = [];

  let inBlock = false;
  let inTriple: string | null = null; // cadena multilínia activa

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    let inString: string | null = null; // cadena normal en esta línia
    let c = 0;
    const n = line.length;

    while (c < n) {
      const two = line.slice(c, c + 2);

      // Dins d'un comentari de bloqueig: busquem només el tancament.
      if (inBlock) {
        if (cfg.blockComment && two === cfg.blockComment[1]) {
          inBlock = false;
          c += 2;
          continue;
        }
        c += 1;
        continue;
      }
      // Dins d'una cadena multilínia (Python «»»).
      if (inTriple) {
        if (line.startsWith(inTriple, c)) {
          c += inTriple.length;
          inTriple = null;
          continue;
        }
        c += 1;
        continue;
      }
      // Dins d'una cadena normal d'esta línia.
      if (inString) {
        if (line[c] === "\\") {
          c += 2; // escape: \n, \" …
          continue;
        }
        if (line[c] === inString) {
          inString = null;
          c += 1;
          continue;
        }
        c += 1;
        continue;
      }

      // Fora de cadenes/comentaris: detectem inici.
      if (cfg.blockComment && two === cfg.blockComment[0]) {
        inBlock = true;
        c += 2;
        continue;
      }
      if (cfg.lineComment.includes(two) || cfg.lineComment.includes(line[c] ?? "")) {
        break; // la resta de la línia és comentari
      }
      const triple = cfg.tripleQuotes.find((t) => line.startsWith(t, c));
      if (triple) {
        // Pot obrir i tancar en la mateixa línia; si no tanca, és multilínia.
        const rest = line.slice(c + triple.length);
        if (rest.includes(triple)) {
          c += triple.length + rest.indexOf(triple) + triple.length;
          continue;
        }
        inTriple = triple;
        c += triple.length;
        continue;
      }
      const ch = line[c];
      // Cometza cadena. En Rust/C l'apòstrof és un caràcter/lifetime: NO
      // tractem «'» com a cadena per no crear falsos positius («&'a str»).
      if (ch === '"' || (ch === "'" && !cfg.charLiteral)) {
        inString = ch;
        c += 1;
        continue;
      }
      if (ch === "(" || ch === "[" || ch === "{") {
        stack.push({ ch, line: i + 1, col: c + 1 });
        c += 1;
        continue;
      }
      if (closers.has(ch)) {
        const want = openers[ch];
        const top = stack[stack.length - 1];
        if (!top || top.ch !== want) {
          issues.push({
            line: i + 1,
            column: c + 1,
            endColumn: c + 2,
            message: top
              ? `«${ch}» tanca però l'últim sense tancar és «${top.ch}» (línia ${top.line}).`
              : `«${ch}» sobra: no hi ha cap «${want}» obert.`,
            error: true,
          });
        } else {
          stack.pop();
        }
        c += 1;
        continue;
      }
      c += 1;
    }

    // Fi de línia amb una cadena normal oberta (i no pot ser multilínia).
    if (inString && !cfg.stringsMultiline && !inTriple) {
      issues.push({
        line: i + 1,
        column: 1,
        endColumn: n + 1,
        message: `Cadena amb ${inString} sense tancar en esta línia.`,
        error: true,
      });
      inString = null;
    }
  }

  // Delimiters que queden oberts al final del fitxer.
  for (const s of stack.reverse()) {
    issues.push({
      line: s.line,
      column: s.col,
      endColumn: s.col + 1,
      message: `«${s.ch}» mai no es tanca.`,
      error: true,
    });
  }
  return issues;
}

/// Converteix els markers de Monaco (nadius o nostres) a Problem i els guarda
/// al magatzem. Cridat per «onDidChangeMarkers».
function syncProblems(monaco: NonNullable<ReturnType<typeof getMonaco>>, uris: readonly Uri[]) {
  const setFile = useProblemsStore.getState().setFileProblems;
  for (const uri of uris) {
    const markers = monaco.editor.getModelMarkers({ resource: uri });
    const problems: Problem[] = markers.map((m) => ({
      path: uri.path,
      name: uri.path.split(/[/\\]/).pop() ?? uri.path,
      line: m.startLineNumber,
      column: m.startColumn,
      endLine: m.endLineNumber,
      endColumn: m.endColumn,
      message: m.message,
      severity:
        m.severity === monaco.MarkerSeverity.Error
          ? "error"
          : m.severity === monaco.MarkerSeverity.Warning
          ? "warning"
          : "info",
      source: m.source ?? "editor",
    }));
    setFile(uri.path, problems);
  }
}

let listenerReady = false;
function ensureListener() {
  const monaco = getMonaco();
  if (!monaco || listenerReady) return;
  listenerReady = true;
  monaco.editor.onDidChangeMarkers((uris) => syncProblems(monaco, uris));
}

/// Valida el contingut del fitxer obert i pinta/esvaeix marcadors propis.
/// S'anomena amb cada edició (EditorPane) i munta l'escolta de markers.
export function validateCurrentFile(name: string, content: string) {
  const monaco = getMonaco();
  const editor = getEditor();
  if (!monaco || !editor) return;
  ensureListener();

  const model = editor.getModel();
  if (!model) return;
  const ext = extOf(name);

  // Nadius: Monaco ja genera els marcadors sols. Neteja només els nostres.
  if (NATIVE.has(ext)) {
    monaco.editor.setModelMarkers(model, "noorbit-lint", []);
    syncProblems(monaco, [model.uri]);
    return;
  }

  const issues = lintSource(ext, content);
  const markers = issues.map((it) => ({
    startLineNumber: it.line,
    startColumn: it.column,
    endLineNumber: it.line,
    endColumn: it.endColumn,
    message: it.message,
    severity: it.error ? monaco.MarkerSeverity.Error : monaco.MarkerSeverity.Warning,
  }));
  monaco.editor.setModelMarkers(model, "noorbit-lint", markers);
  syncProblems(monaco, [model.uri]);
}

/// Revisa un fitxer que NO està obert a l'editor (p. ex. escrit per la IA amb
/// «@file:» o materialitzat en viu). Com que Monaco no en té cap model, no es
/// pinten «squiggle», però SÍ que es registren els problemes al panell perquè
/// l'usuari los vegui i hi salti (que llavors obrirà el fitxer i Monaco ja el
/// validarà nativament). Per a JSON fem un «JSON.parse» real; per a la resta
/// (py, rs, c, ts…), el balanceig de delimiters del motor lleuger.
/// Retorna el nombre d'errors detectats (per afegir un avís al resum de la IA).
export function lintOffline(path: string, name: string, content: string): number {
  const ext = extOf(name);
  const issues: RawIssue[] = [];

  if (ext === "json") {
    try {
      JSON.parse(content);
    } catch (e) {
      const msg = String((e as Error)?.message ?? e);
      // Intentem extreure'n la línia (els motors moderns inclouen la posició).
      const pos = /line\s+(\d+)[^\n]*column\s+(\d+)/i.exec(msg);
      let line = 1;
      let col = 1;
      if (pos) {
        line = Number(pos[1]);
        col = Number(pos[2]);
      } else {
        const at = /position\s+(\d+)/i.exec(msg);
        if (at) {
          const upto = content.slice(0, Number(at[1]));
          line = upto.split("\n").length;
          col = upto.length - upto.lastIndexOf("\n");
        }
      }
      issues.push({
        line,
        column: col,
        endColumn: col + 1,
        message: `JSON no vàlid: ${msg.replace(/\s*\(.*\)\.?$/, "").trim()}.`,
        error: true,
      });
    }
  } else {
    issues.push(...lintSource(ext, content));
  }

  if (issues.length === 0) {
    // Cap problema: neta possibles entrades anteriors d'este mateix fitxer.
    useProblemsStore.getState().setFileProblems(path, []);
    return 0;
  }

  const problems: Problem[] = issues.map((it) => ({
    path,
    name: path.split(/[/\\]/).pop() ?? name,
    line: it.line,
    column: it.column,
    endLine: it.line,
    endColumn: it.endColumn,
    message: it.message,
    severity: it.error ? "error" : "warning",
    source: "noorbit-lint",
  }));
  useProblemsStore.getState().setFileProblems(path, problems);
  return problems.filter((p) => p.severity === "error").length;
}
