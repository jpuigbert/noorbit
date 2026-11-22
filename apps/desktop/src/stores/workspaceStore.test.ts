/// Tests de la lògica MULTI-PESTANYA i SPLIT del workspaceStore amb invoke
/// moquejat: obrir/activar/tancar, derivació del `openFile` de compatibilitat,
/// preservació del contingut brut, divisió vertical i reanomenar/esborrar.
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("../editor/diagnostics", () => ({
  lintOffline: vi.fn(),
  schedulePhpLint: vi.fn(),
}));
vi.mock("./editReviewStore", () => ({ dropMarkOnManualSave: vi.fn() }));
vi.mock("./toastStore", () => ({ toastError: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { schedulePhpLint } from "../editor/diagnostics";
import { useWorkspaceStore } from "./workspaceStore";

const mockInvoke = vi.mocked(invoke);
const mockPhpLint = vi.mocked(schedulePhpLint);

/// Continguts del «disc» fictici per a read_file.
const disk: Record<string, string> = {
  "/proj/a.txt": "contingut A",
  "/proj/b.txt": "contingut B",
  "/proj/c.txt": "contingut C",
  "/proj/index.php": "<?php echo 1;",
};

beforeEach(() => {
  mockInvoke.mockReset();
  mockPhpLint.mockReset();
  mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "read_file") return disk[String((args as { path?: string })?.path)] ?? "";
    if (cmd === "list_dir") return [];
    return undefined;
  });
  useWorkspaceStore.setState({
    root: "/proj",
    tree: [],
    expanded: {},
    openFiles: [],
    activePath: null,
    splitPath: null,
    focusedGroup: "main",
    openFile: null,
  });
});

const st = () => useWorkspaceStore.getState();

describe("pestanyes: obrir i activar", () => {
  it("loadFile obri una pestanya, l'activa i deriva openFile (compatibilitat)", async () => {
    await st().loadFile("/proj/a.txt");
    expect(st().openFiles).toHaveLength(1);
    expect(st().activePath).toBe("/proj/a.txt");
    expect(st().openFile).toEqual({
      path: "/proj/a.txt",
      name: "a.txt",
      content: "contingut A",
      dirty: false,
    });
  });

  it("obrir un segon fitxer afegeix pestanya i canvia l'activa", async () => {
    await st().loadFile("/proj/a.txt");
    await st().loadFile("/proj/b.txt");
    expect(st().openFiles.map((f) => f.path)).toEqual(["/proj/a.txt", "/proj/b.txt"]);
    expect(st().activePath).toBe("/proj/b.txt");
    expect(st().openFile?.name).toBe("b.txt");
  });

  it("pestanya ja oberta SENSE force: només activa, no rellegeix (dirty preservat)", async () => {
    await st().loadFile("/proj/a.txt");
    await st().loadFile("/proj/b.txt");
    st().updateContent("editat sense desar", "/proj/a.txt");
    mockInvoke.mockClear();
    await st().loadFile("/proj/a.txt");
    expect(mockInvoke).not.toHaveBeenCalledWith("read_file", { path: "/proj/a.txt" });
    expect(st().activePath).toBe("/proj/a.txt");
    expect(st().openFile?.content).toBe("editat sense desar");
    expect(st().openFile?.dirty).toBe(true);
  });

  it("force + background: rellegeix del disc sense canviar la pestanya activa", async () => {
    await st().loadFile("/proj/a.txt");
    await st().loadFile("/proj/b.txt");
    st().updateContent("brut", "/proj/a.txt");
    disk["/proj/a.txt"] = "restaurat";
    await st().loadFile("/proj/a.txt", { force: true, background: true });
    expect(st().activePath).toBe("/proj/b.txt");
    const a = st().openFiles.find((f) => f.path === "/proj/a.txt");
    expect(a?.content).toBe("restaurat");
    expect(a?.dirty).toBe(false);
    disk["/proj/a.txt"] = "contingut A";
  });

  it("setActive salta a una pestanya existent; una d'inexistent no fa res", async () => {
    await st().loadFile("/proj/a.txt");
    await st().loadFile("/proj/b.txt");
    st().setActive("/proj/a.txt");
    expect(st().openFile?.name).toBe("a.txt");
    st().setActive("/no/existeix.txt");
    expect(st().activePath).toBe("/proj/a.txt");
  });
});

