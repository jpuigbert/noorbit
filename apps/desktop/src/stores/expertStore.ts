import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface Expert {
  id: string;
  name: string;
  role: string;
  systemPrompt: string;
  model: string | null;
  createdAt: number;
}

export interface ExpertInput {
  name: string;
  role: string;
  systemPrompt: string;
  model: string | null;
}

export interface SubtaskResult {
  expertId: string;
  expertName: string;
  task: string;
  result: string;
  ok: boolean;
}

export interface ProgressEvent {
  stage: "planing" | "running" | "done";
  expert: string | null;
  detail: string;
}

interface ExpertState {
  experts: Expert[];
  busy: boolean;
  teamRunning: boolean;
  teamProgress: ProgressEvent[];
  teamResult: { subtasks: SubtaskResult[]; answer: string } | null;
  soloResult: string | null;
  error: string | null;
  eventsReady: boolean;
  createRequested: boolean;

  load: () => Promise<void>;
  setupEvents: () => Promise<void>;
  create: (input: ExpertInput) => Promise<void>;
  update: (id: string, input: ExpertInput) => Promise<void>;
  remove: (id: string) => Promise<void>;
  requestCreate: () => void;
  clearCreateRequest: () => void;
  runSolo: (
    expertId: string,
    prompt: string,
    opts?: { session?: string; history?: { role: string; content: string }[] }
  ) => Promise<string | null>;
  runTeam: (goal: string, expertIds: string[]) => Promise<void>;
  clearTeam: () => void;
}

export const useExpertStore = create<ExpertState>((set, get) => ({
  experts: [],
  busy: false,
  teamRunning: false,
  teamProgress: [],
  teamResult: null,
  soloResult: null,
  error: null,
  eventsReady: false,
  createRequested: false,

  load: async () => {
    try {
      const experts = await invoke<Expert[]>("experts_list");
      set({ experts, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setupEvents: async () => {
    if (get().eventsReady) return;
    const { listen } = await import("@tauri-apps/api/event");
    listen<ProgressEvent>("experts://progress", (evt) => {
      set((s) => ({ teamProgress: [...s.teamProgress, evt.payload] }));
    });
    set({ eventsReady: true });
  },

  create: async (input) => {
    set({ busy: true, error: null });
    try {
      await invoke("expert_create", { input });
      await get().load();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: false });
    }
  },

  update: async (id, input) => {
    set({ busy: true, error: null });
    try {
      await invoke("expert_update", { id, input });
      await get().load();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: false });
    }
  },

  remove: async (id) => {
    try {
      await invoke("expert_delete", { id });
      await get().load();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  /// Demana des d'un altre panell que el d'Especialistes obri el formulari
  /// de creació en muntar-se.
  requestCreate: () => set({ createRequested: true }),
  clearCreateRequest: () => set({ createRequested: false }),

  /// Tasca individual amb un especialista (el seu rol, instruccions i model).
  /// `opts.session`/`opts.history` fan que la resposta pertanyi al xat que
  /// l'ha cridat i que conega el fil del treball (també en un «fork»).
  runSolo: async (expertId, prompt, opts) => {
    set({ busy: true, error: null, soloResult: null });
    try {
      const text = await invoke<string>("expert_run", {
        expertId,
        prompt,
        session: opts?.session ?? null,
        history: opts?.history ?? null,
      });
      set({ soloResult: text });
      return text;
    } catch (e) {
      set({ error: String(e) });
      return null;
    } finally {
      set({ busy: false });
    }
  },

  /// Treball múltiple: l'equip reparteix l'objectiu i treballa en paral·lel.
  runTeam: async (goal, expertIds) => {
    if (!goal.trim()) return;
    set({
      teamRunning: true,
      teamProgress: [],
      teamResult: null,
      error: null,
    });
    try {
      const report = await invoke<{ subtasks: SubtaskResult[]; answer: string }>(
        "experts_team_run",
        { goal, expertIds }
      );
      set({ teamResult: report });
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ teamRunning: false });
    }
  },

  clearTeam: () => set({ teamProgress: [], teamResult: null, soloResult: null }),
}));
