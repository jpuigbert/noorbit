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
(self as unknown as { MonacoEnvironment: unknown }).MonacoEnvironment = {
  getWorker(_workerId: string, label: string): Worker {
    switch (label) {
      case "json":
        return new jsonWorker();
      case "css":
      case "scss":
      case "less":
        return new cssWorker();
      case "html":
      case "handlebars":
      case "razor":
        return new htmlWorker();
      case "typescript":
      case "javascript":
        return new tsWorker();
      default:
        return new editorWorker();
    }
  },
};

// Força @monaco-editor/react a usar la instància local en lloc del CDN.
loader.config({ monaco });
