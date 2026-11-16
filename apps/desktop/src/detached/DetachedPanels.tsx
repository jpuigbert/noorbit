/// Mini-app per a les FINESTRES DE PANELLS SEPARATS (etiquetes
/// «noorbit-chat» i «noorbit-terminal»): main.tsx la tria segons l'etiqueta
/// de la finestra i aquí es pinta NOMÉS el panell necessari, amb una barra
/// pròpia per tornar-lo al programa. La finestra es pot posar a qualsevol
/// pantalla; la geometria es recorda a «windowing.ts» en tancar.
import React, { useEffect } from "react";
import ReactDOM from "react-dom/client";
import { ExternalLink, Terminal as TerminalIcon } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useT } from "../i18n";
import { applyTheme, loadTheme } from "../stores/uiStore";
import { alignChatBucket } from "../stores/agentStore";
import { closePanelWindow, CHAT_LABEL, TERM_LABEL } from "../windowing";
import { useFunCursor } from "../utils/useFunCursor";
import { initFunFx } from "../utils/funFx";
import AgentPanel from "../components/Agent/AgentPanel";
import TerminalPanel from "../components/Terminal/TerminalPanel";

/// Una finestra separada es tanca amb la seva creu: interceptem la petició
/// per desar la geometria i avisar el programa principal (event
/// «panel://closed») ABANS de destruir-la.
function useCloseToReattach(label: string) {
  useEffect(() => {
    const win = getCurrentWebviewWindow();
    let un: (() => void) | undefined;
    let disposed = false;
    void win
      .onCloseRequested((e) => {
        e.preventDefault();
        void closePanelWindow(win.label);
      })
      .then((u) => {
        // StrictMode munta dues vegades: sense aquest guard, l'unlisten
        // es perdria i el handler quedaria duplicat (vegeu App.tsx).
        if (disposed) u();
        else un = u;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      un?.();
    };
  }, [label]);
}

/// Marc comú de les finestres separades: títol + botó de reenganxar.
function DetachedShell({ label, icon, title, children }: React.PropsWithChildren<{ label: string; icon: React.ReactNode; title: string }>) {
  const { t } = useT();
  useCloseToReattach(label);
  // El cursor divertit i el so de tasca acabada també viuen a les
  // finestres separades: mateixa experiència que al programa principal.
  useFunCursor();
  return (
    <div className="detached-app">
      <div className="detached-head">
        <span className="detached-title">
          {icon} {title}
        </span>
        <button
          className="btn sm"
          title={t("panel.reattach")}
          onClick={() => void closePanelWindow(label)}
        >
          <ExternalLink size={12} /> {t("panel.reattach")}
        </button>
      </div>
      <div className="detached-body">{children}</div>
    </div>
  );
}

function DetachedChat() {
  const { t } = useT();
  return (
    <DetachedShell label={CHAT_LABEL} icon={<ExternalLink size={13} />} title={t("panel.chatTitle")}>
      <AgentPanel />
    </DetachedShell>
  );
}

function DetachedTerminal() {
  const { t } = useT();
  return (
    <DetachedShell label={TERM_LABEL} icon={<TerminalIcon size={13} />} title={t("panel.terminalTitle")}>
      <TerminalPanel detached />
    </DetachedShell>
  );
}

/// Punt d'entrada des de main.tsx: pinta la mini-app que toca. El projecte
/// del calaix de xats ja s'ha establert a «workspaceStore» ABANS d'importar
/// aquest mòdul (l'agentStore el llegeix en inicialitzar-se).
export function mountDetached(kind: "chat" | "terminal") {
  applyTheme(loadTheme());
  alignChatBucket();
  // Si l'usuari canvia el tema al programa principal, aquesta finestra ha
  // de seguir-lo (és un webview independent amb el seu propi estat). El tema
  // nou ja és al localStorage compartit: el rellegim d'allà.
  void listen("theme://changed", () => applyTheme(loadTheme())).catch(() => undefined);
  initFunFx();
  const el = document.getElementById("root");
  if (!el) return;
  ReactDOM.createRoot(el).render(
    <React.StrictMode>{kind === "chat" ? <DetachedChat /> : <DetachedTerminal />}</React.StrictMode>
  );
}