describe("pestanyes: tancar", () => {
  it("closeFile activa la veïna de la DRETA (com VS Code)", async () => {
    await st().loadFile("/proj/a.txt");
    await st().loadFile("/proj/b.txt");
    await st().loadFile("/proj/c.txt");
    st().setActive("/proj/b.txt");
    st().closeFile("/proj/b.txt");
    expect(st().openFiles.map((f) => f.path)).toEqual(["/proj/a.txt", "/proj/c.txt"]);
    expect(st().activePath).toBe("/proj/c.txt");
  });

  it("sense veïna a la dreta: activa la de l'esquerra; l'última → editor buit", async () => {
    await st().loadFile("/proj/a.txt");
    await st().loadFile("/proj/b.txt");
    st().closeFile(); // tanca l'activa (b)
    expect(st().activePath).toBe("/proj/a.txt");
    st().closeFile();
    expect(st().openFiles).toHaveLength(0);
    expect(st().activePath).toBeNull();
    expect(st().openFile).toBeNull();
  });

  it("tancar una pestanya NO activa no canvia l'activa", async () => {
    await st().loadFile("/proj/a.txt");
    await st().loadFile("/proj/b.txt");
    st().closeFile("/proj/a.txt");
    expect(st().activePath).toBe("/proj/b.txt");
  });

  it("closeOthers deixa només la indicada; closeAll ho buida tot", async () => {
    await st().loadFile("/proj/a.txt");
    await st().loadFile("/proj/b.txt");
    await st().loadFile("/proj/c.txt");
    st().closeOthers("/proj/b.txt");
    expect(st().openFiles.map((f) => f.path)).toEqual(["/proj/b.txt"]);
    expect(st().activePath).toBe("/proj/b.txt");
    st().closeAll();
    expect(st().openFiles).toHaveLength(0);
    expect(st().splitPath).toBeNull();
    expect(st().focusedGroup).toBe("main");
  });
});

describe("split (divisió vertical)", () => {
  it("toggleSplit obri el grup dividit amb el fitxer actiu i el focus hi va", async () => {
    await st().loadFile("/proj/a.txt");
    st().toggleSplit();
    expect(st().splitPath).toBe("/proj/a.txt");
    expect(st().focusedGroup).toBe("split");
    st().toggleSplit();
    expect(st().splitPath).toBeNull();
    expect(st().focusedGroup).toBe("main");
  });

  it("openSide obri un fitxer NO obert en background i el posa al split", async () => {
    await st().loadFile("/proj/a.txt");
    await st().openSide("/proj/b.txt");
    expect(st().splitPath).toBe("/proj/b.txt");
    expect(st().focusedGroup).toBe("split");
    // L'activa del grup principal no ha canviat.
    expect(st().activePath).toBe("/proj/a.txt");
    expect(st().openFiles.map((f) => f.path)).toEqual(["/proj/a.txt", "/proj/b.txt"]);
  });

  it("tancar la pestanya que mostra el split: el split passa a l'activa", async () => {
    await st().loadFile("/proj/a.txt");
    await st().loadFile("/proj/b.txt");
    await st().openSide("/proj/b.txt");
    st().closeFile("/proj/b.txt");
    expect(st().splitPath).toBe("/proj/a.txt");
    expect(st().focusedGroup).toBe("split");
  });

  it("closeAll saneja el focus: mai queda apuntant a un split inexistent", async () => {
    await st().loadFile("/proj/a.txt");
    st().toggleSplit();
    st().closeAll();
    expect(st().focusedGroup).toBe("main");
    expect(st().splitPath).toBeNull();
  });
});

