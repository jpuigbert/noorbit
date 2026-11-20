/// Registre central d'accions de l'editor, com la llista de comandaments
/// de VS Code. El fan servir la barra de menús, la paleta de comandes
/// (⇧⌘P) i les dreceres de teclat.
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import * as ed from "./editor/editorApi";
import { saveCurrentFile, useWorkspaceStore } from "./stores/workspaceStore";
import { useUIStore } from "./stores/uiStore";
import { toastSuccess, toastError, toastInfo } from "./stores/toastStore";
import { usePreviewStore } from "./stores/previewStore";
import { openVideoStudio } from "./stores/videoStudioStore";
import { detachChat, detachTerminal, reattachChat, reattachTerminal } from "./windowing";

const rawActions = {
  // Fitxer
  "file.new": () => useWorkspaceStore.getState().newFile(),
  // Zip — empaquetar temes WordPress per pujar-los al marketplace.
  "zip.wpTheme": async () => {
    const dir = await open({ directory: true, title: "Tria la carpeta del tema WordPress" });
    if (typeof dir !== "string") return;
    try {
      const info = await invoke<{
        name: string;
        errors: string[];
      }>("theme_inspect", { dir });
      if (info.errors && info.errors.length > 0) {
        toastError("El tema no està a punt: " + info.errors.join(" "));
        return;
      }
      const [zip] = await invoke<[string, string]>("theme_build_zip", { dir });
      await invoke("reveal_in_finder", { path: zip }).catch(() => undefined);
      toastSuccess("ZIP creat: " + (zip.split("/").pop() ?? zip));
    } catch (e) {
      toastError("No s'ha pogut crear el ZIP", e);
    }
  },
  "file.newProject": () => useUIStore.getState().setShowNewProject(true),
  // Menú «Crear tema de WordPress»: la IA en crea els fitxers inicials.
  "file.newWpTheme": () => useUIStore.getState().setShowWpTheme(true),
  "file.openFile": () => void useWorkspaceStore.getState().openFilePicker(),
  "file.openFolder": () => void useWorkspaceStore.getState().pickWorkspace(),
  "file.save": () => void saveCurrentFile(),
  "file.saveAs": () => void useWorkspaceStore.getState().saveAs(),
  "file.saveAll": () => void useWorkspaceStore.getState().saveAll(),
  "file.close": () => useWorkspaceStore.getState().closeFile(),
  "file.exit": () => void getCurrentWindow().close(),

  // Edició
  "edit.undo": ed.undo,
  "edit.redo": ed.redo,
  "edit.cut": ed.cut,
  "edit.copy": ed.copy,
  "edit.paste": ed.paste,
  "edit.find": ed.find,
  "edit.replace": ed.replace,
  "edit.selectAll": ed.selectAll,
  "edit.format": ed.formatDocument,

  // Selecció
  "selection.nextOccurrence": ed.selectNextOccurrence,
  "selection.columnMode": ed.toggleColumnSelection,

  // Vista
  "view.palette": () => useUIStore.getState().setShowCommandPalette(true),
  "view.quickOpen": () => useUIStore.getState().setShowQuickOpen(true),
  "view.searchProject": () => useUIStore.getState().setShowSearch(true),
  "view.sidebar": () => useUIStore.getState().toggleSidebar(),
  "view.rightPanel": () => useUIStore.getState().toggleRightPanel(),
  // Xat en FINESTRA SEPARADA (posable a qualsevol pantalla); si ja hi és,
  // el torna a enganxar al programa.
  "view.detachChat": () =>
    void (useUIStore.getState().chatDetached ? reattachChat() : detachChat()),
  "view.wordWrap": ed.toggleWordWrap,
  "view.minimap": ed.toggleMinimap,
  // Cursor: «divertit» (es disfressa segons on apuntes) o «clàssic» (el
  // puntador normal de l'ordinador). El menú Vista i la paleta de comandes
  // oferisquen els dos; l'ajust es recorda entre sessions (uiStore).
  "view.cursorFun": () => useUIStore.getState().setFunCursor(true),
  "view.cursorClassic": () => useUIStore.getState().setFunCursor(false),
  "view.cursorToggle": () =>
    useUIStore.getState().setFunCursor(!useUIStore.getState().funCursor),
  "view.zoomIn": () => useUIStore.getState().zoomIn(),
  "view.zoomOut": () => useUIStore.getState().zoomOut(),
  "view.zoomReset": () => useUIStore.getState().zoomReset(),
  "view.settings": () => useUIStore.getState().setShowSettings(true),
  "account.open": () => useUIStore.getState().setShowAccount(true),

  // Ves
  "goto.file": () => useUIStore.getState().setShowQuickOpen(true),
  "goto.line": ed.gotoLine,
  "goto.symbol": ed.goToSymbol,
  // F12: «Ves a la definició» heurístic (busca patrons de definició al projecte).
  "goto.definition": async () => {
    const f = useWorkspaceStore.getState().openFile;
    if (!f) return;
    const r = await ed.findDefinitionHere(f.path);
    if (!r.ok || !r.path || r.line == null) return;
    const same = r.path === f.path;
    if (!same) await useWorkspaceStore.getState().loadFile(r.path);
    setTimeout(() => ed.revealAt(r.line!, r.column ?? 1, 0), same ? 0 : 350);
  },

  // Cerca global: navegació entre coincidències (F3 / ⇧F3).
  "search.nextMatch": ed.goNextMatch,
  "search.prevMatch": ed.goPrevMatch,

  // Executa
  "run.preview": () => void usePreviewStore.getState().startPreview(),
  "run.stopPreview": () => void usePreviewStore.getState().stopPreview(),

  // Terminal
  "terminal.new": () =>
    void invoke("open_terminal", { path: useWorkspaceStore.getState().root }).catch(
      () => undefined
    ),
  // ⌘J: terminal integrat (panell inferior), com a VS Code. Si el terminal
  // està en finestra separada, ⌘J el torna al programa.
  "terminal.toggle": () => {
    const ui = useUIStore.getState();
    if (ui.terminalDetached) void reattachTerminal();
    else ui.toggleTerminal();
  },
  // Terminal en FINESTRA SEPARADA (posable a qualsevol pantalla).
  "terminal.detach": () =>
    void (useUIStore.getState().terminalDetached ? reattachTerminal() : detachTerminal()),

  // Git (obre la pestanya de control de canvis del panell dret)
  "git.open": () => {
    useUIStore.getState().setRightPanelOpen(true);
    usePreviewStore.getState().setRightTab("dev");
  },

  // IA (pròpi de NoOrbit, com el xat de Qoder)
  "ai.models": () => useUIStore.getState().setShowModelManager(true),
  "ai.catalog": () => useUIStore.getState().setShowIaCatalog(true),
  // Menú «Hugging Bay»: cercar i baixar models GGUF del mirror verificat.
  "ai.huggingBay": () => useUIStore.getState().setShowHuggingBay(true),
  // Menú «Pujar»: publicar temes WordPress al marketplace.
  "ai.publishTheme": () => useUIStore.getState().setShowPublishTheme(true),
  "ai.remoteServer": () => useUIStore.getState().setShowRemoteServer(true),
  // Tornar a veure l'assistent de benvinguda (3 passos) en qualsevol moment:
  // apareix a Ajuda ▸ … i també es pot cercar amb ⌘P («benvinguda»).
  "view.onboarding": () => useUIStore.getState().setShowOnboarding(true),
  "ai.plugins": () => useUIStore.getState().setShowPlugins(true),
  // Importa les extensions instal·lades de VS Code (i variants) com a skills.
  "ai.vscodeImport": async () => {
    const rep = await invoke<{ scanned: number; imported: number; details: string[] }>(
      "vscode_import_skills"
    );
    if (rep.imported > 0) {
      toastSuccess("Importades " + rep.imported + " extensions de VS Code com a skills.");
    } else {
      toastInfo(rep.details.join(" · ") || "Cap extensió de VS Code trobada.");
    }
  },
  "ai.vscodeExtensions": () => useUIStore.getState().setShowVsCodeExtensions(true),
  "ai.gameStudios": () => useUIStore.getState().setShowGameStudios(true),
  // Millora autopilotada de QUALSEVOL projecte (temps de treball personalitzable).
  "ai.projectWork": () => useUIStore.getState().setShowProjectWork(true),
  // Panell «Jam» (agent autònom): es viu DINS de l'Agent. Cal obrir també el
  // panell dret: si era plegat, canviar de pestanya no el tornava a mostrar.
  "ai.jam": () => {
    useUIStore.getState().setRightPanelOpen(true);
    usePreviewStore.getState().setRightTab("agent");
    usePreviewStore.getState().setAgentSub("jam");
  },
  "ai.agent": () => {
    useUIStore.getState().setRightPanelOpen(true);
    usePreviewStore.getState().setRightTab("agent");
  },
  // Sub-pestanyes DINS de la pestanya Agent (les «Integracions» ja no són
  // una pestanya a part: sempre es veuen servides al costat del xat).
  "ai.computer": () => {
    useUIStore.getState().setRightPanelOpen(true);
    usePreviewStore.getState().setRightTab("agent");
    usePreviewStore.getState().setAgentSub("computer");
  },
  "ai.experts": () => {
    useUIStore.getState().setRightPanelOpen(true);
    usePreviewStore.getState().setRightTab("agent");
    usePreviewStore.getState().setAgentSub("experts");
  },
  "ai.unreal": () => {
    usePreviewStore.getState().setRightTab("agent");
    usePreviewStore.getState().setAgentSub("unreal");
  },
  "ai.blender": () => {
    usePreviewStore.getState().setRightTab("agent");
    usePreviewStore.getState().setAgentSub("blender");
  },

  // ── MENÚ «VÍDEO» (nou, independent del d'«IA») ─────────────────────────
  // Muntatge professional Blender → Kdenlive/ffmpeg dirigit per IA. Cada
  // entrada obre el panell nou i, quan toca, demana la feina AL MOTOR real
  // (el mateix que la IA fa servir des del xat amb `@video:`).
  "video.panel": () => openVideoStudio((v) => useUIStore.getState().setShowVideoStudio(v)),
  "video.scan": () =>
    openVideoStudio((v) => useUIStore.getState().setShowVideoStudio(v), "scan"),
  // La IA ho fa TOT: mira, escriu el guió, es critica i renderitza (40 min).
  "video.director": () =>
    openVideoStudio((v) => useUIStore.getState().setShowVideoStudio(v), "director"),
  "video.auto": () =>
    openVideoStudio((v) => useUIStore.getState().setShowVideoStudio(v), "auto"),
  "video.preview": () =>
    openVideoStudio((v) => useUIStore.getState().setShowVideoStudio(v), "preview"),
  "video.render": () =>
    openVideoStudio((v) => useUIStore.getState().setShowVideoStudio(v), "render"),
  "video.export": () =>
    openVideoStudio((v) => useUIStore.getState().setShowVideoStudio(v), "export"),
  "video.kdenlive": () =>
    openVideoStudio((v) => useUIStore.getState().setShowVideoStudio(v), "kdenlive"),

  // Temes de colors (tota la pantalla + editor)
  "view.themeDark": () => useUIStore.getState().setTheme("dark"),
  "view.themeLight": () => useUIStore.getState().setTheme("light"),
  "view.themeGray": () => useUIStore.getState().setTheme("gray"),
  "view.themeGreen": () => useUIStore.getState().setTheme("green"),
  "view.themeGold": () => useUIStore.getState().setTheme("gold"),
  "view.themeYellow": () => useUIStore.getState().setTheme("yellow"),

  // Ajuda
  "help.manual": () => useUIStore.getState().setShowHelp(true),
};

