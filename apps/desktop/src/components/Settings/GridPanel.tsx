/// SECCIÓ «XARXA ALTRUISTA (P2P)» DELS AJUSTOS.
///
/// Una sola secció cobreix els dos papers de l'ordinador a la xarxa:
///  - PROPIETARI: comparteix els torns de pensament de la Jam actual
///    («Genera invitació») i, quan el projecte acaba, pot distribuir-ne una
///    còpia als participants.
///  - VOLUNTARI: afegeix projectes d'altri amb la seva invitació i els dona
///    inferència quan l'ordinador és ociós (estalvi obert + res en marxa).
///
/// Tot passa pel backend (`grid_status`, `grid_set_enabled`, …): aquí només
/// es pinta i s'envien les ordres.

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ClipboardCopy, HeartHandshake, Loader2, Network, Plus, Trash2, Users } from "lucide-react";
import { useT } from "../../i18n";
import { useGridStore, type GridStatus } from "../../stores/gridStore";

/// Mida en bytes amb unitat llegible i coma decimal (català).
function fmtBytes(b: number): string {
  if (!b || b <= 0) return "0 MB";
  const gb = b / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(1).replace(".", ",")} GB`;
  return `${Math.round(b / 1024 / 1024)} MB`;
}

export default function GridPanel() {
  const { t } = useT();
  const status = useGridStore((s) => s.status);
  const load = useGridStore((s) => s.load);
  const [busy, setBusy] = useState(false);
  const [port, setPort] = useState(38441);
  const [invitacio, setInvitacio] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [copiat, setCopiat] = useState(false);
  const copiatTimer = useRef<number | undefined>(undefined);

  // Estat inicial + refresc amb cada esdeveniment de la xarxa.
  useEffect(() => {
    void load().then(() => undefined);
    const un = listen("grid://event", () => void load());
    return () => {
      void un.then((u) => u());
    };
  }, [load]);

  // El port de l'últim estat conegut s'ofereix com a valor de l'input.
  useEffect(() => {
    if (status && port === 38441 && status.port !== 38441) setPort(status.port);
  }, [status, port]);

  /// Executa una ordre del backend i refresca l'estat (errors a pantalla).
  const corre = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    setErr(null);
    try {
      await fn();
      await load();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const setEnabled = (enabled: boolean) =>
    void corre(() => invoke("grid_set_enabled", { enabled, port }));

  const comparteix = () =>
    void corre(async () => {
      const s = await invoke<GridStatus>("grid_share_new");
      setInvitacio(s.invitacio);
    });

  const aturaCompartir = () => void corre(() => invoke("grid_share_stop"));

  const afegeixPeer = () =>
    void corre(async () => {
      if (!invitacio.trim()) {
        setErr(t("settings.grid.errInvitacio"));
        return;
      }
      await invoke("grid_peer_add", { invitacio: invitacio.trim() });
    });

  const esborraPeer = (addr: string) =>
    void corre(() => invoke("grid_peer_remove", { addr }));

  const copiaAlPortaretalls = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiat(true);
      window.clearTimeout(copiatTimer.current);
      copiatTimer.current = window.setTimeout(() => setCopiat(false), 2000);
    } catch {
      // El porta-retalls pot estar bloquejat: es deixa el text visible
      // perquè l'usuari el copiï a mà.
    }
  };

  const s = status;
  return (
    <div className="settings-general">
      {/* Interruptor mestre + port */}
      <div className="setting-row">
        <label className="setting-check">
          <input
            type="checkbox"
            checked={s?.enabled ?? false}
            disabled={busy}
            onChange={(e) => setEnabled(e.target.checked)}
          />
          <Network size={13} style={{ verticalAlign: -2, marginRight: 4 }} />
          {t("settings.grid.enabled")}
        </label>
        <div className="lp-hint">{t("settings.grid.enabledHint")}</div>
      </div>

      <div className="setting-row">
        <span>{t("settings.grid.port")}</span>
        <input
          type="number"
          min={1024}
          max={65535}
          value={port}
          disabled={busy}
          onChange={(e) => setPort(Number(e.target.value))}
        />
      </div>

      {/* COMPTADORS: torns donats (voluntari) i rebuts (propietari) */}
      {s && (s.tornsDonats > 0 || s.tornsRebuts > 0) && (
        <div className="setting-row">
          <span className="lp-hint">
            <HeartHandshake size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
            {t("settings.grid.donats")}: <b>{s.tornsDonats}</b>
            {" · "}
            {t("settings.grid.rebuts")}: <b>{s.tornsRebuts}</b>
          </span>
        </div>
      )}

      {/* PROPIETARI: compartir la Jam actual */}
      <div className="setting-row setting-row-col">
        <span>{t("settings.grid.shareTitle")}</span>
        <div className="lp-hint">{t("settings.grid.shareHint")}</div>
        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          {!s?.compartint ? (
            <button className="btn sm primary" disabled={busy} onClick={comparteix}>
              {busy ? <Loader2 size={12} className="spin" /> : <HeartHandshake size={12} />}
              {t("settings.grid.shareBtn")}
            </button>
          ) : (
            <>
              <button className="btn sm" disabled={busy} onClick={aturaCompartir}>
                {t("settings.grid.shareStop")}
              </button>
              <span className="lp-hint" style={{ color: "var(--accent)" }}>
                {t("settings.grid.sharing")}
              </span>
            </>
          )}
        </div>
        {s?.compartint && s.invitacio && (
          <div className="setting-row" style={{ marginTop: 6 }}>
            <input
              readOnly
              value={s.invitacio}
              title={t("settings.grid.invitacioHint")}
              onFocus={(e) => e.currentTarget.select()}
            />
            <button
              className="btn sm"
              onClick={() => void copiaAlPortaretalls(s.invitacio)}
              title={t("settings.grid.copyHint")}
            >
              <ClipboardCopy size={12} /> {copiat ? t("settings.grid.copied") : t("settings.grid.copy")}
            </button>
          </div>
        )}
        {/* Voluntaris connectats a la nostra Jam */}
        {s && s.voluntaris.length > 0 && (
          <div className="lp-hint" style={{ marginTop: 6 }}>
            <Users size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
            {s.voluntaris.map((v) => `${v.nom} (${v.torns})`).join(", ")}
          </div>
        )}
      </div>

      {/* VOLUNTARI: projectes ajudats */}
      <div className="setting-row setting-row-col">
        <span>{t("settings.grid.peersTitle")}</span>
        <div className="lp-hint">{t("settings.grid.peersHint")}</div>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <input
            style={{ flex: 1 }}
            value={invitacio}
            placeholder={t("settings.grid.peersPh")}
            onChange={(e) => setInvitacio(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") afegeixPeer();
            }}
          />
          <button className="btn sm" disabled={busy || !invitacio.trim()} onClick={afegeixPeer}>
            <Plus size={12} /> {t("settings.grid.peersAdd")}
          </button>
        </div>
        {s && s.peers.length > 0 && (
          <div className="lp-hint" style={{ marginTop: 6 }}>
            {s.peers.map((p) => (
              <div key={p.addr} style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <span style={{ flex: 1 }}>
                  {p.name || p.addr} {p.connectat ? `· ${t("settings.grid.connected")}` : ""}
                </span>
                <button className="icon-btn" title={t("settings.grid.peersRemove")} onClick={() => esborraPeer(p.addr)}>
                  <Trash2 size={13} />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* DISTRIBUCIÓ (propietari): estat de la còpia del projecte acabat */}
      {s?.distribucio && (
        <div className="setting-row setting-row-col">
          <span>{t("settings.grid.distTitle")}</span>
          <div className="lp-hint">
            {s.distribucio.activa
              ? t("settings.grid.distActive", {
                  peces: s.distribucio.peces,
                  fitxers: s.distribucio.fitxers,
                  bytes: fmtBytes(s.distribucio.totalBytes),
                  baixades: s.distribucio.baixades,
                })
              : t("settings.grid.distStopped")}
          </div>
        </div>
      )}

      {/* CÒPIES (voluntari): baixades en curs o acabades */}
      {s && s.copies.length > 0 && (
        <div className="setting-row setting-row-col">
          <span>{t("settings.grid.copiesTitle")}</span>
          {s.copies.map((c) => (
            <div key={c.projecte} className="lp-hint">
              {c.projecte} — {t("settings.grid.copyPhase." + (c.fase in { baixant: 1, muntant: 1, feta: 1, error: 1 } ? c.fase : "baixant"))}{" "}
              {c.fase === "feta" ? `· ${c.desti}` : c.error ? `· ${c.error}` : `· ${c.fetes}/${c.total}`}
            </div>
          ))}
        </div>
      )}

      {err && <div className="si-error" style={{ marginTop: 8 }}>{err}</div>}
    </div>
  );
}
