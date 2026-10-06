import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Download, Play, ExternalLink, RefreshCw } from "lucide-react";
import { useT } from "../../i18n";

/// Secció APART per als «harnessos» d'agents (menú propi dins el Gestor de
/// proveïdors, NO a la llista de proveïdors): IAs que no són una API HTTP
/// compatible sinó agents complets que giren al teu ordinador. NoOrbit els
/// descobreix (discovery), els delega tasques en mode headless quan la IA
/// principal no les resol, i els obre la interfície al navegador intern.
/// DeepSeek Harness (dsh, github.com/deepseek-ai/deepseek-harness) és el
/// primer: botó d'instal·lació AUTOMÀTICA que baixa també el Node.js LTS si
/// falta, tot dins de les dades de NoOrbit (mode portàtil inclòs) i sense
/// permisos d'administrador.
export default function HarnessSection() {
  const { t } = useT();
  const [installed, setInstalled] = useState(false);
  const [running, setRunning] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const [inst, run] = await Promise.all([
        invoke<boolean>("dsh_installed"),
        invoke<boolean>("dsh_web_running"),
      ]);
      setInstalled(inst);
      setRunning(run);
    } catch {
      /* backend no preparat: deixem l'últim estat */
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  /// Progrés de la instal·lació: el backend emet «agent://progress» amb la
  /// etiqueta «DeepSeek Harness» a cada pas (baixada de Node, npm install…).
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    const p = listen<{ label?: string; detail?: string }>(
      "agent://progress",
      (e) => {
        if (e.payload?.label === "DeepSeek Harness") {
          setProgress(e.payload.detail ?? null);
        }
      }
    )
      .then((u) => {
        unlisten = u;
      })
      .catch(() => {});
    return () => {
      void p.then(() => unlisten?.());
    };
  }, []);

  const install = async () => {
    setBusy(true);
    setErr(null);
    setMsg(null);
    try {
      setMsg(await invoke<string>("install_dsh"));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
      setProgress(null);
      void refresh();
    }
  };

  const start = async () => {
    setErr(null);
    try {
      setMsg(await invoke<string>("start_dsh_web"));
      // La UI triga uns segons a escoltar el port 3080: tornem a mirar.
      setTimeout(() => void refresh(), 4000);
    } catch (e) {
      setErr(String(e));
    }
  };

  /// Obre la interfície de dsh (http://127.0.0.1:3080) al navegador intern
  /// de NoOrbit — mateixa comanda que usa la IA amb «NB|PREGUNTA_IA|dsh|…».
  const openUi = async () => {
    setErr(null);
    try {
      await invoke("web_ia_open", { name: "dsh" });
    } catch (e) {
      setErr(String(e));
    }
  };

  const status = !installed
    ? t("providers.harnessNotInstalled")
    : running
      ? t("providers.harnessRunning")
      : t("providers.harnessInstalled");

  return (
    <div className="prov-form">
      <h4>{t("providers.harnessSection")}</h4>
      <p className="lp-hint">{t("providers.harnessIntro")}</p>

      <div className="prov-row">
        <div className="prov-main">
          <div className="prov-title">{t("providers.harnessDshName")}</div>
          <div className="prov-url">{t("providers.harnessDshDesc")}</div>
          <div className="prov-token">{status}</div>
        </div>
        <div className="prov-actions">
          {!installed ? (
            <button className="btn sm primary" disabled={busy} onClick={() => void install()}>
              {busy ? <RefreshCw size={12} className="spin" /> : <Download size={12} />}{" "}
              {busy ? t("providers.installingDsh") : t("providers.installDsh")}
            </button>
          ) : (
            <>
              {!running && (
                <button className="btn sm" onClick={() => void start()}>
                  <Play size={12} /> {t("providers.startDsh")}
                </button>
              )}
              <button className="btn sm" onClick={() => void openUi()}>
                <ExternalLink size={12} /> {t("providers.openDsh")}
              </button>
              <button className="icon-btn" onClick={() => void refresh()} title={t("common.refresh")}>
                <RefreshCw size={13} />
              </button>
            </>
          )}
        </div>
      </div>

      {busy && progress && (
        <div className="lp-hint">
          <RefreshCw size={12} className="spin" /> {progress}
        </div>
      )}
      {msg && <div className="settings-msg ok">{msg}</div>}
      {err && <div className="settings-msg err">{err}</div>}

      <p className="lp-hint">{t("providers.harnessDeepSeekNote")}</p>
    </div>
  );
}
