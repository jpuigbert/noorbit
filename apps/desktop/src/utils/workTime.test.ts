/// Tests de les utilitats PURES de temps de treball (progrés, ETA, formatatge).
import { describe, expect, it } from "vitest";
import {
  customMinutes,
  etaByProgress,
  formatDuration,
  formatFinishTime,
  MAX_WORK_MINS,
  minsToMs,
  progressPct,
  remainingByBudget,
} from "./workTime";

describe("progressPct", () => {
  it("calcula el percentatge enter i el retalla a 0..100", () => {
    expect(progressPct(0, 0)).toBe(0);
    expect(progressPct(5, 10)).toBe(50);
    expect(progressPct(10, 10)).toBe(100);
    expect(progressPct(15, 10)).toBe(100);
    expect(progressPct(-3, 10)).toBe(0);
    expect(progressPct(1, 3)).toBe(33);
  });
});

describe("etaByProgress", () => {
  it("extrapola el ritme real (ms restants)", () => {
    // 2 de 10 en 60 s → 30 s/tasca → 8 restants → 240 s.
    expect(etaByProgress(0, 60_000, 2, 10)).toBe(240_000);
  });
  it("sense ritme (cap unitat feta) → null", () => {
    expect(etaByProgress(0, 60_000, 0, 10)).toBeNull();
    expect(etaByProgress(0, 0, 2, 10)).toBeNull();
  });
  it("tot completat → 0", () => {
    expect(etaByProgress(0, 60_000, 10, 10)).toBe(0);
  });
});

describe("remainingByBudget", () => {
  it("torna el temps que falta per omplir el pressupost", () => {
    expect(remainingByBudget(100_000, 30_000)).toBe(70_000);
    expect(remainingByBudget(100_000, 130_000)).toBe(0);
  });
});

describe("formatDuration", () => {
  it("format humà en català per a cada escala", () => {
    expect(formatDuration(0)).toBe("0 s");
    expect(formatDuration(-5)).toBe("0 s");
    expect(formatDuration(Number.NaN)).toBe("0 s");
    expect(formatDuration(45_000)).toBe("45 s");
    expect(formatDuration(60_000)).toBe("1 min");
    expect(formatDuration(90_000)).toBe("1 min 30 s");
    expect(formatDuration(3_600_000)).toBe("1 h");
    expect(formatDuration(3_660_000)).toBe("1 h 1 min");
    expect(formatDuration(86_400_000)).toBe("1 d");
    expect(formatDuration(90_000_000)).toBe("1 d 1 h");
  });
});

describe("formatFinishTime", () => {
  it("cadena buida per a valors invàlids", () => {
    expect(formatFinishTime(0)).toBe("");
    expect(formatFinishTime(Number.NaN)).toBe("");
  });
  it("dia de la setmana + data + hora (mateix any)", () => {
    const finish = new Date(2026, 10, 16, 14, 30).getTime(); // dl. 16/11/2026
    const now = new Date(2026, 10, 16, 12, 0).getTime();
    expect(formatFinishTime(finish, now)).toBe("dl. 16/11 · 14:30");
  });
  it("afegeix l'any si és diferent del de referència", () => {
    const finish = new Date(2027, 0, 5, 9, 5).getTime();
    const now = new Date(2026, 10, 16, 12, 0).getTime();
    expect(formatFinishTime(finish, now)).toContain("2027");
  });
});

describe("minsToMs", () => {
  it("converteix minuts a ms (i retalla negatius)", () => {
    expect(minsToMs(15)).toBe(900_000);
    expect(minsToMs(0)).toBe(0);
    expect(minsToMs(-4)).toBe(0);
  });
});

describe("customMinutes", () => {
  it("converteix valor + unitat a minuts", () => {
    expect(customMinutes(90, "min")).toBe(90);
    expect(customMinutes(2, "h")).toBe(120);
    expect(customMinutes(3, "d")).toBe(4320);
  });
  it("retalla al mínim (1 minut) i al màxim (7 dies)", () => {
    expect(customMinutes(0, "h")).toBe(1);
    expect(customMinutes(-5, "d")).toBe(1);
    expect(customMinutes(Number.NaN, "h")).toBe(1);
    expect(customMinutes(99, "d")).toBe(MAX_WORK_MINS);
    expect(MAX_WORK_MINS).toBe(10080);
  });
});