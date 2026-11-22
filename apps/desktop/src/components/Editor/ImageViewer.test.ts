/// ImageViewer (EditorPane): si la webview no pot decodificar el data-URL
/// (p. ex. un .png que en realitat és text pla escrit per la IA), onError ha
/// de degradar al placeholder ImageOff + nom + «imatge no vàlida», igual que
/// els errors d'invoke. Mòduls pesants (Monaco, LSP…) moquejats: ací només
/// es prova el visor d'imatges.
import { createElement, act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn() }));
vi.mock("@monaco-editor/react", () => ({ default: () => null, DiffEditor: () => null }));
vi.mock("../../editor/editorApi", () => ({
  bindEditor: vi.fn(),
  monacoThemeFor: vi.fn(() => ""),
  setFocusedEditorGroup: vi.fn(),
  getEditor: vi.fn(),
}));
vi.mock("../../editor/diagnostics", () => ({
  validateCurrentFile: vi.fn(),
  lintOffline: vi.fn(),
  schedulePhpLint: vi.fn(),
}));
vi.mock("../../editor/quickFixes", () => ({ registerQuickFixes: vi.fn(), aiQuickFix: vi.fn() }));
vi.mock("../../editor/inlineComplete", () => ({
  registerInlineComplete: vi.fn(),
  autocompleteEnabled: vi.fn(() => false),
  toggleAutocomplete: vi.fn(),
}));
vi.mock("../../editor/aiSelection", () => ({ aiReplaceSelection: vi.fn() }));
vi.mock("../../editor/lspBridge", () => ({
  registerLspProviders: vi.fn(),
  scheduleLspDiagnostics: vi.fn(),
  setLspProjectRoot: vi.fn(),
}));
vi.mock("../../editor/monReady", () => ({ useMonacoReady: vi.fn(() => false) }));
vi.mock("../Illustrations", () => ({ ArtEmpty: () => null }));
vi.mock("./AiSelectionModal", () => ({ default: () => null }));
vi.mock("./ReferencesOverlay", () => ({ default: () => null }));
vi.mock("./Breadcrumbs", () => ({ default: () => null }));

const mockStore = vi.hoisted(() =>
  Object.assign(vi.fn(() => ({})), {
    getState: () => ({}),
    setState: vi.fn(),
    subscribe: vi.fn(),
  })
);
vi.mock("../../stores/workspaceStore", () => ({ useWorkspaceStore: mockStore }));
vi.mock("../../stores/uiStore", () => ({ useUIStore: mockStore }));
vi.mock("../../stores/editorStatusStore", () => ({ useEditorStatusStore: mockStore }));
vi.mock("../../stores/editReviewStore", () => ({
  AUTO_ACCEPT_MS: 35000,
  dropMarkOnManualSave: vi.fn(),
  resolveMark: vi.fn(),
  noteFileEdited: vi.fn(),
  useEditReviewStore: mockStore,
}));

import { invoke } from "@tauri-apps/api/core";
import { ImageViewer } from "./EditorPane";

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

async function mount(path = "/proj/textures/face_diffuse.png", name = "face_diffuse.png") {
  await act(async () => {
    root.render(createElement(ImageViewer, { path, name }));
  });
}

describe("ImageViewer · degradació amb onError", () => {
  it("data-URL vàlid: mostra la <img>", async () => {
    mockInvoke.mockResolvedValue(DATA_URL);
    await mount();
    const img = host.querySelector("img");
    expect(img).not.toBeNull();
    expect(img!.getAttribute("src")).toBe(DATA_URL);
  });

  it("si la <img> no es pot decodificar: placeholder amb nom + «imatge no vàlida»", async () => {
    mockInvoke.mockResolvedValue(DATA_URL);
    await mount();
    const img = host.querySelector("img")!;
    await act(async () => {
      img.dispatchEvent(new Event("error"));
    });
    expect(host.querySelector("img")).toBeNull();
    const msg = host.querySelector(".image-viewer-msg");
    expect(msg).not.toBeNull();
    expect(msg!.textContent).toContain("face_diffuse.png");
    expect(msg!.textContent).toContain("imatge no vàlida");
  });

  it("error d'invoke (p. ex. signatura invàlida al backend): missatge d'error", async () => {
    mockInvoke.mockRejectedValue("El fitxer no és una imatge vàlida (contingut no reconegut)");
    await mount();
    expect(host.querySelector("img")).toBeNull();
    expect(host.querySelector(".image-viewer-msg")!.textContent).toContain(
      "no és una imatge vàlida"
    );
  });
});