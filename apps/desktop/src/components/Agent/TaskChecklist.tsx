import { useState } from "react";
import { ListChecks, Plus, Trash2, Circle, CheckCircle2 } from "lucide-react";
import { useTaskListStore } from "../../stores/taskListStore";
import { useT } from "../../i18n";

/// Checklist compartida del xat: les tasques que la IA (o l'usuari) han posat
/// a la llista. La IA l'omple amb «@todo:» i la completa amb «@done:»; s'injecta
/// al context perquè NI la principal NI els agents es perden cap tasca.
export default function TaskChecklist() {
  const { t } = useT();
  const items = useTaskListStore((s) => s.items);
  const add = useTaskListStore((s) => s.add);
  const toggle = useTaskListStore((s) => s.toggle);
  const remove = useTaskListStore((s) => s.remove);
  const clear = useTaskListStore((s) => s.clear);
  const [draft, setDraft] = useState("");

  const pendents = items.filter((i) => !i.done).length;

  const submit = () => {
    if (!draft.trim()) return;
    add(draft, "usuari");
    setDraft("");
  };

  return (
    <div className="task-checklist" style={{ borderBottom: "1px solid var(--border)", padding: "6px 10px" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: items.length ? 4 : 0 }}>
        <ListChecks size={13} style={{ color: "var(--text-3)" }} />
        <strong style={{ fontSize: 12 }}>{t("tasks.title")}</strong>
        {items.length > 0 && (
          <span style={{ fontSize: 11, color: "var(--text-3)" }}>
            {t("tasks.count", { done: items.length - pendents, total: items.length })}
          </span>
        )}
        <span style={{ flex: 1 }} />
        {items.length > 0 && (
          <button className="icon-btn" title={t("tasks.clear")} onClick={clear}>
            <Trash2 size={12} />
          </button>
        )}
      </div>

      {items.length === 0 && (
        <div style={{ fontSize: 11, color: "var(--text-3)" }}>{t("tasks.empty")}</div>
      )}

      {items.length > 0 && (
        <ul style={{ listStyle: "none", margin: 0, padding: 0, maxHeight: 140, overflowY: "auto" }}>
          {items.map((it) => (
            <li
              key={it.id}
              style={{ display: "flex", alignItems: "center", gap: 6, padding: "2px 0", fontSize: 12 }}
            >
              <button
                className="icon-btn"
                title={it.done ? t("tasks.reopen") : t("tasks.markDone")}
                onClick={() => toggle(it.id)}
                style={{ color: it.done ? "var(--success)" : "var(--text-3)" }}
              >
                {it.done ? <CheckCircle2 size={13} /> : <Circle size={13} />}
              </button>
              <span
                style={{
                  flex: 1,
                  textDecoration: it.done ? "line-through" : "none",
                  color: it.done ? "var(--text-3)" : "var(--text-1)",
                }}
              >
                {it.text}
              </span>
              {it.by === "ia" && (
                <span style={{ fontSize: 10, color: "var(--text-3)" }}>{t("tasks.byIa")}</span>
              )}
              <button className="icon-btn" title={t("tasks.remove")} onClick={() => remove(it.id)}>
                <Trash2 size={11} />
              </button>
            </li>
          ))}
        </ul>
      )}

      <div style={{ display: "flex", gap: 6, marginTop: 6 }}>
        <input
          value={draft}
          placeholder={t("tasks.addPh")}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
          }}
          style={{
            flex: 1,
            fontSize: 12,
            padding: "3px 6px",
            background: "var(--bg-1)",
            border: "1px solid var(--border)",
            borderRadius: 4,
            color: "var(--text-1)",
          }}
        />
        <button className="btn sm" onClick={submit} disabled={!draft.trim()}>
          <Plus size={12} /> {t("tasks.add")}
        </button>
      </div>
    </div>
  );
}
