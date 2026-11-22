/// Tests del lint PHP REAL des de l'editor (runPhpLint via schedulePhpLint):
///  - la ruta ABSOLUTA de la pestanya es relativitza abans de cridar el
///    backend (abans: «ruta fora del projecte» → warning fals a la línia 1);
///  - «verificació omesa» (php no instal·lat) NO és un verd: toast únic,
///    sense markers nets i el balanceig heurístic continua actiu.
/// Estat de mòdul (phpLintActive, phpUnavailableNoted) aïllat per test amb
/// vi.resetModules + imports dinàmics.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockSetProblems } = vi.hoisted(() => ({ mockSetProblems: vi.fn() }));

vi.mock("./editorApi", () => ({ getMonaco: vi.fn(), getEditor: vi.fn() }));
vi.mock("../stores/agent/lintPaths", () => ({ lintPaths: vi.fn() }));
vi.mock("../stores/toastStore", () => ({ toastInfo: vi.fn() }));
vi.mock("../i18n", () => ({ t: (k: string) => k }));
vi.mock("../stores/problemsStore", () => ({
  useProblemsStore: { getState: () => ({ setFileProblems: mockSetProblems }) },
}));

// Els mòduls MOQUEJATS conserven la mateixa instància de spy entre
// resetModules (la fàbrica es cacheja): els resetegem a cada test.
import { getEditor, getMonaco } from "./editorApi";
import { lintPaths } from "../stores/agent/lintPaths";
import { toastInfo } from "../stores/toastStore";

/// Monaco fictici amb un sol model (el fitxer .php obert a l'editor).
function fakeMonaco(path: string) {
  const model = { uri: { path, toString: () => `file://${path}` } };
  const setModelMarkers = vi.fn();
  const monaco = {
    MarkerSeverity: { Error: 8, Warning: 4, Info: 2 },
    editor: {
      getModels: () => [model],
      getModelMarkers: () => [],
      setModelMarkers,
      onDidChangeMarkers: vi.fn(),
    },
  };
  return { monaco, model, setModelMarkers };
}

/// Importa el mòdul NET (estat de lint PHP reïnicialitzat) i els seus mocks.
async function fresh() {
  vi.resetModules();
  const diag = await import("./diagnostics");
  const api = await import("./editorApi");
  const lp = await import("../stores/agent/lintPaths");
  const toast = await import("../stores/toastStore");
  return {
    diag,
    getMonaco: vi.mocked(api.getMonaco),
    getEditor: vi.mocked(api.getEditor),
    lintPaths: vi.mocked(lp.lintPaths),
    toastInfo: vi.mocked(toast.toastInfo),
  };
}

