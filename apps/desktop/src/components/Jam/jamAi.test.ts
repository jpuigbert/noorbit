/// Tests de la tria d'IA del panell «Autonomous Jam» (lògica pura).
import { describe, expect, it } from "vitest";
import { LOCAL, NATIVE, NATIVE_ID, PROV, resolAi } from "./jamAi";
import type { ProviderPublic } from "../../stores/providerStore";

function proveidor(over: Partial<ProviderPublic>): ProviderPublic {
  return {
    id: "openai",
    name: "OpenAI",
    base_url: "https://api.openai.com/v1",
    model: "gpt-4o-mini",
    kind: "openai",
    auth: "bearer",
    header_name: "Authorization",
    has_token: true,
    token_masked: "sk-…",
    enabled: true,
    ...over,
  };
}

describe("resolAi", () => {
  it("un model local d'Ollama → proveïdor ollama + el model triat", () => {
    expect(resolAi(LOCAL + "qwen3:4b", [])).toEqual({
      provider: "ollama",
      model: "qwen3:4b",
    });
  });

  it("la IA integrada → noorbit_native + el model del proveïdor natiu", () => {
    const providers = [proveidor({ id: "noorbit_native", kind: "native", model: "natiu" })];
    expect(resolAi(NATIVE + NATIVE_ID, providers)).toEqual({
      provider: NATIVE_ID,
      model: "natiu",
    });
  });

  it("un proveïdor en línia → el seu id + el seu model", () => {
    const providers = [proveidor({})];
    expect(resolAi(PROV + "openai", providers)).toEqual({
      provider: "openai",
      model: "gpt-4o-mini",
    });
  });

  it("un proveïdor que ja no existeix → null (deixa-ho al backend)", () => {
    expect(resolAi(PROV + "mort", [proveidor({})])).toEqual({
      provider: null,
      model: null,
    });
  });

  it("tria buida o desconeguda → null (caiguda del backend)", () => {
    expect(resolAi("", [])).toEqual({ provider: null, model: null });
    expect(resolAi("qualsevol", [])).toEqual({ provider: null, model: null });
  });

  it("un model local sense nom → ollama sense model (el backend tria)", () => {
    expect(resolAi(LOCAL, [])).toEqual({ provider: "ollama", model: null });
  });
});
