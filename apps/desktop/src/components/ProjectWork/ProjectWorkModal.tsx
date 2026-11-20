import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  Clock,
  Folder,
  Loader2,
  Rocket,
  Sparkles,
  Square,
  X,
} from "lucide-react";
import { useT } from "../../i18n";
import WorkProgress from "../WorkProgress";
import { customMinutes, formatDuration, type TimeUnit } from "../../utils/workTime";
import { useUIStore } from "../../stores/uiStore";
import { useAIStore } from "../../stores/aiStore";
import { useProviderStore } from "../../stores/providerStore";
import "./ProjectWorkModal.css";

/**
 * MENÚ «TREBALL AUTOPILOTAT EN QUALSEVOL PROJECTE».
 *
 * L'usuari tria una carpeta de projecte EXISTENT (joc, web, backend, eina…),
 * explica què vol que la IA hi faci i tria un TEMPS DE TREBALL (preset o
 * personalitzat). Durant eixe temps la IA busca millores i bugs i implementa
 * tot el que se li acudeix, passada a passada, 100% autopilotada: cerca
 * informació a internet sola (headless) i no demana mai tocar el PC.
 *
 * És el mateix motor que la millora de temes de WordPress (`project_work`),
 * generalitzat a qualsevol projecte amb autodetecció del perfil.
 */

const LOCAL = "local|";
const PROV = "prov|";
/// IA nativa integrada al binari («noorbit_native»): sense descàrregues ni
/// claus. El backend la reconeix a chat_inner i respon amb el motor simbòlic
/// (que delega les generacions llargues en qualsevol IA disponible).
const NATIVE = "nat|";
const NATIVE_ID = "noorbit_native";
/// Opció «Personalitzat»: temps exacte (valor + unitat), com a la Jam i WordPress.
const CUSTOM = -1;

/// Estat de la millora (veu del backend `project://work`).
interface ProjectWorkState {
  running: boolean;
  pass: number;
  elapsedMs: number;
  budgetMs: number;
  filesChanged: number;
  detail: string;
  startedAt: number;
  finishedAt: number | null;
  projectPath: string;
  error: string | null;
}

const WORK_DURATIONS = [
  { mins: 15, label: "15 min" },
  { mins: 30, label: "30 min" },
  { mins: 60, label: "1 hora" },
  { mins: 120, label: "2 hores" },
  { mins: 240, label: "4 hores" },
  { mins: 480, label: "8 hores" },
  { mins: 1440, label: "1 dia" },
  { mins: CUSTOM, label: "" },
];