/// Totes les accions passen per aquest embolcall: cap error pot quedar sense
/// gestionar (abans, un rebuig en «desar» o «ves a la definició» es perdia
/// i l'usuari veia que «res passava»).
export const actions: Record<string, () => Promise<void>> = Object.fromEntries(
  Object.entries(rawActions).map(([id, fn]) => [
    id,
    async () => {
      try {
        await fn();
      } catch (e) {
        toastError("L'acció «" + id + "» ha fallat", e);
      }
    },
  ])
) as Record<string, () => Promise<void>>;

export type ActionId = keyof typeof rawActions;

export interface CommandDef {
  id: ActionId;
  labelKey: string; // clau i18n
  kbd?: string;
}

/// Llista que es mostra a la paleta de comandes (⇧⌘P).
export const commandList: CommandDef[] = [
  { id: "file.new", labelKey: "menu.newFile", kbd: "⌘N" },
  { id: "file.newProject", labelKey: "menu.newProject", kbd: "⇧⌘N" },
  { id: "file.newWpTheme", labelKey: "menu.newWpTheme" },
  { id: "file.openFile", labelKey: "menu.openFile", kbd: "⌘O" },
  { id: "file.openFolder", labelKey: "menu.openFolder", kbd: "⌘K ⌘O" },
  { id: "file.save", labelKey: "menu.save", kbd: "⌘S" },
  { id: "file.saveAs", labelKey: "menu.saveAs", kbd: "⇧⌘S" },
  { id: "file.saveAll", labelKey: "menu.saveAll" },
  { id: "file.close", labelKey: "menu.closeEditor", kbd: "⌘W" },
  { id: "edit.undo", labelKey: "menu.undo", kbd: "⌘Z" },
  { id: "edit.redo", labelKey: "menu.redo", kbd: "⇧⌘Z" },
  { id: "edit.cut", labelKey: "menu.cut", kbd: "⌘X" },
  { id: "edit.copy", labelKey: "menu.copy", kbd: "⌘C" },
  { id: "edit.paste", labelKey: "menu.paste", kbd: "⌘V" },
  { id: "edit.find", labelKey: "menu.find", kbd: "⌘F" },
  { id: "edit.replace", labelKey: "menu.replace", kbd: "⌥⌘F" },
  { id: "edit.selectAll", labelKey: "menu.selectAll", kbd: "⌘A" },
  { id: "edit.format", labelKey: "menu.format" },
  { id: "selection.nextOccurrence", labelKey: "menu.selectNext", kbd: "⇧⌘D" },
  { id: "selection.columnMode", labelKey: "menu.columnSelection" },
  { id: "view.quickOpen", labelKey: "menu.gotoFile", kbd: "⌘P" },
  { id: "view.searchProject", labelKey: "menu.searchProject", kbd: "⇧⌘F" },
  { id: "view.sidebar", labelKey: "menu.toggleSidebar", kbd: "⌘B" },
  { id: "view.rightPanel", labelKey: "menu.toggleRightPanel", kbd: "⌥⌘J" },
  { id: "view.wordWrap", labelKey: "menu.wordWrap", kbd: "⌥Z" },
  { id: "view.minimap", labelKey: "menu.minimap" },
  { id: "view.cursorClassic", labelKey: "menu.cursorClassic" },
  { id: "view.cursorFun", labelKey: "menu.cursorFun" },
  { id: "view.zoomIn", labelKey: "menu.zoomIn", kbd: "⌘=" },
  { id: "view.zoomOut", labelKey: "menu.zoomOut", kbd: "⌘-" },
  { id: "view.zoomReset", labelKey: "menu.zoomReset", kbd: "⌘0" },
  { id: "view.settings", labelKey: "settings.title", kbd: "⌘," },
  { id: "goto.line", labelKey: "menu.gotoLine", kbd: "⌃G" },
  { id: "goto.symbol", labelKey: "menu.gotoSymbol", kbd: "⇧⌘O" },
  { id: "goto.definition", labelKey: "menu.gotoDefinition", kbd: "F12" },
  { id: "search.nextMatch", labelKey: "menu.nextMatch", kbd: "F3" },
  { id: "search.prevMatch", labelKey: "menu.prevMatch", kbd: "⇧F3" },
  { id: "run.preview", labelKey: "menu.runPreview" },
  { id: "run.stopPreview", labelKey: "menu.stopPreview" },
  { id: "terminal.new", labelKey: "menu.newTerminal", kbd: "^⌘`" },
  { id: "terminal.toggle", labelKey: "menu.toggleTerminal", kbd: "⌘J" },
  { id: "git.open", labelKey: "menu.gitPanel" },
  { id: "ai.models", labelKey: "ai.manager" },
  { id: "ai.catalog", labelKey: "ai.catalog" },
  { id: "ai.huggingBay", labelKey: "menu.huggingBay" },
  { id: "ai.publishTheme", labelKey: "menu.publishTheme" },
  { id: "zip.wpTheme", labelKey: "menu.zipWpTheme" },
  { id: "ai.remoteServer", labelKey: "menu.remoteServer" },
  { id: "view.onboarding", labelKey: "menu.replayOnboarding" },
  { id: "ai.plugins", labelKey: "plugins.title" },
  { id: "ai.vscodeImport", labelKey: "menu.vscodeImport" },
  { id: "ai.vscodeExtensions", labelKey: "menu.vscodeExtensions" },
  { id: "ai.gameStudios", labelKey: "menu.gameStudios" },
  { id: "ai.projectWork", labelKey: "menu.projectWork" },
  { id: "ai.jam", labelKey: "menu.jam" },
  { id: "ai.agent", labelKey: "rightPanel.agent" },
  { id: "ai.computer", labelKey: "rightPanel.computer" },
  { id: "ai.experts", labelKey: "rightPanel.experts" },
  { id: "ai.unreal", labelKey: "menu.unreal" },
  { id: "ai.blender", labelKey: "menu.blender" },
  { id: "video.panel", labelKey: "menu.videoPanel" },
  { id: "video.director", labelKey: "menu.videoDirector" },
  { id: "video.scan", labelKey: "menu.videoScan" },
  { id: "video.auto", labelKey: "menu.videoAuto" },
  { id: "video.preview", labelKey: "menu.videoPreview" },
  { id: "video.render", labelKey: "menu.videoRender" },
  { id: "video.export", labelKey: "menu.videoExport" },
  { id: "video.kdenlive", labelKey: "menu.videoKdenlive" },
  { id: "account.open", labelKey: "account.title" },
  { id: "view.themeDark", labelKey: "settings.themeDark" },
  { id: "view.themeLight", labelKey: "settings.themeLight" },
  { id: "view.themeGray", labelKey: "settings.themeGray" },
  { id: "view.themeGreen", labelKey: "settings.themeGreen" },
  { id: "view.themeGold", labelKey: "settings.themeGold" },
  { id: "view.themeYellow", labelKey: "settings.themeYellow" },
  { id: "help.manual", labelKey: "menu.manual" },
];
