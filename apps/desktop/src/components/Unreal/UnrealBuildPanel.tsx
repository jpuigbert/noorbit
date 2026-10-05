import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Package, Trash2, FolderSearch, Loader2 } from "lucide-react";
import { useT } from "../../i18n";
import { useUnrealStore } from "../../stores/unrealStore";

export default function UnrealBuildPanel() {
  const { t } = useT();
  const {
    installations,
    projectPath,
    building,
    buildOutput,
    lastBuildResult,
    packageGame,
    clearBuildOutput,
  } = useUnrealStore();

  const [engineRoot, setEngineRoot] = useState("");
  const [platform, setPlatform] = useState("Mac");
  const [configuration, setConfiguration] = useState("Development");
  const [outputDir, setOutputDir] = useState("");
  const outRef = useRef<HTMLPreElement>(null);

  useEffect(() => {
    if (!engineRoot && installations.length > 0) setEngineRoot(installations[0]);
  }, [installations, engineRoot]);

  useEffect(() => {
    if (outRef.current) outRef.current.scrollTop = outRef.current.scrollHeight;
  }, [buildOutput]);

  const project = projectPath
    ? projectPath.replace(/\.uproject$/i, "")
    : "";

  const run = () => {
    if (!engineRoot || !projectPath) return;
    packageGame(engineRoot, {
      project: projectPath,
      platform,
      configuration,
      output_dir: outputDir || undefined,
    });
  };

  const pickEngine = async () => {
    const sel = await open({ directory: true });
    if (typeof sel === "string") setEngineRoot(sel);
  };
  const pickOut = async () => {
    const sel = await open({ directory: true });
    if (typeof sel === "string") setOutputDir(sel);
  };

  return (
    <div className="ue-build">
      <div className="field">
        <label>{t("unreal.engineVersion")} (RunUAT)</label>
        <div className="si-pick">
          <select value={engineRoot} onChange={(e) => setEngineRoot(e.target.value)}>
            <option value="">—</option>
            {installations.map((i) => (
              <option key={i} value={i}>
                {i}
              </option>
            ))}
          </select>
          <button className="btn sm" onClick={pickEngine}>
            <FolderSearch size={13} />
          </button>
        </div>
      </div>

      <div className="ue-build-row">
        <div className="field" style={{ flex: 1 }}>
          <label>{t("unreal.build.platform")}</label>
          <select value={platform} onChange={(e) => setPlatform(e.target.value)}>
            <option value="Mac">Mac</option>
            <option value="Win64">Win64</option>
            <option value="Linux">Linux</option>
          </select>
        </div>
        <div className="field" style={{ flex: 1 }}>
          <label>{t("unreal.build.configuration")}</label>
          <select value={configuration} onChange={(e) => setConfiguration(e.target.value)}>
            <option value="Development">Development</option>
            <option value="Shipping">Shipping</option>
          </select>
        </div>
      </div>

      <div className="field">
        <label>{t("unreal.build.outputDir")}</label>
        <div className="si-pick">
          <input
            value={outputDir}
            onChange={(e) => setOutputDir(e.target.value)}
            placeholder={project ? project + "/Build" : "…"}
          />
          <button className="btn sm" onClick={pickOut}>
            <FolderSearch size={13} />
          </button>
        </div>
      </div>

      <button
        className="btn primary"
        disabled={building || !engineRoot || !projectPath}
        onClick={run}
      >
        {building ? <Loader2 size={13} className="spin" /> : <Package size={13} />}
        {building ? t("unreal.build.building") : t("unreal.build.package")}
      </button>

      {(buildOutput || lastBuildResult) && (
        <div className="ue-build-out">
          <div className="ue-live-head">
            <span>{t("unreal.build.output")}</span>
            <button className="icon-btn" onClick={clearBuildOutput} title={t("unreal.build.clear")}>
              <Trash2 size={13} />
            </button>
          </div>
          {lastBuildResult && (
            <div className={"ue-build-result " + (lastBuildResult.success ? "ok" : "fail")}>
              {lastBuildResult.success ? t("unreal.build.success") : t("unreal.build.failed")}
              {lastBuildResult.output_path ? ` → ${lastBuildResult.output_path}` : ""}
            </div>
          )}
          <pre ref={outRef} className="ue-build-log">
            {buildOutput}
          </pre>
        </div>
      )}
    </div>
  );
}
