/// Tests dels parsers PURS de la resposta de la IA: blocs «@file:»,
/// carpetes, esborrat, edicions quirúrgiques, rescat de prosa i arbres.
import { describe, expect, it } from "vitest";
import {
  applyEditsToContent,
  esBlocDeDocumentacio,
  esContingutPlaceholder,
  extractFromProse,
  extractRecommendedPaths,
  extractSingleFileContent,
  extractTreePaths,
  fragmentMesSemblant,
  nomesDocumentacio,
  parseDeleteDirectives,
  parseDirDirectives,
  parseEditBlocks,
  parseFileBlocks,
  parseManifest,
  relForWrite,
  esRutaForaDeRelatiu,
  stripUiArtifacts,
} from "./fileBlocks";
import type { ProseSkip } from "./fileBlocks";

describe("parseFileBlocks", () => {
  it("amb tanques ``` només agafa el codi de DINS (la prosa no s'arrenega)", () => {
    const raw = [
      "@file: hola.py",
      "```python",
      "print('hola')",
      "```",
      "Això és prosa posterior que NO ha d'anar al fitxer.",
      "RUN|python3 hola.py",
    ].join("\n");
    const b = parseFileBlocks(raw);
    expect(b).toHaveLength(1);
    expect(b[0].path).toBe("hola.py");
    expect(b[0].content).toBe("print('hola')\n");
  });

  it("sense tanques agafa les línies nues fins a la primera directriu", () => {
    const raw = "@file: a.txt\nlínia 1\nlínia 2\n@dir: carpeta/\nmés text";
    const b = parseFileBlocks(raw);
    expect(b[0].content).toBe("línia 1\nlínia 2\n");
  });

  it("netja els artefactes de la interfície (etiqueta de llenguatge i «Copia»)", () => {
    const raw = "@file: index.php\nPHP\nCopia\n<?php echo 1;";
    expect(parseFileBlocks(raw)[0].content).toBe("<?php echo 1;\n");
  });

  it("ignora blocs sense contingut útil i tolera el @ opcional i majúscules", () => {
    expect(parseFileBlocks("@file: buit.py\n@dir: x/")).toEqual([]);
    const b = parseFileBlocks("FILE: Majus.py\ncodi");
    expect(b[0].path).toBe("Majus.py");
  });
});

describe("stripUiArtifacts", () => {
  it("treu etiquetes i botons del davant, però no codi real", () => {
    expect(stripUiArtifacts("Python\nCopia\nimport os")).toBe("import os");
    expect(stripUiArtifacts("import os")).toBe("import os");
  });
});

describe("parseManifest", () => {
  it("netja vinyetes, numeració, cometes i rutes «./»", () => {
    const raw = "- src/a.py\n1. ./src/b.py\n`src/c.py`\n* src/a.py";
    expect(parseManifest(raw)).toEqual(["src/a.py", "src/b.py", "src/c.py"]);
  });

  it("descarta no-rutes i traversal «..»", () => {
    expect(parseManifest("això és prosa\n../etc/passwd\nsrc/../a.py")).toEqual([]);
  });
});

describe("parseDirDirectives", () => {
  it("accepta àlies i rebutja rutes que són FITXERS (extensió coneguda)", () => {
    const raw = "@dir: assets/\n@carpeta: admin/index.php\n@folder: src/utils";
    expect(parseDirDirectives(raw)).toEqual(["assets/", "src/utils"]);
  });

  it("rebutja «..» i deduplica", () => {
    expect(parseDirDirectives("@dir: a/\n@dir: a/\n@dir: ../fora")).toEqual(["a/"]);
  });
});

describe("parseDeleteDirectives", () => {
  it("tolera àlies catalans, normalitza barres i deduplica", () => {
    const raw = "@delete: a.py\n@esborra: b\\c.py\n@borra: a.py\n@rm: ../fora";
    expect(parseDeleteDirectives(raw)).toEqual(["a.py", "b/c.py"]);
  });

  it("conserva la BARRA INICIAL de les absolutes (perquè es puguin relativitzar)", () => {
    expect(parseDeleteDirectives("@delete: /Volumes/Toshi/proj/a.py")).toEqual([
      "/Volumes/Toshi/proj/a.py",
    ]);
  });
});

