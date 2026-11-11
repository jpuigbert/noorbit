// Estat de l'APRENENATGE LOCAL: el registre de converses i les lliçons que
// NoOrbit ha après de les IAs. Tot viu al backend (fitxer «learning.json» a la
// carpeta de dades) i la interfície només en mostra l'estat.
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface Lesson {
  id: string;
  text: string;
  tags: string[];
  source: string;
  hits: number;
  at: number;
}

export interface Exchange {
  id: string;
  question: string;
  answer: string;
  provider: string;
  model: string;
  at: number;
}

export interface LearningStats {
  exchanges: number;
  lessons: number;
  pending: number;
  distilling: boolean;
}

export interface LearningStatus {
  enabled: boolean;
  autoDistill: boolean;
  inject: boolean;
  model: string;
  maxLessons: number;
  stats: LearningStats;
  datasetPath: string;
  localModel: boolean;
}

export interface LearningConfig {
  enabled: boolean;
  autoDistill: boolean;
  inject: boolean;
  model: string;
  maxLessons: number;
}

interface LearningSnapshot {
  lessons: Lesson[];
  exchanges: Exchange[];
}

interface LearningState {
  status: LearningStatus | null;
  lessons: Lesson[];
  exchanges: Exchange[];
  loading: boolean;
  busy: boolean;
  error: string | null;
  notice: string | null;

  refresh: () => Promise<void>;
  setConfig: (patch: Partial<LearningConfig>) => Promise<void>;
  distill: () => Promise<void>;
  removeLesson: (id: string) => Promise<void>;
  clearAll: () => Promise<void>;
  exportDataset: () => Promise<string | null>;
  watch: () => Promise<() => void>;
}

export const useLearningStore = create<LearningState>((set, get) => ({
  status: null,
  lessons: [],
  exchanges: [],
  loading: false,
  busy: false,
  error: null,
  notice: null,

  refresh: async () => {
    try {
      const status = await invoke<LearningStatus>("learning_status");
      const snap = await invoke<LearningSnapshot>("learning_list");
      set({ status, lessons: snap.lessons, exchanges: snap.exchanges, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setConfig: async (patch) => {
    const cur = get().status;
    if (!cur) return;
    const next: LearningConfig = {
      enabled: patch.enabled ?? cur.enabled,
      autoDistill: patch.autoDistill ?? cur.autoDistill,
      inject: patch.inject ?? cur.inject,
      model: patch.model ?? cur.model,
      maxLessons: patch.maxLessons ?? cur.maxLessons,
    };
    set({ busy: true, error: null });
    try {
      const status = await invoke<LearningStatus>("learning_set_config", { config: next });
      set({ status, busy: false });
    } catch (e) {
      set({ busy: false, error: String(e) });
    }
  },

  distill: async () => {
    set({ busy: true, error: null, notice: null });
    try {
      const status = await invoke<LearningStatus>("learning_distill");
      const snap = await invoke<LearningSnapshot>("learning_list");
      const noves = snap.lessons.length - get().lessons.length;
      set({
        status,
        lessons: snap.lessons,
        exchanges: snap.exchanges,
        busy: false,
        notice:
          noves > 0
            ? `S'han après ${noves} lliçó${noves === 1 ? "" : "ns"} nova${noves === 1 ? "" : "s"}.`
            : "No hi ha res nou per aprendre encara.",
      });
    } catch (e) {
      set({ busy: false, error: String(e) });
    }
  },

  removeLesson: async (id) => {
    try {
      const status = await invoke<LearningStatus>("learning_delete_lesson", { id });
      set({ status, lessons: get().lessons.filter((l) => l.id !== id) });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  clearAll: async () => {
    try {
      const status = await invoke<LearningStatus>("learning_clear");
      set({ status, lessons: [], exchanges: [], notice: "Registre i lliçons esborrats." });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  exportDataset: async () => {
    try {
      const path = await invoke<string>("learning_export");
      set({ notice: `Dataset exportat a ${path}` });
      return path;
    } catch (e) {
      set({ error: String(e) });
      return null;
    }
  },

  // El backend avisa quan l'aprenentatge canvia (lliçons noves, registre
  // nou…): la interfície es refresca sola.
  watch: async () => {
    const un = await listen("learning://changed", () => {
      void get().refresh();
    });
    return un;
  },
}));
