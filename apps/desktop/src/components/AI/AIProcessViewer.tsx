import { useEffect, useRef, useState } from "react";
import { RefreshCw, Radio } from "lucide-react";
import { useProcessStore } from "../../stores/processStore";
import { useAgentStore } from "../../stores/agentStore";
import { CopyButton, SaveButton } from "../CopyButton";

/// Visor en directe del procés de la IA (Objectiu 2). S'alimenta de
/// l'event unificat «ai://process» que emeten tots els backends (Ollama i els
/// proveïdors remots via SSE). Ara cada xat té el seu propre llenç: el visor
/// mostra la generació del xat actiu, així dos xats en paral·lel no es mezclen.
export default function AIProcessViewer() {
  const activeId = useAgentStore((s) => s.activeId);
  const snap = useProcessStore((s) => s.per[activeId]);
  const text = snap?.text ?? "";
  const thinking = snap?.thinking ?? "";
  const provider = snap?.provider ?? null;
  const model = snap?.model ?? null;
  const phase = snap?.phase ?? "idle";
  const elapsedMs = snap?.elapsedMs ?? 0;
  const startedAt = snap?.startedAt ?? null;
  const reset = useProcessStore((s) => s.reset);

  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [text, thinking]);

  // Cronòmetre en viu mentre la generació és activa.
  const [, setTick] = useState(0);
  const live = phase === "streaming" || phase === "thinking";
  useEffect(() => {
    if (!live) return;
    const id = setInterval(() => setTick((x) => x + 1), 500);
    return () => clearInterval(id);
  }, [live]);
  const seconds = startedAt
    ? ((Date.now() - startedAt) / 1000).toFixed(1)
    : (elapsedMs / 1000).toFixed(1);

  const phaseClass =
    phase === "streaming"
      ? "proc-live"
      : phase === "thinking"
        ? "proc-think"
        : phase === "error"
          ? "proc-err"
          : phase === "done"
            ? "proc-done"
            : "proc-idle";

  const fullResult = text || thinking;

  return (
    <div className="proc-viewer">
      <div className="proc-head">
        <span className="proc-title">
          <Radio size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
          Procés de la IA
        </span>
        <span className={"proc-phase " + phaseClass}>{phase}</span>
        <span className="proc-meta">{seconds}s</span>
        {fullResult && (
          <>
            <CopyButton text={fullResult} label="Copia" />
            <SaveButton text={fullResult} suggestedName="proces-ia.md" />
          </>
        )}
        <button
          className="btn sm ghost"
          onClick={() => reset(activeId)}
          title="Neteja el visor d'aquest xat"
        >
          <RefreshCw size={12} />
        </button>
      </div>
      <div className="proc-sub">{provider ?? "—"} · {model ?? "—"}</div>

      <div className="proc-body" ref={ref}>
        {thinking && (
          <pre className="proc-thinking">{thinking}</pre>
        )}
        {text ? (
          <pre className="proc-text">{text}</pre>
        ) : !thinking ? (
          <div className="empty-hint">
            Esperant que la IA genere alguna cosa… El text d'aquest visor
            arriba en directe des de qualsevol model (local o en línia).
          </div>
        ) : null}
      </div>
    </div>
  );
}
