import { ShieldQuestion, Terminal } from "lucide-react";
import { useT } from "../../i18n";
import { useComputerStore } from "../../stores/computerStore";
import { useState } from "react";

/// Diàleg de confirmació d'una comanda de l'ordinador. Filosofia «una vegada
/// i prou»: «Recorda» ve per defecte activat — al primer SÍ d'un programa,
/// els següents (cat, python3, brew…) ja no pregunten mai més; i hi ha
/// l'opció «només aquesta sessió» per no deixar res savorat al disc.
export default function ComputerConfirmModal() {
  const { t } = useT();
  const pending = useComputerStore((s) => s.pending);
  const resolve = useComputerStore((s) => s.resolveConfirm);
  const [remember, setRemember] = useState(true);
  const [session, setSession] = useState(false);

  if (!pending) return null;

  const answer = (approved: boolean) => {
    resolve(approved, approved && remember, approved && session);
    setRemember(true); // següent peticio, mateixa filosofia: una vegada i prou
    setSession(false);
  };

  return (
    <div className="modal-overlay">
      <div className="modal cp-confirm">
        <div className="modal-head">
          <span>
            <ShieldQuestion size={15} style={{ verticalAlign: -2, marginRight: 6 }} />
            {t("computer.confirmTitle")}
          </span>
        </div>
        <div className="modal-body">
          <div className="cp-confirm-cmd">
            <Terminal size={13} />
            <code>{pending.command}</code>
          </div>
          <label className="cp-switch">
            <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} />
            <span>{t("computer.confirmRemember")}</span>
          </label>
          <label className="cp-switch">
            <input type="checkbox" checked={session} onChange={(e) => setSession(e.target.checked)} />
            <span>{t("computer.confirmSession")}</span>
          </label>
          <p className="empty-hint" style={{ margin: "6px 0 0" }}>
            {t("computer.confirmOnce")}
          </p>
        </div>
        <div className="modal-foot">
          <button className="btn" onClick={() => answer(false)}>
            {t("computer.deny")}
          </button>
          <button className="btn primary" onClick={() => answer(true)}>
            {t("computer.approve")}
          </button>
        </div>
      </div>
    </div>
  );
}
