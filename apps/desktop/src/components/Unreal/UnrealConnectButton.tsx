import { open } from "@tauri-apps/plugin-dialog";
import { Plug, Unplug, FolderSearch, Loader2 } from "lucide-react";
import { useT } from "../../i18n";
import { useUnrealStore } from "../../stores/unrealStore";

export default function UnrealConnectButton() {
  const { t } = useT();
  const {
    connected,
    status,
    statusMessage,
    projectPath,
    engineVersion,
    projectName,
    installations,
    setProjectPath,
    connect,
    disconnect,
  } = useUnrealStore();

  const busy = status === "detecting" || status === "launching" || status === "waiting" || status === "connecting";

  const pickProject = async () => {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Unreal Project", extensions: ["uproject"] }],
    });
    if (typeof sel === "string") setProjectPath(sel);
  };

  if (connected) {
    return (
      <div className="ue-connect">
        <div className="ue-connected">
          <span className="dot on" /> {t("unreal.connected")}
          <span className="ue-meta">
            {projectName ?? "—"} · UE {engineVersion ?? "?"}
          </span>
        </div>
        <button className="btn sm" onClick={disconnect}>
          <Unplug size={13} /> {t("common.disconnect")}
        </button>
      </div>
    );
  }

  return (
    <div className="ue-connect">
      <div className="field">
        <label>{t("unreal.project")}</label>
        <div className="si-pick">
          <input
            readOnly
            value={projectPath ?? ""}
            placeholder={t("unreal.selectProject")}
          />
          <button className="btn sm" onClick={pickProject}>
            <FolderSearch size={13} /> {t("common.select")}
          </button>
        </div>
        {installations.length > 0 && (
          <p className="lp-hint">
            {t("unreal.installations")}: {installations.length}
          </p>
        )}
        {installations.length === 0 && (
          <p className="lp-hint">{t("unreal.noInstallations")}</p>
        )}
      </div>
      <button className="btn primary" disabled={busy || !projectPath} onClick={() => connect()}>
        {busy ? <Loader2 size={13} className="spin" /> : <Plug size={13} />}
        {busy ? t("unreal.connecting") : t("unreal.connect")}
      </button>
      {statusMessage && (
        <div className={"ue-status " + (status === "error" ? "err" : "")}>{statusMessage}</div>
      )}
    </div>
  );
}
