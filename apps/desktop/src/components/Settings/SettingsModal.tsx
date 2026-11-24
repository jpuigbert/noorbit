import { useEffect, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { X, Globe, Info, SlidersHorizontal, Plug, RefreshCw, Download, HardDrive, Volume2, Network } from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";
import { useUpdateStore } from "../../stores/updateStore";
import { FUN_PREVIEW } from "../../utils/funCursors";
import { playFunDone } from "../../utils/funSound";
import { useEscapeToClose } from "../../utils/useEscapeToClose";
import LanguagePanel from "../Language/LanguagePanel";
import IntegrationsPanel from "./IntegrationsPanel";
import PortablePanel from "./PortablePanel";
import GridPanel from "./GridPanel";

type Tab = "general" | "integracions" | "language" | "xarxa" | "about" | "usb";

const DELAYS = [500, 1000, 2000, 5000];

/// Minuts d'inactivitat per obrir sol l'estalvi de pantalla (1 per a la
/// prova manual, 5 per defecte, fins a mitja hora per a qui el vol tard).
const SAVER_MINUTES = [1, 3, 5, 10, 15, 30];

/// Secció «Actualitzacions» del panell Informació: pregunta a GitHub si hi ha
/// una versió nova publicada (releases del repo) i, si n'hi ha, oferix la
/// descàrrega directa de l'instal·ador per al nostre sistema. NoOrbit NO
/// s'autoinsta·la mai: descarrega/obri i l'usuari decidix.
function UpdateSection() {
  const { info, checking, error, check } = useUpdateStore(
    useShallow((st) => ({
      info: st.info,
      checking: st.checking,
      error: st.error,
      check: st.check,
    }))
  );
  const { t } = useT();
  return (
    <div className="setting-row setting-row-col update-section">
      <span>{t("settings.update.title")}</span>
      <div className="lp-hint">
        {checking
          ? t("settings.update.checking")
          : error
          ? error
          : info
          ? info.available
            ? t("settings.update.available", { latest: info.latest, current: info.current })
            : t("settings.update.upToDate", { current: info.current })
          : t("settings.update.hint")}
      </div>
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <button className="btn sm" disabled={checking} onClick={() => void check(true)}>
          <RefreshCw size={12} className={checking ? "spin" : undefined} /> {t("settings.update.checkNow")}
        </button>
        {info?.available && (
          <button
            className="btn sm primary"
            title={t("settings.update.downloadHint")}
            onClick={() =>
              void invoke("open_externally", { path: info.asset ?? info.url }).catch(() => undefined)
            }
          >
            <Download size={12} /> {t("settings.update.download", { latest: info.latest })}
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
  const saverAuto = useUIStore((s) => s.saverAuto);
  const saverAutoMinutes = useUIStore((s) => s.saverAutoMinutes);
  const setSaverAuto = useUIStore((s) => s.setSaverAuto);
  const setSaverAutoMinutes = useUIStore((s) => s.setSaverAutoMinutes);
  const zoom = useUIStore((s) => s.zoom);
  const setZoom = useUIStore((s) => s.setZoom);
  const theme = useUIStore((s) => s.theme);
  const setTheme = useUIStore((s) => s.setTheme);
  const funCursor = useUIStore((s) => s.funCursor);
  const setFunCursor = useUIStore((s) => s.setFunCursor);
  const funSound = useUIStore((s) => s.funSound);
  const setFunSound = useUIStore((s) => s.setFunSound);
  const professionalMode = useUIStore((s) => s.professionalMode);
  const setProfessionalMode = useUIStore((s) => s.setProfessionalMode);
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
    { key: "gold", label: t("settings.themeGold"), swatch: "#17231e" },
    { key: "yellow", label: t("settings.themeYellow"), swatch: "#302810" },
  ];

  // Esc tanca el modal mentre és obert (com la X i el clic a l'overlay).
  useEscapeToClose(show, () => setShow(false));

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
            className={"pm-tab" + (tab === "xarxa" ? " active" : "")}
            onClick={() => setTab("xarxa")}
          >
            <Network size={14} /> {t("settings.gridTab")}
          </button>
          <button
            className={"pm-tab" + (tab === "usb" ? " active" : "")}
            onClick={() => setTab("usb")}
          >
            <HardDrive size={14} /> {t("settings.portable.tab")}
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

              {/* Estalvi de pantalla automàtic: s'obre sol quan la Jam o la
                  feina autopilotada corre i no es toca res (en lloc de
                  l'estalvi del SO, que amagaria la feina). */}
              <div className="setting-row">
                <label className="setting-check">
                  <input
                    type="checkbox"
                    checked={saverAuto}
                    onChange={(e) => setSaverAuto(e.target.checked)}
                  />
                  {t("settings.saverAuto")}
                </label>
                <div className="lp-hint">{t("settings.saverAutoHint")}</div>
              </div>

              <div className="setting-row">
                <span>{t("settings.saverMinutes")}</span>
                <select
                  className="setting-select"
                  value={saverAutoMinutes}
                  disabled={!saverAuto}
                  onChange={(e) => setSaverAutoMinutes(Number(e.target.value))}
                >
                  {SAVER_MINUTES.map((m) => (
                    <option key={m} value={m}>
                      {m} min
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

              {/* Mode professional: silencia els elements lúdics (cursor,
                  confeti, sons) sense esborrar les preferències de sota. */}
              <div className="setting-row">
                <label className="setting-check">
                  <input
                    type="checkbox"
                    checked={professionalMode}
                    onChange={(e) => setProfessionalMode(e.target.checked)}
                  />
                  {t("settings.professionalMode")}
                </label>
                <div className="lp-hint">{t("settings.professionalModeHint")}</div>
              </div>

              {/* Diversió: cursor disfressat + so de tasca acabada. Les
                  miniatures mostren les disfresses reals (SVG de 32 px).
                  Amb el mode professional actiu queden silenciades. */}
              <div className="setting-row setting-row-col">
                <span>{t("settings.funSection")}</span>
                {professionalMode && (
                  <div className="lp-hint">{t("settings.funMutedHint")}</div>
                )}
                <label className="setting-check">
                  <input
                    type="checkbox"
                    checked={funCursor}
                    disabled={professionalMode}
                    onChange={(e) => setFunCursor(e.target.checked)}
                  />
                  {t("settings.funCursor")}
                </label>
                <div className="lp-hint">{t("settings.funCursorHint")}</div>
                <div className="fun-preview">
                  {FUN_PREVIEW.map((c) => (
                    <img key={c.name} src={c.img} alt={c.name} title={c.name} />
                  ))}
                </div>
                <label className="setting-check">
                  <input
                    type="checkbox"
                    checked={funSound}
                    disabled={professionalMode}
                    onChange={(e) => setFunSound(e.target.checked)}
                  />
                  {t("settings.funSound")}
                </label>
                <div className="lp-hint">{t("settings.funSoundHint")}</div>
                <div>
                  <button
                    className="btn sm"
                    disabled={professionalMode}
                    onClick={() => playFunDone(true)}
                  >
                    <Volume2 size={12} /> {t("settings.funSoundTest")}
                  </button>
                </div>
              </div>
            </div>
          )}

          {tab === "language" && <LanguagePanel />}

          {tab === "usb" && <PortablePanel />}

          {tab === "integracions" && <IntegrationsPanel />}

          {/* XARXA ALTRUISTA P2P: compartir els torns de la Jam i donar
              inferència a projectes d'altri (estil SETI@home, sense servidor
              central: invitacions directes per QUIC). */}
          {tab === "xarxa" && <GridPanel />}

          {tab === "about" && (
            <div className="about-block">
              <h3>{t("app.name")} v{appVersion || "?"}</h3>
              <p>{t("app.tagline")}</p>
              <p className="lp-hint">
                Editor creatiu amb IA local (Ollama), connectors, Blender i Unreal Engine 5.
              </p>
              {/* FEINA FETA PER A JOAN PUIGBERT CARRERAS (thepuigberts.com):
                  el projecte de l'usuari — el marketplace de temes WordPress —
                  i el que s'hi ha fet dins de NoOrbit. */}
              <div className="about-credits">
                <h4>{t("about.creditsTitle")}</h4>
                <p className="lp-hint">{t("about.creditsIntro")}</p>
                <ul>
                  <li>{t("about.credit1")}</li>
                  <li>{t("about.credit2")}</li>
                  <li>{t("about.credit3")}</li>
                  <li>{t("about.credit4")}</li>
                </ul>
                <p className="about-credits-foot">
                  {t("about.creditAuthor")} <b>Joan Puigbert Carreras</b> · {t("about.creditSite")}{" "}
                  <b>thepuigberts.com</b>{" "}
                  <button
                    className="about-credits-link"
                    onClick={() =>
                      void invoke("open_externally", {
                        path: "https://thepuigberts.com",
                      }).catch(() => undefined)
                    }
                  >
                    {t("about.creditLink")}
                  </button>
                </p>
              </div>
              <UpdateSection />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
