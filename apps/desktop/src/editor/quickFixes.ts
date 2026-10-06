/// PROVEÏDOR DE CORRECCIONS RÀPIDES («quick fix») per a tot l'editor.
///
/// Quan polsem ⌘+1 / Ctrl+1 (o la bombeta de Monaco), aquest mòdul ofereix:
///
///  • FIXOS NATIUS. Per a TypeScript/JS/JSON/CSS/HTML, el worker de Monaco ja
///    porta les seues pròpies accions ràpides (importar símbol, afegir
///    tipus…). No les dupliquem: eixixen soles al costat de les nostres.
///
///  • FIXOS PROPIIS. Per als errors del nostre motor lleuger (claudàtors i
///    cadenes sense tancar en Python/Rust/C/…) generem correccions reals:
///    afegir el tancament que falta o suprimir el que sobra. Deterministes
///    i instantànies.
///
/// I a més, una ACCIÓ D'IA («aiQuickFix») lligada a ⇧⌘+1 / ⇧Ctrl+1: pregunta
/// al proveïdor actiu com resoldre l'error de la línia on és el cursor i
/// aplica la correcció. Cobreix llenguaments sense worker (Python, Rust, C…).
///
/// Tot usa la API estàndard de Monaco, així s'integra en el mateix menú ⌘+1
/// del editor sense haver d'inventar cap UI nova.
import { getMonaco, getEditor } from "./editorApi";
import { invoke } from "@tauri-apps/api/core";
import type { editor as E, languages as L, IRange } from "monaco-editor";

/// LLenguaments als quals els registrem el proveïdor (els ids de Monaco).
const LANGS = [
  "python",
  "rust",
  "c",
  "cpp",
  "javascript",
  "typescript",
  "typescriptreact",
  "javascriptreact",
  "json",
  "toml",
  "yaml",
  "plaintext",
];

/// Emparellaments: si falta un tancament, l'afegim al final de la línia on
/// es va obrir (no al final del fitxer: així el codi queda llegible).
const CLOSER: Record<string, string> = { "(": ")", "[": "]", "{": "}" };

interface FixPlan {
  title: string;
  range: IRange;
  text: string;
}

/// Demana a la IA la línia reparada. Retornem només codi: tallem qualsevol
/// envoltament ```…``` i agafem la primera línia no buida.
async function aiRepair(fileName: string, lineText: string, message: string): Promise<string | null> {
  const prompt =
    `Corregix ÚNICAMENT l'error indicat en aquesta línia de codi (${fileName}).\n` +
    `Respon NOMÉS amb la línia corregida, sense explicacions ni blocs de codi.\n\n` +
    `ERROR: ${message}\nLINIA: ${lineText}`;
  try {
    const raw = await invoke<string>("send_prompt", {
      prompt,
      system: "Ets un reparador de codi. Respon només amb la línia corregida, sense text addicional.",
    });
    let out = (raw ?? "").trim();
    const fence = /```[a-zA-Z]*\n?([\s\S]*?)```/.exec(out);
    if (fence) out = fence[1].trim();
    const first = out.split("\n").map((l) => l.trimEnd()).find((l) => l.trim().length > 0);
    return first ?? null;
  } catch {
    return null;
  }
}

