/// Smoke test del renderitzat del xat: una resposta típica de la IA amb
/// targeta «@file:», bloc ```, xips de directrius i prosa s'ha de muntar
/// sense errors (jsdom + i18n real).
import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { act } from "react";
import { describe, expect, it } from "vitest";
import { ChatMarkdown } from "./ChatMarkdown";

// Aviseu a React que som en un entorn de tests (silencia els avisos d'act).
(globalThis as Record<string, unknown>).IS_REACT_ACT_ENVIRONMENT = true;

describe("ChatMarkdown render", () => {
  it("munta una resposta completa sense errors", async () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const root = createRoot(host);
    const text = [
      "Aquí tens la solució:",
      "@file: src/app.py",
      "```python",
      "def hola():",
      "    return 1",
      "```",
      "@dir: assets/",
      "@delete: vell.txt",
      "Prova-ho amb `python3 app.py`.",
    ].join("\n");
    await act(async () => {
      root.render(createElement(ChatMarkdown, { text }));
    });
    expect(host.querySelectorAll(".cmd-file").length).toBe(1);
    expect(host.querySelectorAll(".cmd-dir").length).toBe(2);
    expect(host.textContent).toContain("src/app.py");
    expect(host.textContent).toContain("def hola()");
    await act(async () => root.unmount());
  });

  it("un bloc de codi llarg es mostra SENCER per defecte (no es contrau)", async () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const root = createRoot(host);
    const n = 40;
    const code = Array.from({ length: n }, (_, i) => `linea_${i + 1} = ${i + 1}`).join("\n");
    await act(async () => {
      root.render(createElement(ChatMarkdown, { text: "```python\n" + code + "\n```" }));
    });
    // Totes les línies pintades d'entrada, sense haver de prémer «desplega».
    expect(host.querySelectorAll(".cmd-line").length).toBe(n);
    expect(host.textContent).toContain(`linea_${n} = ${n}`);
    expect(host.textContent).toContain("linea_1 = 1");
    await act(async () => root.unmount());
  });

  it("el botó «Crea al projecte» només apareix si hi ha callback", async () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const root = createRoot(host);
    await act(async () => {
      root.render(createElement(ChatMarkdown, { text: "@file: a.py\nprint(1)" }));
    });
    expect(host.querySelectorAll(".cmd-apply").length).toBe(0);
    await act(async () => {
      root.render(
        createElement(ChatMarkdown, {
          text: "@file: a.py\nprint(1)",
          onApplyFile: () => Promise.resolve("ok"),
        })
      );
    });
    expect(host.querySelectorAll(".cmd-apply").length).toBe(1);
    await act(async () => root.unmount());
  });
});
