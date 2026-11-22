/// Tests del store del TERMINAL PTY multi-sessió (Fase 3b) amb invoke i
/// events MOQUEJATS: contracte exacte dels comandaments (term_start,
/// term_write, term_resize, term_kill, term_list), creació/canvi/tancament
/// de pestanyes, streaming per `term://out` { id, data }, mort per
/// `term://exit` { id, code }, i ignorància dels events legacy { call_id }.
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../i18n", () => ({
  t: (k: string, vars?: Record<string, string | number>) =>
    vars?.n !== undefined ? `${k}:${vars.n}` : k,
}));
vi.mock("./toastStore", () => ({ toastError: vi.fn() }));

/// Handlers capturats dels `listen` (per simular els events del backend).
/// `vi.hoisted`: el factory de vi.mock s'eleva per damunt dels imports.
const handlers = vi.hoisted(() => new Map<string, (evt: { payload: unknown }) => void>());
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (evt: { payload: unknown }) => void) => {
    handlers.set(name, cb);
    return () => handlers.delete(name);
  }),
}));

import { invoke } from "@tauri-apps/api/core";
import { useTerminalStore, onSessionOutput } from "./terminalStore";
import { toastError } from "./toastStore";

const mockInvoke = vi.mocked(invoke);
const mockToastError = vi.mocked(toastError);

function emit(name: string, payload: unknown) {
  handlers.get(name)?.({ payload });
}

beforeEach(() => {
  mockInvoke.mockReset();
  mockToastError.mockReset();
  useTerminalStore.setState({ sessions: [], activeId: null });
});

describe("terminalStore.createSession", () => {
  it("crida el contracte EXACTE: term_start { cwd, cols, rows } i activa la pestanya", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "term_start") return "pty-1";
      if (cmd === "term_list") return [{ id: "pty-1", title: "zsh · projecte" }];
      throw new Error("comandament inesperat: " + cmd);
    });
    const id = await useTerminalStore.getState().createSession("/proj", 100, 30);
    expect(id).toBe("pty-1");
    expect(mockInvoke).toHaveBeenCalledWith("term_start", { cwd: "/proj", cols: 100, rows: 30 });
    const st = useTerminalStore.getState();
    expect(st.activeId).toBe("pty-1");
    expect(st.sessions).toHaveLength(1);
    // El sync posterior agafa el títol real (shell · carpeta) del backend.
    await vi.waitFor(() => {
      expect(useTerminalStore.getState().sessions[0].title).toBe("zsh · projecte");
    });
  });

  it("si el backend falla (límit de sessions), fa toast en català i no crea pestanya", async () => {
    mockInvoke.mockRejectedValue("Límit de 8 sessions de terminal assolit.");
    const id = await useTerminalStore.getState().createSession("/proj");
    expect(id).toBeNull();
    expect(useTerminalStore.getState().sessions).toHaveLength(0);
    expect(mockToastError).toHaveBeenCalledWith(
      "terminal.errStart",
      "Límit de 8 sessions de terminal assolit."
    );
  });
});

describe("terminalStore.write i resize (stdin interactiu)", () => {
  it("onData → term_write { id, data } amb el contingut EXACTE (incloent \\r)", () => {
    mockInvoke.mockResolvedValue(undefined);
    useTerminalStore.getState().write("pty-1", "npm init\r");
    expect(mockInvoke).toHaveBeenCalledWith("term_write", { id: "pty-1", data: "npm init\r" });
    useTerminalStore.getState().write("pty-1", "\x03"); // Ctrl+C
    expect(mockInvoke).toHaveBeenCalledWith("term_write", { id: "pty-1", data: "\x03" });
  });

  it("resize → term_resize { id, cols, rows }", () => {
    mockInvoke.mockResolvedValue(undefined);
    useTerminalStore.getState().resize("pty-1", 120, 40);
    expect(mockInvoke).toHaveBeenCalledWith("term_resize", { id: "pty-1", cols: 120, rows: 40 });
  });
});

