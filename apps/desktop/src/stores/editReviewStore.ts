// ── REVISIÓ D'EDICIONS DE LA IA (estil Qoder / VS Code) ─────────────────────
// Quan la IA toca un fitxer del projecte NO el donem per bo tal com ve: en
// guardem el contingut VELL (el que hi havia abans de la seua escriptura) i el
// NOU, i l'editor els ensenya com un canvi pendent — amb el codi nou en verd i
// el vell en roig (el DiffEditor de Monaco en mode en línia fa exactament eixa
// pintura). L'usuari prem «Accepta» (deixa el codi nou i esborra el vell) o
// «Rebutja» (restaura el vell al disc i a l'editor).
//
// Semàntica de l'escriptura: la IA escriu el contingut nou AL DISC de seguida
// (així les comandes «RUN» del mateix torn, l'LSP i els compiladors ja veuen
// el codi nou); «pendent» vol dir «encara es pot desfer». Si no es fa res, al
// cap de 35 segons el canvi s'ACCEPTA SOL i el botó desapareix.

import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { useWorkspaceStore } from "./workspaceStore";

/// Temps de gràcia abans d'acceptar automàticament un canvi no revisat.
export const AUTO_ACCEPT_MS = 35_000;

export interface EditMark {
  path: string;
  name: string;
  /// Contingut ABANS de la intervenció de la IA.
  original: string;
  /// Cert si el fitxer ja existia (per saber si rebutjar = buidar o esborrar).
  existed: boolean;
  /// Contingut NOU generat per la IA (també l'últim escrit al disc).
  newContent: string;
  /// Mentre la IA encara l'està escrivint en directe: es mostra a l'editor com
  /// a previsualització i el compte enrere NO corre.
  streaming: boolean;
  /// Moment (ms) en què va començar el compte enrere d'acceptació automàtica.
  armedAt: number;
}

interface EditReviewState {
  marks: EditMark[];
  setMarks: (marks: EditMark[]) => void;
}

export const useEditReviewStore = create<EditReviewState>((set) => ({
  marks: [],
  setMarks: (marks) => set({ marks }),
}));

export function markForPath(path: string): EditMark | undefined {
  return useEditReviewStore.getState().marks.find((m) => m.path === path);
}

/// Marques pendents dins d'una carpeta (per a l'indicador de l'arbre).
export function marksForDir(dirAbs: string): EditMark[] {
  const prefix = dirAbs.replace(/\/+$/, "") + "/";
  return useEditReviewStore.getState().marks.filter((m) => m.path.startsWith(prefix));
}

/// Llegeix el contingut actual del disc; null = el fitxer encara no existix.
async function diskContent(abs: string): Promise<string | null> {
  try {
    return await invoke<string>("read_file", { path: abs });
  } catch {
    return null;
  }
}

/// Ajusta la vista de l'editor quan ja no hi ha res a revisar d'aquell fitxer:
/// si el que es veia era la previsualització en directe, el deixem estable
/// sobre el contingut definitiu.
function settleEditor(abs: string, content: string) {
  const ws = useWorkspaceStore.getState();
  const cur = ws.openFile;
  if (cur && cur.path === abs && !markForPath(abs)) {
    ws.setOpenFile({ path: abs, name: cur.name, content, dirty: false });
  }
}

interface MarkInput {
  abs: string;
  newContent: string;
  original: string;
  existed: boolean;
  streaming: boolean;
}

/// Registra un canvi de la IA sobre un fitxer (el contingut NOU ja ha sigut
/// escrit al disc pel cridaner) i ARMA el compte enrere: és el pas definitiu
/// del torn. Si la IA ja havia escrit este fitxer en directe, es conserva
/// l'«original» del principi del torn — el que volem comparar és el codi
/// anterior a TOT el torn, no l'instantani de l'últim fragment.
function markEdited(opts: MarkInput) {
  const st = useEditReviewStore.getState();
  const name = opts.abs.split(/[/\\]/).pop() ?? opts.abs;
  const existing = st.marks.find((m) => m.path === opts.abs);
  const wasPending = !!existing;

  const marks: EditMark[] = existing
    ? st.marks.map((m) =>
        m.path === opts.abs
          ? {
              ...m,
              original: opts.original,
              existed: opts.existed,
              newContent: opts.newContent || m.newContent,
              streaming: opts.streaming,
              armedAt: opts.streaming ? m.armedAt : Date.now(),
            }
          : m
      )
    : [
        ...st.marks,
        {
          path: opts.abs,
          name,
          original: opts.original,
          existed: opts.existed,
          newContent: opts.newContent,
          streaming: opts.streaming,
          armedAt: Date.now(),
        },
      ];
  st.setMarks(marks);

  // Fora del diff de revisió, l'editor mostra el text nou (en viu o definitiu).
  if (!wasPending && opts.newContent) settleEditor(opts.abs, opts.newContent);
}

