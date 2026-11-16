/// Utilitats PURES de temps de treball per a les feines llargues (Jam de jocs
/// multi-dia i millora contínua de temes WordPress). Sense efectes secundaris:
/// reben «ara» com a paràmetre per poder-se provar de manera determinista.
///
/// Dos modes de progrés:
///  - «tasks» (Jam): el progrés és tasques fetes / total i l'ETA es dedueix del
///    ritme real (temps transcorregut per tasca). Té un SOSTRE: la durada màxima
///    que l'usuari ha triat (el backend s'atura sol en arribar-hi).
///  - «time» (WordPress): la feina OMPLI el temps triat buscant millores i bugs;
///    el progrés és temps transcorregut / pressupost i l'ETA és el temps que falta.

/// Percentatge enter 0..100 a partir de dues magnituds (tolera total 0).
export function progressPct(done: number, total: number): number {
  if (!Number.isFinite(total) || total <= 0) return 0;
  const d = Number.isFinite(done) ? done : 0;
  return Math.max(0, Math.min(100, Math.round((d / total) * 100)));
}

/// ETA (ms RESTANTS) segons el ritme de progrés per unitats. Torna `null` si
/// encara no s'ha completat cap unitat (no hi ha ritme per extrapolar).
export function etaByProgress(
  startedAtMs: number,
  nowMs: number,
  done: number,
  total: number,
): number | null {
  if (done <= 0 || total <= 0) return null;
  const elapsed = Math.max(0, nowMs - startedAtMs);
  if (elapsed <= 0) return null;
  const remainingUnits = Math.max(0, total - done);
  if (remainingUnits <= 0) return 0;
  return Math.round((elapsed / done) * remainingUnits);
}

/// Temps restant (ms) d'un pressupost fix: el que falta per omplir el temps.
export function remainingByBudget(budgetMs: number, elapsedMs: number): number {
  return Math.max(0, Math.round(budgetMs - elapsedMs));
}

/// Durada humana en català: «2 d 3 h», «4 h 12 min», «12 min 30 s», «45 s».
/// Els valors negatius o no finits es tracten com 0.
export function formatDuration(ms: number): string {
  const total = Number.isFinite(ms) && ms > 0 ? Math.floor(ms / 1000) : 0;
  if (total <= 0) return "0 s";
  const d = Math.floor(total / 86400);
  const h = Math.floor((total % 86400) / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (d > 0) return h > 0 ? `${d} d ${h} h` : `${d} d`;
  if (h > 0) return m > 0 ? `${h} h ${m} min` : `${h} h`;
  if (m > 0) return s > 0 ? `${m} min ${s} s` : `${m} min`;
  return `${s} s`;
}

const WEEKDAYS = ["dg.", "dl.", "dt.", "dc.", "dj.", "dv.", "ds."];

/// Hora de fin estimada en català: «dl. 16/11 · 14:30». Si l'any és diferent
/// del de referència, l'afegeix. `epochMs` no finit → cadena buida.
export function formatFinishTime(epochMs: number, nowMs: number = Date.now()): string {
  if (!Number.isFinite(epochMs) || epochMs <= 0) return "";
  const dt = new Date(epochMs);
  const now = new Date(nowMs);
  const pad = (n: number) => String(n).padStart(2, "0");
  const wd = WEEKDAYS[dt.getDay()];
  const date = `${pad(dt.getDate())}/${pad(dt.getMonth() + 1)}`;
  const year = dt.getFullYear() !== now.getFullYear() ? `/${dt.getFullYear()}` : "";
  const time = `${pad(dt.getHours())}:${pad(dt.getMinutes())}`;
  return `${wd} ${date}${year} · ${time}`;
}

/// Minuts → mil·lisegons (els selectors de la UI treballen en minuts).
export function minsToMs(mins: number): number {
  return Math.max(0, Math.round(mins * 60_000));
}

/// Unitats del selector de temps PERSONALITZAT (jocs i WordPress).
export type TimeUnit = "min" | "h" | "d";
export const UNIT_FACTOR: Record<TimeUnit, number> = { min: 1, h: 60, d: 1440 };

/// Sostre de temps personalitzat: 7 dies (igual que el backend).
export const MAX_WORK_MINS = 7 * 24 * 60;

/// Valor + unitat → minuts, retallat a [1, MAX_WORK_MINS]. Un valor buit o
/// invàlid es tracta com el mínim (1 minut) perquè sempre hi haja un temps
/// vàlid que enviar al backend (que també retalla a 60 s..7 dies).
export function customMinutes(value: number, unit: TimeUnit): number {
  const v = Number.isFinite(value) && value > 0 ? value : 0;
  return Math.max(1, Math.min(MAX_WORK_MINS, Math.round(v * UNIT_FACTOR[unit])));
}