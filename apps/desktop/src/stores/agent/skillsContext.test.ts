import { describe, expect, it } from "vitest";
import { buildSkillsCatalog } from "./skillsContext";
import type { Skill } from "../skillStore";

function skill(name: string, description = ""): Skill {
  return { id: name, name, description, path: `/dades/skills/ecc/${name}/SKILL.md` };
}

describe("buildSkillsCatalog", () => {
  it("sense skills retorna cadena buida", () => {
    expect(buildSkillsCatalog([])).toBe("");
  });

  it("inclou nom, descripció i ruta de cada skill", () => {
    const out = buildSkillsCatalog([skill("react-patterns", "Patrons de React")]);
    expect(out).toContain("SKILLS INSTAL·LADES");
    expect(out).toContain("- react-patterns — Patrons de React");
    expect(out).toContain("/dades/skills/ecc/react-patterns/SKILL.md");
  });

  it("retalla descripcions llargues a 70 caràcters", () => {
    const llarga = "x".repeat(200);
    const out = buildSkillsCatalog([skill("alpha", llarga)]);
    expect(out).toContain("x".repeat(70) + "…");
    expect(out).not.toContain("x".repeat(71));
  });

  it("respecta el límit de mida i compta les skills tallades", () => {
    const moltes = Array.from({ length: 500 }, (_, i) => skill(`skill-${i}`, "descripció"));
    const out = buildSkillsCatalog(moltes, 1000);
    expect(out.length).toBeLessThan(1200);
    expect(out).toMatch(/i \d+ skills més/);
  });

  it("skills sense descripció mostren només el nom i la ruta", () => {
    const out = buildSkillsCatalog([skill("sense-desc")]);
    expect(out).toContain("- sense-desc (/dades/skills/ecc/sense-desc/SKILL.md)");
  });
});