/// Converteix un marcador del nostre motor en una correcció determinista, o
/// null si el missatge no correspon a cap patró que sapiguem reparar sols.
function planFor(
  model: E.ITextModel,
  msg: string,
  line: number,
  startCol: number,
  endCol: number
): FixPlan | null {
  const eol = model.getLineMaxColumn(line);
  const never = /«(.)» mai no es tanca/.exec(msg);
  if (never && CLOSER[never[1]]) {
    return {
      title: `Afegir «${CLOSER[never[1]]}» per a tancar «${never[1]}» (línia ${line})`,
      range: { startLineNumber: line, startColumn: eol, endLineNumber: line, endColumn: eol },
      text: CLOSER[never[1]],
    };
  }
  const str = /Cadena amb (["']) sense tancar/.exec(msg);
  if (str) {
    return {
      title: `Tancar la cadena amb ${str[1]} (línia ${line})`,
      range: { startLineNumber: line, startColumn: eol, endLineNumber: line, endColumn: eol },
      text: str[1],
    };
  }
  const extra = /«(.)» sobra/.exec(msg);
  if (extra) {
    return {
      title: `Suprimir «${extra[1]}» sobrer (línia ${line})`,
      range: { startLineNumber: line, startColumn: startCol, endLineNumber: line, endColumn: endCol },
      text: "",
    };
  }
  const mismatch = /tanca però l'últim sense tancar és «(.)» \(línia (\d+)\)/.exec(msg);
  if (mismatch && CLOSER[mismatch[1]]) {
    const openLine = Number(mismatch[2]);
    const openEol = model.getLineMaxColumn(openLine);
    return {
      title: `Afegir «${CLOSER[mismatch[1]]}» que falta per a «${mismatch[1]}» (línia ${openLine})`,
      range: { startLineNumber: openLine, startColumn: openEol, endLineNumber: openLine, endColumn: openEol },
      text: CLOSER[mismatch[1]],
    };
  }
  return null;
}

let registered = false;

/// Registrem el proveïdor de quick fix un sola vegada (des de «main.tsx»,
/// després que Monaco estiga configurat i abans de muntar cap editor).
export function registerQuickFixes() {
  const mon = getMonaco();
  if (!mon || registered) return;
  registered = true;

  const provider: L.CodeActionProvider = {
    provideCodeActions(
      model: E.ITextModel,
      _range: IRange,
      context: L.CodeActionContext
    ): L.CodeActionList {
      const actions: L.CodeAction[] = [];

      // Tots els marcadors del nostre motor en el document (errors d'abast:
      // claus que mai no es tanquen es proposen siga on siga el cursor).
      const ours = mon.editor
        .getModelMarkers({ resource: model.uri })
        .filter((m) => m.source === "noorbit-lint");
      const markers = ours.length ? ours : context.markers.filter((m) => (m as { source?: string }).source === "noorbit-lint");

      for (const m of markers) {
        const line = m.startLineNumber;
        const plan = planFor(model, m.message ?? "", line, m.startColumn, m.endColumn);
        if (!plan) continue;
        actions.push({
          title: plan.title,
          kind: "quickfix",
          isPreferred: true,
          diagnostics: [m],
          edit: {
            edits: [{ resource: model.uri, textEdit: { range: plan.range, text: plan.text }, versionId: undefined }],
          },
        });
      }

      return { actions, dispose() {} };
    },
  };

  for (const lang of LANGS) {
    mon.languages.registerCodeActionProvider({ language: lang }, provider);
  }
}

/// Reparació amb IA de l'error a la línia del cursor. Cridada des de la
/// drecera ⇧⌘+1 / ⇧Ctrl+1 (vegeu EditorPane). Retorna true si va aplicar un
/// canvi, perquè la UI puga avisar-ho.
export async function aiQuickFix(): Promise<boolean> {
  const mon = getMonaco();
  const editor = getEditor();
  if (!mon || !editor) return false;
  const model = editor.getModel();
  const pos = editor.getPosition();
  if (!model || !pos) return false;

  const fileName = model.uri.path.split(/[/\\]/).pop() ?? model.uri.path;
  // Marcador del nostre motor a la línia del cursor (si n'hi ha); si no,
  // usem el primer error del fitxer o un missatge genèric.
  const markers = mon.editor
    .getModelMarkers({ resource: model.uri })
    .filter((m) => m.startLineNumber === pos.lineNumber);
  const message = markers[0]?.message ?? "Possible error de sintaxi en esta línia.";
  const lineText = model.getLineContent(pos.lineNumber);

  const fixed = await aiRepair(fileName, lineText, message);
  if (fixed === null) return false;

  const lineNo = pos.lineNumber;
  model.pushEditOperations(
    [],
    [
      {
        range: {
          startLineNumber: lineNo,
          startColumn: 1,
          endLineNumber: lineNo,
          endColumn: model.getLineMaxColumn(lineNo),
        },
        text: fixed,
      },
    ],
    () => null
  );
  return true;
}
