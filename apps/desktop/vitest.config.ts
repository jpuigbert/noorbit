import { defineConfig } from "vitest/config";

// Configuració Vitest per als tests de LÒGICA PURA (i18n, parsers de
// directrius, utilitats del xat). jsdom dona localStorage/document perquè
// l'store d'i18n i les preferències del xat funcionen com al navegador.
export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
    // Els mòduls purs no necessiten el plugin de React ni l'alias de Tauri.
    globals: false,
  },
});