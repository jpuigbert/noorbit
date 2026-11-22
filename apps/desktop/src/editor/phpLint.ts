/// Parser PUR de la sortida de `php -l` (via el comandament `lint_paths`).
/// Sense dependències de Monaco ni de Tauri: testejable aïlladament.
///
/// Format real de PHP CLI:
///   PHP Parse error:  syntax error, unexpected token "}" in /ruta/file.php on line 12
///   PHP Fatal error:  Cannot redeclare foo() in /ruta/file.php on line 8
///   Errors parsing /ruta/file.php   (línia de resum: s'ignora)

export interface PhpLintIssue {
  /// Línia 1-based que reporta PHP.
  line: number;
  message: string;
  severity: "error" | "warning";
}

const PHP_ERROR_RE =
  /PHP (?:Parse|Fatal) error:\s*([^\n]+?)\s+in\s+[^\n]+?\s+on line (\d+)/g;

/// Extreu els errors reals (línia + missatge) de la sortida del lint.
/// Si no hi ha cap error recognoscible però la sortida no és buida, retorna
/// un AVÍS amb la primera línia útil (degradació honesta: p. ex. «php» no
/// instal·lat — mai un fals «verd»).
export function parsePhpLintOutput(output: string): PhpLintIssue[] {
  const issues: PhpLintIssue[] = [];
  for (const m of output.matchAll(PHP_ERROR_RE)) {
    const line = Number(m[2]);
    issues.push({
      line: Number.isFinite(line) && line > 0 ? line : 1,
      message: m[1].trim(),
      severity: "error",
    });
  }
  if (issues.length === 0) {
    const useful = output
      .split("\n")
      .map((l) => l.trim())
      .find((l) => l.length > 0 && !/^Errors parsing\b/i.test(l) && !/^No syntax errors/i.test(l));
    if (useful) {
      issues.push({ line: 1, message: useful, severity: "warning" });
    }
  }
  return issues;
}