/// Abans d'escriure un fitxer del torn, llegix el que HI HA AL DISC per poder
/// ensenyar-lo com a codi vell (roig) en la revisió. Si la IA ja havia tocat
/// este fitxer aquest mateix torn, es conserva l'original d'aquell moment.
export async function captureOriginal(abs: string): Promise<{ original: string; existed: boolean }> {
  const prev = markForPath(abs);
  if (prev) return { original: prev.original, existed: prev.existed };
  const onDisk = await diskContent(abs);
  return { original: onDisk ?? "", existed: onDisk !== null };
}

/// Canvi DEFINITIU d'un torn: la marca queda armada (compta els 35 s).
export function noteFileEdited(abs: string, newContent: string, original: string, existed: boolean) {
  markEdited({ abs, newContent, original, existed, streaming: false });
}

/// Escriptura «en directe» DURANT el streaming: marca sense armar (la
/// previsualització no compta a acceptar-se sola fins que la IA acaba).
export function noteLiveWriteStart(
  abs: string,
  newContent: string,
  original: string,
  existed: boolean
) {
  markEdited({ abs, newContent, original, existed, streaming: true });
}

/// Actualitza la previsualització d'un fitxer que la IA encara està escrivent.
export function noteLiveWriteContent(abs: string, newContent: string) {
  const st = useEditReviewStore.getState();
  if (!st.marks.some((m) => m.path === abs && m.streaming)) return;
  st.setMarks(
    st.marks.map((m) => (m.path === abs ? { ...m, newContent } : m))
  );
}

/// Final d'un torn de la IA: arma TOT el que es va escriure en directe (des
/// d'este moment comença a córrer el compte enrere d'acceptació automàtica).
export function armAllMarks() {
  const st = useEditReviewStore.getState();
  const now = Date.now();
  st.setMarks(
    st.marks.map((m) => (m.streaming ? { ...m, streaming: false, armedAt: now } : m))
  );
  // L'editor ja no està en mode previsualització: si no hi ha revisió oberta,
  // que es vegja el text nou.
  for (const m of [...st.marks]) settleEditor(m.path, m.newContent);
}

/// Accepta o rebutja un canvi. «reject» torna a escriure el vell al disc
/// (deixa el fitxer exactament com estava abans de la IA).
export async function resolveMark(path: string, decision: "accept" | "reject") {
  const st = useEditReviewStore.getState();
  const mark = st.marks.find((m) => m.path === path);
  if (!mark) return;
  st.setMarks(st.marks.filter((m) => m.path !== path));

  const ws = useWorkspaceStore.getState();
  if (decision === "reject") {
    try {
      if (!mark.existed) await invoke("delete_path", { path });
      else await invoke("write_file", { path, content: mark.original });
    } catch {
      /* si no es pot desfer, almenys traiem el diff de l'editor */
    }
    const cur = useWorkspaceStore.getState().openFile;
    if (cur && cur.path === path) {
      ws.setOpenFile({
        path,
        name: cur.name,
        content: mark.existed ? mark.original : "",
        dirty: false,
      });
    }
  } else {
    // Acceptat: el codi nou ja és al disc; l'editor l'ensenya directament.
    settleEditor(path, mark.newContent);
  }
  void ws.refreshTree();
}

/// Rebutja (o accepta) tots els canvis pendents d'una col·lecció de rutes.
export async function resolveAllMarks(decision: "accept" | "reject") {
  for (const m of [...useEditReviewStore.getState().marks]) {
    await resolveMark(m.path, decision);
  }
}

/// L'usuari esborra un fitxer de l'arbre: la revisió d'este perd sentit.
export function dropMark(path: string) {
  const st = useEditReviewStore.getState();
  if (st.marks.some((m) => m.path === path)) {
    st.setMarks(st.marks.filter((m) => m.path !== path));
  }
}

/// L'usuari desa o tanca a mà un fitxer mentre hi ha una revisió oberta: la
/// seua edició guanya i la revisió es tanca (acceptació tàcita).
export function dropMarkOnManualSave(path: string) {
  dropMark(path);
}

// ── Compte enrere global ────────────────────────────────────────────────────
// Un sol temporitzador mira cada segon les marques armades i accepta sola
// qualsevol que acumule més d'AUTO_ACCEPT_MS sense resposta de l'usuari.
if (typeof setInterval !== "undefined") {
  setInterval(() => {
    const now = Date.now();
    const expired = useEditReviewStore
      .getState()
      .marks.filter((m) => !m.streaming && now - m.armedAt >= AUTO_ACCEPT_MS);
    for (const m of expired) void resolveMark(m.path, "accept");
  }, 1000);
}
