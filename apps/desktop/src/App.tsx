import { lazy, Suspense, useEffect, useState, type CSSProperties } from "react";
import "./App.css";
import { useUIStore, applyTheme } from "./stores/uiStore";
import { useWorkspaceStore } from "./stores/workspaceStore";
import { useUnrealStore } from "./stores/unrealStore";
import { useBlenderLiveStore } from "./stores/blenderLiveStore";
import { registerProcessListener } from "./stores/processStore";
import { registerTaskListeners } from "./stores/taskStore";
import { useComputerStore } from "./stores/computerStore";
import { useVideoStudioStore } from "./stores/videoStudioStore";
import { installConsoleCapture, logBackend } from "./stores/consoleStore";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { actions } from "./actions";
import { useT } from "./i18n";
import MenuBar from "./components/MenuBar";
import StatusBar from "./components/StatusBar";
import Sidebar from "./components/Sidebar";
import EditorPane from "./components/Editor/EditorPane";
import RightPanel from "./components/RightPanel/RightPanel";
import PanelErrorBoundary from "./components/PanelErrorBoundary";
import ResizeHandle from "./components/ResizeHandle";
import { initPanelWindowSync } from "./windowing";
// Panells GRANS AMB LAZY: el seu codi és un chunk apart que només baixa quan
// el flag que els obre és actiu per primer cop (arrencada més ràpida i poc
// JS per analitzar). Els diàlegs mitjans (⌘P, quick open…) queden estàtics:
// s'han d'obrir a l'instant.
const ModelManagerModal = lazy(() => import("./components/AI/ModelManagerModal"));
const IaCatalogModal = lazy(() => import("./components/AI/IaCatalogModal"));
const PluginManager = lazy(() => import("./components/Plugins/PluginManager"));
const SettingsModal = lazy(() => import("./components/Settings/SettingsModal"));
const HelpModal = lazy(() => import("./components/Help/HelpModal"));
const AccountModal = lazy(() => import("./components/Account/AccountModal"));
const NewProjectModal = lazy(() => import("./components/NewProjectModal"));
const RemoteServerModal = lazy(() => import("./components/RemoteServerModal"));
const OnboardingModalComponent = lazy(() => import("./components/OnboardingModal"));
/// Detecció de l'entorn a l'obrir: si falta Ollama/Blender, pregunta si els
/// vols instal·lar (instal·lació automàtica nativa del sistema operatiu).
import SetupCheckModal from "./components/SetupCheckModal";
import HuggingBayModal from "./components/AI/HuggingBayModal";
import PublishThemeModal from "./components/Publish/PublishThemeModal";
const WordPressThemeModal = lazy(() => import("./components/WordPress/WordPressThemeModal"));
const ProjectWorkModal = lazy(() => import("./components/ProjectWork/ProjectWorkModal"));
// Menú «Vídeo» propi: muntatge Blender → Kdenlive/ffmpeg dirigit per IA.
const VideoStudioModal = lazy(() => import("./components/VideoStudio/VideoStudioModal"));
const VsCodeExtensionsModal = lazy(() => import("./components/Extensions/VsCodeExtensionsModal"));
const GameStudiosModal = lazy(() => import("./components/GameStudios/GameStudiosModal"));
// Informe de fallida («crash»): només es carrega quan l'usuari el demana.
const CrashReportModal = lazy(() => import("./components/CrashReportModal"));
import ComputerConfirmModal from "./components/Computer/ComputerConfirmModal";
// Permisos de l'agent ACP (GitHub Copilot): sempre muntat perquè la
// petició aparegui encara que l'usuari sigui a una altra pestanya.
import AcpPermissionModal from "./components/Agent/AcpPermissionModal";
import CommandPalette from "./components/CommandPalette";
import QuickOpen from "./components/QuickOpen";
import SearchPalette from "./components/SearchPalette";
import TerminalPanel from "./components/Terminal/TerminalPanel";
import ReviewPill from "./components/Editor/ReviewPill";
import { markForPath, resolveMark } from "./stores/editReviewStore";
import { getEditor } from "./editor/editorApi";
import { useFunCursor } from "./utils/useFunCursor";
import { useActivityPulse } from "./utils/useActivityPulse";
import { initFunFx } from "./utils/funFx";
import StartupScreen from "./components/StartupScreen";
import Toaster from "./components/Toaster";
import { fetchCrashReport } from "./crashReport";
/// Autodesat (com l'«Auto Save» de VS Code): desa TOTES les pestanyes
/// brutes poc després de l'últim canvi i també quan la finestra perd el focus.
function useAutosave() {
  useEffect(() => {
    let timer: number | undefined;

    const flushSave = () => {
      const ws = useWorkspaceStore.getState();
      for (const f of ws.openFiles) {
        if (!f.dirty || f.path.startsWith("untitled:")) continue;
        void ws.saveFile(f.path, f.content).catch(() => undefined);
      }
    };

    const unsub = useWorkspaceStore.subscribe((state, prev) => {
      const ui = useUIStore.getState();
      if (!ui.autoSave) return;
      // Qualsevol pestanya bruta amb contingut nou (o acabada de brutar)
      // rearma el temporitzador: un sol desat conjunt per a totes.
      let changed = false;
      for (const f of state.openFiles) {
        if (!f.dirty || f.path.startsWith("untitled:")) continue;
        const old = prev.openFiles.find((x) => x.path === f.path);
        if (!old || !old.dirty || old.content !== f.content) {
          changed = true;
          break;
        }
      }
      if (changed) {
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
  const sidebarWidth = useUIStore((s) => s.sidebarWidth);
  const rightPanelWidth = useUIStore((s) => s.rightPanelWidth);
  const terminalHeight = useUIStore((s) => s.terminalHeight);
  const terminalOpen = useUIStore((s) => s.terminalOpen);
  const terminalDetached = useUIStore((s) => s.terminalDetached);
  const zoom = useUIStore((s) => s.zoom);
  const theme = useUIStore((s) => s.theme);
  const { t } = useT();
  // Confirmació abans de tancar el programa: el sistema intercepta el
  // tancament, ens pregunta i només tanquem si l'usuari ho confirma
  // (deseant primer el que hi hagi de brut).
  const [askClose, setAskClose] = useState(false);

  useAutosave();
  // Cursor divertit: personatge SVG segons la zona de sota el punter,
  // disfresses que roten cada 5 minuts i mode «avorrit» quan no fas res.
  useFunCursor();
  // Pol d'activitat: diu al backend cada vegada que toques alguna cosa,
  // perquè l'aprenentatge autònom en repòs no navegui mentre treballes.
  useActivityPulse();

  // Finestres de panells separats: quan es tanquen (creu X o «Torna al
  // programa»), tornen a enganxar-se aquí sense reiniciar el programa.
  useEffect(() => {
    let un: (() => void) | undefined;
    let disposed = false;
    void initPanelWindowSync().then((u) => {
      if (disposed) u();
      else un = u;
    });
    return () => {
      disposed = true;
      un?.();
    };
  }, []);

  // Interceptem el tancament de la finestra (creu X, ⌘Q, tancar des de la
  // resta de finestres): preventDefault + modal propi. Sense plugin extern.
  useEffect(() => {
    let un: (() => void) | undefined;
    let disposed = false;
    void getCurrentWebviewWindow()
      .onCloseRequested((e) => {
        e.preventDefault();
        setAskClose(true);
      })
      .then((u) => {
        // Amb StrictMode el cleanup pot córrer ABANS que la promesa resolgui:
        // sense aquest guard, l'unlisten es perdia i el handler quedava viu
        // per sempre (duplicat a cada remuntatge).
        if (disposed) u();
        else un = u;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      un?.();
    };
  }, []);

  const closeNow = () => {
    setAskClose(false);
    // Desem tot abans de tancar, però SENSE permetre que un desat encallat
    // bloquegi el tancament per sempre (màxim 1,5 s d'espera).
    const save = Promise.resolve()
      .then(() => actions["file.saveAll"]())
      .catch(() => undefined);
    const timeout = new Promise((r) => setTimeout(r, 1500));
    void Promise.race([save, timeout]).then(() => {
      // Sortida del TOT del procés: només destruir la finestra deixaria
      // l'app viva al Dock sense cap finestra (comportament de macOS).
      void invoke("exit_app").catch(() => {
        // Sense l'ordre (config antiga): destruim la finestra al menys.
        void getCurrentWebviewWindow().destroy().catch(() => undefined);
      });
    });
  };

  // Arrencada: inicialitza integracions i mostra la pantalla d'inici
  // (projectes recents) quan no hi ha cap carpeta oberta.
  useEffect(() => {
    useUnrealStore.getState().init();
    useBlenderLiveStore.getState().init();
    // El muntatge de vídeo també escolta DES DE L'ARRANCAIDA: un render o un
    // director de 40 minuts pot haver començat des del xat (`@video:`) o des
    // del menú, i els seus avisos han d'arribar encara que el panell no s'haja
    // obert mai en aquesta sessió.
    useVideoStudioStore.getState().init();
    // Escoltadors globals: el procés de qualsevol IA (ai://process) i
    // l'estat de les tasques degradades a segon pla (task://state), perquè
    // funcionen encara que el seu panell no estiga obert ara mateix.
    registerProcessListener();
    registerTaskListeners();
    // Efectes de celebració: toc alegre + confeti + cursor de festa quan
    // una tasca acaba (idempotent: StrictMode no els duplica).
    initFunFx();
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
    // Escoltador global dels passos de l'agent. Es registra amb cleanup
    // (StrictMode munta dues vegades: sense cleanup, cada pas es registra
    // dos cops) i amb catch (un registre fallat no pot deixar el rebuig
    // sense gestionar).
    {
      let un: (() => void) | undefined;
      let disposed = false;
      void listen<{ label: string; detail: string; ok: boolean }>("agent://progress", (e) => {
        const { label, detail, ok } = e.payload ?? { label: "", detail: "", ok: true };
        logBackend(label, detail, ok ? "info" : "error");
      })
        .then((u) => {
          if (disposed) u();
          else un = u;
        })
        .catch(() => undefined);
      // La finestra es manté oberta tota la vida de l'app: el cleanup és
      // per remuntatges (StrictMode/HMR), no per tancament.
      window.addEventListener("beforeunload", () => un?.(), { once: true });
    }
    if (!useWorkspaceStore.getState().root) {
      useUIStore.getState().setShowStartup(true);
      // Restaura l'últim projecte actiu («noorbit.lastRoot»): un reload no ha
      // de deixar l'usuari a la pantalla de benvinguda. Si no n'hi ha o ja no
      // existeix, es queda la StartupScreen amb els recents, com sempre.
      void useWorkspaceStore.getState().restoreLastRoot().then((ok) => {
        if (ok) useUIStore.getState().setShowStartup(false);
      });
    }
    // Assistent de 3 passos NOMÉS a la primera arrencada real (es guarda quan
    // es completa; l'usuari també el pot obrir des de Configuració). La
    // comprovació va en línia (localStorage) i NO importa el modal, perquè
    // el modal es carregui ÀJUS com la resta de diàlegs grans.
    try {
      if (localStorage.getItem("noorbit.onboardingDone") !== "1") {
        useUIStore.getState().setShowOnboarding(true);
      }
    } catch {
      /* sense persistència: no el molestem cada cop */
    }
  }, []);

  // Zoom de tota la interfície (com ⌘+ / ⌘- de VS Code)
  useEffect(() => {
    document.documentElement.style.setProperty("zoom", String(zoom));
  }, [zoom]);

  // Si l'última sessió va acabar amb una caiguda, l'INFORME DE FALLIDA s'obre
  // sol: l'usuari veu què va passar i el pot copiar per enviar-lo, sense haver
  // de buscar cap fitxer pel sistema.
  useEffect(() => {
    let alive = true;
    void (async () => {
      const r = await fetchCrashReport();
      if (!alive || !r) return;
      if (r.previousPanics <= 0) return;
      // Només si la caiguda és recent (menys d'un dia): una fallida de fa
      // setmanes no ha d'interrompre l'arrencada de cada dia.
      if (r.lastCrashAgeMinutes != null && r.lastCrashAgeMinutes > 24 * 60) return;
      // No xoquem amb l'assistent de primera arrencada.
      try {
        if (localStorage.getItem("noorbit.onboardingDone") !== "1") return;
      } catch {
        /* sense persistència: continueu */
      }
      useUIStore.getState().setShowCrashReport(true);
    })();
    return () => {
      alive = false;
    };
  }, []);

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
        // ⌥F12: referències LSP reals; F12 sol: definició (heurística+LSP).
        void (e.altKey ? actions["goto.references"]() : actions["goto.definition"]());
        return;
      }

      if (meta && k === "enter") {
        // ⌘↵: accepta el canvi de la IA que s'està revisant (com a VS Code).
        const ws = useWorkspaceStore.getState();
        const path =
          ws.focusedGroup === "split" && ws.splitPath ? ws.splitPath : ws.openFile?.path;
        const mark = path ? markForPath(path) : undefined;
        if (mark && !mark.streaming) {
          e.preventDefault();
          void resolveMark(mark.path, "accept");
          return;
        }
      }
      if (meta && (k === "backspace" || k === "delete")) {
        // ⌘⌫: rebutja el canvi i torna el codi anterior de la IA.
        const ws = useWorkspaceStore.getState();
        const path =
          ws.focusedGroup === "split" && ws.splitPath ? ws.splitPath : ws.openFile?.path;
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
      } else if (meta && !e.shiftKey && !e.altKey && /^[1-9]$/.test(e.key)) {
        // ⌘1..9: salta a la pestanya n (com VS Code). ⌘0 continua sent el
        // reset de zoom. El focus torna a l'editor del grup actiu.
        const ws = useWorkspaceStore.getState();
        const tab = ws.openFiles[Number(e.key) - 1];
        if (tab) {
          e.preventDefault();
          ws.setActive(tab.path);
          setTimeout(() => getEditor()?.focus(), 0);
        }
      } else if (meta && e.key === "\\") {
        // ⌘\: divideix l'editor (o tanca la divisió), com VS Code.
        e.preventDefault();
        actions["view.splitEditor"]();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div
      className="app"
      /* Les mides de la disposició viatgen en variables CSS: els panells
         (sidebar, panell dret, terminal) les llegeixen del seu CSS i les
         nanses de redimensionat les canvien a través de l'uiStore. */
      style={{
        "--sidebar-w": sidebarWidth + "px",
        "--rp-w": rightPanelWidth + "px",
        "--term-h": terminalHeight + "px",
      } as CSSProperties}
    >
      <MenuBar />
      <div className="app-body">
        {sidebarOpen && <Sidebar />}
        {sidebarOpen && (
          <ResizeHandle
            axis="x"
            dir={1}
            onResize={(d) => useUIStore.getState().addSidebarWidth(d)}
            onReset={() => useUIStore.getState().setSidebarWidth(240)}
            tip={t("layout.resizeTip")}
          />
        )}
        <div className="app-main">
          {/* Cada zona gran va amb la seva pròpia xarxa de seguretat: si un
              panell peta, només mor AQUELL panell — la resta de NoOrbit
              (xat, editor, terminal) continua funcionant. */}
          <PanelErrorBoundary label="Error a l'editor">
            <EditorPane />
          </PanelErrorBoundary>
          <ReviewPill />
        </div>
        {rightPanelOpen && (
          <>
            <ResizeHandle
              axis="x"
              dir={-1}
              onResize={(d) => useUIStore.getState().addRightPanelWidth(d)}
              onReset={() => useUIStore.getState().setRightPanelWidth(380)}
              tip={t("layout.resizeTip")}
            />
            <PanelErrorBoundary label="Error al panell lateral">
              <RightPanel />
            </PanelErrorBoundary>
          </>
        )}
      </div>
      {terminalOpen && !terminalDetached && (
        <ResizeHandle
          axis="y"
          dir={-1}
          onResize={(d) => useUIStore.getState().addTerminalHeight(d)}
          onReset={() => useUIStore.getState().setTerminalHeight(220)}
          tip={t("layout.resizeTip")}
        />
      )}
      {/* El terminal integrat només viu ací quan NO està en finestra
          separada: en separar-lo, la seva UI passa a la finestra pròpia. */}
      {!terminalDetached && (
        <PanelErrorBoundary label="Error al terminal">
          <TerminalPanel />
        </PanelErrorBoundary>
      )}
      <StatusBar />
      {/* Avisos visibles: cap operació important pot fallar en silenci. */}
      <Toaster />

      {/* Modals HQ: només es MUNTEN quan el seu flag és actiu (ja no winyen
          cap byte del seu codi a l'arrencada, i el chunk baixa amb un clic). */}
      <Suspense fallback={null}>
        {useUIStore((s) => s.showModelManager) && <ModelManagerModal />}
        {useUIStore((s) => s.showIaCatalog) && <IaCatalogModal />}
        {useUIStore((s) => s.showPlugins) && <PluginManager />}
        {useUIStore((s) => s.showSettings) && <SettingsModal />}
        {useUIStore((s) => s.showHelp) && <HelpModal />}
                {useUIStore((s) => s.showCrashReport) && <CrashReportModal />}
        {useUIStore((s) => s.showAccount) && <AccountModal />}
        {useUIStore((s) => s.showNewProject) && <NewProjectModal />}
        {useUIStore((s) => s.showRemoteServer) && <RemoteServerModal />}
        {useUIStore((s) => s.showOnboarding) && <OnboardingModalComponent />}
        {/* Menú «Hugging Bay»: SEMPRE muntat perquè la cua de descàrregues
            sobrevisqui al tancament del diàleg. El component es mostra/oculta
            amb el seu estat «visible» intern (no es desmunta en tancar). */}
        <HuggingBayModal />
        {/* Menú «Pujar»: publicar temes WordPress al marketplace. */}
        {useUIStore((s) => s.showPublishTheme) && <PublishThemeModal />}
        {/* Menú «Crear tema de WordPress»: la IA en crea els fitxers inicials. */}
        {useUIStore((s) => s.showWpTheme) && <WordPressThemeModal />}
        {/* Menú «Treball autopilotat»: la IA millora qualsevol projecte un temps triat. */}
        {useUIStore((s) => s.showProjectWork) && <ProjectWorkModal />}
        {/* Menú «Vídeo»: muntatge professional dirigit per IA (un motor nou). */}
        {useUIStore((s) => s.showVideoStudio) && <VideoStudioModal />}
        {useUIStore((s) => s.showVsCodeExtensions) && <VsCodeExtensionsModal />}
        {useUIStore((s) => s.showGameStudios) && <GameStudiosModal />}
      </Suspense>
      <ComputerConfirmModal />
      <AcpPermissionModal />
      {/* Detecció d'entorn a l'obrir: es mostra SOL si falta algun component
          (i cada vegada, llevat que l'usuari premi «No tornar-la a mostrar»). */}
      <SetupCheckModal />
      <CommandPalette />
      <QuickOpen />
      <SearchPalette />
      <StartupScreen />

      {/* SEGUR QUE VOLS TANCAR? El tancament s'ha interceptat a
          onCloseRequested: només es tanca de veritat si es confirma ací. */}
      {askClose && (
        <div className="modal-overlay" onClick={() => setAskClose(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-head">
              <span>{t("app.closeAskTitle")}</span>
            </div>
            <div className="modal-body">
              <p className="lp-hint">{t("app.closeAskHint")}</p>
              <div className="mm-ollama-actions" style={{ justifyContent: "flex-end" }}>
                <button className="btn sm" onClick={() => setAskClose(false)}>
                  {t("common.cancel")}
                </button>
                <button className="btn sm primary" onClick={closeNow}>
                  {t("common.close")}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
