import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface Permissions {
  enabled: boolean;
  confirmEach: boolean;
  allowlist: string[];
}

export interface HistoryEntry {
  command: string;
  ok: boolean;
  exitCode: number;
  excerpt: string;
  at: number;
}

export interface ExecReport {
  command: string;
  ok: boolean;
  exitCode: number;
  stdout: string;
  stderr: string;
  timedOut: boolean;
}

export interface PendingConfirm {
  id: string;
  command: string;
}

export interface AgentStep {
  kind: "run" | "done";
  command?: string;
  reason?: string;
  answer?: string;
}

interface ComputerState {
  perms: Permissions;
  history: HistoryEntry[];
  lastOutput: ExecReport | null;
  pending: PendingConfirm | null;
  agentSteps: AgentStep[];
  agentRunning: boolean;
  busy: boolean;
  error: string | null;
  eventsReady: boolean;

  load: () => Promise<void>;
  setupEvents: () => Promise<void>;
  setPerms: (enabled?: boolean, confirmEach?: boolean) => Promise<void>;
  removePattern: (pattern: string) => Promise<void>;
  clearHistory: () => Promise<void>;
  runCommand: (command: string) => Promise<void>;
  resolveConfirm: (approved: boolean, remember: boolean) => Promise<void>;
  runAgent: (goal: string) => Promise<void>;
}

export const useComputerStore = create<ComputerState>((set, get) => ({
  perms: { enabled: false, confirmEach: true, allowlist: [] },
  history: [],
  lastOutput: null,
  pending: null,
  agentSteps: [],
  agentRunning: false,
  busy: false,
  error: null,
  eventsReady: false,

  load: async () => {
    try {
      const perms = await invoke<Permissions>("computer_permissions");
      const history = await invoke<HistoryEntry[]>("computer_history");
      set({ perms, history, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setupEvents: async () => {
    if (get().eventsReady) return;
    const { listen } = await import("@tauri-apps/api/event");
    // Sol·licitud de confirmació (per una comanda directa o de l'agent).
    listen<{ id: string; command: string }>("computer://confirm", (evt) => {
      set({ pending: { id: evt.payload.id, command: evt.payload.command } });
    });
    // Passos del bucle de l'agent.
    listen<AgentStep>("computer://agent", (evt) => {
      set((s) => ({ agentSteps: [...s.agentSteps, evt.payload] }));
    });
    // La comanda ja s'ha executat: refresca l'historial.
    listen("computer://executed", () => {
      get().load();
    });
    set({ eventsReady: true });
  },

  setPerms: async (enabled, confirmEach) => {
    try {
      const perms = await invoke<Permissions>("computer_set_permissions", {
        enabled: enabled ?? null,
        confirmEach: confirmEach ?? null,
      });
      set({ perms, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  removePattern: async (pattern) => {
    try {
      await invoke("computer_remove_pattern", { pattern });
      await get().load();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  clearHistory: async () => {
    try {
      await invoke("computer_clear_history");
      set({ history: [], lastOutput: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  runCommand: async (command) => {
    if (!command.trim()) return;
    set({ busy: true, error: null });
    try {
      const rep = await invoke<ExecReport>("computer_execute", { command });
      set({ lastOutput: rep });
      await get().load();
    } catch (e) {
      set({ error: String(e), lastOutput: null });
    } finally {
      set({ busy: false });
    }
  },

  resolveConfirm: async (approved, remember) => {
    const p = get().pending;
    if (!p) return;
    set({ pending: null });
    try {
      await invoke("computer_confirm", { id: p.id, approved, remember });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  runAgent: async (goal) => {
    if (!goal.trim()) return;
    set({ agentRunning: true, agentSteps: [], error: null });
    try {
      await invoke<string>("computer_agent_run", { goal });
      await get().load();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ agentRunning: false });
    }
  },
}));
