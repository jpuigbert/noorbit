import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { expertFromMarkdown, expertToMarkdown, expertFileName, type ExpertMode } from "../utils/expertMarkdown";

export type { ExpertMode };

export interface Expert {
  id: string;
  name: string;
  role: string;
  systemPrompt: string;
  model: string | null;
  /// Com pot treballar (patró OpenCode/Qoder): "primary" = cap al xat,
  /// "subagent" = obrer (delegat/equip), "all" o absent = les dues coses.
  mode?: ExpertMode | null;
  createdAt: number;
}

export interface ExpertInput {
  name: string;
  role: string;
  systemPrompt: string;
  model: string | null;
  mode?: ExpertMode | null;
}

export interface SubtaskResult {
  expertId: string;
  expertName: string;
  task: string;
  result: string;
  ok: boolean;
}

export interface ProgressEvent {
  stage: "planing" | "running" | "done" | "cancelled";
  expert: string | null;
  detail: string;
  elapsedMs?: number;
  /// Temps RESTANT estimat de l'equip (ms). 0/absent = el backend encara no
  /// pot estimar-lo (la UI l'oculta).
  etaMs?: number;
}

interface ExpertState {
  experts: Expert[];
  busy: boolean;
  teamRunning: boolean;
  teamProgress: ProgressEvent[];
  teamResult: { subtasks: SubtaskResult[]; answer: string } | null;
  /// Instant en què va començar l'equip (per mostrar el temps transcorregut).
  teamStartedAt: number | null;
  soloResult: string | null;
  error: string | null;
  eventsReady: boolean;
  createRequested: boolean;

  load: () => Promise<void>;
  setupEvents: () => Promise<void>;
  create: (input: ExpertInput) => Promise<void>;
  update: (id: string, input: ExpertInput) => Promise<void>;
  remove: (id: string) => Promise<void>;
  /// Exporta un especialista com a fitxer .md (compartible/versionable).
  exportMarkdown: (e: Expert) => Promise<string | null>;
  /// Importa especialistes des de fitxers .md (crea o sincronitza pel nom).
  importMarkdown: () => Promise<number>;
  requestCreate: () => void;
  clearCreateRequest: () => void;
  runSolo: (
    expertId: string,
    prompt: string,
    opts?: {
      session?: string;
      history?: { role: string; content: string }[];
      /// Text extra que s'afegeix al system prompt de l'especialista (la
      /// llista dels seus companys perquè pugui delegar-hi amb «@use:»).
      systemExtra?: string;
    }
  ) => Promise<string | null>;
  runTeam: (
    goal: string,
    expertIds: string[],
    opts?: { provider?: string | null; session?: string | null }
  ) => Promise<void>;
  stopTeam: () => Promise<void>;
  clearTeam: () => void;
}

/// Registre d'esdeveniments compartit: StrictMode munta l'efecte dues
/// vegades i, sense una promesa única, dues crides concurrents passarien el
/// guard `eventsReady` (que es posa DESPRÉS del await) i registrarien dos
/// listeners → progrés duplicat. Si falla, es torna a null per reintenter.
let eventsSetup: Promise<void> | null = null;

