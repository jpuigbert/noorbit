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
  /// Vot de l'usuari: 1 = 👍, -1 = 👎, null = sense votar. El 👍 és una
  /// PROVA (porta d'avaluació del cervell) i una fila de dataset; el 👎
  /// s'exclou de les lliçons i de l'entrenament.
  vote?: number | null;
}

export interface LearningStats {
  exchanges: number;
  lessons: number;
  pending: number;
  distilling: boolean;
  /// Respostes aprovades amb 👍 (les úniques que valen com a prova).
  votedUp: number;
  /// Respostes rebutjades amb 👎 (d'aquestes no s'aprén res).
  votedDown: number;
}

/// El paquet que `learning_soup_export` escriu a la carpeta de dades, en els
/// formats que documenta Soup (Apache-2.0): línies de conversa i de feedback,
/// `train.jsonl` ChatML i `preference.jsonl` per a DPO, més el `soup.yaml`.
/// Les xifres NO MESURADES són `null`, no 0: és la regla que fem nostre.
export interface SoupPack {
  dir: string;
  tracesPath: string;
  feedbackPath: string;
  trainPath: string;
  provenancePath: string;
  preferencePath: string;
  configPath: string;
  readmePath: string;
  rowsTrain: number;
  rowsPreference: number;
  newTraces: number;
  newFeedback: number;
  totalTraces: number;
  votesUp: number;
  votesDown: number;
  duplicates: number;
  skippedIds: number;
  /// Qualitat del dataset 0..1, amb les raons una per una a `scorecard`.
  quality: number;
  scorecard: string[];
  /// Preguntes que surten alhora a train i a validació (fuita de dades).
  leakage: number;
  baseModel: string;
  /// Pot aquest equip entrenar-hi? Es COMPROVA (`soup --version` + versió de
  /// Python + GPU), no es suposa.
  canTrainHere: boolean;
  trainCheck: string;
  note: string;
}

/// Puntuació d'un costat de la porta d'avaluació (`null` = no mesurable,
/// que NO és el mateix que zero encertades).
export interface GateScore {
  proves: number;
  encertades: number;
  nota: number;
}

/// Veredicte de la PORTA d'avaluació del cervell natiu (`ai::brain_gate`,
/// esperit del `training.eval_gate` de Soup): abans de quedar-se un
/// coneixement nou es comprova si empitjora les respostes ja aprovades.
export interface BrainVerdict {
  acceptat: boolean;
  rollback: boolean;
  peces: number;
  proves: number;
  abans: GateScore | null;
  despres: GateScore | null;
  delta: number | null;
  /// Què ha passat — o QUÈ FALTA per haver-ho pogut comprovar.
  motiu: string;
  at: number;
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

/// Els canvis viatjen com a objecte parcial (camelCase): el backend els barreja
/// amb la configuració persistent, de manera que un canvi no perd la resta.
export type LearningPatch = Partial<LearningConfig>;

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
  /// Últim paquet de Soup escrit al disc (o `null` si encara no s'ha fet).
  soup: SoupPack | null;
  /// Últim veredicte de la porta del cervell (`null` = encara no s'ha
  /// intentat cap fusió des que l'app és oberta — no vol dir «tot bé»).
  verdict: BrainVerdict | null;

  refresh: () => Promise<void>;
  setConfig: (patch: LearningPatch) => Promise<void>;
  distill: () => Promise<void>;
  removeLesson: (id: string) => Promise<void>;
  clearAll: () => Promise<void>;
  exportDataset: () => Promise<string | null>;
  /// Vota una resposta del xat (1 👍 / -1 👎 / 0 retirar-la).
  vote: (question: string, vote: number) => Promise<boolean>;
  /// Aboca el registre al format de Soup i en guarda el resum.
  exportSoup: () => Promise<SoupPack | null>;
  /// Llegeix l'últim veredicte de la porta d'avaluació del cervell.
  refreshVerdict: () => Promise<void>;
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
  soup: null,
  verdict: null,

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
    set({ busy: true, error: null });
    try {
      const status = await invoke<LearningStatus>("learning_set_config", { config: patch });
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

  // Vota per PREGUNTA (el xat no coneix l'`id` del registre). El backend
  // cerca l'últim torn que coincideix i, de retruc, actualitza el paquet de
  // Soup. Si el vot no ha lloc, torna `false` i ho diem: no pintem un botó
  // verd com si hagués funcionat.
  vote: async (question, vote) => {
    try {
      const info = await invoke<{ thumbsUp: number; thumbsDown: number }>("learning_vote", {
        vote,
        question,
      });
      set({
        error: null,
        notice:
          vote === 0
            ? "Vot retirat. El registre conserva la línia anterior: els fitxers de Soup són només-afegits i hi guanya l'últim."
            : vote > 0
              ? `Resposta aprovada (👍 ${info.thumbsUp}). Serveix com a prova i com a fila d'entrenament.`
              : `Resposta rebutjada (👎 ${info.thumbsDown}). No n'aprendrem res.`,
      });
      void get().refresh();
      return true;
    } catch (e) {
      set({ error: String(e) });
      return false;
    }
  },

  // Aboca el registre en els formats de Soup. No instal·la ni entrena res:
  // escriu dades i explica què falta (i si aquest equip hi pot entrenar).
  exportSoup: async () => {
    set({ busy: true, error: null });
    try {
      const pack = await invoke<SoupPack>("learning_soup_export");
      set({ soup: pack, busy: false, notice: `Paquet Soup escrit a ${pack.dir}` });
      return pack;
    } catch (e) {
      set({ busy: false, error: String(e) });
      return null;
    }
  },

  // La porta d'avaluació només informa del que ha fet AQUESTA sessió: si
  // encara no s'ha fusionat cap coneixement, `verdict` continua sent `null`.
  refreshVerdict: async () => {
    try {
      const verdict = await invoke<BrainVerdict | null>("learning_brain_verdict");
      set({ verdict: verdict ?? null });
    } catch {
      /* sense veredicte: es diu que no n'hi ha, no que tot vagi bé */
    }
  },

  // El backend avisa quan l'aprenentatge canvia (lliçons noves, registre
  // nou…): la interfície es refresca sola.
  watch: async () => {
    const un = await listen("learning://changed", () => {
      void get().refresh();
      void get().refreshVerdict();
    });
    return un;
  },
}));
