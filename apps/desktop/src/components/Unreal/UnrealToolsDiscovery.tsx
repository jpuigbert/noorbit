import { useDeferredValue, useEffect, useMemo, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import {
  Search,
  RefreshCw,
  Loader2,
  AlertCircle,
  ChevronRight,
  ChevronDown,
  Play,
  X,
  Wrench,
  Copy,
  Check,
  ShieldAlert,
} from "lucide-react";
import { useT } from "../../i18n";
import { useUnrealStore } from "../../stores/unrealStore";
import {
  filterTools,
  groupToolsByPrefix,
  isMutatingTool,
  truncateToolResult,
  validateToolArgs,
} from "../../utils/unrealAgent";

/// El resultat d'una eina es pinta truncat (un resultat de 10 MB bloqueja el
/// navegador); el text complet es pot copiar.
const MAX_DISPLAY_RESULT = 20_000;
/// Límit dels arguments en text (DoS local: JSON de 50 MB congela el fil).
const MAX_ARGS_BYTES = 100_000;

/// FASE 2 · DESCOBERIMENT — el catàleg d'eines que exposa l'editor i la
/// crida puntual de qualsevol d'elles (depuració o ordres curtes).
export default function UnrealToolsDiscovery() {
  const { t } = useT();
  const { tools, toolsLoading, toolsError, loadTools } = useUnrealStore(
    useShallow((st) => ({
      tools: st.tools,
      toolsLoading: st.toolsLoading,
      toolsError: st.toolsError,
      loadTools: st.loadTools,
    }))
  );

  const [query, setQuery] = useState("");
  // useDeferredValue: el filtre (que pot ser centenars d'eines) no bloqueja
  // el fil d'escriptura del buscador.
  const deferredQuery = useDeferredValue(query);

  // Carrega NOMÉS si mai s'ha intentat (no es fa polling).
  useEffect(() => {
    if (tools.length === 0 && !toolsLoading && !toolsError) {
      void loadTools();
    }
  }, [tools.length, toolsLoading, toolsError, loadTools]);

  const filtered = useMemo(
    () => filterTools(tools, deferredQuery),
    [tools, deferredQuery]
  );
  const grouped = useMemo(() => groupToolsByPrefix(filtered), [filtered]);

  // Noms expandits (un Set de noms és més barat que copiar els objectes).
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const toggle = (name: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  return (
    <div className="ue-tools">
      <div className="ue-tools__bar">
        <div className="ue-tools__search">
          <Search size={14} />
          <input
            type="search"
            placeholder={
              tools.length
                ? t("unreal.agent.tools.searchCount", { n: tools.length })
                : t("unreal.agent.tools.search")
            }
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {query && (
            <button
              className="ue-setup__icon"
              onClick={() => setQuery("")}
              aria-label={t("unreal.agent.tools.clearSearch")}
            >
              <X size={12} />
            </button>
          )}
        </div>
        <button
          className="ue-setup__icon"
          onClick={() => loadTools(true)}
          disabled={toolsLoading}
          aria-label={t("unreal.agent.tools.refresh")}
          title={t("unreal.agent.tools.refresh")}
        >
          {toolsLoading ? (
            <Loader2 size={14} className="spin" />
          ) : (
            <RefreshCw size={14} />
          )}
        </button>
      </div>

      {toolsError && (
        <div className="ue-tools__empty is-error">
          <AlertCircle size={16} />
          <p>{toolsError}</p>
        </div>
      )}

      {!toolsError && tools.length === 0 && !toolsLoading && (
        <div className="ue-tools__empty">
          <Wrench size={20} />
          <p>{t("unreal.agent.tools.empty")}</p>
          <p>{t("unreal.agent.tools.emptyHint")}</p>
        </div>
      )}

      {grouped.map(([prefix, group]) => (
        <div key={prefix} className="ue-tools__group">
          <div className="ue-tools__group-title">
            {prefix} <span>({group.length})</span>
          </div>
          {group.map((tool) => {
            const isOpen = expanded.has(tool.name);
            const mutating = isMutatingTool(tool.name);
            return (
              <div key={tool.name} className="ue-tools__item">
                <button
                  className="ue-tools__row"
                  onClick={() => toggle(tool.name)}
                  aria-expanded={isOpen}
                >
                  {isOpen ? (
                    <ChevronDown size={14} />
                  ) : (
                    <ChevronRight size={14} />
                  )}
                  <code>{tool.name}</code>
                  {mutating && (
                    <span
                      className="ue-tools__badge"
                      title={t("unreal.agent.tools.mutatingHint")}
                    >
                      <ShieldAlert size={12} /> {t("unreal.agent.tools.mutating")}
                    </span>
                  )}
                </button>
                {isOpen && <ToolDetail tool={tool} />}
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}

/// Detall d'una eina: descripció, esquema d'entrada, crida i resultat.
function ToolDetail({ tool }: { tool: { name: string; description?: string | null; input_schema?: unknown } }) {
  const { t } = useT();
  const callTool = useUnrealStore((s) => s.callTool);
  const [args, setArgs] = useState("{}");
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [copied, setCopied] = useState(false);

  const onRun = async () => {
    setRunning(true);
    setResult(null);
    setError(null);

    const check = validateToolArgs(args, MAX_ARGS_BYTES);
    if (check.error) {
      setError(check.error);
      setRunning(false);
      return;
    }

    try {
      const res = await callTool(tool.name, check.parsed ?? {});
      setResult(res.text || t("unreal.agent.tools.noText"));
      if (res.is_error) setError(t("unreal.agent.tools.serverError"));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(false);
    }
  };

  const onCopy = async () => {
    if (!result) return;
    try {
      await navigator.clipboard.writeText(result);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* el porta del navegador no hi ha */
    }
  };

  const shown = result ? truncateToolResult(result, MAX_DISPLAY_RESULT) : null;

  return (
    <div className="ue-tools__detail">
      {tool.description && <p>{tool.description}</p>}
      {tool.input_schema ? (
        <details>
          <summary>{t("unreal.agent.tools.schema")}</summary>
          <pre>{JSON.stringify(tool.input_schema, null, 2)}</pre>
        </details>
      ) : (
        <p className="ue-tools__meta">{t("unreal.agent.tools.noParams")}</p>
      )}

      <label className="ue-tools__arg-label">
        {t("unreal.agent.tools.args")}
        <textarea
          value={args}
          onChange={(e) => setArgs(e.target.value)}
          rows={4}
          spellCheck={false}
        />
      </label>

      <div className="ue-tools__actions">
        <button className="btn sm" onClick={onRun} disabled={running}>
          {running ? (
            <Loader2 size={13} className="spin" />
          ) : (
            <Play size={13} />
          )}
          {running ? t("unreal.agent.tools.running") : t("unreal.agent.tools.run")}
        </button>
        {result && (
          <button
            className="ue-setup__icon"
            onClick={onCopy}
            title={t("unreal.agent.tools.copy")}
            aria-label={t("unreal.agent.tools.copy")}
          >
            {copied ? <Check size={14} /> : <Copy size={14} />}
          </button>
        )}
      </div>

      {error && (
        <div className="ue-tools__error">
          <AlertCircle size={14} /> {error}
        </div>
      )}

      {shown && (
        <div className="ue-tools__result">
          <strong>
            {t("unreal.agent.tools.result")}
            {shown.truncated ? t("unreal.agent.tools.truncated") : ""}:
          </strong>
          <pre>{shown.text}</pre>
        </div>
      )}
    </div>
  );
}
