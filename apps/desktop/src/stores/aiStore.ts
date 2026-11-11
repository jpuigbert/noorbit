import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface OllamaModel {
  name: string;
  size: number;
  modified_at: string;
}

export interface SuggestedModel {
  name: string;
  description: string;
  /// Mida de la descàrrega en bytes (aproximada al catàleg).
  size?: number | null;
}

/// Model disponible al núvol d'Ollama (ollama.com).
export interface CloudModel {
  name: string;
  description: string;
  /// Mida de la descàrrega en bytes, si la font la diu (null: no es mostra).
  size?: number | null;
  /// URL de descàrrega directe (CivitAI): es desa al rodet de CPU amb
  /// «downloadCpuModel». Absent a les fonts natives d'Ollama.
  url?: string | null;
}

export interface PullStatus {
  model: string;
  message: string;
}

/// Llista base del núvol: es mostra mentre no s'ha cercat res i com a
/// replà si la xarxa falla. Així sempre hi ha models triables (p. ex. qwen),
/// encara que Ollama no estigui instal·lat i sense generar cap token.
export const CLOUD_FALLBACK: CloudModel[] = [
  { name: "qwen3", description: "Família Qwen3 de models raonables i multilingües." },
  { name: "qwen2.5-coder", description: "Codi — equilibri qualitat/velocitat." },
  { name: "llama3.2", description: "General — ràpid i multilingüe, versions 1B/3B." },
  { name: "deepseek-r1", description: "Raonament obert, enfocament pas a pas." },
  { name: "mistral", description: "General — bo seguint instruccions." },
  { name: "gemma3", description: "Models lleugers de Google per a ús general." },
  { name: "phi4", description: "Model compact de Microsoft amb bona qualitat." },
];

/// URL de l'API d'Ollama Cloud (compatible amb OpenAI). Els models s'executen
/// al núvol i NO cal baixar-los: només cal un token gratuït d'ollama.com.
export const OLLAMA_CLOUD_URL = "https://ollama.com/v1";

/// Models alliberats i gratuïts disponibles al núvol d'Ollama (ús remot).
/// Són un subconjunt estable; la llista completa és a ollama.com/search?c=cloud.
export const OLLAMA_CLOUD_MODELS: CloudModel[] = [
  { name: "gpt-oss:20b", description: "Raonament obert d'OpenAI, lleuger i ràpid." },
  { name: "qwen3:32b", description: "Razonament potent i multilingüe." },
  { name: "deepseek-r1:32b", description: "Raonament pas a pas per a problemes complexos." },
  { name: "llama-3.3:70b", description: "General de gran escala." },
  { name: "gemma-3:27b", description: "Multimodal lleuger de Google." },
  { name: "qwen3-coder:30b", description: "Especialista en codi." },
];

/// Un model que Ollama té carregat a la memòria (RAM/VRAM) ara mateix.
export interface LoadedModel {
  name: string;
  size: number;
  sizeVram: number;
}

/// Estat del rodet d'imatges en CPU (stable-diffusion.cpp) que retorna
/// «image_cpu_info»: binari instal·lat, carpeta de models i model disponible.
export interface ImageCpuInfo {
  installed: boolean;
  modelsDir: string;
  model: string | null;
}

