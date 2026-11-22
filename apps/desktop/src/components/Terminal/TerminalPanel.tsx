import { useEffect, useRef } from "react";
import { useShallow } from "zustand/react/shallow";
import {
  ChevronDown,
  Eraser,
  ExternalLink,
  Plus,
  Terminal as TerminalIcon,
  X,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useTerminalStore } from "../../stores/terminalStore";
import { closePanelWindow, detachTerminal, TERM_LABEL } from "../../windowing";
import { disposeHost, ensureHost, fitHost, getHost, hostIds, refreshTheme } from "./xtermHosts";

/// Terminal integrat REAL (Fase 3b): sessions PTY multi-pestanya amb xterm.
/// Cada pestanya és una shell interactiva de l'usuari (stdin real: npm init,
/// composer, php -a…); les instàncies xterm viuen fora de React i NO es
/// desmunten en canviar de pestanya (scrollback de 5000 línies intacte).
/// ⌘J l'obre/tanca. Amb «detached» viu en una FINESTRA SEPARADA: la sessió
/// PTY és del backend i sobreviu al detach/reattach (es repinta amb el
/// buffer de `term_scrollback`); el botó de tancar el torna al programa.
export default function TerminalPanel({ detached = false }: { detached?: boolean }) {
  const { t } = useT();
  const openStore = useUIStore((s) => s.terminalOpen);
  const setOpen = useUIStore((s) => s.setTerminalOpen);
  const open = detached || openStore;
  const root = useWorkspaceStore((s) => s.root);
  const { sessions, activeId, init, createSession, closeSession, setActive } =
    useTerminalStore(
      useShallow((st) => ({
        sessions: st.sessions,
        activeId: st.activeId,
        init: st.init,
        createSession: st.createSession,
        closeSession: st.closeSession,
        setActive: st.setActive,
      }))
    );

  /// Slots DOM per sessió: TOTS muntats sempre; el no actiu es oculta amb
  /// display:none (l'xterm amagat conserva buffer i scrollback).
  const slotsRef = useRef(new Map<string, HTMLDivElement>());

  // En obrir: connecta events, sincronitza sessions existents (finestra
  // separada o reattach) i crea la primera sessió si no n'hi ha cap.
  useEffect(() => {
    if (!open) return;
    void init().then(() => useTerminalStore.getState().bootstrap(root ?? ""));
  }, [open, init, root]);

  // Munta els hosts xterm als seus slots i neteja els de sessions tancades.
  useEffect(() => {
    if (!open) return;
    const alive = new Set(sessions.map((s) => s.id));
    for (const id of hostIds()) {
      if (!alive.has(id)) disposeHost(id);
    }
    for (const s of sessions) {
      const slot = slotsRef.current.get(s.id);
      if (!slot) continue;
      const host = ensureHost(s.id);
      if (host.el.parentElement !== slot) slot.appendChild(host.el);
    }
    if (activeId) {
      // Després del layout: el slot visible ja té mida real.
      const raf = requestAnimationFrame(() => {
        fitHost(activeId);
        getHost(activeId)?.term.focus();
      });
      return () => cancelAnimationFrame(raf);
    }
    return undefined;
  }, [open, sessions, activeId]);

  // Resize debounced del slot actiu → addon-fit → term_resize (SIGWINCH).
  useEffect(() => {
    if (!open || !activeId) return;
    const slot = slotsRef.current.get(activeId);
    if (!slot) return;
    let timer: number | undefined;
    const ro = new ResizeObserver(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => fitHost(activeId), 120);
    });
    ro.observe(slot);
    return () => {
      window.clearTimeout(timer);
      ro.disconnect();
    };
  }, [open, activeId, sessions.length]);

  // Canvi de tema del programa (o de la finestra separada): repinta els
  // xterm amb els tokens nous. Guard de StrictMode per no duplicar.
  useEffect(() => {
    if (!open) return;
    let un: (() => void) | undefined;
    let disposed = false;
    void listen("theme://changed", () => refreshTheme())
      .then((u) => {
        if (disposed) u();
        else un = u;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      un?.();
    };
  }, [open]);

  if (!open) return null;

  return (
    <div
      className="terminal-panel"
      /* En finestra separada omple la finestra (la mida la dona l'usuari
         movent la finestra, no la nansa del panell inferior). */
      style={detached ? { height: "100%", minHeight: 0 } : undefined}
    >
      <div className="terminal-head">
        <span className="terminal-title">
          <TerminalIcon size={12} /> {t("terminal.title")}
        </span>
        <div className="terminal-tools">
          {!detached && (
            <button
              className="btn sm"
              onClick={() => void detachTerminal()}
              title={t("panel.detachTerminal")}
            >
              <ExternalLink size={12} />
            </button>
          )}
          <button
            className="btn sm"
            onClick={() => activeId && getHost(activeId)?.term.clear()}
            title={t("terminal.clear")}
          >
            <Eraser size={12} />
          </button>
          <button
            className="btn sm"
            onClick={() => {
              // En finestra separada, tancar = tornar al programa principal
              // (les sessions PTY NO es maten: sobreviuen al reattach).
              if (detached) void closePanelWindow(TERM_LABEL);
              else setOpen(false);
            }}
            title={detached ? t("panel.reattach") : t("terminal.close")}
          >
            <ChevronDown size={12} />
          </button>
        </div>
      </div>
      <div className="terminal-tabs" role="tablist">
        {sessions.map((s) => (
          <div
            key={s.id}
            role="tab"
            tabIndex={0}
            aria-selected={s.id === activeId}
            className={
              "terminal-tab" + (s.id === activeId ? " active" : "") + (s.alive ? "" : " dead")
            }
            title={s.title}
            onClick={() => setActive(s.id)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                setActive(s.id);
              }
            }}
          >
            <span className="terminal-tab-name">
              {s.title}
              {!s.alive && <span className="terminal-tab-exit"> · {t("terminal.exited")}</span>}
            </span>
            <button
              className="terminal-tab-close"
              title={t("terminal.closeSession")}
              onClick={(e) => {
                e.stopPropagation();
                void closeSession(s.id);
              }}
            >
              <X size={11} />
            </button>
          </div>
        ))}
        <button
          className="btn sm terminal-new"
          onClick={() => void createSession(root ?? "")}
          title={t("terminal.newSession")}
        >
          <Plus size={12} />
        </button>
      </div>
      <div className="terminal-xterm-wrap">
        {sessions.length === 0 && <div className="terminal-empty">{t("terminal.hint")}</div>}
        {sessions.map((s) => (
          <div
            key={s.id}
            className="terminal-xterm-slot"
            style={{ display: s.id === activeId ? "block" : "none" }}
            ref={(el) => {
              if (el) slotsRef.current.set(s.id, el);
              else slotsRef.current.delete(s.id);
            }}
          />
        ))}
      </div>
    </div>
  );
}