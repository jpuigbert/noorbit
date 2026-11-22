/// Tests de regressió del nucli xat→codi a agentStore (Fase 1e):
/// - send: routing bàsic (@video, @computer, blender, flux normal) amb invoke
///   MOQUEJAT — contracte exacte dels comandaments del backend.
/// - materialize/autoMaterialize: escriptura de blocs @file:, avisos de blocs
///   sense ruta, duplicats, bloc_N → codi_ia/, esborrats i edicions.
/// - applyEditBlocks: delegació REAL a applyEditsToContent + informe de
///   fragment semblant numerat, no-ops rebutjats i fitxers inexistents.
/// La lògica pura dels parsers (onlyClosed, inferPath, applyEditsToContent,
/// fragmentMesSemblant) JA està coberta a agent/fileBlocks.test.ts: ací NO es
/// duplica — es prova l'ORQUESTACIÓ del store (invoke, marques, informes).
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => vi.fn()) }));
vi.mock("../i18n", () => ({
  t: (k: string, vars?: Record<string, string | number>) =>
    vars ? `${k}:${JSON.stringify(vars)}` : k,
}));
// El motor de diagnòstics importa Monaco: als tests del store el simulem.
vi.mock("../editor/diagnostics", () => ({ lintOffline: vi.fn(() => 0) }));
vi.mock("./editReviewStore", () => ({
  armAllMarks: vi.fn(),
  captureOriginal: vi.fn(async () => ({ original: "", existed: false })),
  dropMark: vi.fn(),
  noteFileEdited: vi.fn(),
  noteLiveWriteContent: vi.fn(),
  noteLiveWriteStart: vi.fn(),
}));
vi.mock("./workspaceStore", () => ({
  useWorkspaceStore: {
    getState: vi.fn(() => ({ root: "/proj", refreshTree: async () => undefined })),
    subscribe: vi.fn(),
  },
}));
vi.mock("./aiStore", () => ({
  useAIStore: { getState: () => ({ selectedModel: null }) },
}));
vi.mock("./expertStore", () => {
  const state = {
    experts: [] as { id: string; name: string; role: string; mode?: string }[],
    load: vi.fn(async () => undefined),
    runSolo: vi.fn(async () => null),
    error: null as string | null,
  };
  return { useExpertStore: { getState: () => state } };
});
vi.mock("./taskListStore", () => {
  const state = {
    asContext: () => "",
    items: [] as unknown[],
    add: vi.fn(),
    complete: vi.fn(),
    markDoing: vi.fn(),
  };
  return { useTaskListStore: { getState: () => state } };
});

import { invoke } from "@tauri-apps/api/core";
import { useAgentStore, applyEditBlocks, autoMaterialize } from "./agentStore";
import { useWorkspaceStore } from "./workspaceStore";
import { lintOffline } from "../editor/diagnostics";
import { armAllMarks, captureOriginal, dropMark, noteFileEdited } from "./editReviewStore";

const mockInvoke = vi.mocked(invoke);
const mockWs = vi.mocked(useWorkspaceStore.getState);
const mockLint = vi.mocked(lintOffline);
const mockCapture = vi.mocked(captureOriginal);
const mockNote = vi.mocked(noteFileEdited);
const mockDrop = vi.mocked(dropMark);
const mockArm = vi.mocked(armAllMarks);

const refreshTree = vi.fn(async () => undefined);

/// Resposta amb un bloc @file: tancat (el format que el backend promet).
const bloc = (path: string, lang: string, cos: string) =>
  `@file: ${path}\n\`\`\`${lang}\n${cos}\n\`\`\``;

