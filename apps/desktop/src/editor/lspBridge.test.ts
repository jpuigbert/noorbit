/// Tests del pont LSP (editor/lspBridge.ts, Fase 3c) amb `invoke` MOQUEJAT:
/// contracte EXACTE dels comandaments, degradació honesta (mai un error que
/// trenque l'editor), mapatges purs LSP→Monaco i debounce/cancel·lació dels
/// diagnòstics. Cap backend real ni Monaco real en estos tests.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Mocks ESTABLES entre resetModules (la fàbrica es reavalua, el fn no).
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  applyLspMarkers: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./diagnostics", () => ({ applyLspMarkers: mocks.applyLspMarkers }));

type Bridge = typeof import("./lspBridge");
let bridge: Bridge;

beforeEach(async () => {
  vi.resetModules(); // estat net: catàleg, timers, guard de providers
  mocks.invoke.mockReset();
  mocks.applyLspMarkers.mockReset();
  bridge = await import("./lspBridge");
});

afterEach(() => {
  vi.useRealTimers();
});

describe("contracte exacte dels comandaments", () => {
  it("lsp_completion rep {projectPath, path, line, col} i torna les entrades", async () => {
    const entries = [{ label: "println!", insertText: "println!", detail: "macro", kind: 3 }];
    mocks.invoke.mockResolvedValue(entries);
    const out = await bridge.lspCompletion("/proj", "/proj/a.rs", 4, 7);
    expect(mocks.invoke).toHaveBeenCalledWith("lsp_completion", {
      projectPath: "/proj",
      path: "/proj/a.rs",
      line: 4,
      col: 7,
    });
    expect(out).toEqual(entries);
  });

  it("lsp_hover rep les mateixes claus i torna {contents}", async () => {
    mocks.invoke.mockResolvedValue({ contents: "fn foo() -> i32" });
    const out = await bridge.lspHover("/proj", "/proj/a.rs", 2, 3);
    expect(mocks.invoke).toHaveBeenCalledWith("lsp_hover", {
      projectPath: "/proj",
      path: "/proj/a.rs",
      line: 2,
      col: 3,
    });
    expect(out).toEqual({ contents: "fn foo() -> i32" });
  });

  it("lsp_references torna [{path, line, col}]", async () => {
    const refs = [{ path: "src/b.rs", line: 10, col: 5 }];
    mocks.invoke.mockResolvedValue(refs);
    const out = await bridge.lspReferences("/proj", "/proj/a.rs", 1, 1);
    expect(mocks.invoke).toHaveBeenCalledWith("lsp_references", {
      projectPath: "/proj",
      path: "/proj/a.rs",
      line: 1,
      col: 1,
    });
    expect(out).toEqual(refs);
  });

  it("lsp_definition rep {projectPath, path, line, col} i torna {path, line, column, preview} (1-based)", async () => {
    const def = { path: "src/b.rs", line: 12, column: 3, preview: "pub fn foo() {" };
    mocks.invoke.mockResolvedValue(def);
    const out = await bridge.lspDefinition("/proj", "/proj/a.rs", 4, 7);
    expect(mocks.invoke).toHaveBeenCalledWith("lsp_definition", {
      projectPath: "/proj",
      path: "/proj/a.rs",
      line: 4,
      col: 7,
    });
    expect(out).toEqual(def);
  });

  it("lsp_definition sense definició (null del backend) → null, cap error", async () => {
    mocks.invoke.mockResolvedValue(null);
    expect(await bridge.lspDefinition("/proj", "/proj/a.rs", 1, 1)).toBeNull();
  });

  it("lsp_definition amb backend mort → null (fallback heurístic qui crida)", async () => {
    mocks.invoke.mockRejectedValue(new Error("sense servidor"));
    expect(await bridge.lspDefinition("/proj", "/proj/a.rs", 1, 1)).toBeNull();
  });

  it("lsp_definition que no respon mai → null per timeout (8,5 s)", async () => {
    vi.useFakeTimers();
    mocks.invoke.mockReturnValue(new Promise(() => undefined)); // penja
    const p = bridge.lspDefinition("/proj", "/proj/a.rs", 2, 3);
    await vi.advanceTimersByTimeAsync(8500);
    expect(await p).toBeNull();
  });

  it("lsp_diagnostics NOMÉS rep {projectPath, path}", async () => {
    const diags = [{ line: 3, col: 1, severity: "error", message: "boom", source: "tsserver" }];
    mocks.invoke.mockResolvedValue(diags);
    const out = await bridge.lspDiagnostics("/proj", "/proj/a.ts");
    expect(mocks.invoke).toHaveBeenCalledWith("lsp_diagnostics", {
      projectPath: "/proj",
      path: "/proj/a.ts",
    });
    expect(out).toEqual(diags);
  });

  it("lsp_status rep {projectPath} i torna {server, language, ready}", async () => {
    const st = { server: "rust-analyzer", language: "rust", ready: true };
    mocks.invoke.mockResolvedValue(st);
    expect(await bridge.lspStatus("/proj")).toEqual(st);
    expect(mocks.invoke).toHaveBeenCalledWith("lsp_status", { projectPath: "/proj" });
  });

  it("list_lsp_servers es consulta sense arguments", async () => {
    mocks.invoke.mockResolvedValue([{ language: "rust", binary: "rust-analyzer", installed: true }]);
    const out = await bridge.listLspServers();
    expect(mocks.invoke).toHaveBeenCalledWith("list_lsp_servers");
    expect(out[0].installed).toBe(true);
  });
});