describe("relForWrite", () => {
  const ARREL = "/Volumes/Toshi/programes/forestin the sky";

  it("la ruta relativa del protocol es respecta TAL QUAL", () => {
    expect(relForWrite(ARREL, "main.py")).toBe("main.py");
    expect(relForWrite(ARREL, "src/main.py")).toBe("src/main.py");
    expect(relForWrite(ARREL, "src\\app.py")).toBe("src/app.py");
    expect(relForWrite(ARREL, "  assets/  ")).toBe("assets/");
    // Una relativa mai es retalla, ni quan el seu primer segment coincideix
    // amb el nom de l'arrel.
    expect(relForWrite("/Users/x/proj/sky", "sky/main.py")).toBe("sky/main.py");
  });

  it("relativitza l'absoluta que és SOTA l'arrel (cas del volum extern)", () => {
    expect(relForWrite(ARREL, `${ARREL}/main.py`)).toBe("main.py");
    expect(relForWrite(ARREL, `${ARREL}/src/config.py`)).toBe("src/config.py");
    // L'arrel duplicada dins la ruta (la forma real del transcript).
    expect(relForWrite(ARREL, `${ARREL}${ARREL}/main.py`)).toBe("main.py");
    // L'arrel escrita sense barra inicial.
    expect(relForWrite(ARREL, "/" + ARREL.slice(1) + "/sky/main.py")).toBe("sky/main.py");
    // Barres inverses al cos de la ruta: es normalitzen.
    expect(relForWrite(ARREL, `${ARREL}/sky\\main.py`)).toBe("sky/main.py");
  });

  it("rebutja l'absoluta FORA de l'arrel, l'escapament i l'arrel mateixa", () => {
    expect(relForWrite(ARREL, "/Users/altra/cosa/main.py")).toBeNull();
    expect(relForWrite(ARREL, "/etc/passwd")).toBeNull();
    // Un prefix comú no la fa del projecte: no se'n retalla res.
    expect(relForWrite(ARREL, "/Volumes/Toshi/programes/altre joc/main.py")).toBeNull();
    expect(relForWrite(ARREL, ARREL)).toBeNull();
    expect(relForWrite(ARREL, "../fora.py")).toBeNull();
    expect(relForWrite(ARREL, "")).toBeNull();
  });

  it("«esRutaForaDeRelatiu» només marca les absolutes realment foranes", () => {
    expect(esRutaForaDeRelatiu(ARREL, "/Volumes/Altre/joc/a.py")).toBe(true);
    expect(esRutaForaDeRelatiu(ARREL, `${ARREL}/a.py`)).toBe(false);
    expect(esRutaForaDeRelatiu(ARREL, "a.py")).toBe(false);
    expect(esRutaForaDeRelatiu(ARREL, "../a.py")).toBe(false);
  });
});

describe("extractRecommendedPaths", () => {
  it("recull @file:, @dir: i rutes en prosa (carpetes amb «/» final)", () => {
    const raw = [
      "@file: src/main.py",
      "print(1)",
      "@dir: assets/",
      "Hauries de crear src/models/order.py i una carpeta src/utils/",
    ].join("\n");
    const r = extractRecommendedPaths(raw);
    expect(r).toContain("src/main.py");
    expect(r).toContain("assets/");
    expect(r).toContain("src/models/order.py");
    expect(r).toContain("src/utils/");
  });

  it("descarta URLs «www.», rutes node_modules i traversal «..»", () => {
    const raw = "Mira https://www.example.com/a.py i node_modules/x.js i ../fora.py";
    const r = extractRecommendedPaths(raw);
    expect(r).not.toContain("www.example.com/a.py");
    expect(r).not.toContain("node_modules/x.js");
    expect(r).not.toContain("../fora.py");
  });
});

describe("extractTreePaths", () => {
  it("mapeja noms de fitxer a la carpeta de l'arbre ASCII", () => {
    const raw = [
      "store-wp-one/",
      "├── style.css",
      "└── inc/",
      "    └── options.php",
    ].join("\n");
    const m = extractTreePaths(raw);
    expect(m.get("style.css")).toBe("store-wp-one/style.css");
    expect(m.get("options.php")).toBe("store-wp-one/inc/options.php");
  });
});

