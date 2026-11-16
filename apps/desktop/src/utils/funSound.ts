/// Sons de «tasca acabada» de NoOrbit: sintetitzats amb WebAudio (cap
/// fitxer d'àudio extern, com la resta de recursos de l'app). Un toc
/// alegre quan una tasca acaba, un de greu si falla i un «pop» suau
/// quan la checklist marca una tasca com a feta. El commutador ràpid
/// és a la barra d'estat; la preferència es desa a l'uiStore.
import { useUIStore } from "../stores/uiStore";

let ctx: AudioContext | null = null;

/// Crea (o desperta) el context d'àudio. Els navegadors/webviews només
/// deixen sonar després d'un gest de l'usuari: initFunFx el crida al
/// primer clic o tecla, i a partir d'ací tot sona sense retard.
export function unlockFunSound() {
  try {
    if (!ctx) {
      const AC =
        window.AudioContext ??
        (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
      if (!AC) return;
      ctx = new AC();
    }
    if (ctx.state === "suspended") void ctx.resume().catch(() => undefined);
  } catch {
    /* sense àudio: l'app continua igual */
  }
}

function ready(force: boolean): AudioContext | null {
  if (!force && !useUIStore.getState().funSound) return null;
  unlockFunSound();
  // Sense gest previ el context queda suspès: no intentem sonar.
  return ctx && ctx.state === "running" ? ctx : null;
}

/// Una nota amb envelope suau (atac ràpid, caiguda exponencial).
function tone(
  c: AudioContext,
  freq: number,
  at: number,
  dur: number,
  type: OscillatorType,
  peak: number
) {
  const osc = c.createOscillator();
  const gain = c.createGain();
  osc.type = type;
  osc.frequency.setValueAtTime(freq, at);
  gain.gain.setValueAtTime(0.0001, at);
  gain.gain.exponentialRampToValueAtTime(peak, at + 0.015);
  gain.gain.exponentialRampToValueAtTime(0.0001, at + dur);
  osc.connect(gain).connect(c.destination);
  osc.start(at);
  osc.stop(at + dur + 0.05);
}

/// Tasca ACABADA: arpegi major ascendent (do-mi-sol-do) + espurna final.
export function playFunDone(force = false) {
  const c = ready(force);
  if (!c) return;
  const t0 = c.currentTime + 0.02;
  [523.25, 659.25, 783.99, 1046.5].forEach((f, i) =>
    tone(c, f, t0 + i * 0.09, 0.3, "triangle", 0.13)
  );
  tone(c, 1567.98, t0 + 0.38, 0.45, "sine", 0.05);
}

/// Tasca FALLIDA: dues notes descendents, suaus i sense estridència.
export function playFunError(force = false) {
  const c = ready(force);
  if (!c) return;
  const t0 = c.currentTime + 0.02;
  tone(c, 392, t0, 0.3, "sawtooth", 0.05);
  tone(c, 311.13, t0 + 0.16, 0.4, "sawtooth", 0.05);
}

/// Checklist «@done:»: un pop curt i alegre (una sola nota que puja).
export function playFunPop(force = false) {
  const c = ready(force);
  if (!c) return;
  const t0 = c.currentTime + 0.01;
  const osc = c.createOscillator();
  const gain = c.createGain();
  osc.type = "sine";
  osc.frequency.setValueAtTime(880, t0);
  osc.frequency.exponentialRampToValueAtTime(1318.5, t0 + 0.07);
  gain.gain.setValueAtTime(0.0001, t0);
  gain.gain.exponentialRampToValueAtTime(0.09, t0 + 0.012);
  gain.gain.exponentialRampToValueAtTime(0.0001, t0 + 0.12);
  osc.connect(gain).connect(c.destination);
  osc.start(t0);
  osc.stop(t0 + 0.15);
}