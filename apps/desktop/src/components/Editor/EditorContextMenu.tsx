/// Menú contextual PROPI de l'editor (clic dret). Monaco porta el seu menú
/// en anglès i sense localització catalana disponible; el desactivem amb
/// `contextmenu: false` i pintem aquest, amb les mateixes accions del pont
/// `editorApi` (actuen sobre l'editor del grup amb focus, que el clic dret
/// acaba de seleccionar) més el salt a definició (LSP + heurístic) i
/// l'edició de la selecció amb la IA.
import {
  Undo2,
  Redo2,
  Scissors,
  Copy,
  ClipboardPaste,
  Search,
  Replace,
  LocateFixed,
  Braces,
  Sparkles,
  SquareMousePointer,
  PencilLine,
} from "lucide-react";
import { useRef, useState } from "react";
import { useT } from "../../i18n";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import * as ed from "../../editor/editorApi";

/// Salt a la definició del símbol sota el cursor: mateix camí que el F12
/// (LSP real primer, heurístic del backend després) i mateix patró de
/// càrrega del fitxer destí.
async function gotoDefinition() {
  const s = useWorkspaceStore.getState();
  const f =
    s.focusedGroup === "split" && s.splitPath
      ? s.openFiles.find((x) => x.path === s.splitPath)
      : s.openFile;
  if (!f) return;
  const r = await ed.findDefinitionHere(f.path, s.root);
  if (!r.ok || !r.path || r.line == null) return;
  const same = r.path === f.path;
  if (!same) await s.loadFile(r.path);
  setTimeout(() => ed.revealAt(r.line!, r.column ?? 1, 0), same ? 0 : 350);
}

export default function EditorContextMenu({
  x,
  y,
  onClose,
  onAiSel,
}: {
  x: number;
  y: number;
  onClose: () => void;
  onAiSel: () => void;
}) {
  const { t } = useT();
  // Renombrament del símbol: en clicar «Reanomena el símbol», el menú es
  // converteix en un camp de text (preomplert amb la paraula del cursor).
  // Enter aplica; Esc torna al menú.
  const [renaming, setRenaming] = useState(false);
  const [renErr, setRenErr] = useState<string | null>(null);
  const renInput = useRef<HTMLInputElement>(null);

  const row = (icon: React.ReactNode, label: string, act: () => void) => (
    <button
      className="ctx-row"
      onClick={() => {
        onClose();
        act();
      }}
    >
      {icon} {label}
    </button>
  );

  const beginRename = () => {
    setRenaming(true);
    setRenErr(null);
  };

  const execRename = async (raw: string) => {
    const s = useWorkspaceStore.getState();
    const f =
      s.focusedGroup === "split" && s.splitPath
        ? s.openFiles.find((o) => o.path === s.splitPath)
        : s.openFile;
    const nou = raw.trim();
    if (!nou || !f) {
      setRenaming(false);
      return;
    }
    const r = await ed.renameSymbolHere(f.path, nou, s.root);
    if (!r.ok) {
      setRenErr(t("editor.ctxRenameFail"));
      return;
    }
    // Els fitxers que el backend ha tocat es tornen a llegir del disc: els
    // models oberts encara tenen el símbol vell.
    const rel = new Set((r.paths ?? []).map((p) => p.replace(/\\/g, "/")));
    const arrel = (s.root ?? "").replace(/\/+$/, "");
    for (const o of s.openFiles) {
      const orel = arrel ? o.path.replace(arrel, "").replace(/^\//, "") : o.path;
      if (rel.has(o.path) || rel.has(orel)) void s.loadFile(o.path, { force: true });
    }
    onClose();
  };

  return (
    <div
      className="ctx-overlay"
      onClick={onClose}
      onContextMenu={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
      <div
        className="ctx-menu"
        style={{
          left: Math.min(x, window.innerWidth - 240),
          top: Math.min(y, window.innerHeight - 340),
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {renaming ? (
          <div
            className="ctx-rename"
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => e.stopPropagation()}
          >
            <input
              ref={renInput}
              autoFocus
              className="ctx-rename-input"
              placeholder={t("editor.ctxRenamePh")}
              defaultValue={
                (() => {
                  const e = ed.getEditor();
                  const pos = e?.getPosition();
                  return pos ? e?.getModel()?.getWordAtPosition(pos)?.word ?? "" : "";
                })()
              }
              onFocus={(e) => e.currentTarget.select()}
              onKeyDown={(e) => {
                if (e.key === "Enter") void execRename(e.currentTarget.value);
                if (e.key === "Escape") {
                  setRenaming(false);
                  setRenErr(null);
                }
              }}
            />
            {renErr && <div className="lp-hint" style={{ marginTop: 6 }}>{renErr}</div>}
            <div className="ctx-rename-hint">{t("editor.ctxRenameHint")}</div>
          </div>
        ) : (
          <>
            {row(<Undo2 size={13} />, t("editor.ctxUndo"), ed.undo)}
            {row(<Redo2 size={13} />, t("editor.ctxRedo"), ed.redo)}
            <div className="ctx-sep" />
            {row(<Scissors size={13} />, t("editor.ctxCut"), ed.cut)}
            {row(<Copy size={13} />, t("editor.ctxCopy"), ed.copy)}
            {row(<ClipboardPaste size={13} />, t("editor.ctxPaste"), ed.paste)}
            {row(<SquareMousePointer size={13} />, t("editor.ctxSelectAll"), ed.selectAll)}
            <div className="ctx-sep" />
            {row(<Search size={13} />, t("editor.ctxFind"), ed.find)}
            {row(<Replace size={13} />, t("editor.ctxReplace"), ed.replace)}
            <div className="ctx-sep" />
            {row(<LocateFixed size={13} />, t("editor.ctxGotoDef"), () => void gotoDefinition())}
            <button className="ctx-row" onClick={beginRename}>
              <PencilLine size={13} /> {t("editor.ctxRename")}
            </button>
            {row(<Braces size={13} />, t("editor.ctxFormat"), ed.formatDocument)}
            <div className="ctx-sep" />
            {row(<Sparkles size={13} />, t("editor.ctxAiSel"), onAiSel)}
          </>
        )}
      </div>
    </div>
  );
}
