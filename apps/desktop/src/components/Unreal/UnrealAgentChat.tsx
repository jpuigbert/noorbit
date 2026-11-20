import { useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import {
  Send,
  Loader2,
  Trash2,
  Bot,
  Wrench,
  CheckCircle2,
  AlertTriangle,
  Info,
} from "lucide-react";
import { useT } from "../../i18n";
import { useUnrealStore } from "../../stores/unrealStore";
import {
  agentPhaseKind,
  type UnrealAgentEvent,
} from "../../utils/unrealAgent";

/// FASE 3 · EXECUCIÓ — xat lliure amb l'agent: escriu què vols («crea una
/// fila de 5 llums…») i el model local tria les eines MCP, les executa i
/// respon. El bucle es veu EN DIRECTE (cada eina que es crida apareix).
export default function UnrealAgentChat() {
  const { t } = useT();
  const {
    taskEvents,
    taskRunning,
    taskAnswer,
    taskError,
    runTask,
    clearTask,
  } = useUnrealStore(
    useShallow((st) => ({
      taskEvents: st.taskEvents,
      taskRunning: st.taskRunning,
      taskAnswer: st.taskAnswer,
      taskError: st.taskError,
      runTask: st.runTask,
      clearTask: st.clearTask,
    }))
  );

  const [prompt, setPrompt] = useState("");
  const bottomRef = useRef<HTMLDivElement | null>(null);

  // Cronòmetre de la tasca en curs (l'agent pot trigar minuts).
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    if (!taskRunning) {
      setElapsed(0);
      return;
    }
    const start = Date.now();
    const timer = window.setInterval(
      () => setElapsed(Math.floor((Date.now() - start) / 1000)),
      1000
    );
    return () => window.clearInterval(timer);
  }, [taskRunning]);

  // Baixa sol al final quan arriben esdeveniments nous.
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: "end" });
  }, [taskEvents.length]);

  const send = () => {
    const text = prompt.trim();
    if (!text || taskRunning) return;
    setPrompt("");
    void runTask(text);
  };

  const hasHistory = taskEvents.length > 0 || !!taskAnswer || !!taskError;

  return (
    <div className="ue-chat">
      <div className="ue-chat__composer">
        <textarea
          className="ue-agent-task"
          value={prompt}
          spellCheck={false}
          disabled={taskRunning}
          onChange={(e) => setPrompt(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
          placeholder={t("unreal.agent.chat.ph")}
        />
        <div className="ue-chat__actions">
          <button
            className="btn primary sm"
            onClick={send}
            disabled={taskRunning || !prompt.trim()}
          >
            {taskRunning ? (
              <Loader2 size={13} className="spin" />
            ) : (
              <Send size={13} />
            )}
            {taskRunning
              ? t("unreal.agent.chat.working", { s: elapsed })
              : t("unreal.agent.chat.send")}
          </button>
          {hasHistory && !taskRunning && (
            <button
              className="ue-setup__icon"
              onClick={clearTask}
              aria-label={t("unreal.agent.chat.clear")}
              title={t("unreal.agent.chat.clear")}
            >
              <Trash2 size={14} />
            </button>
          )}
        </div>
      </div>

      {hasHistory && (
        <div className="ue-chat__log" role="log" aria-live="polite">
          {taskEvents.map((ev) => (
            <EventLine key={ev.id} ev={ev} />
          ))}
          <div ref={bottomRef} />
        </div>
      )}

      {taskAnswer && (
        <div className="ue-agent-answer">
          <strong>{t("unreal.agent.chat.answer")}:</strong>
          <div>{taskAnswer}</div>
        </div>
      )}

      {taskError && (
        <div className="ue-chat__error">
          <AlertTriangle size={14} /> {taskError}
        </div>
      )}
    </div>
  );
}

/// Una línia del bucle d'eines: icona segons la fase, el text del backend i
/// el nom de l'eina quan n'hi ha.
function EventLine({ ev }: { ev: UnrealAgentEvent }) {
  const { t } = useT();
  const kind = agentPhaseKind(ev.phase);
  const Icon = iconFor(kind);
  const time = new Date(ev.ts).toLocaleTimeString("ca-ES");

  return (
    <div className={`ue-chat__event is-${kind}`}>
      <Icon size={13} />
      <div className="ue-chat__event-text">
        {ev.tool && <code className="ue-chat__tool">{ev.tool}</code>}
        <span>{ev.text}</span>
      </div>
      <span className="ue-chat__time">
        {ev.turn ? t("unreal.agent.chat.round", { n: ev.turn }) : time}
      </span>
    </div>
  );
}

function iconFor(kind: ReturnType<typeof agentPhaseKind>) {
  switch (kind) {
    case "tool":
      return Wrench;
    case "ok":
      return CheckCircle2;
    case "error":
      return AlertTriangle;
    case "answer":
      return Bot;
    default:
      return Info;
  }
}
