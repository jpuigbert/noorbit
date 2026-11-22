/// Barra de ruta (breadcrumbs) del fitxer actiu, sobre l'editor (Fase 4):
/// carpeta > … > fitxer. Els segments de carpeta són clicables: obren la
/// branca corresponent a l'arbre del Sidebar (expandeixen els avantpassats
/// i refresquen l'arbre). Amb rutes llargues, els segments del mig es
/// pleguen darrere «…» (clic per desplegar-los) i el text porta el·lipsi.
import { useState } from "react";
import { ChevronRight, File as FileIcon, Folder } from "lucide-react";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useUIStore } from "../../stores/uiStore";

/// Obre una carpeta a l'arbre: marca tots els avantpassats com a expandits
/// (el refreshTree ja omple els fills de les carpetes obertes) i mostra el
/// Sidebar si estava amagat.
function revealDirInTree(dirPath: string) {
  const ws = useWorkspaceStore.getState();
  const root = ws.root;
  const expanded = { ...ws.expanded };
  if (root) {
    // Acumulem cada prefix de la ruta dins del projecte: root, root/a, …
    let acc = root;
    expanded[acc] = true;
    const rel = dirPath.startsWith(root + "/") ? dirPath.slice(root.length + 1) : "";
    for (const part of rel.split("/").filter(Boolean)) {
      acc += "/" + part;
      expanded[acc] = true;
    }
  } else {
    expanded[dirPath] = true;
  }
  useWorkspaceStore.setState({ expanded });
  if (!useUIStore.getState().sidebarOpen) useUIStore.getState().toggleSidebar();
  void ws.refreshTree();
}

export default function Breadcrumbs() {
  const root = useWorkspaceStore((s) => s.root);
  const activePath = useWorkspaceStore((s) => s.activePath);
  const splitPath = useWorkspaceStore((s) => s.splitPath);
  const focusedGroup = useWorkspaceStore((s) => s.focusedGroup);
  const [showAll, setShowAll] = useState(false);

  const path = focusedGroup === "split" && splitPath ? splitPath : activePath;
  if (!path) return null;

  // Fitxer sense títol: només el nom, sense ruta de carpetes.
  const untitled = path.startsWith("untitled:");
  const name = untitled ? path.slice("untitled:".length) : path.split(/[/\\]/).pop() ?? path;
  const rel =
    !untitled && root && path.startsWith(root + "/") ? path.slice(root.length + 1) : null;
  const parts = untitled ? [] : (rel ?? path).split(/[/\\]/).filter(Boolean);
  const dirs = parts.slice(0, -1);
  const rootName = root ? root.split(/[/\\]/).pop() ?? root : null;

  // Ruta absoluta del segment i-èssim de carpeta (per obrir-la a l'arbre).
  const dirPathAt = (i: number): string => {
    const base = rel != null && root ? root : "";
    return (base ? base + "/" : "") + dirs.slice(0, i + 1).join("/");
  };

  // Rutes fondes: root + «…» + les 2 darreres carpetes + fitxer. El botó
  // «…» desplega la ruta completa (i la torna a plegar en tornar-hi).
  const collapsed = !showAll && dirs.length > 3;
  const hidden = collapsed ? dirs.slice(0, dirs.length - 2) : [];
  const visible = collapsed ? dirs.slice(dirs.length - 2) : dirs;
  const visibleOffset = collapsed ? dirs.length - 2 : 0;

  return (
    <nav className="breadcrumbs" aria-label="Ruta del fitxer" title={path}>
      {rootName && rel != null && (
        <>
          <button className="crumb" onClick={() => revealDirInTree(root!)}>
            <Folder size={12} /> {rootName}
          </button>
          <ChevronRight size={11} className="crumb-sep" />
        </>
      )}
      {collapsed && (
        <>
          <button
            className="crumb crumb-more"
            title={hidden.join(" / ")}
            onClick={() => setShowAll(true)}
          >
            …
          </button>
          <ChevronRight size={11} className="crumb-sep" />
        </>
      )}
      {visible.map((d, i) => (
        <span className="crumb-item" key={d + i}>
          <button className="crumb" onClick={() => revealDirInTree(dirPathAt(visibleOffset + i))}>
            {d}
          </button>
          <ChevronRight size={11} className="crumb-sep" />
        </span>
      ))}
      <span className="crumb crumb-file">
        <FileIcon size={12} /> <span className="crumb-name">{name}</span>
      </span>
      {showAll && dirs.length > 3 && (
        <button className="crumb crumb-more" title="Plega la ruta" onClick={() => setShowAll(false)}>
          …
        </button>
      )}
    </nav>
  );
}