describe("compatibilitat setOpenFile (revisió IA)", () => {
  it("pestanya existent: reemplaça EN LLOC sense activar-la", async () => {
    await st().loadFile("/proj/a.txt");
    await st().loadFile("/proj/b.txt");
    st().setOpenFile({ path: "/proj/a.txt", name: "a.txt", content: "definitiu", dirty: false });
    expect(st().activePath).toBe("/proj/b.txt");
    expect(st().openFiles.find((f) => f.path === "/proj/a.txt")?.content).toBe("definitiu");
  });

  it("fitxer nou: s'obri i s'activa (semàntica antiga)", () => {
    st().setOpenFile({ path: "/proj/nou.txt", name: "nou.txt", content: "x", dirty: false });
    expect(st().activePath).toBe("/proj/nou.txt");
    expect(st().openFile?.content).toBe("x");
  });

  it("null: tanca la pestanya activa", async () => {
    await st().loadFile("/proj/a.txt");
    st().setOpenFile(null);
    expect(st().openFiles).toHaveLength(0);
  });
});

describe("contingut i desat", () => {
  it("updateContent amb path marca NOMÉS aquella pestanya com a bruta", async () => {
    await st().loadFile("/proj/a.txt");
    await st().loadFile("/proj/b.txt");
    st().updateContent("canvi", "/proj/a.txt");
    expect(st().openFiles.find((f) => f.path === "/proj/a.txt")?.dirty).toBe(true);
    expect(st().openFiles.find((f) => f.path === "/proj/b.txt")?.dirty).toBe(false);
    // openFile derivat (b) continua net.
    expect(st().openFile?.dirty).toBe(false);
  });

  it("saveFile escriu al disc i neteja el dirty de la pestanya", async () => {
    await st().loadFile("/proj/a.txt");
    st().updateContent("final", "/proj/a.txt");
    await st().saveFile("/proj/a.txt", "final");
    expect(mockInvoke).toHaveBeenCalledWith("write_file", { path: "/proj/a.txt", content: "final" });
    expect(st().openFile?.dirty).toBe(false);
    expect(st().openFile?.content).toBe("final");
  });

  it("newFile crea sense-títols incrementals i bruts", () => {
    st().newFile();
    st().newFile();
    expect(st().openFiles.map((f) => f.path)).toEqual(["untitled:1", "untitled:2"]);
    expect(st().openFile?.dirty).toBe(true);
    st().closeFile("untitled:1");
    st().newFile();
    expect(st().openFiles.map((f) => f.path)).toEqual(["untitled:2", "untitled:3"]);
  });
});

