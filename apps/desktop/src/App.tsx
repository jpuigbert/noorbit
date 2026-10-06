import { useEffect } from "react";
import "./App.css";
import { useUIStore, applyTheme } from "./stores/uiStore";
import { useWorkspaceStore } from "./stores/workspaceStore";
import { useUnrealStore } from "./stores/unrealStore";
import { useBlenderLiveStore } from "./stores/blenderLiveStore";
import { registerProcessListener } from "./stores/processStore";
import { registerTaskListeners } from "./stores/taskStore";
import { useComputerStore } from "./stores/computerStore";
import { installConsoleCapture, logBackend } from "./stores/consoleStore";
import { listen } from "@tauri-apps/api/event";
import { actions } from "./actions";
import MenuBar from "./components/MenuBar";
import StatusBar from "./components/StatusBar";
import Sidebar from "./components/Sidebar";
import EditorPane from "./components/Editor/EditorPane";
import RightPanel from "./components/RightPanel/RightPanel";
import ModelManagerModal from "./components/AI/ModelManagerModal";
import IaCatalogModal from "./components/AI/IaCatalogModal";
import PluginManager from "./components/Plugins/PluginManager";
import SettingsModal from "./components/Settings/SettingsModal";
import HelpModal from "./components/Help/HelpModal";
import ComputerConfirmModal from "./components/Computer/ComputerConfirmModal";
import CommandPalette from "./components/CommandPalette";
import QuickOpen from "./components/QuickOpen";
import SearchPalette from "./components/SearchPalette";
import TerminalPanel from "./components/Terminal/TerminalPanel";
import ReviewPill from "./components/Editor/ReviewPill";
import { markForPath, resolveMark } from "./stores/editReviewStore";
import NewProjectModal from "./components/NewProjectModal";
import StartupScreen from "./components/StartupScreen";

/// Autodesat (com l'«Auto Save» de VS Code): desa el fitxer obert poc
/// després de l'últim canvi i també quan la finestra perd el focus.
function useAutosave() {
  useEffect(() => {
    let timer: number | undefined;

    const flushSave = () => {
      const f = useWorkspaceStore.getState().openFile;
      if (!f || f.dirty === false || f.path.startsWith("untitled:")) return;
      void useWorkspaceStore.getState().saveFile(f.path, f.content).catch(() => undefined);
    };

    const unsub = useWorkspaceStore.subscribe((state, prev) => {
      const ui = useUIStore.getState();
      if (!ui.autoSave) return;
      const cur = state.openFile;
      const old = prev.openFile;

      // Canvi de fitxer: desa l'anterior si era brut
      if (old && old.dirty && !old.path.startsWith("untitled:") && cur?.path !== old.path) {
        void useWorkspaceStore.getState().saveFile(old.path, old.content).catch(() => undefined);
      }

      // contingut modificat → desa poc després de l'últim canvi
      if (cur && cur.dirty && !cur.path.startsWith("untitled:") && cur.content !== old?.content) {
        window.clearTimeout(timer);
        timer = window.setTimeout(flushSave, ui.autoSaveDelay);
      }
    });

    // En perdre el focus de la finestra, desa de seguida (focusChange)
    const onBlur = () => {
      if (!useUIStore.getState().autoSave) return;
      window.clearTimeout(timer);
      flushSave();
    };
    window.addEventListener("blur", onBlur);

    return () => {
      unsub();
      window.clearTimeout(timer);
      window.removeEventListener("blur", onBlur);
    };
  }, []);
}

