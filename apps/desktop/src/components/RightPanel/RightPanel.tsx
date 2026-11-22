import { useEffect, type ReactNode } from "react";
import { useShallow } from "zustand/react/shallow";
import {
  Eye, Bot, Play, Square, Globe, Users, Monitor, Boxes, Clapperboard, ExternalLink,
  Smartphone, Brain, Activity, GitBranch, AlertTriangle, Terminal, Gamepad2,
} from "lucide-react";
import { useT } from "../../i18n";
import { usePreviewStore, type AgentSub, type RightTab } from "../../stores/previewStore";
import { useUIStore } from "../../stores/uiStore";
import { useProblemsStore } from "../../stores/problemsStore";
import { useGitStatusStore } from "../../stores/gitStatusStore";
import { detachChat, reattachChat } from "../../windowing";
import { useExpertStore } from "../../stores/expertStore";
import AgentPanel from "../Agent/AgentPanel";
import WebAiPanel from "../WebAi/WebAiPanel";
import AIProcessViewer from "../AI/AIProcessViewer";
import ComputerPanel from "../Computer/ComputerPanel";
import ExpertsPanel from "../Experts/ExpertsPanel";
import LearningPanel from "../Learning/LearningPanel";
import JamPanel from "../Jam/JamPanel";
import UnrealPanel from "../Unreal/UnrealPanel";
import BlenderPanel from "../Blender/BlenderPanel";
import PanelErrorBoundary from "../PanelErrorBoundary";
import MobilePanel from "../Mobile/MobilePanel";
import GitPanel from "../Git/GitPanel";
import ProblemsPanel from "../Problems/ProblemsPanel";
import DebugConsole from "../Console/DebugConsole";
import WorkLive from "../WorkLive";

/// PESTANYES (Fase 4): Git, Problemes i Consola són de PRIMER NIVELL —
/// l'usuari no ha de plegar «Treball» per veure què passa al projecte.
/// Mapa final: Agent (sub-pestanyes chat/experts/jam/learning/computer/
/// unreal/blender/mobile intactes) · Procés IA · Git · Problemes · Consola ·
/// IA web · Vista prèvia. «Procés IA» es manté com a pestanya pròpia: és
/// activitat de la IA, no de l'equip, i les sub-pestanyes d'Agent no es toquen.
/// Pestanyes compactes: icona + tooltip; l'activa mostra també l'etiqueta.
const SUB_AGENT = [
  "chat",
  "experts",
  "jam",
  "learning",
  "computer",
  "unreal",
  "blender",
  "mobile",
] as const;
type SubKeyAgent = (typeof SUB_AGENT)[number];

const SUB_ICONS: Record<SubKeyAgent, ReactNode> = {
  chat: <Bot size={12} />,
  experts: <Users size={12} />,
  jam: <Gamepad2 size={12} />,
  learning: <Brain size={12} />,
  computer: <Monitor size={12} />,
  unreal: <Boxes size={12} />,
  blender: <Clapperboard size={12} />,
  mobile: <Smartphone size={12} />,
};

