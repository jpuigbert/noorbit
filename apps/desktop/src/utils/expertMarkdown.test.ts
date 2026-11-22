import { describe, expect, it } from "vitest";
import { expertFileName, expertFromMarkdown, expertToMarkdown } from "./expertMarkdown";

describe("expertMarkdown (experts com a fitxers .md compartibles)", () => {
  it("serialitza capçalera i cos, i camps opcionals", () => {
    const md = expertToMarkdown({
      name: "Revisor",
      role: "Qualitat i errors",
      systemPrompt: "Revisa el codi.\nSense canviar res.",
      model: "llama3.1:8b",
      mode: "subagent",
    });
    expect(md).toContain("name: Revisor");
    expect(md).toContain("description: Qualitat i errors");
    expect(md).toContain("model: llama3.1:8b");
    expect(md).toContain("mode: subagent");
    expect(md).toContain("Revisa el codi.\nSense canviar res.");
  });

  it("no escriu model ni mode quan no toquen (mode 'all' és el per defecte)", () => {
    const md = expertToMarkdown({ name: "X", role: "", systemPrompt: "cos", model: null, mode: "all" });
    expect(md).not.toContain("model:");
    expect(md).not.toContain("mode:");
  });

  it("ida i volta: el que s'exporta es reimporta igual", () => {
    const src = {
      name: "Front-end",
      role: "Interfícies",
      systemPrompt: "Ets el front-end. Emet @file: sempre.",
      model: "qwen2.5-coder:7b",
      mode: "primary" as const,
    };
    const back = expertFromMarkdown(expertToMarkdown(src));
    expect(back).toEqual(src);
  });

  it("tolera fitxers sense capçalera: tot el text és el system prompt", () => {
    const back = expertFromMarkdown("Ets el tester de QA.\nEscrius proves reals.");
    expect(back.systemPrompt).toBe("Ets el tester de QA.\nEscrius proves reals.");
    // El nom surt de la primera línia perquè es pugui crear igualment.
    expect(back.name).toBe("Ets el tester de QA.");
  });

  it("accepta alias: role, 'el cap' i modes en català", () => {
    const back = expertFromMarkdown(
      ["---", "name: Arquitecte", "role: Estructura", "mode: xat", "---", "Cos."].join("\n")
    );
    expect(back.role).toBe("Estructura");
    expect(back.mode).toBe("primary");
    const sub = expertFromMarkdown(["---", "name: A", "mode: Equip", "---", "x"].join("\n"));
    expect(sub.mode).toBe("subagent");
  });

  it("ignora línies no-clau de la capçalera (YML complex)", () => {
    const back = expertFromMarkdown(
      ["---", "permissions:", "  - edit: deny", "name: Secure", "---", "Cos"].join("\n")
    );
    expect(back.name).toBe("Secure");
    expect(back.systemPrompt).toBe("Cos");
  });

  it("normalitza noms d'arxiu per exportar", () => {
    expect(expertFileName("Dissenyador 3D / WebGL")).toBe("dissenyador-3d-webgl.md");
    expect(expertFileName("  ")).toBe("especialista.md");
  });
});
