import { useEffect, useState, type ReactNode } from "react";
import {
  Eye, Bot, Play, Square, Globe, Users, Monitor, Boxes, Clapperboard,
  Smartphone, Wrench, Workflow, Brain,
} from "lucide-react";
import { useT } from "../../i18n";
import { usePreviewStore, type AgentSub, type RightTab } from "../../stores/previewStore";
import { useExpertStore } from "../../stores/expertStore";
import AgentPanel from "../Agent/AgentPanel";
import WebAiPanel from "../WebAi/WebAiPanel";
import AIProcessViewer from "../AI/AIProcessViewer";
import ComputerPanel from "../Computer/ComputerPanel";
import ExpertsPanel from "../Experts/ExpertsPanel";
import LearningPanel from "../Learning/LearningPanel";
import UnrealPanel from "../Unreal/UnrealPanel";
import BlenderPanel from "../Blender/BlenderPanel";
import MobilePanel from "../Mobile/MobilePanel";
import GitPanel from "../Git/GitPanel";
import ProblemsPanel from "../Problems/ProblemsPanel";
import DebugConsole from "../Console/DebugConsole";

/// PESTANYES: des de la v0.71 les «Integracions» NO són una pestanya a part:
/// viuen DINS de la pestanya Agent com a sub-pestanyes (així veus sempre, al
/// costat del xat, si l'equip d'especialistes està treballant i què fa).
/// Exterior: «Agent» · «IA de web» · «Vista prèvia» · «Treball».
const SUB_AGENT = [
  "chat",
  "experts",
  "learning",
  "computer",
  "unreal",
  "blender",
  "mobile",
] as const;
const SUB_DEV = ["process", "git", "problems", "console"] as const;
type SubKeyAgent = (typeof SUB_AGENT)[number];
type SubKeyDev = (typeof SUB_DEV)[number];

const SUB_ICONS: Record<SubKeyAgent, ReactNode> = {
  chat: <Bot size={12} />,
  experts: <Users size={12} />,
  learning: <Brain size={12} />,
  computer: <Monitor size={12} />,
  unreal: <Boxes size={12} />,
  blender: <Clapperboard size={12} />,
  mobile: <Smartphone size={12} />,
};
const SUB_DEV_ICONS: Record<SubKeyDev, ReactNode> = {
  process: <Workflow size={12} />,
  git: <Globe size={12} />,
  problems: <Workflow size={12} />,
  console: <Wrench size={12} />,
};

export default function RightPanel() {
  const { t } = useT();
  const { rightTab, setRightTab, agentSub, setAgentSub, previewUrl, previewActive, init, startPreview, stopPreview } =
    usePreviewStore();
  // Record de la sub-pestanya de Treball (local al panell, no cal persistir).
  const [subDev, setSubDev] = useSubDev();
  const teamRunning = useExpertStore((s) => s.teamRunning);
  const teamProgress = useExpertStore((s) => s.teamProgress);

  useEffect(() => {
    init();
  }, [init]);

  const tabs: { key: RightTab; label: string; icon: ReactNode }[] = [
    { key: "agent", label: t("rightPanel.agent"), icon: <Bot size={13} /> },
    { key: "web", label: t("webAi.title"), icon: <Globe size={13} /> },
    { key: "preview", label: t("rightPanel.preview"), icon: <Eye size={13} /> },
    { key: "dev", label: t("rightPanel.dev"), icon: <Wrench size={13} /> },
  ];

  const subAgentLabels: Record<SubKeyAgent, string> = {
    chat: t("agent.chat"),
    experts: t("rightPanel.experts"),
    learning: t("rightPanel.learning"),
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
  // Últim pas broadcast per l'equip (per la franja «equip en marxa»).
  const lastStep = teamProgress[teamProgress.length - 1];

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
            {tb.key === "agent" && teamRunning && <i className="rp-team-dot" title={t("agent.teamOn")} />}
          </button>
        ))}
      </div>
      <div className="rp-body">
        {rightTab === "agent" && (
          <div className="rp-group">
            {/* Franja viva: l'equip d'especialistes treballant — sempre
                visible des del costat del xat, amb un clic veges el detall. */}
            {teamRunning && agentSub !== "experts" && (
              <button className="rp-team-strip" onClick={() => setAgentSub("experts")}>
                <Users size={12} className="spin-slow" />
                <span>{t("agent.teamOn")}</span>
                {lastStep && <em>{lastStep.detail}</em>}
              </button>
            )}
            <div className="rp-subtabs rp-subtabs-agent">
              {SUB_AGENT.map((k) => (
                <button
                  key={k}
                  className={"rp-subtab" + (agentSub === k ? " active" : "")}
                  onClick={() => setAgentSub(k as AgentSub)}
                >
                  {SUB_ICONS[k]} {subAgentLabels[k]}
                </button>
              ))}
            </div>
            <div className="rp-group-body">
              {agentSub === "chat" && <AgentPanel />}
              {agentSub === "experts" && <ExpertsPanel />}
              {agentSub === "learning" && <LearningPanel />}
              {agentSub === "computer" && <ComputerPanel />}
              {agentSub === "unreal" && <UnrealPanel />}
              {agentSub === "blender" && <BlenderPanel />}
              {agentSub === "mobile" && <MobilePanel />}
            </div>
          </div>
        )}

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
                  {SUB_DEV_ICONS[k]} {subDevLabels[k]}
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

/// (hook mínim per poder canviar-ho a estat compartit després)
function useSubDev(): [SubKeyDev, (k: SubKeyDev) => void] {
  const [k, setK] = useState<SubKeyDev>("process");
  return [k, setK];
}
