import { useCallback, useEffect, useState } from "react";
import { HardDrive, Copy, CheckCircle2, AlertCircle, Usb } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../../i18n";

/// Panell de mode PORTÀTIL (USB / disc extern): mostra com està configurat
/// l'emmagatzematge, permet copiar l'aplicació a un disc extern ja preparada
/// (marcador + dades migrades) i activar la migració manualment.
type PortableStatus = {
  portable: boolean;
  data_dir: string | null;
  mac_dir: string | null;
  portable_root: string | null;
  volumes: string[];
  activatable: boolean;
  in_system_location: boolean;
};
type MigrateReport = {
  destination: string | null;
  copied: string[];
  skipped: string[];
  excluded: string[];
  error: string | null;
};

export default function PortablePanel() {
  const { t } = useT();
  const [st, setSt] = useState<PortableStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<MigrateReport | null>(null);
  const [err, setErr] = useState("");
  const [takeData, setTakeData] = useState(true);

  const refresh = useCallback(async () => {
    try {
      setSt(await invoke<PortableStatus>("portable_status"));
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function copyTo(volume: string) {
    setBusy(true);
    setErr("");
    setReport(null);
    try {
      const rep = await invoke<MigrateReport>("portable_copy_app_to", {
        volumePath: volume,
        migrate: takeData,
      });
      setReport(rep);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function activateHere() {
    setBusy(true);
    setErr("");
    try {
      const rep = await invoke<MigrateReport>("portable_enable", { migrate: takeData });
      setReport(rep);
      await refresh();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  if (!st) {
    return <div className="empty-hint">{err || t("settings.portable.loading")}</div>;
  }

  return (
    <div className="settings-general">
      {/* ESTAT ACTUAL */}
      <div className="setting-row setting-row-col">
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          {st.portable ? (
            <Usb size={14} color="var(--success)" />
          ) : (
            <HardDrive size={14} />
          )}
          <span style={{ fontWeight: 600 }}>
            {st.portable
              ? t("settings.portable.onTitle")
              : t("settings.portable.offTitle")}
          </span>
        </div>
        <div className="lp-hint">{t("settings.portable.explain")}</div>
        {st.data_dir && (
          <div className="lp-hint" style={{ fontSize: 11 }}>
            {t("settings.portable.dataAt")} <code>{st.data_dir}</code>
          </div>
        )}
      </div>

      {/* COP IA A UN DISC EXTERN */}
      {st.volumes.length > 0 ? (
        <div className="setting-row setting-row-col">
          <span style={{ fontWeight: 600 }}>{t("settings.portable.copyTitle")}</span>
          <div className="lp-hint">{t("settings.portable.copyHint")}</div>
          <label className="setting-check" style={{ fontSize: 12 }}>
            <input
              type="checkbox"
              checked={takeData}
              onChange={(e) => setTakeData(e.target.checked)}
            />
            {t("settings.portable.takeData")}
          </label>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
            {st.volumes.map((v) => (
              <button key={v} className="btn sm" disabled={busy} onClick={() => void copyTo(v)}>
                <Copy size={12} /> {t("settings.portable.copyTo")} {v.split("/").pop()} {v}
              </button>
            ))}
          </div>
        </div>
      ) : (
        <div className="setting-row setting-row-col">
          <span style={{ fontWeight: 600 }}>{t("settings.portable.copyTitle")}</span>
          <div className="lp-hint">{t("settings.portable.noVolumes")}</div>
        </div>
      )}

      {/* ACTIVAR AL MAC (rares) */}
      {st.activatable && !st.portable && (
        <div className="setting-row setting-row-col">
          {st.in_system_location && (
            <div className="lp-hint">{t("settings.portable.systemWarn")}</div>
          )}
          <button className="btn sm" disabled={busy} onClick={() => void activateHere()}>
            {t("settings.portable.activate")}
          </button>
        </div>
      )}

      {/* RESULTAT */}
      {err && (
        <div className="lp-hint" style={{ color: "var(--error)" }}>
          <AlertCircle size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
          {err}
        </div>
      )}
      {report && (
        <div className="setting-row setting-row-col">
          <div style={{ color: "var(--success)" }}>
            <CheckCircle2 size={13} style={{ verticalAlign: -2, marginRight: 4 }} />
            {report.error
              ? `${t("settings.portable.partial")} ${report.error}`
              : t("settings.portable.done")}
          </div>
          {report.destination && (
            <div className="lp-hint" style={{ fontSize: 11 }}>
              {t("settings.portable.readyAt")} <code>{report.destination}</code>
            </div>
          )}
          <div className="lp-hint">{t("settings.portable.restartHint")}</div>
          {report.excluded.length > 0 && (
            <div className="lp-hint" style={{ fontSize: 11 }}>
              {t("settings.portable.excluded")}: {report.excluded.join(", ")}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
