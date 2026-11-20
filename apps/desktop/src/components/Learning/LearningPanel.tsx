// Panell «Aprenentatge»: la IA local que APRÈN de les altres IAs i de les
// preguntes/respostes de l'usuari.
//
// Tot és local: el registre de converses i les lliçons viuen a la carpeta de
// dades de NoOrbit; la destil·lació la fa el model d'Ollama de la màquina (en
// segon pla) i, si no n'hi ha, una heurística que no necessita cap IA.
// El dataset es pot exportar en JSONL per entrenar un model local (LoRA)
// més endavant: LLaMA-Factory, Unsloth o Axolotl.
//
// DES DE 0.71.12 el panell també mostra les dues peces que venen de Soup
// (Apache-2.0) i que són el que fa que aquest registre valgui alguna cosa:
//  · els VOTS (👍/👎) — sense ells el dataset és sorra, no aprenentatge;
//  · la PORTA d'avaluació — abans de quedar-se un coneixement nou es
//    comprova si empitjora el que l'usuari ja havia aprovat.
import { useEffect, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import {
  Brain,
  Database,
  Download,
  FolderOpen,
  GraduationCap,
  ShieldCheck,
  Sparkles,
  ThumbsDown,
  ThumbsUp,
  Trash2,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../../i18n";
import { useLearningStore } from "../../stores/learningStore";

function when(at: number): string {
  try {
    return new Date(at * 1000).toLocaleString();
  } catch {
    return "";
  }
}

export default function LearningPanel() {
  const { t } = useT();
  const {
    status,
    lessons,
    exchanges,
    busy,
    error,
    notice,
    soup,
    verdict,
    refresh,
    setConfig,
    distill,
    removeLesson,
    clearAll,
    exportDataset,
    exportSoup,
    refreshVerdict,
    vote,
    watch,
  } = useLearningStore(
    useShallow((st) => ({
      status: st.status,
      lessons: st.lessons,
      exchanges: st.exchanges,
      busy: st.busy,
      error: st.error,
      notice: st.notice,
      soup: st.soup,
      verdict: st.verdict,
      refresh: st.refresh,
      setConfig: st.setConfig,
      distill: st.distill,
      removeLesson: st.removeLesson,
      clearAll: st.clearAll,
      exportDataset: st.exportDataset,
      exportSoup: st.exportSoup,
      refreshVerdict: st.refreshVerdict,
      vote: st.vote,
      watch: st.watch,
    }))
  );
  const [showExchanges, setShowExchanges] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);

  useEffect(() => {
    void refresh();
    void refreshVerdict();
    let cancelled = false;
    let un: (() => void) | null = null;
    void watch().then((fn) => {
      if (cancelled) fn();
      else un = fn;
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, [refresh, refreshVerdict, watch]);

  const stats = status?.stats;

  return (
    <div className="learning-panel">
      <div className="learning-head">
        <Brain size={14} />
        <span>{t("learning.title")}</span>
        {status?.enabled && <i className="learning-dot" title={t("learning.on")} />}
        {status?.stats.distilling && (
          <em className="learning-working">
            <Sparkles size={11} /> {t("learning.working")}
          </em>
        )}
      </div>

      {/* ── Interruptor principal ─────────────────────────────────────── */}
      <div className="learning-switch">
        <button
          className={"learning-switch-btn" + (status?.enabled ? " on" : "")}
          onClick={() => void setConfig({ enabled: !status?.enabled })}
          disabled={busy}
        >
          <span className="learning-knob" />
        </button>
        <div className="learning-switch-text">
          <strong>{t("learning.enable")}</strong>
          <span>{t("learning.enableHint")}</span>
        </div>
      </div>

      {/* ── Comptadors ───────────────────────────────────────────────── */}
      <div className="learning-stats">
        <div className="learning-stat">
          <b>{stats?.exchanges ?? 0}</b>
          <span>{t("learning.statExchanges")}</span>
        </div>
        <div className="learning-stat">
          <b>{stats?.lessons ?? 0}</b>
          <span>{t("learning.statLessons")}</span>
        </div>
        <div className="learning-stat">
          <b>{stats?.pending ?? 0}</b>
          <span>{t("learning.statPending")}</span>
        </div>
        <div className="learning-stat">
          <b>
            <ThumbsUp size={13} style={{ verticalAlign: -2 }} /> {stats?.votedUp ?? 0}
          </b>
          <span>{t("learning.statUp")}</span>
        </div>
        <div className="learning-stat">
          <b>
            <ThumbsDown size={13} style={{ verticalAlign: -2 }} /> {stats?.votedDown ?? 0}
          </b>
          <span>{t("learning.statDown")}</span>
        </div>
      </div>

      {/* ── Opccions ─────────────────────────────────────────────────── */}
      {status?.enabled && (
        <>
          <label className="learning-check">
            <input
              type="checkbox"
              checked={status.inject}
              onChange={(e) => void setConfig({ inject: e.target.checked })}
            />
            <span>{t("learning.inject")}</span>
          </label>
          <label className="learning-check">
            <input
              type="checkbox"
              checked={status.autoDistill}
              onChange={(e) => void setConfig({ autoDistill: e.target.checked })}
            />
            <span>{t("learning.autoDistill")}</span>
          </label>
        </>
      )}

      {/* ── Accions ──────────────────────────────────────────────────── */}
      <div className="learning-actions">
        <button className="btn sm primary" onClick={() => void distill()} disabled={busy}>
          <GraduationCap size={12} /> {t("learning.distill")}
        </button>
        <button className="btn sm" onClick={() => void exportDataset()} disabled={busy}>
          <Download size={12} /> {t("learning.export")}
        </button>
        {status?.datasetPath && (
          <button
            className="btn sm"
            title={t("learning.reveal")}
            onClick={() => {
              const dir = status.datasetPath.replace(/[\\/][^\\/]+$/, "");
              void invoke("reveal_in_finder", { path: dir }).catch(() => undefined);
            }}
          >
            <FolderOpen size={12} /> {t("learning.revealShort")}
          </button>
        )}
        <button
          className={"btn sm" + (confirmClear ? " danger" : "")}
          onClick={() => {
            // Dos passos: primer pregunta, segon esborra (cap diàleg del
            // sistema: el confirm del navegador no sempre hi és).
            if (!confirmClear) {
              setConfirmClear(true);
              return;
            }
            setConfirmClear(false);
            void clearAll();
          }}
          onBlur={() => setConfirmClear(false)}
          disabled={busy}
        >
          <Trash2 size={12} /> {confirmClear ? t("learning.confirmClearBtn") : t("learning.clear")}
        </button>
      </div>

      {!status?.localModel && (
        <p className="learning-note">
          {t("learning.noModel")}
        </p>
      )}
      {notice && <p className="learning-notice">{notice}</p>}
      {error && <p className="learning-error">{error}</p>}

      {/* ── Bucle de dades: el paquet que entén Soup ────────────────── */}
      {/* No instal·la ni entrena res: escriu els fitxers als formats de Soup
          i diu què falta (vots, files, fugues de validació) i si aquest
          equip hi pot entrenar — es comprova, no es suposa. */}
      <div className="learning-section-title">
        <Database size={12} /> {t("learning.soupTitle")}
      </div>
      <div className="learning-card">
        <div className="learning-card-title">
          <span>{soup ? t("learning.soupWritten") : t("learning.soupNever")}</span>
          <button className="btn sm" onClick={() => void exportSoup()} disabled={busy}>
            <Download size={12} /> {t("learning.soupExport")}
          </button>
        </div>
        {soup && (
          <>
            <div className="learning-score">
              <b>{Math.round(soup.quality * 100)}%</b>
              <span className="learning-score-bar">
                <i style={{ width: `${Math.max(2, Math.round(soup.quality * 100))}%` }} />
              </span>
              <span className="learning-source">{t("learning.soupQuality")}</span>
            </div>
            <div className="learning-kv">
              <span>
                {t("learning.soupRows")}: <b>{soup.rowsTrain}</b>
              </span>
              <span>
                {t("learning.soupPreference")}: <b>{soup.rowsPreference}</b>
              </span>
              <span>
                {t("learning.soupVotes")}: <b>{soup.votesUp}</b> 👍 / <b>{soup.votesDown}</b> 👎
              </span>
              <span>
                {t("learning.soupDuplicates")}: <b>{soup.duplicates}</b>
              </span>
              <span>
                {t("learning.soupLeakage")}: <b>{soup.leakage}</b>
              </span>
              <span>
                {t("learning.soupBase")}: <b>{soup.baseModel}</b>
              </span>
            </div>
            {soup.scorecard.length > 0 && (
              <ul className="learning-list">
                {soup.scorecard.map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
            )}
            <p
              className={"learning-note " + (soup.canTrainHere ? "learning-ok" : "learning-warn")}
            >
              {soup.trainCheck}
            </p>
            <p className="learning-path">{soup.trainPath}</p>
          </>
        )}
      </div>

      {/* ── Porta d'avaluació del cervell ─────────────────────────── */}
      {/* El veredicte de l'ÚLTIMA fusió d'aquesta sessió. Sense proves (cap
          👍) la porta diu que NO ha pogut avaluar: no fingeix un 100%. */}
      <div className="learning-section-title">
        <ShieldCheck size={12} /> {t("learning.gateTitle")}
      </div>
      <div className="learning-card">
        {verdict ? (
          <>
            <div className="learning-card-title">
              <span
                className={
                  verdict.rollback ? "learning-warn" : verdict.acceptat ? "learning-ok" : ""
                }
              >
                {verdict.rollback ? t("learning.gateRollback") : t("learning.gateKept")}
              </span>
              <span className="learning-source">{when(verdict.at)}</span>
            </div>
            <div className="learning-kv">
              <span>
                {t("learning.gatePieces")}: <b>{verdict.peces}</b>
              </span>
              <span>
                {t("learning.gateProofs")}: <b>{verdict.proves}</b>
              </span>
              {verdict.abans && (
                <span>
                  {t("learning.gateBefore")}: <b>{verdict.abans.encertades}</b>/
                  {verdict.abans.proves}
                </span>
              )}
              {verdict.despres && (
                <span>
                  {t("learning.gateAfter")}: <b>{verdict.despres.encertades}</b>/
                  {verdict.despres.proves}
                </span>
              )}
            </div>
            <p className="learning-note">{verdict.motiu}</p>
          </>
        ) : (
          <p className="learning-note">{t("learning.gateNone")}</p>
        )}
      </div>

      {/* ── Lliçons apreses ──────────────────────────────────────────── */}
      <div className="learning-section-title">
        <Sparkles size={12} /> {t("learning.lessons")}
      </div>
      {lessons.length === 0 ? (
        <p className="learning-empty">{t("learning.noLessons")}</p>
      ) : (
        <div className="learning-lessons">
          {lessons.map((l) => (
            <div key={l.id} className="learning-lesson">
              <div className="learning-lesson-text">{l.text}</div>
              <div className="learning-lesson-meta">
                {l.tags.map((tg) => (
                  <span key={tg} className="learning-tag">
                    {tg}
                  </span>
                ))}
                <span className="learning-source">{l.source}</span>
                <button
                  className="icon-btn"
                  title={t("learning.deleteLesson")}
                  onClick={() => void removeLesson(l.id)}
                >
                  <Trash2 size={11} />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* ── Registre de converses ───────────────────────────────────── */}
      <button
        className="learning-section-title learning-toggle"
        onClick={() => setShowExchanges((v) => !v)}
      >
        {t("learning.exchanges")} ({exchanges.length})
      </button>
      {showExchanges && (
        <div className="learning-lessons">
          {exchanges.length === 0 && <p className="learning-empty">{t("learning.noExchanges")}</p>}
          {exchanges.map((e) => (
            <div key={e.id} className="learning-exchange">
              <div className="learning-q">{e.question}</div>
              <div className="learning-a">{e.answer}</div>
              <div className="learning-lesson-meta">
                <span className="learning-tag">
                  {e.provider}
                  {e.model ? `/${e.model}` : ""}
                </span>
                <span className="learning-source">{when(e.at)}</span>
                {/* Votar des del registre: el mateix que fa el xat, però per
                    revisar el que ja es va dir. El 👍 val com a prova i com a
                    fila de dataset; el 👎 exclou el torn de tot. */}
                <span className="learning-vote">
                  <button
                    className={"learning-vote-btn up" + (e.vote === 1 ? " on" : "")}
                    title={t("learning.voteUp")}
                    aria-pressed={e.vote === 1}
                    onClick={() => void vote(e.question, e.vote === 1 ? 0 : 1)}
                  >
                    <ThumbsUp size={11} />
                  </button>
                  <button
                    className={"learning-vote-btn down" + (e.vote === -1 ? " on" : "")}
                    title={t("learning.voteDown")}
                    aria-pressed={e.vote === -1}
                    onClick={() => void vote(e.question, e.vote === -1 ? 0 : -1)}
                  >
                    <ThumbsDown size={11} />
                  </button>
                </span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
