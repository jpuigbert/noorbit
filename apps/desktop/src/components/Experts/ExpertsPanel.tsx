import { useEffect, useState } from "react";
import { Loader2, Plus, Trash2, Pencil, Users, X } from "lucide-react";
import { useT } from "../../i18n";
import { useExpertStore, type ExpertInput } from "../../stores/expertStore";
import { useAIStore } from "../../stores/aiStore";

const EMPTY: ExpertInput = { name: "", role: "", systemPrompt: "", model: null };

/// Panell d'especialistes: l'usuari crea agents d'IA amb rol i model propis,
/// i pot fer-los treballar junts en paral·lel (treball múltiple).
export default function ExpertsPanel() {
  const { t } = useT();
  const s = useExpertStore();
  const { models } = useAIStore();
  const [form, setForm] = useState<ExpertInput | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [goal, setGoal] = useState("");
  const [picked, setPicked] = useState<string[]>([]);

  useEffect(() => {
    s.setupEvents();
    s.load();
    useAIStore.getState().loadModels();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const startCreate = () => {
    setEditingId(null);
    setForm({ ...EMPTY });
  };

  // Si s'ha demanat crear un especialista des d'un altre panell (p. ex. el
  // xat de l'Agent), obrim el formulari en arribar.
  useEffect(() => {
    if (s.createRequested) {
      s.clearCreateRequest();
      startCreate();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [s.createRequested]);
  const startEdit = (id: string) => {
    const e = s.experts.find((x) => x.id === id);
    if (!e) return;
    setEditingId(id);
    setForm({ name: e.name, role: e.role, systemPrompt: e.systemPrompt, model: e.model });
  };
  const submitForm = async () => {
    if (!form || !form.name.trim()) return;
    if (editingId) await s.update(editingId, form);
    else await s.create(form);
    setForm(null);
    setEditingId(null);
  };

  const togglePick = (id: string) =>
    setPicked((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p, id]));

  const runTeam = () => {
    void s.runTeam(goal, picked.length > 0 ? picked : s.experts.map((e) => e.id));
  };

  return (
    <div className="ex-panel">
      <div className="ex-head">
        <span>{t("experts.intro")}</span>
        <button className="btn sm primary" onClick={startCreate}>
          <Plus size={12} /> {t("experts.new")}
        </button>
      </div>

      {form && (
        <div className="ex-form">
          <div className="ex-form-title">
            {editingId ? t("experts.editTitle") : t("experts.createTitle")}
            <button className="icon-btn" onClick={() => setForm(null)}>
              <X size={13} />
            </button>
          </div>
          <input
            className="ex-input"
            placeholder={t("experts.namePh")}
            value={form.name}
            onChange={(e) => setForm({ ...form, name: e.target.value })}
          />
          <input
            className="ex-input"
            placeholder={t("experts.rolePh")}
            value={form.role}
            onChange={(e) => setForm({ ...form, role: e.target.value })}
          />
          <textarea
            className="ex-textarea"
            placeholder={t("experts.promptPh")}
            value={form.systemPrompt}
            onChange={(e) => setForm({ ...form, systemPrompt: e.target.value })}
          />
          <select
            className="ex-input"
            value={form.model ?? ""}
            onChange={(e) => setForm({ ...form, model: e.target.value || null })}
          >
            <option value="">{t("experts.useActiveModel")}</option>
            {models.map((m) => (
              <option key={m.name} value={m.name}>
                {m.name}
              </option>
            ))}
          </select>
          <button
            className="btn sm primary"
            disabled={!form.name.trim() || s.busy}
            onClick={submitForm}
          >
            {s.busy && <Loader2 size={12} className="spin" />} {t("experts.save")}
          </button>
        </div>
      )}

      {s.experts.length === 0 && !form && (
        <div className="empty-hint" style={{ padding: 12 }}>
          {t("experts.empty")}
        </div>
      )}

      <div className="ex-list">
        {s.experts.map((e) => (
          <div key={e.id} className="ex-card">
            <label className="ex-pick" title={t("experts.pickForTeam")}>
              <input
                type="checkbox"
                checked={picked.includes(e.id)}
                onChange={() => togglePick(e.id)}
              />
            </label>
            <div className="ex-card-main">
              <div className="ex-card-name">{e.name}</div>
              <div className="ex-card-role">
                {e.role}
                {e.model && <span className="ex-model-chip">{e.model}</span>}
              </div>
            </div>
            <button className="icon-btn" title={t("common.edit")} onClick={() => startEdit(e.id)}>
              <Pencil size={13} />
            </button>
            <button
              className="icon-btn"
              title={t("common.delete")}
              onClick={() => s.remove(e.id)}
            >
              <Trash2 size={13} />
            </button>
          </div>
        ))}
      </div>

      {/* Treball múltiple en equip */}
      <div className="ex-team">
        <div className="ex-team-title">
          <Users size={13} /> {t("experts.teamTitle")}
        </div>
        <div className="ex-team-hint">{t("experts.teamHint")}</div>
        <textarea
          className="ex-textarea"
          placeholder={t("experts.goalPh")}
          value={goal}
          onChange={(e) => setGoal(e.target.value)}
        />
        <button
          className="btn sm primary"
          disabled={s.teamRunning || !goal.trim() || s.experts.length === 0}
          onClick={runTeam}
        >
          {s.teamRunning && <Loader2 size={12} className="spin" />} {t("experts.teamRun")}
        </button>
        {picked.length > 0 && (
          <div className="ex-team-picked">
            {t("experts.workingWith")}{" "}
            {picked
              .map((id) => s.experts.find((e) => e.id === id)?.name)
              .filter(Boolean)
              .join(", ")}
          </div>
        )}

        {s.teamProgress.length > 0 && (
          <div className="ex-progress">
            {s.teamProgress.map((p, i) => (
              <div key={i} className="ex-progress-row">
                {p.expert ? `${p.expert}: ${p.detail}` : p.detail}
              </div>
            ))}
          </div>
        )}

        {s.teamResult && (
          <div className="ex-results">
            {s.teamResult.subtasks.map((st) => (
              <details key={st.expertId + st.task} className="ex-sub" open={!st.ok}>
                <summary className={st.ok ? "ok" : "fail"}>
                  {st.expertName} — {st.task}
                </summary>
                <pre className="ex-sub-body">{st.result}</pre>
              </details>
            ))}
            <div className="ex-answer">
              <div className="ex-answer-title">{t("experts.finalAnswer")}</div>
              <pre className="ex-sub-body">{s.teamResult.answer}</pre>
            </div>
          </div>
        )}
      </div>

      {s.error && <div className="si-error">{s.error}</div>}
    </div>
  );
}
