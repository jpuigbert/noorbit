import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Configuració Vite per NoOrbit (Tauri)
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
    chunkSizeWarningLimit: 800,
    // UN SOL paquet de 3,8 MB feia l'arrancada lenta i omplia de pressió de
    // memòria el WKWebView del Mac. Dividir en capes (Monaco, React, icones,
    // sintaxis de llenguatges) permet càrrega paral·lela del navegador i un
    // primer pintat molt més ràpid — millor encara en Macs Intel antics.
    rollupOptions: {
      output: {
        manualChunks(id: string) {
          if (id.includes("node_modules")) {
            if (id.includes("monaco-editor") || id.includes("@monaco-editor")) return "vendor-monaco";
            if (/react-dom|react\/|scheduler/.test(id)) return "vendor-react";
            if (id.includes("zustand") || id.includes("@tauri-apps")) return "vendor-core";
            if (id.includes("lucide-react")) return "vendor-icons";
            if (id.includes("prismjs") || id.includes("shiki") || id.includes("highlight.js")) return "vendor-syntax";
            if (id.includes("d3")) return "vendor-d3";
            if (id.includes("apexcharts")) return "vendor-charts";
            return "vendor";
          }
          return undefined;
        },
      },
    },
  },
});