export default function RightPanel() {
  const { t } = useT();
  const { rightTab, setRightTab, agentSub, setAgentSub, previewUrl, previewActive, init, startPreview, stopPreview } =
    usePreviewStore(
      useShallow((st) => ({
        rightTab: st.rightTab,
        setRightTab: st.setRightTab,
        agentSub: st.agentSub,
        setAgentSub: st.setAgentSub,
        previewUrl: st.previewUrl,
        previewActive: st.previewActive,
        init: st.init,
        startPreview: st.startPreview,
        stopPreview: st.stopPreview,
      }))
    );
  // El xat pot estar en FINESTRA SEPARADA (per posar-lo a una altra pantalla):
  // mentre ho sigui, aquí només es mostra un marc amb el botó de reenganxar
  // (un sol lloc edita els xats: el calaix de localStorage no es baralla).
  const chatDetached = useUIStore((s) => s.chatDetached);
  const teamRunning = useExpertStore((s) => s.teamRunning);
  const teamProgress = useExpertStore((s) => s.teamProgress);
  // Badges BARATS de primer nivell: errors de Problemes (store local) i
  // fitxers canviats de Git (store compartit, sense consulta extra).
  const byFile = useProblemsStore((s) => s.byFile);
  const gitChanges = useGitStatusStore((s) => s.status?.changes.length ?? 0);
  let errorCount = 0;
  for (const list of Object.values(byFile)) {
    for (const p of list) if (p.severity === "error") errorCount++;
  }

  useEffect(() => {
    init();
  }, [init]);

  const tabs: { key: RightTab; label: string; icon: ReactNode; badge?: number }[] = [
    { key: "agent", label: t("rightPanel.agent"), icon: <Bot size={14} /> },
    { key: "process", label: t("rightPanel.process"), icon: <Activity size={14} /> },
    { key: "git", label: t("rightPanel.git"), icon: <GitBranch size={14} />, badge: gitChanges },
    { key: "problems", label: t("rightPanel.problems"), icon: <AlertTriangle size={14} />, badge: errorCount },
    { key: "console", label: t("rightPanel.console"), icon: <Terminal size={14} /> },
    { key: "web", label: t("webAi.title"), icon: <Globe size={14} /> },
    { key: "preview", label: t("rightPanel.preview"), icon: <Eye size={14} /> },
  ];

  const subAgentLabels: Record<SubKeyAgent, string> = {
    chat: t("agent.chat"),
    experts: t("rightPanel.experts"),
    jam: t("jam.tab"),
    learning: t("rightPanel.learning"),
    computer: t("rightPanel.computer"),
    unreal: t("menu.unreal"),
    blender: t("menu.blender"),
    mobile: t("menu.mobile"),
  };
  // Últim pas broadcast per l'equip (per la franja «equip en marxa»).
  const lastStep = teamProgress[teamProgress.length - 1];

  return (
    <div className="rightpanel">
      <div className="rp-tabs" role="tablist">
        {tabs.map((tb) => (
          <button
            key={tb.key}
            role="tab"
            aria-selected={rightTab === tb.key}
            className={"rp-tab" + (rightTab === tb.key ? " active" : "")}
            title={tb.label}
            onClick={() => setRightTab(tb.key)}
          >
            {tb.icon}
            <span className="rp-tab-label">{tb.label}</span>
            {!!tb.badge && <span className="rp-badge">{tb.badge > 99 ? "99+" : tb.badge}</span>}
            {tb.key === "agent" && teamRunning && <i className="rp-team-dot" title={t("agent.teamOn")} />}
          </button>
        ))}
      </div>
      <div className="rp-body">
        {/* Feina autònoma en marxa (Jam / treball autopilotat): franja viva
            a les pestanyes de treball, com abans a «Treball». Es plega sola
            (null) quan no hi ha res en marxa. */}
        {(rightTab === "process" || rightTab === "git" || rightTab === "problems" || rightTab === "console") && (
          <WorkLive />
        )}
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
              {agentSub === "chat" && (
                <button
                  className="icon-btn rp-detach"
                  title={t("panel.detachChat")}
                  onClick={() => void detachChat()}
                >
                  <ExternalLink size={12} />
                </button>
              )}
            </div>
            <div className="rp-group-body">
              {/* Cada panell va amb la seva pròpia xarxa de seguretat: si un
                  peta (un null d'una dada), només mor AQUELL panell — la resta
                  de NoOrbit (xat, editor, terminal) continua funcionant. */}
              {agentSub === "chat" &&
                (chatDetached ? (
                  <div className="empty-hint" style={{ marginTop: 48 }}>
                    {t("panel.detachedHint")}
                    <div style={{ marginTop: 10 }}>
                      <button
                        className="btn sm primary"
                        onClick={() => void reattachChat()}
                      >
                        {t("panel.reattachHere")}
                      </button>
                    </div>
                  </div>
                ) : (
                  <PanelErrorBoundary label={t("rightPanel.errChat")}>
                    <AgentPanel />
                  </PanelErrorBoundary>
                ))}
              {agentSub === "experts" && (
                <PanelErrorBoundary label={t("rightPanel.errExperts")}>
                  <ExpertsPanel />
                </PanelErrorBoundary>
              )}
              {agentSub === "jam" && (
                <PanelErrorBoundary label={t("rightPanel.errJam")}>
                  <JamPanel />
                </PanelErrorBoundary>
              )}
              {agentSub === "learning" && (
                <PanelErrorBoundary label={t("rightPanel.errLearning")}>
                  <LearningPanel />
                </PanelErrorBoundary>
              )}
              {agentSub === "computer" && (
                <PanelErrorBoundary label={t("rightPanel.errComputer")}>
                  <ComputerPanel />
                </PanelErrorBoundary>
              )}
              {agentSub === "unreal" && (
                <PanelErrorBoundary label={t("rightPanel.errUnreal")}>
                  <UnrealPanel />
                </PanelErrorBoundary>
              )}
              {agentSub === "blender" && (
                <PanelErrorBoundary label={t("rightPanel.errBlender")}>
                  <BlenderPanel />
                </PanelErrorBoundary>
              )}
              {agentSub === "mobile" && (
                <PanelErrorBoundary label={t("rightPanel.errMobile")}>
                  <MobilePanel />
                </PanelErrorBoundary>
              )}
            </div>
          </div>
        )}

        {rightTab === "web" && (
          <PanelErrorBoundary label={t("rightPanel.errWebAi")}>
            <WebAiPanel />
          </PanelErrorBoundary>
        )}

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
              <iframe className="preview-frame" src={previewUrl} title={t("rightPanel.previewFrame")} />
            ) : (
              <div className="empty-hint" style={{ marginTop: 40 }}>
                {t("rightPanel.previewNotRunning")}
              </div>
            )}
          </div>
        )}

        {/* ── PESTANYES DE TREBALL DE PRIMER NIVELL (Fase 4) ──
            «Què està fent la IA ara mateix» (Procés), Git, Problemes i
            Consola: un clic des de qualsevol lloc, sense sub-pestanyes. */}
        {rightTab === "process" && (
          <PanelErrorBoundary label={t("rightPanel.errProcess")}>
            <AIProcessViewer />
          </PanelErrorBoundary>
        )}
        {rightTab === "git" && (
          <PanelErrorBoundary label={t("rightPanel.errGit")}>
            <GitPanel />
          </PanelErrorBoundary>
        )}
        {rightTab === "problems" && (
          <PanelErrorBoundary label={t("rightPanel.errProblems")}>
            <ProblemsPanel />
          </PanelErrorBoundary>
        )}
        {rightTab === "console" && (
          <PanelErrorBoundary label={t("rightPanel.errConsole")}>
            <DebugConsole />
          </PanelErrorBoundary>
        )}
      </div>
    </div>
  );
}
