import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  X, Gamepad2, Zap, Cloud, Bot, Globe, Image as ImageIcon, Box, Rocket, Loader2, AlertTriangle,
} from "lucide-react";
import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";
import { usePreviewStore } from "../../stores/previewStore";

const SHORTCUTS: { icon: any; title: string; items: string[] }[] = [
  {
    icon: Bot,
    title: "Flux base (totes les IA)",
    items: [
      "1. Nou projecte → Game Studios (clona la plantilla).",
      "2. Agent → tria la IA (local o núvol).",
      "3. Escriu: /prototype <idea del joc>.",
      "4. Respon les preguntes («tu tries» funciona).",
      "5. Revisa design/game-brief.md → «Construeix-lo».",
      "6. El joc queda a prototypes/<nom>/ (doble clic per obrir).",
    ],
  },
  {
    icon: Zap,
    title: "IA local (Ollama / llama.cpp)",
    items: [
      "Gestor de models → tria el model actiu (GGUF o Ollama).",
      "Recomanat per a proves ràpides i barates.",
      "Si el model és petit, demana «construeix-lo a mitjà».",
      "L'aprenentatge local recorda les teves preferències entre sessions.",
    ],
  },
  {
    icon: Cloud,
    title: "IA al núvol (OpenAI, Claude, Gemini, DeepSeek, Qwen, GLM, Grok…)",
    items: [
      "IA ▸ Gestor de models → connecta el token.",
      "Millor qualitat i velocitat per al primer build.",
      "Els especialistes poden usar models diferents per rol.",
    ],
  },
  {
    icon: Globe,
    title: "IA de web (DeepSeek, ChatGPT, Claude, Gemini, Perplexity, Grok…)",
    items: [
      "Panell «IA de web» → pregunta a una IA web (amb permís).",
      "NoOrbit extreu el codi (blocs estructurats) i el materialitza al projecte.",
      "Útil per comparar respostes o aprofitar models sense API.",
    ],
  },
  {
    icon: ImageIcon,
    title: "Imatges i assets",
    items: [
      "ComfyUI local (si la RAM ho permet) o via API amb la teua clau.",
      "Si la IA respon amb prosa, NoOrbit fa nudge «IMG|».",
      "Enganxa imatges al xat (⌘V) com a entrada per als models amb visió.",
    ],
  },
  {
    icon: Box,
    title: "3D (Blender / Unreal)",
    items: [
      "Game Studios es centra en jocs de navegador.",
      "Per a models 3D, usa els panells Blender / Unreal.",
      "Els assets es poden portar al projecte Vite+TS posterior.",
    ],
  },
];

export default function GameStudiosModal() {
  const show = useUIStore((s) => s.showGameStudios);
  const setShow = useUIStore((s) => s.setShowGameStudios);
  const { t } = useT();
  // Formulari de creació automàtica: la idea del joc + motor. La Jam
  // autònoma construeix el joc SOLA (sense tocar el PC) i el panell Jam
  // en mostra la feina en viu.
  const [desc, setDesc] = useState("");
  const [engine, setEngine] = useState("custom");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const createGame = async () => {
    if (!desc.trim()) {
      setErr(t("gameStudios.errDesc"));
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      await invoke("jam_start", {
        request: { brief: desc.trim(), engine, durationSecs: 60 * 60, buildCmd: null },
      });
      setShow(false);
      setDesc("");
      // Mostra la feina en viu al panell Jam de la pestanya Agent (obrint
      // també el panell dret per si l'usuari el tenia plegat).
      useUIStore.getState().setRightPanelOpen(true);
      usePreviewStore.getState().setRightTab("agent");
      usePreviewStore.getState().setAgentSub("jam");
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!show) return null;

  return (
    <div className="modal-overlay" onClick={() => setShow(false)}>
      <div className="modal" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 720 }}>
        <div className="modal-head">
          <span>
            <Gamepad2 size={15} style={{ marginRight: 6, verticalAlign: -2 }} />
            {t("menu.gameStudios")}
          </span>
          <button className="icon-btn" onClick={() => setShow(false)}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body">
          <p className="lp-hint">
            Plantilla i flux per crear jocs de navegador a partir d'una idea, compatible amb
            totes les IA connectades a NoOrbit. Repo original: donchitos/claude-code-game-studios.
          </p>

          {/* ── CREACIÓ AUTOMÀTICA: escriu la idea i la Jam construeix el joc
                 sola (cerca informació a internet sense obrir finestres ni
                 demanar permís). ── */}
          <div className="gs-create">
            <div className="field">
              <label>{t("gameStudios.desc")}</label>
              <textarea
                rows={3}
                value={desc}
                placeholder={t("gameStudios.descPh")}
                onChange={(e) => setDesc(e.target.value)}
              />
            </div>
            <div className="jam-row" style={{ marginTop: 8 }}>
              <div className="field" style={{ flex: 1 }}>
                <label>{t("gameStudios.engine")}</label>
                <select value={engine} onChange={(e) => setEngine(e.target.value)}>
                  <option value="custom">🌐 Web / HTML (navegador)</option>
                  <option value="godot4">🎮 Godot 4</option>
                  <option value="bevy">🦀 Bevy (Rust)</option>
                </select>
              </div>
            </div>
            <div className="lp-hint" style={{ marginTop: 6 }}>
              {t("gameStudios.hint")}
            </div>
            <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 10 }}>
              <button
                className="btn primary"
                disabled={busy || !desc.trim()}
                onClick={() => void createGame()}
              >
                {busy ? <Loader2 size={14} className="spin" /> : <Rocket size={14} />}{" "}
                {t("gameStudios.create")}
              </button>
            </div>
            {err && (
              <div className="si-error" style={{ marginTop: 8 }}>
                <AlertTriangle size={12} /> {err}
              </div>
            )}
          </div>

          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12, marginTop: 12 }}>
            {SHORTCUTS.map((s, i) => {
              const Icon = s.icon;
              return (
                <div key={i} className="si-result" style={{ flexDirection: "column", alignItems: "flex-start" }}>
                  <div className="si-result-name" style={{ display: "flex", alignItems: "center", gap: 6 }}>
                    <Icon size={13} /> {s.title}
                  </div>
                  <ul style={{ margin: "6px 0 0 16px", padding: 0, fontSize: 12 }}>
                    {s.items.map((it, j) => (
                      <li key={j} style={{ marginBottom: 2 }}>{it}</li>
                    ))}
                  </ul>
                </div>
              );
            })}
          </div>
          <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 16 }}>
            <button className="btn" onClick={() => setShow(false)}>
              {t("common.close")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}