describe("parseEditBlocks", () => {
  it("extreu CERCA/NOU i acumula dos @edit: del mateix fitxer", () => {
    const raw = [
      "@edit: a.py",
      "<<<<<<< CERCA",
      "vell",
      "=======",
      "nou",
      ">>>>>>>",
      "@edit: a.py",
      "<<<<<<< CERCA",
      "altre",
      "=======",
      ">>>>>>>",
    ].join("\n");
    const b = parseEditBlocks(raw);
    expect(b).toHaveLength(1);
    expect(b[0].path).toBe("a.py");
    expect(b[0].edits).toEqual([
      { find: "vell", replace: "nou" },
      { find: "altre", replace: "" }, // NOU buit = esborrar
    ]);
  });

  it("tolera seccions sense tancar i descarta blocs sense edicions", () => {
    const raw = "@edit: a.py\n<<<<<<< CERCA\nvell\n=======\nnou";
    expect(parseEditBlocks(raw)[0].edits).toEqual([{ find: "vell", replace: "nou" }]);
    expect(parseEditBlocks("@edit: buit.py\nprosa sense marcadors")).toEqual([]);
  });

  it("conserva les seccions no-op (CERCA == NOU) perquè l'aplicador les rebutge", () => {
    // applyEditBlocks és qui descarta el bloc idèntic i ho informa al xat;
    // el parser no l'ha de fer desaparèixer en silenci.
    const raw = "@edit: req.txt\n<<<<<<< CERCA\nflask\n=======\nflask\n>>>>>>>";
    expect(parseEditBlocks(raw)[0].edits).toEqual([{ find: "flask", replace: "flask" }]);
  });
});

describe("extractSingleFileContent / documentació", () => {
  it("agafa el bloc @file: més gran; si no n'hi ha, la tanca ```; si no, el text", () => {
    expect(extractSingleFileContent("@file: a.py\nAAA\n@file: b.py\nBBBBBB")).toBe("BBBBBB");
    expect(extractSingleFileContent("```py\ncodi\n```")).toBe("codi");
    expect(extractSingleFileContent("<?php echo 1;")).toBe("<?php echo 1;");
  });

  it("esBlocDeDocumentacio detecta els bloc_N.md inventats", () => {
    expect(esBlocDeDocumentacio("bloc_1.md")).toBe(true);
    expect(esBlocDeDocumentacio("bloc-2.txt")).toBe(true);
    expect(esBlocDeDocumentacio("src/main.py")).toBe(false);
  });

  it("nomesDocumentacio: tots els blocs són .md/.txt", () => {
    expect(nomesDocumentacio("@file: a.md\nx\n@file: b.txt\ny")).toBe(true);
    expect(nomesDocumentacio("@file: a.md\nx\n@file: b.py\ny")).toBe(false);
    expect(nomesDocumentacio("sense blocs")).toBe(false);
  });
});

describe("extractFromProse (rescat)", () => {
  it("dedueix el nom del comentari inicial i del text adjacent", () => {
    const raw = "Ací tens main.py:\n```python\n# main.py\nprint(1)\n```";
    const r = extractFromProse(raw);
    expect(r).toHaveLength(1);
    expect(r[0].path).toBe("main.py");
    expect(r[0].content).toBe("# main.py\nprint(1)\n");
  });

  it("si el model repeteix el fitxer, es queda l'última versió (sense _2, _3)", () => {
    const raw = "```python\n# a.py\nv1\n```\n```python\n# a.py\nv2\n```";
    const r = extractFromProse(raw);
    expect(r).toHaveLength(1);
    expect(r[0].content).toBe("# a.py\nv2\n");
  });

  it("rescata l'últim bloc SENSE tancar (resposta tallada)", () => {
    const raw = "```python\n# tallat.py\nprint('a mitges')";
    const r = extractFromProse(raw);
    expect(r).toHaveLength(1);
    expect(r[0].path).toBe("tallat.py");
  });

  it("usa l'arbre del projecte per posar el fitxer a la carpeta correcta", () => {
    // L'arbre només s'usa quan el nom apareix a prop de la tanca però amb una
    // extensió NO reconeguda (p. ex. .blade): si no, el nom ja s'ha deduït abans.
    const raw = [
      "tema/",
      "└── plantilla.blade",
      "Ací tens plantilla.blade:",
      "```blade",
      "<h1>Hola</h1>",
      "```",
    ].join("\n");
    const r = extractFromProse(raw);
    expect(r[0].path).toBe("tema/plantilla.blade");
  });
});

