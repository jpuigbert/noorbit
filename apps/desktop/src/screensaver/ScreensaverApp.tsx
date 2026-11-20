/// PANTALLA DE FEINA (estalvi propi de NoOrbit).
///
/// Es pinta DINS de la finestra «noorbit-screensaver» (fullscreen, sense
/// decoració) que crea `screensaver/window.ts`. L'usuari la deixa posada
/// mentre la Jam o el treball autopilotat fan hores o dies de codi: veu de
/// lluny QUÈ s'està fent, QUANT queda i en QUIN projecte, tot sense tocar
/// res. Un clic o qualsevol tecla la tanca i torna al programa.
///
/// No fa servir cap store (esta finestra no carrega App): l'estat ve
/// DIRECTAMENT del backend (`jam_state`/`project_work_state` + els events
/// globals), perquè una webview separada no comparteix la memòria viva dels
/// stores del programa principal.
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Gamepad2, Rocket, Folder, Brain, Wrench } from "lucide-react";
import { useT } from "../i18n";
import WorkProgress from "../components/WorkProgress";
import { clockText } from "../components/AutonomousClock";
import { tancaEstalvi } from "./window";
import "./Screensaver.css";

interface JamState {
  running: boolean;
  title: string;
  completed: number;
  blocked: number;
  skipped: number;
  total: number;
  currentTask: string;
  currentDetail: string;
  startedAt: number;
  finishedAt: number | null;
  durationSecs: number;
  projectPath: string;
}

interface WorkState {
  running: boolean;
  pass: number;
  elapsedMs: number;
  budgetMs: number;
  filesChanged: number;
  detail: string;
  startedAt: number;
  finishedAt: number | null;
  projectPath: string;
}

/// Esdeveniment del registre de la Jam (`jam://event`): el consumim per
/// pintar en viu QUÈ fa la IA (kind «note») i el seu PENSAMENT («thinking»).
interface JamEvent {
  kind: string;
  task: string;
  detail: string;
  ok: boolean;
}

/// Estrelles del fons: generades una sola vegada (sempre les mateixes durant
/// la vida de la finestra; un estalvi no ha de canviar de fons cada segon).
const STARS = Array.from({ length: 70 }, (_, i) => ({
  id: i,
  left: `${(i * 137.3) % 100}%`,
  top: `${(i * 71.7) % 100}%`,
  size: 1 + ((i * 13) % 3),
  delay: `${(i % 17) * 0.9}s`,
}));

/// Nom de la carpeta del projecte (últim segment del camí): és el que
/// l'usuari reconeix, no el camí absolut sencer.
const nomProjecte = (p?: string) =>
  (p ?? "").replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "";

