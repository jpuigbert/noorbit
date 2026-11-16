/// Efectes de celebració de NoOrbit: quan una tasca acaba (l'agent del
/// xat, una tasca en segon pla o una «@done:» de la checklist) sona un
/// toc alegre, el cursor es disfressa de festa uns segons i salta un
/// confeti petit al punt on hi ha el ratolí. Tot és local i lleuger:
/// SVG + WebAudio, cap dependència nova.
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useTaskStore, type TaskInfo } from "../stores/taskStore";
import { useAgentStore } from "../stores/agentStore";
import { useTaskListStore } from "../stores/taskListStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import { playFunDone, playFunError, playFunPop, unlockFunSound } from "./funSound";

/// Darrer punt del ratolí (el passa useFunCursor): el confeti surt d'ací.
let pointer = { x: window.innerWidth / 2, y: window.innerHeight / 2 };
export function noteFunPointer(x: number, y: number) {
  pointer = { x, y };
}

/// Finestra de celebració del cursor de festa (la llig useFunCursor).
let celebrateUntil = 0;
export function funCelebrating(): boolean {
  return Date.now() < celebrateUntil;
}

/// Anti-metralleta: com a màxim un toc alegre cada 1,2 s (una resposta
/// pot marcar diverses tasques de cop i no volem una cassola).
let lastChime = 0;

const CONFETTI_COLORS = ["#ffd166", "#ff5d5d", "#7dd3fc", "#4ade80", "#f472b6", "#c4b5fd"];

/// Peca de confeti (estel, rodona o quadrat) construïda amb DOM SVG:
/// res de markup injectat, tot nodes creats a mà.
function confettiPiece(kind: number): SVGSVGElement {
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 8 8");
  let el: SVGElement;
  if (kind === 0) {
    el = document.createElementNS(NS, "path");
    el.setAttribute("d", "M4 0 L5 3 L8 4 L5 5 L4 8 L3 5 L0 4 L3 3 Z");
  } else if (kind === 1) {
    el = document.createElementNS(NS, "circle");
    el.setAttribute("cx", "4");
    el.setAttribute("cy", "4");
    el.setAttribute("r", "3.4");
  } else {
    el = document.createElementNS(NS, "rect");
    el.setAttribute("x", "1");
    el.setAttribute("y", "1");
    el.setAttribute("width", "6");
    el.setAttribute("height", "6");
    el.setAttribute("rx", "1.5");
  }
  el.setAttribute("fill", "currentColor");
  svg.appendChild(el);
  return svg;
}

/// Confeti petit al voltant del punter: 12 peces que salten i cauen
/// esvaïnt-se (900 ms). Es neteja sol: cap node viu després de l'efecte.
function confettiBurst(x: number, y: number) {
  // Límit de cortesia: si ja hi ha 3 celebracions en pantalla, no n'afegim més.
  if (document.querySelectorAll(".fun-confetti").length >= 3) return;
  const host = document.createElement("div");
  host.className = "fun-confetti";
  host.style.left = x + "px";
  host.style.top = y + "px";
  for (let i = 0; i < 12; i++) {
    const p = document.createElement("span");
    const ang = (Math.PI * 2 * i) / 12 + Math.random() * 0.5;
    const dist = 45 + Math.random() * 65;
    p.style.color = CONFETTI_COLORS[i % CONFETTI_COLORS.length];
    p.style.setProperty("--dx", Math.round(Math.cos(ang) * dist) + "px");
    p.style.setProperty("--dy", Math.round(Math.sin(ang) * dist - 55) + "px");
    p.style.setProperty("--rot", Math.round(Math.random() * 540 - 270) + "deg");
    p.appendChild(confettiPiece(i % 3));
    host.appendChild(p);
  }
  document.body.appendChild(host);
  window.setTimeout(() => host.remove(), 1100);
}

/// Tasca acabada amb èxit: so + cursor de festa + confeti.
export function celebrateFun() {
  const now = Date.now();
  celebrateUntil = now + 2600;
  if (now - lastChime > 1200) {
    lastChime = now;
    playFunDone();
  }
  confettiBurst(pointer.x, pointer.y);
}

/// Tasca fallida: només el toc greu (res de festa).
export function failFun() {
  playFunError();
}

let initialized = false;

/// Arrenca els efectes (una sola vegada per finestra). Se subscriu als
/// stores que saben quan una tasca acaba:
///  - taskStore: tasques degradades a segon pla («task://state» done/error).
///  - agentStore: qualsevol xat que passa de «generant» a «aturat» (les
///    delegacions «#delegate» no sonen: ja sona el torn principal).
///  - taskListStore: checklist «@done:» (pop suau, sense confeti).
export function initFunFx() {
  if (initialized) return;
  initialized = true;

  // El webview només deixa sonar després d'un gest: despertem l'àudio
  // al primer clic o tecla (capture, per arribar abans que qualsevol handler).
  const unlock = () => unlockFunSound();
  window.addEventListener("pointerdown", unlock, { capture: true });
  window.addEventListener("keydown", unlock, { capture: true });

  // Les tasques en segon pla s'anuncien a TOTES les finestres: que només
  // soni a la principal, si no el mateix «done» se sentiria duplicat.
  let isMain = true;
  try {
    isMain = getCurrentWebviewWindow().label === "main";
  } catch {
    /* fora de Tauri (dev al navegador): tractem-ho com a principal */
  }
  if (isMain) {
    let prevTasks: Record<string, TaskInfo> = {};
    useTaskStore.subscribe((s) => {
      for (const [id, t] of Object.entries(s.tasks)) {
        const old = prevTasks[id];
        if (old?.state === t.state) continue;
        if (t.state === "done") celebrateFun();
        else if (t.state === "error") failFun();
      }
      prevTasks = s.tasks;
    });
  }

  // Torn de l'agent acabat: running true → false. Si l'usuari l'ha aturat
  // («stopped») no hi ha res a celebrar; si ha acabat amb error, toc greu.
  const prevRunning = new Map<string, boolean>();
  useAgentStore.subscribe((s) => {
    for (const sess of s.sessions) {
      const was = prevRunning.get(sess.id) ?? false;
      if (was && !sess.running && !sess.id.includes("#delegate") && !sess.stopped) {
        if (sess.error) failFun();
        else celebrateFun();
      }
      prevRunning.set(sess.id, sess.running);
    }
  });

  // Checklist: cada tasca que passa a «feta» fa un pop. En canviar de
  // projecte el calaix es recarrega sencer: això NO és una celebració.
  let lastRoot = useWorkspaceStore.getState().root;
  let prevDone = new Set(
    useTaskListStore.getState().items.filter((i) => i.done).map((i) => i.id)
  );
  useTaskListStore.subscribe((s) => {
    const root = useWorkspaceStore.getState().root;
    const doneNow = new Set(s.items.filter((i) => i.done).map((i) => i.id));
    if (root !== lastRoot) {
      lastRoot = root;
      prevDone = doneNow;
      return;
    }
    let hit = false;
    for (const id of doneNow) if (!prevDone.has(id)) hit = true;
    prevDone = doneNow;
    if (hit) playFunPop();
  });
}