describe("esContingutPlaceholder", () => {
  it("detecta el placeholder PROHIBIT a disc", () => {
    expect(
      esContingutPlaceholder("a.php\n\n(Pendent: el model no ha retornat contingut per a aquest fitxer)\n"),
    ).toBe(true);
    expect(esContingutPlaceholder("<?php echo 1;")).toBe(false);
    expect(esContingutPlaceholder("")).toBe(false);
  });
});

describe("parseFileBlocks onlyClosed (buffer del live materialize)", () => {
  it("un fence OBERT (preview parcial) NO s'escriu; el tancat sí", () => {
    const raw = [
      "@file: fet.py",
      "```python",
      "print('fet')",
      "```",
      "@file: a_mitges.py",
      "```python",
      "print('a mit",
    ].join("\n");
    const closed = parseFileBlocks(raw, { onlyClosed: true });
    expect(closed).toHaveLength(1);
    expect(closed[0].path).toBe("fet.py");
    // Sense l'opció, el comportament antic es manté (autoMaterialize final).
    expect(parseFileBlocks(raw)).toHaveLength(2);
  });

  it("un bloc tallat per una directriu posterior compta com a tancat", () => {
    const raw = "@file: a.txt\nlínia 1\n@dir: carpeta/\nprosa";
    const closed = parseFileBlocks(raw, { onlyClosed: true });
    expect(closed).toHaveLength(1);
    expect(closed[0].content).toBe("línia 1\n");
  });

  it("un bloc sense tanques al final del stream encara pot créixer → no tancat", () => {
    const raw = "@file: bare.py\nprint(1)";
    expect(parseFileBlocks(raw, { onlyClosed: true })).toEqual([]);
  });

  it("el fence es tanca en un fragment posterior: el bloc apareix llavors", () => {
    const obert = "@file: x.js\n```js\nconsole.log(1);";
    expect(parseFileBlocks(obert, { onlyClosed: true })).toEqual([]);
    const tancat = obert + "\n```";
    const r = parseFileBlocks(tancat, { onlyClosed: true });
    expect(r).toHaveLength(1);
    expect(r[0].content).toBe("console.log(1);\n");
  });
});

describe("extractFromProse: mai rutes inventades «bloc_N»", () => {
  it("un bloc sense nom ni context S'OMET amb avís (abans: bloc_1.py)", () => {
    const skipped: ProseSkip[] = [];
    const raw = "Ací tens el codi:\n```python\nprint(1)\n```";
    const r = extractFromProse(raw, { skipped });
    expect(r).toEqual([]);
    expect(skipped).toHaveLength(1);
    expect(skipped[0].reason).toBe("sense-nom");
    expect(skipped[0].preview).toBe("print(1)");
  });

  it("amb manifest, un bloc sense nom va a l'ÚNIC candidat de l'extensió", () => {
    const raw = "```python\nprint(1)\n```";
    const r = extractFromProse(raw, { manifest: ["src/main.py", "README.md"] });
    expect(r).toHaveLength(1);
    expect(r[0].path).toBe("src/main.py");
  });

  it("amb dos candidats de la mateixa extensió és AMBIGU → s'omet amb avís", () => {
    const skipped: ProseSkip[] = [];
    const raw = "```python\nprint(1)\n```";
    const r = extractFromProse(raw, { manifest: ["a.py", "b.py"], skipped });
    expect(r).toEqual([]);
    expect(skipped[0].reason).toBe("ambigu");
  });

  it("el manifest MANA sobre el nom deduït (basename → ruta del pla)", () => {
    const raw = "```css\n/* style.css */\nbody{}\n```";
    const r = extractFromProse(raw, { manifest: ["tema-x/style.css"] });
    expect(r[0].path).toBe("tema-x/style.css");
  });

  it("cap resultat conté mai una ruta «bloc_N.ext»", () => {
    const raw = "```js\nconsole.log(1)\n```\n```python\nprint(2)\n```";
    const r = extractFromProse(raw);
    expect(r.every((f) => !/^bloc_\d+\./.test(f.path))).toBe(true);
  });
});

