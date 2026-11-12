import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { OLLAMA_CLOUD_URL } from "./aiStore";
import { toastError } from "./toastStore";

export type AuthScheme = "bearer" | "header" | "none";
/// Format d'API del proveïdor: compatible OpenAI o Anthropic (Claude).
export type ProviderKind = "openai" | "anthropic";

export interface ProviderPublic {
  id: string;
  name: string;
  base_url: string;
  model: string;
  kind: string;
  auth: AuthScheme;
  header_name: string;
  has_token: boolean;
  token_masked: string;
  enabled: boolean;
}

export interface ProviderInput {
  id?: string;
  name: string;
  baseUrl: string;
  model: string;
  kind?: ProviderKind;
  auth: AuthScheme;
  headerName?: string;
  token?: string;
  enabled?: boolean;
}

interface ProviderState {
  providers: ProviderPublic[];
  active: string;
  loading: boolean;
  testingId: string | null;
  lastTest: Record<string, { ok: boolean; msg: string }>;
  /// Darrer diagnòstic del formulari (botó «Comprova»).
  checking: boolean;
  lastCheck: EndpointCheck | null;

  load: () => Promise<void>;
  add: (p: ProviderInput) => Promise<void>;
  update: (patch: ProviderInput & { id: string }) => Promise<void>;
  /// ACTIVACIÓ MANUAL d'un proveïdor (p. ex. «DeepInfer (servidor GPU)», que
  /// ve desactivat perquè no té sentit fins que tens el servidor en marxa —
  /// però quan el tens, l'activa tu amb un clic, sense cap token).
  setEnabled: (id: string, enabled: boolean) => Promise<void>;
  remove: (id: string) => Promise<void>;
  test: (id: string) => Promise<void>;
  select: (id: string) => Promise<void>;
  /** Diagnòstic prèviament a desar: verifica subre servidor, clau, URL i model. */
  checkEndpoint: (
    input: Pick<ProviderInput, "baseUrl" | "model" | "token" | "kind">
  ) => Promise<EndpointCheck | null>;
}

/// Diu si un proveïdor és el NÚVOL d'Ollama (models gratuïts a ollama.com).
/// Es reconeix per id o per URL base: l'usuari pot haver-lo afegit a mà des
/// del gestor de proveïdors, on rep un id automàtic («prov_…»).
export function isOllamaCloudProvider(p?: ProviderPublic | null): boolean {
  if (!p) return false;
  const base = (u: string) => u.trim().replace(/\/+$/, "");
  return p.id === "ollama-cloud" || base(p.base_url) === base(OLLAMA_CLOUD_URL);
}

/// Diagnòstic pas a pas d'un servei d'IA (comanda `ai_endpoint_check`):
/// ok=false sempre ve amb EL MOTIU — servidor aturat, clau dolenta, URL
/// errònia o model inexistent — i la llista de models que oferece el servei.
export interface EndpointCheck {
  ok: boolean;
  stage: "connexio" | "auth" | "url" | "model" | "llest";
  title: string;
  detail: string;
  models: string[];
}

export const useProviderStore = create<ProviderState>((set, get) => ({
  providers: [],
  active: "ollama",
  loading: false,
  testingId: null,
  lastTest: {},
  checking: false,
  lastCheck: null,

  load: async () => {
    set({ loading: true });
    try {
      const providers = await invoke<ProviderPublic[]>("ai_provider_list");
      const active = await invoke<string>("ai_current_provider");
      set({ providers, active, loading: false });
    } catch {
      set({ loading: false });
    }
  },

  add: async (p) => {
    // Sense aquests catch, un error del backend (permisos, disc ple, JSON
    // invàlid) es convertia en un rebuig sense gestionar: el formulari es
    // tanca net però el proveïdor NO s'ha desat, i l'usuari no veia res.
    try {
      await invoke("ai_provider_add", { provider: p });
      await get().load();
    } catch (e) {
      toastError("No s'ha pogut afegir el proveïdor", e);
      throw e;
    }
  },

  update: async (patch) => {
    try {
      await invoke("ai_provider_update", { patch });
      await get().load();
    } catch (e) {
      toastError("No s'ha pogut desar el proveïdor", e);
      throw e;
    }
  },

  setEnabled: async (id, enabled) => {
    const p = get().providers.find((x) => x.id === id);
    if (!p) return;
    // El backend conserva tot el que no enviem (el token sobretot).
    try {
      await invoke("ai_provider_update", {
        patch: {
          id: p.id,
          name: p.name,
          baseUrl: p.base_url,
          model: p.model,
          kind: (p.kind === "anthropic" ? "anthropic" : "openai") as ProviderKind,
          auth: p.auth,
          headerName: p.header_name,
          enabled,
        },
      });
      await get().load();
    } catch (e) {
      toastError("No s'ha pogut canviar l'estat del proveïdor", e);
    }
  },

  remove: async (id) => {
    try {
      await invoke("ai_provider_remove", { id });
      if (get().active === id) await get().select("ollama");
      await get().load();
    } catch (e) {
      toastError("No s'ha pogut esborrar el proveïdor", e);
    }
  },

  test: async (id) => {
    set({ testingId: id });
    try {
      const reply = await invoke<string>("ai_provider_test", { id });
      set((s) => ({ testingId: null, lastTest: { ...s.lastTest, [id]: { ok: true, msg: reply } } }));
    } catch (e) {
      set((s) => ({
        testingId: null,
        lastTest: { ...s.lastTest, [id]: { ok: false, msg: String(e) } },
      }));
    }
  },

  select: async (id) => {
    try {
      await invoke("ai_select_provider", { id });
      set({ active: id });
    } catch (e) {
      toastError("No s'ha pogut canviar el proveïdor actiu", e);
    }
  },

  /// Diagnòstic de la connexió ABANS de desar el proveïdor: prova el
  /// llistat de models del servidor i classifica l'error amb motiu clar.
  checkEndpoint: async ({ baseUrl, model, token, kind }) => {
    set({ checking: true, lastCheck: null });
    try {
      const check = await invoke<EndpointCheck>("ai_endpoint_check", {
        baseUrl,
        model,
        token: token || null,
        kind: kind ?? "openai",
      });
      set({ checking: false, lastCheck: check });
      return check;
    } catch {
      set({ checking: false });
      return null;
    }
  },
}));
