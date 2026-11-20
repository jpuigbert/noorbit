import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BookOpen, RefreshCw, WifiOff } from "lucide-react";
import { useT } from "../../i18n";

/// Una nota del diari del vagueig (mateixa forma que `wander::Nota`).
interface Nota {
  quan: number;
  font: string;
  titol: string;
  url: string;
  conceptes: number;
}

/// El que l'informe del vagueig conté (mateixa forma que `wander::Estat`).
interface EstatVagueig {
  activat: boolean;
  segons_ocios: number;
  bloqueig: string | null;
  pagines: number;
  conceptes: number;
  acces_tor: boolean;
  adreces_profundes: number;
  /** El text de la configuració, per pre-replendir el camp d'adreces. */
  profunda_text: string;
  ram_procents: number | null;
  historial: Nota[];
}

/// Panell d'APRENENTATGE AUTÒNOM EN REPÒS (Configuració ▸ Integracions):
/// la IA nativa navega sola i aprén de contingut lliure mentre ningú no fa
/// servir el programa. Aquí es pot activar/desactivar, forçar una sessió ara
/// i veure QUÈ ha après, d'on ve i per què s'atura.
export default function AutonomousLearning() {
  const { t } = useT();
  const [estat, setEstat] = useState<EstatVagueig | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [adreces, setAdreces] = useState("");
  // Les adreces es carregen UNA sola vegada: si no, cada refrec de 10 s
  // esborraria el que l'usuari està escrivint.
  const adrecesCarregades = useRef(false);

  const refresca = useCallback(async () => {
    try {
      const e = await invoke<EstatVagueig>("wander_status");
      setEstat(e);
      if (!adrecesCarregades.current) {
        adrecesCarregades.current = true;
        setAdreces(e.profunda_text);
      }
    } catch (err) {
      setErr(String(err));
    }
  }, []);

  // Estat inicial + refrec cada 10 s mentre el panell és obert: el vagueig
  // treballa en segon pla i l'usuari ha de veure com avança sense reiniciar.
  useEffect(() => {
    void refresca();
    const id = window.setInterval(() => void refresca(), 10_000);
    return () => window.clearInterval(id);
  }, [refresca]);

  const commuta = async (actiu: boolean) => {
    setBusy(true);
    setErr(null);
    try {
      await invoke("wander_set", { actiu });
      setMsg(actiu ? t("settings.wanderOn") : t("settings.wanderOff"));
    } catch (e) {
      setErr(String(e));
    } finally {
      await refresca();
      setBusy(false);
    }
  };

  const aprenAra = async () => {
    setBusy(true);
    setErr(null);
    setMsg(null);
    try {
      const n = await invoke<number>("wander_run");
      setMsg(n > 0 ? t("settings.wanderRanLearned") + ": " + n : t("settings.wanderRanNothing"));
    } catch (e) {
      setErr(String(e));
    } finally {
      await refresca();
      setBusy(false);
    }
  };

  const desaProfunda = async () => {
    setBusy(true);
    setErr(null);
    try {
      const n = await invoke<number>("wander_profunda_set", { adreces });
      setMsg(t("settings.wanderDeepSaved") + ": " + n);
    } catch (e) {
      setErr(String(e));
    } finally {
      await refresca();
      setBusy(false);
    }
  };

  const ram =
    estat?.ram_procents != null ? estat.ram_procents.toFixed(1).replace(".", ",") + " %" : "—";

  return (
    <div className="setting-row setting-row-col">
      <span className="settings-section-title">
        <BookOpen size={13} /> {t("settings.wanderSection")}
      </span>
      <label className="setting-check">
        <input
          type="checkbox"
          checked={estat?.activat ?? true}
          disabled={busy}
          onChange={(e) => void commuta(e.target.checked)}
        />
        {t("settings.wanderLabel")}
      </label>
      <div className="lp-hint">{t("settings.wanderHint")}</div>

      {/* ── Què està passant ara mateix ── */}
      <div className="lp-hint">
        {estat ? (
          estat.bloqueig ? (
            <>
              <WifiOff size={11} /> {t("settings.wanderAturat")} — {estat.bloqueig}
            </>
          ) : (
            t("settings.wanderLliure")
          )
        ) : (
          <RefreshCw size={11} className="spin" />
        )}
      </div>
      <div className="lp-hint">
        {t("settings.wanderPagines")}: {estat?.pagines ?? 0} ·{" "}
        {t("settings.wanderConceptes")}: {estat?.conceptes ?? 0} · {t("settings.wanderRam")}:{" "}
        {ram} · {t("settings.wanderTor")}:{" "}
        {estat?.acces_tor ? t("settings.wanderTorYes") : t("settings.wanderTorNo")} (
        {estat?.adreces_profundes ?? 0})
      </div>

      <div className="settings-actions">
        <button className="btn sm" disabled={busy} onClick={() => void aprenAra()}>
          {busy ? <RefreshCw size={12} className="spin" /> : <BookOpen size={12} />}{" "}
          {t("settings.wanderRun")}
        </button>
        {msg && <span className="settings-msg ok">{msg}</span>}
        {err && <span className="settings-msg err">{err}</span>}
      </div>

      {/* ── Últimes lectures (amb la font i els conceptes incorporats) ── */}
      {estat && estat.historial.length > 0 && (
        <div className="wander-notes">
          {estat.historial.map((n, i) => (
            <div key={i} className="wander-note" title={n.url}>
              <span className="wander-note-font">{n.font}</span>
              <span className="wander-note-title">{n.titol}</span>
              <span className="wander-note-count">{n.conceptes}</span>
            </div>
          ))}
        </div>
      )}

      {/* ── Internet profunda: només si l'usuari hi posa punts d'entrada ── */}
      <div className="lp-hint">{t("settings.wanderDeepHint")}</div>
      <textarea
        className="wander-textarea"
        rows={3}
        placeholder="http://xxxx...onion"
        value={adreces}
        onChange={(e) => setAdreces(e.target.value)}
      />
      <div className="settings-actions">
        <button className="btn sm" disabled={busy} onClick={() => void desaProfunda()}>
          {t("settings.wanderDeepSave")}
        </button>
      </div>
    </div>
  );
}
