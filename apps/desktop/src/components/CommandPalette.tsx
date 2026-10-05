import { useEffect, useMemo, useRef, useState } from "react";
import { useT } from "../i18n";
import { actions, commandList } from "../actions";
import { useUIStore } from "../stores/uiStore";

/// Paleta de comandes estil VS Code (⇧⌘P): cercar i executar comandaments.
export default function CommandPalette() {
  const show = useUIStore((s) => s.showCommandPalette);
  const setShow = useUIStore((s) => s.setShowCommandPalette);
  const { t } = useT();
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const items = useMemo(() => {
    const all = commandList.map((c) => ({ ...c, label: t(c.labelKey) }));
    if (!q.trim()) return all;
    const needle = q.toLowerCase();
    return all.filter((c) => c.label.toLowerCase().includes(needle));
  }, [q, t]);

  useEffect(() => {
    if (show) {
      setQ("");
      setSel(0);
      setTimeout(() => inputRef.current?.focus(), 30);
    }
  }, [show]);

  if (!show) return null;

  const runAt = (i: number) => {
    const item = items[i];
    if (item) actions[item.id]?.();
    setShow(false);
  };

  return (
    <div className="modal-overlay" onClick={() => setShow(false)}>
      <div className="modal cp-palette" onClick={(e) => e.stopPropagation()}>
        <input
          ref={inputRef}
          className="cp-palette-input"
          placeholder={t("palette.placeholder")}
          value={q}
          onChange={(e) => {
            setQ(e.target.value);
            setSel(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setSel((s) => Math.min(items.length - 1, s + 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setSel((s) => Math.max(0, s - 1));
            } else if (e.key === "Enter") {
              e.preventDefault();
              runAt(sel);
            } else if (e.key === "Escape") {
              setShow(false);
            }
          }}
        />
        <div className="cp-palette-list">
          {items.map((c, i) => (
            <button
              key={c.id}
              className={"cp-palette-row" + (i === sel ? " selected" : "")}
              onMouseEnter={() => setSel(i)}
              onClick={() => runAt(i)}
            >
              <span>{c.label}</span>
              {c.kbd && <span className="kbd">{c.kbd}</span>}
            </button>
          ))}
          {items.length === 0 && <div className="cp-empty">{t("palette.empty")}</div>}
        </div>
      </div>
    </div>
  );
}
