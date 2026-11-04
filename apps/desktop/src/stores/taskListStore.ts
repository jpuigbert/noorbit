import { create } from "zustand";
import { useWorkspaceStore } from "./workspaceStore";

/// Checklist compartida del xat: un llistat de tasques a complir que la IA
/// principal OMPLE amb la directriu «@todo:» i MARCA com a feta amb «@done:».
/// S'injecta al context de cada torn perquè NI la IA principal NI els agents
/// es perdan: tots veuen què falta. També l'usuari pot afegir-hi tasques a mà.
///
/// Persistència: com els xats, un calaix PER PROJECTE («noorbit.tasks::<arrel>»)
/// perquè al canviar de treball no aparega la llista d'un altre projecte.

export interface TaskItem {
  id: string;
  text: string;
  done: boolean;
  /// Qui la va afegir: «ia» (directriu) o «usuari» (a mà). Només informatiu.
  by: "ia" | "usuari";
  at: number;
}

interface TaskListState {
  items: TaskItem[];
  add: (text: string, by?: "ia" | "usuari") => void;
  complete: (match: string) => void;
  toggle: (id: string) => void;
  remove: (id: string) => void;
  clear: () => void;
  /// Retorna el bloc llegible per al system prompt (buit si no hi ha tasques).
  asContext: () => string;
  setupBucket: (root: string | null) => void;
}

const LS_PREFIX = "noorbit.tasks";

function taskKey(root: string | null): string {
  return root ? `${LS_PREFIX}::${root}` : `${LS_PREFIX}::__no_project__`;
}

let currentRoot: string | null = useWorkspaceStore.getState().root;

function loadTasks(root: string | null): TaskItem[] {
  try {
    const raw = localStorage.getItem(taskKey(root));
    if (!raw) return [];
    const arr = JSON.parse(raw) as unknown;
    if (!Array.isArray(arr)) return [];
    return arr
      .filter((x): x is TaskItem => !!x && typeof (x as TaskItem).id === "string")
      .map((x) => ({ ...x, done: !!x.done }));
  } catch {
    return [];
  }
}

function persist(items: TaskItem[]) {
  try {
    localStorage.setItem(taskKey(currentRoot), JSON.stringify(items));
  } catch {
    /* sense persistència */
  }
}

function newId(): string {
  return `t-${Date.now()}-${Math.floor(Math.random() * 1e4)}`;
}

export const useTaskListStore = create<TaskListState>((set, get) => ({
  items: loadTasks(currentRoot),

  add: (text, by = "ia") => {
    const clean = text.trim();
    if (!clean) return;
    // Deduplica pel text (normalitzat): la IA pot reemetre la mateixa tasca.
    const norm = clean.toLowerCase();
    if (get().items.some((t) => t.text.trim().toLowerCase() === norm)) return;
    set((s) => ({
      items: [...s.items, { id: newId(), text: clean, done: false, by, at: Date.now() }],
    }));
    persist(get().items);
  },

  /// Marca com a feta la tasca si el text coincideix (normalitzat) o CONTÉ una
  /// paraula clau suficient. Si no n'hi ha cap de pending que coinciga, no fa res.
  complete: (match) => {
    const norm = match.trim().toLowerCase();
    if (!norm) return;
    const hit = get().items.find((t) => !t.done && t.text.trim().toLowerCase() === norm);
    if (hit) {
      set((s) => ({ items: s.items.map((t) => (t.id === hit.id ? { ...t, done: true } : t)) }));
      persist(get().items);
      return;
    }
    // Segona passada: coincidència per contenció (la IA escriu «@done: auth» i la
    // tasca deia «Implementar auth JWT»).
    const partial = get().items.find(
      (t) => !t.done && (t.text.toLowerCase().includes(norm) || norm.includes(t.text.toLowerCase()))
    );
    if (partial) {
      set((s) => ({ items: s.items.map((t) => (t.id === partial.id ? { ...t, done: true } : t)) }));
      persist(get().items);
    }
  },

  toggle: (id) => {
    set((s) => ({ items: s.items.map((t) => (t.id === id ? { ...t, done: !t.done } : t)) }));
    persist(get().items);
  },

  remove: (id) => {
    set((s) => ({ items: s.items.filter((t) => t.id !== id) }));
    persist(get().items);
  },

  clear: () => {
    set({ items: [] });
    persist([]);
  },

  asContext: () => {
    const items = get().items;
    if (items.length === 0) return "";
    const pendents = items.filter((i) => !i.done);
    const fetes = items.filter((i) => i.done);
    const lines = [
      "LLISTA DE TASQUES DEL PROJECTE (no et perdes res: completa-les i no en " +
        "deixes cap sense resoldre):",
      ...pendents.map((i) => `[ ] ${i.text}`),
      ...fetes.map((i) => `[x] ${i.text}`),
      "Quan resolguis una tasca, emet una línia «@done: <text exacte de la tasca>».",
      "Si detectes una tasca nova, afegeix-la amb «@todo: <descripció>».",
    ];
    return lines.join("\n") + "\n\n";
  },

  setupBucket: (root) => {
    if (root === currentRoot) return;
    // Desa la llista actual al calaix del projecte d'origen i carrega'n el nou.
    persist(get().items);
    currentRoot = root;
    set({ items: loadTasks(root) });
    persist(get().items);
  },
}));

// En obrir un altre projecte (o tancar-ne un), el checklist que es veu és el
// D'AQUELL projecte: ja no es barregen les tasques de treballs diferents.
useWorkspaceStore.subscribe((state, prev) => {
  if (state.root === prev.root) return;
  useTaskListStore.getState().setupBucket(state.root);
});
