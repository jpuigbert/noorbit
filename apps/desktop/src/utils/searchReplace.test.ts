/// Tests de les funcions PURES de cerca/substitució global (SearchPalette):
/// mode literal (escapat de caràcters regex i de «$&»), mode regex amb grups
/// de substitució, validació de patrons invàlids i límit de resultats.
import { describe, expect, it, vi } from "vitest";
import {
  buildSearchRegex,
  countMatches,
  escapeRegExp,
  findLineMatches,
  replaceAll,
  replaceAllInFile,
} from "./searchReplace";

describe("escapeRegExp", () => {
  it("escapa tots els metacaràcters de regex", () => {
    expect(escapeRegExp("a.b*c?")).toBe("a\\.b\\*c\\?");
    expect(escapeRegExp("(x)[y]{z}")).toBe("\\(x\\)\\[y\\]\\{z\\}");
    expect(escapeRegExp("$^|\\/")).toBe("\\$\\^\\|\\\\/");
  });
});

describe("buildSearchRegex", () => {
  it("mode literal: el patró és text pla (els metacaràcters no actuen)", () => {
    const re = buildSearchRegex("a.b", false);
    expect(re.test("a.b")).toBe(true);
    expect(re.test("axb")).toBe(false);
    expect(re.flags).toContain("g");
    expect(re.flags).toContain("i");
  });

  it("mode regex: el patró actua com a expressió regular", () => {
    const re = buildSearchRegex("fo+\\d+", true);
    expect(re.test("foo42")).toBe(true);
    expect(re.test("bar")).toBe(false);
  });

  it("mode regex: patró invàlid LLANÇA (la UI mostra l'error, mai cerca buida)", () => {
    expect(() => buildSearchRegex("(sense tancar", true)).toThrow();
    expect(() => buildSearchRegex("[a-", true)).toThrow();
  });

  it("mode literal: un patró que seria regex invàlid NO llança", () => {
    expect(() => buildSearchRegex("(sense tancar", false)).not.toThrow();
  });
});

describe("findLineMatches", () => {
  const content = "Hola món\nun altre hola\nHOLA final";

  it("retorna línia i columna 1-based i la preview de la línia", () => {
    const ms = findLineMatches(content, buildSearchRegex("hola", false));
    expect(ms).toHaveLength(3);
    expect(ms[0]).toEqual({ line: 1, column: 1, matchLen: 4, preview: "Hola món" });
    expect(ms[1].line).toBe(2);
    expect(ms[1].column).toBe(10);
    expect(ms[2].line).toBe(3);
  });

  it("és case-insensitive (com el backend literal)", () => {
    const ms = findLineMatches(content, buildSearchRegex("HOLA", false));
    expect(ms).toHaveLength(3);
  });

  it("diverses coincidències a la mateixa línia", () => {
    const ms = findLineMatches("a a a", buildSearchRegex("a", false));
    expect(ms.map((m) => m.column)).toEqual([1, 3, 5]);
    expect(ms.every((m) => m.line === 1)).toBe(true);
  });

  it("el límit talla la cerca (avís de truncament a la UI)", () => {
    const ms = findLineMatches("x x x x x", buildSearchRegex("x", false), 3);
    expect(ms).toHaveLength(3);
  });

  it("mode regex amb grups: posicions de la coincidència sencera", () => {
    const ms = findLineMatches("num=42 i num=7", buildSearchRegex("num=(\\d+)", true));
    expect(ms).toHaveLength(2);
    expect(ms[0]).toMatchObject({ line: 1, column: 1, matchLen: 6 });
    expect(ms[1]).toMatchObject({ column: 10, matchLen: 5 });
  });
});

describe("countMatches", () => {
  it("compta totes les coincidències (no per línia)", () => {
    expect(countMatches("a a\na", buildSearchRegex("a", false))).toBe(3);
  });

  it("les coincidències de longitud zero no compten (no són un canvi)", () => {
    expect(countMatches("abc", buildSearchRegex("x*", true))).toBe(0);
  });
});

