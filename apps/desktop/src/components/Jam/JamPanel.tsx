import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Gamepad2, Play, Square, ListChecks, AlertTriangle, CheckCircle2, Loader2, X, Trophy, Rocket, Sparkles } from "lucide-react";
import { useT } from "../../i18n";
import WorkProgress from "../WorkProgress";
import { customMinutes, formatDuration, type TimeUnit } from "../../utils/workTime";

interface JamTask {
  id: string;
  title: string;
  description: string;
  acceptance: string[];
}

interface JamPlan {
  title: string;
  tasks: JamTask[];
  durationSecs: number;
}

interface JamState {
  running: boolean;
  title: string;
  completed: number;
  blocked: number;
  skipped: number;
  /// Total de tasques del pla que s'està EXECUTANT (veu del backend). Sense
  /// això la barra de progrés no tenia denominador: el `plan` es buida en
  /// prémer «Comença» i el percentatge es calculava amb els ja fets.
  total: number;
  currentTask: string;
  currentDetail: string;
  startedAt: number;
  finishedAt: number | null;
  /// Durada màxima triada (segons): sostre de l'ETA i de la fin estimada.
  durationSecs: number;
  reportPath: string;
  error: string | null;
}

interface JamEvent {
  kind: string;
  task: string;
  detail: string;
  ok: boolean;
}

/// Presets de jocs per a nens (un clic i a jugar).
const PRESETS = [
  { emoji: "🎣", label: "Un joc de pesca", brief: "un joc de pesca on el peix rar només pica a la posta de sol" },
  { emoji: "🏎️", label: "Un joc de curses", brief: "un joc de curses de cotxes amb 3 circuits i turbo" },
  { emoji: "🚀", label: "Un joc de naus", brief: "un joc de naus que disparen a asteroides, amb vides i puntuació" },
  { emoji: "🧱", label: "Un Tetris", brief: "un Tetris clàssic amb nivells i música" },
  { emoji: "🐍", label: "Una serp", brief: "el joc de la serp (snake) amb menjar i obstacles" },
];

const ENGINES = [
  { id: "godot4", label: "🎮 Godot 4" },
  { id: "blender", label: "🧊 Blender" },
  { id: "unreal5", label: "🎬 Unreal 5" },
  { id: "unity", label: "Unity" },
  { id: "custom", label: "🌐 Web / HTML" },
];

/// Opció «Personalitzat»: l'usuari escriu el temps exacte (valor + unitat).
const CUSTOM = -1;

const DURATIONS = [
  { mins: 15, label: "⏱️ 15 min" },
  { mins: 60, label: "⏱️ 1 hora" },
  { mins: 240, label: "⏱️ 4 hores" },
  { mins: 480, label: "⏱️ 8 hores" },
  { mins: 1440, label: "📅 1 dia" },
  { mins: 2880, label: "📅 2 dies" },
  { mins: 4320, label: "📅 3 dies" },
  { mins: 10080, label: "📅 7 dies" },
  { mins: CUSTOM, label: "" },
];

/// Etiqueta humana de cada esdeveniment del registre (el backend en parla
/// internament en anglès; l'usuari ha de veure què està passant).
const KIND_LABEL: Record<string, string> = {
  task_start: "començo",
  task_done: "fet",
  task_error: "no ho aconsegueixo",
  memory: "memòria",
  restart: "reinici",
  compact: "neteja de memòria",
  summary: "resum",
  finished: "acabat",
  error: "error",
};

