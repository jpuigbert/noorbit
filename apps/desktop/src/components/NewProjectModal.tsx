import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { X, FolderPlus, Folder, Palette } from "lucide-react";
import { useT } from "../i18n";
import { useUIStore } from "../stores/uiStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import { useEscapeToClose } from "../utils/useEscapeToClose";

/// Crea un projecte nou: carpeta amb un README dins d'una carpeta pare
/// (per defecte ~/Projects) i l'obre com a carpeta de treball.
///
/// Té dos apartats:
///  · **Projecte**: una carpeta buida, com sempre.
///  · **Tema de WordPress**: obre l'assistent que demana a la IA els
///    fitxers inicials d'un tema (explica què vols, tries la IA i es crea).
export default function NewProjectModal() {
  const show = useUIStore((s) => s.showNewProject);
  const setShow = useUIStore((s) => s.setShowNewProject);
  const setShowWp = useUIStore((s) => s.setShowWpTheme);
  const { t } = useT();
  const [tab, setTab] = useState<"project" | "wp" | "gamestudios">("project");
  const [name, setName] = useState("");
  const [parent, setParent] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const close = () => {
    setShow(false);
    setName("");
    setParent("");
    setError(null);
    setTab("project");
  };

  // Esc tanca el modal mentre és obert (com la X i el clic a l'overlay).
  useEscapeToClose(show, close);

  if (!show) return null;

  const browse = async () => {
    const selected = await open({ directory: true });
    if (typeof selected === "string") setParent(selected);
  };

  const create = async () => {
    if (!name.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      const path = await invoke<string>("create_project", {
        name: name.trim(),
        parent: parent.trim() ? parent : null,
      });
      await useWorkspaceStore.getState().openWorkspace(path);
      close();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const createGameStudios = async () => {
    if (!name.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      const path = await invoke<string>("clone_game_studios", {
        name: name.trim(),
        parent: parent.trim() ? parent.trim() : null,
      });
      await useWorkspaceStore.getState().openWorkspace(path);
      close();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={close}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>
            <FolderPlus size={15} style={{ marginRight: 6, verticalAlign: -2 }} />
            {t("newProject.title")}
          </span>
          <button className="icon-btn" onClick={close}>
            <X size={16} />
          </button>
        </div>
        {/* APARTATS */}
        <div className="pm-tabs">
          <button
            className={"pm-tab" + (tab === "project" ? " active" : "")}
            onClick={() => setTab("project")}
          >
            <FolderPlus size={14} /> {t("newProject.tabProject")}
          </button>
          <button
            className={"pm-tab" + (tab === "wp" ? " active" : "")}
            onClick={() => setTab("wp")}
          >
            <Palette size={14} /> {t("newProject.tabWpTheme")}
          </button>
          <button
            className={"pm-tab" + (tab === "gamestudios" ? " active" : "")}
            onClick={() => setTab("gamestudios")}
          >
            <FolderPlus size={14} /> {t("newProject.tabGameStudios")}
          </button>
        </div>
        <div className="modal-body">
          {tab === "project" && (
            <>
              <div className="field">
                <label>{t("newProject.name")}</label>
                <input
                  autoFocus
                  placeholder={t("newProject.namePh")}
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && create()}
                />
              </div>
              <div className="field">
                <label>{t("newProject.parent")}</label>
                <div style={{ display: "flex", gap: 8 }}>
                  <input
                    style={{ flex: 1 }}
                    placeholder={t("newProject.parentPh")}
                    value={parent}
                    onChange={(e) => setParent(e.target.value)}
                  />
                  <button className="btn sm" onClick={() => void browse()}>
                    <Folder size={12} /> {t("common.browse")}
                  </button>
                </div>
              </div>
              {error && <div className="si-error">{error}</div>}
              <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 16 }}>
                <button className="btn" onClick={close}>
                  {t("common.cancel")}
                </button>
                <button
                  className="btn primary"
                  disabled={!name.trim() || busy}
                  onClick={() => void create()}
                >
                  <FolderPlus size={13} /> {t("newProject.create")}
                </button>
              </div>
            </>
          )}

          {tab === "wp" && (
            <>
              <p className="lp-hint">{t("newProject.wpHint")}</p>
              <ul className="lp-list">
                <li>{t("newProject.wpStep1")}</li>
                <li>{t("newProject.wpStep2")}</li>
                <li>{t("newProject.wpStep3")}</li>
              </ul>
              <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 16 }}>
                <button className="btn" onClick={close}>
                  {t("common.cancel")}
                </button>
                <button
                  className="btn primary"
                  onClick={() => {
                    close();
                    setShowWp(true);
                  }}
                >
                  <Palette size={13} /> {t("newProject.openWpTheme")}
                </button>
              </div>
            </>
          )}

          {tab === "gamestudios" && (
            <>
              <p className="lp-hint">{t("newProject.gsHint")}</p>
              <ul className="lp-list">
                <li>{t("newProject.gsStep1")}</li>
                <li>{t("newProject.gsStep2")}</li>
                <li>{t("newProject.gsStep3")}</li>
              </ul>
              <div className="field">
                <label>{t("newProject.name")}</label>
                <input
                  placeholder={t("newProject.namePh")}
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && createGameStudios()}
                />
              </div>
              <div className="field">
                <label>{t("newProject.parent")}</label>
                <div style={{ display: "flex", gap: 8 }}>
                  <input
                    style={{ flex: 1 }}
                    placeholder={t("newProject.parentPh")}
                    value={parent}
                    onChange={(e) => setParent(e.target.value)}
                  />
                  <button className="btn sm" onClick={() => void browse()}>
                    <Folder size={12} /> {t("common.browse")}
                  </button>
                </div>
              </div>
              {error && <div className="si-error">{error}</div>}
              <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 16 }}>
                <button className="btn" onClick={close}>
                  {t("common.cancel")}
                </button>
                <button
                  className="btn primary"
                  disabled={!name.trim() || busy}
                  onClick={() => void createGameStudios()}
                >
                  <FolderPlus size={13} /> {t("newProject.createGs")}
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
