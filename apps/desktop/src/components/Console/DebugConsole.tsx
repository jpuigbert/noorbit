/// Vista de la CONSOLA DE DEPURACIÓ: mostra els missatges capturats
/// (console, errors del webview, esdeveniments del backend) amb filtre per
/// nivell, hora i botons per buidar/pausar. A l'estil de la consola del
/// desenvolupador, però dins de NoOrbit.
import { useEffect, useRef, useState } from "react";
import { Ban, Check, Eraser, Terminal } from "lucide-react";
import { useConsoleStore, type LogLevel } from "../../stores/consoleStore";

const LEVEL_CLASS: Record<LogLevel, string> = {
  log: "c-log",
  info: "c-info",
  warn: "c-warn",
  error: "c-error",
  system: "c-system",
};

type Filter = "all" | LogLevel;

export default function DebugConsole() {
  const entries = useConsoleStore((s) => s.entries);
  const paused = useConsoleStore((s) => s.paused);
  const clear = useConsoleStore((s) => s.clear);
  const setPaused = useConsoleStore((s) => s.setPaused);
  const [filter, setFilter] = useState<Filter>("all");
  const bodyRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = bodyRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [entries, filter]);

  const shown = filter === "all" ? entries : entries.filter((e) => e.level === filter);
  const hhmmss = (t: number) =>
    new Date(t).toLocaleTimeString("ca", { hour12: false });

  return (
    <div className="debug-console">
      <div className="dc-head">
        <Terminal size={13} />
        <span>Consola de depuració</span>
        <select
          className="ap-model-select dc-filter"
          value={filter}
          onChange={(e) => setFilter(e.target.value as Filter)}
        >
          <option value="all">Tot ({entries.length})</option>
          <option value="error">Errors</option>
          <option value="warn">Avisos</option>
          <option value="info">Info</option>
          <option value="log">Registre</option>
          <option value="system">Sistema</option>
        </select>
        <button
          className={"btn sm ghost" + (paused ? " active" : "")}
          onClick={() => setPaused(!paused)}
          title={paused ? "Reprén la captura" : "Pausa la captura (excepte errors)"}
        >
          {paused ? <Check size={12} /> : <Ban size={12} />} {paused ? "Reprén" : "Pausa"}
        </button>
        <button className="icon-btn" onClick={() => clear()} title="Buida la consola">
          <Eraser size={13} />
        </button>
      </div>
      <div className="dc-body" ref={bodyRef}>
        {shown.length === 0 ? (
          <div className="dc-empty">
            Encara no hi ha res. Els missatges de la interfície, els errors del
            programa i els esdeveniments del backend (IA, terminal, previsualització)
            apareixeran ací.
          </div>
        ) : (
          shown.map((e, i) => (
            <div key={i} className={"dc-line " + LEVEL_CLASS[e.level]}>
              <span className="dc-time">{hhmmss(e.at)}</span>
              <span className="dc-src">{e.source}</span>
              <span className="dc-text">{e.text}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
