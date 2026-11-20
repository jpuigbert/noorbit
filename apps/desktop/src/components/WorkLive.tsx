import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Gamepad2, Rocket } from "lucide-react";
import { useT } from "../i18n";
import { usePreviewStore } from "../stores/previewStore";
import { useUIStore } from "../stores/uiStore";
import { progressPct } from "../utils/workTime";
import { clockText } from "./AutonomousClock";

/// Estat de la Jam que arriba per `jam://state` (el backend hi envia el total
/// de tasques del pla que s'està executant: el denominador real del progrés).
interface JamState {
  running: boolean;
  title: string;
  completed: number;
  total: number;
  currentTask: string;
  startedAt: number;
  /// Projecte (arrel) on la Jam treballa: la franja l'ha de dir perquè amb
  /// diverses finestres obertes l'usuari sàpiga QUIN s'està tocant.
  projectPath?: string;
}

/// Estat del treball autopilotat que arriba per `project://work`.
interface WorkState {
  running: boolean;
  pass: number;
  elapsedMs: number;
  budgetMs: number;
  filesChanged: number;
  detail: string;
  startedAt: number;
  projectPath?: string;
}

/// FRANJA DE FEINA VIVA (pestanya «Treball»): mentre la Jam o el treball
/// autopilotat estan en marxa, aquí es veu la feina en viu — tasca actual,
/// barra de progrés i cronòmetre — sense haver de canviar de pestanya.
/// Un clic porta al panell corresponent. Desapareix quan no hi ha res.
export default function WorkLive() {
  const { t } = useT();
  const setRightTab = usePreviewStore((s) => s.setRightTab);
  const setAgentSub = usePreviewStore((s) => s.setAgentSub);
  const setShowProjectWork = useUIStore((s) => s.setShowProjectWork);
  const setRightPanelOpen = useUIStore((s) => s.setRightPanelOpen);
  const [jam, setJam] = useState<JamState | null>(null);
  const [work, setWork] = useState<WorkState | null>(null);
  const [, setTick] = useState(0);

  useEffect(() => {
    void invoke<JamState>("jam_state").then(setJam).catch(() => undefined);
    void invoke<WorkState>("project_work_state").then(setWork).catch(() => undefined);
    const u1 = listen<JamState>("jam://state", (e) => setJam(e.payload));
    const u2 = listen<WorkState>("project://work", (e) => setWork(e.payload));
    return () => {
      void u1.then((u) => u());
      void u2.then((u) => u());
    };
  }, []);

  const jamRunning = jam?.running === true;
  const workRunning = work?.running === true;

  // Tic cada segon mentre hi ha feina: el cronòmetre avança en viu.
  useEffect(() => {
    if (!jamRunning && !workRunning) return;
    const id = setInterval(() => setTick((x) => x + 1), 1000);
    return () => clearInterval(id);
  }, [jamRunning, workRunning]);

  if (!jamRunning && !workRunning) return null;
  const now = Date.now();
  const projName = (p?: string) =>
    (p ?? "").replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "";

  return (
    <div className="work-live">
      {jamRunning && (
        <button
          className="work-live-item"
          title={t("status.autonomousGo")}
          onClick={() => {
            // Obrir TAMBÉ el panell dret: si era tancat, canviar de pestanya
            // no deixava a l'usuari «tornar a obrir la pestanya».
            setRightPanelOpen(true);
            setRightTab("agent");
            setAgentSub("jam");
          }}
        >
          <Gamepad2 size={12} className="spin-slow" />
          <span className="work-live-body">
            <span className="work-live-title">
              <b>{jam?.title || t("jam.tab")}</b>
              <em>
                {jam?.currentTask ?? ""}
                {projName(jam?.projectPath) ? ` · ${t("jam.project")}: ${projName(jam?.projectPath)}` : ""}
              </em>
            </span>
            <span className="work-live-bar">
              <i style={{ width: `${progressPct(jam?.completed ?? 0, jam?.total ?? 0)}%` }} />
            </span>
          </span>
          <span className="work-live-time">{clockText(now - (jam?.startedAt ?? 0) * 1000)}</span>
        </button>
      )}
      {workRunning && (
        <button
          className="work-live-item"
          title={t("status.autonomousGo")}
          onClick={() => setShowProjectWork(true)}
        >
          <Rocket size={12} className="spin-slow" />
          <span className="work-live-body">
            <span className="work-live-title">
              <b>{t("projectWork.title")}</b>
              <em>
                {work?.pass ? `${t("wpTheme.workPass")} ${work.pass}` : ""}
                {work?.filesChanged ? ` · ${work.filesChanged} ${t("wpTheme.workFiles")}` : ""}
                {projName(work?.projectPath) ? ` · ${t("jam.project")}: ${projName(work?.projectPath)}` : ""}
                {work?.detail ? ` · ${work.detail}` : ""}
              </em>
            </span>
            <span className="work-live-bar">
              <i style={{ width: `${progressPct(work?.elapsedMs ?? 0, work?.budgetMs ?? 0)}%` }} />
            </span>
          </span>
          <span className="work-live-time">{clockText(now - (work?.startedAt ?? 0) * 1000)}</span>
        </button>
      )}
    </div>
  );
}
