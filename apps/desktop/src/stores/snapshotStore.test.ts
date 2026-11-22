/// Tests del flux «Desfés el pas» (snapshotStore) amb invoke MOQUEJAT:
/// contracte exacte dels comandaments snapshot_count/snapshot_undo,
/// refresc de l'arbre i del fitxer obert, descart de les marques de revisió
/// verd/roig, toasts en català i degradació honesta si el backend falla.
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../i18n", () => ({
  t: (k: string, vars?: Record<string, string | number>) =>
    vars?.n !== undefined ? `${k}:${vars.n}` : vars?.name !== undefined ? `${k}:${vars.name}` : k,
}));
vi.mock("./toastStore", () => ({
  toastError: vi.fn(),
  toastInfo: vi.fn(),
  toastSuccess: vi.fn(),
}));
vi.mock("./editReviewStore", () => ({ dropMark: vi.fn() }));
vi.mock("./workspaceStore", () => ({
  useWorkspaceStore: { getState: vi.fn() },
}));

import { invoke } from "@tauri-apps/api/core";
import { useSnapshotStore } from "./snapshotStore";
import { useWorkspaceStore } from "./workspaceStore";
import { dropMark } from "./editReviewStore";
import { toastError, toastInfo, toastSuccess } from "./toastStore";

const mockInvoke = vi.mocked(invoke);
const mockWs = vi.mocked(useWorkspaceStore.getState);
const mockDropMark = vi.mocked(dropMark);
const mockToastError = vi.mocked(toastError);
const mockToastInfo = vi.mocked(toastInfo);
const mockToastSuccess = vi.mocked(toastSuccess);

const refreshTree = vi.fn(async () => undefined);
const loadFile = vi.fn(async (_p: string, _o?: { force?: boolean; background?: boolean }) => true);
const closeFile = vi.fn((_p?: string) => undefined);

beforeEach(() => {
  mockInvoke.mockReset();
  mockDropMark.mockReset();
  mockToastError.mockReset();
  mockToastInfo.mockReset();
  mockToastSuccess.mockReset();
  refreshTree.mockClear();
  loadFile.mockClear();
  loadFile.mockResolvedValue(true);
  closeFile.mockClear();
  mockWs.mockReturnValue({
    root: "/proj",
    openFiles: [{ path: "/proj/a.txt", name: "a.txt", content: "nou", dirty: false }],
    openFile: { path: "/proj/a.txt", name: "a.txt", content: "nou", dirty: false },
    refreshTree,
    loadFile,
    closeFile,
  } as any);
  useSnapshotStore.setState({ count: 0, busy: false });
});

describe("snapshotStore.refresh", () => {
  it("crida el contracte EXACTE: snapshot_count amb { projectPath }", async () => {
    mockInvoke.mockResolvedValue(3);
    await useSnapshotStore.getState().refresh("/proj");
    expect(mockInvoke).toHaveBeenCalledWith("snapshot_count", { projectPath: "/proj" });
    expect(useSnapshotStore.getState().count).toBe(3);
  });

  it("sense projecte obert no crida el backend i posa el comptador a 0", async () => {
    useSnapshotStore.setState({ count: 5 });
    await useSnapshotStore.getState().refresh(null);
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(useSnapshotStore.getState().count).toBe(0);
  });

  it("si el comandament no existeix (backend antic), degrada a 0 sense petar", async () => {
    mockInvoke.mockRejectedValue(new Error("snapshot_count not found"));
    await useSnapshotStore.getState().refresh("/proj");
    expect(useSnapshotStore.getState().count).toBe(0);
  });
});