export default function ProjectWorkModal() {
  const { t } = useT();
  const show = useUIStore((s) => s.showProjectWork);
  const setShow = useUIStore((s) => s.setShowProjectWork);

  // Selectors de zustand: només estat CRU i estable (res de llistes noves).
  const models = useAIStore((s) => s.models);
  const loadModels = useAIStore((s) => s.loadModels);
  const providers = useProviderStore((s) => s.providers);
  const loadProviders = useProviderStore((s) => s.load);

  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [idea, setIdea] = useState("");
  const [ai, setAi] = useState("");
  const [workMins, setWorkMins] = useState(60);
  const [customVal, setCustomVal] = useState(2);
  const [customUnit, setCustomUnit] = useState<TimeUnit>("h");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [pwState, setPwState] = useState<ProjectWorkState | null>(null);

  /// En obrir: carrega IA + estat de la millora (pot continuar en segon pla).
  useEffect(() => {
    if (!show) return;
    void loadModels();
    void loadProviders();
    void invoke<ProjectWorkState>("project_work_state").then(setPwState).catch(() => undefined);
    const un = listen<ProjectWorkState>("project://work", (e) => setPwState(e.payload));
    return () => {
      void un.then((u) => u());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [show]);

  /// IA per defecte: el model local si n'hi ha; si no, la IA NATIVA integrada
  /// (sempre funciona, sense tokens); els proveïdors actius queden triables.
  useEffect(() => {
    if (ai) return;
    if (models.length > 0) {
      setAi(LOCAL + models[0].name);
      return;
    }
    setAi(NATIVE + NATIVE_ID);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [models]);

  /// L'IA triada es resol en (proveïdor, model) per als arguments del backend.
  const resolveAi = useMemo((): { provider: string | undefined; model: string | undefined } => {
    if (ai.startsWith(LOCAL)) return { provider: "ollama", model: ai.slice(LOCAL.length) };
    if (ai.startsWith(NATIVE)) {
      const nat = providers.find((x) => x.kind === "native");
      return { provider: NATIVE_ID, model: nat?.model };
    }
    if (ai.startsWith(PROV)) {
      const p = providers.find((x) => x.id === ai.slice(PROV.length));
      return { provider: p?.id, model: p?.model };
    }
    return { provider: undefined, model: undefined };
  }, [ai, providers]);

  /// Temps efectiu (minuts): preset o valor personalitzat (retallat 1 min..7 dies).
  const effectiveMins = workMins === CUSTOM ? customMinutes(customVal, customUnit) : workMins;
  const running = pwState?.running === true;

  const close = () => {
    setShow(false);
    setErr(null);
    setMsg(null);
    // NO aturem el backend: la millora continua autopilotada en segon pla.
  };

  const browse = async () => {
    const sel = await open({ directory: true, title: t("projectWork.pickDir") });
    if (typeof sel === "string") setProjectPath(sel);
  };

  const start = async () => {
    if (!projectPath) {
      setErr(t("projectWork.errDir"));
      return;
    }
    if (!idea.trim()) {
      setErr(t("projectWork.errIdea"));
      return;
    }
    if (!ai) {
      setErr(t("projectWork.errAi"));
      return;
    }
    const { provider, model } = resolveAi;
    const name = projectPath.split("/").filter(Boolean).pop() ?? "projecte";
    setBusy(true);
    setErr(null);
    setMsg(null);
    try {
      await invoke("project_work", {
        projectPath,
        name,
        idea: idea.trim(),
        budgetSecs: effectiveMins * 60,
        provider: provider ?? undefined,
        model: model ?? undefined,
      });
      setMsg(t("projectWork.started"));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    await invoke("project_work_stop").catch(() => undefined);
  };

  if (!show) return null;

  return (
    <div className="modal-overlay" onClick={close}>
      <div className="modal pw-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>
            <Rocket size={15} style={{ marginRight: 6, verticalAlign: -2 }} />
            {t("projectWork.title")}
          </span>
          <button className="icon-btn" onClick={close}>
            <X size={16} />
          </button>
        </div>

        <div className="modal-body pw-body">
          <p className="lp-hint">{t("projectWork.intro")}</p>

          {/* CARPETA DEL PROJECTE */}
          <div className="field">
            <label>{t("projectWork.dir")}</label>
            <div className="pw-path">
              <input readOnly value={projectPath ?? ""} placeholder={t("projectWork.dirPh")} />
              <button className="btn sm" disabled={running} onClick={() => void browse()}>
                <Folder size={12} /> {t("common.browse")}
              </button>
            </div>
          </div>

          {/* OBJECTIU */}
          <div className="field">
            <label>{t("projectWork.idea")}</label>
            <textarea
              rows={4}
              disabled={running}
              value={idea}
              placeholder={t("projectWork.ideaPh")}
              onChange={(e) => setIdea(e.target.value)}
            />
            <div className="lp-hint" style={{ marginTop: 4 }}>
              {t("projectWork.ideaHint")}
            </div>
          </div>

          {/* IA */}
          <div className="field">
            <label>{t("projectWork.ai")}</label>
            <select
              className="lp-select"
              value={ai}
              disabled={running}
              onChange={(e) => setAi(e.target.value)}
            >
              {/* La IA integrada va primer de tot: funciona amb tot instal·lat. */}
              <optgroup label={t("projectWork.aiNativeGroup")}>
                <option value={NATIVE + NATIVE_ID}>{t("projectWork.aiNativeOption")}</option>
              </optgroup>
              {models.length > 0 && (
                <optgroup label={t("wpTheme.aiLocal")}>
                  {models.map((m) => (
                    <option key={m.name} value={LOCAL + m.name}>
                      {m.name}
                    </option>
                  ))}
                </optgroup>
              )}
              {providers.filter((p) => p.kind !== "native").length > 0 && (
                <optgroup label={t("wpTheme.aiCloud")}>
                  {providers
                    .filter((p) => p.kind !== "native")
                    .map((p) => (
                      <option key={p.id} value={PROV + p.id}>
                        {p.name} · {p.model}
                        {p.has_token ? "" : ` (${t("wpTheme.noToken")})`}
                      </option>
                    ))}
                </optgroup>
              )}
            </select>
            {models.length === 0 && providers.filter((p) => p.kind !== "native" && p.has_token).length === 0 && (
              <div className="lp-hint" style={{ marginTop: 4 }}>
                {t("projectWork.aiNativeOnlyHint")}
              </div>
            )}
          </div>

          {/* TEMPS DE TREBALL (preset o personalitzat) */}
          <div className="field">
            <label>
              <Clock size={12} style={{ marginRight: 4, verticalAlign: -2 }} />
              {t("projectWork.time")}
            </label>
            <select
              className="lp-select"
              value={workMins}
              disabled={running || busy}
              onChange={(e) => setWorkMins(Number(e.target.value))}
            >
              {WORK_DURATIONS.map((d) => (
                <option key={d.mins} value={d.mins}>
                  {d.mins === CUSTOM ? t("work.custom") : d.label}
                </option>
              ))}
            </select>
            {workMins === CUSTOM && (
              <div className="pw-2" style={{ marginTop: 8 }}>
                <div className="field">
                  <label>{t("work.customTime")}</label>
                  <input
                    type="number"
                    min={1}
                    value={customVal}
                    disabled={running || busy}
                    onChange={(e) => setCustomVal(Number(e.target.value))}
                  />
                </div>
                <div className="field">
                  <label>{t("work.unit")}</label>
                  <select
                    className="lp-select"
                    value={customUnit}
                    disabled={running || busy}
                    onChange={(e) => setCustomUnit(e.target.value as TimeUnit)}
                  >
                    <option value="min">{t("work.unitMin")}</option>
                    <option value="h">{t("work.unitH")}</option>
                    <option value="d">{t("work.unitD")}</option>
                  </select>
                </div>
              </div>
            )}
            <div className="lp-hint" style={{ marginTop: 4 }}>
              {formatDuration(effectiveMins * 60_000)} · {t("projectWork.timeHint")}
            </div>
          </div>

          {/* ARRENCAR */}
          {!running && (
            <button
              className="btn primary"
              disabled={busy || !projectPath || !idea.trim() || !ai}
              onClick={() => void start()}
            >
              {busy ? <Loader2 size={14} className="spin" /> : <Sparkles size={14} />}{" "}
              {t("projectWork.start")}
            </button>
          )}

          {/* PROGRÉS EN VIU (transcorregut / restant / fin estimada) */}
          {pwState && (pwState.running || pwState.finishedAt) && (
            <div className="pw-work">
              <WorkProgress
                running={pwState.running}
                startedAtMs={(pwState.startedAt || 0) * 1000}
                mode="time"
                budgetMs={pwState.budgetMs}
                finishedAtMs={pwState.finishedAt ? pwState.finishedAt * 1000 : null}
                label={`${t("wpTheme.workPass")} ${pwState.pass} · ${pwState.filesChanged} ${t("wpTheme.workFiles")}`}
              />
              {pwState.detail && <div className="lp-hint">{pwState.detail}</div>}
              {pwState.running && (
                <button className="btn sm" onClick={() => void stop()}>
                  <Square size={12} /> {t("projectWork.stop")}
                </button>
              )}
              {pwState.error && (
                <div className="pw-msg err">
                  <AlertTriangle size={13} /> {pwState.error}
                </div>
              )}
            </div>
          )}

          {msg && <div className="pw-msg">{msg}</div>}
          {err && (
            <div className="pw-msg err">
              <AlertTriangle size={13} /> {err}
            </div>
          )}
        </div>

        <div className="pw-foot">
          <span className="pw-credit">
            {t("wpTheme.credit")} <b>Joan Puigbert Carreras</b> · <b>Thepuigberts.com</b>
          </span>
          <button className="btn sm" onClick={close}>
            {t("projectWork.close")}
          </button>
        </div>
      </div>
    </div>
  );
}