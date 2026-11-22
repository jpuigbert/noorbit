import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useShallow } from "zustand/react/shallow";
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
  FolderTree,
  ArrowRightToLine,
} from "lucide-react";
import { useT } from "../i18n";
import { useWorkspaceStore, type FileNode } from "../stores/workspaceStore";
import { computeWindow, flattenTree } from "../utils/treeWindow";

/// Alçada FIXA de cada fila de l'arbre (px): el windowing la necessita per
/// calcular quines files caben a la finestra visible (styles/shell.css).
const ROW_H = 24;
/// Refresc automàtic lleuger: cada 5 s mentre la finestra és visible, i
/// també quan la finestra recupera el focus (fitxers creats des de fora).
const REFRESH_MS = 5000;

/// Diàleg de nom per a les operacions de l'arbre.
type Dialog =
  | { mode: "newFile"; dir: string }
  | { mode: "newFolder"; dir: string }
  | { mode: "rename"; path: string; current: string }
  | null;

/// Menú contextual d'un node (clic dret): fitxer → obre / obre al costat /
/// reanomena / esborra; carpeta → fitxer nou / carpeta nova / …
type CtxMenu = { x: number; y: number; node: FileNode } | null;

/// Uneix un directori i un nom en un camí absolut net.
function joinPath(dir: string, name: string): string {
  return `${dir.replace(/\/+$/, "")}/${name.trim()}`;
}

