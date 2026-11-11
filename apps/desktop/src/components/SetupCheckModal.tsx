import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertCircle,
  CheckCircle2,
  Download,
  Loader2,
  Play,
  RefreshCw,
  Sparkles,
  X,
} from "lucide-react";
import { useAIStore } from "../stores/aiStore";
import { useUIStore } from "../stores/uiStore";
import "./SetupCheckModal.css";

/**
 * DETECCIÓ DE L'ENTORN A L'OBRIR NoOrbit: quatre passos reals, un darrere
 * l'altre, com un assistent d'instal·lació. En lloc de «falta X», mostra
 * «pas 2 de 4» amb el progrés real de la baixada i un botó per continuar.
 *
 *  1. Ollama (motor de la IA local)  → instal·la (nadiu del SO)
 *  2. Servei en marxa (port 11434)   → engega
 *  3. Model inicial (mínim 1)        → baixa (segons la RAM de la màquina)
 *  4. Eines 3D (Blender / ComfyUI)   → instal·lador oficial
 *
 * La finestra apareix SEMPRE que falti alguna cosa, tret que l'usuari premi
 * «No tornar-la a mostrar» (persistit a localStorage).
 */

type StepState = "ok" | "todo" | "busy" | "warn" | "skip";

interface Step {
  id: string;
  name: string;
  desc: string;
  state: StepState;
  /// Botó d'acció (null = res a fer).
  action?: { label: string; run: () => Promise<void> };
  /// Nota en vermell (error de l'última acció).
  error?: string | null;
  /// Text de progrés (p. ex. «descarregant 42 %»).
  progress?: string | null;
}

