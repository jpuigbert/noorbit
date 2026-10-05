import { useState } from "react";
import { Play, Download, Video, RefreshCw, Boxes, Link } from "lucide-react";
import { useT } from "../../i18n";
import { useBlenderLiveStore } from "../../stores/blenderLiveStore";
import "./Blender.css";

export default function BlenderPanel() {
  const { t } = useT();
  const {
    connected,
    installed,
    live,
    autoConnect,
    busy,
    info,
    frame,
    lastError,
    lastMessage,
    launch,
    installAddon,
    toggleLive,
    setAutoConnect,
    refreshStatus,
    execute,
  } = useBlenderLiveStore();

  const [code, setCode] = useState(
    "import bpy, json\nprint(json.dumps({'objects': len(bpy.context.scene.objects)}))"
  );
  const [out, setOut] = useState<string | null>(null);

  const run = async () => {
    const r = await execute(code);
    setOut(r);
  };

  return (
    <div className="bl-panel">
      {/* Estat: binari + connexió al pont */}
      <div className="bl-status">
        <span className={"bl-dot" + (installed ? " ok" : "")} title={t("blender.binary")} />
        <span className="bl-status-text">
          {installed ? t("blender.installed") : t("blender.notInstalled")}
        </span>
        <span className={"bl-dot" + (connected ? " ok" : "")} title={t("blender.connection")} />
        <span className="bl-status-text">
          {connected ? t("blender.connected") : t("blender.disconnected")}
        </span>
        <button className="btn sm ghost" onClick={refreshStatus} title={t("blender.refresh")}>
          <RefreshCw size={12} />
        </button>
      </div>

      {/* Accions d'arrancada / instal·lació */}
      <div className="bl-actions">
        <button className="btn primary sm" onClick={launch} disabled={busy || !installed}>
          <Play size={12} /> {t("blender.launch")}
        </button>
        <button className="btn sm" onClick={installAddon} disabled={busy || !installed}>
          <Download size={12} /> {t("blender.installAddon")}
        </button>
        <button
          className={"btn sm" + (live ? " active" : "")}
          onClick={toggleLive}
          disabled={!connected}
        >
          <Video size={12} /> {t("blender.live")}
        </button>
        {/* Connexió automàtica (Objectiu 1): vigilant que connecta sol. */}
        <button
          className={"btn sm" + (autoConnect ? " active" : "")}
          onClick={() => void setAutoConnect(!autoConnect)}
          title="Connecta automàticament quan Blender obri el port"
        >
          <Link size={12} /> Auto
        </button>
      </div>

      {!installed && <div className="empty-hint">{t("blender.notInstalledHint")}</div>}

      {busy && <div className="bl-busy">{t("blender.working")}</div>}
      {lastMessage && <div className="bl-msg">{lastMessage}</div>}
      {lastError && <div className="bl-error">{lastError}</div>}

      {connected && info && (
        <div className="bl-info">
          <Boxes size={12} /> v{info.version ?? "?"} · {info.objects ?? 0} {t("blender.objects")}
          {info.file ? ` · ${info.file}` : ""}
        </div>
      )}

      {connected && frame && <img className="bl-frame" src={frame} alt="Blender" />}

      {connected ? (
        <div className="bl-exec">
          <textarea
            className="ue-code"
            value={code}
            spellCheck={false}
            onChange={(e) => setCode(e.target.value)}
          />
          <button className="btn primary sm" onClick={run}>
            <Play size={12} /> {t("blender.run")}
          </button>
          {out && <pre className="bl-out">{out}</pre>}
        </div>
      ) : (
        <div className="empty-hint">{t("blender.connectHint")}</div>
      )}
    </div>
  );
}
