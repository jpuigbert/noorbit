import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw } from "lucide-react";
import { useT } from "../../i18n";
import { useBlenderLiveStore } from "../../stores/blenderLiveStore";

/// Forma mínima del config que necessitem tocar. Conservem la resta de camps
/// tal com venen de `get_config` per no perdre'ls en reenviar tot l'objecte a
/// `update_config` (aquest deserialitza un `AppConfig` sencer).
interface AppConfigShape {
  blender?: { socket_port?: number; binary?: string; auto_connect?: boolean };
  tasks?: { foreground_limit_s?: number; total_limit_s?: number };
  [k: string]: unknown;
}

/// Panell d'integracions (Settings ▸ Integracions): connecta el modal amb
/// `get_config`/`update_config` per editar opcions persistents: auto-connexió
/// a Blender (Objectiu 1) i els límits de temps de les tasques llargues
/// (Objectiu 4, degradació a segon pla).
export default function IntegrationsPanel() {
  const { t } = useT();
  const setAutoConnect = useBlenderLiveStore((s) => s.setAutoConnect);

  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const [cfg, setCfg] = useState<AppConfigShape | null>(null);
  const [autoConnect, setAutoConnectLocal] = useState(false);
  const [fg, setFg] = useState(60);
  const [total, setTotal] = useState(3600);

  // Carrega la configuració completa des del backend.
  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const c = await invoke<AppConfigShape>("get_config");
        if (!alive) return;
        setCfg(c);
        setAutoConnectLocal(c.blender?.auto_connect ?? true);
        setFg(c.tasks?.foreground_limit_s ?? 60);
        setTotal(c.tasks?.total_limit_s ?? 3600);
      } catch (e) {
        if (alive) setErr(String(e));
      } finally {
        if (alive) setLoaded(true);
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  // Escriu la configuració sencera amb els camps editats aplicats.
  const save = async () => {
    if (!cfg) return;
    setBusy(true);
    setMsg(null);
    setErr(null);
    // Copia superficial preservant tots els altres camps/estructures.
    const next: AppConfigShape = {
      ...cfg,
      blender: { ...(cfg.blender ?? {}), auto_connect: autoConnect },
      tasks: { ...(cfg.tasks ?? {}), foreground_limit_s: fg, total_limit_s: total },
    };
    try {
      await invoke("update_config", { config: next });
      // Arrenca/atura el vigilant ara mateix, no només al proper arrancada.
      await setAutoConnect(autoConnect);
      setCfg(next);
      setMsg(t("settings.configSaved"));
    } catch (e) {
      setErr(t("settings.configSaveError") + ": " + String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!loaded) {
    return (
      <div className="settings-general">
        <div className="lp-hint">
          <RefreshCw size={12} className="spin" /> {t("settings.configLoading")}
        </div>
      </div>
    );
  }

  return (
    <div className="settings-general">
      {/* ── Blender (Objectiu 1) ── */}
      <div className="setting-row setting-row-col">
        <span className="settings-section-title">{t("settings.blenderSection")}</span>
        <label className="setting-check">
          <input
            type="checkbox"
            checked={autoConnect}
            onChange={(e) => setAutoConnectLocal(e.target.checked)}
          />
          {t("settings.autoConnectLabel")}
        </label>
        <div className="lp-hint">{t("settings.autoConnectHint")}</div>
      </div>

      {/* ── Tasques llargues (Objectiu 4) ── */}
      <div className="setting-row setting-row-col">
        <span className="settings-section-title">{t("settings.tasksSection")}</span>
        <div className="lp-hint">{t("settings.tasksHint")}</div>

        <div className="setting-row">
          <span>{t("settings.foregroundLimit")}</span>
          <input
            className="setting-num"
            type="number"
            min={1}
            step={5}
            value={fg}
            onChange={(e) => setFg(Math.max(1, Number(e.target.value) || 0))}
          />
        </div>
        <div className="setting-row">
          <span>{t("settings.totalLimit")}</span>
          <input
            className="setting-num"
            type="number"
            min={0}
            step={60}
            value={total}
            onChange={(e) => setTotal(Math.max(0, Number(e.target.value) || 0))}
          />
        </div>
      </div>

      <div className="settings-actions">
        <button className="btn primary sm" disabled={busy} onClick={() => void save()}>
          {busy ? <RefreshCw size={12} className="spin" /> : null} {t("common.save")}
        </button>
        {msg && <span className="settings-msg ok">{msg}</span>}
        {err && <span className="settings-msg err">{err}</span>}
      </div>
    </div>
  );
}
