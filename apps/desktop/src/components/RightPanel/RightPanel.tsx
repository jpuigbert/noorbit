import { useEffect, useState, type ReactNode } from "react";
import { Eye, Bot, Play, Square, Globe, Layers, Wrench } from "lucide-react";
import { useT } from "../../i18n";
import { usePreviewStore, type RightTab } from "../../stores/previewStore";
import AgentPanel from "../Agent/AgentPanel";
import WebAiPanel from "../WebAi/WebAiPanel";
import AIProcessViewer from "../AI/AIProcessViewer";
import ComputerPanel from "../Computer/ComputerPanel";
import ExpertsPanel from "../Experts/ExpertsPanel";
import UnrealPanel from "../Unreal/UnrealPanel";
import BlenderPanel from "../Blender/BlenderPanel";
import MobilePanel from "../Mobile/MobilePanel";
import GitPanel from "../Git/GitPanel";
import ProblemsPanel from "../Problems/ProblemsPanel";
import DebugConsole from "../Console/DebugConsole";

/// PESTANYES AGRUPADES (poc de lios fora): hi havia 12 capes i ofegava. Ara
/// mateix hi ha 5 pestanyes superiors; les eines viuen a dins de dues carpetes
/// amb sub-pestanyes (mateixa vista, zero canvi d'estructura dels panellets):
/// · «IA» (xat agent) · «IA de web» · «Vista prèvia» · «Integracions» · «Treball»
const SUB_INTEGRA = ["experts", "computer", "unreal", "blender", "mobile"] as const;
const SUB_DEV = ["process", "git", "problems", "console"] as const;
type SubKeyInt = (typeof SUB_INTEGRA)[number];
type SubKeyDev = (typeof SUB_DEV)[number];

export default function RightPanel() {
  const { t } = useT();
  const { rightTab, setRightTab, previewUrl, previewActive, init, startPreview, stopPreview } =
    usePreviewStore();
  // Record de les sub-pestanyes (local al panell, no cal persistir).
  const [subInt, setSubInt] = useState<SubKeyInt>("experts");
  const [subDev, setSubDev] = useState<SubKeyDev>("process");

  useEffect(() => {
    init();
  }, [init]);

  const tabs: { key: RightTab; label: string; icon: ReactNode }[] = [
    { key: "agent", label: t("rightPanel.agent"), icon: <Bot size={13} /> },
    { key: "web", label: t("webAi.title"), icon: <Globe size={13} /> },
    { key: "preview", label: t("rightPanel.preview"), icon: <Eye size={13} /> },
    { key: "integra", label: t("rightPanel.integra"), icon: <Layers size={13} /> },
    { key: "dev", label: t("rightPanel.dev"), icon: <Wrench size={13} /> },
  ];

  const subIntLabels: Record<SubKeyInt, string> = {
    experts: t("rightPanel.experts"),
    computer: t("rightPanel.computer"),
    unreal: t("menu.unreal"),
    blender: t("menu.blender"),
    mobile: t("menu.mobile"),
  };
  const subDevLabels: Record<SubKeyDev, string> = {
    process: t("rightPanel.process"),
    git: t("rightPanel.git"),
    problems: t("rightPanel.problems"),
    console: t("rightPanel.console"),
  };

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
        {rightTab === "agent" && <AgentPanel />}
        {rightTab === "web" && <WebAiPanel />}

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

        {/* ── INTEGRACIONS: totes les eines d'integració en UN lloc ── */}
        {rightTab === "integra" && (
          <div className="rp-group">
            <div className="rp-subtabs">
              {SUB_INTEGRA.map((k) => (
                <button
                  key={k}
                  className={"rp-subtab" + (subInt === k ? " active" : "")}
                  onClick={() => setSubInt(k)}
                >
                  {subIntLabels[k]}
                </button>
              ))}
            </div>
            <div className="rp-group-body">
              {subInt === "experts" && <ExpertsPanel />}
              {subInt === "computer" && <ComputerPanel />}
              {subInt === "unreal" && <UnrealPanel />}
              {subInt === "blender" && <BlenderPanel />}
              {subInt === "mobile" && <MobilePanel />}
            </div>
          </div>
        )}

        {/* ── TREBALL: activitat IA, Git, problemes i consola en UN lloc ──
            AÇÍ és on viu «què està fent la IA ara mateix», TOT en un sol
            registre — el xat només queda amb les respostes. */}
        {rightTab === "dev" && (
          <div className="rp-group">
            <div className="rp-subtabs">
              {SUB_DEV.map((k) => (
                <button
                  key={k}
                  className={"rp-subtab" + (subDev === k ? " active" : "")}
                  onClick={() => setSubDev(k)}
                >
                  {subDevLabels[k]}
                </button>
              ))}
            </div>
            <div className="rp-group-body">
              {subDev === "process" && <AIProcessViewer />}
              {subDev === "git" && <GitPanel />}
              {subDev === "problems" && <ProblemsPanel />}
              {subDev === "console" && <DebugConsole />}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
