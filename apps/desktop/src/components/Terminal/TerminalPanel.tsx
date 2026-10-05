import { useEffect, useRef, useState } from "react";
import { ChevronDown, Eraser, Terminal as TerminalIcon } from "lucide-react";
import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";
import { useTerminalStore } from "../../stores/terminalStore";

/// Terminal integrat (panell inferior, com VS Code). Executa comandes dins
/// la carpeta del projecte obert i mostra la sortida mentre va arribant.
/// ⌘J l'obre/tanca; ↑/↓ naveguen l'historial.
export default function TerminalPanel() {
  const { t } = useT();
  const open = useUIStore((s) => s.terminalOpen);
  const setOpen = useUIStore((s) => s.setTerminalOpen);
  const { lines, running, history, run, clear } = useTerminalStore();

  const [value, setValue] = useState("");
  const [histIdx, setHistIdx] = useState(-1);
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Auto-scroll a l'última línia quan arriba sortida nova.
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines]);

  useEffect(() => {
    if (open) {
      const id = setTimeout(() => inputRef.current?.focus(), 30);
      return () => clearTimeout(id);
    }
    return undefined;
  }, [open]);

  if (!open) return null;

  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && !running) {
      e.preventDefault();
      run(value);
      setValue("");
      setHistIdx(-1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (history.length === 0) return;
      const i = histIdx < 0 ? history.length - 1 : Math.max(0, histIdx - 1);
      setHistIdx(i);
      setValue(history[i]);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (histIdx < 0) return;
      const i = histIdx + 1;
      if (i >= history.length) {
        setHistIdx(-1);
        setValue("");
      } else {
        setHistIdx(i);
        setValue(history[i]);
      }
    }
  };

  return (
    <div className="terminal-panel">
      <div className="terminal-head">
        <span className="terminal-title">
          <TerminalIcon size={12} /> {t("terminal.title")}
          {running && <span className="terminal-running">{t("terminal.running")}</span>}
        </span>
        <div className="terminal-tools">
          <button className="btn sm" onClick={clear} title={t("terminal.clear")}>
            <Eraser size={12} />
          </button>
          <button
            className="btn sm"
            onClick={() => setOpen(false)}
            title={t("terminal.close")}
          >
            <ChevronDown size={12} />
          </button>
        </div>
      </div>
      <div className="terminal-output" ref={scrollRef}>
        {lines.length === 0 && <div className="terminal-empty">{t("terminal.hint")}</div>}
        {lines.map((l) => (
          <div key={l.id} className={"term-line term-" + l.kind}>
            {l.kind === "cmd" ? "$ " : ""}
            {l.text}
          </div>
        ))}
        {running && <div className="term-line term-out term-caret">…</div>}
      </div>
      <div className="terminal-input-row">
        <span className="terminal-prompt">$</span>
        <input
          ref={inputRef}
          className="terminal-input"
          value={value}
          placeholder={running ? t("terminal.running") : t("terminal.placeholder")}
          disabled={running}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={onKey}
          spellCheck={false}
        />
      </div>
    </div>
  );
}
