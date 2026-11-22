/// Tests de la CUA de continuació automàtica del scaffold (mock d'invoke):
/// lots successius més enllà de MAX_PER_LOT, reintent ÚNIC si un fitxer surt
/// buit, PROHIBICIÓ de placeholders i cancel·lació amb cua restant en ordre.
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { esContingutPlaceholder } from "./fileBlocks";
import { MAX_PER_LOT, generateFileQueue } from "./scaffoldQueue";

const mockInvoke = vi.mocked(invoke);

/// Extreu la ruta «rel» del prompt que la cua envia al backend.
const relOf = (args: unknown): string =>
  ((args as { prompt: string }).prompt.match(/«([^»]+)»/)?.[1] ?? "");

const baseOpts = {
  projectText: "projecte de prova",
  system: "SYSTEM",
  isCancelled: () => false,
};

beforeEach(() => {
  mockInvoke.mockReset();
});

describe("generateFileQueue: continuació automàtica en lots", () => {
  it("processa TOTS els fitxers (> MAX_PER_LOT) en lots successius", async () => {
    const paths = Array.from({ length: 20 }, (_, i) => `src/f${i}.py`);
    mockInvoke.mockImplementation(async (_cmd, args) => `print("codi de ${relOf(args)}")`);
    const lots: [number, number][] = [];
    const done: string[] = [];
    const r = await generateFileQueue({
      ...baseOpts,
      paths,
      onLot: (lot, totalLots) => lots.push([lot, totalLots]),
      onDone: (rel) => done.push(rel),
    });
    expect(r.generated).toHaveLength(20);
    expect(r.failed).toEqual([]);
    expect(r.pending).toEqual([]);
    expect(mockInvoke).toHaveBeenCalledTimes(20);
    expect(lots).toEqual([[1, 3], [2, 3], [3, 3]]);
    expect(done).toEqual(paths);
    expect(MAX_PER_LOT).toBe(8);
  });

  it("envia el contracte exacte d'ai_chat_plain (prompt, system, provider, model)", async () => {
    mockInvoke.mockResolvedValue("codi");
    await generateFileQueue({
      ...baseOpts,
      paths: ["a.py"],
      provider: "ollama",
      model: "qwen3:8b",
    });
    expect(mockInvoke).toHaveBeenCalledWith(
      "ai_chat_plain",
      expect.objectContaining({
        system: "SYSTEM",
        provider: "ollama",
        model: "qwen3:8b",
        prompt: expect.stringContaining("«a.py»"),
      }),
    );
  });
});

describe("generateFileQueue: rebuig de placeholders", () => {
  it("un fitxer buit es REGENERA un cop i, si va bé, es compta com a fet", async () => {
    const calls: string[] = [];
    mockInvoke.mockImplementation(async (_cmd, args) => {
      const rel = relOf(args);
      calls.push(rel);
      if (rel === "b.py" && calls.filter((c) => c === "b.py").length === 1) return "";
      return `codi de ${rel}`;
    });
    const retried: string[] = [];
    const r = await generateFileQueue({
      ...baseOpts,
      paths: ["a.py", "b.py", "c.py"],
      onRetry: (rel) => retried.push(rel),
    });
    expect(retried).toEqual(["b.py"]);
    expect(r.generated.map((f) => f.path)).toEqual(["a.py", "b.py", "c.py"]);
    expect(r.failed).toEqual([]);
    expect(calls.filter((c) => c === "b.py")).toHaveLength(2);
  });

  it("si el reintent també surt buit, el fitxer queda SENSE generar (mai placeholder)", async () => {
    mockInvoke.mockImplementation(async (_cmd, args) =>
      relOf(args) === "b.py" ? "" : `codi de ${relOf(args)}`,
    );
    const skipped: string[] = [];
    const r = await generateFileQueue({
      ...baseOpts,
      paths: ["a.py", "b.py", "c.py"],
      onSkip: (rel) => skipped.push(rel),
    });
    expect(r.generated.map((f) => f.path)).toEqual(["a.py", "c.py"]);
    expect(r.failed).toEqual(["b.py"]);
    expect(skipped).toEqual(["b.py"]);
    for (const f of r.generated) {
      expect(f.content.trim()).not.toBe("");
      expect(esContingutPlaceholder(f.content)).toBe(false);
    }
  });

  it("un placeholder del model es tracta com a buit: reintent i, si persisteix, failed", async () => {
    mockInvoke.mockImplementation(async (_cmd, args) =>
      relOf(args) === "b.py"
        ? "b.py\n\n(Pendent: el model no ha retornat contingut per a aquest fitxer)\n"
        : `codi de ${relOf(args)}`,
    );
    const r = await generateFileQueue({ ...baseOpts, paths: ["a.py", "b.py"] });
    expect(r.failed).toEqual(["b.py"]);
    expect(r.generated.map((f) => f.path)).toEqual(["a.py"]);
  });

  it("si invoke REBUTJA (backend caigut), no es bloqueja: retry i failed", async () => {
    mockInvoke.mockImplementation(async (_cmd, args) => {
      if (relOf(args) === "a.py") throw new Error("backend caigut");
      return "codi";
    });
    const r = await generateFileQueue({ ...baseOpts, paths: ["a.py", "b.py"] });
    expect(r.failed).toEqual(["a.py"]);
    expect(r.generated.map((f) => f.path)).toEqual(["b.py"]);
  });
});

describe("generateFileQueue: cancel·lació", () => {
  it("en tallar a mitja cua, la resta queda a pending EN ORDRE original", async () => {
    mockInvoke.mockImplementation(async (_cmd, args) => `codi de ${relOf(args)}`);
    const paths = Array.from({ length: 10 }, (_, i) => `f${i}.py`);
    let fets = 0;
    const r = await generateFileQueue({
      ...baseOpts,
      paths,
      lotSize: 4,
      isCancelled: () => fets >= 3,
      onDone: () => fets++,
    });
    expect(r.generated.map((f) => f.path)).toEqual(paths.slice(0, 3));
    expect(r.pending).toEqual(paths.slice(3));
    expect(r.failed).toEqual([]);
  });

  it("si ja està cancel·lada abans de començar, no crida el model", async () => {
    const r = await generateFileQueue({
      ...baseOpts,
      paths: ["a.py", "b.py"],
      isCancelled: () => true,
    });
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(r.generated).toEqual([]);
    expect(r.pending).toEqual(["a.py", "b.py"]);
  });
});