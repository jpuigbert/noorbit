/// SNAPSHOTS PER PAS D'AGENT (Fase 1d) — estat mínim del «Desfés el pas».
///
/// El backend captura un snapshot git shadow ABANS de cada torn amb
/// escriptures (automàtic, no hi ha cap `snapshot_take` exposat). Ací només
/// es consulta la mida de la pila (`snapshot_count`) i es demana el desfer
/// (`snapshot_undo`), que restaura el work-tree i torna les rutes relatives
/// afectades. Després cal refrescar l'arbre i el fitxer obert perquè l'editor
/// no ensenyi el contingut vell (ja restaurat al disc).
///
/// La revisió verd/roig NO es trenca: les marques pendents dels fitxers
/// restaurats es descarten (`dropMark`) — el disc ja torna a ser l'original
/// i una acceptació/rebuig posterior no ha de sobreescriure el desfer.
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n";
import { useWorkspaceStore } from "./workspaceStore";
import { dropMark } from "./editReviewStore";
import { toastError, toastInfo, toastSuccess } from "./toastStore";

interface SnapshotState {
  /// Mida de la pila de snapshots del projecte actiu (0 = sense passos).
  count: number;
  /// Un desfer en curs: evita dos clics seguits que pops dos passos.
  busy: boolean;
  /// Refresca el comptador (degradació honesta: backend antic → 0).
  refresh: (projectPath: string | null) => Promise<void>;
  /// Desfés el darrer pas i refresca editor/arbre. Retorna les rutes
  /// relatives restaurades, o null si ha fallat (toast ja mostrat).
  undoStep: (projectPath: string) => Promise<string[] | null>;
}

export const useSnapshotStore = create<SnapshotState>((set, get) => ({
  count: 0,
  busy: false,

  refresh: async (projectPath) => {
    if (!projectPath) {
      set({ count: 0 });
      return;
    }
    try {
      const n = await invoke<number>("snapshot_count", { projectPath });
      set({ count: typeof n === "number" && n > 0 ? n : 0 });
    } catch {
      // Backend sense el comandament (o projecte tancat): sense snapshots.
      set({ count: 0 });
    }
  },

  undoStep: async (projectPath) => {
    if (get().busy || !projectPath) return null;
    set({ busy: true });
    try {
      const restored = await invoke<string[]>("snapshot_undo", { projectPath });
      const paths = Array.isArray(restored) ? restored : [];
      const base = projectPath.replace(/\/+$/, "");
      const ws = useWorkspaceStore.getState();
      // Arbre + pestanyes afectades pel pas. `force` recarrega del disc i
      // `background` no canvia la pestanya activa. Les edicions sense desar
      // de fitxers ALIENS al pas són sagrades: només es recarreguen les
      // pestanyes que el pas ha tocat o que estan netes.
      await ws.refreshTree();
      const affected = new Set(paths.map((rel) => `${base}/${rel}`));
      const tabs = useWorkspaceStore.getState().openFiles;
      for (const f of tabs) {
        if (f.path.startsWith("untitled:") || !f.path.startsWith(base + "/")) continue;
        const touched = affected.has(f.path);
        if (!touched && f.dirty) continue;
        const ok = await ws.loadFile(f.path, { force: true, background: true });
        if (!ok && touched) {
          // L'undo ha esborrat el fitxer (l'agent l'havia creat): sense això
          // quedava una pestanya fantasma que ⌘S ressuscitava al disc.
          ws.closeFile(f.path);
          toastInfo(t("agent.undoStepClosed", { name: f.name }));
        }
      }
      // Les revisions pendents d'aquests fitxers ja no tenen sentit: el
      // contingut restaurat és l'original que la revisió guardava.
      for (const rel of paths) dropMark(`${base}/${rel}`);
      toastSuccess(
        paths.length === 1
          ? t("agent.undoStepDoneOne")
          : t("agent.undoStepDone", { n: paths.length })
      );
      await get().refresh(projectPath);
      return paths;
    } catch (e) {
      toastError(t("agent.undoStepError"), e);
      await get().refresh(projectPath);
      return null;
    } finally {
      set({ busy: false });
    }
  },
}));