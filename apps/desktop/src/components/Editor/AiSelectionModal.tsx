import { useState } from "react";
import { Sparkles, X } from "lucide-react";
import { useT } from "../../i18n";

/// Petit diàleg «Edita la selecció amb la IA»: input d'instrucció amb
/// suggeriments d'un clic. El resultat s'aplicapel flux de revisió (diff
/// verd/vermell a l'editor + Accepta/Rebutja), igual que la resta de les
/// edicions de la IA de NoOrbit.
export default function AiSelectionModal(props: {
  selected: string;
  empty: boolean;
  fileName: string;
  busy: boolean;
  error: string | null;
  onApply: (instruction: string) => void;
  onClose: () => void;
}) {
  const { t } = useT();
  const [text, setText] = useState("");
  const chips: string[] = [
    t("aiEdit.chipFix"),
    t("aiEdit.chipRefactor"),
    t("aiEdit.chipDocs"),
    t("aiEdit.chipExplain"),
  ];

  if (props.empty) {
    return (
      <div className="modal-overlay">
        <div className="modal" style={{ maxWidth: 380 }}>
          <div className="modal-head">
            <span>
              <Sparkles size={15} style={{ verticalAlign: -2, marginRight: 6 }} />
              {t("aiEdit.title")}
            </span>
            <button className="icon-btn" onClick={props.onClose}>
              <X size={16} />
            </button>
          </div>
          <div className="modal-body">
            <p className="empty-hint" style={{ margin: 0 }}>{t("aiEdit.noSelection")}</p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="modal-overlay">
      <div className="modal" style={{ maxWidth: 560 }}>
        <div className="modal-head">
          <span>
            <Sparkles size={15} style={{ verticalAlign: -2, marginRight: 6 }} />
            {t("aiEdit.title")} · {props.fileName}
          </span>
          <button className="icon-btn" onClick={props.onClose}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body">
          <table
            style={{
              width: "100%",
              marginBottom: 10,
              borderCollapse: "collapse",
              fontFamily: "var(--font-mono)",
              fontSize: 11,
              color: "var(--text-1)",
            }}
          >
            <tbody>
              <tr>
                <td style={{ verticalAlign: "top", whiteSpace: "pre-wrap", maxHeight: 130, overflow: "hidden" }}>
                  <div
                    style={{
                      maxHeight: 130,
                      overflow: "auto",
                      padding: 8,
                      background: "var(--bg-2)",
                      border: "1px solid var(--border)",
                      borderRadius: "var(--radius)",
                      whiteSpace: "pre-wrap",
                    }}
                  >
                    {props.selected.length > 1200
                      ? props.selected.slice(0, 1200) + "\n…"
                      : props.selected}
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
          <div className="prov-chips" style={{ marginBottom: 8 }}>
            {chips.map((c) => (
              <button key={c} className="btn sm" onClick={() => setText(c)}>
                {c}
              </button>
            ))}
          </div>
          <input
            style={{ width: "100%" }}
            value={text}
            autoFocus
            placeholder={t("aiEdit.placeholder")}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && text.trim() && !props.busy) props.onApply(text.trim());
            }}
          />
          {props.error && (
            <p className="empty-hint" style={{ color: "var(--error)", margin: "8px 0 0" }}>
              ✗ {props.error}
            </p>
          )}
        </div>
        <div className="modal-foot">
          <button className="btn" onClick={props.onClose}>
            {t("aiEdit.cancel")}
          </button>
          <button
            className="btn primary"
            disabled={props.busy || !text.trim()}
            onClick={() => props.onApply(text.trim())}
          >
            <Sparkles size={13} /> {props.busy ? t("aiEdit.working") : t("aiEdit.apply")}
          </button>
        </div>
      </div>
    </div>
  );
}