beforeEach(() => {
  vi.useFakeTimers();
  mockSetProblems.mockReset();
  vi.mocked(getMonaco).mockReset();
  vi.mocked(getEditor).mockReset();
  vi.mocked(lintPaths).mockReset();
  vi.mocked(toastInfo).mockReset();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("runPhpLint: ruta relativa al backend (finding ALT)", () => {
  it("la ruta ABSOLUTA de la pestanya es relativitza abans de lint_paths", async () => {
    const f = await fresh();
    f.getMonaco.mockReturnValue(null);
    f.lintPaths.mockResolvedValue({
      available: true,
      results: [{ path: "sub/index.php", ok: true, output: "No syntax errors detected" }],
    });
    f.diag.schedulePhpLint("/proj", "/proj/sub/index.php");
    await vi.advanceTimersByTimeAsync(500);
    // El backend rep la ruta RELATIVA: ja no la rebutja com a «fora del projecte».
    expect(f.lintPaths).toHaveBeenCalledWith("/proj", ["sub/index.php"]);
    // Verd real (lint executat): el panell es neteja per a la ruta absoluta.
    expect(mockSetProblems).toHaveBeenCalledWith("/proj/sub/index.php", []);
    expect(f.toastInfo).not.toHaveBeenCalled();
  });

  it("una ruta de fora del projecte es passa tal qual (sense relativitzar)", async () => {
    const f = await fresh();
    f.getMonaco.mockReturnValue(null);
    f.lintPaths.mockResolvedValue({ available: true, results: [] });
    f.diag.schedulePhpLint("/proj", "/altre/x.php");
    await vi.advanceTimersByTimeAsync(500);
    expect(f.lintPaths).toHaveBeenCalledWith("/proj", ["/altre/x.php"]);
  });
});

describe("runPhpLint: «verificació omesa» no és un verd (finding MITJÀ)", () => {
  it("toast informatiu UN sol cop i el balanceig heurístic continua actiu", async () => {
    const f = await fresh();
    const { monaco, model, setModelMarkers } = fakeMonaco("/proj/index.php");
    f.getMonaco.mockReturnValue(monaco as never);
    f.getEditor.mockReturnValue({ getModel: () => model } as never);
    f.lintPaths.mockResolvedValue({
      available: true,
      results: [{ path: "index.php", ok: true, output: "verificació omesa: php no instal·lat" }],
    });
    f.diag.schedulePhpLint("/proj", "/proj/index.php");
    await vi.advanceTimersByTimeAsync(500);
    // Avís honest (una vegada) i CAP marcador «verd» pintat.
    expect(f.toastInfo).toHaveBeenCalledTimes(1);
    expect(f.toastInfo).toHaveBeenCalledWith("editor.phpLintUnavailable");
    expect(setModelMarkers).not.toHaveBeenCalled();
    // El fallback heurístic NO s'ha desactivat: una clau sense tancar es marca.
    f.diag.validateCurrentFile("index.php", "<?php function f() {", "/proj/index.php");
    expect(setModelMarkers).toHaveBeenCalledTimes(1);
    const [, owner, markers] = setModelMarkers.mock.calls[0];
    expect(owner).toBe("noorbit-lint");
    expect(markers.length).toBeGreaterThan(0);
    // Segona passada amb el mateix avís: el toast NO es repeteix.
    f.diag.schedulePhpLint("/proj", "/proj/index.php");
    await vi.advanceTimersByTimeAsync(500);
    expect(f.toastInfo).toHaveBeenCalledTimes(1);
  });

  it("sense editor (fitxer no obert): tampoc no es pinta un fals verd al panell", async () => {
    const f = await fresh();
    f.getMonaco.mockReturnValue(null);
    f.lintPaths.mockResolvedValue({
      available: true,
      results: [{ path: "a.php", ok: true, output: "verificació omesa: php no instal·lat" }],
    });
    f.diag.schedulePhpLint("/proj", "/proj/a.php");
    await vi.advanceTimersByTimeAsync(500);
    expect(f.toastInfo).toHaveBeenCalledTimes(1);
    expect(mockSetProblems).not.toHaveBeenCalled();
  });

  it("un lint REAL ok sí que desactiva el balanceig (comportament existent)", async () => {
    const f = await fresh();
    const { monaco, model, setModelMarkers } = fakeMonaco("/proj/index.php");
    f.getMonaco.mockReturnValue(monaco as never);
    f.getEditor.mockReturnValue({ getModel: () => model } as never);
    f.lintPaths.mockResolvedValue({
      available: true,
      results: [{ path: "index.php", ok: true, output: "No syntax errors detected" }],
    });
    f.diag.schedulePhpLint("/proj", "/proj/index.php");
    await vi.advanceTimersByTimeAsync(500);
    expect(setModelMarkers).toHaveBeenCalledWith(model, "noorbit-lint", []);
    expect(f.toastInfo).not.toHaveBeenCalled();
    // Amb el lint real respost, el balanceig ja no pot aixafar els markers.
    f.diag.validateCurrentFile("index.php", "<?php function f() {", "/proj/index.php");
    expect(setModelMarkers).toHaveBeenCalledTimes(1);
  });
});