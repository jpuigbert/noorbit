import { useEffect, useState } from "react";
import {
  ListChecks,
  Plus,
  Trash2,
  Circle,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  RefreshCw,
} from "lucide-react";
import { useTaskListStore } from "../../stores/taskListStore";
import { useT } from "../../i18n";

/// Checklist compartida del xat: les tasques que la IA (o l'usuari) han posat
/// a la llista. La IA l'omple amb «@todo:» i la completa amb «@done:»; s'injecta
/// al context perquè NI la principal NI els agents es perdan cap tasca.
///
/// L'usuari pot AMAGAR-LA (un botó ▲ al títol): queda una franja prima amb el
/// comptador. La IA el recorda (localStorage) entre sessions. Quan la IA treballa,
/// el panell mostra EN VIU quina tasca té ENTRE MANS («@doing:» → «~ En curs»)
/// i l'últim pas de progrés rebut del motor.
export default function TaskChecklist({
  running = false,
  liveStep = null,
}: {
  running?: boolean;
  liveStep?: { label: string; detail: string } | null;
}) {
  const { t } = useT();
  const items = useTaskListStore((s) => s.items);
  const add = useTaskListStore((s) => s.add);
  const toggle = useTaskListStore((s) => s.toggle);
  const remove = useTaskListStore((s) => s.remove);
  const clear = useTaskListStore((s) => s.clear);
  const [draft, setDraft] = useState("");
  // Amagat/mostrat: recordat a localStorage («noorbit.tasks.hidden») —
  // preferència de la UI, no una dada del projecte (igual per a tots els calaixos).
  // Si encara no hi ha cap preferència i la llista és buida, es mostra només
  // la franja prima: així el xat no perd 90 px en una llista que no diu res.
  const [hidden, setHidden] = useState(() => {
    try {
      const pref = localStorage.getItem("noorbit.tasks.hidden");
      if (pref !== null) return pref === "1";
    } catch {
      /* sense persistència */
    }
    return items.length === 0;
  });
  // Quan la IA afig la primera tasca, la llista s'obri sola (si l'usuari no
  // ha triat explícitament amagar-la).
  useEffect(() => {
    try {
      if (localStorage.getItem("noorbit.tasks.hidden") !== null) return;
    } catch {
      return;
    }
    if (items.length > 0) setHidden(false);
  }, [items.length]);

  const pendents = items.filter((i) => !i.done).length;
  const doingItem = items.find((i) => i.doing && !i.done);

  const submit = () => {
    if (!draft.trim()) return;
    add(draft, "usuari");
    setDraft("");
  };

  const toggleHidden = () => {
    setHidden((h) => {
      try {
        localStorage.setItem("noorbit.tasks.hidden", h ? "0" : "1");
      } catch {
        /* sense persistència */
      }
      return !h;
    });
  };

  // ── AMAGADA: franja prima amb el comptador i el que fa la IA ────────────
  if (hidden) {
    return (
      <div
        className="task-checklist task-checklist-collapsed"
        style={{
          borderBottom: "1px solid var(--border)",
          padding: "3px 10px",
          display: "flex",
          alignItems: "center",
          gap: 6,
          cursor: "pointer",
        }}
        onClick={toggleHidden}
        title={t("tasks.show")}
      >
        <ListChecks size={12} style={{ color: "var(--text-3)" }} />
        <strong style={{ fontSize: 11 }}>{t("tasks.title")}</strong>
        {items.length > 0 && (
          <span style={{ fontSize: 11, color: "var(--text-3)" }}>
            {t("tasks.count", { done: items.length - pendents, total: items.length })}
          </span>
        )}
        {doingItem && (
          <span
            style={{
              fontSize: 11,
              color: "var(--accent, var(--text-1))",
              display: "flex",
              alignItems: "center",
              gap: 4,
            }}
          >
            <RefreshCw size={10} className="spin" /> {doingItem.text}
          </span>
        )}
        <span style={{ flex: 1 }} />
        <ChevronDown size={13} style={{ color: "var(--text-3)" }} />
      </div>
    );
  }

  // ── MOSTRADA: títol + tasques (+ «en curs») + camp d'afegir ─────────────
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
        {doingItem && (
          <span style={{ fontSize: 11, color: "var(--accent, var(--text-1))" }}>⏳ {t("tasks.doing")}</span>
        )}
        <span style={{ flex: 1 }} />
        {items.length > 0 && (
          <button className="icon-btn" title={t("tasks.clear")} onClick={clear}>
            <Trash2 size={12} />
          </button>
        )}
        <button className="icon-btn" title={t("tasks.hide")} onClick={toggleHidden}>
          <ChevronUp size={13} />
        </button>
      </div>

      {/* EL QUE FA LA IA EN VIU: la tasca marcada «@doing:» i l'últim pas de
          progrés del motor (la IA escrivint, executant ordres, generant…). */}
      {(doingItem || (running && liveStep)) && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            fontSize: 11,
            color: "var(--text-2, var(--text-1))",
            background: "var(--bg-1)",
            border: "1px solid var(--border)",
            borderRadius: 4,
            padding: "3px 6px",
            marginBottom: 6,
          }}
        >
          <RefreshCw size={11} className="spin" style={{ flexShrink: 0 }} />
          <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {doingItem ? `${t("tasks.doingLive")}: ${doingItem.text}` : `${liveStep?.label}: ${liveStep?.detail}`}
          </span>
        </div>
      )}

      {items.length === 0 && (
        <div style={{ fontSize: 11, color: "var(--text-3)" }}>{t("tasks.empty")}</div>
      )}

      {items.length > 0 && (
        <ul style={{ listStyle: "none", margin: 0, padding: 0, maxHeight: 140, overflowY: "auto" }}>
          {items.map((it) => (
            <li
              key={it.id}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "2px 0",
                fontSize: 12,
                // La tasca EN CURS es destaca: és el que la IA té entre mans.
                background: it.doing && !it.done ? "var(--bg-1)" : "transparent",
                borderRadius: 4,
              }}
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
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {it.doing && !it.done && <RefreshCw size={10} className="spin" style={{ marginRight: 4 }} />}
                {it.text}
              </span>
              {it.doing && !it.done && (
                <span style={{ fontSize: 10, color: "var(--accent, var(--text-2))" }}>{t("tasks.doing")}</span>
              )}
              {it.by === "ia" && !it.doing && (
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