export default function ScreensaverApp() {
  const { t } = useT();
  const [jam, setJam] = useState<JamState | null>(null);
  const [work, setWork] = useState<WorkState | null>(null);
  const [, setTick] = useState(0);
  // Feed viu de «què fa la IA» + pensament (només la Jam ho emet).
  const [feed, setFeed] = useState<{ kind: string; text: string }[]>([]);
  const feedRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void invoke<JamState>("jam_state").then(setJam).catch(() => undefined);
    void invoke<WorkState>("project_work_state").then(setWork).catch(() => undefined);
    const u1 = listen<JamState>("jam://state", (e) => setJam(e.payload));
    const u2 = listen<WorkState>("project://work", (e) => setWork(e.payload));
    const u3 = listen<JamEvent>("jam://event", (e) => {
      const ev = e.payload;
      if (ev.kind !== "note" && ev.kind !== "thinking") return;
      // La feina de la Jam es reinicia entre tasques: buidem el feed
      // quan comenca una Jam nova (un «thinking» és el senyal de vida).
      setFeed((prev) => [...prev.slice(-30), { kind: ev.kind, text: ev.detail }]);
    });
    return () => {
      void u1.then((u) => u());
      void u2.then((u) => u());
      void u3.then((u) => u());
    };
  }, []);

  // Autoscroll: l'últim que diu la IA sempre visible (estil terminal).
  useEffect(() => {
    const el = feedRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [feed]);

  const jamRunning = jam?.running === true;
  const workRunning = work?.running === true;
  const actiu = jamRunning || workRunning;

  // Tic d'un segon: el gran cronòmetre central avança en viu.
  useEffect(() => {
    const id = setInterval(() => setTick((x) => x + 1), 1000);
    return () => clearInterval(id);
  }, []);

  // Clic o qualsevol tecla → tornar al programa (la feina continua al darrere).
  useEffect(() => {
    const close = () => void tancaEstalvi();
    window.addEventListener("keydown", close);
    window.addEventListener("pointerdown", close);
    return () => {
      window.removeEventListener("keydown", close);
      window.removeEventListener("pointerdown", close);
    };
  }, []);

  const now = Date.now();
  const elapsedMs = actiu
    ? now - (jamRunning ? jam?.startedAt ?? 0 : work?.startedAt ?? 0) * 1000
    : 0;

  const doneTasks = jam
    ? Math.min(jam.total || jam.completed, jam.completed + jam.blocked + jam.skipped)
    : 0;
  const project = jamRunning ? jam?.projectPath : workRunning ? work?.projectPath : "";
  const mode: "tasks" | "time" = jamRunning ? "tasks" : "time";
  const detail = jamRunning ? jam?.currentTask || jam?.currentDetail || "" : work?.detail || "";

  return (
    <div className="saver-root">
      <div className="saver-stars" aria-hidden>
        {STARS.map((s) => (
          <i
            key={s.id}
            style={{
              left: s.left,
              top: s.top,
              width: s.size,
              height: s.size,
              animationDelay: s.delay,
            }}
          />
        ))}
      </div>

      <div className="saver-brand">
        <span className="saver-logo">NoOrbit</span>
        <span className={"saver-mode" + (actiu ? "" : " idle")}>
          {jamRunning ? (
            <Gamepad2 size={13} />
          ) : workRunning ? (
            <Rocket size={13} />
          ) : null}
          {jamRunning
            ? t("jam.title")
            : workRunning
              ? t("projectWork.title")
              : t("screensaver.idle")}
        </span>
      </div>

      {actiu ? (
        <>
          <div className="saver-title">{jamRunning ? jam?.title || t("jam.title") : t("projectWork.title")}</div>
          <div className="saver-clock">{clockText(elapsedMs)}</div>
          <div className="saver-progress">
            <WorkProgress
              running={actiu}
              startedAtMs={(jamRunning ? jam?.startedAt ?? 0 : work?.startedAt ?? 0) * 1000}
              mode={mode}
              doneUnits={doneTasks}
              totalUnits={jam?.total ?? 0}
              budgetMs={work?.budgetMs ?? 0}
              deadlineMs={
                jamRunning && jam && jam.durationSecs > 0
                  ? (jam.startedAt + jam.durationSecs) * 1000
                  : null
              }
              finishedAtMs={
                (jamRunning ? jam?.finishedAt : work?.finishedAt)
                  ? (jamRunning ? jam?.finishedAt : work?.finishedAt)! * 1000
                  : null
              }
            />
          </div>
          {detail && <div className="saver-detail">{detail}</div>}
          {jamRunning && feed.length > 0 && (
            <div className="saver-live">
              <div className="saver-live-head">
                <Wrench size={13} /> {t("screensaver.doing")}
              </div>
              <div className="saver-feed" ref={feedRef}>
                {feed.map((f, i) => (
                  <div key={i} className={"saver-feed-row " + f.kind}>
                    {f.kind === "thinking" ? (
                      <Brain size={12} />
                    ) : (
                      <Wrench size={12} />
                    )}
                    <span className="saver-feed-text">{f.text}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
          {project && (
            <div className="saver-project" title={project}>
              <Folder size={14} /> {nomProjecte(project)}
            </div>
          )}
        </>
      ) : (
        <div className="saver-title idle">{t("screensaver.done")}</div>
      )}

      <div className="saver-hint">{t("screensaver.hint")}</div>
    </div>
  );
}
