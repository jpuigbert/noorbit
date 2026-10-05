import { useEffect, useState } from "react";
import {
  Smartphone,
  Apple,
  RefreshCw,
  Play,
  Square,
  Eye,
  EyeOff,
  ExternalLink,
  AlertTriangle,
  CheckCircle2,
  XCircle,
} from "lucide-react";
import { useT } from "../../i18n";
import {
  useMobileStore,
  type AndroidAvd,
  type IosDevice,
  type IosRuntimeGroup,
} from "../../stores/mobileStore";

type Sub = "android" | "ios";

export default function MobilePanel() {
  const { t } = useT();
  const [sub, setSub] = useState<Sub>("android");
  const {
    status,
    avds,
    iosGroups,
    loading,
    error,
    liveShot,
    livePlatform,
    liveTarget,
    liveError,
    refreshStatus,
    refreshAndroid,
    refreshIos,
    startAvd,
    stopAvd,
    bootIos,
    shutdownIos,
    openIos,
    startLive,
    stopLive,
  } = useMobileStore();

  // En muntar: detectem SDKs. Si un panell ja està actiu, carregem la seva
  // llista (que és més lenta que la detecció).
  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);
  useEffect(() => {
    if (status?.android.installed) void refreshAndroid();
  }, [status, refreshAndroid]);
  useEffect(() => {
    if (status?.ios.installed) void refreshIos();
  }, [status, refreshIos]);

  // Neteja el timer en desmuntar.
  useEffect(() => {
    return () => stopLive();
  }, [stopLive]);

  const isLiveHere = livePlatform === sub;

  return (
    <div className="mobile-panel" style={{ padding: 10, display: "flex", flexDirection: "column", gap: 10 }}>
      {/* Subpestanyes */}
      <div style={{ display: "flex", gap: 6 }}>
        <TabButton active={sub === "android"} onClick={() => setSub("android")}>
          <Smartphone size={13} /> {t("mobile.android.tab")}
        </TabButton>
        <TabButton active={sub === "ios"} onClick={() => setSub("ios")}>
          <Apple size={13} /> {t("mobile.ios.tab")}
        </TabButton>
      </div>

      {/* Capçalera d'estat */}
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <button className="btn sm" disabled={loading} onClick={() => void refreshStatus()}>
          <RefreshCw size={12} className={loading ? "spin" : undefined} /> {t("mobile.rescan")}
        </button>
        {sub === "android" && <SdkBadge ok={!!status?.android.installed} hint={status?.android.hint ?? null} />}
        {sub === "ios" && (
          <SdkBadge
            ok={!!status?.ios.installed}
            hint={status?.ios.supported ? status?.ios.hint ?? null : t("mobile.ios.unsupported")}
          />
        )}
      </div>

      {error && (
        <div className="si-error" style={{ fontSize: 12 }}>
          {error}
        </div>
      )}

      {sub === "android" ? (
        <AndroidSection
          avds={avds}
          installed={!!status?.android.installed}
          onStart={(n) => void startAvd(n)}
          onStop={(n) => void stopAvd(n)}
          onLive={(n) => void startLive("android", n)}
          onStopLive={stopLive}
          liveActive={isLiveHere && liveTarget === null}
          busyLive={isLiveHere}
          t={t}
        />
      ) : (
        <IosSection
          groups={iosGroups}
          installed={!!status?.ios.installed}
          supported={!!status?.ios.supported}
          onBoot={(u) => void bootIos(u)}
          onShutdown={(u) => void shutdownIos(u)}
          onOpen={(u) => void openIos(u)}
          onLive={(u) => void startLive("ios", u)}
          onStopLive={stopLive}
          busyLiveUdid={isLiveHere ? liveTarget : null}
          t={t}
        />
      )}

      {/* Vista en viu (s'ensenya en ambdós sub-panells si està activa) */}
      {isLiveHere && (
        <div style={{ marginTop: 8 }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <b style={{ fontSize: 12 }}>{t("mobile.live.title")}</b>
            <button className="btn sm" onClick={stopLive}>
              <EyeOff size={12} /> {t("mobile.live.stop")}
            </button>
          </div>
          {liveError ? (
            <div className="si-error" style={{ fontSize: 11, marginTop: 6 }}>
              {liveError}
            </div>
          ) : liveShot ? (
            <img
              src={liveShot}
              alt="screenshot"
              style={{
                width: "100%",
                maxHeight: 460,
                objectFit: "contain",
                marginTop: 6,
                border: "1px solid var(--border)",
                borderRadius: 6,
                background: "#111",
              }}
            />
          ) : (
            <div style={{ fontSize: 11, marginTop: 6, color: "var(--text-3)" }}>
              {t("mobile.live.waiting")}
            </div>
          )}
        </div>
      )}

      {/* Ajuda sobre codi natiu */}
      <div
        style={{
          fontSize: 11,
          color: "var(--text-2)",
          border: "1px dashed var(--border)",
          borderRadius: 6,
          padding: 8,
          marginTop: 8,
        }}
      >
        <b style={{ color: "var(--text-0)" }}>{t("mobile.nativeNote.title")}</b>
        <div style={{ marginTop: 4 }}>{t("mobile.nativeNote.body")}</div>
      </div>
    </div>
  );
}

// ── Components auxiliars ────────────────────────────────────────────────────

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      className={"btn sm" + (active ? " primary" : "")}
      onClick={onClick}
      style={{ display: "inline-flex", alignItems: "center", gap: 4 }}
    >
      {children}
    </button>
  );
}

