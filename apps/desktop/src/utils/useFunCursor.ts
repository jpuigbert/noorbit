/// Hook del cursor divertit: decideix QUIN personatge toca segons on
/// apunta el ratolí (editor, terminal, xat, fitxers, botons, text,
/// nanses…), i el pinta amb una sola variable CSS (--fun-cursor).
///
///  - Zonejat per delegació: un «mouseover» global resol la zona amb
///    selectors (cap listener per element, cap cost per píxel).
///  - Cada 5 minuts el cursor «normal» canvia de disfressa (coet → ovni
///    → estrella → planeta → cometa).
///  - Si no fas res durant 1 minut, entra el mode «avorrit»: disfresses
///    ultra-divertides (ànec, pizza, dònut, gat, fantasma…) que també
///    roten cada 5 minuts. Qualsevol moviment et torna al cursor de zona.
///  - Mentre la IA treballa, la zona per defecte mostra el rellotge de
///    sorra; quan una tasca acaba, l'estrella de festa (funFx).
import { useEffect } from "react";
import { useUIStore } from "../stores/uiStore";
import { useAgentStore } from "../stores/agentStore";
import { useComputerStore } from "../stores/computerStore";
import { useAIStore } from "../stores/aiStore";
import { DEFAULT_SET, IDLE_SET, ZONE_CURSORS, type FunZone } from "./funCursors";
import { funCelebrating, noteFunPointer } from "./funFx";
import "../styles/fun.css";

/// Elements que ja són «clicables» a l'app: la vareta màgica hi va sempre.
const INTERACTIVE =
  "button, a, [role='button'], select, summary, .btn, .icon-btn, .pm-tab, " +
  ".rp-tab, .rp-subtab, .theme-swatch, .tree-node, .status-item, .setting-check";

const TEXTUAL =
  "input:not([type='checkbox']):not([type='radio']):not([type='range']):not([type='button']):not([type='submit']), " +
  "textarea, [contenteditable='true']";

/// Resol la zona del cursor per a l'element sota el punter.
function resolveZone(el: Element, dragging: boolean): FunZone {
  if (dragging) return "grab";
  if (el.closest(INTERACTIVE)) return "pointer";
  if (el.closest(TEXTUAL)) return "text";
  const rh = el.closest(".resize-handle");
  if (rh) return rh.classList.contains("rh-y") ? "resizeY" : "resizeX";
  if (el.closest(".monaco-editor")) return "code";
  if (el.closest(".terminal-panel")) return "terminal";
  if (el.closest(".agent-panel")) return "chat";
  if (el.closest(".sidebar")) return "files";
  if (el.closest(".menubar, .statusbar, .detached-head")) return "nav";
  return "default";
}

/// «La IA està treballant ara mateix?» (agent, ordinador o baixada de model).
function workingNow(): boolean {
  const agent = useAgentStore.getState().sessions.some((s) => s.running);
  const computer = useComputerStore.getState();
  const pulling = Object.keys(useAIStore.getState().pulling).length > 0;
  return agent || computer.busy || computer.agentRunning || pulling;
}

const IDLE_AFTER_MS = 60_000; // 1 min sense tocar res → mode disfressa
const ROTATE_MS = 300_000; // canvi de disfressa cada 5 minuts

export function useFunCursor() {
  // Mode professional: el cursor disfressat queda silenciat encara que
  // l'usuari el tinguera activat (el flag lúdic es conserva intacte).
  const enabled = useUIStore((s) => s.funCursor && !s.professionalMode);

  useEffect(() => {
    const root = document.documentElement;
    if (!enabled) {
      root.classList.remove("fun-cursors");
      root.style.removeProperty("--fun-cursor");
      return;
    }
    root.classList.add("fun-cursors");

    let zone: FunZone = "default";
    let dragging = false;
    let lastInput = Date.now();
    // Comencem en una disfressa aleatòria: cada arrencada és diferent.
    let variant = Math.floor(Math.random() * DEFAULT_SET.length);
    let idleIndex = 0;
    let wasIdle = false;
    let applied = "";

    const over = (e: MouseEvent) => {
      if (e.target instanceof Element) zone = resolveZone(e.target, dragging);
    };
    const move = (e: PointerEvent) => {
      lastInput = Date.now();
      noteFunPointer(e.clientX, e.clientY);
    };
    const down = (e: PointerEvent) => {
      lastInput = Date.now();
      if (e.target instanceof Element && e.target.closest(".resize-handle")) dragging = true;
    };
    const up = () => {
      dragging = false;
    };
    const input = () => {
      lastInput = Date.now();
    };

    document.addEventListener("mouseover", over, true);
    window.addEventListener("pointermove", move, { passive: true });
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("pointerup", up, true);
    window.addEventListener("keydown", input, true);
    window.addEventListener("wheel", input, { passive: true });

    /// Aplica el cursor que toca (només escriu al DOM quan canvia).
    const tick = () => {
      const now = Date.now();
      const idle = now - lastInput > IDLE_AFTER_MS;
      if (idle && !wasIdle) idleIndex = Math.floor(Math.random() * IDLE_SET.length);
      wasIdle = idle;

      let css: string;
      if (funCelebrating()) css = ZONE_CURSORS.party.css;
      else if (idle) css = IDLE_SET[idleIndex % IDLE_SET.length].css;
      else if (zone === "default")
        css = workingNow() ? ZONE_CURSORS.busy.css : DEFAULT_SET[variant % DEFAULT_SET.length].css;
      else css = ZONE_CURSORS[zone].css;

      if (css !== applied) {
        applied = css;
        root.style.setProperty("--fun-cursor", css);
      }
    };
    const tickId = window.setInterval(tick, 250);
    const rotateId = window.setInterval(() => {
      variant++;
      idleIndex++;
    }, ROTATE_MS);
    tick();

    return () => {
      window.clearInterval(tickId);
      window.clearInterval(rotateId);
      document.removeEventListener("mouseover", over, true);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("pointerup", up, true);
      window.removeEventListener("keydown", input, true);
      window.removeEventListener("wheel", input);
      root.classList.remove("fun-cursors");
      root.style.removeProperty("--fun-cursor");
    };
  }, [enabled]);
}