import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface Skill {
  id: string;
  name: string;
  description: string;
  path: string;
  builtin?: boolean;
}

/// Bloc de codi trobat al SKILL.md (preview abans d'instal·lar).
export interface SkillCodeBlock {
  lang: string;
  code: string;
}

/// Resum d'una skill ABANS d'instal·lar-la: el que l'agent pot executar.
/// `warnings` són codis que el frontend tradueix amb t("skills.warn.<codi>").
export interface SkillPreview {
  name: string;
  description: string;
  code_blocks: SkillCodeBlock[];
  scripts: string[];
  warnings: string[];
  file_count: number;
  total_bytes: number;
}

interface SkillState {
  skills: Skill[];
  loading: boolean;
  busy: boolean;
  lastError: string | null;
  /// Missatge REAL del backend (el `lastError` només és el codi de la font).
  /// Sense això la UI només podia dir «no s'ha pogut instal·lar» i l'usuari
  /// no sabia si era el repo, la xarxa o un SKILL.md que no existeix.
  lastErrorDetail: string | null;

  load: () => Promise<void>;
  reload: () => Promise<void>;
  installGithub: (repo: string) => Promise<boolean>;
  installUrl: (url: string) => Promise<boolean>;
  installZip: (path: string) => Promise<boolean>;
  installLocal: (path: string) => Promise<boolean>;
  preview: (kind: string, source: string) => Promise<SkillPreview | null>;
  uninstall: (id: string) => Promise<void>;
  restoreBuiltins: () => Promise<boolean>;
  exportExternal: (directory: string) => Promise<number | null>;
}

/// Executa una crida i retorna `null` si va bé o el missatge de l'error.
/// Tauri rebutja amb la cadena que ha produït el comandament Rust.
async function runInstall(fn: () => Promise<void>): Promise<string | null> {
  try {
    await fn();
    return null;
  } catch (e) {
    return typeof e === "string" && e.trim() ? e : String(e);
  }
}

export const useSkillStore = create<SkillState>((set, get) => ({
  skills: [],
  loading: false,
  busy: false,
  lastError: null,
  lastErrorDetail: null,

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
    set({ busy: true, lastError: null, lastErrorDetail: null });
    const detail = await runInstall(async () => {
      await invoke("skills_install_github", { repo });
      await get().load();
    });
    if (detail !== null) set({ lastError: "github", lastErrorDetail: detail });
    set({ busy: false });
    return detail === null;
  },

  installUrl: async (url) => {
    set({ busy: true, lastError: null, lastErrorDetail: null });
    const detail = await runInstall(async () => {
      await invoke("skills_install_url", { url });
      await get().load();
    });
    if (detail !== null) set({ lastError: "url", lastErrorDetail: detail });
    set({ busy: false });
    return detail === null;
  },

  installZip: async (path) => {
    set({ busy: true, lastError: null, lastErrorDetail: null });
    const detail = await runInstall(async () => {
      await invoke("skills_install_zip", { source: path });
      await get().load();
    });
    if (detail !== null) set({ lastError: "zip", lastErrorDetail: detail });
    set({ busy: false });
    return detail === null;
  },

  installLocal: async (path) => {
    set({ busy: true, lastError: null, lastErrorDetail: null });
    const detail = await runInstall(async () => {
      await invoke("skills_install_local", { path });
      await get().load();
    });
    if (detail !== null) set({ lastError: "local", lastErrorDetail: detail });
    set({ busy: false });
    return detail === null;
  },

  preview: async (kind, source) => {
    set({ busy: true, lastError: null, lastErrorDetail: null });
    try {
      const p = await invoke<SkillPreview>("skills_preview", { kind, source });
      set({ busy: false });
      return p;
    } catch (e) {
      const detail = typeof e === "string" && e.trim() ? e : String(e);
      set({ busy: false, lastError: kind, lastErrorDetail: detail });
      return null;
    }
  },

  uninstall: async (id) => {
    await invoke("skills_uninstall", { id }).catch(() => undefined);
    await get().load();
  },

  restoreBuiltins: async () => {
    set({ busy: true, lastError: null, lastErrorDetail: null });
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
    set({ busy: true, lastError: null, lastErrorDetail: null });
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
