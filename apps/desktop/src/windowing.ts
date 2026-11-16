/// Finestres de panells separats («destacar»): el xat i el terminal poden
/// viure en una finestra pròpia que l'usuari pot portar a QUASEVOL pantalla
/// del sistema. El programa principal només n'atura el flag (uiStore) i els
/// panells es tornen a enganxar quan la finestra es tanca (event
/// «panel://closed» que emet la mateixa finestra separada en marxar).
import { emit, listen } from "@tauri-apps/api/event";
import { availableMonitors } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { t } from "./i18n";
import { useUIStore } from "./stores/uiStore";
import { useWorkspaceStore } from "./stores/workspaceStore";
import { toastError } from "./stores/toastStore";
import { reloadChatsFromStorage } from "./stores/agentStore";

/// Etiquetes de les finestres separades (les detecta main.tsx en arrencar
/// per pintar NOMÉS el panell corresponent dins la finestra nova).
export const CHAT_LABEL = "noorbit-chat";
export const TERM_LABEL = "noorbit-terminal";

/// Clau de localStorage on el programa deixa el projecte del xat ABANS de
/// crear la finestra separada (les webviews de l'app comparteixen el mateix
/// origen, per tant el mateix localStorage). No va per l'URL perquè el
/// projecte pot contenir qualsevol caràcter i el calaix n'ha de ser exacte.
export const DETACHED_CHAT_ROOT = "noorbit.detachedChatRoot";

interface PanelGeo {
  x: number;
  y: number;
  width: number;
  height: number;
}

const geoKey = (label: string) => "noorbit.win." + label;

function loadPanelGeo(label: string): PanelGeo | undefined {
  try {
    const raw = localStorage.getItem(geoKey(label));
    if (!raw) return undefined;
    const g = JSON.parse(raw) as PanelGeo;
    if (!Number.isFinite(g.x) || !Number.isFinite(g.y)) return undefined;
    if (!Number.isFinite(g.width) || !Number.isFinite(g.height)) return undefined;
    // Mides corruptes (0, negatives o NaN d'un desat malmès): la finestra
    // sortiria invisible o la creació fallaria.
    if (g.width < 240 || g.height < 180) return undefined;
    return g;
  } catch {
    return undefined;
  }
}

/// Desa la geometria d'una finestra separada (es crida just abans de tancar-la).
async function savePanelGeo(label: string) {
  try {
    const w = await WebviewWindow.getByLabel(label);
    if (!w) return;
    const [pos, size, scale] = await Promise.all([w.outerPosition(), w.outerSize(), w.scaleFactor()]);
    // `outerPosition`/`outerSize` tornen PÍXELS FÍSICS mentre que les
    // opcions de creació de finestra són LÒGICS: sense la conversió, a les
    // pantalles Retina la geometria es DOBLAVA a cada reobertura.
    const logical = pos.toLogical(scale);
    const logicalSize = size.toLogical(scale);
    localStorage.setItem(
      geoKey(label),
      JSON.stringify({ x: logical.x, y: logical.y, width: logicalSize.width, height: logicalSize.height })
    );
  } catch {
    /* sense persistència de geometria: la finestra surt centrada */
  }
}

/// Si la pantalla on estava la finestra ja no existeix (portàtil desconnectat),
/// el gestor la posaria fora de la vista: la tornem dins l'àrea disponible.
async function clampGeo(geo: PanelGeo): Promise<PanelGeo> {
  try {
    const monitors = await availableMonitors();
    if (monitors.length === 0) return geo;
    // Els monitors venen en píxels físics: els passem a LÒGICS (cada un amb
    // el seu factor d'escala) per comparar-los amb la geometria desada.
    const logical = monitors.map((m) => ({
      x: m.position.toLogical(m.scaleFactor).x,
      y: m.position.toLogical(m.scaleFactor).y,
      w: m.size.toLogical(m.scaleFactor).width,
      h: m.size.toLogical(m.scaleFactor).height,
    }));
    const minX = Math.min(...logical.map((m) => m.x));
    const minY = Math.min(...logical.map((m) => m.y));
    const maxX = Math.max(...logical.map((m) => m.x + m.w));
    const maxY = Math.max(...logical.map((m) => m.y + m.h));
    const width = Math.min(geo.width, maxX - minX);
    const height = Math.min(geo.height, maxY - minY);
    return {
      width,
      height,
      x: Math.min(Math.max(geo.x, minX), maxX - width),
      y: Math.min(Math.max(geo.y, minY), maxY - height),
    };
  } catch {
    return geo;
  }
}

/// Crides en curs (doble clic ràpid sobre el botó de separar): sense això,
/// dues crides simultànies crearien DUES finestres amb la mateixa etiqueta
/// i la segona fallaria amb «label already exists».
const opening = new Map<string, Promise<boolean>>();

