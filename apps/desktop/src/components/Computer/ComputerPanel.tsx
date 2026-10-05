import { useEffect, useState } from "react";
import {
  Terminal,
  Play,
  Trash2,
  ShieldCheck,
  Bot,
  RefreshCw,
  X,
  History,
} from "lucide-react";
import { useT } from "../../i18n";
import { useComputerStore } from "../../stores/computerStore";

function fmtTime(secs: number): string {
  if (!secs) return "";
  const d = new Date(secs * 1000);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export default function ComputerPanel() {
  const { t } = useT();
  const s = useComputerStore();
  const [cmd, setCmd] = useState("");
  const [goal, setGoal] = useState("");

  useEffect(() => {
    s.setupEvents();
    s.load();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <div className="computer-panel">
      <div className="cp-intro">
        <ShieldCheck size={14} />
        <span>{t("computer.intro")}</span>
      </div>

      {/* Permisos */}
      <div className="field cp-perms">
        <label className="cp-switch">
          <input
            type="checkbox"
            checked={s.perms.enabled}
            onChange={(e) => s.setPerms(e.target.checked, undefined)}
          />
          <span>{t("computer.enable")}</span>
        </label>
        <label className="cp-switch">
          <input
            type="checkbox"
            checked={s.perms.confirmEach}
            onChange={(e) => s.setPerms(undefined, e.target.checked)}
          />
          <span>{t("computer.confirmEach")}</span>
        </label>
      </div>

      {!s.perms.enabled && <div className="cp-warn">{t("computer.disabledWarn")}</div>}

      {/* Allowlist */}
      <div className="field">
        <label>{t("computer.allowlist")}</label>
        {s.perms.allowlist.length === 0 ? (
          <div className="empty-hint">{t("computer.allowlistEmpty")}</div>
        ) : (
          <div className="cp-chips">
            {s.perms.allowlist.map((p) => (
              <span key={p} className="cp-chip">
                <code>{p}</code>
                <button className="icon-btn" title={t("computer.remove")} onClick={() => s.removePattern(p)}>
                  <X size={11} />
                </button>
              </span>
            ))}
          </div>
        )}
      </div>

      {/* Execució directa */}
      <div className="field">
        <label>{t("computer.directTitle")}</label>
        <div className="cp-run">
          <input
            placeholder={t("computer.directPlaceholder")}
            value={cmd}
            onChange={(e) => setCmd(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && (s.runCommand(cmd), setCmd(""))}
          />
          <button
            className="btn primary sm"
            disabled={s.busy || !cmd.trim()}
            onClick={() => {
              s.runCommand(cmd);
              setCmd("");
            }}
          >
            <Terminal size={13} /> {t("common.run")}
          </button>
        </div>
      </div>

      {/* Agent IA */}
      <div className="field">
        <label>{t("computer.agentTitle")}</label>
        <div className="cp-run">
          <input
            placeholder={t("computer.agentPlaceholder")}
            value={goal}
            onChange={(e) => setGoal(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && (s.runAgent(goal), setGoal(""))}
          />
          <button
            className="btn sm"
            disabled={s.agentRunning || !goal.trim() || !s.perms.enabled}
            onClick={() => {
              s.runAgent(goal);
              setGoal("");
            }}
          >
            <Bot size={13} /> {t("computer.agentRun")}
          </button>
        </div>
        {s.agentSteps.length > 0 && (
          <div className="cp-steps">
            <div className="agent-section-head">{t("computer.agentSteps")}</div>
            {s.agentSteps.map((st, i) => (
              <div key={i} className={"cp-step " + (st.kind === "done" ? "done" : "run")}>
                {st.kind === "done" ? (
                  <span className="cp-step-ans">{st.answer}</span>
                ) : (
                  <>
                    <code>{st.command}</code>
                    {st.reason && <span className="cp-step-reason">{st.reason}</span>}
                  </>
                )}
              </div>
            ))}
          </div>
        )}
        {s.agentRunning && <div className="empty-hint">{t("computer.agentRunning")}</div>}
      </div>

      {s.error && <div className="si-error">{s.error}</div>}

      {/* Sortida de darrera execució */}
      {s.lastOutput && (
        <div className="field">
          <label>{t("computer.output")}</label>
          <div className={"cp-out " + (s.lastOutput.ok ? "ok" : "fail")}>
            <div className="cp-out-meta">
              {t("computer.exit")} {s.lastOutput.exitCode}
              {s.lastOutput.timedOut && " · timeout"}
            </div>
            <pre>{s.lastOutput.stdout || s.lastOutput.stderr || "(buit)"}</pre>
          </div>
        </div>
      )}

      {/* Historial */}
      <div className="field">
        <div className="agent-section-head">
          <span>
            <History size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
            {t("computer.history")}
          </span>
          <div>
            <button className="icon-btn" title={t("common.refresh")} onClick={() => s.load()}>
              <RefreshCw size={12} />
            </button>
            <button className="icon-btn" title={t("computer.clearHistory")} onClick={() => s.clearHistory()}>
              <Trash2 size={12} />
            </button>
          </div>
        </div>
        {s.history.length === 0 ? (
          <div className="empty-hint">{t("computer.historyEmpty")}</div>
        ) : (
          <div className="cp-hist">
            {s.history.map((h, i) => (
              <div key={i} className={"cp-hist-row " + (h.ok ? "ok" : "fail")}>
                <span className="cp-hist-time">{fmtTime(h.at)}</span>
                <Play size={10} className="cp-hist-play" />
                <code className="cp-hist-cmd" title={h.excerpt}>
                  {h.command}
                </code>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
