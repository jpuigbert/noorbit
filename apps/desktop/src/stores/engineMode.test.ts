/// Tests del MOTOR de la IA (Fase 2) amb invoke MOQUEJAT: la preferència
/// «auto/local/cloud» es desa a localStorage (patró de la resta de
/// preferències de la UI), se sincronitza amb el backend amb el contracte
/// exacte «ai_engine_mode_set», i un valor desat invàlid o un backend caigut
/// MAI trenquen el funcionament local (cau a «auto»).
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(async () => undefined) }));

import { invoke } from "@tauri-apps/api/core";
import { loadEngineMode, useUIStore } from "./uiStore";

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  localStorage.clear();
  mockInvoke.mockReset();
  mockInvoke.mockResolvedValue("auto");
  useUIStore.setState({ engineMode: "auto" });
});

describe("engineMode (preferència del motor)", () => {
  it("per defecte és «auto» i un valor desat invàlid cau a «auto»", () => {
    expect(loadEngineMode()).toBe("auto");
    localStorage.setItem("noorbit.engineMode", "qualsevol-cosa");
    expect(loadEngineMode()).toBe("auto");
  });

  it("la preferència desada es rellegeix entre sessions", () => {
    localStorage.setItem("noorbit.engineMode", "local");
    expect(loadEngineMode()).toBe("local");
    localStorage.setItem("noorbit.engineMode", "cloud");
    expect(loadEngineMode()).toBe("cloud");
  });

  it("setEngineMode desa, actualitza l'estat i sincronitza el contracte exacte", () => {
    useUIStore.getState().setEngineMode("cloud");
    expect(localStorage.getItem("noorbit.engineMode")).toBe("cloud");
    expect(useUIStore.getState().engineMode).toBe("cloud");
    expect(mockInvoke).toHaveBeenCalledWith("ai_engine_mode_set", { mode: "cloud" });
  });

  it("tornar a «auto» també es persisteix i se sincronitza", () => {
    useUIStore.getState().setEngineMode("local");
    useUIStore.getState().setEngineMode("auto");
    expect(localStorage.getItem("noorbit.engineMode")).toBe("auto");
    expect(useUIStore.getState().engineMode).toBe("auto");
    expect(mockInvoke).toHaveBeenLastCalledWith("ai_engine_mode_set", { mode: "auto" });
  });

  it("si el backend falla, la preferència queda igualment desada (mai trenca el local)", () => {
    mockInvoke.mockRejectedValue(new Error("sense backend"));
    useUIStore.getState().setEngineMode("local");
    expect(useUIStore.getState().engineMode).toBe("local");
    expect(localStorage.getItem("noorbit.engineMode")).toBe("local");
  });
});