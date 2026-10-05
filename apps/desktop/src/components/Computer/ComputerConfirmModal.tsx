import { ShieldQuestion, Terminal } from "lucide-react";
import { useT } from "../../i18n";
import { useComputerStore } from "../../stores/computerStore";
import { useState } from "react";

export default function ComputerConfirmModal() {
  const { t } = useT();
  const pending = useComputerStore((s) => s.pending);
  const resolve = useComputerStore((s) => s.resolveConfirm);
  const [remember, setRemember] = useState(false);

  if (!pending) return null;

  const answer = (approved: boolean) => {
    resolve(approved, approved && remember);
    setRemember(false);
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
