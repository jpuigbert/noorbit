/// Tests del sistema i18n: lookup de claus, fallback al català, interpolació
/// i gestió de catàlegs personalitzats. jsdom dona localStorage/document.
import { afterEach, describe, expect, it } from "vitest";
import { BASE_LOCALE, t, useI18n } from "./store";

afterEach(() => {
  // Deixa l'store com estava: idioma base i sense catàlegs de prova.
  const s = useI18n.getState();
  for (const code of Object.keys(s.catalogs)) {
    if (code !== BASE_LOCALE && code !== "ca") s.removeLocale(code);
  }
  s.setLocale(BASE_LOCALE);
});

describe("t() — catàleg base", () => {
  it("resol claus anidades del català", () => {
    expect(t("app.name")).toBe("NoOrbit");
    expect(t("app.tagline")).toBe("Editor creative amb IA local");
  });

  it("una clau inexistent torna la mateixa clau (mai undefined)", () => {
    expect(t("no.existeix.cap.clau")).toBe("no.existeix.cap.clau");
  });

  it("una clau que apunta a un objecte (no string) torna la clau", () => {
    expect(t("app")).toBe("app");
  });
});

describe("t() — catàlegs personalitzats i fallback", () => {
  it("interpola variables {nom} i deixa les desconegudes intactes", () => {
    useI18n.getState().addLocale("prova", { saluda: "Hola {nom} i {altre}" });
    useI18n.getState().setLocale("prova");
    expect(t("saluda", { nom: "Joan" })).toBe("Hola Joan i {altre}");
    expect(t("saluda")).toBe("Hola {nom} i {altre}");
  });

  it("si l'idioma actiu no té la clau, cau al català", () => {
    useI18n.getState().addLocale("prova", { nomes: "una cosa" });
    useI18n.getState().setLocale("prova");
    expect(t("app.name")).toBe("NoOrbit"); // ve del catàleg base
    expect(t("nomes")).toBe("una cosa");
  });

  it("setLocale ignora codis sense catàleg i removeLocale no esborra el base", () => {
    const s = useI18n.getState();
    s.setLocale("idioma-fantasma");
    expect(useI18n.getState().locale).toBe(BASE_LOCALE);
    s.removeLocale(BASE_LOCALE);
    expect(useI18n.getState().catalogs[BASE_LOCALE]).toBeDefined();
  });

  it("addLocale normalitza el codi i exportCatalog torna JSON", () => {
    useI18n.getState().addLocale("  PROVA  ", { a: "b" }, "Idioma de prova");
    const s = useI18n.getState();
    expect(s.catalogs["prova"]).toEqual({ a: "b" });
    expect(s.names["prova"]).toBe("Idioma de prova");
    expect(JSON.parse(s.exportCatalog("prova"))).toEqual({ a: "b" });
  });
});