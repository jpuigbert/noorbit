/// Magatzem d'ACTUALITZACIONS. Guarda el resultat de preguntar a GitHub si
/// existeix una versió nova de NoOrbit (allà on publiquen els releases amb un
/// tag «v*»). La comprovació és manual o de l'arrancada: mai no baixa res ni
/// instal·la res sol — només INFORMA i obri la pàgina del release amb el botó.
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface UpdateInfo {
  /// Versió d'aquest NoOrbit en execució.
  current: string;
  /// Última versió publicada a GitHub.
  latest: string;
  /// Cert només si «latest» és més nova que «current».
  available: boolean;
  /// Pàgina del release a github.com.
  url: string;
  name: string;
  date: string;
  /// Descarrega directa de l'instal·ador per al nostre SO, si n'hi ha.
  asset: string | null;
}

interface UpdateState {
  info: UpdateInfo | null;
  checking: boolean;
  error: string | null;
  /// Instants de l'última comprovació (per a no repetir-la a cada mount).
  checkedAt: number | null;
  check: (force?: boolean) => Promise<void>;
}

let inFlight = false;

export const useUpdateStore = create<UpdateState>((set, get) => ({
  info: null,
  checking: false,
  error: null,
  checkedAt: null,
  check: async (force) => {
    if (inFlight) return;
    // Sense «force», no tornem a preguntar si ja ho vam fer fa poc.
    const last = get().checkedAt;
    if (!force && last && Date.now() - last < 60_000) return;
    inFlight = true;
    set({ checking: true, error: null });
    try {
      const info = await invoke<UpdateInfo>("check_update");
      set({ info, checking: false, checkedAt: Date.now() });
    } catch (e) {
      set({ checking: false, error: String(e), checkedAt: Date.now() });
    } finally {
      inFlight = false;
    }
  },
}));
