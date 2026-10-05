import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface Skill {
  id: string;
  name: string;
  description: string;
  path: string;
  builtin?: boolean;
}

interface SkillState {
  skills: Skill[];
  loading: boolean;
  busy: boolean;
  lastError: string | null;

  load: () => Promise<void>;
  reload: () => Promise<void>;
  installGithub: (repo: string) => Promise<boolean>;
  installUrl: (url: string) => Promise<boolean>;
  installZip: (path: string) => Promise<boolean>;
  installLocal: (path: string) => Promise<boolean>;
  uninstall: (id: string) => Promise<void>;
  restoreBuiltins: () => Promise<boolean>;
  exportExternal: (directory: string) => Promise<number | null>;
}

async function runInstall(fn: () => Promise<void>): Promise<boolean> {
  try {
    await fn();
    return true;
  } catch {
    return false;
  }
}

export const useSkillStore = create<SkillState>((set, get) => ({
  skills: [],
  loading: false,
  busy: false,
  lastError: null,

  load: async () => {
    set({ loading: true });
    try {
      const skills = await invoke<Skill[]>("list_skills");
      set({ skills, loading: false });
    } catch {
      set({ skills: [], loading: false });
    }
  },

  reload: async () => {
    try {
      const skills = await invoke<Skill[]>("reload_skills");
      set({ skills });
    } catch {
      /* ignore */
    }
  },

  installGithub: async (repo) => {
    set({ busy: true, lastError: null });
    const ok = await runInstall(async () => {
      await invoke("skills_install_github", { repo });
      await get().load();
    });
    if (!ok) set({ lastError: "github" });
    set({ busy: false });
    return ok;
  },

  installUrl: async (url) => {
    set({ busy: true, lastError: null });
    const ok = await runInstall(async () => {
      await invoke("skills_install_url", { url });
      await get().load();
    });
    if (!ok) set({ lastError: "url" });
    set({ busy: false });
    return ok;
  },

  installZip: async (path) => {
    set({ busy: true, lastError: null });
    const ok = await runInstall(async () => {
      await invoke("skills_install_zip", { source: path });
      await get().load();
    });
    if (!ok) set({ lastError: "zip" });
    set({ busy: false });
    return ok;
  },

  installLocal: async (path) => {
    set({ busy: true, lastError: null });
    const ok = await runInstall(async () => {
      await invoke("skills_install_local", { path });
      await get().load();
    });
    if (!ok) set({ lastError: "local" });
    set({ busy: false });
    return ok;
  },

  uninstall: async (id) => {
    await invoke("skills_uninstall", { id }).catch(() => undefined);
    await get().load();
  },

  restoreBuiltins: async () => {
    set({ busy: true, lastError: null });
    let ok = false;
    try {
      const skills = await invoke<Skill[]>("skills_restore_builtin");
      set({ skills });
      ok = true;
    } catch {
      set({ lastError: "restore" });
    }
    set({ busy: false });
    return ok;
  },

  exportExternal: async (directory) => {
    set({ busy: true, lastError: null });
    try {
      const n = await invoke<number>("skills_export_external", { directory });
      set({ busy: false });
      return n;
    } catch {
      set({ busy: false, lastError: "export" });
      return null;
    }
  },
}));
