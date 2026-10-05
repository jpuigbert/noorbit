import { FolderOpen, FolderPlus, X, History, ArrowRight } from "lucide-react";
import { useT } from "../i18n";
import { useUIStore } from "../stores/uiStore";
import { useWorkspaceStore } from "../stores/workspaceStore";

/// Pantalla de benvinguda: apareix quan no hi ha cap projecte obert i també
/// amb la drecera ⌘⌥R per canviar entre projectes recents.
export default function StartupScreen() {
  const show = useUIStore((s) => s.showStartup);
  const setShow = useUIStore((s) => s.setShowStartup);
  const setShowNewProject = useUIStore((s) => s.setShowNewProject);
  const { t } = useT();

  const recents = useWorkspaceStore((s) => s.recents);
  const hasRoot = useWorkspaceStore((s) => s.root !== null);
  const pickWorkspace = useWorkspaceStore((s) => s.pickWorkspace);
  const openWorkspace = useWorkspaceStore((s) => s.openWorkspace);
  const removeRecent = useWorkspaceStore((s) => s.removeRecent);

  if (!show) return null;

  const close = () => setShow(false);

  const open = async (path: string) => {
    try {
      await openWorkspace(path);
      close();
    } catch {
      /* el projecte ja no existeix: l'usuari el pot esborrar de la llista */
    }
  };

  const nameOf = (p: string) => p.replace(/[/\\]+$/, "").split(/[/\\]/).pop() || p;

  return (
    <div className="modal-overlay" onClick={hasRoot ? close : undefined}>
      <div className="modal startup-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>
            <History size={15} style={{ marginRight: 6, verticalAlign: -2 }} />
            {hasRoot ? t("startup.switchTitle") : t("startup.title")}
          </span>
          {hasRoot && (
            <button className="icon-btn" onClick={close}>
              <X size={16} />
            </button>
          )}
        </div>

        <div className="modal-body">
          <div className="startup-actions">
            <button
              className="btn primary"
              onClick={() => {
                setShowNewProject(true);
                close();
              }}
            >
              <FolderPlus size={13} /> {t("startup.newProject")}
            </button>
            <button className="btn" onClick={() => void pickWorkspace().then(close)}>
              <FolderOpen size={13} /> {t("startup.openFolder")}
            </button>
          </div>

          <div className="startup-recents-head">{t("startup.recentTitle")}</div>
          {recents.length === 0 ? (
            <div className="empty-hint">{t("startup.recentEmpty")}</div>
          ) : (
            <ul className="startup-list">
              {recents.map((p) => (
                <li key={p} className="startup-item">
                  <button className="startup-open" onClick={() => void open(p)}>
                    <span className="startup-name">{nameOf(p)}</span>
                    <span className="startup-path">{p}</span>
                    <ArrowRight size={13} />
                  </button>
                  <button
                    className="icon-btn"
                    title={t("startup.forget")}
                    onClick={() => removeRecent(p)}
                  >
                    <X size={13} />
                  </button>
                </li>
              ))}
            </ul>
          )}

          {!hasRoot && (
            <div style={{ marginTop: 12, textAlign: "right" }}>
              <button className="btn sm ghost" onClick={close}>
                {t("startup.skip")}
              </button>
            </div>
          )}

          <div className="startup-footer">
            <div>{t("about.by")}</div>
            <div className="startup-footer-sub">{t("about.license")}</div>
          </div>
        </div>
      </div>
    </div>
  );
}
