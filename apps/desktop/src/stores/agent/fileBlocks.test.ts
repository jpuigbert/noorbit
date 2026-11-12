/// Tests dels parsers PURS de la resposta de la IA: blocs «@file:»,
/// carpetes, esborrat, edicions quirúrgiques, rescat de prosa i arbres.
import { describe, expect, it } from "vitest";
import {
  esBlocDeDocumentacio,
  extractFromProse,
  extractRecommendedPaths,
  extractSingleFileContent,
  extractTreePaths,
  nomesDocumentacio,
  parseDeleteDirectives,
  parseDirDirectives,
  parseEditBlocks,
  parseFileBlocks,
  parseManifest,
  stripUiArtifacts,
} from "./fileBlocks";

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