/// PANELL «JAM» — l'agent autònom que construeix el joc sol, en hores,
/// sense que el Mac es pengi (supervisió de memòria al backend).
/// Dissenyat perquè fins i tot un nen de 5 anys pugui fer-ho servir:
/// tria un preset, prem «Comença» i mira com el joc es construeix sol.
export default function JamPanel() {
  const { t } = useT();
  const [brief, setBrief] = useState("");
  const [engine, setEngine] = useState("godot4");
  const [mins, setMins] = useState(60);
  const [customVal, setCustomVal] = useState(2);
  const [customUnit, setCustomUnit] = useState<TimeUnit>("h");
  const [buildCmd, setBuildCmd] = useState("");
  const [plan, setPlan] = useState<JamPlan | null>(null);
  const [state, setState] = useState<JamState | null>(null);
  const [events, setEvents] = useState<JamEvent[]>([]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [reportText, setReportText] = useState<string | null>(null);

  // Estat inicial + esdeveniments en viu.
  useEffect(() => {
    void invoke<JamState>("jam_state").then(setState).catch(() => undefined);
    const un1 = listen<JamState>("jam://state", (e) => setState(e.payload));
    const un2 = listen<JamEvent>("jam://event", (e) => {
      setEvents((prev) => [...prev.slice(-200), e.payload]);
    });
    return () => {
      void un1.then((u) => u());
      void un2.then((u) => u());
    };
  }, []);

  /// Temps efectiu en minuts: el preset triat o el valor personalitzat (retallat
  /// a 1 minut..7 dies, igual que el backend).
  const effectiveMins = mins === CUSTOM ? customMinutes(customVal, customUnit) : mins;

  const doPlan = async () => {
    if (!brief.trim()) {
      setErr(t("jam.errBrief"));
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      const p = await invoke<JamPlan>("jam_plan", {
        request: { brief, engine, durationSecs: effectiveMins * 60, buildCmd: buildCmd || null },
      });
      setPlan(p);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const doStart = async () => {
    setBusy(true);
    setErr(null);
    setEvents([]);
    setReportText(null);
    try {
      await invoke("jam_start", {
        request: { brief, engine, durationSecs: effectiveMins * 60, buildCmd: buildCmd || null },
      });
      setPlan(null);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const doStop = async () => {
    await invoke("jam_stop").catch(() => undefined);
  };

  const openReport = async () => {
    if (!state?.reportPath) return;
    try {
      const text = await invoke<string>("read_file", { path: state.reportPath });
      setReportText(text);
    } catch (e) {
      setErr(String(e));
    }
  };

  const running = state?.running ?? false;
  // Denominador REAL: el `total` que envia el backend (el pla que s'està
  // executant de veritat). El `plan` de la preview només serveix mentre no
  // hagi arrencat, i el backend ja el passa a `state.total`.
  const totalTasks = state?.total || plan?.tasks.length || (state?.completed ?? 0);
  // Una tasca «processada» també és la que s'ha bloquejat o s'ha saltat: si
  // no, la barra es quedava enrere quan la IA no acabava una tasca.
  const doneTasks = Math.min(
    totalTasks,
    (state?.completed ?? 0) + (state?.blocked ?? 0) + (state?.skipped ?? 0),
  );

  return (
    <div className="jam-panel">
      <div className="jam-head">
        <Gamepad2 size={14} />
        <span>{t("jam.title")}</span>
        {running && <span className="jam-live">{t("jam.live")}</span>}
      </div>

      {!running && (
        <>
          {/* ── PRESETS PER A NENS (un clic i a jugar) ── */}
          <div className="jam-presets">
            {PRESETS.map((p) => (
              <button
                key={p.label}
                className="jam-preset-btn"
                onClick={() => setBrief(p.brief)}
                title={p.brief}
              >
                <span className="jam-preset-emoji">{p.emoji}</span>
                <span className="jam-preset-label">{p.label}</span>
              </button>
            ))}
          </div>

          <div className="field">
            <label>{t("jam.brief")}</label>
            <textarea
              rows={4}
              value={brief}
              placeholder={t("jam.briefPh")}
              onChange={(e) => setBrief(e.target.value)}
            />
          </div>

          <div className="jam-row">
            <div className="field" style={{ flex: 1 }}>
              <label>{t("jam.engine")}</label>
              <select value={engine} onChange={(e) => setEngine(e.target.value)}>
                {ENGINES.map((e) => (
                  <option key={e.id} value={e.id}>
                    {e.label}
                  </option>
                ))}
              </select>
            </div>
            <div className="field" style={{ flex: 1 }}>
              <label>{t("jam.duration")}</label>
              <select value={mins} onChange={(e) => setMins(Number(e.target.value))}>
                {DURATIONS.map((d) => (
                  <option key={d.mins} value={d.mins}>
                    {d.mins === CUSTOM ? t("work.custom") : d.label}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {mins === CUSTOM && (
            <div className="jam-row">
              <div className="field" style={{ flex: 1 }}>
                <label>{t("work.customTime")}</label>
                <input
                  type="number"
                  min={1}
                  value={customVal}
                  onChange={(e) => setCustomVal(Number(e.target.value))}
                />
              </div>
              <div className="field" style={{ flex: 1 }}>
                <label>{t("work.unit")}</label>
                <select
                  value={customUnit}
                  onChange={(e) => setCustomUnit(e.target.value as TimeUnit)}
                >
                  <option value="min">{t("work.unitMin")}</option>
                  <option value="h">{t("work.unitH")}</option>
                  <option value="d">{t("work.unitD")}</option>
                </select>
              </div>
            </div>
          )}
          {mins === CUSTOM && (
            <div className="lp-hint" style={{ marginTop: -4 }}>
              {formatDuration(effectiveMins * 60_000)} · {t("work.customHint")}
            </div>
          )}

          <div className="field">
            <label>{t("jam.buildCmd")}</label>
            <input
              value={buildCmd}
              placeholder={t("jam.buildCmdPh")}
              onChange={(e) => setBuildCmd(e.target.value)}
            />
          </div>

          <div className="jam-actions">
            <button className="btn sm" disabled={busy} onClick={() => void doPlan()}>
              <ListChecks size={12} /> {t("jam.plan")}
            </button>
            <button
              className="btn primary"
              style={{ padding: "8px 20px", fontSize: 13, fontWeight: 700 }}
              disabled={busy || !brief.trim()}
              onClick={() => void doStart()}
            >
              {busy ? <Loader2 size={14} className="spin" /> : <Rocket size={14} />} {t("jam.start")}
            </button>
          </div>
        </>
      )}

      {err && (
        <div className="si-error" style={{ marginTop: 8 }}>
          <AlertTriangle size={12} /> {err}
        </div>
      )}

      {/* ── PLA (preview) ── */}
      {plan && !running && (
        <div className="jam-plan">
          <div className="jam-plan-title">
            <ListChecks size={12} /> {plan.title}
          </div>
          <ul className="jam-plan-list">
            {plan.tasks.map((task, i) => (
              <li key={task.id}>
                <b>{i + 1}.</b> {task.title}
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* ── ESTAT EN CURS (i resum final: si no, els comptadors desapareixen
             amb la Jam i l'usuari no veu què s'ha aconseguit) ── */}
      {state && (running || state.finishedAt) && (
        <div className="jam-live-box">
          {/* Barra de progrés + temps (transcorregut, ETA i fin estimada) en
              viu. Mode «tasks»: el progrés són tasques fetes/total i l'ETA es
              retalla al sostre de durada triada (la Jam s'atura sola allí). */}
          <WorkProgress
            running={running}
            startedAtMs={(state.startedAt || 0) * 1000}
            mode="tasks"
            doneUnits={doneTasks}
            totalUnits={totalTasks}
            deadlineMs={
              state.durationSecs > 0
                ? (state.startedAt + state.durationSecs) * 1000
                : null
            }
            finishedAtMs={state.finishedAt ? state.finishedAt * 1000 : null}
          />
          <div className="jam-stats">
            <span>
              <CheckCircle2 size={11} /> {state.completed} de {totalTasks} fets
            </span>
            <span className="jam-bad">
              <AlertTriangle size={11} /> {state.blocked} fallats
            </span>
            <span className="jam-muted">{state.skipped} saltats</span>
          </div>
          {!running && state.finishedAt && (
            <div className="jam-current">
              <Trophy size={11} /> <b>{t("jam.title")}</b> {t("jam.finished")}
            </div>
          )}
          {running && (
            <div className="jam-current">
              <Sparkles size={11} /> <b>{state.currentTask || "…"}</b> {state.currentDetail}
            </div>
          )}
          {running && (
            <button className="btn sm" onClick={() => void doStop()}>
              <Square size={12} /> {t("jam.stop")}
            </button>
          )}
        </div>
      )}

      {/* ── REGISTRE ── */}
      {events.length > 0 && (
        <div className="jam-log">
          {events.map((e, i) => (
            <div key={i} className={"jam-log-row" + (e.ok ? "" : " bad")}>
              {e.kind === "task_done" ? (
                <CheckCircle2 size={11} />
              ) : e.kind === "task_error" || e.kind === "error" ? (
                <AlertTriangle size={11} />
              ) : e.kind === "finished" ? (
                <Trophy size={11} />
              ) : (
                <Loader2 size={11} className="spin-slow" />
              )}
              <span className="jam-log-kind">{KIND_LABEL[e.kind] ?? e.kind}</span>
              {e.task && <b>{e.task}</b>}
              <span className="jam-log-detail">{e.detail}</span>
            </div>
          ))}
        </div>
      )}

      {state?.finishedAt && !running && state.reportPath && (
        <div className="jam-report">
          <Trophy size={12} /> {t("jam.report")}: {state.reportPath}
          <button className="btn sm" style={{ marginLeft: 8 }} onClick={() => void openReport()}>
            {t("jam.openReport")}
          </button>
        </div>
      )}

      {reportText && (
        <div className="jam-report-viewer">
          <div className="jam-report-viewer-head">
            <span>REPORT.md</span>
            <button className="icon-btn" onClick={() => setReportText(null)}>
              <X size={14} />
            </button>
          </div>
          <pre className="jam-report-viewer-body">{reportText}</pre>
        </div>
      )}

      {state?.error && (
        <div className="si-error" style={{ marginTop: 8 }}>
          <X size={12} /> {state.error}
        </div>
      )}
    </div>
  );
}
