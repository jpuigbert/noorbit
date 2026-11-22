/// Hosts xterm del terminal PTY (Fase 3b): UNA instància de Terminal per
/// sessió, viva fora del cicle de React. Canviar de pestanya només amaga
/// el slot (display:none), NO desmunta l'xterm: el scrollback (5000 línies)
/// es conserva. En separar/reeenganxar la finestra, el backend manté la
/// sessió i un buffer de sortida recent (`term_scrollback`) que es repinta
/// ací en crear el host.
///
/// El tema es deriva dels tokens de theme.css (getComputedStyle), mai
/// colors literals: si l'usuari canvia de tema, `refreshTheme` els repinta.
import { Terminal, type ITheme } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { invoke } from "@tauri-apps/api/core";
import { onSessionOutput, useTerminalStore } from "../../stores/terminalStore";

export interface XtermHost {
  term: Terminal;
  fit: FitAddon;
  el: HTMLDivElement;
}

const hosts = new Map<string, XtermHost>();
const unsubs = new Map<string, () => void>();

/// Valor computat d'un token de theme.css (amb encadenament de fallbacks
/// entre tokens, cap color literal).
function cssVar(name: string, fallback = ""): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

/// Paleta ANSI derivada dels tokens: els 16 colors apunten a la jerarquia
/// de fons/text i als accents semàntics del tema actiu.
function xtermTheme(): ITheme {
  const bg = cssVar("--bg-1", cssVar("--bg-0"));
  return {
    background: bg,
    foreground: cssVar("--text-1", cssVar("--text-0")),
    cursor: cssVar("--accent-2", cssVar("--text-0")),
    cursorAccent: bg,
    selectionBackground: cssVar("--bg-4", cssVar("--bg-3")),
    black: cssVar("--bg-0", bg),
    red: cssVar("--error"),
    green: cssVar("--success"),
    yellow: cssVar("--warning"),
    blue: cssVar("--accent"),
    magenta: cssVar("--accent-hover", cssVar("--accent")),
    cyan: cssVar("--accent-2"),
    white: cssVar("--text-1", cssVar("--text-0")),
    brightBlack: cssVar("--text-3", cssVar("--text-2")),
    brightRed: cssVar("--error"),
    brightGreen: cssVar("--success"),
    brightYellow: cssVar("--warning"),
    brightBlue: cssVar("--accent-hover", cssVar("--accent")),
    brightMagenta: cssVar("--accent-hover", cssVar("--accent")),
    brightCyan: cssVar("--accent-2"),
    brightWhite: cssVar("--text-0"),
  };
}

/// Crea (o recupera) el host d'una sessió. Idempotent: cridar-lo en cada
/// render no perd scrollback ni duplica subscripcions.
export function ensureHost(id: string): XtermHost {
  const existing = hosts.get(id);
  if (existing) return existing;

  const el = document.createElement("div");
  el.className = "xterm-host";
  const term = new Terminal({
    scrollback: 5000,
    fontFamily: cssVar("--font-mono", "monospace"),
    fontSize: 12,
    lineHeight: 1.2,
    cursorBlink: true,
    theme: xtermTheme(),
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(el);

  // stdin interactiu: cada tecla (incloent \r, Ctrl+C, fletxes…) va al PTY.
  term.onData((data) => useTerminalStore.getState().write(id, data));

  // Copy estàndard: ⌘C / Ctrl+C AMB selecció copia al porta-retalls;
  // sense selecció deixa passar la tecla (SIGINT). El paste el gestiona
  // l'xterm de forma nativa (event «paste» del seu textarea ocult).
  term.attachCustomKeyEventHandler((ev) => {
    if (
      ev.type === "keydown" &&
      (ev.metaKey || ev.ctrlKey) &&
      ev.key.toLowerCase() === "c" &&
      term.hasSelection()
    ) {
      void navigator.clipboard.writeText(term.getSelection()).catch(() => undefined);
      term.clearSelection();
      return false;
    }
    return true;
  });

  // Sortida en streaming. Subscripció ABANS del replay: els trossos que
  // arriben mentre es demana el buffer es posen en cua i s'aboquen després
  // (l'ordre es conserva; un tros pot duplicar-se si arriba just entre la
  // subscripció i la foto del backend — finestra de pocs ms, cosmètic).
  const queue: string[] = [];
  let replaying = true;
  const un = onSessionOutput(id, (data) => {
    if (replaying) queue.push(data);
    else term.write(data);
  });
  unsubs.set(id, un);

  const host: XtermHost = { term, fit, el };
  hosts.set(id, host);

  // Repintat del buffer del backend (clau en reenganxar la finestra
  // separada: la sessió ha sobreviscut amb la seva sortida recent).
  void invoke<string>("term_scrollback", { id })
    .then((buf) => {
      if (buf) term.write(buf);
    })
    .catch(() => undefined)
    .finally(() => {
      replaying = false;
      for (const chunk of queue) term.write(chunk);
      queue.length = 0;
    });

  return host;
}

export function getHost(id: string): XtermHost | undefined {
  return hosts.get(id);
}

/// Identificadors amb host viu (per netejar els de sessions tancades).
export function hostIds(): string[] {
  return Array.from(hosts.keys());
}

/// Ajusta l'xterm al seu contenidor (debounced des del panell) i propaga
/// la mida al PTY (SIGWINCH). Només si el slot és visible i té mida.
export function fitHost(id: string): void {
  const host = hosts.get(id);
  if (!host) return;
  if (!host.el.isConnected || host.el.clientWidth <= 0 || host.el.clientHeight <= 0) return;
  try {
    host.fit.fit();
  } catch {
    /* contenidor sense mida encara: el proper resize ho tornarà a provar */
  }
  useTerminalStore.getState().resize(id, host.term.cols, host.term.rows);
}

/// Destrueix el host d'una sessió tancada (allibera renderer i subscripció).
export function disposeHost(id: string): void {
  const host = hosts.get(id);
  if (!host) return;
  unsubs.get(id)?.();
  unsubs.delete(id);
  hosts.delete(id);
  host.term.dispose();
  host.el.remove();
}

/// Repinta el tema de tots els hosts (event «theme://changed»).
export function refreshTheme(): void {
  const theme = xtermTheme();
  for (const host of hosts.values()) {
    host.term.options.theme = theme;
  }
}