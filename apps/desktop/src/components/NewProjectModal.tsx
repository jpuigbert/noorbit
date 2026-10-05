import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { X, FolderPlus, Folder } from "lucide-react";
import { useT } from "../i18n";
import { useUIStore } from "../stores/uiStore";
import { useWorkspaceStore } from "../stores/workspaceStore";

/// Crea un projecte nou: carpeta amb un README dins d'una carpeta pare
/// (per defecte ~/Projects) i l'obre com a carpeta de treball.
export default function NewProjectModal() {
  const show = useUIStore((s) => s.showNewProject);
  const setShow = useUIStore((s) => s.setShowNewProject);
  const { t } = useT();
  const [name, setName] = useState("");
  const [parent, setParent] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  if (!show) return null;

  const close = () => {
    setShow(false);
    setName("");
    setParent("");
    setError(null);
  };

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
        <div className="modal-body">
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
        </div>
      </div>
    </div>
  );
}
