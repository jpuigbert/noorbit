/// ChatImage (AgentPanel): si la webview no pot decodificar el data-URL
/// (p. ex. un .png que en realitat és text pla escrit per la IA), onError ha
/// de degradar al placeholder existent (ImageOff + nom) amb «imatge no
/// vàlida», igual que els errors d'invoke. Stores i components pesants
/// moquejats: ací només es prova la imatge del xat.
import { createElement, act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: vi.fn(() => ({ onDragDropEvent: vi.fn(async () => () => {}) })),
}));

const mockStore = vi.hoisted(() =>
  Object.assign(vi.fn(() => ({})), {
    getState: () => ({}),
    setState: vi.fn(),
    subscribe: vi.fn(),
  })
);
vi.mock("../../stores/agentStore", () => ({
  useAgentStore: mockStore,
  useActiveChat: vi.fn(() => null),
  extractRecommendedPaths: vi.fn(() => []),
  extractChatImagePaths: vi.fn(() => []),
}));
vi.mock("../../stores/aiStore", () => ({ useAIStore: mockStore, allCloudModels: vi.fn(() => []) }));
vi.mock("../../stores/expertStore", () => ({ useExpertStore: mockStore }));
vi.mock("../../stores/providerStore", () => ({
  useProviderStore: mockStore,
  isOllamaCloudProvider: vi.fn(() => false),
}));
vi.mock("../../stores/previewStore", () => ({ usePreviewStore: mockStore }));
vi.mock("../../stores/workspaceStore", () => ({ useWorkspaceStore: mockStore }));
vi.mock("../../stores/snapshotStore", () => ({ useSnapshotStore: mockStore }));
vi.mock("../../stores/systemStatsStore", () => ({
  useSystemStatsStore: mockStore,
  memPercent: vi.fn(() => 0),
  formatBytes: vi.fn(() => "0 B"),
}));
vi.mock("../../stores/uiStore", () => ({ useUIStore: mockStore }));
vi.mock("../../stores/processStore", () => ({ useProcessStore: mockStore }));
vi.mock("../../stores/webAiStore", () => ({ useWebAiStore: mockStore }));
vi.mock("./TaskChecklist", () => ({ default: () => null }));
vi.mock("./ChatVote", () => ({ default: () => null }));
vi.mock("./VoiceButton", () => ({ default: () => null }));
vi.mock("../CopyButton", () => ({ CopyButton: () => null, SaveButton: () => null }));
vi.mock("./ChatMarkdown", () => ({ default: () => null }));
vi.mock("./ChatThought", () => ({ default: () => null }));
vi.mock("../AI/LocalAIStatus", () => ({ default: () => null }));

import { invoke } from "@tauri-apps/api/core";
import { ChatImage } from "./AgentPanel";

const mockInvoke = vi.mocked(invoke);
(globalThis as Record<string, unknown>).IS_REACT_ACT_ENVIRONMENT = true;

const DATA_URL = "data:image/png;base64,iVBORw0KGgo=";

let host: HTMLElement;
let root: Root;

beforeEach(() => {
  mockInvoke.mockReset();
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

/// Ruta única per test: la caché d'imatges del mòdul és persistent.
async function mount(path: string) {
  await act(async () => {
    root.render(createElement(ChatImage, { path }));
  });
}

describe("ChatImage · degradació amb onError", () => {
  it("data-URL vàlid: mostra la <img>", async () => {
    mockInvoke.mockResolvedValue(DATA_URL);
    await mount("/proj/img-ok/cara.png");
    const img = host.querySelector("img");
    expect(img).not.toBeNull();
    expect(img!.getAttribute("src")).toBe(DATA_URL);
  });

  it("si la <img> no es pot decodificar: placeholder ImageOff + nom + «imatge no vàlida»", async () => {
    mockInvoke.mockResolvedValue(DATA_URL);
    await mount("/proj/textures/face_diffuse.png");
    const img = host.querySelector("img")!;
    await act(async () => {
      img.dispatchEvent(new Event("error"));
    });
    expect(host.querySelector("img")).toBeNull();
    const broken = host.querySelector(".chat-img-broken");
    expect(broken).not.toBeNull();
    expect(broken!.textContent).toContain("face_diffuse.png");
    expect(broken!.textContent).toContain("imatge no vàlida");
  });

  it("error d'invoke (signatura invàlida al backend): placeholder amb el nom", async () => {
    mockInvoke.mockRejectedValue("El fitxer no és una imatge vàlida (contingut no reconegut)");
    await mount("/proj/textures/falsa.png");
    expect(host.querySelector("img")).toBeNull();
    expect(host.querySelector(".chat-img-broken")!.textContent).toContain("falsa.png");
  });
});