function SdkBadge({ ok, hint }: { ok: boolean; hint: string | null }) {
  const color = ok ? "var(--success)" : "var(--text-3)";
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 4, color, fontSize: 12 }}>
      {ok ? <CheckCircle2 size={13} /> : hint ? <AlertTriangle size={13} /> : <XCircle size={13} />}
      {hint && (
        <span title={hint} style={{ color: "var(--text-3)", maxWidth: 240, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", display: "inline-block", verticalAlign: "middle" }}>
          {hint}
        </span>
      )}
    </span>
  );
}

function AndroidSection({
  avds,
  installed,
  onStart,
  onStop,
  onLive,
  onStopLive,
  busyLive,
  t,
}: {
  avds: AndroidAvd[];
  installed: boolean;
  onStart: (n: string) => void;
  onStop: (n: string) => void;
  onLive: (n?: string) => void;
  onStopLive: () => void;
  liveActive: boolean;
  busyLive: boolean;
  t: (k: string) => string;
}) {
  if (!installed) {
    return <div style={{ fontSize: 12, color: "var(--text-3)" }}>{t("mobile.android.needSdk")}</div>;
  }
  if (avds.length === 0) {
    return (
      <div style={{ fontSize: 12, color: "var(--text-3)" }}>
        {t("mobile.android.noAvds")}
        <div style={{ marginTop: 4 }}>{t("mobile.android.noAvdsHint")}</div>
      </div>
    );
  }
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      {avds.map((a) => (
        <div
          key={a.name}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            padding: "6px 8px",
            border: "1px solid var(--border)",
            borderRadius: 6,
          }}
        >
          <span style={{ flex: 1, fontFamily: "var(--mono)", fontSize: 12 }}>{a.name}</span>
          {a.running && (
            <span style={{ color: "var(--success)", fontSize: 11 }}>● {t("mobile.android.running")}</span>
          )}
          {a.running ? (
            <>
              <button className="btn sm" onClick={() => onLive(a.name)}>
                <Eye size={12} /> {t("mobile.live.view")}
              </button>
              <button className="btn sm danger" onClick={() => onStop(a.name)}>
                <Square size={12} /> {t("mobile.android.stop")}
              </button>
            </>
          ) : (
            <button className="btn sm primary" onClick={() => onStart(a.name)}>
              <Play size={12} /> {t("mobile.android.start")}
            </button>
          )}
        </div>
      ))}
      {busyLive && (
        <div style={{ display: "flex", gap: 6, marginTop: 4 }}>
          <button className="btn sm" onClick={() => onLive(undefined)}>
            <RefreshCw size={12} /> {t("mobile.live.retry")}
          </button>
          <button className="btn sm" onClick={onStopLive}>
            <EyeOff size={12} /> {t("mobile.live.stop")}
          </button>
        </div>
      )}
    </div>
  );
}

function IosSection({
  groups,
  installed,
  supported,
  onBoot,
  onShutdown,
  onOpen,
  onLive,
  onStopLive,
  busyLiveUdid,
  t,
}: {
  groups: IosRuntimeGroup[];
  installed: boolean;
  supported: boolean;
  onBoot: (u: string) => void;
  onShutdown: (u: string) => void;
  onOpen: (u: string) => void;
  onLive: (u: string) => void;
  onStopLive: () => void;
  busyLiveUdid: string | null;
  t: (k: string) => string;
}) {
  if (!supported) {
    return <div style={{ fontSize: 12, color: "var(--text-3)" }}>{t("mobile.ios.onlyMac")}</div>;
  }
  if (!installed) {
    return <div style={{ fontSize: 12, color: "var(--text-3)" }}>{t("mobile.ios.needXcode")}</div>;
  }
  if (groups.length === 0) {
    return (
      <div style={{ fontSize: 12, color: "var(--text-3)" }}>
        {t("mobile.ios.noDevices")}
        <div style={{ marginTop: 4 }}>{t("mobile.ios.noDevicesHint")}</div>
      </div>
    );
  }
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      {groups.map((g) => (
        <div key={g.runtime}>
          <div style={{ fontSize: 11, color: "var(--text-2)", marginBottom: 4 }}>iOS {g.build}</div>
          {g.devices.map((d: IosDevice) => (
            <div
              key={d.udid}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "6px 8px",
                border: "1px solid var(--border)",
                borderRadius: 6,
                marginBottom: 4,
              }}
            >
              <span style={{ flex: 1, fontSize: 12 }}>{d.name}</span>
              {d.state === "Booted" && (
                <span style={{ color: "var(--success)", fontSize: 11 }}>● {t("mobile.ios.booted")}</span>
              )}
              {d.state === "Booted" ? (
                <>
                  <button className="btn sm" onClick={() => onOpen(d.udid)} title={t("mobile.ios.openHint")}>
                    <ExternalLink size={12} /> {t("mobile.ios.open")}
                  </button>
                  <button className="btn sm" onClick={() => onLive(d.udid)}>
                    <Eye size={12} /> {t("mobile.live.view")}
                  </button>
                  <button className="btn sm danger" onClick={() => onShutdown(d.udid)}>
                    <Square size={12} /> {t("mobile.ios.shutdown")}
                  </button>
                </>
              ) : (
                <button className="btn sm primary" onClick={() => onBoot(d.udid)}>
                  <Play size={12} /> {t("mobile.ios.boot")}
                </button>
              )}
            </div>
          ))}
        </div>
      ))}
      {busyLiveUdid && (
        <div style={{ display: "flex", gap: 6, marginTop: 4 }}>
          <button className="btn sm" onClick={onStopLive}>
            <EyeOff size={12} /> {t("mobile.live.stop")}
          </button>
        </div>
      )}
    </div>
  );
}