beforeEach(() => {
  mockInvoke.mockReset();
  // Contracte per defecte: skills buides, sense thinking, escriptures OK.
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === "list_skills") return [];
    if (cmd === "ai_last_thinking") return null;
    if (cmd === "write_file" || cmd === "create_dir" || cmd === "delete_path") return undefined;
    throw new Error("comandament inesperat: " + cmd);
  });
  mockLint.mockReset();
  mockLint.mockReturnValue(0);
  mockCapture.mockClear();
  mockNote.mockClear();
  mockDrop.mockClear();
  mockArm.mockClear();
  refreshTree.mockClear();
  mockWs.mockReturnValue({ root: "/proj", refreshTree } as any);
  localStorage.clear();
  // Xat «main» net per a cada test (mateixa forma que blankSession).
  const base = useAgentStore.getState().sessions[0];
  useAgentStore.setState({
    sessions: [
      {
        ...base,
        id: "main",
        title: "",
        messages: [],
        queue: [],
        running: false,
        stopped: false,
        startedAt: null,
        error: null,
        lastResult: null,
        lastThinking: null,
        stream: null,
        provider: null,
        model: null,
        expertId: "",
        includeContext: false,
      },
    ],
    activeId: "main",
    usageBySession: {},
  });
});

const sess = () => useAgentStore.getState().sessions[0];
const crides = (cmd: string) => mockInvoke.mock.calls.filter((c) => c[0] === cmd);

// ── applyEditBlocks: delegació a applyEditsToContent + informe honest ──────
describe("applyEditBlocks", () => {
  it("aplica CERCA/NOU al fitxer REAL: read_file → write_file + marques de revisió", async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "read_file") {
        expect(args).toEqual({ path: "/proj/src/a.ts" });
        return "hola mon\n";
      }
      if (cmd === "write_file") return undefined;
      throw new Error("inesperat: " + cmd);
    });
    const r = await applyEditBlocks("/proj", [
      { path: "src/a.ts", edits: [{ find: "hola", replace: "adeu" }] },
    ]);
    expect(r.edited).toEqual(["src/a.ts (1 edició)"]);
    expect(r.failed).toEqual([]);
    expect(r.noops).toEqual([]);
    expect(crides("write_file")).toEqual([
      ["write_file", { path: "/proj/src/a.ts", content: "adeu mon\n" }],
    ]);
    // El contingut VELL es captura ABANS d'escriure (diff roig/verd).
    expect(mockCapture).toHaveBeenCalledWith("/proj/src/a.ts");
    expect(mockNote).toHaveBeenCalledWith("/proj/src/a.ts", "adeu mon\n", "", false);
  });

  it("CERCA que no coincideix → failed amb el fragment REAL semblant NUMERAT", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "read_file") return "function suma(a, b) {\n  return a + b;\n}\n";
      throw new Error("inesperat: " + cmd);
    });
    const r = await applyEditBlocks("/proj", [
      {
        path: "src/a.ts",
        edits: [
          {
            find: "function suma(a, b) {\n  return a - b;\n}",
            replace: "function suma(a, b) {\n  return a * b;\n}",
          },
        ],
      },
    ]);
    expect(r.edited).toEqual([]);
    expect(r.failed).toHaveLength(1);
    // Missatge de «no coincideix» + fragment semblant amb números de línia
    // (dues de les tres línies coincideixen: la finestra real s'hi mostra).
    expect(r.failed[0]).toContain("agentFlow.editNoMatch");
    expect(r.failed[0]).toContain("agentFlow.editClosest");
    expect(r.failed[0]).toContain("1│ function suma(a, b) {");
    expect(r.failed[0]).toContain("2│   return a + b;");
    // Cap escriptura: el fitxer no es toca si el CERCA no hi és.
    expect(crides("write_file")).toEqual([]);
  });

  it("CERCA == NOU → edició INÚTIL rebutjada (noops), el disc no es toca", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "read_file") return "hola mon\n";
      throw new Error("inesperat: " + cmd);
    });
    const r = await applyEditBlocks("/proj", [
      { path: "a.ts", edits: [{ find: "hola", replace: "hola" }] },
    ]);
    expect(r.noops).toHaveLength(1);
    expect(r.noops[0]).toContain("idèntiques");
    expect(r.edited).toEqual([]);
    expect(crides("write_file")).toEqual([]);
  });

  it("fitxer inexistent → failed explícit, MAI es crea un fitxer nou", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "read_file") throw new Error("no such file");
      throw new Error("inesperat: " + cmd);
    });
    const r = await applyEditBlocks("/proj", [
      { path: "no-hi.ts", edits: [{ find: "a", replace: "b" }] },
    ]);
    expect(r.failed).toEqual(["no-hi.ts: el fitxer no existeix (no es pot editar)"]);
    expect(crides("write_file")).toEqual([]);
  });

  it("rutes amb «..» s'ometen: cap lectura ni escriptura fora del projecte", async () => {
    const r = await applyEditBlocks("/proj", [
      { path: "../fora.ts", edits: [{ find: "a", replace: "b" }] },
    ]);
    expect(r).toEqual({ edited: [], failed: [], noops: [], outside: [] });
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("@edit: amb RUTA ABSOLUTA del projecte → relativizada (mai carpeta falsa)", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "read_file") return "hola mon\n";
      if (cmd === "write_file") return undefined;
      throw new Error("inesperat: " + cmd);
    });
    const base = "/Volumes/Toshi/programes/forestin the sky";
    const r = await applyEditBlocks(base, [
      { path: `${base}/main.py`, edits: [{ find: "hola", replace: "adeu" }] },
    ]);
    expect(r.edited).toEqual(["main.py (1 edició)"]);
    expect(crides("write_file")).toEqual([
      ["write_file", { path: `${base}/main.py`, content: "adeu mon\n" }],
    ]);
  });

  it("@edit: amb ruta ABSOLUTA FORA del projecte → «outside», mai escriure en silenci", async () => {
    const r = await applyEditBlocks("/proj", [
      { path: "/Volumes/Altre/joc/main.py", edits: [{ find: "a", replace: "b" }] },
    ]);
    expect(r.outside).toEqual(["/Volumes/Altre/joc/main.py"]);
    expect(r.edited).toEqual([]);
    expect(mockInvoke).not.toHaveBeenCalled();
  });
});