export default function SetupCheckModal() {
  const [visible, setVisible] = useState(false);
  const [steps, setSteps] = useState<Step[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [starter, setStarter] = useState<string | null>(null);
  /// «No tornar-la a mostrar» de l'usuari (persistant).
  const [muted, setMuted] = useState(false);

  const pulling = useAIStore((s) => s.pulling);
  const models = useAIStore((s) => s.models);
  const loadModels = useAIStore((s) => s.loadModels);
  /// Obertura manual (barra d'estat): l'usuari pot demanar l'assistent
  /// quan vulgui, encara que l'hagi silenciat.
  const showSetupCheck = useUIStore((s) => s.showSetupCheck);
  const setShowSetupCheck = useUIStore((s) => s.setShowSetupCheck);

  useEffect(() => {
    if (showSetupCheck) {
      setVisible(true);
      void detect();
      setShowSetupCheck(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showSetupCheck]);

  const muteForever = () => {
    try {
      localStorage.setItem("noorbit.setupCheckHidden", "1");
    } catch {
      /* sense persistència: només aquesta sessió */
    }
    setMuted(true);
    setVisible(false);
  };

  const detect = async () => {
    const [ollama, ollamaUp, blender, comfy] = await Promise.all([
      invoke<boolean>("ollama_installed").catch(() => false),
      invoke<boolean>("ollama_is_running").catch(() => false),
      invoke<{ installed: boolean }>("blender_status").catch(() => ({ installed: false })),
      invoke<boolean>("comfyui_installed").catch(() => false),
    ]);
    await loadModels().catch(() => undefined);
    let starterName = starter;
    if (!starterName) {
      starterName = await invoke<string>("ollama_starter_model").catch(() => null);
      setStarter(starterName);
    }
    const progress = Object.values(pulling)[0] ?? null;
    const next: Step[] = [
      {
        id: "ollama",
        name: "Ollama — motor de la IA local",
        desc: "El motor que fa que el xat funcioni al teu ordinador, sense Internet.",
        state: ollama ? "ok" : "todo",
        action: ollama
          ? undefined
          : {
              label: "Instal·la",
              run: async () => {
                setMsg(await invoke<string>("install_ollama"));
              },
            },
      },
      {
        id: "serve",
        name: "Servei de l'IA en marxa",
        desc: "Ollama ha d'escoltar al port 11434 per poder respondre.",
        state: ollamaUp ? "ok" : ollama ? "todo" : "skip",
        action:
          ollamaUp || !ollama
            ? undefined
            : {
                label: "Engega",
                run: async () => {
                  setMsg(await invoke<string>("start_ollama"));
                },
              },
      },
      {
        id: "model",
        name: "Model de llenguatge",
        desc: starterName
          ? `Cap model no serveix de res: baixa «${starterName}» (l'adaptat a la memòria d'aquest equip).`
          : "Cap model no serveix de res: baixa un model perquè la IA pugui pensar.",
        state:
          models.length > 0
            ? "ok"
            : !ollamaUp
              ? "skip"
              : pulling.length > 0
                ? "busy"
                : "todo",
        progress,
        action:
          models.length > 0 || !ollamaUp
            ? undefined
            : {
                label: starterName ? `Baixa ${starterName}` : "Baixa el model",
                run: async () => {
                  const name = await invoke<string>("ollama_pull_starter");
                  setMsg(`Model «${name}» a punt.`);
                },
              },
      },
      {
        id: "blender",
        name: "Blender — 3D i vídeo",
        desc: "Per modelar en 3D i fer vídeos amb ordres de la IA.",
        state: blender.installed ? "ok" : "todo",
        action: blender.installed
          ? undefined
          : {
              label: "Instal·la",
              run: async () => {
                await invoke("open_externally", {
                  path: "https://www.blender.org/download/",
                });
                setMsg(
                  "S'ha obert la pàgina oficial de Blender. Un cop instal·lat, prem «Revisa» i NoOrbit el detectarà sol."
                );
              },
            },
      },
      {
        id: "comfyui",
        name: "ComfyUI — imatges (opcional)",
        desc: "Generació d'imatges local en CPU. No és imprescindible: es pot endur endarrere.",
        state: comfy ? "ok" : "warn",
        action: comfy
          ? undefined
          : {
              label: "Instal·la",
              run: async () => {
                setMsg(await invoke<string>("install_comfyui"));
              },
            },
      },
    ];
    setSteps(next);
    // Obre la finestra si falta alguna cosa IMPORTANT (Ollama, servei, model
    // o Blender). ComfyUI és opcional i no la fa obrir sol.
    const faltaImportant = next.some(
      (s) => (s.id === "ollama" || s.id === "serve" || s.id === "model" || s.id === "blender") && s.state !== "ok"
    );
    if (faltaImportant) setVisible(true);
  };

  useEffect(() => {
    try {
      if (localStorage.getItem("noorbit.setupCheckHidden") === "1") {
        setMuted(true);
        return; // l'usuari va triar «no tornar-la a mostrar»: respecta-ho
      }
    } catch {
      /* sense persistència */
    }
    void detect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Si el backend baixa un model automàticament (l'IA engegada en segon pla),
  // l'interface s'actualitza sola.
  useEffect(() => {
    if (!visible) return;
    const id = setInterval(() => void detect(), 4000);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible, pulling]);

  const act = async (step: Step) => {
    if (!step.action) return;
    setBusy(step.id);
    setMsg(null);
    try {
      await step.action.run();
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(null);
      await detect();
    }
  };

  if (!visible) return null;
  const list = steps ?? [];
  const pendents = list.filter((s) => s.state === "todo" || s.state === "busy").length;
  const nextStep = list.find((s) => s.action && s.state !== "ok");

  return (
    <div className="modal-overlay" onClick={() => setVisible(false)}>
      <div className="modal sc-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>Posant NoOrbit en marxa</span>
          <button className="icon-btn" onClick={() => setVisible(false)}>
            <X size={14} />
          </button>
        </div>
        <div className="modal-body">
          <p className="sc-intro">
            {steps === null
              ? "Comprovant l'entorn…"
              : pendents === 0
                ? "Tot a punt: NoOrbit ja pot treballar amb la IA local."
                : `Falt ${pendents} pas${pendents === 1 ? "" : "s"} perquè tot funcioni. Tu pots continuar i nosaltres ho fem tot sol.`}
          </p>

          <div className="sc-list">
            {list.map((s, i) => (
              <div className={"sc-item" + (s.state === "ok" ? " ok" : "")} key={s.id}>
                <span className="sc-step-no">
                  {s.state === "ok" ? (
                    <CheckCircle2 size={15} className="sc-ok" />
                  ) : s.state === "busy" ? (
                    <Loader2 size={15} className="sc-spin sc-accent" />
                  ) : s.state === "skip" ? (
                    <span className="sc-skip">—</span>
                  ) : s.state === "warn" ? (
                    <AlertCircle size={15} className="sc-warn" />
                  ) : (
                    <span className="sc-num">{i + 1}</span>
                  )}
                </span>
                <div className="sc-info">
                  <div className="sc-name">
                    {s.name}
                    {s.state === "skip" ? <span className="sc-off"> · pendent</span> : null}
                    {s.state === "ok" ? <span className="sc-ok-tag"> · a punt</span> : null}
                  </div>
                  <div className="sc-desc">{s.desc}</div>
                  {s.progress ? <div className="sc-progress">{s.progress}</div> : null}
                </div>
                {s.action ? (
                  <button
                    className="btn primary sm"
                    disabled={busy !== null}
                    onClick={() => void act(s)}
                  >
                    {busy === s.id ? (
                      <Loader2 size={12} className="sc-spin" />
                    ) : s.id === "model" ? (
                      <Sparkles size={12} />
                    ) : s.id === "serve" ? (
                      <Play size={12} />
                    ) : (
                      <Download size={12} />
                    )}
                    {busy === s.id ? "En curs…" : s.action.label}
                  </button>
                ) : s.state === "ok" ? (
                  <span className="sc-state">Fet</span>
                ) : null}
              </div>
            ))}
          </div>

          {msg && <div className="sc-msg">{msg}</div>}

          <div className="mm-ollama-actions" style={{ justifyContent: "space-between" }}>
            <div className="sc-left-actions">
              <button className="btn sm ghost" disabled={busy !== null} onClick={() => void detect()}>
                <RefreshCw size={12} /> Revisa
              </button>
              {!muted && (
                <button className="btn sm ghost" disabled={busy !== null} onClick={muteForever}>
                  <X size={12} /> No tornar-la a mostrar
                </button>
              )}
            </div>
            <button className="btn primary sm" onClick={() => setVisible(false)}>
              {nextStep ? "Continua igual" : "Comença"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
