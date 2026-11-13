/// Vista de la CONSOLA DE DEPURACIÓ: mostra els missatges capturats
/// (console, errors del webview, esdeveniments del backend) amb filtre per
/// nivell, hora i botons per buidar/pausar. A l'estil de la consola del
/// desenvolupador, però dins de NoOrbit.
import { useEffect, useRef, useState } from "react";
import { Ban, Check, Eraser, Terminal } from "lucide-react";
import { useConsoleStore, type LogLevel } from "../../stores/consoleStore";
import { useT } from "../../i18n";

const LEVEL_CLASS: Record<LogLevel, string> = {
  log: "c-log",
  info: "c-info",
  warn: "c-warn",
  error: "c-error",
  system: "c-system",
};

type Filter = "all" | LogLevel;

export default function DebugConsole() {
  const { t } = useT();
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
  const hhmmss = (ms: number) =>
    new Date(ms).toLocaleTimeString("ca", { hour12: false });

  return (
    <div className="debug-console">
      <div className="dc-head">
        <Terminal size={13} />
        <span>{t("console.title")}</span>
        <select
          className="ap-model-select dc-filter"
          value={filter}
          onChange={(e) => setFilter(e.target.value as Filter)}
        >
          <option value="all">{t("console.all", { n: entries.length })}</option>
          <option value="error">{t("console.errors")}</option>
          <option value="warn">{t("console.warnings")}</option>
          <option value="info">{t("console.info")}</option>
          <option value="log">{t("console.log")}</option>
          <option value="system">{t("console.system")}</option>
        </select>
        <button
          className={"btn sm ghost" + (paused ? " active" : "")}
          onClick={() => setPaused(!paused)}
          title={paused ? t("console.resumeHint") : t("console.pauseHint")}
        >
          {paused ? <Check size={12} /> : <Ban size={12} />} {paused ? t("console.resume") : t("console.pause")}
        </button>
        <button className="icon-btn" onClick={() => clear()} title={t("console.clear")}>
          <Eraser size={13} />
        </button>
      </div>
      <div className="dc-body" ref={bodyRef}>
        {shown.length === 0 ? (
          <div className="dc-empty">{t("console.empty")}</div>
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
