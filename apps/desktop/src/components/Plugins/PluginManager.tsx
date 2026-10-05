import { useState } from "react";
import { X, Puzzle, Sparkles } from "lucide-react";
import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";
import SkillInstaller from "../Skills/SkillInstaller";
import ProviderManager from "../AI/ProviderManager";

type Tab = "skills" | "providers";

export default function PluginManager() {
  const show = useUIStore((s) => s.showPlugins);
  const setShow = useUIStore((s) => s.setShowPlugins);
  const { t } = useT();
  const [tab, setTab] = useState<Tab>("skills");

  if (!show) return null;

  return (
    <div className="modal-overlay" onClick={() => setShow(false)}>
      <div className="modal pm-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>{t("plugins.title")}</span>
          <button className="icon-btn" onClick={() => setShow(false)}>
            <X size={16} />
          </button>
        </div>
        <div className="pm-tabs">
          <button
            className={"pm-tab" + (tab === "skills" ? " active" : "")}
            onClick={() => setTab("skills")}
          >
            <Puzzle size={14} /> {t("skills.title")}
          </button>
          <button
            className={"pm-tab" + (tab === "providers" ? " active" : "")}
            onClick={() => setTab("providers")}
          >
            <Sparkles size={14} /> {t("providers.title")}
          </button>
        </div>
        <div className="modal-body">{tab === "skills" ? <SkillInstaller /> : <ProviderManager />}</div>
      </div>
    </div>
  );
}
