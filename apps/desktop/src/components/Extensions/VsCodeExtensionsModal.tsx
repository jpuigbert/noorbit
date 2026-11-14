import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { X, Code2, RefreshCw, Download, Check, Languages, FileText, Palette } from "lucide-react";
import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";
import "./VsCodeExtensionsModal.css";

interface VsCodeExt {
  id: string;
  display: string;
  version: string;
  description: string;
  source: string;
  commands: string[];
  languages: string[];
  snippets: [string, string][];
  themes: string[];
  imported: boolean;
}

export default function VsCodeExtensionsModal() {
  const { t } = useT();
  const show = useUIStore((s) => s.showVsCodeExtensions);
  const setShow = useUIStore((s) => s.setShowVsCodeExtensions);
  const [exts, setExts] = useState<VsCodeExt[]>([]);
  const [busy, setBusy] = useState(false);
  const [info, setInfo] = useState<string | null>(null);

  const load = async () => {
    setBusy(true);
    try {
      const list = await invoke<VsCodeExt[]>("vscode_list_extensions");
      setExts(list);
    } catch (e) {
      setInfo(String(e));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (show) void load();
  }, [show]);

  const importAll = async () => {
    setBusy(true);
    setInfo(null);
    try {
      const rep = await invoke<{ scanned: number; imported: number; details: string[] }>(
        "vscode_import_skills"
      );
      setInfo(
        rep.imported > 0
          ? `Importades ${rep.imported} extensions com a skills.`
          : rep.details.join(" · ") || "Cap extensió importada."
      );
      await load();
    } catch (e) {
      setInfo(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!show) return null;

  return (
    <div className="modal-overlay" onClick={() => setShow(false)}>
      <div className="modal vsc-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>
            <Code2 size={15} style={{ marginRight: 6, verticalAlign: -2 }} />
            {t("vsc.title")}
          </span>
          <button className="icon-btn" onClick={() => setShow(false)}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body vsc-body">
          <p className="lp-hint">{t("vsc.intro")}</p>
          <div className="vsc-actions">
            <button className="btn sm" onClick={() => void load()} disabled={busy}>
              <RefreshCw size={12} /> {t("common.refresh")}
            </button>
            <button className="btn primary sm" onClick={() => void importAll()} disabled={busy}>
              <Download size={12} /> {t("vsc.importAll")}
            </button>
          </div>
          {info && <div className="vsc-info">{info}</div>}
          {busy && exts.length === 0 ? (
            <div className="empty-hint">{t("common.loading")}</div>
          ) : exts.length === 0 ? (
            <div className="empty-hint">{t("vsc.empty")}</div>
          ) : (
            <div className="vsc-list">
              {exts.map((e) => (
                <div key={e.id} className="vsc-row">
                  <div className="vsc-main">
                    <div className="vsc-name">
                      {e.display}
                      <span className="vsc-ver">v{e.version}</span>
                      {e.imported && (
                        <span className="vsc-badge">
                          <Check size={10} /> {t("vsc.imported")}
                        </span>
                      )}
                    </div>
                    <div className="vsc-id">{e.id} · {e.source}</div>
                    {e.description && <div className="vsc-desc">{e.description}</div>}
                    <div className="vsc-contribs">
                      {e.languages.length > 0 && (
                        <span className="vsc-chip">
                          <Languages size={10} /> {e.languages.join(", ")}
                        </span>
                      )}
                      {e.commands.length > 0 && (
                        <span className="vsc-chip">
                          <Code2 size={10} /> {e.commands.length} comandes
                        </span>
                      )}
                      {e.snippets.length > 0 && (
                        <span className="vsc-chip">
                          <FileText size={10} /> {e.snippets.length} fragments
                        </span>
                      )}
                      {e.themes.length > 0 && (
                        <span className="vsc-chip">
                          <Palette size={10} /> {e.themes.length} temes
                        </span>
                      )}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