// ── autoMaterialize: el camí automàtic del final de torn ───────────────────
describe("autoMaterialize", () => {
  it("sense projecte obert + codi → AVÍS explícit al xat, cap escriptura", async () => {
    mockWs.mockReturnValue({ root: null, refreshTree } as any);
    const out = await autoMaterialize(bloc("a.ts", "ts", "export const a = 1;"));
    expect(out).toContain("NO HI HA CAP PROJECTE OBERT");
    expect(crides("write_file")).toEqual([]);
  });

  it("escriu els blocs @file: tancats amb el contingut EXACTE (sense fences)", async () => {
    const raw =
      bloc("src/a.ts", "ts", "export const a = 1;") +
      "\n" +
      bloc("src/b.ts", "ts", "export const b = 2;");
    const out = await autoMaterialize(raw);
    expect(crides("write_file")).toEqual([
      ["write_file", { path: "/proj/src/a.ts", content: "export const a = 1;\n" }],
      ["write_file", { path: "/proj/src/b.ts", content: "export const b = 2;\n" }],
    ]);
    expect(out).toContain("📁 Aplicats automàticament al projecte (2 fitxers)");
    expect(out).toContain("• src/a.ts");
    // Cada fitxer escrit passa per la revisió verd/roig i l'arbre es refresca.
    expect(mockNote).toHaveBeenCalledTimes(2);
    expect(mockArm).toHaveBeenCalled();
    expect(refreshTree).toHaveBeenCalled();
  });

  it("blocs de PROSA sense ruta → avís skippedBlocks, MAI rutes inventades", async () => {
    const out = await autoMaterialize(
      "Aquí tens el codi demanat:\n```ts\nexport const x = 1;\n```"
    );
    expect(out).toContain("agentFlow.skippedBlocks");
    expect(out).toContain("•");
    expect(crides("write_file")).toEqual([]);
  });

  it("blocs DUPLICATS → només s'escriu l'última versió + avís ♻️", async () => {
    const raw =
      bloc("a.ts", "ts", "export const a = 1;") +
      "\n" +
      bloc("a.ts", "ts", "export const a = 2;");
    const out = await autoMaterialize(raw);
    expect(crides("write_file")).toEqual([
      ["write_file", { path: "/proj/a.ts", content: "export const a = 2;\n" }],
    ]);
    expect(out).toContain("♻️ La IA ha repetit 1 bloc");
  });

  it("bloc_N de CODI → codi_ia/; bloc_N.md (documentació) es descarta", async () => {
    const raw =
      bloc("bloc_1.py", "py", "print(1)") + "\n" + bloc("bloc_2.md", "md", "# Document");
    const out = await autoMaterialize(raw);
    expect(crides("write_file")).toEqual([
      ["write_file", { path: "/proj/codi_ia/bloc_1.py", content: "print(1)\n" }],
    ]);
    expect(out).toContain("• codi_ia/bloc_1.py");
    expect(out).not.toContain("bloc_2.md");
  });

  it("rutes amb «..» s'ometen: cap escriptura fora del projecte", async () => {
    const out = await autoMaterialize(bloc("../fora.txt", "txt", "fora"));
    expect(out).toBeNull();
    expect(crides("write_file")).toEqual([]);
  });

  it("@file: amb RUTA ABSOLUTA del projecte → relativizada (mai carpeta falsa)", async () => {
    // El cwd el posa NoOrbit al system prompt i el model el copia dins la
    // directriu: abans naixia «<arrel>/Volumes/Toshi/…» al mig del projecte.
    const base = "/Volumes/Toshi/programes/forestin the sky";
    mockWs.mockReturnValue({ root: base, refreshTree } as any);
    const out = await autoMaterialize(bloc(`${base}/src/main.py`, "py", "print(1)"));
    expect(crides("write_file")).toEqual([
      ["write_file", { path: `${base}/src/main.py`, content: "print(1)\n" }],
    ]);
    expect(out).toContain("• src/main.py");
    expect(out).not.toContain("Volumes/Toshi/programes/forestin the sky/Volumes");
  });

  it("@file: amb ruta ABSOLUTA FORA del projecte → ⛔ avís visible, cap escriptura", async () => {
    const out = await autoMaterialize(bloc("/Volumes/Altre/joc/main.py", "py", "print(1)"));
    expect(out).toContain("agentFlow.pathsOutsideProject");
    expect(out).toContain("/Volumes/Altre/joc/main.py");
    expect(crides("write_file")).toEqual([]);
  });

  it("@delete: va DESPRÉS de les escriptures i descarta la marca de revisió", async () => {
    const raw = bloc("a.ts", "ts", "export const a = 1;") + "\n@delete: vell.ts";
    const out = await autoMaterialize(raw);
    expect(crides("delete_path")).toEqual([["delete_path", { path: "/proj/vell.ts" }]]);
    expect(mockDrop).toHaveBeenCalledWith("/proj/vell.ts");
    expect(out).toContain("🗑 Esborrats del projecte (1)");
    expect(out).toContain("• vell.ts");
  });

  it("@edit: delega a applyEditBlocks i informa de les edicions aplicades", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "read_file") return "hola mon\n";
      if (cmd === "write_file") return undefined;
      throw new Error("inesperat: " + cmd);
    });
    const raw =
      "@edit: src/a.ts\n<<<<<<< SEARCH\nhola\n=======\nadeu\n>>>>>>> REPLACE";
    const out = await autoMaterialize(raw);
    expect(crides("write_file")).toEqual([
      ["write_file", { path: "/proj/src/a.ts", content: "adeu mon\n" }],
    ]);
    expect(out).toContain("✏️ Edicions aplicades al codi existent (1)");
  });

  it("errors de sintaxi del motor lleuger → avís visible al resum", async () => {
    mockLint.mockReturnValue(2);
    const out = await autoMaterialize(bloc("a.ts", "ts", "export const a = {"));
    expect(out).toContain("⚠️ Detectats 2 errors de sintaxi");
    // El fitxer S'escriu igualment (l'avís és per al panell Problemes).
    expect(crides("write_file")).toHaveLength(1);
  });

  it("resposta sense res materialitzable → null (cap soroll al xat)", async () => {
    expect(await autoMaterialize("D'acord, ho faré després.")).toBeNull();
    expect(mockInvoke).not.toHaveBeenCalled();
  });
});