describe("arbre: reanomenar i esborrar afecten les pestanyes", () => {
  it("renameNode remapeja la pestanya, l'activa i el split", async () => {
    await st().loadFile("/proj/a.txt");
    st().toggleSplit();
    await st().renameNode("/proj/a.txt", "renombrat.txt");
    expect(mockInvoke).toHaveBeenCalledWith("rename_path", {
      from: "/proj/a.txt",
      to: "/proj/renombrat.txt",
    });
    expect(st().openFiles[0].path).toBe("/proj/renombrat.txt");
    expect(st().openFiles[0].name).toBe("renombrat.txt");
    expect(st().activePath).toBe("/proj/renombrat.txt");
    expect(st().splitPath).toBe("/proj/renombrat.txt");
  });

  it("reanomenar una CARPETA remapeja totes les pestanyes de dins", async () => {
    disk["/proj/src/x.txt"] = "x";
    disk["/proj/src/y.txt"] = "y";
    await st().loadFile("/proj/src/x.txt");
    await st().loadFile("/proj/src/y.txt");
    await st().renameNode("/proj/src", "lib");
    expect(st().openFiles.map((f) => f.path)).toEqual(["/proj/lib/x.txt", "/proj/lib/y.txt"]);
    expect(st().activePath).toBe("/proj/lib/y.txt");
    delete disk["/proj/src/x.txt"];
    delete disk["/proj/src/y.txt"];
  });

  it("renameNode remapeja també imagePath (visor d'imatges)", async () => {
    await st().loadFile("/proj/foto.png");
    expect(st().openFiles[0].imagePath).toBe("/proj/foto.png");
    await st().renameNode("/proj/foto.png", "nova.png");
    expect(st().openFiles[0].path).toBe("/proj/nova.png");
    expect(st().openFiles[0].name).toBe("nova.png");
    expect(st().openFiles[0].imagePath).toBe("/proj/nova.png");
  });

  it("renameNode remapeja també videoPath (visor de vídeos)", async () => {
    await st().loadFile("/proj/clip.mp4");
    expect(st().openFiles[0].videoPath).toBe("/proj/clip.mp4");
    await st().renameNode("/proj/clip.mp4", "trailer.mp4");
    expect(st().openFiles[0].videoPath).toBe("/proj/trailer.mp4");
  });

  it("reanomenar una CARPETA remapeja imagePath/videoPath de les pestanyes de dins", async () => {
    await st().loadFile("/proj/assets/foto.png");
    await st().loadFile("/proj/assets/clip.mp4");
    await st().renameNode("/proj/assets", "media");
    expect(st().openFiles[0].imagePath).toBe("/proj/media/foto.png");
    expect(st().openFiles[1].videoPath).toBe("/proj/media/clip.mp4");
  });

  it("removeNode tanca les pestanyes dins del node esborrat", async () => {
    disk["/proj/src/x.txt"] = "x";
    await st().loadFile("/proj/a.txt");
    await st().loadFile("/proj/src/x.txt");
    await st().removeNode("/proj/src");
    expect(st().openFiles.map((f) => f.path)).toEqual(["/proj/a.txt"]);
    expect(st().activePath).toBe("/proj/a.txt");
    delete disk["/proj/src/x.txt"];
  });
});

describe("lint PHP programat", () => {
  it("obrir un .php del projecte programa el lint real (php -l)", async () => {
    await st().loadFile("/proj/index.php");
    expect(mockPhpLint).toHaveBeenCalledWith("/proj", "/proj/index.php");
  });

  it("un .txt no programa cap lint PHP", async () => {
    await st().loadFile("/proj/a.txt");
    expect(mockPhpLint).not.toHaveBeenCalled();
  });

  it("desar un .php també reprograma el lint", async () => {
    await st().loadFile("/proj/index.php");
    mockPhpLint.mockClear();
    await st().saveFile("/proj/index.php", "<?php echo 2;");
    expect(mockPhpLint).toHaveBeenCalledWith("/proj", "/proj/index.php");
  });
});

describe("loadFile: resultat de la lectura (pestanyes fantasma de l'undo)", () => {
  it("lectura correcta → true; lectura fallida (fitxer esborrat) → false", async () => {
    expect(await st().loadFile("/proj/a.txt")).toBe(true);
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "read_file") throw new Error("No such file or directory");
      if (cmd === "list_dir") return [];
      return undefined;
    });
    expect(await st().loadFile("/proj/esborrat.txt", { force: true })).toBe(false);
  });
});

