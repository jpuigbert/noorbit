/// Virtualització LLEUGERA de l'arbre de fitxers (Sidebar): sense
/// dependències. Dues funcions pures: aplanar l'arbre visible (només les
/// carpetes expandides) i calcular la finestra de files a renderitzar
/// segons l'scrollTop (windowing clàssic amb overscan).
import type { FileNode } from "../stores/workspaceStore";

export interface TreeRow {
  node: FileNode;
  depth: number;
}

/// Files visibles en ordre DFS: una carpeta tancada amaga tot el subarbre.
export function flattenTree(tree: FileNode[], expanded: Record<string, boolean>): TreeRow[] {
  const out: TreeRow[] = [];
  const walk = (nodes: FileNode[], depth: number) => {
    for (const n of nodes) {
      out.push({ node: n, depth });
      if (n.is_dir && expanded[n.path] && n.children) {
        walk(n.children, depth + 1);
      }
    }
  };
  walk(tree, 0);
  return out;
}

export interface WindowSpec {
  /// Primera fila a renderitzar (índex a la llista aplanada).
  start: number;
  /// Una més enllà de la darrera fila a renderitzar (exclusiu).
  end: number;
  /// Desplaçament superior del bloc renderitzat (px).
  padTop: number;
  /// Alçada total del contingut (px) per mantenir la barra de scroll real.
  totalHeight: number;
}

/// Finestra de files per a un contenidor amb scroll. `overscan` files extra
/// per damunt/baix perquè un scroll ràpid no mostri forats blancs.
export function computeWindow(
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
  total: number,
  overscan = 6
): WindowSpec {
  const totalHeight = Math.max(0, total) * rowHeight;
  if (total <= 0 || rowHeight <= 0) {
    return { start: 0, end: 0, padTop: 0, totalHeight: 0 };
  }
  const top = Math.max(0, Math.min(scrollTop, Math.max(0, totalHeight - 1)));
  const firstVisible = Math.floor(top / rowHeight);
  const visibleCount = Math.max(1, Math.ceil(Math.max(0, viewportHeight) / rowHeight));
  const start = Math.max(0, firstVisible - overscan);
  const end = Math.min(total, firstVisible + visibleCount + overscan);
  return { start, end, padTop: start * rowHeight, totalHeight };
}