// ── materialize: el botó «Convertir en fitxers» ────────────────────────────
describe("materialize", () => {
  it("sense projecte obert → error visible i null, cap escriptura", async () => {
    mockWs.mockReturnValue({ root: null, refreshTree } as any);
    const r = await useAgentStore.getState().materialize(bloc("a.ts", "ts", "x"));
    expect(r).toBeNull();
    expect(sess().error).toContain("Obre primer una carpeta de projecte");
    expect(crides("write_file")).toEqual([]);
  });

  it("happy path → «✅ Creats N fitxers a «proj»» + lastResult + revisió", async () => {
    const raw =
      bloc("src/a.ts", "ts", "export const a = 1;") +
      "\n" +
      bloc("src/b.ts", "ts", "export const b = 2;");
    const r = await useAgentStore.getState().materialize(raw);
    expect(r).toContain("✅ Creats 2 fitxers a «proj»");
    expect(r).toContain("• src/a.ts");
    expect(sess().lastResult).toBe(r);
    expect(sess().error).toBeNull();
    expect(crides("write_file")).toHaveLength(2);
    expect(mockArm).toHaveBeenCalled();
  });

  it("només DOCUMENTACIÓ (bloc_N.md) → error honest, no es crea res", async () => {
    const r = await useAgentStore.getState().materialize(bloc("bloc_1.md", "md", "# Títol"));
    expect(r).toBeNull();
    expect(sess().error).toContain("DOCUMENTACIÓ");
    expect(crides("write_file")).toEqual([]);
  });

  it("sense cap bloc de codi → error amb instruccions, no materialitza", async () => {
    const r = await useAgentStore.getState().materialize("hola, com estàs?");
    expect(r).toBeNull();
    expect(sess().error).toContain("no té cap bloc de codi");
    expect(crides("write_file")).toEqual([]);
  });
});

