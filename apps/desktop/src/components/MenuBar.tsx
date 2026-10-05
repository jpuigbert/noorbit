import { useEffect, useRef, useState } from "react";
import { Loader2 } from "lucide-react";
import { useT } from "../i18n";
import { actions } from "../actions";
import { useAIActivity } from "../stores/activityStore";
import { useUIStore } from "../stores/uiStore";

interface Row {
  label: string;
  kbd?: string;
  action?: () => void;
  sep?: boolean;
}

/// Barra de menús amb la mateixa estructura que VS Code / Qoder:
/// Fitxer · Edició · Selecció · Vista · Ves · Executa · Terminal · IA · Ajuda
export default function MenuBar() {
  const { t } = useT();
  const [open, setOpen] = useState<string | null>(null);
  const barRef = useRef<HTMLDivElement>(null);
  const activity = useAIActivity();

  // tanca el desplegable en fer clic fora
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (barRef.current && !barRef.current.contains(e.target as Node)) setOpen(null);
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, []);

  const act = (id: string) => () => actions[id]?.();

  const menus: Record<string, Row[]> = {
    file: [
      { label: t("menu.newProject"), kbd: "⇧⌘N", action: act("file.newProject") },
      { label: t("menu.newFile"), kbd: "⌘N", action: act("file.new") },
      { label: t("menu.openFile"), kbd: "⌘O", action: act("file.openFile") },
      { label: t("menu.openFolder"), kbd: "⌘K ⌘O", action: act("file.openFolder") },
      {
        label: t("menu.recentProjects"),
        kbd: "⌥⌘R",
        action: () => useUIStore.getState().setShowStartup(true),
      },
      { sep: true, label: "" },
      { label: t("menu.save"), kbd: "⌘S", action: act("file.save") },
      { label: t("menu.saveAs"), kbd: "⇧⌘S", action: act("file.saveAs") },
      { label: t("menu.saveAll"), kbd: "⌥⌘S", action: act("file.saveAll") },
      { sep: true, label: "" },
      { label: t("menu.closeEditor"), kbd: "⌘W", action: act("file.close") },
      { sep: true, label: "" },
      { label: t("menu.exit"), kbd: "⌘Q", action: act("file.exit") },
    ],
    edit: [
      { label: t("menu.undo"), kbd: "⌘Z", action: act("edit.undo") },
      { label: t("menu.redo"), kbd: "⇧⌘Z", action: act("edit.redo") },
      { sep: true, label: "" },
      { label: t("menu.cut"), kbd: "⌘X", action: act("edit.cut") },
      { label: t("menu.copy"), kbd: "⌘C", action: act("edit.copy") },
      { label: t("menu.paste"), kbd: "⌘V", action: act("edit.paste") },
      { sep: true, label: "" },
      { label: t("menu.find"), kbd: "⌘F", action: act("edit.find") },
      { label: t("menu.replace"), kbd: "⌥⌘F", action: act("edit.replace") },
      { sep: true, label: "" },
      { label: t("menu.selectAll"), kbd: "⌘A", action: act("edit.selectAll") },
      { label: t("menu.format"), kbd: "⇧⌥F", action: act("edit.format") },
    ],
    selection: [
      { label: t("menu.selectAll"), kbd: "⌘A", action: act("edit.selectAll") },
      { label: t("menu.selectNext"), kbd: "⇧⌘D", action: act("selection.nextOccurrence") },
      { label: t("menu.columnSelection"), action: act("selection.columnMode") },
    ],
    view: [
      { label: t("menu.commandPalette"), kbd: "⇧⌘P", action: act("view.palette") },
      { label: t("menu.gotoFile"), kbd: "⌘P", action: act("view.quickOpen") },
      { sep: true, label: "" },
      { label: t("menu.toggleSidebar"), kbd: "⌘B", action: act("view.sidebar") },
      { label: t("menu.toggleRightPanel"), kbd: "⌥⌘J", action: act("view.rightPanel") },
      { sep: true, label: "" },
      { label: t("menu.wordWrap"), kbd: "⌥Z", action: act("view.wordWrap") },
      { label: t("menu.minimap"), action: act("view.minimap") },
      { sep: true, label: "" },
      { label: t("menu.zoomIn"), kbd: "⌘=", action: act("view.zoomIn") },
      { label: t("menu.zoomOut"), kbd: "⌘-", action: act("view.zoomOut") },
      { label: t("menu.zoomReset"), kbd: "⌘0", action: act("view.zoomReset") },
      { sep: true, label: "" },
      { label: t("settings.themeDark"), action: act("view.themeDark") },
      { label: t("settings.themeLight"), action: act("view.themeLight") },
      { label: t("settings.themeGray"), action: act("view.themeGray") },
      { label: t("settings.themeGreen"), action: act("view.themeGreen") },
      { label: t("settings.themeYellow"), action: act("view.themeYellow") },
      { sep: true, label: "" },
      { label: t("settings.title"), kbd: "⌘,", action: act("view.settings") },
    ],
    goto: [
      { label: t("menu.gotoFile"), kbd: "⌘P", action: act("goto.file") },
      { label: t("menu.gotoLine"), kbd: "⌃G", action: act("goto.line") },
      { label: t("menu.gotoSymbol"), kbd: "⇧⌘O", action: act("goto.symbol") },
      { label: t("menu.gotoDefinition"), kbd: "F12", action: act("goto.definition") },
    ],
    run: [
      { label: t("menu.runPreview"), kbd: "⌘R", action: act("run.preview") },
      { label: t("menu.stopPreview"), action: act("run.stopPreview") },
    ],
    terminal: [
      { label: t("menu.toggleTerminal"), kbd: "⌘J", action: act("terminal.toggle") },
      { label: t("menu.newTerminal"), kbd: "^⌘`", action: act("terminal.new") },
      { sep: true, label: "" },
      { label: t("menu.gitPanel"), action: act("git.open") },
    ],
    ai: [
      { label: t("ai.manager"), action: act("ai.models") },
      { label: t("plugins.title"), action: act("ai.plugins") },
      { sep: true, label: "" },
      { label: t("rightPanel.agent"), action: act("ai.agent") },
      { label: t("rightPanel.computer"), action: act("ai.computer") },
      { label: t("rightPanel.experts"), action: act("ai.experts") },
      { label: t("menu.unreal"), action: act("ai.unreal") },
      { label: t("menu.blender"), action: act("ai.blender") },
    ],
    help: [
      { label: t("menu.manual"), action: act("help.manual") },
      { sep: true, label: "" },
      { label: t("menu.about"), action: act("view.settings") },
    ],
  };

  const titles: Record<string, string> = {
    file: t("menu.file"),
    edit: t("menu.edit"),
    selection: t("menu.selection"),
    view: t("menu.view"),
    goto: t("menu.goto"),
    run: t("menu.run"),
    terminal: t("menu.terminal"),
    ai: t("menu.ai"),
    help: t("menu.help"),
  };

  const renderMenu = (key: string) => (
    <div className="menu-item" onMouseEnter={() => open && setOpen(key)}>
      <button onClick={() => setOpen(open === key ? null : key)}>{titles[key]}</button>
      {open === key && (
        <div className="menu-dropdown">
          {menus[key].map((r, i) =>
            r.sep ? (
              <div key={i} className="menu-sep" />
            ) : (
              <button
                key={i}
                className="menu-row"
                onClick={() => {
                  r.action?.();
                  setOpen(null);
                }}
              >
                <span>{r.label}</span>
                {r.kbd && <span className="kbd">{r.kbd}</span>}
              </button>
            )
          )}
        </div>
      )}
    </div>
  );

  return (
    <div className="menubar" ref={barRef}>
      <div className="brand">
        <span className="logo" />
        {t("app.name")}
      </div>
      {Object.keys(titles).map(renderMenu)}
      <div className="spacer" />
      {activity.working && (
        <div className="menubar-activity" title={activity.detail ?? undefined}>
          <Loader2 size={12} className="spin" />
          <span>{t(activity.labelKey)}</span>
        </div>
      )}
    </div>
  );
}
