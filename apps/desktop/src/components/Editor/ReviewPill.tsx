// Pastilla global de revisió d'edicions de la IA.
//
// Quan la IA toca fitxers que NO són els que tens oberts (o n'ha tocat varios
// de cop), cal un lloc on veure-ho i decidir-ho: aquesta pastilla, enganxada a
// la barra d'estat, resumix els canvis pendents, fa el compte enrere fins que
// s'accepten sols (35 s) i permet «Accepta tot» / «Rebutja tot» o saltar a
// cada fitxer per mirar-ne el diff (verd = nou, roig = vell).

import { useEffect, useState } from "react";
import { Check, Sparkles, Undo2, X } from "lucide-react";
import { useT } from "../../i18n";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import {
  AUTO_ACCEPT_MS,
  resolveAllMarks,
  useEditReviewStore,
} from "../../stores/editReviewStore";

export default function ReviewPill() {
  const { t } = useT();
  const marks = useEditReviewStore((s) => s.marks);
  const loadFile = useWorkspaceStore((s) => s.loadFile);
  const pending = marks.filter((m) => !m.streaming);
  const [left, setLeft] = useState<number | null>(null);
  const [minimized, setMinimized] = useState(false);

  // El compte enrere que es mostra és el del canvi més antic (el que abans
  // s'acceptarà sol), que és també el que l'usuari ha de poder revisar primer.
  useEffect(() => {
    if (pending.length === 0) {
      setLeft(null);
      return;
    }
    const tick = () => {
      const oldest = Math.min(...pending.map((m) => m.armedAt));
      const ms = AUTO_ACCEPT_MS - (Date.now() - oldest);
      setLeft(ms > 0 ? Math.ceil(ms / 1000) : 0);
    };
    tick();
    const id = setInterval(tick, 500);
    return () => clearInterval(id);
  }, [pending.length, pending.map((m) => m.armedAt).join(",")]);

  if (pending.length === 0) return null;

  if (minimized) {
    return (
      <button
        className="review-pill minimized"
        title={t("editor.reviewShow")}
        onClick={() => setMinimized(false)}
      >
        <Sparkles size={12} /> {pending.length}
      </button>
    );
  }

  return (
    <div className="review-pill">
      <div className="review-pill-head">
        <Sparkles size={13} className="review-spark" />
        <span className="review-pill-title">
          {t("editor.reviewPending", { n: pending.length })}
        </span>
        {left !== null && <b className="review-count">{t("editor.reviewAuto", { n: left })}</b>}
        <div style={{ flex: 1 }} />
        <button
          className="icon-btn"
          style={{ width: 16, height: 16 }}
          title={t("editor.reviewMinimize")}
          onClick={() => setMinimized(true)}
        >
          <X size={11} />
        </button>
      </div>
      <div className="review-pill-files">
        {pending.slice(0, 6).map((m) => (
          <button
            key={m.path}
            className="review-pill-file"
            title={m.path}
            onClick={() => void loadFile(m.path)}
          >
            {m.name}
          </button>
        ))}
        {pending.length > 6 && <span className="review-pill-more">+{pending.length - 6}</span>}
      </div>
      <div className="review-pill-actions">
        <button className="btn sm review-accept" onClick={() => void resolveAllMarks("accept")}>
          <Check size={12} /> {t("editor.reviewAcceptAll")}
        </button>
        <button className="btn sm review-reject" onClick={() => void resolveAllMarks("reject")}>
          <Undo2 size={12} /> {t("editor.reviewRejectAll")}
        </button>
      </div>
    </div>
  );
}
