/// Tancament amb Esc del modal de Compte (jsdom + i18n real, authStore real
/// sobre localStorage): obert + Escape → tancat; tancat → Esc no fa res;
/// altres tecles no tanquen.
import { createElement, act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, beforeEach, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(async () => undefined),
  listen: vi.fn(async () => vi.fn()),
}));

import AccountModal from "./AccountModal";
import { useUIStore } from "../../stores/uiStore";
import { useAuthStore } from "../../stores/authStore";

// Aviseu a React que som en un entorn de tests (silencia els avisos d'act).
(globalThis as Record<string, unknown>).IS_REACT_ACT_ENVIRONMENT = true;

async function mount() {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const root = createRoot(host);
  await act(async () => {
    root.render(createElement(AccountModal));
  });
  return { host, root };
}

async function pressEscape() {
  await act(async () => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  });
}

describe("AccountModal · Esc", () => {
  beforeEach(() => {
    useUIStore.getState().setShowAccount(false);
    // Sense sessió: la vista d'entrada (registre/login), com a l'app neta.
    useAuthStore.setState({ user: null, accounts: [] });
  });

  it("obert + Escape → es tanca", async () => {
    useUIStore.getState().setShowAccount(true);
    const { host, root } = await mount();
    expect(host.querySelector(".modal-overlay")).toBeTruthy();
    await pressEscape();
    expect(useUIStore.getState().showAccount).toBe(false);
    expect(host.querySelector(".modal-overlay")).toBeNull();
    await act(async () => root.unmount());
  });

  it("tancat: Escape no fa res (i el listener es retira en desmuntar)", async () => {
    const { host, root } = await mount();
    expect(host.querySelector(".modal-overlay")).toBeNull();
    await pressEscape();
    expect(useUIStore.getState().showAccount).toBe(false);
    expect(host.querySelector(".modal-overlay")).toBeNull();
    await act(async () => root.unmount());
    // Després de desmuntar, Esc no pot tocar cap estat ni petar.
    await pressEscape();
    expect(useUIStore.getState().showAccount).toBe(false);
  });

  it("altres tecles no tanquen el modal", async () => {
    useUIStore.getState().setShowAccount(true);
    const { host, root } = await mount();
    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "a" }));
    });
    expect(useUIStore.getState().showAccount).toBe(true);
    expect(host.querySelector(".modal-overlay")).toBeTruthy();
    await act(async () => root.unmount());
  });
});