import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { RefreshCw, Radio } from "lucide-react";
import { useProcessStore } from "../../stores/processStore";
import type { ProcessTurn } from "../../stores/processStore";
import { useAgentStore } from "../../stores/agentStore";
import { CopyButton, SaveButton } from "../CopyButton";
import { useT } from "../../i18n";

/// El backend diu la fase en anglès; l'usuari l'ha de llegir en català i amb
/// el matís de què està passant realment (carregar un model no és «pensar»).
const PHASE_KEY: Record<string, string> = {
  idle: "process.phaseIdle",
  thinking: "process.phaseThinking",
  streaming: "process.phaseStreaming",
  done: "process.phaseDone",
  error: "process.phaseError",
};

function timeLabel(ms: number): string {
  return ms >= 60000
    ? `${Math.floor(ms / 60000)}m ${Math.round((ms % 60000) / 1000)}s`
    : `${(ms / 1000).toFixed(1)}s`;
}

/// Secció desplegable del visor: el raonament i la resposta queden GUARDADES
/// i obertes en marxa, i es poden plegar o obrir després sense perdre-les.
function Section(props: {
  title: string;
  hint?: string;
  open: boolean;
  text: string;
  tone?: string;
  children?: ReactNode;
}) {
  // Control propi: la IA l'obre quan comença a generar, però un plegat manual
  // de l'usuari no es trenca en cada fragment que arriba (el navegador és qui
  // commuta l'atribut, l'estat només mana els canvis de fase).
  const [open, setOpen] = useState(props.open);
  useEffect(() => setOpen(props.open), [props.open]);
  return (
    <details className={"proc-sec " + (props.tone ?? "")} open={open}>
      <summary>
        {props.title}
        {props.hint ? <span className="proc-sec-hint">{props.hint}</span> : null}
      </summary>
      <pre className="proc-block">{props.text}</pre>
      {props.children}
    </details>
  );
}

/// Visor en directe del procés de la IA (Objectiu 2). S'alimenta de
/// l'event unificat «ai://process» que emeten tots els backends (Ollama i els
/// proveïdors remots via SSE). Ara cada xat té el seu propre llenç: el visor
/// mostra la generació del xat actiu, així dos xats en paral·lel no es mezclen.
export default function AIProcessViewer() {
  const { t } = useT();
  const activeId = useAgentStore((s) => s.activeId);
  const snap = useProcessStore((s) => s.per[activeId]);
  const text = snap?.text ?? "";
  const thinking = snap?.thinking ?? "";
  const error = snap?.error ?? null;
  const turns = snap?.turns ?? [];
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
  const seconds = timeLabel(startedAt ? Date.now() - startedAt : elapsedMs);

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

  const fullResult = [thinking, text].filter(Boolean).join("\n\n———\n\n");

  // El pensament es mostra OBERT mentre es genera; un cop acabat el torn
  // l'usuari el pot plegar, però mai desapareix del visor.
  const openNow = live;

  return (
    <div className="proc-viewer">
      <div className="proc-head">
        <span className="proc-title">
          <Radio size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
          {t("process.title")}
        </span>
        <span className={"proc-phase " + phaseClass}>
          {PHASE_KEY[phase] ? t(PHASE_KEY[phase]) : phase}
        </span>
        <span className="proc-meta">{seconds}</span>
        {fullResult && (
          <>
            <CopyButton text={fullResult} label={t("common.copy")} />
            <SaveButton text={fullResult} suggestedName="proces-ia.md" />
          </>
        )}
        <button
          className="btn sm ghost"
          onClick={() => reset(activeId)}
          title={t("process.clearHint")}
        >
          <RefreshCw size={12} />
        </button>
      </div>
      <div className="proc-sub">{provider ?? "—"} · {model ?? "—"}</div>

      <div className="proc-body" ref={ref}>
        {error && (
          <div className="proc-error">
            <strong>{t("process.whatHappened")}</strong> {error}
          </div>
        )}
        {thinking && (
          <Section
            title={t("process.reasoning")}
            hint={live ? t("process.live") : `${timeLabel(elapsedMs)}`}
            open={openNow || !text}
            text={thinking}
            tone="sec-think"
          />
        )}
        {text && (
          <Section title={t("process.answer")} open={!live} text={text} tone="sec-answer" />
        )}
        {!thinking && !text && !error && (
          <div className="empty-hint">{t("process.empty")}</div>
        )}
        {turns.length > 0 && (
          <details className="proc-sec sec-old">
            <summary>
              {t("process.prevTurns")}
              <span className="proc-sec-hint">{turns.length}</span>
            </summary>
            <div className="proc-turns">
              {[...turns].reverse().map((tn, i) => (
                // La clau NO pot ser l'índex: en invertir la llista, un torn
                // nou entra a l'índex 0 i TOTS els anteriors canvien
                // d'índex → React els desmunta i es per l'estat obert/plegat
                // que l'usuari havia deixat.
                <Turn key={tn.id ?? `t-${turns.length - i}`} turn={tn} label={t("process.turn", { n: turns.length - i })} />
              ))}
            </div>
          </details>
        )}
      </div>
    </div>
  );
}

/// Un torn ja acabat: es pot obrir per rellegir el pensament i la resposta.
function Turn({ turn, label }: { turn: ProcessTurn; label: string }) {
  const { t } = useT();
  // Un torn interromput pot no tenir `endedAt`: «Invalid Date» no serveix.
  const when = turn.endedAt
    ? new Date(turn.endedAt).toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
      })
    : "—";
  return (
    <details className="proc-turn">
      <summary>
        {label} · {turn.model ?? "?"} · {timeLabel(turn.elapsedMs)} · {when}
      </summary>
      {turn.error && <pre className="proc-block proc-block-err">{turn.error}</pre>}
      {turn.thinking && (
        <pre className="proc-block proc-block-think">{turn.thinking}</pre>
      )}
      {turn.text && <pre className="proc-block">{turn.text}</pre>}
      {!turn.error && !turn.thinking && !turn.text && (
        <pre className="proc-block">{t("process.emptyTurn")}</pre>
      )}
    </details>
  );
}
