/// Tests de les utilitats PURES del xat: historial, errors, claus de
/// persistència i detecció de models petits.
import { describe, expect, it } from "vitest";
import {
  chatLsKey,
  defaultCtx,
  defaultShowThinking,
  extractChatImagePaths,
  isBodyCut,
  isCancelled,
  isOOM,
  isRetriableError,
  isSmallModel,
  newId,
  threadOf,
  toErrorText,
  toHistory,
} from "./chatUtils";
import type { ChatMessage } from "./types";

const msg = (role: "user" | "assistant", text: string, error?: boolean): ChatMessage =>
  ({ id: "x", role, text, error, ts: 0 }) as unknown as ChatMessage;

describe("toHistory / threadOf", () => {
  it("filtra missatges buits i amb error, i es queda els últims 16", () => {
    const msgs = [
      msg("user", "  "),
      msg("assistant", "fallit", true),
      ...Array.from({ length: 20 }, (_, i) => msg("user", `m${i}`)),
    ];
    const h = toHistory(msgs);
    expect(h).toHaveLength(16);
    expect(h[0].content).toBe("m4");
    expect(h[15].content).toBe("m19");
  });

  it("retalla cada torn a 12.000 caràcters", () => {
    const h = toHistory([msg("user", "a".repeat(20000))]);
    expect(h[0].content).toHaveLength(12000);
  });

  it("threadOf conserva 6 torns de 800 caràcters (últim recurs de memòria)", () => {
    const hist = Array.from({ length: 10 }, (_, i) => ({
      role: "user" as const,
      content: `t${i}` + "x".repeat(1000),
    }));
    const th = threadOf(hist);
    expect(th).toHaveLength(6);
    expect(th[0].content.startsWith("t4")).toBe(true);
    expect(th.every((t) => t.content.length <= 800)).toBe(true);
  });
});

describe("classificació d'errors", () => {
  it("isOOM detecta les variants de memòria d'Ollama", () => {
    expect(isOOM("CUDA out of memory")).toBe(true);
    expect(isOOM("could not allocate buffer")).toBe(true);
    expect(isOOM("failed to allocate")).toBe(true);
    expect(isOOM("error qualsevol")).toBe(false);
  });

  it("isCancelled només reconeix el marcador català", () => {
    expect(isCancelled("Tasca cancel·lat per l'usuari")).toBe(true);
    expect(isCancelled("cancelled")).toBe(false);
  });

  it("isBodyCut i isRetriableError", () => {
    expect(isBodyCut("error decoding response body")).toBe(true);
    expect(isBodyCut("stream tallat")).toBe(true);
    expect(isRetriableError("out of memory")).toBe(true);
    expect(isRetriableError("payload was not fully received")).toBe(true);
    expect(isRetriableError("syntax error")).toBe(false);
  });

  it("toErrorText tradueix OOM/body-cut i deixa la resta igual", () => {
    expect(toErrorText("out of memory")).toContain("Memòria insuficient");
    expect(toErrorText("stream tallat")).toContain("s'ha tallat");
    expect(toErrorText("hola")).toBe("hola");
  });
});

describe("preferències i persistència", () => {
  it("defaultShowThinking respecta el «0» de localStorage", () => {
    localStorage.setItem("noorbit.showThinking", "0");
    expect(defaultShowThinking()).toBe(false);
    localStorage.setItem("noorbit.showThinking", "1");
    expect(defaultShowThinking()).toBe(true);
    localStorage.removeItem("noorbit.showThinking");
  });

  it("defaultCtx és sempre actiu (la IA treballa amb el codi real)", () => {
    expect(defaultCtx()).toBe(true);
  });

  it("chatLsKey separa per projecte i té calaix propi sense projecte", () => {
    expect(chatLsKey("/home/joan/projecte")).toBe("noorbit.chats::/home/joan/projecte");
    expect(chatLsKey(null)).toBe("noorbit.chats::__no_project__");
  });

  it("newId genera identificadors únics amb prefix «x»", () => {
    const a = newId();
    const b = newId();
    expect(a.startsWith("x")).toBe(true);
    expect(a).not.toBe(b);
  });
});

describe("isSmallModel", () => {
  it("detecta models ≤3,5B pel tag d'Ollama", () => {
    expect(isSmallModel("qwen2.5:3b")).toBe(true);
    expect(isSmallModel("llama3.2:3.5b")).toBe(true);
    expect(isSmallModel("mistral:7b")).toBe(false);
    expect(isSmallModel("gemma:12b")).toBe(false);
    expect(isSmallModel(null)).toBe(false);
    expect(isSmallModel("sense-tag")).toBe(false);
  });
});

describe("extractChatImagePaths", () => {
  it("extreu rutes d'imatge absolutes i les deduplica", () => {
    const text =
      "Mira /tmp/out.png i /tmp/out.png també /var/fotos/a.jpg; ignora /tmp/x.txt";
    expect(extractChatImagePaths(text)).toEqual(["/tmp/out.png", "/var/fotos/a.jpg"]);
  });

  it("sense imatges torna buit", () => {
    expect(extractChatImagePaths("cap imatge ací")).toEqual([]);
  });
});