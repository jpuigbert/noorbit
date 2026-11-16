import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useShallow } from "zustand/react/shallow";
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
    rcPort,
    lastError,
    setProjectPath,
    setRcPort,
    connect,
    disconnect,
  } = useUnrealStore(
    useShallow((st) => ({
      connected: st.connected,
      status: st.status,
      statusMessage: st.statusMessage,
      projectPath: st.projectPath,
      engineVersion: st.engineVersion,
      projectName: st.projectName,
      installations: st.installations,
      rcPort: st.rcPort,
      lastError: st.lastError,
      setProjectPath: st.setProjectPath,
      setRcPort: st.setRcPort,
      connect: st.connect,
      disconnect: st.disconnect,
    }))
  );

  const busy = status === "detecting" || status === "launching" || status === "waiting" || status === "connecting";
  // Llista d'instal·lacions: sempre un array (mai null) — un null aquí
  // feia caure el component i, amb ell, TOTA la interfície.
  const inst = Array.isArray(installations) ? installations : [];

  // El port el resol el backend (config.toml) i arriba DESPRÉS del primer
  // dibuix, així que el camp segueix l'store fins que l'usuari l'edita.
  const [portText, setPortText] = useState(String(rcPort));
  useEffect(() => {
    setPortText(String(rcPort));
  }, [rcPort]);
  const portNet = Math.trunc(Number(portText) || 0);
  const portCanviat = portNet >= 1 && portNet <= 65535 && portNet !== rcPort;

  const pickProject = async () => {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Unreal Project", extensions: ["uproject"] }],
    });
    if (typeof sel === "string") setProjectPath(sel);
  };

  const PortField = () => (
    <div className="field">
      <label>{t("unreal.rcPort")}</label>
      <div className="si-pick">
        <input
          value={portText}
          inputMode="numeric"
          onChange={(e) => setPortText(e.target.value)}
        />
        <button
          className="btn sm"
          disabled={!portCanviat}
          onClick={() => setRcPort(portNet)}
        >
          {t("common.save")}
        </button>
      </div>
      <p className="lp-hint">{t("unreal.rcPortHint")}</p>
    </div>
  );

  // Els errors del panell (port, captura del viewport…) ja són en català i
  // diuen el MOTIU: abans es quedaven a l'store i l'usuari només veia que
  // «no passava res».
  const ErrorLine = () =>
    lastError ? <div className="ue-status err">{lastError}</div> : null;

  if (connected) {
    return (
      <div className="ue-connect">
        <div className="ue-connected">
          <span className="dot on" /> {t("unreal.connected")}
          <span className="ue-meta">
            {projectName ?? "—"} · UE {engineVersion ?? "?"} · :{rcPort}
          </span>
        </div>
        <button className="btn sm" onClick={disconnect}>
          <Unplug size={13} /> {t("common.disconnect")}
        </button>
        <PortField />
        <ErrorLine />
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
        {inst.length > 0 && (
          <p className="lp-hint">
            {t("unreal.installations")}: {inst.length}
          </p>
        )}
        {inst.length === 0 && (
          <p className="lp-hint">{t("unreal.noInstallations")}</p>
        )}
      </div>
      <PortField />
      <button className="btn primary" disabled={busy || !projectPath} onClick={() => connect()}>
        {busy ? <Loader2 size={13} className="spin" /> : <Plug size={13} />}
        {busy ? t("unreal.connecting") : t("unreal.connect")}
      </button>
      {statusMessage && (
        <div className={"ue-status " + (status === "error" ? "err" : "")}>{statusMessage}</div>
      )}
      <ErrorLine />
    </div>
  );
}