export default function App() {
  const sidebarOpen = useUIStore((s) => s.sidebarOpen);
  const rightPanelOpen = useUIStore((s) => s.rightPanelOpen);
  const zoom = useUIStore((s) => s.zoom);
  const theme = useUIStore((s) => s.theme);

  useAutosave();

  // Arrencada: inicialitza integracions i mostra la pantalla d'inici
  // (projectes recents) quan no hi ha cap carpeta oberta.
  useEffect(() => {
    useUnrealStore.getState().init();
    useBlenderLiveStore.getState().init();
    // Escoltadors globals: el procés de qualsevol IA (ai://process) i
    // l'estat de les tasques degradades a segon pla (task://state), perquè
    // funcionen encara que el seu panell no estiga obert ara mateix.
    registerProcessListener();
    registerTaskListeners();
    // El control de l'ordinador: els seus escoltadors (computer://confirm, 
    // …/agent, …/executed) i permisos s'activen DES DE L'ARRANCAIDA, no només
    // quan el panell «Ordinador» està obert. Si no, una comanda «RUN|» del xat
    // que demane confirmació no trobaria cap listener: el modal no eixiria i
    // l'execució caducaria (l'usuari es quedaria «sense accés a l'ordinador»).
    void useComputerStore.getState().setupEvents();
    void useComputerStore.getState().load();
    // Consola de depuració: para els console.* i els errors del webview, i
    // aboca els passos de l'agent (i els seus fallos) perquè l'usuari puga
    // veure QUÈ ha fallat quan una comanda o generació no ix bé.
    installConsoleCapture();
    void listen<{ label: string; detail: string; ok: boolean }>("agent://progress", (e) => {
      const { label, detail, ok } = e.payload;
      logBackend(label, detail, ok ? "info" : "error");
    });
    if (!useWorkspaceStore.getState().root) {
      useUIStore.getState().setShowStartup(true);
    }
  }, []);

  // Zoom de tota la interfície (com ⌘+ / ⌘- de VS Code)
  useEffect(() => {
    document.documentElement.style.setProperty("zoom", String(zoom));
  }, [zoom]);

  // Tema de colors: s'aplica a l'arrel del document i se'n dona avís
  // perquè l'editor (Monaco) canviï el seu tema en conseqüència.
  useEffect(() => {
    applyTheme(theme);
    window.dispatchEvent(new CustomEvent("noorbit:theme", { detail: theme }));
  }, [theme]);

  // Dreceres de teclat globals
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      // Sense modificador només passen tecles de funció: F1 (paleta),
      // F3/⇧F3 (coincidències) i F12 (ves a la definició).
      if (!meta && !/^F(1|3|12)$/.test(e.key)) return;
      const k = e.key.toLowerCase();

      if (e.key === "F1") {
        e.preventDefault();
        actions["view.palette"]();
        return;
      }
      if (e.key === "F3") {
        // F3/⇧F3: coincidència següent/anterior. Sense sessió de cerca,
        // goNextMatch obri el cercador intern de Monaco (que llavors
        // gestiona ell les pulsacions si l'editor te el focus).
        e.preventDefault();
        void (e.shiftKey ? actions["search.prevMatch"]() : actions["search.nextMatch"]());
        return;
      }
      if (e.key === "F12" && !meta) {
        e.preventDefault();
        void actions["goto.definition"]();
        return;
      }

      if (meta && k === "enter") {
        // ⌘↵: accepta el canvi de la IA que s'està revisant (com a VS Code).
        const path = useWorkspaceStore.getState().openFile?.path;
        const mark = path ? markForPath(path) : undefined;
        if (mark && !mark.streaming) {
          e.preventDefault();
          void resolveMark(mark.path, "accept");
          return;
        }
      }
      if (meta && (k === "backspace" || k === "delete")) {
        // ⌘⌫: rebutja el canvi i torna el codi anterior de la IA.
        const path = useWorkspaceStore.getState().openFile?.path;
        const mark = path ? markForPath(path) : undefined;
        if (mark) {
          e.preventDefault();
          void resolveMark(mark.path, "reject");
          return;
        }
      }

      if (meta && e.shiftKey && k === "p") {
        e.preventDefault();
        actions["view.palette"]();
      } else if (meta && e.shiftKey && k === "f") {
        // ⇧⌘F: cerca de text a TOT el projecte (estil VS Code).
        e.preventDefault();
        actions["view.searchProject"]();
      } else if (meta && e.shiftKey && k === "s") {
        e.preventDefault();
        actions["file.saveAs"]();
      } else if (meta && e.altKey && k === "s") {
        e.preventDefault();
        actions["file.saveAll"]();
      } else if (meta && e.shiftKey && k === "o") {
        e.preventDefault();
        actions["goto.symbol"]();
      } else if (meta && k === "s") {
        e.preventDefault();
        actions["file.save"]();
      } else if (meta && e.shiftKey && k === "n") {
        e.preventDefault();
        actions["file.newProject"]();
      } else if (meta && k === "n") {
        e.preventDefault();
        actions["file.new"]();
      } else if (meta && k === "o") {
        e.preventDefault();
        actions["file.openFile"]();
      } else if (meta && k === "p") {
        e.preventDefault();
        actions["view.quickOpen"]();
      } else if (meta && k === "w") {
        e.preventDefault();
        actions["file.close"]();
      } else if (meta && k === "b") {
        e.preventDefault();
        actions["view.sidebar"]();
      } else if (meta && e.altKey && k === "r") {
        // ⌘⌥R: obrir/canviar entre projectes recents.
        e.preventDefault();
        useUIStore.getState().setShowStartup(true);
      } else if (meta && e.altKey && k === "j") {
        // ⌥⌘J: panell dret (l'acordió de IA/eines).
        e.preventDefault();
        actions["view.rightPanel"]();
      } else if (meta && k === "j") {
        // ⌘J: terminal integrat, com a VS Code.
        e.preventDefault();
        actions["terminal.toggle"]();
      } else if (meta && k === ",") {
        e.preventDefault();
        actions["view.settings"]();
      } else if (meta && e.ctrlKey && k === "`") {
        e.preventDefault();
        actions["terminal.new"]();
      } else if (meta && (k === "=" || k === "+")) {
        e.preventDefault();
        actions["view.zoomIn"]();
      } else if (meta && k === "-") {
        e.preventDefault();
        actions["view.zoomOut"]();
      } else if (meta && k === "0") {
        e.preventDefault();
        actions["view.zoomReset"]();
      } else if (meta && e.ctrlKey && k === "g") {
        e.preventDefault();
        actions["goto.line"]();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="app">
      <MenuBar />
      <div className="app-body">
        {sidebarOpen && <Sidebar />}
        <div className="app-main">
          <EditorPane />
          <ReviewPill />
        </div>
        {rightPanelOpen && <RightPanel />}
      </div>
      <TerminalPanel />
      <StatusBar />

      <ModelManagerModal />
      <IaCatalogModal />
      <PluginManager />
      <SettingsModal />
      <HelpModal />
      <ComputerConfirmModal />
      <CommandPalette />
      <QuickOpen />
      <SearchPalette />
      <NewProjectModal />
      <StartupScreen />
    </div>
  );
}
