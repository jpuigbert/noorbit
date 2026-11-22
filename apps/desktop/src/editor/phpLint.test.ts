/// Tests del parser PUR de la sortida de `php -l` (editor/phpLint.ts):
/// formats reals del CLI de PHP, múltiples errors, línia de resum ignorada
/// i degradació honesta quan la sortida no és un error recognoscible.
import { describe, expect, it } from "vitest";
import { parsePhpLintOutput } from "./phpLint";

describe("parsePhpLintOutput", () => {
  it("parseja un «PHP Parse error» estàndard (línia + missatge)", () => {
    const out =
      'PHP Parse error:  syntax error, unexpected token "}" in /proj/index.php on line 12\n' +
      "Errors parsing /proj/index.php";
    const issues = parsePhpLintOutput(out);
    expect(issues).toEqual([
      { line: 12, message: 'syntax error, unexpected token "}"', severity: "error" },
    ]);
  });

  it("parseja un «PHP Fatal error» (p. ex. redeclaració de funció)", () => {
    const out = "PHP Fatal error:  Cannot redeclare foo() in /proj/lib.php on line 8";
    const issues = parsePhpLintOutput(out);
    expect(issues).toHaveLength(1);
    expect(issues[0].line).toBe(8);
    expect(issues[0].severity).toBe("error");
    expect(issues[0].message).toContain("Cannot redeclare foo()");
  });

  it("múltiples errors en una sola sortida", () => {
    const out = [
      "PHP Parse error:  syntax error, unexpected end of file in /proj/a.php on line 30",
      "PHP Fatal error:  Cannot redeclare bar() in /proj/a.php on line 4",
      "Errors parsing /proj/a.php",
    ].join("\n");
    const issues = parsePhpLintOutput(out);
    expect(issues.map((i) => i.line)).toEqual([30, 4]);
    expect(issues.every((i) => i.severity === "error")).toBe(true);
  });

  it("la línia de resum «Errors parsing …» NO compta com a error", () => {
    expect(parsePhpLintOutput("Errors parsing /proj/a.php")).toEqual([]);
  });

  it("«No syntax errors detected» (lint net) → cap problema", () => {
    expect(parsePhpLintOutput("No syntax errors detected in /proj/a.php")).toEqual([]);
  });

  it("sortida buida → cap problema", () => {
    expect(parsePhpLintOutput("")).toEqual([]);
    expect(parsePhpLintOutput("   \n  ")).toEqual([]);
  });

  it("sortida no recognoscible (p. ex. php no instal·lat) → AVÍS, mai fals verd", () => {
    const issues = parsePhpLintOutput("sh: php: command not found");
    expect(issues).toEqual([
      { line: 1, message: "sh: php: command not found", severity: "warning" },
    ]);
  });

  it("missatge amb «in» dins del text: es queda amb l'últim «in … on line N»", () => {
    const out =
      'PHP Parse error:  syntax error, unexpected "in" in /proj/a.php on line 3';
    const issues = parsePhpLintOutput(out);
    expect(issues).toHaveLength(1);
    expect(issues[0].line).toBe(3);
  });
});