describe("replaceAll", () => {
  it("mode literal: substitueix totes i compta", () => {
    const r = replaceAll("foo bar foo", buildSearchRegex("foo", false), "baz", false);
    expect(r).toEqual({ text: "baz bar baz", count: 2 });
  });

  it("mode literal: «$&» i «$1» al text de substitució són LITERALS", () => {
    const r = replaceAll("preu", buildSearchRegex("preu", false), "$& $1", false);
    expect(r.text).toBe("$& $1");
    expect(r.count).toBe(1);
  });

  it("mode regex: els grups ($1, $&) funcionen com a JavaScript", () => {
    const r = replaceAll("num=42", buildSearchRegex("num=(\\d+)", true), "val[$1]", true);
    expect(r).toEqual({ text: "val[42]", count: 1 });
  });

  it("mode literal amb metacaràcters: cerca el text exacte", () => {
    const r = replaceAll("f(x) i f(y)", buildSearchRegex("f(", false), "g(", false);
    expect(r.text).toBe("g(x) i g(y)");
    expect(r.count).toBe(2);
  });

  it("sense coincidències: text intacte i count 0", () => {
    const r = replaceAll("igual", buildSearchRegex("altre", false), "nou", false);
    expect(r).toEqual({ text: "igual", count: 0 });
  });

  it("substitució multilínia (regex . no creua línies, \\n sí)", () => {
    const r = replaceAll("a\nb\na", buildSearchRegex("a", true), "z", true);
    expect(r).toEqual({ text: "z\nb\nz", count: 2 });
  });
});

describe("replaceAllInFile (carrera del «Substitueix tot»)", () => {
  it("sense carrera: escriu al disc i actualitza la pestanya amb el text nou", async () => {
    let live = "foo foo";
    const writes: string[] = [];
    const n = await replaceAllInFile("/proj/a.txt", buildSearchRegex("foo", false), "bar", false, {
      readFile: async () => live,
      writeFile: async (_p, c) => {
        writes.push(c);
      },
      getTab: () => ({ content: live }),
      setTab: (_p, c) => {
        live = c;
      },
    });
    expect(n).toBe(2);
    expect(writes).toEqual(["bar bar"]);
    expect(live).toBe("bar bar");
  });

  it("el que l'usuari tecleja DURANT l'escriptura es conserva (reaplicació en viu)", async () => {
    let live = "foo foo";
    const writes: string[] = [];
    let release!: () => void;
    const gate = new Promise<void>((res) => {
      release = res;
    });
    const p = replaceAllInFile("/proj/a.txt", buildSearchRegex("foo", false), "bar", false, {
      readFile: async () => live,
      writeFile: async (_p, c) => {
        writes.push(c);
        await gate; // l'escriptura al disc és lenta: l'usuari tecleja mentrestant
      },
      getTab: () => ({ content: live }),
      setTab: (_p, c) => {
        live = c;
      },
    });
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    // L'usuari tecleja mentre el write és en curs (abans: setOpenFile ho aixafava).
    live = "foo foo TECLEJAT";
    release();
    const n = await p;
    expect(n).toBe(2);
    // La pestanya conserva el teclejat + el reemplaçament reaplicat.
    expect(live).toBe("bar bar TECLEJAT");
    // I el disc queda alineat amb l'editor (segon write amb el text viu).
    expect(writes).toEqual(["bar bar", "bar bar TECLEJAT"]);
  });

  it("fitxer NO obert: llegeix del disc i no toca cap pestanya", async () => {
    const writes: string[] = [];
    const setTab = vi.fn();
    const n = await replaceAllInFile("/proj/a.txt", buildSearchRegex("foo", false), "bar", false, {
      readFile: async () => "foo",
      writeFile: async (_p, c) => {
        writes.push(c);
      },
      getTab: () => undefined,
      setTab,
    });
    expect(n).toBe(1);
    expect(writes).toEqual(["bar"]);
    expect(setTab).not.toHaveBeenCalled();
  });

  it("sense coincidències: no escriu res i retorna 0", async () => {
    const writeFile = vi.fn(async () => undefined);
    const n = await replaceAllInFile("/proj/a.txt", buildSearchRegex("zzz", false), "bar", false, {
      readFile: async () => "igual",
      writeFile,
      getTab: () => ({ content: "igual" }),
      setTab: vi.fn(),
    });
    expect(n).toBe(0);
    expect(writeFile).not.toHaveBeenCalled();
  });

  it("pestanya tancada durant el write: no es reobri (setTab no es crida)", async () => {
    let open = true;
    const setTab = vi.fn();
    const n = await replaceAllInFile("/proj/a.txt", buildSearchRegex("foo", false), "bar", false, {
      readFile: async () => "foo",
      writeFile: async () => {
        open = false; // l'usuari tanca la pestanya mentre s'escriu
      },
      getTab: () => (open ? { content: "foo" } : undefined),
      setTab,
    });
    expect(n).toBe(1);
    expect(setTab).not.toHaveBeenCalled();
  });
});