describe("fragmentMesSemblant (feedback difús per al reintent)", () => {
  const contingut = "def suma(a, b):\n    return a + b\n\ndef resta(a, b):\n    return a - b\n";

  it("troba la finestra amb màxima coincidència normalitzada (espais/majúscules)", () => {
    const f = fragmentMesSemblant(contingut, "DEF SUMA(a, b):\n        return a + b");
    expect(f).not.toBeNull();
    expect(f!.startLine).toBe(1);
    expect(f!.endLine).toBe(2);
    expect(f!.score).toBe(1);
    expect(f!.lines[0]).toBe("def suma(a, b):");
  });

  it("coincidència parcial: retorna la millor finestra amb score < 1", () => {
    const f = fragmentMesSemblant(contingut, "return a + b\nreturn a * b");
    expect(f).not.toBeNull();
    expect(f!.startLine).toBe(2);
    expect(f!.score).toBe(0.5);
  });

  it("sense cap línia semblant → null (no s'inventa feedback)", () => {
    expect(fragmentMesSemblant(contingut, "zzz\nqqq")).toBeNull();
    expect(fragmentMesSemblant(contingut, "   ")).toBeNull();
  });

  it("un CERCA llarg es retalla a 16 línies mostrades", () => {
    const big = Array.from({ length: 30 }, (_, i) => `line ${i}`).join("\n");
    const cerca = Array.from({ length: 20 }, (_, i) => `line ${i}`).join("\n");
    const f = fragmentMesSemblant(big, cerca);
    expect(f!.lines).toHaveLength(16);
    expect(f!.startLine).toBe(1);
    expect(f!.endLine).toBe(16);
  });
});

describe("applyEditsToContent (matching difús + no-ops)", () => {
  it("coincidència exacta i per línies retallades (indentació diferent)", () => {
    const old = "def f():\n    return 1\n";
    const exact = applyEditsToContent(old, [{ find: "return 1", replace: "return 2" }]);
    expect(exact.ok).toBe(1);
    expect(exact.updated).toBe("def f():\n    return 2\n");
    const trimmed = applyEditsToContent(old, [
      { find: "def f():\nreturn 1", replace: "def f():\n    return 3" },
    ]);
    expect(trimmed.ok).toBe(1);
    expect(trimmed.updated).toContain("return 3");
  });

  it("rebutja els no-ops (CERCA == NOU): el contingut NO canvia", () => {
    const old = "flask\n";
    const r = applyEditsToContent(old, [{ find: "flask", replace: "flask" }]);
    expect(r.noop).toBe(1);
    expect(r.ok).toBe(0);
    expect(r.updated).toBe(old);
  });

  it("un CERCA que no coincideix NO toca el fitxer i reporta el fragment real", () => {
    const old = "def suma(a, b):\n    return a + b\n";
    const r = applyEditsToContent(old, [
      { find: "def suma(a, b):\n    return a - b", replace: "xxx" },
    ]);
    expect(r.ok).toBe(0);
    expect(r.fail).toBe(1);
    expect(r.updated).toBe(old);
    expect(r.failures).toHaveLength(1);
    const frag = r.failures[0].fragment;
    expect(frag).not.toBeNull();
    expect(frag!.startLine).toBe(1);
    expect(frag!.lines[0]).toBe("def suma(a, b):");
  });

  it("sense cap fragment semblant, failures porta fragment null", () => {
    const r = applyEditsToContent("aaa\n", [{ find: "zzz", replace: "xxx" }]);
    expect(r.fail).toBe(1);
    expect(r.failures[0].fragment).toBeNull();
  });

  it("el text NOU és LITERAL: $&/$'/$` no s'expandeixen (codi JS/regex intacte)", () => {
    const old = "const re = /OLD/;\n";
    const nou = "const re = /$&$'$`$$/;";
    const r = applyEditsToContent(old, [{ find: "const re = /OLD/;", replace: nou }]);
    expect(r.ok).toBe(1);
    expect(r.updated).toBe(nou + "\n");
  });

  it("un replace ABSENT esborra el fragment (mai el text «undefined»)", () => {
    const old = "a\nESBORRA\nb\n";
    // El parser pot produir un replace absent en runtime (resposta tallada).
    const r = applyEditsToContent(old, [
      { find: "ESBORRA\n", replace: undefined as unknown as string },
    ]);
    expect(r.ok).toBe(1);
    expect(r.updated).toBe("a\nb\n");
    expect(r.updated).not.toContain("undefined");
  });
});