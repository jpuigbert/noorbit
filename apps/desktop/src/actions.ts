/// Registre central d'accions de l'editor, com la llista de comandaments
/// de VS Code. El fan servir la barra de menús, la paleta de comandes
/// (⇧⌘P) i les dreceres de teclat.
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as ed from "./editor/editorApi";
import { saveCurrentFile, useWorkspaceStore } from "./stores/workspaceStore";
import { useUIStore } from "./stores/uiStore";
import { usePreviewStore } from "./stores/previewStore";

export const actions: Record<string, () => void | Promise<void>> = {
  // Fitxer
  "file.new": () => useWorkspaceStore.getState().newFile(),
  "file.newProject": () => useUIStore.getState().setShowNewProject(true),
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
  "view.wordWrap": ed.toggleWordWrap,
  "view.minimap": ed.toggleMinimap,
  "view.zoomIn": () => useUIStore.getState().zoomIn(),
  "view.zoomOut": () => useUIStore.getState().zoomOut(),
  "view.zoomReset": () => useUIStore.getState().zoomReset(),
  "view.settings": () => useUIStore.getState().setShowSettings(true),

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
  // ⌘J: terminal integrat (panell inferior), com a VS Code.
  "terminal.toggle": () => useUIStore.getState().toggleTerminal(),

  // Git (obre la pestanya de control de canvis del panell dret)
  "git.open": () => {
    useUIStore.getState().setRightPanelOpen(true);
    usePreviewStore.getState().setRightTab("git");
  },

  // IA (pròpi de NoOrbit, com el xat de Qoder)
  "ai.models": () => useUIStore.getState().setShowModelManager(true),
  "ai.catalog": () => useUIStore.getState().setShowIaCatalog(true),
  "ai.plugins": () => useUIStore.getState().setShowPlugins(true),
  "ai.agent": () => usePreviewStore.getState().setRightTab("agent"),
  "ai.computer": () => usePreviewStore.getState().setRightTab("computer"),
  "ai.experts": () => usePreviewStore.getState().setRightTab("experts"),
  "ai.unreal": () => usePreviewStore.getState().setRightTab("unreal"),
  "ai.blender": () => usePreviewStore.getState().setRightTab("blender"),

  // Temes de colors (tota la pantalla + editor)
  "view.themeDark": () => useUIStore.getState().setTheme("dark"),
  "view.themeLight": () => useUIStore.getState().setTheme("light"),
  "view.themeGray": () => useUIStore.getState().setTheme("gray"),
  "view.themeGreen": () => useUIStore.getState().setTheme("green"),
  "view.themeYellow": () => useUIStore.getState().setTheme("yellow"),

  // Ajuda
  "help.manual": () => useUIStore.getState().setShowHelp(true),
};

export type ActionId = keyof typeof actions;

export interface CommandDef {
  id: ActionId;
  labelKey: string; // clau i18n
  kbd?: string;
}

/// Llista que es mostra a la paleta de comandes (⇧⌘P).
export const commandList: CommandDef[] = [
  { id: "file.new", labelKey: "menu.newFile", kbd: "⌘N" },
  { id: "file.newProject", labelKey: "menu.newProject", kbd: "⇧⌘N" },
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
  { id: "ai.plugins", labelKey: "plugins.title" },
  { id: "ai.agent", labelKey: "rightPanel.agent" },
  { id: "ai.computer", labelKey: "rightPanel.computer" },
  { id: "ai.experts", labelKey: "rightPanel.experts" },
  { id: "ai.unreal", labelKey: "menu.unreal" },
  { id: "ai.blender", labelKey: "menu.blender" },
  { id: "view.themeDark", labelKey: "settings.themeDark" },
  { id: "view.themeLight", labelKey: "settings.themeLight" },
  { id: "view.themeGray", labelKey: "settings.themeGray" },
  { id: "view.themeGreen", labelKey: "settings.themeGreen" },
  { id: "view.themeYellow", labelKey: "settings.themeYellow" },
  { id: "help.manual", labelKey: "menu.manual" },
];
