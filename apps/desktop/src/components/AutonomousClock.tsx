import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Gamepad2, Rocket } from "lucide-react";
import { useT } from "../i18n";
import { usePreviewStore } from "../stores/previewStore";
import { useUIStore } from "../stores/uiStore";

/// Estat mínim de la Jam (el backend envia `jam://state`).
interface JamState {
  running: boolean;
  title: string;
  startedAt: number;
  /// Camí del projecte: sense això el tooltip no sabia QUIN projecte es
  /// tocava quan hi ha diverses finestres de NoOrbit obertes.
  projectPath?: string;
}

/// Estat mínim del treball autopilotat (el backend envia `project://work`).
interface WorkState {
  running: boolean;
  startedAt: number;
  projectPath?: string;
}

/// Temps en format rellotge «hh:mm:ss» (les hores poden passar de 24: és un
/// cronòmetre de treball, no l'hora del dia).
export function clockText(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(Math.floor(s / 3600))}:${pad(Math.floor((s % 3600) / 60))}:${pad(s % 60)}`;
}

/// RELLOTGE TEMPORAL de la barra d'estat: apareix NOMÉS mentre hi ha treball
/// autònom en marxa (Jam o treball autopilotat) i desapareix sol en acabar.
/// Un clic porta al lloc on es veu la feina en viu (panell Jam o el modal de
/// treball autopilotat).
export default function AutonomousClock() {
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

  // Tic cada segon mentre hi ha feina: el rellotge avança en viu i, quan
  // arriba l'event final (running=false), el component desapareix sol.
  useEffect(() => {
    if (!jamRunning && !workRunning) return;
    const id = setInterval(() => setTick((x) => x + 1), 1000);
    return () => clearInterval(id);
  }, [jamRunning, workRunning]);

  if (!jamRunning && !workRunning) return null;
  const now = Date.now();
  // Nom de la carpeta del projecte (últim segment del camí): és el que
  // l'usuari reconeix; el camí complet va al tooltip.
  const projName = (p?: string) =>
    (p ?? "").replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "";

  return (
    <>
      {jamRunning && (
        <button
          className="status-item as-btn status-auton"
          title={`${t("jam.tab")}: ${jam?.title ?? ""}${projName(jam?.projectPath) ? ` · ${projName(jam?.projectPath)}` : ""} · ${t("status.autonomousGo")}`}
          onClick={() => {
            // El panell dret pot estar TANCAT (l'usuari el va plegar o es va
            // tancar amb la Jam en marxa): canviar de pestanya sense obrir-lo
            // no deixava «tornar a obrir la pestanya» des del rellotge.
            setRightPanelOpen(true);
            setRightTab("agent");
            setAgentSub("jam");
          }}
        >
          <Gamepad2 size={12} /> {clockText(now - (jam?.startedAt ?? 0) * 1000)}
        </button>
      )}
      {workRunning && (
        <button
          className="status-item as-btn status-auton"
          title={`${t("projectWork.title")}${projName(work?.projectPath) ? ` · ${projName(work?.projectPath)}` : ""} · ${t("status.autonomousGo")}`}
          onClick={() => setShowProjectWork(true)}
        >
          <Rocket size={12} /> {clockText(now - (work?.startedAt ?? 0) * 1000)}
        </button>
      )}
    </>
  );
}
