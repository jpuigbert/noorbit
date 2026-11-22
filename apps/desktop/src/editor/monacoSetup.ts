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

// ---- GDScript (Godot): Monaco no el porta de sèrie -------------------------
// Tokenizer monarch propi i senzill: paraules clau de GDScript 2.x (Godot 4),
// anotacions (@export, @onready…), referències de node ($Node, %Unique),
// cadenes, números i comentaris «#». La resta de llenguatges «barats»
// (php, yaml, ini/toml…) ja venen amb els basic-languages del paquet local.
if (!monaco.languages.getLanguages().some((l) => l.id === "gdscript")) {
  monaco.languages.register({ id: "gdscript", extensions: [".gd"], aliases: ["GDScript", "gdscript"] });
  monaco.languages.setLanguageConfiguration("gdscript", {
    comments: { lineComment: "#" },
    brackets: [
      ["(", ")"],
      ["[", "]"],
      ["{", "}"],
    ],
    autoClosingPairs: [
      { open: "(", close: ")" },
      { open: "[", close: "]" },
      { open: "{", close: "}" },
      { open: '"', close: '"', notIn: ["string"] },
      { open: "'", close: "'", notIn: ["string"] },
    ],
    indentationRules: {
      increaseIndentPattern: /^\s*(func|if|elif|else|for|while|match|class|static\s+func)\b.*:\s*(#.*)?$/,
      decreaseIndentPattern: /^\s*(elif|else)\b.*:\s*(#.*)?$/,
    },
  });
  monaco.languages.setMonarchTokensProvider("gdscript", {
    defaultToken: "",
    ignoreCase: false,
    keywords: [
      "if", "elif", "else", "for", "while", "match", "break", "continue", "pass",
      "return", "and", "or", "not", "in", "is", "as", "self", "super", "true",
      "false", "null", "func", "var", "const", "enum", "class", "class_name",
      "extends", "signal", "static", "void", "await", "yield", "breakpoint",
      "assert", "preload", "tool", "export", "onready", "setget", "when",
    ],
    typeKeywords: [
      "int", "float", "bool", "String", "StringName", "Vector2", "Vector2i",
      "Vector3", "Vector3i", "Vector4", "Vector4i", "Rect2", "Rect2i", "Color",
      "NodePath", "Array", "Dictionary", "Basis", "Quaternion", "Transform2D",
      "Transform3D", "Plane", "AABB", "Projection", "RID", "Callable", "Signal",
      "PackedByteArray", "PackedInt32Array", "PackedInt64Array", "PackedFloat32Array",
      "PackedFloat64Array", "PackedStringArray", "PackedVector2Array",
      "PackedVector3Array", "PackedColorArray", "Object", "Variant",
    ],
    tokenizer: {
      root: [
        [/@[a-zA-Z_]\w*/, "annotation"],
        [/[a-zA-Z_]\w*(?=\s*\()/, { cases: { "@keywords": "keyword", "@default": "entity.name.function" } }],
        [/[a-zA-Z_]\w*/, { cases: { "@keywords": "keyword", "@typeKeywords": "type", "@default": "identifier" } }],
        [/[$%][A-Za-z_][\w/]*/, "variable.predefined"],
        { include: "@whitespace" },
        [/\d+\.\d+([eE][-+]?\d+)?/, "number.float"],
        [/0[xX][0-9a-fA-F_]+|0[bB][01_]+|\d[\d_]*/, "number"],
        [/"""/, "string", "@tripleDouble"],
        [/'''/, "string", "@tripleSingle"],
        [/"/, "string", "@doubleString"],
        [/'/, "string", "@singleString"],
        [/[()\[\]{}]/, "delimiter.bracket"],
        [/[,:;.]/, "delimiter"],
        [/[=<>!+\-*/%&|^~]+/, "operator"],
      ],
      whitespace: [
        [/[ \t\r\n]+/, ""],
        [/#.*$/, "comment"],
      ],
      doubleString: [
        [/\\./, "string.escape"],
        [/"/, "string", "@pop"],
        [/[^\\"]+/, "string"],
      ],
      singleString: [
        [/\\./, "string.escape"],
        [/'/, "string", "@pop"],
        [/[^\\']+/, "string"],
      ],
      tripleDouble: [
        [/"""/, "string", "@pop"],
        [/\\./, "string.escape"],
        [/[^\\"]+/, "string"],
        [/"/, "string"],
      ],
      tripleSingle: [
        [/'''/, "string", "@pop"],
        [/\\./, "string.escape"],
        [/[^\\']+/, "string"],
        [/'/, "string"],
      ],
    },
  });
}

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
