/// Tests dels parsers PURS de directrius: «@agent:», «@use:», «@todo:» i el
/// filtre de línies per a «Materialitzar».
import { describe, expect, it } from "vitest";
import {
  matchExpert,
  parseAgentDirectives,
  parseTodoDirectives,
  parseUseDirectives,
  stripAgentDirectiveLines,
} from "./directives";

describe("parseAgentDirectives", () => {
  it("format complet: Nom | Rol | Instruccions", () => {
    const r = parseAgentDirectives("@agent: Tester | QA | Escriu tests reals");
    expect(r).toEqual([
      { name: "Tester", role: "QA", systemPrompt: "Escriu tests reals" },
    ]);
  });

  it("dues parts: genera prompt per defecte amb el rol", () => {
    const r = parseAgentDirectives("@agent: Dissenyador | UX");
    expect(r).toHaveLength(1);
    expect(r[0].name).toBe("Dissenyador");
    expect(r[0].systemPrompt).toContain("Dissenyador");
    expect(r[0].systemPrompt).toContain("UX");
  });

  it("una part: nom curt i prompt genèric", () => {
    const r = parseAgentDirectives("@agent: Ajudant");
    expect(r[0].name).toBe("Ajudant");
    expect(r[0].role).toBe("");
    expect(r[0].systemPrompt).toContain("Ajudant");
  });

  it("tolera «·», doble pipe i àlies (@expert:, @especialista:)", () => {
    const r = parseAgentDirectives(
      "@expert: A · rol · instruccions\n@especialista: B || rol2 || inst2"
    );
    expect(r.map((x) => x.name)).toEqual(["A", "B"]);
    expect(r[1].systemPrompt).toBe("inst2");
  });

  it("deduplica pel nom (case-insensitive) i ignora línies sense nom", () => {
    const r = parseAgentDirectives("@agent: Rep | r | i\n@agent: rep | r | i\n@agent:");
    expect(r).toHaveLength(1);
  });

  it("instruccions amb pipes extra s'uneixen amb « · »", () => {
    const r = parseAgentDirectives("@agent: X | r | part1 | part2");
    expect(r[0].systemPrompt).toBe("part1 · part2");
  });
});

describe("stripAgentDirectiveLines", () => {
  it("treu @agent:/@use:/@todo: i deixa la resta intacta", () => {
    const raw = [
      "@agent: A | r | i",
      "codi normal",
      "@use: B | tasca",
      "@todo: fer alguna cosa",
      "@done: cosa feta",
      "altra línia",
    ].join("\n");
    expect(stripAgentDirectiveLines(raw)).toBe("codi normal\naltra línia");
  });
});

describe("parseUseDirectives", () => {
  it("extreu agent i tasca, i deduplica", () => {
    const r = parseUseDirectives(
      "@use: Tester | escriu tests\n@use: tester | escriu tests\n@use: Dissenyador"
    );
    expect(r).toEqual([
      { agent: "Tester", task: "escriu tests" },
      { agent: "Dissenyador", task: "" },
    ]);
  });

  it("tolera @delega: i @delegate:", () => {
    const r = parseUseDirectives("@delega: A | t1\n@delegate: B | t2");
    expect(r.map((x) => x.agent)).toEqual(["A", "B"]);
  });
});

describe("parseTodoDirectives", () => {
  it("separa add/done/doing amb els àlies catalans", () => {
    const r = parseTodoDirectives(
      [
        "@todo: primera tasca",
        "@pendent: segona",
        "@done: cosa acabada",
        "@feta: altra acabada",
        "@doing: en marxa",
        "@fent: també en marxa",
      ].join("\n")
    );
    expect(r.add).toEqual(["primera tasca", "segona"]);
    expect(r.done).toEqual(["cosa acabada", "altra acabada"]);
    expect(r.doing).toEqual(["en marxa", "també en marxa"]);
  });

  it("ignora línies buides i sense directriu", () => {
    const r = parseTodoDirectives("text normal\n@todo:\n@done:   ");
    expect(r).toEqual({ add: [], done: [], doing: [] });
  });

  it("netja cometes i backticks dels extrems", () => {
    const r = parseTodoDirectives('@todo: "tasca entre cometes"');
    expect(r.add).toEqual(["tasca entre cometes"]);
  });
});

describe("matchExpert", () => {
  const experts = [
    { name: "Tester de QA" },
    { name: "Dissenyador Frontend" },
    { name: "Revisor de Seguretat" },
  ];

  it("exacte ignoreix majúscules i accents", () => {
    expect(matchExpert("tester de qa", experts)?.name).toBe("Tester de QA");
    expect(matchExpert("Revisor de Seguretat", experts)?.name).toBe("Revisor de Seguretat");
  });

  it("tolera articles, cometes i noms parcials", () => {
    expect(matchExpert("«el Tester»", experts)?.name).toBe("Tester de QA");
    expect(matchExpert("Tester", experts)?.name).toBe("Tester de QA");
    expect(matchExpert("Tester", [])).toBeNull();
  });

  it("emparella per sobreposició de paraules", () => {
    expect(matchExpert("Dissenyador d'interfícies frontend", experts)?.name).toBe(
      "Dissenyador Frontend"
    );
  });

  it("no inventa: cap semblança raonable retorna null", () => {
    expect(matchExpert("Cuiner de pastisseria", experts)).toBeNull();
    expect(matchExpert("", experts)).toBeNull();
  });
});