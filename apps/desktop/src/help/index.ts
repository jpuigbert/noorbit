//! Manual d'ús de NoOrbit (només text), preparat per traduir.
//!
//! Cada idioma és un fitxer a ./manuals/<codi>.ts que exporta un objecte `Manual`.
//! El Loader tria el manual de l'idioma actual i, si no existeix, cau al català
//! (idioma base). Per traduir el manual, copia `ca.ts` amb el nou codi d'idioma i
//! traduit els textos — sense tocar els components.

import { useI18n, BASE_LOCALE } from "../i18n/store";

export interface ManualSection {
  title: string;
  intro?: string;
  items: string[];
}

export interface Manual {
  language: string;
  title: string;
  intro: string;
  sections: ManualSection[];
}

const modules = import.meta.glob("./manuals/*.ts", {
  eager: true,
}) as Record<string, { manual: Manual }>;

const byCode: Record<string, Manual> = {};
for (const [path, mod] of Object.entries(modules)) {
  const code = path.replace("./manuals/", "").replace(/\.ts$/, "");
  if (mod && mod.manual) byCode[code] = mod.manual;
}

export function availableManuals(): string[] {
  return Object.keys(byCode);
}

/// Retorna el manual per a l'idioma indicat (o l'actual), ambfallback al català.
export function getManual(code?: string): Manual {
  const locale = code ?? useI18n.getState().locale;
  return byCode[locale] ?? byCode[BASE_LOCALE] ?? Object.values(byCode)[0]!;
}
