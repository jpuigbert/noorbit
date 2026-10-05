import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

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

  load: () => Promise<void>;
  add: (p: ProviderInput) => Promise<void>;
  update: (patch: ProviderInput & { id: string }) => Promise<void>;
  remove: (id: string) => Promise<void>;
  test: (id: string) => Promise<void>;
  select: (id: string) => Promise<void>;
}

export const useProviderStore = create<ProviderState>((set, get) => ({
  providers: [],
  active: "ollama",
  loading: false,
  testingId: null,
  lastTest: {},

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
    await invoke("ai_provider_add", { provider: p });
    await get().load();
  },

  update: async (patch) => {
    await invoke("ai_provider_update", { patch });
    await get().load();
  },

  remove: async (id) => {
    await invoke("ai_provider_remove", { id });
    if (get().active === id) await get().select("ollama");
    await get().load();
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
    await invoke("ai_select_provider", { id });
    set({ active: id });
  },
}));