describe("snapshotStore.undoStep (flux del botó)", () => {
  it("desfés, refresca arbre + fitxer obert, descarta marques i fa toast", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "snapshot_undo") return ["a.txt", "nou.txt"];
      if (cmd === "snapshot_count") return 0;
      throw new Error("comandament inesperat: " + cmd);
    });
    const paths = await useSnapshotStore.getState().undoStep("/proj");
    expect(mockInvoke).toHaveBeenCalledWith("snapshot_undo", { projectPath: "/proj" });
    expect(paths).toEqual(["a.txt", "nou.txt"]);
    // Arbre i pestanyes afectades/netes refrescades del disc (force)
    // sense canviar la pestanya activa (background).
    expect(refreshTree).toHaveBeenCalled();
    expect(loadFile).toHaveBeenCalledWith("/proj/a.txt", { force: true, background: true });
    // Les revisions pendents dels fitxers restaurats es descarten.
    expect(mockDropMark).toHaveBeenCalledWith("/proj/a.txt");
    expect(mockDropMark).toHaveBeenCalledWith("/proj/nou.txt");
    // Toast en català amb el nombre de fitxers restaurats.
    expect(mockToastSuccess).toHaveBeenCalledWith("agent.undoStepDone:2");
    expect(useSnapshotStore.getState().count).toBe(0);
    expect(useSnapshotStore.getState().busy).toBe(false);
  });

  it("un sol fitxer → toast en singular", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "snapshot_undo") return ["a.txt"];
      return 0;
    });
    await useSnapshotStore.getState().undoStep("/proj");
    expect(mockToastSuccess).toHaveBeenCalledWith("agent.undoStepDoneOne");
  });

  it("pila buida (Err del backend) → null + toast d'error, sense refresc de l'editor", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "snapshot_undo") throw new Error("No hi ha cap pas d'agent per desfer");
      return 0;
    });
    const r = await useSnapshotStore.getState().undoStep("/proj");
    expect(r).toBeNull();
    expect(mockToastError).toHaveBeenCalledWith(
      "agent.undoStepError",
      expect.any(Error)
    );
    expect(refreshTree).not.toHaveBeenCalled();
    expect(useSnapshotStore.getState().busy).toBe(false);
  });

  it("dos clics seguits NO pops dos passos (guarda busy)", async () => {
    let resolveUndo: (v: string[]) => void = () => undefined;
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "snapshot_undo")
        return new Promise<string[]>((res) => {
          resolveUndo = res;
        });
      return Promise.resolve(0);
    });
    const first = useSnapshotStore.getState().undoStep("/proj");
    const second = await useSnapshotStore.getState().undoStep("/proj");
    expect(second).toBeNull();
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    resolveUndo(["a.txt"]);
    await first;
    expect(useSnapshotStore.getState().busy).toBe(false);
  });

  it("fitxer obert FORA del projecte (o untitled) no es recarrega", async () => {
    mockWs.mockReturnValue({
      root: "/proj",
      openFiles: [{ path: "untitled:1", name: "sense-títol-1", content: "", dirty: true }],
      openFile: { path: "untitled:1", name: "sense-títol-1", content: "", dirty: true },
      refreshTree,
      loadFile,
      closeFile,
    } as any);
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "snapshot_undo") return ["a.txt"];
      return 0;
    });
    await useSnapshotStore.getState().undoStep("/proj");
    expect(loadFile).not.toHaveBeenCalled();
  });

  it("multi-pestanya: recarrega les afectades i les netes; les dirty alienes NO", async () => {
    mockWs.mockReturnValue({
      root: "/proj",
      openFiles: [
        { path: "/proj/a.txt", name: "a.txt", content: "a", dirty: false },
        { path: "/altre/b.txt", name: "b.txt", content: "b", dirty: false },
        { path: "/proj/sub/c.txt", name: "c.txt", content: "edicions sense desar", dirty: true },
      ],
      openFile: { path: "/proj/a.txt", name: "a.txt", content: "a", dirty: false },
      refreshTree,
      loadFile,
      closeFile,
    } as any);
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "snapshot_undo") return ["a.txt"];
      return 0;
    });
    await useSnapshotStore.getState().undoStep("/proj");
    // a.txt (afectada, neta) sí; c.txt (dirty i NO tocada pel pas) es conserva
    // tal qual: el force no pot descartar edicions sense desar d'aliens.
    expect(loadFile).toHaveBeenCalledTimes(1);
    expect(loadFile).toHaveBeenCalledWith("/proj/a.txt", { force: true, background: true });
    expect(closeFile).not.toHaveBeenCalled();
  });

  it("una pestanya DIRTY tocada pel pas sí que es recarrega (el pas mana)", async () => {
    mockWs.mockReturnValue({
      root: "/proj",
      openFiles: [
        { path: "/proj/sub/c.txt", name: "c.txt", content: "brut", dirty: true },
      ],
      openFile: { path: "/proj/sub/c.txt", name: "c.txt", content: "brut", dirty: true },
      refreshTree,
      loadFile,
      closeFile,
    } as any);
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "snapshot_undo") return ["sub/c.txt"];
      return 0;
    });
    await useSnapshotStore.getState().undoStep("/proj");
    expect(loadFile).toHaveBeenCalledWith("/proj/sub/c.txt", { force: true, background: true });
  });

  it("fitxer ESBORRAT per l'undo: recàrrega fallida → pestanya tancada + toast", async () => {
    loadFile.mockResolvedValue(false);
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "snapshot_undo") return ["a.txt"];
      return 0;
    });
    await useSnapshotStore.getState().undoStep("/proj");
    // Sense pestanya fantasma que ⌘S ressuscitaria al disc.
    expect(closeFile).toHaveBeenCalledWith("/proj/a.txt");
    expect(mockToastInfo).toHaveBeenCalledWith("agent.undoStepClosed:a.txt");
  });

  it("recàrrega fallida d'un fitxer NO afectat: la pestanya NO es tanca", async () => {
    mockWs.mockReturnValue({
      root: "/proj",
      openFiles: [
        { path: "/proj/a.txt", name: "a.txt", content: "a", dirty: false },
        { path: "/proj/b.txt", name: "b.txt", content: "b", dirty: false },
      ],
      openFile: { path: "/proj/a.txt", name: "a.txt", content: "a", dirty: false },
      refreshTree,
      loadFile,
      closeFile,
    } as any);
    loadFile.mockImplementation(async (p: string) => p !== "/proj/b.txt");
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "snapshot_undo") return ["a.txt"];
      return 0;
    });
    await useSnapshotStore.getState().undoStep("/proj");
    expect(closeFile).not.toHaveBeenCalled();
    expect(mockToastInfo).not.toHaveBeenCalled();
  });
});