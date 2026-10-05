import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Eye } from "lucide-react";
import { useT } from "../i18n";
import { useTaskStore } from "../stores/taskStore";

/// Indicador compacte (barra d'estat) de les tasques degradades a segon pla
/// (Objectiu 4). Només apareix si hi ha alguna tasca en «background». El botó
/// «Mostra» demana al backend que la torne a primer pla.
export default function BackgroundTaskBadge() {
  const { t } = useT();
  // El selector ha de retornar SEMPRE la mateixa referència: si filtrem dins
  // del selector, cada lectura crearia un llistat nou i React es repiaria
  // sense fi («Maximum update depth exceeded»). Filtrem amb useMemo.
  const tasksMap = useTaskStore((s) => s.tasks);
  const tasks = useMemo(
    () => Object.values(tasksMap).filter((tk) => tk.state === "background"),
    [tasksMap]
  );
  // Tic discret per refrescar el temps sense re-render per cada event.
  const [, setTick] = useState(0);
  useEffect(() => {
    if (tasks.length === 0) return;
    const id = setInterval(() => setTick((tk) => tk + 1), 5000);
    return () => clearInterval(id);
  }, [tasks.length]);

  if (tasks.length === 0) return null;
  const first = tasks[0];

  return (
    <span className="status-item bg-task-badge" title={t("status.bgTasksTitle")}>
      <span className="bg-task-dot" />
      {t("status.bgTasksCount", { count: tasks.length })}
      <button
        className="btn sm ghost"
        onClick={() => void invoke("task_bring_to_foreground", { taskId: first.taskId })}
      >
        <Eye size={11} /> {t("status.bgTasksShow")}
      </button>
    </span>
  );
}
