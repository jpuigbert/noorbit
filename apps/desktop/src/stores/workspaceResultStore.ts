import { create } from "zustand";

export interface ResultItem {
  id: string;
  kind: "file" | "message" | "asset";
  label: string;
  detail?: string;
  path?: string;
  at: number;
}

interface WorkspaceResultState {
  items: ResultItem[];
  add: (item: Omit<ResultItem, "id" | "at">) => void;
  addMany: (items: Omit<ResultItem, "id" | "at">[]) => void;
  remove: (id: string) => void;
  clear: () => void;
}

let counter = 0;
const nextId = () => `r${Date.now()}_${counter++}`;

export const useWorkspaceResultStore = create<WorkspaceResultState>((set) => ({
  items: [],

  add: (item) =>
    set((s) => ({ items: [{ ...item, id: nextId(), at: Date.now() }, ...s.items] })),

  addMany: (items) =>
    set((s) => ({
      items: [
        ...items.map((i) => ({ ...i, id: nextId(), at: Date.now() })),
        ...s.items,
      ],
    })),

  remove: (id) => set((s) => ({ items: s.items.filter((i) => i.id !== id) })),

  clear: () => set({ items: [] }),
}));
