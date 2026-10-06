import { useState, type KeyboardEvent } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  ChevronRight,
  File as FileIcon,
  Folder,
  FolderOpen,
  RefreshCw,
  FilePlus,
  FolderPlus,
  Trash2,
  Pencil,
  X,
} from "lucide-react";
import { useT } from "../i18n";
import { useWorkspaceStore, type FileNode } from "../stores/workspaceStore";

/// Diàleg de nom per a les operacions de l'arbre.
type Dialog =
  | { mode: "newFile"; dir: string }
  | { mode: "newFolder"; dir: string }
  | { mode: "rename"; path: string; current: string }
  | null;

/// Uneix un directori i un nom en un camí absolut net.
function joinPath(dir: string, name: string): string {
  return `${dir.replace(/\/+$/, "")}/${name.trim()}`;
}

function TreeNode({
  node,
  depth,
  onDialog,
}: {
  node: FileNode;
  depth: number;
  onDialog: (d: Dialog) => void;
}) {
  const { t } = useT();
  const { expanded, toggleDir, loadFile, openFile, removeNode } = useWorkspaceStore();
  const isOpen = !!expanded[node.path];
  const active = openFile?.path === node.path;

  const doDelete = async () => {
    const ok = await ask(
      `${t("sidebar.deleteConfirm1")} «${node.name}» ${t("sidebar.deleteConfirm2")}`,
      { title: t("common.delete"), kind: "warning" }
    );
    if (ok) {
      try {
        await removeNode(node.path);
      } catch (e) {
        await ask(String(e), { title: t("common.error"), kind: "error" });
      }
    }
  };

  return (
    <>
      <div
        className={"tree-node" + (active ? " active" : "")}
        style={{ paddingLeft: 8 + depth * 12 }}
        draggable
        onDragStart={(e) => {
          // Arrossega este fitxer/carpeta fins al xat per adjuntar-lo.
          e.dataTransfer.setData("application/noorbit-path", node.path);
          e.dataTransfer.effectAllowed = "copy";
        }}
        onClick={() => (node.is_dir ? toggleDir(node.path) : loadFile(node.path))}
        onContextMenu={(e) => {
          e.preventDefault();
          onDialog({ mode: "rename", path: node.path, current: node.name });
        }}
      >
        {node.is_dir ? (
          <>
            <ChevronRight
              size={12}
              style={{ transform: isOpen ? "rotate(90deg)" : "none", transition: "0.1s" }}
            />
            {isOpen ? <FolderOpen size={13} /> : <Folder size={13} />}
          </>
        ) : (
          <>
            <span style={{ width: 12 }} />
            <FileIcon size={13} />
          </>
        )}
        <span className="tree-name">{node.name}</span>
        {/* Accions visibles en passar-hi el ratolí. */}
        <span className="tree-actions" onClick={(e) => e.stopPropagation()}>
          {node.is_dir && (
            <>
              <button
                className="icon-btn tiny"
                title={t("sidebar.newFile")}
                onClick={() => onDialog({ mode: "newFile", dir: node.path })}
              >
                <FilePlus size={12} />
              </button>
              <button
                className="icon-btn tiny"
                title={t("sidebar.newFolder")}
                onClick={() => onDialog({ mode: "newFolder", dir: node.path })}
              >
                <FolderPlus size={12} />
              </button>
            </>
          )}
          <button
            className="icon-btn tiny"
            title={t("sidebar.rename")}
            onClick={() => onDialog({ mode: "rename", path: node.path, current: node.name })}
          >
            <Pencil size={12} />
          </button>
          <button className="icon-btn tiny danger" title={t("common.delete")} onClick={() => void doDelete()}>
            <Trash2 size={12} />
          </button>
        </span>
      </div>
      {node.is_dir &&
        isOpen &&
        node.children?.map((c) => (
          <TreeNode key={c.path} node={c} depth={depth + 1} onDialog={onDialog} />
        ))}
    </>
  );
}

export default function Sidebar() {
  const { t } = useT();
  const { tree, root, pickWorkspace, refreshTree, createFile, createFolder, renameNode, loadFile } =
    useWorkspaceStore();
  const [dialog, setDialog] = useState<Dialog>(null);
  const [value, setValue] = useState("");

  const openDialog = (d: Dialog) => {
    setValue(d && d.mode === "rename" ? d.current : "");
    setDialog(d);
  };

  const submit = async () => {
    if (!dialog) return;
    const name = value.trim();
    if (!name) return;
    try {
      if (dialog.mode === "newFile") {
        const path = joinPath(dialog.dir, name);
        await createFile(path);
        await loadFile(path); // obre'l per editar-lo de seguida
      } else if (dialog.mode === "newFolder") {
        await createFolder(joinPath(dialog.dir, name));
      } else {
        await renameNode(dialog.path, name);
      }
      setDialog(null);
    } catch (e) {
      await ask(String(e), { title: t("common.error"), kind: "error" });
    }
  };

  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") void submit();
    if (e.key === "Escape") setDialog(null);
  };

  const dialogTitle =
    dialog?.mode === "newFile"
      ? t("sidebar.newFile")
      : dialog?.mode === "newFolder"
        ? t("sidebar.newFolder")
        : t("sidebar.rename");

  return (
    <div className="sidebar">
      <div className="panel-header">
        <span>{t("sidebar.title")}</span>
        {root && (
          <>
            <button
              className="icon-btn"
              title={t("sidebar.newFile")}
              onClick={() => openDialog({ mode: "newFile", dir: root })}
            >
              <FilePlus size={13} />
            </button>
            <button
              className="icon-btn"
              title={t("sidebar.newFolder")}
              onClick={() => openDialog({ mode: "newFolder", dir: root })}
            >
              <FolderPlus size={13} />
            </button>
            <button className="icon-btn" title={t("common.refresh")} onClick={() => refreshTree()}>
              <RefreshCw size={13} />
            </button>
          </>
        )}
      </div>
      <div className="panel-body">
        {!root ? (
          <div className="empty-hint">
            {t("sidebar.empty")}
            <div style={{ marginTop: 10 }}>
              <button className="btn sm" onClick={() => pickWorkspace()}>
                {t("sidebar.openFolder")}
              </button>
            </div>
          </div>
        ) : (
          tree.map((n) => <TreeNode key={n.path} node={n} depth={0} onDialog={openDialog} />)
        )}
      </div>

      {/* Diàleg de nom (crear/reanomenar). */}
      {dialog && (
        <div className="modal-overlay" onClick={() => setDialog(null)}>
          <div className="modal small" onClick={(e) => e.stopPropagation()}>
            <div className="modal-head">
              <span>{dialogTitle}</span>
              <button className="icon-btn" onClick={() => setDialog(null)}>
                <X size={16} />
              </button>
            </div>
            <div className="modal-body">
              <input
                className="tree-input"
                autoFocus
                value={value}
                placeholder={
                  dialog.mode === "newFolder" ? t("sidebar.nameFolderPh") : t("sidebar.nameFilePh")
                }
                onChange={(e) => setValue(e.target.value)}
                onKeyDown={onKey}
              />
              <div className="tree-dialog-actions">
                <button className="btn sm" onClick={() => setDialog(null)}>
                  {t("common.cancel")}
                </button>
                <button className="btn sm primary" disabled={!value.trim()} onClick={() => void submit()}>
                  {t("common.ok")}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