describe("degradació honesta (mai error cap a l'editor)", () => {
  it("invoke rebutjat → llista buida / null / estat buit", async () => {
    mocks.invoke.mockRejectedValue(new Error("backend mort"));
    expect(await bridge.lspCompletion("/p", "/p/a.rs", 1, 1)).toEqual([]);
    expect(await bridge.lspHover("/p", "/p/a.rs", 1, 1)).toBeNull();
    expect(await bridge.lspReferences("/p", "/p/a.rs", 1, 1)).toEqual([]);
    expect(await bridge.lspDefinition("/p", "/p/a.rs", 1, 1)).toBeNull();
    expect(await bridge.lspDiagnostics("/p", "/p/a.ts")).toEqual([]);
    expect(await bridge.lspStatus("/p")).toEqual({ server: null, language: null, ready: false });
    expect(await bridge.listLspServers()).toEqual([]);
  });

  it("invoke resol null (comandament degradat) → mateixos valors per defecte", async () => {
    mocks.invoke.mockResolvedValue(null);
    expect(await bridge.lspCompletion("/p", "/p/a.rs", 1, 1)).toEqual([]);
    expect(await bridge.lspHover("/p", "/p/a.rs", 1, 1)).toBeNull();
    expect(await bridge.lspReferences("/p", "/p/a.rs", 1, 1)).toEqual([]);
    expect(await bridge.lspDiagnostics("/p", "/p/a.ts")).toEqual([]);
    expect(await bridge.lspStatus("/p")).toEqual({ server: null, language: null, ready: false });
    expect(await bridge.listLspServers()).toEqual([]);
  });
});

describe("mapatges purs LSP → Monaco", () => {
  it("monacoKind: ordres DIFERENTS entre protocols (taula explícita)", () => {
    expect(bridge.monacoKind(2)).toBe(0); // Method
    expect(bridge.monacoKind(3)).toBe(1); // Function
    expect(bridge.monacoKind(6)).toBe(4); // Variable
    expect(bridge.monacoKind(7)).toBe(5); // Class
    expect(bridge.monacoKind(15)).toBe(27); // Snippet
    expect(bridge.monacoKind(25)).toBe(24); // TypeParameter
  });

  it("monacoKind desconegut → Text (18), mai undefined", () => {
    expect(bridge.monacoKind(99)).toBe(18);
    expect(bridge.monacoKind(0)).toBe(18);
  });

  it("markerSeverity: error 8 / warning 4 / info 2 (i desconegut → info)", () => {
    expect(bridge.markerSeverity("error")).toBe(8);
    expect(bridge.markerSeverity("warning")).toBe(4);
    expect(bridge.markerSeverity("info")).toBe(2);
    expect(bridge.markerSeverity("hint")).toBe(2);
  });

  it("toMarker: 1-based, clamp de coordenades invàlides i source intacte", () => {
    const m = bridge.toMarker({
      line: 0,
      col: -3,
      severity: "error",
      message: "TS2322: tipus erroni",
      source: "tsserver",
    });
    expect(m).toEqual({
      startLineNumber: 1,
      startColumn: 1,
      endLineNumber: 1,
      endColumn: 2,
      message: "TS2322: tipus erroni",
      severity: 8,
      source: "tsserver",
    });
  });

  it("toSuggestion: insertText buit → label; range de la paraula intacte", () => {
    const range = { startLineNumber: 2, endLineNumber: 2, startColumn: 1, endColumn: 4 };
    const s = bridge.toSuggestion({ label: "foo", insertText: "", detail: "fn", kind: 3 }, range);
    expect(s.insertText).toBe("foo");
    expect(s.kind).toBe(1);
    expect(s.range).toEqual(range);
  });
});

