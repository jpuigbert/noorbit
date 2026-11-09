/// Configura Monaco perquè es carregue des del paquet local (no des de cap
/// CDN). Així l'editor i el diff costat a costat del Git funcionen **sense
/// connexió**. `@monaco-editor/react`, per defecte, descarrega Monaco de
/// jsdelivr; amb `loader.config({ monaco })` l'obrigem a usar la còpia que ja
/// empaquetem (la dependència `monaco-editor`), que és el mateix motor de
/// Visual Studio Code.
///
/// Els workers han de declarar-se abans que es munti cap editor: Vite los
/// empaqueta com a chunks apartats amb el sufijo `?worker`.
import * as monaco from "monaco-editor";
import { loader } from "@monaco-editor/react";

import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import jsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import cssWorker from "monaco-editor/esm/vs/language/css/css.worker?worker";
import htmlWorker from "monaco-editor/esm/vs/language/html/html.worker?worker";
import tsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker";

// (self as any) perquè els tipus de Monaco no declaren MonacoEnvironment a
// nivel global; és el punt d'entrada que llegeix Monaco per crear els workers.
//
// RENDIMENT: els workers es CREEN UNA i únic de vegades per llenguatge i es
// reusen (Monaco demanaria una instància nova cada cop que s'obre/canvia de llenguatge:
// memòria extra i primer obrir més lent). Un diccionari per etiqueta mantén
// el worker viu i el retorna reutilitzat.
const workerCache = new Map<string, Worker>();

function workerFor(
  label: string,
  make: () => Worker
): Worker {
  const cached = workerCache.get(label);
  if (cached) return cached;
  const w = make();
  workerCache.set(label, w);
  return w;
}

(self as unknown as { MonacoEnvironment: unknown }).MonacoEnvironment = {
  getWorker(_workerId: string, label: string): Worker {
    switch (label) {
      case "json":
        return workerFor("json", () => new jsonWorker());
      case "css":
      case "scss":
      case "less":
        return workerFor("css", () => new cssWorker());
      case "html":
      case "handlebars":
      case "razor":
        return workerFor("html", () => new htmlWorker());
      case "typescript":
      case "javascript":
        return workerFor("typescript", () => new tsWorker());
      default:
        return workerFor("editor", () => new editorWorker());
    }
  },
};

// Força @monaco-editor/react a usar la instància local en lloc del CDN.
loader.config({ monaco });

/// Pegat per un error conegut de @monaco-editor/react 4.7 amb Monaco ≥ 0.52:
/// en desmuntar un DiffEditor (la revisió verd/roig dels canvis de la IA o el
/// diff del Git), la llibreria allibera els models de text ABANS de tancar el
/// widget i Monaco ho comunica com a error («TextModel got disposed before
/// DiffEditorWidget model got reset»). El widget ja es recupera sol (fa
/// `setModel(null)` i tot seguit es tanca), així que el missatge és soroll
/// inofensiu. El maneig per defecte de Monaco el rellança en async i acabaria
/// a la Consola de depuració espantant l'usuari; ací el filtreig a l'origen i
/// la resta d'errors segueixen pel maneig habitual sense tocar-los.
// @ts-expect-error — Monaco no publica tipus per a este mòdul intern, però
// existeix en temps d'execució i és la mateixa instància que usa el bundle.
// (Monaco 0.52 ja no exporta `setUnexpectedErrorHandler`: intercanviem el
// maneig directament sobre l'objecte `errorHandler`.)
import { errorHandler as monacoErrorHandler } from "monaco-editor/esm/vs/base/common/errors.js";

const handlerOwner = monacoErrorHandler as {
  unexpectedErrorHandler: (err: unknown) => void;
};
const previousHandler = handlerOwner.unexpectedErrorHandler;
handlerOwner.unexpectedErrorHandler = (err: unknown) => {
  const msg =
    err instanceof Error ? err.message : typeof err === "string" ? err : String(err ?? "");
  if (msg.includes("TextModel got disposed before DiffEditorWidget model got reset")) {
    return;
  }
  previousHandler(err);
};
