import React, { Component, type ErrorInfo, type ReactNode } from "react";
import ReactDOM from "react-dom/client";
// NOTA v0.70: Monaco (3,9 MB + workers) JA NO es carrega a l'arrencada.
// Es importa ÀJUS quan s'obre el primer fitxer de codi: fa l'app molt més
// lleugera i ràpida d'iniciar (consulta src/editor/monReady.ts).
import App from "./App";
import { t } from "./i18n";
import "./theme.css";

/// Xarxa de seguretat del renderitzat: sense ella, qualsevol error dins d'un
/// component desmuntava tot l'arbre i la finestra quedava en blanc (un pantalla
/// buida no diu res de què ha fallat). Ara es mostra un avís llegible amb el
/// motiu i una opció per tornar-ho a provar.
class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null; stack: string }> {
  state: { error: Error | null; stack: string } = { error: null, stack: "" };

  static getDerivedStateFromError(error: Error) {
    return { error, stack: error.stack ?? "" };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Error de renderitzat:", error, info.componentStack);
    this.setState({ stack: (error.stack ?? "") + (info.componentStack ?? "") });
  }

  render() {
    const { error, stack } = this.state;
    if (!error) return this.props.children;

    return (
      <div
        style={{
          height: "100%",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          gap: 12,
          padding: 24,
          background: "var(--bg-0)",
          color: "var(--text-0)",
          fontFamily: "var(--font-ui)",
          textAlign: "center",
        }}
      >
        <h1 style={{ fontSize: 18, fontWeight: 600 }}>{t("app.crashTitle")}</h1>
        <p style={{ maxWidth: 520, color: "var(--text-1)", fontSize: 13, lineHeight: 1.5 }}>
          {t("app.crashBody")}
        </p>
        <pre
          style={{
            maxWidth: 680,
            width: "100%",
            maxHeight: 180,
            overflow: "auto",
            textAlign: "left",
            fontSize: 11,
            fontFamily: "var(--font-mono)",
            color: "var(--text-2)",
            background: "var(--bg-2)",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            padding: 10,
            whiteSpace: "pre-wrap",
          }}
        >
          {t("app.crashDetail")}: {error.message}
          {"\n\n"}
          {stack}
        </pre>
        <button
          className="btn primary"
          onClick={() => window.location.reload()}
          style={{ fontSize: 13, padding: "6px 14px" }}
        >
          {t("app.crashReload")}
        </button>
      </div>
    );
  }
}

// ── GUARDA DE NAVEGADOR ────────────────────────────────────────────────
// `pnpm dev` aixeca Vite a localhost:1420, però NoOrbit no és una web: les
// crides a Tauri (finestra, fitxers, IA local, diàlegs) necessiten el pont
// natiu. Si s'obri eixa URL en un navegador normal, abans petava amb
// «Cannot read properties of undefined (reading 'metadata')» i eixia la
// pantalla d'error genèrica. Ara expliquem què passa i com obrir l'app de
// veritat, sense arrencar res.
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

if (!isTauri) {
  const root = document.getElementById("root");
  if (root) {
    root.innerHTML = `
      <div style="height:100%;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:14px;padding:32px;background:#0d0d12;color:#e8e8f0;font-family:-apple-system,'Segoe UI',sans-serif;text-align:center">
        <h1 style="font-size:19px;margin:0">NoOrbit s'ha d'obrir amb l'aplicació d'escriptori</h1>
        <p style="max-width:560px;font-size:13px;line-height:1.6;color:#b4b4c4;margin:0">
          Esta és la vista de desenvolupament (Vite). El navegador no té accés al
          pont natiu de Tauri, així que no pot llegir el projecte, els fitxers ni
          la IA local. No és cap error del codi.
        </p>
        <p style="max-width:560px;font-size:13px;line-height:1.6;color:#b4b4c4;margin:0">
          Engega l'aplicació amb <code style="background:#1b1b24;padding:2px 6px;border-radius:5px;color:#e8e8f0">pnpm tauri dev</code>
          (o obri la NoOrbit ja instal·lada). La finestra de Tauri carregarà
          esta mateixa adreça amb totes les capacitats.
        </p>
      </div>`;
  }
} else {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <ErrorBoundary>
        <App />
      </ErrorBoundary>
    </React.StrictMode>
  );
}
