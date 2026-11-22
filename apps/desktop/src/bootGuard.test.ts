/// Vigilà d'arrencada d'index.html: cobreix NOMÉS la finestra d'arrencada.
/// Abans que React pinte, qualsevol error (script o recurs) ha de pintar el
/// panell de pànic; un cop React ha pintat, el vigilà s'ha de retirar del tot
/// (listener fora + panell esborrat) perquè un error trivial de sessió —p. ex.
/// una <img> amb un data-URL indecodificable— no segreste la UI.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import html from "../index.html?raw";

const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
if (!script) throw new Error("No s'ha trobat el script inline del vigilà a index.html");

/// Arma una instància nova del vigilà (avalua l'script inline al global).
function armGuard(): void {
  (0, eval)(script!);
}

/// Deixa córrer els microtasks (MutationObserver de jsdom) i un torn de timer.
async function flush(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0));
}

/// Simula que React ha pintat dins de #root.
function paintReact(): void {
  document.getElementById("root")!.appendChild(document.createElement("div"));
}

function panicPanel(): HTMLElement | null {
  return document.getElementById("boot-error");
}

beforeEach(() => {
  document.body.innerHTML = '<div id="root"></div>';
});

afterEach(async () => {
  // Desarma els vigilans que queden vius (els que NO han vist pintar React):
  // en pintar #root, el seu observer els lleva el listener i es desconnecta.
  vi.useRealTimers();
  paintReact();
  await flush();
});

describe("vigilà d'arrencada · cobertura real d'arrencada", () => {
  it("un error de script abans de pintar mostra el panell de pànic", () => {
    armGuard();
    window.dispatchEvent(new ErrorEvent("error", { message: "SyntaxError: paquet trencat" }));
    const panel = panicPanel();
    expect(panel).not.toBeNull();
    expect(panel!.textContent).toContain("NoOrbit no s'ha iniciat");
    expect(document.getElementById("boot-error-detail")!.textContent).toContain(
      "SyntaxError: paquet trencat"
    );
  });

  it("un error de recurs (img/script) abans de pintar també mostra el panell", () => {
    armGuard();
    const img = document.createElement("img");
    img.src = "data:image/png;base64,AAAA";
    document.body.appendChild(img);
    img.dispatchEvent(new Event("error"));
    expect(panicPanel()).not.toBeNull();
    expect(document.getElementById("boot-error-detail")!.textContent).toContain(
      "No s'ha pogut carregar el recurs"
    );
  });

  it("si React no pinta, l'avís dels 6 segons continua sortint", async () => {
    vi.useFakeTimers();
    armGuard();
    await vi.advanceTimersByTimeAsync(6000);
    expect(panicPanel()).not.toBeNull();
    expect(document.getElementById("boot-error-detail")!.textContent).toContain("segueix buit");
  });
});

describe("vigilà d'arrencada · retirada quan React pinta", () => {
  it("en pintar React, el panell existent s'esborra", async () => {
    armGuard();
    window.dispatchEvent(new ErrorEvent("error", { message: "error primerenc" }));
    expect(panicPanel()).not.toBeNull();
    paintReact();
    await flush();
    expect(panicPanel()).toBeNull();
  });

  it("després de pintar, els errors de sessió NO tornen a pintar el panell", async () => {
    armGuard();
    paintReact();
    await flush();
    // Error de script de sessió (abans segrestava la UI).
    window.dispatchEvent(new ErrorEvent("error", { message: "TypeError de sessió" }));
    // I el cas de l'incident: una <img> amb contingut indecodificable.
    const img = document.createElement("img");
    img.src = "data:image/png;base64,W0NvbnRpbmd1dCBkZSBsYSB0ZXh0dXJhXQ==";
    document.body.appendChild(img);
    img.dispatchEvent(new Event("error"));
    await flush();
    expect(panicPanel()).toBeNull();
  });

  it("el panell no reapareix ni tan sols amb molts errors després de l'arrencada", async () => {
    armGuard();
    paintReact();
    await flush();
    for (let i = 0; i < 10; i++) {
      window.dispatchEvent(new ErrorEvent("error", { message: "error " + i }));
    }
    expect(panicPanel()).toBeNull();
  });
});