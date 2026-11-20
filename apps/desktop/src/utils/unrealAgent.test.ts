import { describe, expect, it } from "vitest";
import {
  agentPhaseKind,
  filterTools,
  groupToolsByPrefix,
  isMutatingTool,
  normalizeAgentPhase,
  shortResult,
  truncateToolResult,
  validateToolArgs,
} from "./unrealAgent";

const CATALOG = [
  { name: "spawn_actor", description: "Spawn a new actor in the level" },
  { name: "spawn_lights", description: "Spawn a row of lights" },
  { name: "list_level", description: "List the actors of the level" },
];

describe("normalització de fases", () => {
  it("conserva les fases conegudes del backend", () => {
    expect(normalizeAgentPhase("eina_ok")).toBe("eina_ok");
    expect(normalizeAgentPhase("answer")).toBe("answer");
    expect(normalizeAgentPhase("error")).toBe("error");
  });

  it("una fase nova no ha de fer caure la UI", () => {
    expect(normalizeAgentPhase("fase_del_any")).toBe("info");
  });
});

describe("pintada de cada fase", () => {
  it("distingeix eina, èxit, error i resposta", () => {
    expect(agentPhaseKind("eina")).toBe("tool");
    expect(agentPhaseKind("eina_ok")).toBe("ok");
    expect(agentPhaseKind("eina_err")).toBe("error");
    expect(agentPhaseKind("error")).toBe("error");
    expect(agentPhaseKind("answer")).toBe("answer");
    expect(agentPhaseKind("ronda")).toBe("info");
  });
});

describe("eines modificadores", () => {
  it("detecta els prefixes que poden modificar l'editor", () => {
    expect(isMutatingTool("spawn_actor")).toBe(true);
    expect(isMutatingTool("DeleteActor")).toBe(true);
    expect(isMutatingTool("list_level")).toBe(false);
  });
});

describe("cercadora del catàleg", () => {
  it("sense cerca retorna tot", () => {
    expect(filterTools(CATALOG, "   ")).toHaveLength(3);
  });

  it("filtra pel nom", () => {
    const r = filterTools(CATALOG, "spawn");
    expect(r.map((t) => t.name)).toEqual(["spawn_actor", "spawn_lights"]);
  });

  it("filtra per la descripció", () => {
    const r = filterTools(CATALOG, "lights");
    expect(r.map((t) => t.name)).toEqual(["spawn_lights"]);
  });
});

describe("agrupació per prefixe", () => {
  it("agrupa i ordena alfabèticament", () => {
    const g = groupToolsByPrefix(CATALOG);
    expect(g.map(([p]) => p)).toEqual(["list", "spawn"]);
    expect(g[1][1]).toHaveLength(2);
  });
});

describe("resultats grans", () => {
  it("retalla i ho marca", () => {
    const r = truncateToolResult("x".repeat(100), 10);
    expect(r.truncated).toBe(true);
    expect(r.text).toHaveLength(10);
  });

  it("no toca els resultats petits", () => {
    const r = truncateToolResult("curt", 10);
    expect(r.truncated).toBe(false);
    expect(r.text).toBe("curt");
  });

  it("el text curt aplana els espais", () => {
    expect(shortResult("a\n  b   c")).toBe("a b c");
  });
});

describe("validació dels arguments", () => {
  it("accepta un objecte JSON", () => {
    const r = validateToolArgs('{"name":"Cube"}', 1000);
    expect(r.error).toBeNull();
    expect(r.parsed).toEqual({ name: "Cube" });
  });

  it("buit equival a objecte buit", () => {
    expect(validateToolArgs("", 1000).parsed).toEqual({});
  });

  it("rebutja el JSON invàlid", () => {
    expect(validateToolArgs("{no és json}", 1000).error).toBeTruthy();
  });

  it("rebutja un array (cal un objecte)", () => {
    expect(validateToolArgs("[1,2]", 1000).error).toBeTruthy();
  });

  it("rebutja els arguments massa grans", () => {
    const r = validateToolArgs("x".repeat(500), 100);
    expect(r.error).toContain("massa grans");
    expect(r.parsed).toBeNull();
  });
});
