// Panell «Problemes»: llista tots els diagnòstics detectats (errors/avisos),
// tant dels fitxers oberts a l'editor com dels que la IA escriu al disc sense
// obrir-los. Agrupats per fitxer. Fer clic obri el fitxer i salta a la línia
// marcada. És el «lloc on es registren els problemes», a l'estil del panell
// Problems de VS Code / Sublime.
import { AlertCircle, AlertTriangle, Info, ListChecks, Trash2 } from "lucide-react";
import { useT } from "../../i18n";
import { useProblemsStore, sortProblems, type Problem } from "../../stores/problemsStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { usePreviewStore } from "../../stores/previewStore";
import { revealAt } from "../../editor/editorApi";

function sevIcon(s: Problem["severity"]) {
  if (s === "error") return <AlertCircle size={13} color="var(--error, #f85149)" />;
  if (s === "warning") return <AlertTriangle size={13} color="var(--warning, #d29922)" />;
  return <Info size={13} color="var(--text-2)" />;
}

export default function ProblemsPanel() {
  const { t } = useT();
  const byFile = useProblemsStore((s) => s.byFile);
  const clearAll = useProblemsStore((s) => s.clearAll);
  const loadFile = useWorkspaceStore((s) => s.loadFile);

  const files = Object.keys(byFile).sort();
  const total = files.reduce((n, f) => n + byFile[f].length, 0);
  const errors = files.reduce(
    (n, f) => n + byFile[f].filter((p) => p.severity === "error").length,
    0
  );

  const jump = async (p: Problem) => {
    await loadFile(p.path);
    // L'editor encara s'està muntant; dona temps abans de desplaçar.
    setTimeout(() => revealAt(p.line, p.column), 120);
  };

  return (
    <div className="problems-panel">
      <div className="problems-head">
        <ListChecks size={14} />
        <span>
          {total === 0
            ? t("problems.none")
            : `${total} ${total === 1 ? t("problems.problem") : t("problems.problems")} · ${errors} ${
                errors === 1 ? t("problems.error") : t("problems.errors")
              }`}
        </span>
        {total > 0 && (
          <button className="icon-btn" title={t("problems.clear")} onClick={() => clearAll()}>
            <Trash2 size={13} />
          </button>
        )}
      </div>

      {total === 0 ? (
        <div className="problems-empty">
          <p>{t("problems.empty")}</p>
          <p className="problems-empty-hint">
            {t("rightPanel.problems")}: {t("problems.filesWith", { n: files.length })}
          </p>
        </div>
      ) : (
        <div className="problems-body">
          {files.map((f) => (
            <div key={f} className="problems-file">
              <div className="problems-file-name" title={f}>
                {f.split(/[/\\]/).pop()}
              </div>
              {sortProblems(byFile[f]).map((p, i) => (
                <button
                  key={i}
                  className="problems-item"
                  onClick={() => void jump(p)}
                  title={t("problems.jumpHint", { loc: `${f}:${p.line}:${p.column}` })}
                >
                  {sevIcon(p.severity)}
                  <span className="problems-msg">{p.message}</span>
                  <span className="problems-loc">
                    {t("problems.loc", { line: p.line, col: p.column })}
                  </span>
                  <span className="problems-src">{p.source}</span>
                </button>
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/// Botó compacte per a la barra d'estat: total d'errors + salta al panell.
export function ProblemsBadge() {
  const { t } = useT();
  const byFile = useProblemsStore((s) => s.byFile);
  const setTab = usePreviewStore((s) => s.setRightTab);
  let errors = 0;
  let warnings = 0;
  for (const list of Object.values(byFile)) {
    for (const p of list) {
      if (p.severity === "error") errors++;
      else if (p.severity === "warning") warnings++;
    }
  }
  if (errors === 0 && warnings === 0) return null;
  return (
    <button
      className="status-problems"
      onClick={() => setTab("dev")}
      title={t("problems.badgeHint")}
    >
      <AlertCircle size={12} color="var(--error)" /> {errors}
      <AlertTriangle size={12} color="var(--warning)" /> {warnings}
    </button>
  );
}
