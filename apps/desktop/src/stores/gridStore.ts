/// ESTAT DE LA XARXA ALTRUISTA P2P (grid).
///
/// El backend (mòdul `grid` de Rust) és qui té la veritat: aquest store
/// només en guarda l'ÚLTIM `grid_status` conegut (el refresca cada cop que
/// la UI ho demana) i el darrer esdeveniment `grid://event` (perquè els
/// components puguin pintar «la comunitat ha respost el torn» sense
/// preguntar al backend cada segon).

import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

/// Peer ajudat (projecte d'un altre al qual donem inferència).
export interface PeerInfo {
  name: string;
  addr: string;
  /// Connectat ara mateix?
  connectat: boolean;
}

/// Voluntari connectat al NOSTRE projecte (nosaltres som el propietari).
export interface VoluntariInfo {
  nodeId: string;
  nom: string;
  /// Treballant ara?
  ocupat: boolean;
  /// Torns donats en aquesta sessió.
  torns: number;
}

/// Unitat que aquest node té entre mans (vista del voluntari, per a l'estalvi).
export interface WorkView {
  projecte: string;
  label: string;
  detail: string;
}

/// Una còpia rebuda (o en curs) com a voluntari.
export interface CopyState {
  projecte: string;
  desti: string;
  fetes: number;
  total: number;
  fase: string;
  error: string | null;
}

/// Distribució en curs del projecte acabat (costat propietari).
export interface DistInfo {
  activa: boolean;
  projecte: string;
  fitxers: number;
  peces: number;
  totalBytes: number;
  baixades: number;
}

/// Estat complet de la xarxa tal com el torna `grid_status`.
export interface GridStatus {
  enabled: boolean;
  port: number;
  compartint: boolean;
  shareCode: string;
  invitacio: string;
  peers: PeerInfo[];
  voluntaris: VoluntariInfo[];
  tornsDonats: number;
  tornsRebuts: number;
  treball: WorkView | null;
  copies: CopyState[];
  distribucio: DistInfo | null;
}

/// Payload de l'esdeveniment global «grid://event».
export interface GridEventPayload {
  kind: string;
  detail: string;
  projecte: string;
  ok: boolean;
}

interface GridStore {
  status: GridStatus | null;
  lastEvent: GridEventPayload | null;
  load: () => Promise<void>;
  noteEvent: (ev: GridEventPayload) => void;
}

export const useGridStore = create<GridStore>((set) => ({
  status: null,
  lastEvent: null,
  load: async () => {
    try {
      set({ status: await invoke<GridStatus>("grid_status") });
    } catch {
      // El backend no hi és (o el grid no s'ha inicialitzat): es manté
      // l'estat conegut i la UI ja pinta el que té.
    }
  },
  noteEvent: (ev) => set({ lastEvent: ev }),
}));