describe("terminalStore.closeSession", () => {
  it("crida term_kill { id }, treu la pestanya i activa la veïna", async () => {
    mockInvoke.mockResolvedValue(undefined);
    useTerminalStore.setState({
      sessions: [
        { id: "a", title: "a", alive: true, code: null },
        { id: "b", title: "b", alive: true, code: null },
      ],
      activeId: "a",
    });
    await useTerminalStore.getState().closeSession("a");
    expect(mockInvoke).toHaveBeenCalledWith("term_kill", { id: "a" });
    const st = useTerminalStore.getState();
    expect(st.sessions.map((s) => s.id)).toEqual(["b"]);
    expect(st.activeId).toBe("b");
  });

  it("tancar l'última sessió deixa activeId a null", async () => {
    mockInvoke.mockResolvedValue(undefined);
    useTerminalStore.setState({
      sessions: [{ id: "a", title: "a", alive: true, code: null }],
      activeId: "a",
    });
    await useTerminalStore.getState().closeSession("a");
    expect(useTerminalStore.getState().activeId).toBeNull();
    expect(useTerminalStore.getState().sessions).toHaveLength(0);
  });
});

describe("terminalStore events (streaming i mort)", () => {
  beforeEach(async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "term_list") return [{ id: "pty-1", title: "zsh · proj" }];
      return undefined;
    });
    await useTerminalStore.getState().init();
  });

  it("term://out { id, data } arriba al subscriptor de la sessió", () => {
    const got: string[] = [];
    const un = onSessionOutput("pty-1", (d) => got.push(d));
    emit("term://out", { id: "pty-1", data: "hola\r\n" });
    emit("term://out", { id: "altra", data: "no meua" });
    un();
    emit("term://out", { id: "pty-1", data: "després d'unlisten" });
    expect(got).toEqual(["hola\r\n"]);
  });

  it("term://exit { id, code } marca la sessió morta i escriu la línia de comiat", () => {
    const got: string[] = [];
    const un = onSessionOutput("pty-1", (d) => got.push(d));
    emit("term://exit", { id: "pty-1", code: 3 });
    un();
    const sess = useTerminalStore.getState().sessions.find((s) => s.id === "pty-1");
    expect(sess?.alive).toBe(false);
    expect(sess?.code).toBe(3);
    expect(got.join("")).toContain("terminal.exitedCode:3");
  });

  it("term://exit amb code null (mort per senyal) també marca la sessió", () => {
    emit("term://exit", { id: "pty-1", code: null });
    const sess = useTerminalStore.getState().sessions.find((s) => s.id === "pty-1");
    expect(sess?.alive).toBe(false);
    expect(sess?.code).toBeNull();
  });

  it("IGNORA els events legacy { call_id } de run_command (compatibilitat)", () => {
    const got: string[] = [];
    const un = onSessionOutput("pty-1", (d) => got.push(d));
    emit("term://out", { call_id: "t123", kind: "out", text: "llegat" });
    emit("term://exit", { call_id: "t123", code: 0 });
    un();
    expect(got).toEqual([]);
    expect(useTerminalStore.getState().sessions[0].alive).toBe(true);
  });
});

describe("terminalStore.sync (finestra separada / reattach)", () => {
  it("incorpora sessions creades des d'una altra finestra", async () => {
    mockInvoke.mockResolvedValue([
      { id: "pty-x", title: "zsh · proj" },
      { id: "pty-y", title: "bash · altra" },
    ]);
    await useTerminalStore.getState().sync();
    const st = useTerminalStore.getState();
    expect(st.sessions.map((s) => s.id)).toEqual(["pty-x", "pty-y"]);
    expect(st.activeId).toBe("pty-x");
  });

  it("una sessió local viva que el backend ja no té queda marcada com a morta", async () => {
    useTerminalStore.setState({
      sessions: [{ id: "vella", title: "vella", alive: true, code: null }],
      activeId: "vella",
    });
    mockInvoke.mockResolvedValue([]);
    await useTerminalStore.getState().sync();
    const st = useTerminalStore.getState();
    expect(st.sessions[0].alive).toBe(false);
    // La pestanya es conserva (scrollback) i segueix sent l'activa.
    expect(st.activeId).toBe("vella");
  });
});