interface AIState {
  ollamaRunning: boolean;
  ollamaInstalled: boolean;
  comfyRunning: boolean;
  comfyInstalled: boolean;
  /// Rodet d'imatges en CPU (stable-diffusion.cpp): funciona fins i tot en
  /// Mac Intel, on ComfyUI no està publicat. idem info del binari + model.
  imageCpu: ImageCpuInfo | null;
  /// Progrés de la baixada del model inicial del rodet de CPU.
  imageCpuProgress: string | null;
  models: OllamaModel[];
  /// Models carregats ARA a la RAM (per poder-los alliberar).
  loaded: LoadedModel[];
  /// Quant de temps Ollama manté els models carregats (buit = per defecte).
  keepAlive: string;
  suggested: SuggestedModel[];
  cloud: CloudModel[];
  cloudLoading: boolean;
  cloudError: string | null;
  /// Models trobats a INTERNET (Hugging Face) quan NO són a la biblioteca
  /// d'Ollama. Els noms «hf.co/…» es baixen en natiu amb pullModel.
  internet: CloudModel[];
  internetLoading: boolean;
  internetError: string | null;
  selectedModel: string | null;
  pulling: Record<string, string>; // model -> missatge de progrés
  modelsDir: string;
  volumes: string[];
  /// Progrés de la migració de models al disc extern (nu = no s'està movent).
  migrating: string | null;
  /// Avís informatiu de l'última operació (p. ex. tria automàtica de model).
  lastMessage: string | null;

  checkOllama: () => Promise<void>;
  checkInstalled: () => Promise<void>;
  checkComfy: () => Promise<void>;
  loadModels: () => Promise<void>;
  /// Llegeix els models carregats a la RAM i el temps de permanència configurat.
  loadLoaded: () => Promise<void>;
  /// Allibera de la RAM un model concret (o tots els carregats).
  unloadModels: (model?: string | null) => Promise<number>;
  /// Canvia quant de temps Ollama manté els models a la memòria.
  setKeepAlive: (value: string) => Promise<void>;
  refreshProviders: () => Promise<void>;
  searchCloud: (query: string) => Promise<void>;
  searchInternet: (query: string) => Promise<void>;
  /// Origen de la cerca «a internet»: Hugging Face (LLM GGUF) o CivitAI
  /// (models d'imatge per al rodet de CPU).
  internetSource: "hf" | "civitai";
  setInternetSource: (s: "hf" | "civitai") => void;
  /// Instal·la una ALTRA IA local apart d'Ollama (LM Studio, GPT4All, Jan,
  /// llama.cpp); NoOrbit la detecta sols un cop instal·lada.
  installLocalAi: (which: string) => Promise<void>;
  installOllama: () => Promise<void>;
  startOllama: () => Promise<void>;
  installComfy: () => Promise<void>;
  startComfy: () => Promise<void>;
  /// Rodet d'imatges en CPU: llegeix l'estat, n'instal·la el binari i baixa
  /// el model inicial (per defecte SD 1.5, ≈2 GB) amb progrés a la UI.
  loadImageCpu: () => Promise<void>;
  installImageCpu: () => Promise<void>;
  downloadCpuModel: (url?: string, name?: string) => Promise<void>;
  loadModelsDir: () => Promise<void>;
  setModelsDir: (path: string | null) => Promise<void>;
  migrateModels: () => Promise<void>;
  selectModel: (name: string | null) => Promise<void>;
  pullModel: (name: string) => Promise<void>;
  deleteModel: (name: string) => Promise<void>;
}

/// Model que càpiga còmodament: un model Q4 necessita ~1,4× el seu pes en RAM
/// (pes + KV cache + marge del sistema). Es considera còmode si ocupa com a
/// màxim el 60 % de la RAM total; així evitem swap i escalfament excessiu.
/// Entre els còmodes guanya el més gran (més capacitat); si no n'hi cap de
/// còmode, retorna null perquè el cridat decideixi.
export function pickBestFittingModel(
  models: OllamaModel[],
  totalRamBytes: number
): string | null {
  if (models.length === 0) return null;
  if (totalRamBytes <= 0) return null;
  const budget = totalRamBytes * 0.6;
  const fits = models.filter((m) => m.size > 0 && m.size <= budget);
  if (fits.length === 0) return null;
  // El més gran que hi càpiga = el més capable sense escalfar.
  const best = fits.reduce((a, b) => (b.size > a.size ? b : a));
  return best.name;
}

