/// Tests del wrapper TOLERANT de lint_paths: contracte exacte del backend
/// i degradació honesta (available:false) si el comandament no existeix.
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { lintPaths } from "./lintPaths";

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
});

describe("lintPaths", () => {
  it("sense fitxers no crida el backend i torna available:true", async () => {
    const r = await lintPaths("/proj", []);
    expect(r).toEqual({ available: true, results: [] });
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("crida el contracte EXACTE: lint_paths amb { projectPath, paths }", async () => {
    const results = [
      { path: "index.php", ok: true, output: "No syntax errors detected" },
      { path: "broken.php", ok: false, output: "PHP Parse error: syntax error, unexpected '}'" },
    ];
    mockInvoke.mockResolvedValue(results);
    const r = await lintPaths("/proj/tema", ["index.php", "broken.php"]);
    expect(mockInvoke).toHaveBeenCalledWith("lint_paths", {
      projectPath: "/proj/tema",
      paths: ["index.php", "broken.php"],
    });
    expect(r.available).toBe(true);
    expect(r.results).toEqual(results);
    expect(r.results.filter((x) => !x.ok).map((x) => x.path)).toEqual(["broken.php"]);
  });

  it("si el comandament NO EXISTEIX (reject), degrada sense bloquejar", async () => {
    mockInvoke.mockRejectedValue(new Error("lint_paths not found"));
    const r = await lintPaths("/proj", ["a.php"]);
    expect(r).toEqual({ available: false, results: [] });
  });

  it("una resposta que no és un array també degrada (backend antic)", async () => {
    mockInvoke.mockResolvedValue({ weird: true });
    const r = await lintPaths("/proj", ["a.php"]);
    expect(r).toEqual({ available: false, results: [] });
  });
});