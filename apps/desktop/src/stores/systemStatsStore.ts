import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

/// Estat de la RAM llegit des del sistema operatiu (bytes).
interface MemInfo {
  total: number;
  used: number;
}

interface SystemStatsState {
  mem: MemInfo | null;
  error: string | null;
  /// Inicia el mostreig en temps real: llegeix la RAM cada `everyMs` ms.
  /// Idempotent — si ja està actiu, no duplica el temporitzador.
  start: (everyMs?: number) => void;
  stop: () => void;
}

let timer: ReturnType<typeof setInterval> | null = null;

async function readOnce(set: (p: Partial<SystemStatsState>) => void) {
  try {
    const mem = await invoke<MemInfo>("system_memory_status");
    set({ mem, error: null });
  } catch (e) {
    set({ error: String(e) });
  }
}

/// Format llegible de bytes (GB amb un decimal; MB si és petit).
export function formatBytes(n: number): string {
  const gb = n / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  return `${Math.round(n / 1024 ** 2)} MB`;
}

/// Percentatge de RAM usada (0–100), o null si encara no hi ha dades.
export function memPercent(mem: MemInfo | null): number | null {
  if (!mem || mem.total <= 0) return null;
  return Math.min(100, Math.round((mem.used / mem.total) * 100));
}

export const useSystemStatsStore = create<SystemStatsState>((set, get) => ({
  mem: null,
  error: null,

  start: (everyMs = 2000) => {
    if (timer) return; // ja actiu
    void readOnce(set);
    timer = setInterval(() => void readOnce(set), everyMs);
  },

  stop: () => {
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
    void get;
  },
}));
