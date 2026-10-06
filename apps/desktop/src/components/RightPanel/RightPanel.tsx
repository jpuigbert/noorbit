import { useEffect, type ReactNode } from "react";
import { Eye, Bot, Gamepad2, Play, Square, Terminal, UserCog, Boxes, GitBranch, Radio, Smartphone, AlertTriangle } from "lucide-react";
import { useT } from "../../i18n";
import { usePreviewStore, type RightTab } from "../../stores/previewStore";
import AgentPanel from "../Agent/AgentPanel";
import AIProcessViewer from "../AI/AIProcessViewer";
import ComputerPanel from "../Computer/ComputerPanel";
import ExpertsPanel from "../Experts/ExpertsPanel";
import UnrealPanel from "../Unreal/UnrealPanel";
import BlenderPanel from "../Blender/BlenderPanel";
import MobilePanel from "../Mobile/MobilePanel";
import GitPanel from "../Git/GitPanel";
import ProblemsPanel from "../Problems/ProblemsPanel";
import DebugConsole from "../Console/DebugConsole";

export default function RightPanel() {
  const { t } = useT();
  const { rightTab, setRightTab, previewUrl, previewActive, init, startPreview, stopPreview } =
    usePreviewStore();

  useEffect(() => {
    init();
  }, [init]);

  const tabs: { key: RightTab; label: string; icon: ReactNode }[] = [
    { key: "preview", label: t("rightPanel.preview"), icon: <Eye size={13} /> },
    { key: "agent", label: t("rightPanel.agent"), icon: <Bot size={13} /> },
    { key: "process", label: t("rightPanel.process"), icon: <Radio size={13} /> },
    { key: "computer", label: t("rightPanel.computer"), icon: <Terminal size={13} /> },
    { key: "experts", label: t("rightPanel.experts"), icon: <UserCog size={13} /> },
    { key: "unreal", label: t("menu.unreal"), icon: <Gamepad2 size={13} /> },
    { key: "blender", label: t("menu.blender"), icon: <Boxes size={13} /> },
    { key: "mobile", label: t("menu.mobile"), icon: <Smartphone size={13} /> },
    { key: "git", label: t("rightPanel.git"), icon: <GitBranch size={13} /> },
    { key: "problems", label: t("rightPanel.problems"), icon: <AlertTriangle size={13} /> },
    { key: "console", label: t("rightPanel.console"), icon: <Terminal size={13} /> },
  ];

  return (
    <div className="rightpanel">
      <div className="rp-tabs">
        {tabs.map((tb) => (
          <button
            key={tb.key}
            className={"rp-tab" + (rightTab === tb.key ? " active" : "")}
            onClick={() => setRightTab(tb.key)}
          >
            {tb.icon} {tb.label}
          </button>
        ))}
      </div>
      <div className="rp-body">
        {rightTab === "preview" && (
          <div className="preview-wrap">
            <div className="preview-bar">
              <span className="preview-url">{previewUrl ?? t("rightPanel.previewNotRunning")}</span>
              {previewActive ? (
                <button className="btn sm" onClick={stopPreview}>
                  <Square size={12} /> {t("common.stop")}
                </button>
              ) : (
                <button className="btn sm" onClick={() => startPreview()}>
                  <Play size={12} /> {t("rightPanel.startPreview")}
                </button>
              )}
            </div>
            {previewActive && previewUrl ? (
              <iframe className="preview-frame" src={previewUrl} title="preview" />
            ) : (
              <div className="empty-hint" style={{ marginTop: 40 }}>
                {t("rightPanel.previewNotRunning")}
              </div>
            )}
          </div>
        )}
        {rightTab === "agent" && <AgentPanel />}
        {rightTab === "process" && <AIProcessViewer />}
        {rightTab === "computer" && <ComputerPanel />}
        {rightTab === "experts" && <ExpertsPanel />}
        {rightTab === "unreal" && <UnrealPanel />}
        {rightTab === "blender" && <BlenderPanel />}
        {rightTab === "mobile" && <MobilePanel />}
        {rightTab === "git" && <GitPanel />}
        {rightTab === "problems" && <ProblemsPanel />}
        {rightTab === "console" && <DebugConsole />}
      </div>
    </div>
  );
}
