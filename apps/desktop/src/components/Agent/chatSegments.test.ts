/// Tests del tokenitzador PUR del xat: separa prosa, blocs ```, targetes
/// «@file:» i directrius «@dir:»/«@delete:» (el que es pinta al xat).
import { describe, expect, it } from "vitest";
import { tokenizeChat } from "./chatSegments";
import { langFromPath, sniffLang } from "./codeHighlight";

describe("tokenizeChat", () => {
  it("separa la prosa dels blocs de codi amb tanca", () => {
    const segs = tokenizeChat("Mira això:\n```js\nconst x = 1;\n```\nAixò és tot.");
    expect(segs).toEqual([
      { kind: "prose", text: "Mira això:" },
      { kind: "code", lang: "js", code: "const x = 1;" },
      { kind: "prose", text: "Això és tot." },
    ]);
  });

  it("una tanca sense parell (streaming) es pinta amb el codi que porti", () => {
    const segs = tokenizeChat("```python\nprint('hola')\n");
    expect(segs).toEqual([{ kind: "code", lang: "python", code: "print('hola')" }]);
  });

  it("reconeix «@file:» i el converteix en targeta (fences dins són decoració)", () => {
    const segs = tokenizeChat("@file: app.py\n```python\nprint(1)\n```");
    expect(segs).toEqual([{ kind: "file", path: "app.py", code: "print(1)\n" }]);
  });

  it("tolera decoracions i numeració a les directrius («1. @file: x»)", () => {
    const segs = tokenizeChat("1. @file: src/a.py\nprint(2)");
    expect(segs).toEqual([{ kind: "file", path: "src/a.py", code: "print(2)\n" }]);
  });

  it("una altra directriu tanca el fitxer en curs (prosa no s'arrenega)", () => {
    const segs = tokenizeChat("@file: a.py\nprint(1)\n@file: b.py\nprint(2)");
    expect(segs).toEqual([
      { kind: "file", path: "a.py", code: "print(1)\n" },
      { kind: "file", path: "b.py", code: "print(2)\n" },
    ]);
  });

  it("«@dir:» i «@delete:» es converteixen en xips informatius", () => {
    const segs = tokenizeChat("@dir: assets/\n@delete: vell.txt");
    expect(segs).toEqual([
      { kind: "dir", path: "assets/" },
      { kind: "del", path: "vell.txt" },
    ]);
  });

  it("netja els artefactes de la interfície dins de la targeta", () => {
    const segs = tokenizeChat("@file: index.php\nPHP\nCopia\n<?php echo 1;");
    expect(segs).toEqual([{ kind: "file", path: "index.php", code: "<?php echo 1;\n" }]);
  });

  it("«RUN|» interromp el fitxer i torna a la prosa", () => {
    const segs = tokenizeChat("@file: a.py\nprint(1)\nRUN|python3 a.py");
    expect(segs).toEqual([
      { kind: "file", path: "a.py", code: "print(1)\n" },
      { kind: "prose", text: "RUN|python3 a.py" },
    ]);
  });

  it("les directrius dins d'una tanca ``` són codi, no directrius", () => {
    const segs = tokenizeChat("```txt\n@file: això_és_text_pla\n```");
    expect(segs).toEqual([{ kind: "code", lang: "txt", code: "@file: això_és_text_pla" }]);
  });
});

describe("langFromPath", () => {
  it("dedueix el llenguatge de l'extensió i dels noms especials", () => {
    expect(langFromPath("src/models/order.py")).toBe("py");
    expect(langFromPath("App.tsx")).toBe("tsx");
    expect(langFromPath("docker-compose.yml")).toBe("yaml");
    expect(langFromPath("Dockerfile")).toBe("dockerfile");
    expect(langFromPath("senseext")).toBeNull();
  });
});

describe("sniffLang", () => {
  it("endevina el llenguatge per l'aparença del codi", () => {
    expect(sniffLang("<?php echo 1;")).toBe("php");
    expect(sniffLang("<!doctype html>")).toBe("html");
    expect(sniffLang("#!/bin/sh")).toBe("sh");
    expect(sniffLang("def hola():\n    pass")).toBe("py");
    expect(sniffLang("fn main() {}")).toBe("rs");
    expect(sniffLang("const x = 1;")).toBe("js");
    expect(sniffLang('{"a": 1}')).toBe("json");
    expect(sniffLang("SELECT * FROM t")).toBe("sql");
  });
});
