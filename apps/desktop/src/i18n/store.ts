//! Sistema d'internacionalització (i18n) lleuger per a NoOrbit.
//!
//! Filosofia: cada idioma és un fitxer JSON de diccionari amb les MATEIXES claus
//! que el catàleg base (`ca`). Per afegir un idioma nou n'hi ha prou amb pujar un
//! `.json` traduït — sense tocar codi ni recompilar. El text visible de la UI mai
//! no ha d'estar "endurit": sempre ha de passar per `t("seccio.clau")`.

import { create } from "zustand";

export const BASE_LOCALE = "ca";

export type Catalog = Record<string, unknown>;

export interface LanguageMeta {
  code: string;
  name: string;
  /** base = ve amb l'app; custom = importat per l'usuari */
  source: "base" | "custom";
}

const LS_LOCALES = "noorbit.i18n.locales";
const LS_CURRENT = "noorbit.i18n.current";

/// Diccionaris que venen empaquetats amb l'aplicació (carpeta ./locales).
/// `import.meta.glob` recull tots els `.json` automàticament.
const builtinModules = import.meta.glob("./locales/*.json", {
  eager: true,
}) as Record<string, { default: Catalog }>;

const builtin: Record<string, Catalog> = {};
const builtinNames: Record<string, string> = {
  ca: "Català",
};

for (const [path, mod] of Object.entries(builtinModules)) {
  const code = path.replace("./locales/", "").replace(/\.json$/, "");
  builtin[code] = (mod && (mod.default ?? (mod as unknown as Catalog))) || {};
}

/// Noms legibles per a codis d'idioma coneguts (es poden estendre en importar).
const KNOWN_NAMES: Record<string, string> = {
  ca: "Català",
  es: "Castellà",
  en: "Anglès",
  fr: "Francès",
  de: "Alemany",
  it: " Italià",
  pt: "Portuguès",
  gl: "Gallec",
  eu: "Basc",
};

function readCustom(): Record<string, Catalog> {
  try {
    const raw = localStorage.getItem(LS_LOCALES);
    return raw ? (JSON.parse(raw) as Record<string, Catalog>) : {};
  } catch {
    return {};
  }
}

function persistCustom(map: Record<string, Catalog>) {
  try {
    localStorage.setItem(LS_LOCALES, JSON.stringify(map));
  } catch {
    /* quota o mode privat: ignorem */
  }
}

/// Resol una clau anidada ("a.b.c") dins d'un diccionari.
function lookup(dict: Catalog | undefined, key: string): string | undefined {
  if (!dict) return undefined;
  const parts = key.split(".");
  let cur: unknown = dict;
  for (const p of parts) {
    if (cur && typeof cur === "object" && p in (cur as Catalog)) {
      cur = (cur as Catalog)[p];
    } else {
      return undefined;
    }
  }
  return typeof cur === "string" ? cur : undefined;
}

/// Interpola variables del tipus {nom}.
function interpolate(text: string, vars?: Record<string, string | number>): string {
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (_, k) =>
    k in vars ? String(vars[k]) : `{${k}}`
  );
}

interface I18nState {
  locale: string;
  catalogs: Record<string, Catalog>;
  names: Record<string, string>;

  t: (key: string, vars?: Record<string, string | number>) => string;
  setLocale: (code: string) => void;
  languages: () => LanguageMeta[];
  addLocale: (code: string, catalog: Catalog, name?: string) => void;
  removeLocale: (code: string) => void;
  /// Retorna el contingut JSON (pretty) d'un catàleg per exportar/traduir.
  exportCatalog: (code?: string) => string;
}

export const useI18n = create<I18nState>((set, get) => {
  const custom = readCustom();
  const initialLocale =
    localStorage.getItem(LS_CURRENT) &&
    (builtin[localStorage.getItem(LS_CURRENT)!] || custom[localStorage.getItem(LS_CURRENT)!])
      ? localStorage.getItem(LS_CURRENT)!
      : BASE_LOCALE;

  return {
    locale: initialLocale,
    catalogs: { ...builtin, ...custom },
    names: { ...builtinNames, ...KNOWN_NAMES },

    t: (key, vars) => {
      const { locale, catalogs } = get();
      const text =
        lookup(catalogs[locale], key) ??
        lookup(catalogs[BASE_LOCALE], key) ??
        key;
      return interpolate(text, vars);
    },

    setLocale: (code) => {
      if (!get().catalogs[code]) return;
      try {
        localStorage.setItem(LS_CURRENT, code);
      } catch {
        /* ignore */
      }
      document.documentElement.lang = code;
      set({ locale: code });
    },

    languages: () => {
      const { catalogs } = get();
      const { names } = get();
      return Object.keys(catalogs).map((code) => ({
        code,
        name: names[code] ?? code,
        source: (builtin[code] ? "base" : "custom") as LanguageMeta["source"],
      }));
    },

    addLocale: (code, catalog, name) => {
      const clean = code.trim().toLowerCase();
      if (!clean) return;
      const mergedCatalogs = { ...get().catalogs, [clean]: catalog };
      const customNext = { ...readCustom(), [clean]: catalog };
      persistCustom(customNext);
      const names = { ...get().names };
      if (name) names[clean] = name;
      else if (!names[clean]) names[clean] = KNOWN_NAMES[clean] ?? clean;
      set({ catalogs: mergedCatalogs, names });
    },

    removeLocale: (code) => {
      if (code === BASE_LOCALE) return;
      const customNext = { ...readCustom() };
      delete customNext[code];
      persistCustom(customNext);
      const catalogs = { ...get().catalogs };
      delete catalogs[code];
      set({ catalogs });
      if (get().locale === code) get().setLocale(BASE_LOCALE);
    },

    exportCatalog: (code) => {
      const target = get().catalogs[code ?? BASE_LOCALE] ?? get().catalogs[BASE_LOCALE];
      return JSON.stringify(target, null, 2);
    },
  };
});

/// Funció `t` per a usos fora de components React (per exemple, altres stores).
/// Llegeix l'estat corrent de l'i18n de forma no reactiu.
export const t = (key: string, vars?: Record<string, string | number>) =>
  useI18n.getState().t(key, vars);

/// Hook pràctic: retorna { t, locale }.
export const useT = () => {
  const locale = useI18n((s) => s.locale);
  const translate = useI18n((s) => s.t);
  return { t: translate, locale };
};
