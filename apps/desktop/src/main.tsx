import React, { Component, type ErrorInfo, type ReactNode } from "react";
import ReactDOM from "react-dom/client";
// Ha d'importar-se primer: configura Monaco local (offline) + els seus workers
// abans que qualsevol component <Editor /> es munti.
import "./editor/monacoSetup";
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

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>
);
