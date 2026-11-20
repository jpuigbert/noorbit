import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Gamepad2, Play, Square, ListChecks, AlertTriangle, CheckCircle2, Loader2, X, Trophy, Rocket, Sparkles, Folder, MonitorPlay, Brain, Bot } from "lucide-react";
import { useT } from "../../i18n";
import WorkProgress from "../WorkProgress";
import { obrirEstalvi } from "../../screensaver/window";
import { customMinutes, formatDuration, type TimeUnit } from "../../utils/workTime";
import { useProviderStore } from "../../stores/providerStore";

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
  /// Arrel del projecte on la Jam està (o ha estat) treballant. Sense això,
  /// al final l'usuari no podia saber QUIN projecte havia tocat.
  projectPath: string;
  error: string | null;
  /// Proveïdor i model d'IA que fa servir la Jam. Sense això l'usuari no
  /// sabia amb quina IA treballava de debò (Ollama, un proveïdor en línia
  /// o la IA integrada de NoOrbit).
  provider: string;
  model: string;
  /// L'aturada ja s'ha demanat però la Jam encara no s'ha aturat del tot:
  /// el backend encara està tallant el pla o el procés fill.
  stopping: boolean;
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
  { id: "bevy", label: "🦀 Bevy (Rust)" },
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
  note: "fent",
  thinking: "pensament",
  memory: "memòria",
  restart: "reinici",
  compact: "neteja de memòria",
  summary: "resum",
  finished: "acabat",
  error: "error",
};