describe("últim projecte actiu (noorbit.lastRoot): persistència i restauració", () => {
  beforeEach(() => {
    localStorage.removeItem("noorbit.lastRoot");
    // open_workspace resol la ruta (el backend real retorna l'absoluta).
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "open_workspace") return String((args as { path?: string })?.path);
      if (cmd === "list_dir") return [];
      return undefined;
    });
  });

  it("openWorkspace amb èxit desa el root resolt a localStorage", async () => {
    await st().openWorkspace("/proj");
    expect(localStorage.getItem("noorbit.lastRoot")).toBe("/proj");
    expect(st().root).toBe("/proj");
  });

  it("openWorkspace fallit NO desa cap lastRoot", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "open_workspace") throw new Error("El directori no existeix");
      return undefined;
    });
    await expect(st().openWorkspace("/fantasma")).rejects.toThrow();
    expect(localStorage.getItem("noorbit.lastRoot")).toBeNull();
  });

  it("restoreLastRoot reobri l'últim projecte (restauració a l'arrencada)", async () => {
    useWorkspaceStore.setState({ root: null });
    localStorage.setItem("noorbit.lastRoot", "/altre-projecte");
    expect(await st().restoreLastRoot()).toBe(true);
    expect(mockInvoke).toHaveBeenCalledWith("open_workspace", { path: "/altre-projecte" });
    expect(st().root).toBe("/altre-projecte");
  });

  it("sense lastRoot desat: restoreLastRoot → false i no obri res", async () => {
    useWorkspaceStore.setState({ root: null });
    expect(await st().restoreLastRoot()).toBe(false);
    expect(mockInvoke).not.toHaveBeenCalledWith("open_workspace", expect.anything());
    expect(st().root).toBeNull();
  });

  it("fallback: si el root ja no existeix → false, clau esborrada i root null", async () => {
    useWorkspaceStore.setState({ root: null });
    localStorage.setItem("noorbit.lastRoot", "/esborrat");
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "open_workspace") throw new Error("El directori no existeix");
      if (cmd === "list_dir") return [];
      return undefined;
    });
    expect(await st().restoreLastRoot()).toBe(false);
    expect(st().root).toBeNull();
    expect(localStorage.getItem("noorbit.lastRoot")).toBeNull();
  });

  it("root inexistent amb rebuig de STRING pla (contracte real d'invoke) → clau esborrada", async () => {
    useWorkspaceStore.setState({ root: null });
    localStorage.setItem("noorbit.lastRoot", "/esborrat");
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "open_workspace") throw "La carpeta /esborrat no existeix";
      if (cmd === "list_dir") return [];
      return undefined;
    });
    expect(await st().restoreLastRoot()).toBe(false);
    expect(localStorage.getItem("noorbit.lastRoot")).toBeNull();
  });

  it("fallada TRANSITÒRIA (permís TCC pendent) → false però clau CONSERVADA", async () => {
    useWorkspaceStore.setState({ root: null });
    localStorage.setItem("noorbit.lastRoot", "/Users/x/Desktop/projecte");
    mockInvoke.mockImplementation(async (cmd: string) => {
      // Missatge real del backend quan metadata falla amb EPERM (TCC).
      if (cmd === "open_workspace") {
        throw "No s'ha pogut accedir a la carpeta /Users/x/Desktop/projecte: Operation not permitted (os error 1)";
      }
      if (cmd === "list_dir") return [];
      return undefined;
    });
    expect(await st().restoreLastRoot()).toBe(false);
    expect(st().root).toBeNull();
    expect(localStorage.getItem("noorbit.lastRoot")).toBe("/Users/x/Desktop/projecte");
  });

  it("error GENÈRIC/desconegut → false però clau CONSERVADA", async () => {
    useWorkspaceStore.setState({ root: null });
    localStorage.setItem("noorbit.lastRoot", "/proj");
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "open_workspace") throw new Error("backend ocupat");
      if (cmd === "list_dir") return [];
      return undefined;
    });
    expect(await st().restoreLastRoot()).toBe(false);
    expect(localStorage.getItem("noorbit.lastRoot")).toBe("/proj");
  });

  it("després d'una fallada transitòria, la pròxima arrencada restaura el projecte", async () => {
    useWorkspaceStore.setState({ root: null });
    localStorage.setItem("noorbit.lastRoot", "/proj");
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "open_workspace") throw new Error("Operation not permitted (os error 1)");
      if (cmd === "list_dir") return [];
      return undefined;
    });
    expect(await st().restoreLastRoot()).toBe(false);
    // Segona arrencada: el TCC ja està concedit i el mateix root es reobri.
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "open_workspace") return String((args as { path?: string })?.path);
      if (cmd === "list_dir") return [];
      return undefined;
    });
    expect(await st().restoreLastRoot()).toBe(true);
    expect(st().root).toBe("/proj");
    expect(localStorage.getItem("noorbit.lastRoot")).toBe("/proj");
  });
});