export const useExpertStore = create<ExpertState>((set, get) => ({
  experts: [],
  busy: false,
  teamRunning: false,
  teamProgress: [],
  teamResult: null,
  teamStartedAt: null,
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
    if (!eventsSetup) {
      eventsSetup = (async () => {
        try {
          const { listen } = await import("@tauri-apps/api/event");
          // Espera el registre abans de marcar «llest»: si falla, es reintenta
          // (abans el flag es posava a cegues i el progrés dels experts es
          // quedava congelat en silenci, per sempre).
          await listen<ProgressEvent>("experts://progress", (evt) => {
            if (!evt.payload) return;
            set((s) => ({ teamProgress: [...s.teamProgress, evt.payload] }));
          });
          set({ eventsReady: true, error: null });
        } catch (e) {
          eventsSetup = null;
          set({ error: "No s'han pogut connectar els esdeveniments dels experts: " + String(e) });
        }
      })();
    }
    await eventsSetup;
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

  // Els fitxers .md fan el que ja fan els subagents de Qoder i d'OpenCode:
  // un especialista és declaratiu, es pot guardar al repo i compartir.
  exportMarkdown: async (e) => {
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const { writeTextFile } = await import("@tauri-apps/plugin-fs");
      const path = await save({
        defaultPath: expertFileName(e.name),
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (!path) return null;
      const file = path.endsWith(".md") ? path : `${path}.md`;
      await writeTextFile(file, expertToMarkdown(e));
      return file;
    } catch (err) {
      set({ error: String(err) });
      return null;
    }
  },

  importMarkdown: async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const { readTextFile } = await import("@tauri-apps/plugin-fs");
      const sel = await open({
        multiple: true,
        title: "Tria els fitxers .md d'especialistes",
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (!sel) return 0;
      const files = Array.isArray(sel) ? sel : [sel];
      let n = 0;
      for (const f of files) {
        let parsed;
        try {
          parsed = expertFromMarkdown(await readTextFile(f));
        } catch {
          continue; // un fitxer il·legible no pot aturar la resta de la remesa
        }
        if (!parsed.name.trim()) continue;
        // El nom és la clau: si ja existeix, el fitxer MANA (sincronització
        // declarativa, com els agents .md d'OpenCode); si no, es crea.
        const existing = get().experts.find(
          (x) => x.name.trim().toLowerCase() === parsed.name.trim().toLowerCase()
        );
        const input: ExpertInput = {
          name: parsed.name,
          role: parsed.role,
          systemPrompt: parsed.systemPrompt,
          model: parsed.model,
          mode: parsed.mode ?? "all",
        };
        try {
          if (existing) await invoke("expert_update", { id: existing.id, input });
          else await invoke("expert_create", { input });
          n++;
        } catch (e) {
          set({ error: String(e) });
        }
      }
      if (n > 0) await get().load();
      return n;
    } catch (err) {
      set({ error: String(err) });
      return 0;
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
        systemExtra: opts?.systemExtra ?? null,
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

  /// Treball múltiple: l'equip reparteix l'objectiu i treballa (en sèrie si
  /// el model és local, per estalviar RAM; en paral·lel si és al núvol).
  /// ABANS de llançar-ho, garantim un xat PROPI per a cada especialista
  /// («team:<id>», amb el mateix id que fa servir el backend): el flux de
  /// «ai://chunk» s'acumula a eixe xat i l'usuari veu EN DIRECTE el que
  /// escriu cada especialista, amb el mateix format del xat principal.
  runTeam: async (goal, expertIds, opts) => {
    if (!goal.trim()) return;
    const agent = (await import("./agentStore").then((m) => m.useAgentStore)).getState();
    agent.ensureChat("team:plan", "Equip · planificació");
    for (const id of expertIds) {
      const e = get().experts.find((x) => x.id === id);
      agent.ensureChat(`team:${id}`, e ? `Especialista · ${e.name}` : `Especialista ${id}`);
    }
    agent.ensureChat("team:final", "Equip · resposta final");
    // Neteja els streams anteriors: sense eixò el text de runs previs es
    // quedaria acumulat a cada xat d'especialista (el backend condueix
    // eixes sessions i ningú més fa el «finalize» del xat principal).
    agent.clearStream("team:plan");
    agent.clearStream("team:final");
    for (const id of expertIds) agent.clearStream(`team:${id}`);
    set({
      teamRunning: true,
      teamProgress: [],
      teamResult: null,
      teamStartedAt: Date.now(),
      error: null,
    });
    try {
      const report = await invoke<{ subtasks: SubtaskResult[]; answer: string }>(
        "experts_team_run",
        {
          goal,
          expertIds,
          provider: opts?.provider ?? null,
          session: opts?.session ?? null,
        }
      );
      set({ teamResult: report });
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ teamRunning: false });
    }
  },

  /// Atura l'equip en curs: el backend cancel·la la generació i retorna els
  /// resultats que ja havien acabat.
  stopTeam: async () => {
    try {
      await invoke("experts_team_stop");
    } catch (e) {
      set({ error: String(e) });
    }
  },

  clearTeam: () =>
    set({ teamProgress: [], teamResult: null, teamStartedAt: null, soloResult: null }),
}));