describe("scheduleLspDiagnostics: debounce i cancel·lació", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  it("sense arrel o fora del projecte → cap consulta", async () => {
    bridge.scheduleLspDiagnostics(null, "/proj/a.ts");
    bridge.scheduleLspDiagnostics("/proj", "/altre/a.ts");
    await vi.advanceTimersByTimeAsync(2000);
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("extensió sense servidor possible (gdscript, txt…) → cap consulta", async () => {
    bridge.scheduleLspDiagnostics("/proj", "/proj/joc.gd");
    bridge.scheduleLspDiagnostics("/proj", "/proj/notes.txt");
    await vi.advanceTimersByTimeAsync(2000);
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("debounce de 900 ms: dues crides seguides = UNA consulta", async () => {
    mocks.invoke.mockResolvedValue([]);
    bridge.scheduleLspDiagnostics("/proj", "/proj/a.ts");
    await vi.advanceTimersByTimeAsync(500);
    bridge.scheduleLspDiagnostics("/proj", "/proj/a.ts");
    await vi.advanceTimersByTimeAsync(899);
    expect(mocks.invoke).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    expect(mocks.invoke).toHaveBeenCalledWith("lsp_diagnostics", {
      projectPath: "/proj",
      path: "/proj/a.ts",
    });
    expect(mocks.applyLspMarkers).toHaveBeenCalledWith("/proj/a.ts", []);
  });

  it("una resposta VELLA no aixafa una de nova (generació)", async () => {
    let resolveOld!: (v: unknown) => void;
    const old = new Promise((res) => {
      resolveOld = res;
    });
    const nous = [{ line: 9, col: 1, severity: "error", message: "nou", source: "tsserver" }];
    mocks.invoke.mockReturnValueOnce(old).mockResolvedValueOnce(nous);
    bridge.scheduleLspDiagnostics("/proj", "/proj/a.ts");
    await vi.advanceTimersByTimeAsync(900); // la consulta vella ja vola
    bridge.scheduleLspDiagnostics("/proj", "/proj/a.ts");
    await vi.advanceTimersByTimeAsync(900); // la nova arriba i s'aplica
    expect(mocks.applyLspMarkers).toHaveBeenCalledWith("/proj/a.ts", nous);
    resolveOld([{ line: 1, col: 1, severity: "error", message: "vell", source: "tsserver" }]);
    await vi.advanceTimersByTimeAsync(0);
    // la resposta vella es DESCARTA: només s'ha aplicat la nova
    expect(mocks.applyLspMarkers).toHaveBeenCalledTimes(1);
  });
});

describe("registerLspProviders: només llenguatges AMB servidor", () => {
  const monFake = () =>
    ({
      languages: {
        registerCompletionItemProvider: vi.fn(),
        registerHoverProvider: vi.fn(),
      },
    }) as unknown as Parameters<Bridge["registerLspProviders"]>[0];

  it("registra rust instal·lat; MAI typescript (workers natius) ni no instal·lats", async () => {
    mocks.invoke.mockResolvedValue([
      { language: "rust", binary: "rust-analyzer", installed: true },
      { language: "typescript", binary: "tsserver", installed: true },
      { language: "php", binary: "intelephense", installed: false },
      { language: "gdscript", binary: "gdscript-lsp", installed: true },
    ]);
    const mon = monFake();
    bridge.registerLspProviders(mon);
    await vi.waitFor(() => {
      expect(mon.languages.registerCompletionItemProvider).toHaveBeenCalledTimes(1);
    });
    expect(mon.languages.registerCompletionItemProvider).toHaveBeenCalledWith(
      "rust",
      expect.objectContaining({ provideCompletionItems: expect.any(Function) })
    );
    expect(mon.languages.registerHoverProvider).toHaveBeenCalledTimes(1);
    expect(mon.languages.registerHoverProvider).toHaveBeenCalledWith(
      "rust",
      expect.objectContaining({ provideHover: expect.any(Function) })
    );
  });

  it("idempotent: remuntar l'editor NO duplica providers; catàleg cau una sola vegada", async () => {
    mocks.invoke.mockResolvedValue([{ language: "rust", binary: "rust-analyzer", installed: true }]);
    const mon = monFake();
    bridge.registerLspProviders(mon);
    bridge.registerLspProviders(mon);
    await bridge.getServerCatalog();
    await bridge.getServerCatalog();
    expect(mon.languages.registerCompletionItemProvider).toHaveBeenCalledTimes(1);
    expect(mocks.invoke).toHaveBeenCalledTimes(1); // catàleg: UNA consulta
  });

  it("sense catàleg (backend degradat) → cap provider, cap error", async () => {
    mocks.invoke.mockRejectedValue(new Error("sense backend"));
    const mon = monFake();
    bridge.registerLspProviders(mon);
    await bridge.getServerCatalog();
    expect(mon.languages.registerCompletionItemProvider).not.toHaveBeenCalled();
    expect(mon.languages.registerHoverProvider).not.toHaveBeenCalled();
  });
});