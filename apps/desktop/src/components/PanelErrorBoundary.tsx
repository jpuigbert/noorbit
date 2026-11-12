import { Component, type ErrorInfo, type ReactNode } from "react";
import { t } from "../i18n";

/**
 * Xarxa de seguretat del RENDERITZAT d'un panell: si un component peta
 * (un `null` d'una dada que no ha arribat, per exemple), només mor AQUEST
 * panell — la resta de l'aplicació (xat, editor, terminal) continua viva.
 *
 * Abans l'única frontera era global: un sol error en el panell d'Unreal
 * deixava tota la finestra en blanc i NoOrbit quedava inutilitzable.
 */
export default class PanelErrorBoundary extends Component<
  { children: ReactNode; label?: string },
  { error: Error | null; stack: string }
> {
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
    const label = this.props.label ?? t("app.crashTitle");
    return (
      <div
        style={{
          padding: 16,
          display: "flex",
          flexDirection: "column",
          gap: 10,
          color: "var(--text-0)",
          fontFamily: "var(--font-ui)",
        }}
      >
        <strong style={{ fontSize: 13 }}>{label}</strong>
        <p style={{ color: "var(--text-1)", fontSize: 12, lineHeight: 1.5 }}>
          {t("app.crashBody")}
        </p>
        <pre
          style={{
            maxHeight: 160,
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
          {error.message}
          {"\n\n"}
          {stack}
        </pre>
        <button
          className="btn primary sm"
          onClick={() => window.location.reload()}
          style={{ alignSelf: "flex-start" }}
        >
          {t("app.crashReload")}
        </button>
      </div>
    );
  }
}
