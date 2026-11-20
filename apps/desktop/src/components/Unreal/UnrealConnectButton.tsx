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
    pendingInstallations,
    engineRoot,
    rcPort,
    lastError,
    setProjectPath,
    setEngineRoot,
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
      pendingInstallations: st.pendingInstallations,
      engineRoot: st.engineRoot,
      rcPort: st.rcPort,
      lastError: st.lastError,
      setProjectPath: st.setProjectPath,
      setEngineRoot: st.setEngineRoot,
      setRcPort: st.setRcPort,
      connect: st.connect,
      disconnect: st.disconnect,
    }))
  );

  const busy = status === "detecting" || status === "launching" || status === "waiting" || status === "connecting";
  // Llista d'instal·lacions: sempre un array (mai null) — un null aquí
  // feia caure el component i, amb ell, TOTA la interfície.
  const inst = Array.isArray(installations) ? installations : [];
  // Motors a mitges (l'Epic Launcher encara baixant-los): no serveixen per
  // connectar, però l'usuari ha de saber que hi són i on.
  const pending = Array.isArray(pendingInstallations)
    ? pendingInstallations
    : [];

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

  /// Tria la carpeta del motor a mà: necessari quan el Unreal viu en un lloc
  /// que la detecció automàtica no veu (un disc extern no muntat, una unitat
  /// de xarxa amb permisos, una carpeta amb només el motor).
  const pickEngine = async () => {
    const sel = await open({
      directory: true,
      multiple: false,
      title: t("unreal.engine"),
    });
    if (typeof sel === "string") setEngineRoot(sel);
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
      </div>
      <div className="field">
        <label>{t("unreal.engine")}</label>
        <div className="si-pick">
          <select
            value={engineRoot ?? ""}
            onChange={(e) => setEngineRoot(e.target.value || null)}
          >
            <option value="">{t("unreal.engineAuto")}</option>
            {inst.map((i) => (
              <option key={i} value={i}>
                {i}
              </option>
            ))}
            {engineRoot && !inst.includes(engineRoot) && (
              <option value={engineRoot}>{engineRoot}</option>
            )}
          </select>
          <button className="btn sm" onClick={pickEngine}>
            <FolderSearch size={13} /> {t("common.select")}
          </button>
        </div>
        {inst.length > 0 && (
          <>
            <p className="lp-hint">
              {t("unreal.installations")}: {inst.length}
            </p>
            <ul className="ue-paths">
              {inst.map((i) => (
                <li key={i}>{i}</li>
              ))}
            </ul>
          </>
        )}
        {inst.length === 0 && (
          <p className="lp-hint">{t("unreal.noInstallations")}</p>
        )}
        {pending.length > 0 && (
          <div className="ue-status warn">
            {t("unreal.pendingInstallations")}
            <ul className="ue-paths">
              {pending.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          </div>
        )}
        <p className="lp-hint">{t("unreal.engineHint")}</p>
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