/// Registre VIU compartit entre muntatges del panell: si l'usuari canvia de
/// sub-pestanya (Agent ▸ xat/Jam/integracions) mentre la Jam treballa, el
/// panell es desmunta i tornava buit, fent pensar que «no fa res». Viure
/// fora del component (mòdul) ho conserva mentre la finestra estigui oberta.
let registreCompartit: JamEvent[] = [];

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
  const [events, setEvents] = useState<JamEvent[]>(() => registreCompartit.slice());
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [reportText, setReportText] = useState<string | null>(null);
  // Proveïdors configurats: només en volem el nom llegible del que fa
  // servir la Jam («OpenAI» en lloc de «openai»).
  const providers = useProviderStore((s) => s.providers);
  const loadProviders = useProviderStore((s) => s.load);

  // Estat inicial + esdeveniments en viu.
  useEffect(() => {
    void invoke<JamState>("jam_state").then(setState).catch(() => undefined);
    const un1 = listen<JamState>("jam://state", (e) => setState(e.payload));
    const un2 = listen<JamEvent>("jam://event", (e) => {
      registreCompartit = [...registreCompartit.slice(-200), e.payload];
      setEvents((prev) => [...prev.slice(-200), e.payload]);
    });
    return () => {
      void un1.then((u) => u());
      void un2.then((u) => u());
    };
  }, []);

  // Noms llegibles dels proveïdors (només si encara no tenim la llista).
  useEffect(() => {
    if (providers.length === 0) void loadProviders();
  }, [providers.length, loadProviders]);

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
    registreCompartit = [];
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
    // Resposta immediata: el botó es posa a «aturant…» abans que torni el
    // backend (que també emet l'estat). Abans no hi havia cap retorn visible
    // i prémer «Atura» semblava que no feia res.
    setState((s) => (s ? { ...s, stopping: true } : s));
    try {
      await invoke("jam_stop");
    } catch (e) {
      // Si l'ordre no arriba al backend es torna enrere: el botó no es pot
      // quedar «Aturant…» per sempre sense que la Jam s'aturi.
      setState((s) => (s ? { ...s, stopping: false } : s));
      setErr(String(e));
    }
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
  // L'aturada ja s'ha demanat? (el botó ho diu i es desactiva: prémer-lo
  // dos cops no fa res i abans semblava que el botó no funcionava).
  const stopping = state?.stopping ?? false;
  // Nom llegible del proveïdor: el backend dona l'id («ollama»,
  // «noorbit_native»…) i el store el nom de veritat («OpenAI», «Claude…»).
  // Ollama i la IA integrada NO són al store (no són proveïdors
  // configurables), així que tenen el nom aquí.
  const providerLabel = (id: string) =>
    providers.find((p) => p.id === id)?.name ??
    (id === "noorbit_native"
      ? t("jam.aiNative")
      : id === "ollama"
        ? t("jam.aiOllama")
        : id);
  // Etiqueta de la IA que fa servir la Jam: «OpenAI · gpt-4o-mini».
  const aiLabel = state?.provider
    ? [providerLabel(state.provider), state.model].filter(Boolean).join(" · ")
    : "";
  // Nom de la carpeta del projecte (l'últim segment del camí): és el que
  // l'usuari reconeix de la barra lateral, no tot el camí absolut.
  const projectName = (state?.projectPath ?? "")
    .replace(/[\\/]+$/, "")
    .split(/[\\/]/)
    .pop() ?? "";
  const revealProject = () => {
    if (!state?.projectPath) return;
    void invoke("reveal_in_finder", { path: state.projectPath }).catch(() => undefined);
  };
  // Motor on què es pot obrir el projecte amb UN CLIC. Només els que tenen
  // editor o terminal propi: Blender, Unity i el web no s'hi obren així.
  const openableEngine =
    engine === "godot4"
      ? "godot"
      : engine === "unreal5"
        ? "unreal"
        : engine === "bevy"
          ? "bevy"
          : null;
  const openInEngine = async () => {
    if (!state?.projectPath || !openableEngine) return;
    setNotice(null);
    try {
      setNotice(
        await invoke<string>("open_project_in_engine", {
          projectPath: state.projectPath,
          engine: openableEngine,
        }),
      );
    } catch (e) {
      setErr(String(e));
    }
  };
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
        <span className="jam-head-right">
          {aiLabel && (
            <span className="jam-ai" title={`${t("jam.ai")}: ${aiLabel}`}>
              <Bot size={12} /> {aiLabel}
            </span>
          )}
          {running && <span className="jam-live">{t("jam.live")}</span>}
        </span>
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
          {/* El projecte concret on la Jam treballa (o ha treballat): visible
              TAMBÉ després d'acabar, quan la pestanya es tanca i es reobre. */}
          {projectName && (
            <div className="jam-current" title={state.projectPath}>
              <Folder size={11} /> <b>{t("jam.project")}</b> {projectName}
              <button className="btn sm" style={{ marginLeft: 8 }} onClick={revealProject}>
                {t("jam.reveal")}
              </button>
              {openableEngine && (
                <button className="btn sm" style={{ marginLeft: 6 }} onClick={() => void openInEngine()}>
                  <MonitorPlay size={12} /> {t("jam.openInEngine")}
                </button>
              )}
            </div>
          )}
          {notice && (
            <div className="jam-current" style={{ color: "var(--accent)" }}>
              {notice}
            </div>
          )}
          {!running && state.finishedAt && (
            <div className="jam-current">
              <Trophy size={11} /> <b>{t("jam.title")}</b> {t("jam.finished")}
            </div>
          )}
          {running && (
            <div className="jam-current">
              <Sparkles size={11} /> <b>{state.currentTask || t("jam.workingNow")}</b> {state.currentDetail}
            </div>
          )}
          {running && (
            <button className="btn sm" disabled={stopping} onClick={() => void doStop()}>
              {stopping ? <Loader2 size={12} className="spin" /> : <Square size={12} />}{" "}
              {stopping ? t("jam.stopping") : t("jam.stop")}
            </button>
          )}
          {/* Pantalla de feina a pantalla completa: deixa el Mac/PC mostrant
              el progrés gran mentre la IA treballa sola (i keep_awake evita
              que el sistema s'adormi per inactivitat). */}
          <button
            className="btn sm"
            style={{ marginLeft: 8 }}
            onClick={() => void obrirEstalvi()}
            title={t("jam.workScreen")}
          >
            <MonitorPlay size={12} /> {t("jam.workScreen")}
          </button>
        </div>
      )}

      {/* ── REGISTRE ── */}
      {events.length > 0 && (
        <div className="jam-log">
          {events.map((e, i) => (
            <div key={i} className={"jam-log-row" + (e.ok ? "" : " bad") + (e.kind === "thinking" ? " thought" : "")}>
              {e.kind === "task_done" ? (
                <CheckCircle2 size={11} />
              ) : e.kind === "task_error" || e.kind === "error" ? (
                <AlertTriangle size={11} />
              ) : e.kind === "finished" ? (
                <Trophy size={11} />
              ) : e.kind === "thinking" ? (
                <Brain size={11} />
              ) : (
                <Loader2 size={11} className="spin-slow" />
              )}
              <span className="jam-log-kind">{KIND_LABEL[e.kind] ?? e.kind}</span>
              {e.task && e.kind !== "thinking" && e.kind !== "note" && <b>{e.task}</b>}
              <span
                className="jam-log-detail"
                style={e.kind === "thinking" ? { whiteSpace: "pre-wrap" } : undefined}
              >
                {e.detail}
              </span>
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
