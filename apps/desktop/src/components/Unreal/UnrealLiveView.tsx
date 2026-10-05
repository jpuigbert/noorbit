import { Camera, Loader2 } from "lucide-react";
import { useT } from "../../i18n";
import { useUnrealStore } from "../../stores/unrealStore";

export default function UnrealLiveView() {
  const { t } = useT();
  const { connected, liveCapture, capturing, captureViewport } = useUnrealStore();

  return (
    <div className="ue-live">
      <div className="ue-live-head">
        <span>{t("unreal.viewport.capture")}</span>
        <button
          className="btn sm"
          disabled={!connected || capturing}
          onClick={captureViewport}
        >
          {capturing ? <Loader2 size={12} className="spin" /> : <Camera size={12} />}
          {t("unreal.viewport.capture")}
        </button>
      </div>
      {liveCapture ? (
        <img className="ue-live-img" src={liveCapture} alt="viewport" />
      ) : (
        <div className="empty-hint">{t("unreal.viewport.empty")}</div>
      )}
    </div>
  );
}
