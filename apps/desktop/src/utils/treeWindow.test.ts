/// Tests del windowing de l'arbre (utils/treeWindow.ts): aplanat DFS només
/// amb carpetes expandides i finestra de files per scrollTop amb overscan.
import { describe, expect, it } from "vitest";
import type { FileNode } from "../stores/workspaceStore";
import { computeWindow, flattenTree } from "./treeWindow";

const f = (path: string): FileNode => ({
  name: path.split("/").pop() ?? path,
  path,
  is_dir: false,
});
const dir = (path: string, children?: FileNode[]): FileNode => ({
  name: path.split("/").pop() ?? path,
  path,
  is_dir: true,
  children,
});

describe("flattenTree", () => {
  const tree = [
    dir("/proj/src", [f("/proj/src/a.ts"), dir("/proj/src/lib", [f("/proj/src/lib/b.ts")])]),
    f("/proj/README.md"),
  ];

  it("carpetes tancades: només les arrels (el subarbre queda amagat)", () => {
    const rows = flattenTree(tree, {});
    expect(rows.map((r) => r.node.name)).toEqual(["src", "README.md"]);
    expect(rows.every((r) => r.depth === 0)).toBe(true);
  });

  it("carpeta expandida: fills en ordre DFS amb la seua profunditat", () => {
    const rows = flattenTree(tree, { "/proj/src": true });
    expect(rows.map((r) => [r.node.name, r.depth])).toEqual([
      ["src", 0],
      ["a.ts", 1],
      ["lib", 1],
      ["README.md", 0],
    ]);
  });

  it("expansió imbricada: només les carpetes REALMENT expandides", () => {
    const rows = flattenTree(tree, { "/proj/src": true, "/proj/src/lib": true });
    expect(rows.map((r) => [r.node.name, r.depth])).toEqual([
      ["src", 0],
      ["a.ts", 1],
      ["lib", 1],
      ["b.ts", 2],
      ["README.md", 0],
    ]);
  });

  it("arbre buit → cap fila", () => {
    expect(flattenTree([], { "/proj/src": true })).toEqual([]);
  });
});

describe("computeWindow", () => {
  const ROW = 24;

  it("a dalt de tot: finestra des de 0 amb overscan inferior", () => {
    // 1000 files, viewport 240 px (10 files), overscan 6.
    const w = computeWindow(0, 240, ROW, 1000);
    expect(w.start).toBe(0);
    expect(w.end).toBe(16); // 10 visibles + 6 overscan
    expect(w.padTop).toBe(0);
    expect(w.totalHeight).toBe(24000);
  });

  it("scroll al mig: finestra centrada amb overscan als dos costats", () => {
    const w = computeWindow(2400, 240, ROW, 1000); // primera visible = 100
    expect(w.start).toBe(94);
    expect(w.end).toBe(116);
    expect(w.padTop).toBe(94 * ROW);
  });

  it("prop del final: la finestra es retalla al total (sense files fantasma)", () => {
    const w = computeWindow(24000, 240, ROW, 1000);
    expect(w.end).toBe(1000);
    expect(w.start).toBeLessThan(1000);
    expect(w.padTop).toBe(w.start * ROW);
  });

  it("scrollTop negatiu o passat es clampen (mai índexs negatius)", () => {
    const a = computeWindow(-50, 240, ROW, 100);
    expect(a.start).toBe(0);
    const b = computeWindow(999999, 240, ROW, 100);
    expect(b.end).toBe(100);
    expect(b.start).toBeGreaterThanOrEqual(0);
  });

  it("arbre buit o alçada 0 → finestra buida i alçada total 0", () => {
    expect(computeWindow(0, 240, ROW, 0)).toEqual({
      start: 0,
      end: 0,
      padTop: 0,
      totalHeight: 0,
    });
    expect(computeWindow(0, 240, 0, 100).totalHeight).toBe(0);
  });

  it("poques files (menys que el viewport): es renderitzen totes", () => {
    const w = computeWindow(0, 240, ROW, 5);
    expect(w.start).toBe(0);
    expect(w.end).toBe(5);
    expect(w.totalHeight).toBe(120);
  });

  it("viewport 0 (panell plegat): almenys una fila + overscan", () => {
    const w = computeWindow(0, 0, ROW, 100);
    expect(w.end).toBeGreaterThan(w.start);
  });
});