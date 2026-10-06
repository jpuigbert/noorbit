import { useEffect, useState } from "react";
import { X, Globe, Info, SlidersHorizontal, Plug, RefreshCw, Download } from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";
import { useUpdateStore } from "../../stores/updateStore";
import LanguagePanel from "../Language/LanguagePanel";
import IntegrationsPanel from "./IntegrationsPanel";

type Tab = "general" | "integracions" | "language" | "about";

const DELAYS = [500, 1000, 2000, 5000];

/// Secció «Actualitzacions» del panell Informació: pregunta a GitHub si hi ha
/// una versió nova publicada (releases del repo) i, si n'hi ha, oferix la
/// descàrrega directa de l'instal·ador per al nostre sistema. NoOrbit NO
/// s'autoinsta·la mai: descarrega/obri i l'usuari decidix.
function UpdateSection() {
  const { info, checking, error, check } = useUpdateStore();
  return (
    <div className="setting-row setting-row-col update-section">
      <span>Actualitzacions</span>
      <div className="lp-hint">
        {checking
          ? "Comprovant GitHub…"
          : error
          ? error
          : info
          ? info.available
            ? `Hi ha una versió nova: ${info.latest} (tu tens la ${info.current}).`
            : `Ja tens l'última versió publicada (${info.current}).`
          : "Compara la teua versió amb l'últim release publicat a GitHub."}
      </div>
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <button className="btn sm" disabled={checking} onClick={() => void check(true)}>
          <RefreshCw size={12} className={checking ? "spin" : undefined} /> Comprova ara
        </button>
        {info?.available && (
          <button
            className="btn sm primary"
            title="Obri la descàrrega de la versió nova"
            onClick={() =>
              void invoke("open_externally", { path: info.asset ?? info.url }).catch(() => undefined)
            }
          >
            <Download size={12} /> Descarregar {info.latest}
          </button>
        )}
      </div>
    </div>
  );
}

export default function SettingsModal() {
  const show = useUIStore((s) => s.showSettings);
  const setShow = useUIStore((s) => s.setShowSettings);
  const autoSave = useUIStore((s) => s.autoSave);
  const autoSaveDelay = useUIStore((s) => s.autoSaveDelay);
  const setAutoSave = useUIStore((s) => s.setAutoSave);
  const setAutoSaveDelay = useUIStore((s) => s.setAutoSaveDelay);
  const zoom = useUIStore((s) => s.zoom);
  const setZoom = useUIStore((s) => s.setZoom);
  const theme = useUIStore((s) => s.theme);
  const setTheme = useUIStore((s) => s.setTheme);
  const { t } = useT();
  const [tab, setTab] = useState<Tab>("general");
  // Versió real de l'app (tauri.conf.json): ja no cal tocar-la en cada bump.
  const [appVersion, setAppVersion] = useState("");
  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => undefined);
  }, []);

  const THEMES: { key: Parameters<typeof setTheme>[0]; label: string; swatch: string }[] = [
    { key: "dark", label: t("settings.themeDark"), swatch: "#121218" },
    { key: "light", label: t("settings.themeLight"), swatch: "#ffffff" },
    { key: "gray", label: t("settings.themeGray"), swatch: "#3b3f45" },
    { key: "green", label: t("settings.themeGreen"), swatch: "#143022" },
    { key: "yellow", label: t("settings.themeYellow"), swatch: "#302810" },
  ];

  if (!show) return null;

  return (
    <div className="modal-overlay" onClick={() => setShow(false)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>{t("settings.title")}</span>
          <button className="icon-btn" onClick={() => setShow(false)}>
            <X size={16} />
          </button>
        </div>
        <div className="pm-tabs">
          <button
            className={"pm-tab" + (tab === "general" ? " active" : "")}
            onClick={() => setTab("general")}
          >
            <SlidersHorizontal size={14} /> {t("settings.general")}
          </button>
          <button
            className={"pm-tab" + (tab === "integracions" ? " active" : "")}
            onClick={() => setTab("integracions")}
          >
            <Plug size={14} /> {t("settings.integrations")}
          </button>
          <button
            className={"pm-tab" + (tab === "language" ? " active" : "")}
            onClick={() => setTab("language")}
          >
            <Globe size={14} /> {t("settings.language")}
          </button>
          <button
            className={"pm-tab" + (tab === "about" ? " active" : "")}
            onClick={() => setTab("about")}
          >
            <Info size={14} /> {t("menu.about")}
          </button>
        </div>
        <div className="modal-body">
          {tab === "general" && (
            <div className="settings-general">
              <div className="setting-row">
                <label className="setting-check">
                  <input
                    type="checkbox"
                    checked={autoSave}
                    onChange={(e) => setAutoSave(e.target.checked)}
                  />
                  {t("settings.autoSaveEnabled")}
                </label>
                <div className="lp-hint">{t("settings.autoSaveHint")}</div>
              </div>

              <div className="setting-row">
                <span>{t("settings.autoSaveDelay")}</span>
                <select
                  className="setting-select"
                  value={autoSaveDelay}
                  disabled={!autoSave}
                  onChange={(e) => setAutoSaveDelay(Number(e.target.value))}
                >
                  {DELAYS.map((d) => (
                    <option key={d} value={d}>
                      {d} ms
                    </option>
                  ))}
                </select>
              </div>

              <div className="setting-row">
                <span>{t("settings.zoom")}</span>
                <input
                  type="range"
                  min={0.6}
                  max={2}
                  step={0.1}
                  value={zoom}
                  onChange={(e) => setZoom(Number(e.target.value))}
                />
                <span className="setting-value">{Math.round(zoom * 100)}%</span>
              </div>

              <div className="setting-row setting-row-col">
                <span>{t("settings.themeSection")}</span>
                <div className="lp-hint">{t("settings.themeHint")}</div>
                <div className="theme-swatches">
                  {THEMES.map((th) => (
                    <button
                      key={th.key}
                      className={
                        "theme-swatch" + (theme === th.key ? " active" : "")
                      }
                      onClick={() => setTheme(th.key)}
                      title={th.label}
                    >
                      <span
                        className="theme-dot"
                        style={{ background: th.swatch }}
                      />
                      {th.label}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          )}

          {tab === "language" && <LanguagePanel />}

          {tab === "integracions" && <IntegrationsPanel />}

          {tab === "about" && (
            <div className="about-block">
              <h3>{t("app.name")} v{appVersion || "?"}</h3>
              <p>{t("app.tagline")}</p>
              <p className="lp-hint">
                Editor creatiu amb IA local (Ollama), connectors, Blender i Unreal Engine 5.
              </p>
              <UpdateSection />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
