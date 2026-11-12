import { useEffect, useMemo, useState } from "react";
import { Loader2, Plus, Trash2, Pencil, Users, X, Square, MessagesSquare } from "lucide-react";
import { useT } from "../../i18n";
import { useExpertStore, type Expert, type ExpertInput } from "../../stores/expertStore";
import { useAIStore } from "../../stores/aiStore";
import { useAgentStore } from "../../stores/agentStore";
import { usePreviewStore } from "../../stores/previewStore";
import { ChatMarkdown } from "../Agent/ChatMarkdown";

const EMPTY: ExpertInput = { name: "", role: "", systemPrompt: "", model: null };

/// La miniaturà del mini-chat d'un especialista: sessió «team:<id>», el
/// mateix format del xat principal (ChatMarkdown), passos al sobre i bom.
function ExpertLiveCard({ expert }: { expert: Expert }) {
  const { t } = useT();
  const sid = `team:${expert.id}`;
  const sess = useAgentStore((s) => s.sessions.find((x) => x.id === sid));
  // El selector ha de tornar SEMPRE la mateixa referència: filtrar dins del
  // selector creava un llistat nou a cada lectura i React es repiava sense fi
  // («Maximum update depth exceeded»). Es selecciona l'estat cru i es filtra
  // amb useMemo (mateix patró que BackgroundTaskBadge).
  const allProgress = useExpertStore((s) => s.teamProgress);
  const steps = useMemo(
    () => allProgress.filter((p) => p.expert === expert.name),
    [allProgress, expert.name]
  );
  const lastStep = steps[steps.length - 1];
  const teamRunning = useExpertStore((s) => s.teamRunning);

  const openInChat = () => {
    const a = useAgentStore.getState();
    a.ensureChat(sid, t("experts.liveTitle").replace("{name}", expert.name));
    a.setActive(sid);
    usePreviewStore.getState().setAgentSub("chat");
  };

  // El que mostrarem: els ÚLTIMS 2 torns del fil + el que s'està escrivint
  // ara (stream, quan la IA encara treballa).
  const visible = sess?.messages.slice(-2) ?? [];
  const streaming = (sess?.stream?.length ?? 0) > 0;
  return (
    <div className="ex-live-card">
      <div className="ex-live-head">
        <span className="ex-live-name">
          {expert.name}
          {teamRunning && streaming && (
            <Loader2 size={11} className="spin" style={{ marginLeft: 6, verticalAlign: -1 }} />
          )}
        </span>
        <span className="ex-live-status" title={lastStep?.detail}>
          {lastStep ? lastStep.detail : "—"}
        </span>
        <button
          className="icon-btn"
          title={t("experts.openChat")}
          onClick={openInChat}
        >
          <MessagesSquare size={12} />
        </button>
      </div>
      {(visible.length > 0 || streaming) && (
        <div className="ex-live-body">
          {visible.map((m) => (
            <div key={String(m.at)} className="ex-live-msg" data-role={m.role}>
              <ChatMarkdown text={m.text} />
            </div>
          ))}
          {streaming && (
            <div className="ex-live-msg" data-role="assistant">
              <ChatMarkdown text={sess!.stream!} />
            </div>
          )}
        </div>
      )}
    </div>
  );
}

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

  // CREA EQUIP RÀPID: sense haver de crear especialistes a mà, un clic
  // prepara l'equip bàsic (Arquitecte · Programador · Revisor) amb rols
  // pensats per treballar JUNTS sobre el projecte.
  const QUICK_TEAM: ExpertInput[] = [
    {
      name: "Arquitecte",
      role: "Estructura i decisions",
      systemPrompt:
        "Ets l'ARQUITECTE de l'equip. Decideixes l'estructura del projecte, quins fitxers " +
        "cal crear o canviar i per què. Respon breu i concret: llista de fitxers i decisions.",
      model: null,
    },
    {
      name: "Programador",
      role: "Escriu el codi",
      systemPrompt:
        "Ets el PROGRAMADOR de l'equip. Escrius/modifiques el codi que l'arquitecte defineix. " +
        "Emet SEMPRE cada fitxer com «@file: ruta/fitxer.ext» seguit del codi COMPLET en bloc ```.",
      model: null,
    },
    {
      name: "Revisor",
      role: "Qualitat i errors",
      systemPrompt:
        "Ets el REVISOR de l'equip. Revises el codi dels altres, detectes errors de sintaxi " +
        "o lògica i proposes la versió corregida mínima. Concís i pràctic.",
      model: null,
    },
  ];
  const quickTeam = async () => {
    for (const e of QUICK_TEAM) await s.create(e);
  };

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
          <div style={{ marginTop: 10 }}>
            <button
              className="btn sm primary"
              disabled={s.busy}
              title={t("experts.quickTeamHint")}
              onClick={() => void quickTeam()}
            >
              {s.busy && <Loader2 size={12} className="spin" />}
              <Users size={12} /> {t("experts.quickTeam")}
            </button>
          </div>
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
        {s.teamRunning && (
          <button className="btn sm" onClick={() => void s.stopTeam()}>
            <Square size={11} /> {t("experts.teamStop")}
          </button>
        )}
        {picked.length > 0 && (
          <div className="ex-team-picked">
            {t("experts.workingWith")}{" "}
            {picked
              .map((id) => s.experts.find((e) => e.id === id)?.name)
              .filter(Boolean)
              .join(", ")}
          </div>
        )}

        {/* EN DIRECTE: el que cada especialista està escrivint ara mateix,
            amb el mateix format del xat principal (sessió «team:<id>»).
            Es veu mentre corre l'equip i fins que es tanca el panell. */}
        {(s.teamRunning || s.teamProgress.length > 0) && (
          <div className="ex-live">
            {[...(picked.length > 0 ? picked : s.experts.map((e) => e.id))]
              .map((id) => s.experts.find((e) => e.id === id))
              .filter((e): e is Expert => Boolean(e))
              .map((e) => (
                <ExpertLiveCard key={e.id} expert={e} />
              ))}
          </div>
        )}

        {s.teamProgress.length > 0 && (
          <div className="ex-progress">
            <div className="ex-progress-row" style={{ opacity: 0.7 }}>
              {s.teamRunning
                ? t("experts.teamElapsed").replace(
                    "{s}",
                    String(
                      Math.max(
                        0,
                        Math.round(
                          ((s.teamProgress[s.teamProgress.length - 1]?.elapsedMs ?? 0) ||
                            (s.teamStartedAt ? Date.now() - s.teamStartedAt : 0)) / 1000
                        )
                      )
                    )
                  )
                : t("experts.teamFinished")}
            </div>
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
