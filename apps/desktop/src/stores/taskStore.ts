import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";

/// Estat d'una tasca de l'agent amb pressupost de temps (Objectiu 4). El
/// backend emet «task://state» quan la tasca passa de primer pla a segon pla
/// (degradació) i quan acaba; «task://progress» arriba amb polls espaiats
/// mentre treballa en segon pla.
export interface TaskInfo {
  taskId: string;
  state: "foreground" | "background" | "done" | "error";
  elapsedMs: number;
}

interface TaskState {
  tasks: Record<string, TaskInfo>;
  setTask: (t: TaskInfo) => void;
  remove: (id: string) => void;
}

export const useTaskStore = create<TaskState>((set) => ({
  tasks: {},
  setTask: (t) =>
    set((s) => ({ tasks: { ...s.tasks, [t.taskId]: t } })),
  remove: (id) =>
    set((s) => {
      const next = { ...s.tasks };
      delete next[id];
      return { tasks: next };
    }),
}));

let registered = false;

/// Registra (una sola vegada) els escoltadors globals d'estat de tasques.
export function registerTaskListeners() {
  if (registered) return;
  registered = true;
  void listen<{ task_id: string; state: TaskInfo["state"]; elapsed_ms: number }>(
    "task://state",
    (e) => {
      const { task_id, state, elapsed_ms } = e.payload;
      useTaskStore.getState().setTask({ taskId: task_id, state, elapsedMs: elapsed_ms });
      // Les tasques acabades o fallides deixen d'aparindre a l'indicador.
      if (state === "done" || state === "error") {
        setTimeout(() => useTaskStore.getState().remove(task_id), 4000);
      }
    }
  );
  void listen<{ task_id: string; elapsed_ms: number }>("task://progress", (e) => {
    const { task_id, elapsed_ms } = e.payload;
    const cur = useTaskStore.getState().tasks[task_id];
    if (cur && cur.state === "background") {
      useTaskStore.getState().setTask({ ...cur, elapsedMs: elapsed_ms });
    }
  });
}
