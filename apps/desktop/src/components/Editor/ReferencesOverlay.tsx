/// Llista navegable de REFERÈNCIES reals del servidor LSP (⌥F12, Fase 3c).
/// Fletxes ↑/↓ per moure's, ↵ per saltar, Esc per tancar; clic també salta.
/// Estils: classes compartides del menú contextual + tokens de theme.css.
import { useEffect, useState } from "react";
import { FileCode, X } from "lucide-react";
import { useT } from "../../i18n";
import { useReferencesStore } from "../../stores/referencesStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { revealAt } from "../../editor/editorApi";
import type { LspRef } from "../../editor/lspBridge";

export default function ReferencesOverlay() {
  const { t } = useT();
  const open = useReferencesStore((s) => s.open);
  const symbol = useReferencesStore((s) => s.symbol);
  const items = useReferencesStore((s) => s.items);
  const close = useReferencesStore((s) => s.close);
  const root = useWorkspaceStore((s) => s.root);
  const [sel, setSel] = useState(0);

  // Cada consulta nova comença amb la primera referència seleccionada.
  useEffect(() => {
    setSel(0);
  }, [items]);

  const jump = (ref: LspRef) => {
    if (!root) return;
    const abs = ref.path.startsWith("/") ? ref.path : `${root}/${ref.path}`;
    const ws = useWorkspaceStore.getState();
    const already = ws.openFiles.some((f) => f.path === abs);
    close();
    void ws.loadFile(abs).then(() => {
      // Mateix criteri que «Ves a la definició»: marge perquè el model es
      // munte quan el fitxer no era obert.
      setTimeout(() => revealAt(ref.line, ref.col, 0), already ? 0 : 350);
    });
  };

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setSel((i) => Math.min(items.length - 1, i + 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSel((i) => Math.max(0, i - 1));
      } else if (e.key === "Enter") {
        e.preventDefault();
        const ref = items[sel];
        if (ref) jump(ref);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, items, sel, root]);

  if (!open) return null;

  return (
    <div className="ctx-overlay" onClick={close} onContextMenu={(e) => e.preventDefault()}>
      <div className="refs-panel" onClick={(e) => e.stopPropagation()}>
        <div className="refs-head">
          <FileCode size={13} />
          <span>
            {t("editor.referencesTitle")}
            {symbol ? `: ${symbol}` : ""} · {items.length}
          </span>
          <div style={{ flex: 1 }} />
          <button className="icon-btn" title={t("editor.referencesClose")} onClick={close}>
            <X size={13} />
          </button>
        </div>
        {items.length === 0 ? (
          <div className="refs-empty">{t("editor.referencesEmpty")}</div>
        ) : (
          <div className="refs-list" role="listbox">
            {items.map((ref, i) => (
              <button
                key={`${ref.path}:${ref.line}:${ref.col}:${i}`}
                role="option"
                aria-selected={i === sel}
                className={"ctx-row refs-row" + (i === sel ? " active" : "")}
                onMouseEnter={() => setSel(i)}
                onClick={() => jump(ref)}
              >
                <FileCode size={12} />
                <span className="refs-name">{ref.path.split(/[/\\]/).pop()}</span>
                <span className="refs-path">{ref.path}</span>
                <span className="refs-pos">
                  {ref.line}:{ref.col}
                </span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}