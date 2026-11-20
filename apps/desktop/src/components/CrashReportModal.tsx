import { useEffect, useState } from "react";
import { Check, Copy, ExternalLink, FolderOpen, RefreshCw, Save, TriangleAlert, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../i18n";
import { useUIStore } from "../stores/uiStore";
import { toastError, toastSuccess } from "../stores/toastStore";
import { copyText } from "../utils/clipboard";
import { saveTextToFile } from "./CopyButton";
import {
  buildCrashReport,
  fetchCrashReport,
  CRASH_REPORT_DESTINATION,
  type CrashReportDto,
} from "../crashReport";
import "./CrashReportModal.css";

/// DIÀLEG DE L'INFORME DE FALLIDA. Mostra què ha anat malament (la
/// darrera caiguda del programa, els errors de la interfície) i deixa
/// copiar-ho tot sencer per enviar-lo a l'autor. És el lloc on l'usuari
/// arriba quan el programa peta: en comptes de buscar fitxers pel sistema,
/// fa dos clics i ja està.
export default function CrashReportModal() {
  const show = useUIStore((s) => s.showCrashReport);
  const setShow = useUIStore((s) => s.setShowCrashReport);
  const { t } = useT();
  const [report, setReport] = useState<CrashReportDto | null>(null);
  const [text, setText] = useState("");
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);

  // Cada cop que s'obre, l'informe es torna a llegir: si l'usuari ha fet
  // servir NoOrbit un rato, el registre pot haver canviat.
  useEffect(() => {
    if (!show) return;
    let alive = true;
    setLoading(true);
    void (async () => {
      const [r, full] = await Promise.all([fetchCrashReport(), buildCrashReport()]);
      if (!alive) return;
      setReport(r);
      setText(full);
      setLoading(false);
    })();
    return () => {
      alive = false;
    };
  }, [show]);

  if (!show) return null;

  const doCopy = async () => {
    const ok = await copyText(text || (await buildCrashReport()));
    if (ok) {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
      toastSuccess(t("crash.copied"));
    } else {
      toastError(t("crash.copyFail"));
    }
  };

  const doSave = async () => {
    const path = await saveTextToFile(text || (await buildCrashReport()), "informe-fallida-noorbit.md");
    if (path) toastSuccess(t("crash.saved"));
  };

  const doReveal = () => {
    if (!report?.logPath) return;
    void invoke("reveal_in_finder", { path: report.logPath }).catch(() => undefined);
  };

  // Envia: copia l'informe al porta-retalls I obre la pàgina d'incidents del
  // projecte. L'usuari només ha de prémer ⌘V al formulari.
  const doSend = async () => {
    const ok = await copyText(text || (await buildCrashReport()));
    if (ok) toastSuccess(t("crash.copiedForIssue"));
    await invoke("open_externally", { path: `${CRASH_REPORT_DESTINATION}/new` }).catch(
      () => undefined
    );
  };

  const lastCrash =
    report?.lastCrashAt != null
      ? `${report.lastCrashAt}${
          report.lastCrashAgeMinutes != null ? t("crash.minutesAgo", { n: report.lastCrashAgeMinutes }) : ""
        }`
      : t("crash.none");

  return (
    <div className="modal-overlay" onClick={() => setShow(false)}>
      <div className="modal crash-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>
            <TriangleAlert size={15} style={{ marginRight: 6, verticalAlign: -2 }} />
            {t("crash.title")}
          </span>
          <button className="icon-btn" onClick={() => setShow(false)} title={t("common.close")}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body crash-body">
          <p className="crash-intro">{t("crash.intro")}</p>
          <div className="crash-chips">
            <span className="crash-chip">
              {t("crash.sessionPanics")}: <b>{report?.sessionPanics ?? 0}</b>
            </span>
            <span className="crash-chip">
              {t("crash.previousPanics")}: <b>{report?.previousPanics ?? 0}</b>
            </span>
            <span className="crash-chip">
              {t("crash.version")}: <b>{report?.appVersion ?? "—"}</b>
            </span>
            <span className="crash-chip">
              {t("crash.system")}: <b>{report ? `${report.os} (${report.arch})` : "—"}</b>
            </span>
          </div>
          <p className="crash-last">
            {t("crash.lastCrash")}: <b>{lastCrash}</b>
          </p>
          <pre className="crash-text">{loading ? t("crash.loading") : text || t("crash.empty")}</pre>
          <div className="crash-actions">
            <button className="btn primary sm" onClick={() => void doCopy()}>
              {copied ? <Check size={12} /> : <Copy size={12} />} {copied ? t("crash.copied") : t("crash.copy")}
            </button>
            <button className="btn sm" onClick={() => void doSave()}>
              <Save size={12} /> {t("crash.save")}
            </button>
            <button
              className="btn sm"
              onClick={doReveal}
              disabled={!report?.logPath}
              title={t("crash.revealHint")}
            >
              <FolderOpen size={12} /> {t("crash.reveal")}
            </button>
            <button
              className="btn sm ghost"
              onClick={() => {
                setLoading(true);
                void (async () => {
                  const [r, full] = await Promise.all([fetchCrashReport(), buildCrashReport()]);
                  setReport(r);
                  setText(full);
                  setLoading(false);
                })();
              }}
            >
              <RefreshCw size={12} /> {t("crash.reload")}
            </button>
            <button className="btn sm ghost" onClick={() => void doSend()} title={t("crash.openIssuesHint")}>
              <ExternalLink size={12} /> {t("crash.openIssues")}
            </button>
          </div>
          <p className="crash-foot">
            {t("crash.sendTo")}{" "}
            <button
              className="crash-link"
              onClick={() => void invoke("open_externally", { path: CRASH_REPORT_DESTINATION }).catch(() => undefined)}
            >
              {CRASH_REPORT_DESTINATION}
            </button>
          </p>
        </div>
      </div>
    </div>
  );
}
