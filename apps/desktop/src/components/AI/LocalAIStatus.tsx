import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  CheckCircle2,
  Cpu,
  Download,
  Loader2,
  Play,
  Sparkles,
} from "lucide-react";
import { useAIStore } from "../../stores/aiStore";
import "./LocalAIStatus.css";

/**
 * «ESTAT DE LA IA LOCAL» — la peça de disseny que fa que NoOrbit funcioni
 * de debò per a tothom: un xat amb un model local ha de deixar clar PER QUÈ
 * no respon i oferir la solució amb UN CLIC (instal·lar / engegar / baixar
 * el model inicial). Sense això, l'usuari veu un error genèric i pensa que
 * l'aplicació no funciona.
 */

export type LocalAIState =
  | "checking"
  | "missing" // Ollama no instal·lat
  | "stopped" // instal·lat però aturat
  | "no-model" // en marxa però sense cap model
  | "downloading" // baixant el model inicial
  | "ready";

export interface LocalAIHealth {
  state: LocalAIState;
  /// Model triat/actiu (si n'hi ha).
  model: string | null;
  /// Model inicial recomanat per a aquesta màquina.
  starter: string | null;
  /// Missatge de progrés de la baixada en curs.
  progress: string | null;
  /// Missatge d'error de l'última acció.
  error: string | null;
  busy: boolean;
  /// Executa l'acció que toca per a l'estat actual.
  fix: () => Promise<void>;
  refresh: () => Promise<void>;
}

/// Calcula la salut de la IA local i l'acció que cal fer.
export function useLocalAIHealth(): LocalAIHealth {
  const ollamaInstalled = useAIStore((s) => s.ollamaInstalled);
  const ollamaRunning = useAIStore((s) => s.ollamaRunning);
  const models = useAIStore((s) => s.models);
  const pulling = useAIStore((s) => s.pulling);
  const checkOllama = useAIStore((s) => s.checkOllama);
  const checkInstalled = useAIStore((s) => s.checkInstalled);
  const loadModels = useAIStore((s) => s.loadModels);
  const selectModel = useAIStore((s) => s.selectModel);

  const [starter, setStarter] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [checked, setChecked] = useState(false);

  const refresh = async () => {
    await checkInstalled();
    await checkOllama();
    await loadModels();
    setChecked(true);
  };

  useEffect(() => {
    void refresh();
    // El model inicial recomanat no canvia gairebé mai: es consulta un cop.
    invoke<string>("ollama_starter_model")
      .then(setStarter)
      .catch(() => setStarter(null));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Mentre hi hagi una baixada en curs, l'estat és «downloading» (amb el
  // progrés real que emet el backend a «ai://pull»).
  const pulls = Object.entries(pulling);
  const progress = pulls.length > 0 ? pulls[0][1] : null;
  const selected = useAIStore((s) => s.selectedModel);

  let state: LocalAIState = "checking";
  if (checked) {
    if (!ollamaInstalled) state = "missing";
    else if (!ollamaRunning) state = "stopped";
    else if (pulls.length > 0) state = "downloading";
    else if (models.length === 0) state = "no-model";
    else state = "ready";
  }

  const fix = async () => {
    setBusy(true);
    setError(null);
    try {
      if (state === "missing") {
        await invoke<string>("install_ollama");
      } else if (state === "stopped") {
        await invoke<string>("start_ollama");
      } else if (state === "no-model") {
        const name = await invoke<string>("ollama_pull_starter");
        await loadModels();
        if (name) await selectModel(name);
      }
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return {
    state,
    model: selected ?? models[0]?.name ?? null,
    starter,
    progress,
    error,
    busy,
    fix,
    refresh,
  };
}

/// Text curt de l'estat (català).
export function localAIStateLabel(h: LocalAIHealth): string {
  switch (h.state) {
    case "checking":
      return "Comprovant la IA local…";
    case "missing":
      return "Ollama no està instal·lat";
    case "stopped":
      return "Ollama està instal·lat però aturat";
    case "no-model":
      return "Cap model instal·lat: la IA no pot respondre";
    case "downloading":
      return h.progress ?? "Baixant el model…";
    case "ready":
      return h.model ? `A punt · ${h.model}` : "A punt";
  }
}

/// Botó d'acció segons l'estat (l'única cosa que l'usuari necessita).
function ActionButton({ h }: { h: LocalAIHealth }) {
  const label =
    h.state === "missing"
      ? "Instal·la"
      : h.state === "stopped"
        ? "Engega"
        : h.state === "no-model"
          ? h.starter
            ? `Baixa ${h.starter}`
            : "Baixa el model"
          : "";
  if (!label) return null;
  const Icon =
    h.state === "missing" ? Download : h.state === "stopped" ? Play : Sparkles;
  const busyLabel =
    h.state === "missing"
      ? "Instal·lant…"
      : h.state === "stopped"
        ? "Engegant…"
        : "Baixant…";
  return (
    <button className="btn sm primary lps-action" disabled={h.busy} onClick={() => void h.fix()}>
      {h.busy ? <Loader2 size={12} className="spin" /> : <Icon size={12} />}
      {h.busy ? busyLabel : label}
    </button>
  );
}

/// Barra de salut de la IA local per al panell del xat.
export default function LocalAIStatus() {
  const h = useLocalAIHealth();
  if (h.state === "ready") {
    // Estat correcte: només una línia discreta (no crida l'atenció).
    return (
      <div className="lps lps-ready" title="La IA local està instal·lada i en marxa.">
        <CheckCircle2 size={13} />
        <span className="lps-label">IA local</span>
        <span className="lps-detail">{h.model ?? "a punt"}</span>
      </div>
    );
  }
  return (
    <div className={"lps lps-" + h.state}>
      {h.state === "downloading" ? (
        <Loader2 size={13} className="spin" />
      ) : (
        <AlertTriangle size={13} />
      )}
      <Cpu size={13} className="lps-cpu" />
      <span className="lps-label">IA local</span>
      <span className="lps-detail">{localAIStateLabel(h)}</span>
      <ActionButton h={h} />
      {h.error && <span className="lps-error">{h.error}</span>}
    </div>
  );
}
