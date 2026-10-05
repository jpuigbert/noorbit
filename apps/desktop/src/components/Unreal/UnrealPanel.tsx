import { useState, type ReactNode } from "react";
import { Code2, Boxes, Lightbulb, Video, Package, Play } from "lucide-react";
import { useT } from "../../i18n";
import { useUnrealStore } from "../../stores/unrealStore";
import UnrealConnectButton from "./UnrealConnectButton";
import UnrealLiveView from "./UnrealLiveView";
import UnrealBuildPanel from "./UnrealBuildPanel";
import "./Unreal.css";

type Tab = "python" | "objects" | "lighting" | "live" | "build";

export default function UnrealPanel() {
  const { t } = useT();
  const { connected, lastPythonResult, runPython, createBlueprint, spawnActor, buildLighting } =
    useUnrealStore();

  const [tab, setTab] = useState<Tab>("python");
  const [py, setPy] = useState(
    "import unreal\nprint(unreal.SystemLibrary.get_engine_version())"
  );
  const [bpName, setBpName] = useState("BP_NoOrbit");
  const [bpParent, setBpParent] = useState("Actor");
  const [actorClass, setActorClass] = useState("/Script/Engine.StaticMeshActor");
  const [locX, setLocX] = useState("0");
  const [locY, setLocY] = useState("0");
  const [locZ, setLocZ] = useState("0");
  const [quality, setQuality] = useState("Medium");

  const tabs: { key: Tab; label: string; icon: ReactNode }[] = [
    { key: "python", label: t("unreal.python"), icon: <Code2 size={13} /> },
    { key: "objects", label: t("unreal.blueprint.title"), icon: <Boxes size={13} /> },
    { key: "lighting", label: t("unreal.lighting.title"), icon: <Lightbulb size={13} /> },
    { key: "live", label: t("unreal.viewport.capture"), icon: <Video size={13} /> },
    { key: "build", label: t("unreal.build.title"), icon: <Package size={13} /> },
  ];

  const Result = () =>
    lastPythonResult ? (
      <pre className={"ue-result " + (lastPythonResult.success ? "ok" : "fail")}>
        {lastPythonResult.output.join("\n")}
        {lastPythonResult.errors.length > 0 &&
          "\n⚠ " + lastPythonResult.errors.join("\n")}
      </pre>
    ) : null;

  return (
    <div className="ue-panel">
      <UnrealConnectButton />

      <div className="ue-tabs">
        {tabs.map((tb) => (
          <button
            key={tb.key}
            className={"ue-tab" + (tab === tb.key ? " active" : "")}
            onClick={() => setTab(tb.key)}
          >
            {tb.icon} {tb.label}
          </button>
        ))}
      </div>

      <div className="ue-content">
        {!connected && tab !== "build" && (
          <div className="empty-hint">{t("unreal.notConnected")}</div>
        )}

        {connected && tab === "python" && (
          <div>
            <textarea
              className="ue-code"
              value={py}
              spellCheck={false}
              onChange={(e) => setPy(e.target.value)}
            />
            <button className="btn primary sm" onClick={() => runPython(py)}>
              <Play size={12} /> {t("unreal.runPython")}
            </button>
            <Result />
          </div>
        )}

        {connected && tab === "objects" && (
          <div className="ue-objects">
            <div className="field">
              <label>{t("unreal.blueprint.name")}</label>
              <input value={bpName} onChange={(e) => setBpName(e.target.value)} />
            </div>
            <div className="field">
              <label>{t("unreal.blueprint.parent")}</label>
              <input value={bpParent} onChange={(e) => setBpParent(e.target.value)} />
            </div>
            <button
              className="btn sm"
              onClick={() => createBlueprint(bpName, bpParent)}
              disabled={!bpName}
            >
              {t("unreal.blueprint.create")}
            </button>

            <div className="ue-sep" />

            <div className="field">
              <label>{t("unreal.actor.class")}</label>
              <input value={actorClass} onChange={(e) => setActorClass(e.target.value)} />
            </div>
            <div className="field">
              <label>Location (X, Y, Z)</label>
              <div className="ue-loc">
                <input value={locX} onChange={(e) => setLocX(e.target.value)} />
                <input value={locY} onChange={(e) => setLocY(e.target.value)} />
                <input value={locZ} onChange={(e) => setLocZ(e.target.value)} />
              </div>
            </div>
            <button
              className="btn sm"
              onClick={() =>
                spawnActor(actorClass, [
                  Number(locX) || 0,
                  Number(locY) || 0,
                  Number(locZ) || 0,
                ])
              }
            >
              {t("unreal.actor.spawn")}
            </button>
            <Result />
          </div>
        )}

        {connected && tab === "lighting" && (
          <div>
            <div className="field">
              <label>{t("unreal.lighting.quality")}</label>
              <select value={quality} onChange={(e) => setQuality(e.target.value)}>
                <option value="Preview">Preview</option>
                <option value="Medium">Medium</option>
                <option value="High">High</option>
                <option value="Production">Production</option>
              </select>
            </div>
            <button className="btn primary sm" onClick={() => buildLighting(quality)}>
              {t("unreal.lighting.build")}
            </button>
            <Result />
          </div>
        )}

        {connected && tab === "live" && <UnrealLiveView />}

        {tab === "build" && <UnrealBuildPanel />}
      </div>
    </div>
  );
}
