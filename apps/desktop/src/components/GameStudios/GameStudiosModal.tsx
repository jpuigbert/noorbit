import { X, Gamepad2, Zap, Cloud, Bot, Globe, Image as ImageIcon, Box } from "lucide-react";
import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";

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