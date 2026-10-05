import { useEffect } from "react";
import { Loader2 } from "lucide-react";
import { useT } from "../i18n";
import { useWorkspaceStore } from "../stores/workspaceStore";
import { useBlenderLiveStore } from "../stores/blenderLiveStore";
import { useUnrealStore } from "../stores/unrealStore";
import { useAIStore } from "../stores/aiStore";
import { useAIActivity } from "../stores/activityStore";
import BackgroundTaskBadge from "./BackgroundTaskBadge";

function Dot({ on }: { on: boolean }) {
  return <span className={"status-dot " + (on ? "on" : "off")} />;
}

export default function StatusBar() {
  const { t } = useT();
  const root = useWorkspaceStore((s) => s.root);
  const blender = useBlenderLiveStore((s) => s.connected);
  const unreal = useUnrealStore((s) => s.connected);
  const ollama = useAIStore((s) => s.ollamaRunning);
  const activity = useAIActivity();

  useEffect(() => {
    useAIStore.getState().checkOllama();
    const id = setInterval(() => useAIStore.getState().checkOllama(), 15000);
    return () => clearInterval(id);
  }, []);

  const folderName = root ? root.split(/[/\\]/).pop() : null;

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
      <div className="spacer" />
      <BackgroundTaskBadge />
      <span className="status-item">
        <Dot on={ollama} /> {t("status.ai")}
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
