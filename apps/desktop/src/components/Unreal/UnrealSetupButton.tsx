import { useEffect, useMemo, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import {
  Rocket,
  Loader2,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  RotateCw,
  Trash2,
  ChevronDown,
  ChevronRight,
  Lock,
} from "lucide-react";
import { useT } from "../../i18n";
import { useUnrealStore } from "../../stores/unrealStore";

/// Passos que fa l'orquestrador al backend (Ollama, plugins, auto-arrencada,
/// servidor i sonda). Són cinc, sempre en el mateix ordre.
const EXPECTED_STEPS = 5;

/// FASE 1 · SETUP — un sol clic que deixa l'entorn PREPARAT perquè l'IA
/// local (Ollama) pugui treballar dins de l'editor via MCP.
///
/// Mostra els passos en directe i, si la crida es queda penjada, bloqueja
/// el reintent: un altre intent obriria un SEGON editor d'Unreal (doble RAM
/// i dos ports MCP al mateix temps).
export default function UnrealSetupButton() {
  const { t } = useT();
  const {
    setupSteps,
    setupRunning,
    setupReport,
    setupError,
    setupProject,
    setupFinishedAt,
    setupAbandonedAt,
    runOneClickSetup,
    clearSetup,
  } = useUnrealStore(
    useShallow((st) => ({
      setupSteps: st.setupSteps,
      setupRunning: st.setupRunning,
      setupReport: st.setupReport,
      setupError: st.setupError,
      setupProject: st.setupProject,
      setupFinishedAt: st.setupFinishedAt,
      setupAbandonedAt: st.setupAbandonedAt,
      runOneClickSetup: st.runOneClickSetup,
      clearSetup: st.clearSetup,
    }))
  );

  const pct = useMemo(() => {
    if (setupSteps.length === 0) return 0;
    return Math.min(
      100,
      Math.round((setupSteps.length / EXPECTED_STEPS) * 100)
    );
  }, [setupSteps.length]);

  // Opcions del setup: obrir l'editor si el servidor MCP no és a peu.
  const [launchOn, setLaunchOn] = useState(true);

  // «Encara treballant»: només si el darrer pas porta més de 3 s (el
  // pas «servidor» pot trigar minuts: l'editor arrenca i compila shaders).
  const [waitingFor, setWaitingFor] = useState<string | null>(null);
  const lastStepName = setupSteps[setupSteps.length - 1]?.name ?? null;
  useEffect(() => {
    if (!setupRunning || !lastStepName) {
      setWaitingFor(null);
      return;
    }
    const timer = setTimeout(() => setWaitingFor(lastStepName), 3000);
    return () => {
      clearTimeout(timer);
      setWaitingFor(null);
    };
  }, [lastStepName, setupRunning]);

  // Es col·lapsa en èxit, però respecta l'expansió manual de l'usuari.
  const userExpandedManually = useRef(false);
  const [expanded, setExpanded] = useState(true);
  const [collapsedOnce, setCollapsedOnce] = useState(false);
  useEffect(() => {
    const isSuccess = !!setupReport && setupReport.ok;
    if (isSuccess && !collapsedOnce && !userExpandedManually.current) {
      setExpanded(false);
      setCollapsedOnce(true);
    }
    if (!isSuccess) {
      setCollapsedOnce(false);
      userExpandedManually.current = false;
    }
  }, [setupReport, collapsedOnce]);

  const onRun = () => {
    userExpandedManually.current = false;
    setExpanded(true);
    void runOneClickSetup({ launch: launchOn });
  };
  const onToggle = () => {
    userExpandedManually.current = true;
    setExpanded((v) => !v);
  };

  const isSuccess = !!setupReport && setupReport.ok;
  const isError = !!setupError || (!!setupReport && !setupReport.ok);
  const hasHistory = setupSteps.length > 0 || !!setupReport || !!setupError;
  const retryLocked = !!setupAbandonedAt && !setupRunning;

  return (
    <div className="ue-setup">
      <div className="ue-setup__bar">
        <button
          className="btn primary sm"
          onClick={onRun}
          disabled={setupRunning || retryLocked}
          aria-busy={setupRunning}
          title={
            retryLocked
              ? t("unreal.agent.setup.waitRetryHint")
              : t("unreal.agent.setup.hint")
          }
        >
          {setupRunning ? (
            <Loader2 size={13} className="spin" />
          ) : retryLocked ? (
            <Lock size={13} />
          ) : isError ? (
            <RotateCw size={13} />
          ) : (
            <Rocket size={13} />
          )}
          {setupRunning
            ? t("unreal.agent.setup.preparing", { pct })
            : retryLocked
              ? t("unreal.agent.setup.waitRetry")
              : isError
                ? t("unreal.agent.setup.retry")
                : isSuccess
                  ? t("unreal.agent.setup.reverify")
                  : t("unreal.agent.setup.activate")}
        </button>

        {hasHistory && (
          <button
            className="ue-setup__icon"
            onClick={onToggle}
            aria-label={
              expanded
                ? t("unreal.agent.setup.collapse")
                : t("unreal.agent.setup.expand")
            }
          >
            {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </button>
        )}
        {hasHistory && !setupRunning && (
          <button
            className="ue-setup__icon"
            onClick={clearSetup}
            aria-label={t("unreal.agent.setup.clear")}
            title={t("unreal.agent.setup.clear")}
          >
            <Trash2 size={14} />
          </button>
        )}
      </div>

      <label className="ue-setup__launch">
        <input
          type="checkbox"
          checked={launchOn}
          disabled={setupRunning}
          onChange={(e) => setLaunchOn(e.target.checked)}
        />{" "}
        {t("unreal.agent.launch")}
      </label>

      {expanded && hasHistory && (
        <div className="ue-setup__panel" role="log" aria-live="polite">
          {setupRunning && (
            <div className="ue-setup__progress" aria-hidden="true">
              <div
                className="ue-setup__progress-fill"
                style={{ width: `${pct}%` }}
              />
            </div>
          )}

          {setupProject && (
            <div className="ue-setup__project" title={setupProject}>
              <strong>{t("unreal.agent.setup.project")}:</strong>{" "}
              <code>{shortPath(setupProject)}</code>
            </div>
          )}

          {setupSteps.map((s, i) => {
            const isLast = i === setupSteps.length - 1;
            const waiting = isLast && setupRunning && waitingFor === s.name;
            return (
              <div
                key={`${s.name}-${i}`}
                className={`ue-setup__step is-${s.ok ? "ok" : "err"}`}
              >
                {s.ok ? (
                  <CheckCircle2 size={14} />
                ) : (
                  <AlertTriangle size={14} />
                )}
                <div className="ue-setup__text">
                  <strong>{stepLabel(t, s.name)}</strong>
                  <span>{s.message}</span>
                  {waiting && (
                    <em className="ue-setup__waiting">
                      {t("unreal.agent.setup.working")}
                    </em>
                  )}
                </div>
              </div>
            );
          })}

          {setupReport && (
            <div
              className={`ue-setup__summary is-${setupReport.ok ? "ok" : "err"}`}
            >
              <strong>
                {setupReport.ok ? "✓ " : "✗ "}
                {setupReport.message}
              </strong>
              {setupReport.mcp_connected && (
                <span>
                  {" · "}
                  {t("unreal.agent.toolsReady", { n: setupReport.tools })}
                </span>
              )}
              {setupFinishedAt && (
                <span className="ue-setup__meta">
                  {" · "}
                  {new Date(setupFinishedAt).toLocaleTimeString("ca-ES")}
                </span>
              )}
            </div>
          )}

          {setupError && !setupReport && (
            <div className="ue-setup__step is-err">
              <XCircle size={14} />
              <div className="ue-setup__text">
                <strong>{t("unreal.agent.setup.error")}</strong>
                <span>{setupError}</span>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/// Etiqueta llegible de cada pas del setup (el backend envia el nom cru).
/// Si la clau no existeix al catàleg, `t` retorna la clau mateixa: en aquest
/// cas es mostra el nom cru del pas.
function stepLabel(
  t: (key: string, vars?: Record<string, string | number>) => string,
  name: string
): string {
  const key = `unreal.agent.setup.steps.${name}`;
  const label = t(key);
  return label.startsWith("unreal.agent.setup.steps.") ? name : label;
}

/// Camí curt: el projecte pot ser llarg i el panell és estret.
function shortPath(p: string): string {
  if (!p) return "";
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts.length <= 2 ? p : `…/${parts.slice(-2).join("/")}`;
}
