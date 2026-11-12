import { useEffect } from "react";
import { Loader2 } from "lucide-react";
import { useT } from "../i18n";
import { useWorkspaceStore } from "../stores/workspaceStore";
import { useBlenderLiveStore } from "../stores/blenderLiveStore";
import { useUnrealStore } from "../stores/unrealStore";
import { useAIStore } from "../stores/aiStore";
import { useProviderStore } from "../stores/providerStore";
import { useUIStore } from "../stores/uiStore";
import { useAIActivity } from "../stores/activityStore";
import { useSystemStatsStore, memPercent, formatBytes } from "../stores/systemStatsStore";
import BackgroundTaskBadge from "./BackgroundTaskBadge";
import AccountChip from "./Account/AccountChip";
import { ProblemsBadge } from "./Problems/ProblemsPanel";
import UpdateBadge from "./Update/UpdateBadge";

function Dot({ on }: { on: boolean }) {
  return <span className={"status-dot " + (on ? "on" : "off")} />;
}

export default function StatusBar() {
  const { t } = useT();
  const root = useWorkspaceStore((s) => s.root);
  const blender = useBlenderLiveStore((s) => s.connected);
  const unreal = useUnrealStore((s) => s.connected);
  const ollama = useAIStore((s) => s.ollamaRunning);
  const setShowSetupCheck = useUIStore((s) => s.setShowSetupCheck);
  const activity = useAIActivity();

  // RAM en temps real: arrenca el mostreig global (idempotent) i llig.
  const mem = useSystemStatsStore((s) => s.mem);
  useEffect(() => {
    useSystemStatsStore.getState().start();
  }, []);
  const ramPct = memPercent(mem);

  useEffect(() => {
    useAIStore.getState().checkOllama();
    const id = setInterval(() => useAIStore.getState().checkOllama(), 15000);
    return () => clearInterval(id);
  }, []);

  const folderName = root ? root.split(/[/\\]/).pop() : null;

  // Salut de la IA local (3 estils): instal·lada i amb model = a punt;
  // instal·lada però sense model = hi ha un pas pendent; no instal·lada.
  const ollamaInstalled = useAIStore((s) => s.ollamaInstalled);
  const models = useAIStore((s) => s.models);
  // IA EN LÍNIA activa: Ollama és opcional i NoOrbit funciona igual. La
  // barra no ha d'avisar de res en aquest cas.
  const cloudActive = useProviderStore((s) => s.active !== "ollama" && s.active !== "");
  const aiOk = cloudActive || (ollama && ollamaInstalled && models.length > 0);
  const aiLabel = cloudActive
    ? "IA: al núvol"
    : !ollamaInstalled
      ? "IA: no instal·lada"
      : !ollama
        ? "IA: aturada"
        : models.length === 0
          ? "IA: sense model"
          : `IA: ${models.length} model${models.length === 1 ? "" : "s"}`;
  const aiTitle = cloudActive
    ? "Estàs fent servir una IA en línia: Ollama no és necessari. Fes clic per veure l'entorn."
    : aiOk
      ? "La IA local està instal·lada i en marxa."
      : "Fes clic per posar la IA local en marxa (instal·lar, engegar o baixar model).";

  return (
    <div className="statusbar">
      {activity.working ? (
        <span className="status-item working">
          <Loader2 size={12} className="spin" /> {t(activity.labelKey)}
          {activity.detail && <span className="working-detail"> · {activity.detail}</span>}
        </span>
      ) : (
        <span className="status-item">{t("status.ready")}</span>
      )}
      <span className="status-item">
        {t("status.workspace")}: {folderName ?? t("status.noWorkspace")}
      </span>
      <ProblemsBadge />
      <UpdateBadge />
      <div className="spacer" />
      <AccountChip />
      <BackgroundTaskBadge />
      {ramPct !== null && (
        <span
          className="status-item"
          title={`RAM usada ${mem ? formatBytes(mem.used) : ""} de ${mem ? formatBytes(mem.total) : ""}`}
        >
          {t("status.ram")}: {ramPct}%
        </span>
      )}
      <span className="status-item status-ai" onClick={() => setShowSetupCheck(true)} title={aiTitle}>
        <Dot on={aiOk} /> {aiLabel}
      </span>
      <span className="status-item">
        <Dot on={blender} /> {t("status.blender")}
      </span>
      <span className="status-item">
        <Dot on={unreal} /> {t("status.unreal")}
      </span>
    </div>
  );
}