// ── send: routing bàsic amb invoke moquejat ────────────────────────────────
describe("send (routing)", () => {
  it("text buit → null sense cridar el backend", async () => {
    expect(await useAgentStore.getState().send("   ")).toBeNull();
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("@video sense projecte → nota explícita, MAI video_directive", async () => {
    mockWs.mockReturnValue({ root: null, refreshTree } as any);
    const r = await useAgentStore.getState().send("@video: munta els clips");
    expect(r).toBe("@video: munta els clips");
    expect(crides("video_directive")).toEqual([]);
    const last = sess().messages[sess().messages.length - 1];
    expect(last.text).toContain("Per muntar vídeo necessito un projecte obert");
  });

  it("@video amb projecte → video_directive amb el contracte EXACTE", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "video_directive") return "muntatge fet";
      throw new Error("inesperat: " + cmd);
    });
    const r = await useAgentStore.getState().send("@video: munta els clips");
    expect(r).toBe("@video: munta els clips");
    expect(crides("video_directive")).toEqual([
      ["video_directive", { projectPath: "/proj", args: "munta els clips" }],
    ]);
    expect(crides("send_prompt_stream")).toEqual([]);
    const last = sess().messages[sess().messages.length - 1];
    expect(last.text).toContain("muntatge fet");
  });

  it("@computer sense permís → nota de permís, MAI computer_agent_run", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "computer_permissions") return { enabled: false };
      throw new Error("inesperat: " + cmd);
    });
    const r = await useAgentStore.getState().send("@computer: obre Blender");
    expect(r).toBe("obre Blender");
    expect(crides("computer_agent_run")).toEqual([]);
    const last = sess().messages[sess().messages.length - 1];
    expect(last.text).toContain("necessito el teu permís");
  });

  it("@computer amb permís → computer_agent_run { goal } i resposta al xat", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "computer_permissions") return { enabled: true };
      if (cmd === "computer_agent_run") return "Blender obert";
      throw new Error("inesperat: " + cmd);
    });
    const r = await useAgentStore.getState().send("@computer: obre Blender");
    expect(r).toBe("obre Blender");
    expect(crides("computer_agent_run")).toEqual([
      ["computer_agent_run", { goal: "obre Blender" }],
    ]);
    const last = sess().messages[sess().messages.length - 1];
    expect(last.text).toContain("Blender obert");
  });

  it("ordre de Blender → delega a apply('blender', …), NO al model de text", async () => {
    const applySpy = vi
      .spyOn(useAgentStore.getState(), "apply")
      .mockResolvedValue("blender-ok");
    try {
      const r = await useAgentStore.getState().send("dibuixa un cub a blender");
      expect(r).toBe("blender-ok");
      expect(applySpy).toHaveBeenCalledWith("blender", "dibuixa un cub", undefined);
      expect(crides("send_prompt_stream")).toEqual([]);
    } finally {
      applySpy.mockRestore();
    }
  });

  it("flux normal → send_prompt_stream (question/history) + el codi de la resposta es materialitza", async () => {
    const resposta = bloc("wp-content/themes/orbit/style.css", "css", "/* Theme Name: Orbit */");
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_skills") return [];
      if (cmd === "ai_last_thinking") return null;
      if (cmd === "send_prompt_stream") return resposta;
      if (cmd === "write_file") return undefined;
      throw new Error("inesperat: " + cmd);
    });
    const text = "crea el codi d'un tema de wordpress anomenat Orbit";
    const r = await useAgentStore.getState().send(text);
    // Contracte del comandament: la pregunta REAL viatja separada del prompt.
    expect(crides("send_prompt_stream")).toHaveLength(1);
    expect(mockInvoke).toHaveBeenCalledWith(
      "send_prompt_stream",
      expect.objectContaining({
        prompt: text,
        question: text,
        session: "main",
        provider: null,
        model: null,
        history: [],
      })
    );
    // El camí autònom COMPLET: resposta → autoMaterialize → disc.
    expect(crides("write_file")).toEqual([
      [
        "write_file",
        { path: "/proj/wp-content/themes/orbit/style.css", content: "/* Theme Name: Orbit */\n" },
      ],
    ]);
    expect(r).toContain("📁 Aplicats automàticament al projecte (1 fitxers)");
    expect(sess().running).toBe(false);
    expect(sess().error).toBeNull();
    // El torn queda tancat: missatge d'usuari + resposta de l'assistent.
    const msgs = sess().messages;
    expect(msgs[0]).toMatchObject({ role: "user", text });
    expect(msgs[msgs.length - 1].role).toBe("assistant");
  });

  it("error del backend → error visible al xat, sense materialitzar res", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_skills") return [];
      if (cmd === "ai_last_thinking") return null;
      if (cmd === "send_prompt_stream") throw new Error("model no trobat");
      throw new Error("inesperat: " + cmd);
    });
    const r = await useAgentStore.getState().send("hola");
    expect(r).toBeNull();
    expect(sess().error).toContain("model no trobat");
    expect(sess().running).toBe(false);
    expect(crides("write_file")).toEqual([]);
  });
});