export const useAIStore = create<AIState>((set, get) => ({
  ollamaRunning: false,
  ollamaInstalled: false,
  internetSource: "hf",
  comfyRunning: false,
  comfyInstalled: false,
  imageCpu: null,
  imageCpuProgress: null,
  models: [],
  loaded: [],
  keepAlive: "",
  suggested: [],
  cloud: CLOUD_FALLBACK,
  cloudLoading: false,
  cloudError: null,
  internet: [],
  internetLoading: false,
  internetError: null,
  selectedModel: null,
  pulling: {},
  modelsDir: "",
  volumes: [],
  migrating: null,
  lastMessage: null,

  checkOllama: async () => {
    try {
      const running = await invoke<boolean>("ollama_is_running");
      set({ ollamaRunning: running });
    } catch {
      set({ ollamaRunning: false });
    }
  },

  /// Detecta si Ollama és al sistema (encara que no estigui en marxa).
  checkInstalled: async () => {
    try {
      const installed = await invoke<boolean>("ollama_installed");
      set({ ollamaInstalled: installed });
    } catch {
      set({ ollamaInstalled: false });
    }
  },

  /// Estat de ComfyUI (generació d'imatge): en marxa i/o instal·lat.
  checkComfy: async () => {
    try {
      const [running, installed] = await Promise.all([
        invoke<boolean>("comfyui_running"),
        invoke<boolean>("comfyui_installed"),
      ]);
      set({ comfyRunning: running, comfyInstalled: installed });
    } catch {
      set({ comfyRunning: false, comfyInstalled: false });
    }
  },

  loadModels: async () => {
    try {
      const models = await invoke<OllamaModel[]>("ollama_list_models");
      set({ models });
      const current = get().selectedModel;
      // Restaura la triada desada localment, si encara existeix.
      const saved = (() => {
        try {
          return localStorage.getItem("noorbit.model");
        } catch {
          return null;
        }
      })();
      const wanted =
        current ?? (saved && models.some((m) => m.name === saved) ? saved : null);
      if (wanted && models.some((m) => m.name === wanted)) {
        await get().selectModel(wanted);
      } else if (models.length > 0) {
        // Per defecte: el millor model LOCAL que càpiga còmodament a la RAM
        // (no saturar la memòria = no escalfar ni intercanviar amb disc).
        let ram = 0;
        try {
          ram = await invoke<number>("system_total_memory");
        } catch {
          /* sense dades de memòria */
        }
        const fit = pickBestFittingModel(models, ram);
        await get().selectModel(fit ?? models[0].name);
        // Amb poca RAM (p. ex. 8 GB) i cap model còmode, suggerim un de lleuger.
        if (ram > 0 && ram <= 10_000_000_000 && !fit && models.length > 0) {
          set({
            lastMessage:
              "Tens poca RAM per als models instal·lats; Instal·la'n un de lleuger " +
              "com «llama3.2:3b» des del gestor de models.",
          });
        }
      }
    } catch {
      set({ models: [] });
    }
  },

  /// Llegeix quins models té Ollama carregats a la RAM i el temps de
  /// permanència configurat. Amb això l'usuari veu quanta memòria ocupa i la
  /// pot alliberar d'un clic.
  loadLoaded: async () => {
    try {
      const [loaded, keepAlive] = await Promise.all([
        invoke<LoadedModel[]>("ollama_loaded_models"),
        invoke<string>("ollama_get_keep_alive"),
      ]);
      set({ loaded, keepAlive: keepAlive ?? "" });
    } catch {
      set({ loaded: [] });
    }
  },

  unloadModels: async (model) => {
    try {
      const n = await invoke<number>("ollama_unload_models", { model: model ?? null });
      await get().loadLoaded();
      return n;
    } catch {
      return 0;
    }
  },

  setKeepAlive: async (value) => {
    set({ keepAlive: value });
    try {
      await invoke("ollama_set_keep_alive", { value: value || null });
    } catch {
      /* sense persistència */
    }
  },

  refreshProviders: async () => {
    try {
      const suggested = await invoke<SuggestedModel[]>("ollama_suggested_models");
      set({ suggested });
    } catch {
      /* ignore */
    }
  },

  /// Cerca models al NÚVOL d'ollama.com. No cal Ollama en marxa ni genera
  /// tokens: només descarrega la llista. Si falla la xarxa, usa la llista base.
  searchCloud: async (query) => {
    set({ cloudLoading: true, cloudError: null });
    try {
      const cloud = await invoke<CloudModel[]>("ollama_search_cloud", {
        query: query ?? "",
      });
      set({
        cloud: cloud.length > 0 ? cloud : CLOUD_FALLBACK,
        cloudLoading: false,
      });
    } catch (e) {
      set({
        cloud: CLOUD_FALLBACK,
        cloudLoading: false,
        cloudError: String(e),
      });
    }
  },

  /// Instal·la Ollama (obre el mètode adequat al sistema operatiu).
  installOllama: async () => {
    try {
      await invoke("install_ollama");
    } catch {
      /* ignore */
    }
  },

  /// Si un model NO és a la biblioteca d'Ollama, el cerquem a INTERNET
  /// (Hugging Face). No cal Ollama en marxa: només consulta l'API pública.
  /// Els resultats tenen noms «hf.co/…» que pullModel baixa en natiu.
  searchInternet: async (query) => {
    const source = get().internetSource;
    set({ internetLoading: true, internetError: null });
    try {
      // Hugging Face: GGUF que Ollama baixa natiu («hf.co/…»). CivitAI:
      // models d'imatge amb URL directa cap al rodet de CPU.
      const internet = await invoke<CloudModel[]>(
        source === "hf" ? "ollama_search_huggingface" : "civitai_search",
        { query: query ?? "" }
      );
      set({ internet, internetLoading: false });
    } catch (e) {
      set({ internet: [], internetLoading: false, internetError: String(e) });
    }
  },

  setInternetSource: (s) => {
    set({ internetSource: s, internet: [] });
    // En canviar a CivitAI fem una cerca d'exemple perquè la llista no qüedi
    // buida; Hugging Face espera la consulta de l'usuari.
    if (s === "civitai") void get().searchInternet("stable diffusion");
  },

  installLocalAi: async (which) => {
    try {
      await invoke<string>("install_local_ai", { which });
    } catch (e) {
      set({ lastMessage: String(e) });
    }
  },

  /// Instal·la ComfyUI (Terminal amb Homebrew o pàgina oficial).
  installComfy: async () => {
    try {
      await invoke("install_comfyui");
    } catch {
      /* ignore */
    }
  },

  /// Arrenca el servidor ComfyUI i torna a comprobar l'estat.
  startComfy: async () => {
    try {
      await invoke("start_comfyui");
    } catch {
      /* ignore */
    }
    // El servidor triga uns segons a escoltar el port 8188.
    await new Promise((r) => setTimeout(r, 2000));
    await get().checkComfy();
  },

  /// Estat del rodet d'imatges en CPU (stable-diffusion.cpp).
  loadImageCpu: async () => {
    try {
      const info = await invoke<ImageCpuInfo>("image_cpu_info");
      set({ imageCpu: info });
    } catch {
      /* ignore */
    }
  },

  /// Instal·la el binari del rodet de CPU (Homebrew / pàgina de descàrrega).
  installImageCpu: async () => {
    try {
      await invoke<string>("install_image_cpu");
    } catch {
      /* ignore */
    }
    // Homebrew triga: tornem a consultar d'aquí uns segons per refrescar l'estat.
    await new Promise((r) => setTimeout(r, 5000));
    await get().loadImageCpu();
  },

  /// Baixa el model inicial del rodet de CPU amb progrés a la UI.
  /// Per defecte és el SD 1.5 (≈2 GB), suficient per començar a generar.
  downloadCpuModel: async (url, name) => {
    if (get().imageCpuProgress) return;
    set({ imageCpuProgress: "Iniciant la baixada del model…" });
    const { listen } = await import("@tauri-apps/api/event");
    const unlisten = listen<{ message: string }>("ai://sd-model", (evt) => {
      set({ imageCpuProgress: evt.payload.message });
    });
    try {
      await invoke("image_cpu_download_model", {
        url: url ?? null,
        name: name ?? null,
      });
      await get().loadImageCpu();
    } finally {
      (await unlisten)();
      set({ imageCpuProgress: null });
    }
  },

  /// Arrenca el servei local (aplicant la carpeta de models configurada,
  /// si cal, reinicia Ollama) i torna a comprobar l'estat.
  startOllama: async () => {
    try {
      const msg = await invoke<string>("start_ollama");
      set({ lastMessage: msg });
    } catch {
      /* ignore */
    }
    // Dona un moment al servei per escoltar abans de tornar a comprovar.
    await new Promise((r) => setTimeout(r, 1500));
    await get().checkOllama();
    await get().loadModels();
  },

  /// Llegeix la carpeta de models desada i els volums externs muntats.
  loadModelsDir: async () => {
    try {
      const dir = await invoke<string>("ollama_get_models_dir");
      set({ modelsDir: dir });
    } catch {
      /* ignore */
    }
    try {
      const volumes = await invoke<string[]>("list_external_volumes");
      set({ volumes });
    } catch {
      set({ volumes: [] });
    }
  },

  /// Desa la carpeta on viuran els models (p. ex. un USB extern) i l'aplica:
  /// el backend reinicia Ollama amb OLLAMA_MODELS perquè les descàrregues
  /// següents vagin al disc extern. El missatge de tornada ho confirma.
  setModelsDir: async (path) => {
    const dir = path ?? "";
    try {
      const msg = await invoke<string>("ollama_set_models_dir", { path: dir });
      set({ modelsDir: dir, lastMessage: msg });
    } catch {
      set({ modelsDir: dir });
    }
  },

  /// Mou els models ja baixats al disc intern cap al disc extern triat,
  /// amb progrés per l'event «ai://migrate».
  migrateModels: async () => {
    if (get().migrating) return;
    set({ migrating: "Preparant la migració…" });
    const { listen } = await import("@tauri-apps/api/event");
    const unlisten = listen<{ message: string }>("ai://migrate", (evt) =>
      set({ migrating: evt.payload.message })
    );
    try {
      const msg = await invoke<string>("ollama_migrate_models");
      set({ migrating: null, lastMessage: msg });
      await get().loadModels();
    } catch (e) {
      set({ migrating: null, lastMessage: String(e) });
    } finally {
      (await unlisten)();
    }
  },

  /// Triat el model i en dona compte al backend: tots els xats i agents
  /// (inclòs el control de l'ordinador) l'usaran a partir d'ara.
  selectModel: async (name) => {
    set({ selectedModel: name });
    try {
      localStorage.setItem("noorbit.model", name ?? "");
    } catch {
      /* sense persistència */
    }
    await invoke("ollama_set_model", { name }).catch(() => undefined);
  },

  pullModel: async (name) => {
    set((s) => ({ pulling: { ...s.pulling, [name]: "Iniciant descàrrega…" } }));
    const { listen } = await import("@tauri-apps/api/event");
    const unlisten = listen<{ model: string; message: string }>(
      "ai://pull",
      (evt) => {
        set((s) => ({
          pulling: { ...s.pulling, [evt.payload.model]: evt.payload.message },
        }));
      }
    );
    try {
      await invoke("ollama_pull_model", { name });
      await get().loadModels();
      // Un model acabat de baixar es tria automàticament per treballar-hi.
      await get().selectModel(name);
    } finally {
      (await unlisten)();
      set((s) => {
        const pulling = { ...s.pulling };
        delete pulling[name];
        return { pulling };
      });
    }
  },

  deleteModel: async (name) => {
    await invoke("ollama_delete_model", { name });
    await get().loadModels();
  },
}));
