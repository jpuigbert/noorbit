/// Tokenitzador PUR de les respostes de la IA per al xat: separa la prosa
/// dels blocs de codi amb tanca (```), de les targetes «@file:» i de les
/// directrius «@dir:»/«@delete:». Sense React ni stores: es pot testejar
/// sol (Vitest), igual que els parsers de fileBlocks.ts.
///
/// Les regles de reconeixement són LES MATEIXES que les de parseFileBlocks
/// (el que la IA diu a la pantalla és el que després es materialitza al
/// projecte): mateixa tolerància a marques decoratives, mateixes directrius
/// que interrompen un fitxer, mateixa neteja de camins.

import {
  BRANCH_DIRECTIVE_RE,
  normalizeBranchAction,
  stripUiArtifacts,
} from "../../stores/agent/fileBlocks";

export type ChatSeg =
  | { kind: "prose"; text: string }
  | { kind: "code"; lang: string; code: string }
  | { kind: "file"; path: string; code: string }
  | { kind: "dir"; path: string }
  | { kind: "del"; path: string }
  | { kind: "branch"; action: string; name: string };

/// Línia «@file: ruta» (tolerant a «file:», numeració «1.», decoracions).
const FILE_RE = /^[ \t>#*-]*(?:\d+[.)]\s+)?@?file:\s*(.+?)\s*$/i;
const DIR_RE = /^[ \t>#*-]*(?:\d+[.)]\s+)?@?(?:dir|directori|carpeta|folder)\w*:\s*(.+?)\s*$/i;
const DEL_RE = /^[ \t>#*-]*(?:\d+[.)]\s+)?@?(?:delete|esborra|borra|elimina|remove|rm)\w*:\s*(.+?)\s*$/i;
/// Directriu de BRANCA (Git): «@branch: create feature/x». La regex és la
/// MESMA que usa el materialitzador (`BRANCH_DIRECTIVE_RE` de fileBlocks):
/// «@» obligatori, perquè un «branch: main» d'un YAML no és cap directriu.
const BRANCH_RE = BRANCH_DIRECTIVE_RE;
/// Directrius que INTERROMPEN un bloc @file: en curs (mateixes que el
/// materialitzador): edicions, execució i arbres d'imatge.
const EDIT_RE = /^[ \t>#*-]*@?(?:edit|modifica|substitueix|reempla[çc]a)\w*:/i;
const RUN_RE = /^[ \t>#*-]*(?:RUN|IMG|NB)\|/i;

const cleanPath = (p: string) =>
  p
    .replace(/^[`'"*«]+|[`'"*»;:.,\s]+$/g, "")
    .trim()
    .replace(/^\/+/, "")
    .replace(/\\/g, "/");

const cleanDir = (p: string) =>
  p.replace(/^[`'"]+|[`'"]+$/g, "").trim().replace(/^\/+/, "").replace(/\\/g, "/");

/// Converteix el text d'un missatge en segments ordenats. Un fitxer
/// «@file:» sense tancar (resposta encara en streaming) es pinta amb el
/// codi que porti fins ara; el mateix passa amb una tanca ``` sense parell.
export function tokenizeChat(text: string): ChatSeg[] {
  const segs: ChatSeg[] = [];
  let prose: string[] = [];
  let file: { path: string; lines: string[] } | null = null;
  let fence: { lang: string; lines: string[] } | null = null;

  const flushProse = () => {
    if (prose.length) {
      segs.push({ kind: "prose", text: prose.join("\n") });
      prose = [];
    }
  };
  const flushFile = () => {
    if (file) {
      const code = stripUiArtifacts(
        file.lines.join("\n").replace(/^\s*\n+/, "").replace(/\s+$/, "")
      );
      if (code.trim()) segs.push({ kind: "file", path: file.path, code: `${code}\n` });
      file = null;
    }
  };
  const flushFence = () => {
    if (fence) {
      const code = fence.lines.join("\n").replace(/\s+$/, "");
      segs.push({ kind: "code", lang: fence.lang, code });
      fence = null;
    }
  };

  for (const line of text.split("\n")) {
    // Dins d'una tanca ``` només hi ha codi: res d'aquí és directriu.
    if (fence) {
      if (/^\s*```/.test(line)) flushFence();
      else fence.lines.push(line);
      continue;
    }
    const fm = FILE_RE.exec(line);
    if (fm) {
      flushProse();
      flushFile();
      file = { path: cleanPath(fm[1]), lines: [] };
      continue;
    }
    const dm = DIR_RE.exec(line);
    const rm = DEL_RE.exec(line);
    const dpath = dm ? cleanDir(dm[1]) : rm ? cleanDir(rm[1]) : "";
    if (dm || rm) {
      flushFile();
      flushProse();
      if (dpath) segs.push({ kind: dm ? "dir" : "del", path: dpath });
      else prose.push(line);
      continue;
    }
    // Directriu de BRANCA (Git): «@branch: create feature/x». L'acció es
    // NORMALITZA amb la mateixa funció que el materialitzador (fileBlocks):
    // «@branch: crea X» es veu com a «create», no com a esborrat.
    const bm = BRANCH_RE.exec(line);
    if (bm) {
      flushFile();
      flushProse();
      const body = bm[1].replace(/^[`'"]+|[`'"]+$/g, "").trim();
      const bp = body.split(/\s+/);
      const action = normalizeBranchAction(bp[0] || "");
      const bname = bp.slice(1).join(" ").trim();
      if (bname && action) {
        segs.push({ kind: "branch", action, name: bname });
      } else {
        prose.push(line);
      }
      continue;
    }
    // Dins d'@file:: les tanques ``` són decoració (el marc ja és la
    // targeta) i una altra directriu tanca aquest fitxer i comença el nou.
    if (file) {
      if (EDIT_RE.test(line) || RUN_RE.test(line)) {
        flushFile();
        prose.push(line);
        continue;
      }
      if (/^\s*```/.test(line)) continue;
      file.lines.push(line);
      continue;
    }
    if (/^\s*```/.test(line)) {
      flushProse();
      fence = {
        lang: (/^```([a-zA-Z0-9_+#.-]*)/.exec(line) ?? [])[1] || "",
        lines: [],
      };
      continue;
    }
    prose.push(line);
  }
  flushFile();
  flushFence();
  flushProse();
  return segs;
}