/// Retorna «true» si la finestra s'ha pogut crear o ja existia; «false» si
/// la creació ha fallat (el qui crida només ha de marcar el panell com a
/// separat en el primer cas).
async function openPanelWindow(
  label: string,
  title: string,
  defaults: { width: number; height: number; minWidth: number; minHeight: number },
  url?: string
): Promise<boolean> {
  const pending = opening.get(label);
  if (pending) return pending;

  const task = (async () => {
    try {
      const existing = await WebviewWindow.getByLabel(label);
      if (existing) {
        // Ja oberta (per exemple després d'una recàrrega del programa): la
        // posem al davant en comptes de crear una de nova.
        void existing.setFocus().catch(() => undefined);
        return true;
      }
    } catch {
      /* si la consulta falla, provem de crear-la igualment */
    }

    const saved = loadPanelGeo(label);
    const geo = saved ? await clampGeo(saved) : undefined;
    try {
      const win = new WebviewWindow(label, {
        title,
        ...(url ? { url } : {}),
        ...(geo
          ? { x: geo.x, y: geo.y, width: geo.width, height: geo.height }
          : { ...defaults, center: true }),
        minWidth: defaults.minWidth,
        minHeight: defaults.minHeight,
        resizable: true,
        theme: "dark",
        backgroundColor: "#0d0d12",
      });
      win.once("tauri://error", (e) => {
        // Si la creació falla DESPRÉS d'haver marcat el panell com a separat,
        // desfem el flag: si no, l'usuari quedaria amb el marc «torna al
        // programa» i cap finestra per tornar-hi.
        const ui = useUIStore.getState();
        if (label === CHAT_LABEL) ui.setChatDetached(false);
        else if (label === TERM_LABEL) ui.setTerminalDetached(false);
        toastError(t("panel.errOpen"), String((e.payload as { label?: string } | undefined)?.label ?? e.payload ?? ""));
      });
      return true;
    } catch (e) {
      toastError(t("panel.errOpen"), String(e));
      return false;
    }
  })();

  opening.set(label, task);
  try {
    return await task;
  } finally {
    opening.delete(label);
  }
}

/// Separar el XAT (panell Agent) en una finestra pròpia. El projecte actual
/// es deixa a localStorage (DETACHED_CHAT_ROOT) perquè la finestra nova
/// carregui el calaix de xats del mateix projecte.
export async function detachChat() {
  try {
    localStorage.setItem(DETACHED_CHAT_ROOT, useWorkspaceStore.getState().root ?? "");
  } catch {
    /* sense persistència: el xat separat obrirà el calaix «sense projecte» */
  }
  const ok = await openPanelWindow(
    CHAT_LABEL,
    t("panel.chatTitle"),
    { width: 480, height: 780, minWidth: 380, minHeight: 520 },
    "index.html"
  );
  // Només marquem el panell com a separat si la finestra s'ha pogut crear
  // (o ja existia): si la creació ha fallat, el panell es queda al programa.
  useUIStore.getState().setChatDetached(ok);
}

/// Separar el TERMINAL en una finestra pròpia (el seu historial comença buit,
/// com un terminal nou).
export async function detachTerminal() {
  const ok = await openPanelWindow(
    TERM_LABEL,
    t("panel.terminalTitle"),
    { width: 820, height: 460, minWidth: 560, minHeight: 320 },
    "index.html"
  );
  useUIStore.getState().setTerminalDetached(ok);
}

/// Reenganxar: tanquem la finestra separada; el flag es neteja aquí i els
/// xats es recarreguen del calaix (la finestra separada els hi va desant).
export async function reattachChat() {
  useUIStore.getState().setChatDetached(false);
  // Recordem on era la finestra abans de tancar-la: abans, reenganxar des
  // d'aquí PERDIA la geometria (només la desava la pròpia finestra en
  // tancar-se amb la seva creu).
  await savePanelGeo(CHAT_LABEL);
  try {
    const w = await WebviewWindow.getByLabel(CHAT_LABEL);
    void w?.destroy().catch(() => undefined);
  } catch {
    /* ja no existeix */
  }
  reloadChatsFromStorage();
}

export async function reattachTerminal() {
  useUIStore.getState().setTerminalDetached(false);
  await savePanelGeo(TERM_LABEL);
  try {
    const w = await WebviewWindow.getByLabel(TERM_LABEL);
    void w?.destroy().catch(() => undefined);
  } catch {
    /* ja no existeix */
  }
}

/// Tanca la pròpia finestra separada (el fa servir EL CONTINGUT de la
/// finestra: botó «Torna al programa» i la creu de la finestra). Desa la
/// geometria, avisa el programa principal i destrueix la finestra.
export async function closePanelWindow(label: string) {
  await savePanelGeo(label);
  await emit("panel://closed", { label }).catch(() => undefined);
  try {
    const w = await WebviewWindow.getByLabel(label);
    void w?.destroy().catch(() => undefined);
  } catch {
    /* ja no existeix */
  }
}

/// Escoltador del programa principal: quan una finestra separada es tanca
/// (creu X o botó «Torna al programa»), neteja el flag i recupera els xats.
/// Pot arribar DOS avisos per tancament (un de la pròpia finestra via JS i
/// un altre del backend en destruir-la): amb el flag net, el segon no fa res.
export function initPanelWindowSync(): Promise<() => void> {
  return listen<{ label: string }>("panel://closed", async ({ payload }) => {
    const ui = useUIStore.getState();
    if (payload?.label === CHAT_LABEL && ui.chatDetached) {
      // Si hi ha una finestra VIVA amb aquesta etiqueta (l'usuari l'ha tornat
      // a separar abans que l'avís d'un tancament antic arribés, o la
      // destrucció ha fallat), no toquem res: netejaríem el flag de la
      // finestra nova i el xat s'editaria des de DOS llocs.
      const alive = await WebviewWindow.getByLabel(CHAT_LABEL).catch(() => null);
      if (alive) return;
      ui.setChatDetached(false);
      reloadChatsFromStorage();
    } else if (payload?.label === TERM_LABEL && ui.terminalDetached) {
      const alive = await WebviewWindow.getByLabel(TERM_LABEL).catch(() => null);
      if (alive) return;
      ui.setTerminalDetached(false);
    }
  });
}
