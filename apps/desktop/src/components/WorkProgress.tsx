import { useEffect, useState } from "react";
import { CalendarClock, Clock, Hourglass } from "lucide-react";
import { useT } from "../i18n";
import {
  etaByProgress,
  formatDuration,
  formatFinishTime,
  progressPct,
  remainingByBudget,
} from "../utils/workTime";
import "./WorkProgress.css";

/// Barra de progrés + temps per a les feines llargues AUTOPILOTADES.
///
/// Dos modes:
///  - «tasks» (Jam de jocs): progrés = tasques fetes / total; l'ETA es calcula
///    del ritme real i es retalla al SOSTRE de durada triada (`deadlineMs`).
///  - «time» (millora de WordPress): la feina OMPLI el temps triat; progrés =
///    temps transcorregut / pressupost i l'ETA és el temps que falta.
///
/// El rellotge intern fa tick cada segon mentre `running` és cert, de manera
/// que el temps transcorregut, l'ETA i l'hora de fin estimada es mostren en viu
/// sense que l'usuari haja de fer res.
export interface WorkProgressProps {
  running: boolean;
  /// Epoch en MIL·LISEGONS de l'inici de la feina.
  startedAtMs: number;
  mode: "tasks" | "time";
  /// Mode «tasks»: unitats fetes i totals.
  doneUnits?: number;
  totalUnits?: number;
  /// Mode «time»: pressupost de temps en ms.
  budgetMs?: number;
  /// Sostre de durada (epoch ms) per al mode «tasks» (la Jam s'atura sol aquí).
  deadlineMs?: number | null;
  /// Epoch ms de finalització real (si la feina ja ha acabat).
  finishedAtMs?: number | null;
  /// Línia secundària opcional (p. ex. «3 de 10 fets» o «passada 4»).
  label?: string;
}

export default function WorkProgress({
  running,
  startedAtMs,
  mode,
  doneUnits = 0,
  totalUnits = 0,
  budgetMs = 0,
  deadlineMs = null,
  finishedAtMs = null,
  label,
}: WorkProgressProps) {
  const { t } = useT();
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!running) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [running]);

  const endMs = finishedAtMs ?? now;
  const elapsedMs = Math.max(0, endMs - startedAtMs);

  let pct: number;
  let remainingMs: number | null;
  let finishMs: number | null;

  if (mode === "time") {
    pct = finishedAtMs ? 100 : progressPct(elapsedMs, budgetMs);
    remainingMs = remainingByBudget(budgetMs, elapsedMs);
    finishMs = startedAtMs + budgetMs;
  } else {
    pct = progressPct(doneUnits, totalUnits);
    remainingMs = etaByProgress(startedAtMs, endMs, doneUnits, totalUnits);
    finishMs = remainingMs != null ? endMs + remainingMs : null;
    // Sostre de durada: la Jam no pot acabar més tard del temps triat.
    if (deadlineMs != null && deadlineMs > 0) {
      if (finishMs != null && finishMs > deadlineMs) finishMs = deadlineMs;
      if (remainingMs != null) {
        remainingMs = Math.max(0, Math.min(remainingMs, deadlineMs - endMs));
      }
    }
  }

  return (
    <div className="work-progress">
      <div className="work-bar">
        <div className="work-fill" style={{ width: `${pct}%` }} />
        <span className="work-pct">{pct}%</span>
      </div>
      <div className="work-times">
        <span className="work-chip">
          <Clock size={11} /> {t("work.elapsed")}: <b>{formatDuration(elapsedMs)}</b>
        </span>
        {remainingMs != null ? (
          <span className="work-chip">
            <Hourglass size={11} /> {t("work.remaining")}: <b>{formatDuration(remainingMs)}</b>
          </span>
        ) : (
          running && (
            <span className="work-chip muted">
              <Hourglass size={11} /> {t("work.remaining")}: {t("work.calculating")}
            </span>
          )
        )}
        {finishMs != null && finishMs > 0 && (
          <span className="work-chip">
            <CalendarClock size={11} /> {t("work.finish")}:{" "}
            <b>{formatFinishTime(finishMs, now)}</b>
          </span>
        )}
      </div>
      {label && <div className="work-label">{label}</div>}
    </div>
  );
}