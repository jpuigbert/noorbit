/// Tancament amb Esc del modal «Nou projecte» (jsdom + i18n real): obert +
/// Escape → tancat; tancat → Esc no fa res; altres tecles no tanquen.
/// El workspaceStore es simula perquè només es fa servir en crear el
/// projecte (i la seva cadena d'imports arrossega Monaco).
import { createElement, act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, beforeEach, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(async () => undefined),
  listen: vi.fn(async () => vi.fn()),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("../stores/workspaceStore", () => ({
  useWorkspaceStore: { getState: () => ({ openWorkspace: vi.fn(async () => undefined) }) },
}));

import NewProjectModal from "./NewProjectModal";
import { useUIStore } from "../stores/uiStore";

// Aviseu a React que som en un entorn de tests (silencia els avisos d'act).
(globalThis as Record<string, unknown>).IS_REACT_ACT_ENVIRONMENT = true;

async function mount() {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const root = createRoot(host);
  await act(async () => {
    root.render(createElement(NewProjectModal));
  });
  return { host, root };
}

async function pressEscape() {
  await act(async () => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  });
}

describe("NewProjectModal · Esc", () => {
  beforeEach(() => {
    useUIStore.getState().setShowNewProject(false);
  });

  it("obert + Escape → es tanca", async () => {
    useUIStore.getState().setShowNewProject(true);
    const { host, root } = await mount();
    expect(host.querySelector(".modal-overlay")).toBeTruthy();
    await pressEscape();
    expect(useUIStore.getState().showNewProject).toBe(false);
    expect(host.querySelector(".modal-overlay")).toBeNull();
    await act(async () => root.unmount());
  });

  it("tancat: Escape no fa res (i el listener es retira en desmuntar)", async () => {
    const { host, root } = await mount();
    expect(host.querySelector(".modal-overlay")).toBeNull();
    await pressEscape();
    expect(useUIStore.getState().showNewProject).toBe(false);
    expect(host.querySelector(".modal-overlay")).toBeNull();
    await act(async () => root.unmount());
    // Després de desmuntar, Esc no pot tocar cap estat ni petar.
    await pressEscape();
    expect(useUIStore.getState().showNewProject).toBe(false);
  });

  it("altres tecles no tanquen el modal", async () => {
    useUIStore.getState().setShowNewProject(true);
    const { host, root } = await mount();
    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "a" }));
    });
    expect(useUIStore.getState().showNewProject).toBe(true);
    expect(host.querySelector(".modal-overlay")).toBeTruthy();
    await act(async () => root.unmount());
  });
});