/// Una fila de l'arbre APLANAT (sense recursió): amb windowing només es
/// munten les files visibles, així un projecte amb 10.000 fitxers no penalitza.
function TreeRow({
  node,
  depth,
  onDialog,
  onMenu,
}: {
  node: FileNode;
  depth: number;
  onDialog: (d: Dialog) => void;
  onMenu: (m: CtxMenu) => void;
}) {
  const { t } = useT();
  // Selectors per node (primitius estables): cada fila només es re-renderitza
  // quan canvia el SEU estat d'obertura o el fitxer actiu.
  const isOpen = useWorkspaceStore((st) => !!st.expanded[node.path]);
  const active = useWorkspaceStore((st) => st.activePath === node.path);
  const toggleDir = useWorkspaceStore((st) => st.toggleDir);
  const loadFile = useWorkspaceStore((st) => st.loadFile);
  const removeNode = useWorkspaceStore((st) => st.removeNode);

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
    <div
      className={"tree-node" + (active ? " active" : "")}
      style={{ paddingLeft: 8 + depth * 12, height: ROW_H }}
      draggable
      onDragStart={(e) => {
        // Arrossega este fitxer/carpeta fins al xat per adjuntar-lo.
        e.dataTransfer.setData("application/noorbit-path", node.path);
        e.dataTransfer.effectAllowed = "copy";
      }}
      onClick={() => (node.is_dir ? toggleDir(node.path) : loadFile(node.path))}
      onContextMenu={(e) => {
        e.preventDefault();
        onMenu({ x: e.clientX, y: e.clientY, node });
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
  );
}

export default function Sidebar() {
  const { t } = useT();
  const { tree, root, expanded, pickWorkspace, refreshTree, createFile, createFolder, renameNode, loadFile } =
    useWorkspaceStore(
      useShallow((st) => ({
        tree: st.tree,
        root: st.root,
        expanded: st.expanded,
        pickWorkspace: st.pickWorkspace,
        refreshTree: st.refreshTree,
        createFile: st.createFile,
        createFolder: st.createFolder,
        renameNode: st.renameNode,
        loadFile: st.loadFile,
      }))
    );
  const [dialog, setDialog] = useState<Dialog>(null);
  const [value, setValue] = useState("");
  const [menu, setMenu] = useState<CtxMenu>(null);
  // Windowing: scrollTop del panell + alçada visible (ResizeObserver).
  const bodyRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewH, setViewH] = useState(600);

  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return undefined;
    const ro = new ResizeObserver(() => setViewH(el.clientHeight));
    ro.observe(el);
    setViewH(el.clientHeight);
    return () => ro.disconnect();
  }, [root]);

  // Refresc automàtic LLEUGER: polling cada 5 s només mentre la pestanya és
  // visible + en recuperar el focus de la finestra. `refreshTree` compara
  // l'arbre nou amb l'antic: si no ha canviat res, no hi ha re-render.
  useEffect(() => {
    if (!root) return undefined;
    const tick = () => {
      if (document.visibilityState === "visible") {
        void useWorkspaceStore.getState().refreshTree();
      }
    };
    const id = window.setInterval(tick, REFRESH_MS);
    const onFocus = () => tick();
    window.addEventListener("focus", onFocus);
    return () => {
      window.clearInterval(id);
      window.removeEventListener("focus", onFocus);
    };
  }, [root]);

  const rows = flattenTree(tree, expanded);
  const win = computeWindow(scrollTop, viewH, ROW_H, rows.length);

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

  const doDeleteNode = async (node: FileNode) => {
    const ok = await ask(
      `${t("sidebar.deleteConfirm1")} «${node.name}» ${t("sidebar.deleteConfirm2")}`,
      { title: t("common.delete"), kind: "warning" }
    );
    if (!ok) return;
    try {
      await useWorkspaceStore.getState().removeNode(node.path);
    } catch (e) {
      await ask(String(e), { title: t("common.error"), kind: "error" });
    }
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
        <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
          <FolderTree size={13} /> {t("sidebar.title")}
        </span>
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
      <div
        className="panel-body"
        ref={bodyRef}
        onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
      >
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
          /* Contenidor amb l'alçada TOTAL (barra de scroll real) i només les
             files de la finestra visible, desplaçades al seu lloc. */
          <div style={{ height: win.totalHeight, position: "relative" }}>
            <div style={{ transform: `translateY(${win.padTop}px)` }}>
              {rows.slice(win.start, win.end).map((r) => (
                <TreeRow
                  key={r.node.path}
                  node={r.node}
                  depth={r.depth}
                  onDialog={openDialog}
                  onMenu={setMenu}
                />
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Menú contextual del node (mateix llenguatge visual que el de les
          pestanyes de l'editor: .ctx-* compartits a shell.css). */}
      {menu && (
        <div
          className="ctx-overlay"
          onClick={() => setMenu(null)}
          onContextMenu={(e) => {
            e.preventDefault();
            setMenu(null);
          }}
        >
          <div
            className="ctx-menu"
            style={{
              left: Math.min(menu.x, window.innerWidth - 240),
              top: Math.min(menu.y, window.innerHeight - 220),
            }}
            onClick={(e) => e.stopPropagation()}
          >
            {menu.node.is_dir ? (
              <>
                <button
                  className="ctx-row"
                  onClick={() => {
                    openDialog({ mode: "newFile", dir: menu.node.path });
                    setMenu(null);
                  }}
                >
                  <FilePlus size={13} /> {t("sidebar.newFile")}
                </button>
                <button
                  className="ctx-row"
                  onClick={() => {
                    openDialog({ mode: "newFolder", dir: menu.node.path });
                    setMenu(null);
                  }}
                >
                  <FolderPlus size={13} /> {t("sidebar.newFolder")}
                </button>
              </>
            ) : (
              <>
                <button
                  className="ctx-row"
                  onClick={() => {
                    void loadFile(menu.node.path);
                    setMenu(null);
                  }}
                >
                  <FileIcon size={13} /> {t("sidebar.open")}
                </button>
                <button
                  className="ctx-row"
                  onClick={() => {
                    void useWorkspaceStore.getState().openSide(menu.node.path);
                    setMenu(null);
                  }}
                >
                  <ArrowRightToLine size={13} /> {t("sidebar.openSide")}
                </button>
              </>
            )}
            <div className="ctx-sep" />
            <button
              className="ctx-row"
              onClick={() => {
                openDialog({ mode: "rename", path: menu.node.path, current: menu.node.name });
                setMenu(null);
              }}
            >
              <Pencil size={13} /> {t("sidebar.rename")}
            </button>
            <button
              className="ctx-row danger"
              onClick={() => {
                const n = menu.node;
                setMenu(null);
                void doDeleteNode(n);
              }}
            >
              <Trash2 size={13} /> {t("common.delete")}
            </button>
          </div>
        </div>
      )}

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