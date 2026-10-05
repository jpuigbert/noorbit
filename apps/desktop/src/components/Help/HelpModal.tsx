import { X, BookOpen } from "lucide-react";
import { useI18n } from "../../i18n/store";
import { useT } from "../../i18n";
import { getManual } from "../../help";
import { useUIStore } from "../../stores/uiStore";

export default function HelpModal() {
  const show = useUIStore((s) => s.showHelp);
  const setShow = useUIStore((s) => s.setShowHelp);
  const locale = useI18n((s) => s.locale);
  const { t } = useT();

  if (!show) return null;
  const manual = getManual(locale);

  return (
    <div className="modal-overlay" onClick={() => setShow(false)}>
      <div className="modal help-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>
            <BookOpen size={15} style={{ marginRight: 6, verticalAlign: -2 }} />
            {manual.title}
          </span>
          <button className="icon-btn" onClick={() => setShow(false)}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body help-body">
          <p className="help-intro">{manual.intro}</p>
          {manual.sections.map((sec, i) => (
            <section key={i} className="help-section">
              <h3>{sec.title}</h3>
              {sec.intro && <p className="help-sec-intro">{sec.intro}</p>}
              <ul>
                {sec.items.map((it, j) =>
                  // Les línies que comencen per «—» són caps de categoria:
                  // es mostren com a subtítols, no com a elements de la llista.
                  it.startsWith("—") ? (
                    <div key={j} className="help-subhead">
                      {it.replace(/^—\s*/, "").replace(/\s*—$/, "")}
                    </div>
                  ) : (
                    <li key={j}>{it}</li>
                  )
                )}
              </ul>
            </section>
          ))}
          <p className="help-foot">
            {t("help.footer")} {manual.language}.
          </p>
        </div>
      </div>
    </div>
  );
}
