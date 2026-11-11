import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertCircle,
  CheckCircle2,
  Download,
  Loader2,
  Play,
  RefreshCw,
  X,
} from "lucide-react";
import "./SetupCheckModal.css";

/**
 * Detecció de l'entorn A L'OBRIRE NoOrbit (primera vegada i totes les
 * demés): comprova si falten components (Ollama, Blender, ComfyUI) i, si
 * n'hi ha, pregunta a l'usuari si els vols instal·lar — amb botó que fa la
 * instal·lació AUTOMÀTICA nativa del sistema (Ollama: zip oficial / install.sh
 * / OllamaSetup.exe segons el SO). Si tot hi és, no molesta gens.
 */

type Item = {
  id: "ollama" | "blender" | "comfyui";
  name: string;
  desc: string;
  installed: boolean;
  running?: boolean;
  optional?: boolean;
};

export default function SetupCheckModal() {
  const [visible, setVisible] = useState(false);
  const [items, setItems] = useState<Item[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  /// «No tornar-la a mostrar» de l'usuari (persistant): si és «1», aquesta
  /// finestra no apareix mai més — ho pot tornar a activar des de Configuració.
  const [muted, setMuted] = useState(false);

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
    const list: Item[] = [
      {
        id: "ollama",
        name: "Ollama — IA local",
        desc: "Sense això el xat no pot pensar en local (no respon al port 11434).",
        installed: ollama,
        running: ollamaUp,
      },
      {
        id: "blender",
        name: "Blender — 3D i vídeo",
        desc: "Modelatge 3D i vídeos amb ordres de la IA.",
        installed: blender.installed,
      },
      {
        id: "comfyui",
        name: "ComfyUI — imatges (opcional)",
        desc: "Generació d'imatges local en CPU. No és imprescindible.",
        installed: comfy,
        optional: true,
      },
    ];
    setItems(list);
    // Només molesta si falta un component IMPRESCINDIBLE (Ollama o Blender);
    // ComfyUI és opcional i no obri la finestra sol, es veu dins la llista.
    if (!ollama || !blender.installed) setVisible(true);
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

  const act = async (item: Item) => {
    setBusy(item.id);
    setMsg(null);
    try {
      if (item.id === "ollama") {
        // Auto-instal·lació nativa del SO (o engegada del servei si ja hi és).
        setMsg(
          item.installed && !item.running
            ? await invoke<string>("start_ollama")
            : await invoke<string>("install_ollama")
        );
      } else if (item.id === "blender") {
        // Blender es baixa des de la pàgina oficial (instal·lador d'usuari).
        await invoke("open_externally", { path: "https://www.blender.org/download/" });
        setMsg(
          "S'ha obert la pàgina de descàrrega de Blender. Un cop instal·lat, prem «Revisa» i NoOrbit el detectarà sol."
        );
      } else {
        setMsg(await invoke<string>("install_comfyui"));
      }
      await detect();
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(null);
    }
  };

  if (!visible) return null;
  const missing = (items ?? []).filter((i) => !i.installed);
  return (
    <div className="modal-overlay" onClick={() => setVisible(false)}>
      <div className="modal sc-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>Comprovació de l'entorn de NoOrbit</span>
          <button className="icon-btn" onClick={() => setVisible(false)}>
            <X size={14} />
          </button>
        </div>
        <div className="modal-body">
          <p className="sc-intro">
            {items === null
              ? "Detectant components…"
              : missing.length === 0
                ? "Tot a punt: no falta res."
                : "Falten components perquè NoOrbit funcioni del tot. Es poden instal·lar automàticament:"}
          </p>
          <div className="sc-list">
            {items?.map((i) => (
              <div className={"sc-item" + (i.installed ? " ok" : "")} key={i.id}>
                {i.installed ? (
                  <CheckCircle2 size={15} className="sc-ok" />
                ) : (
                  <AlertCircle size={15} className="sc-warn" />
                )}
                <div className="sc-info">
                  <div className="sc-name">
                    {i.name}
                    {i.installed && i.running === false ? (
                      <span className="sc-off"> · aturat</span>
                    ) : null}
                  </div>
                  <div className="sc-desc">{i.desc}</div>
                </div>
                {i.installed ? (
                  i.running === false ? (
                    <button
                      className="btn sm"
                      disabled={busy !== null}
                      onClick={() => void act(i)}
                    >
                      {busy === i.id ? (
                        <Loader2 size={12} className="sc-spin" />
                      ) : (
                        <Play size={12} />
                      )}
                      Engega
                    </button>
                  ) : (
                    <span className="sc-state">Instal·lat</span>
                  )
                ) : (
                  <button
                    className="btn primary sm"
                    disabled={busy !== null}
                    onClick={() => void act(i)}
                  >
                    {busy === i.id ? (
                      <Loader2 size={12} className="sc-spin" />
                    ) : (
                      <Download size={12} />
                    )}
                    {busy === i.id ? "Instal·lant…" : "Instal·la"}
                  </button>
                )}
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
              Continua
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
