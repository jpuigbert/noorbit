// Panell «Aprenentatge»: la IA local que APRÈN de les altres IAs i de les
// preguntes/respostes de l'usuari.
//
// Tot és local: el registre de converses i les lliçons viuen a la carpeta de
// dades de NoOrbit; la destil·lació la fa el model d'Ollama de la màquina (en
// segon pla) i, si no n'hi ha, una heurística que no necessita cap IA.
// El dataset es pot exportar en JSONL per entrenar un model local (LoRA)
// més endavant: LLaMA-Factory, Unsloth o Axolotl.
import { useEffect, useState } from "react";
import {
  Brain,
  Download,
  FolderOpen,
  GraduationCap,
  Sparkles,
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
    refresh,
    setConfig,
    distill,
    removeLesson,
    clearAll,
    exportDataset,
    watch,
  } = useLearningStore();
  const [showExchanges, setShowExchanges] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);

  useEffect(() => {
    void refresh();
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
  }, [refresh, watch]);

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
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
