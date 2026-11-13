import Editor, { DiffEditor } from "@monaco-editor/react";
import { useEffect, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { invoke } from "@tauri-apps/api/core";
import { X, ExternalLink, FolderOpen, ImageOff, Check, Undo2, Sparkles } from "lucide-react";
import { useT } from "../../i18n";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import {
  AUTO_ACCEPT_MS,
  dropMarkOnManualSave,
  resolveMark,
  noteFileEdited,
  useEditReviewStore,
  type EditMark,
} from "../../stores/editReviewStore";
import { useUIStore } from "../../stores/uiStore";
import { bindEditor, monacoThemeFor } from "../../editor/editorApi";
import { validateCurrentFile } from "../../editor/diagnostics";
import { registerQuickFixes, aiQuickFix } from "../../editor/quickFixes";
import {
  registerInlineComplete,
  autocompleteEnabled,
  toggleAutocomplete,
} from "../../editor/inlineComplete";
import { aiReplaceSelection } from "../../editor/aiSelection";
import { useMonacoReady } from "../../editor/monReady";
import { ArtEmpty } from "../Illustrations";
import { getEditor } from "../../editor/editorApi";
import AiSelectionModal from "./AiSelectionModal";

const EXT_LANG: Record<string, string> = {
  ts: "typescript",
  tsx: "typescript",
  js: "javascript",
  jsx: "javascript",
  json: "json",
  py: "python",
  rs: "rust",
  css: "css",
  html: "html",
  md: "markdown",
  toml: "ini",
  yml: "yaml",
  yaml: "yaml",
  c: "c",
  cpp: "cpp",
  h: "c",
  glsl: "glsl",
  usf: "glsl",
};

function langFor(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  return EXT_LANG[ext] ?? "plaintext";
}

/// Segons que falten per a l'acceptació automàtica (null si encara no està
/// armada o ja ha expirat). S'actualitza cada mig segon mentre hi ha revisió.
function useAutoAcceptCountdown(mark: EditMark | undefined): number | null {
  const [left, setLeft] = useState<number | null>(null);
  // Primitives estables: l'efecte només es rearma quan canvia el compte
  // enrere (armedAt) o l'estat de streaming, no amb cada actualització del mark.
  const armedAt = mark?.armedAt;
  const streaming = mark?.streaming;
  useEffect(() => {
    if (armedAt === undefined || streaming) {
      setLeft(null);
      return;
    }
    const tick = () => {
      const ms = AUTO_ACCEPT_MS - (Date.now() - armedAt);
      setLeft(ms > 0 ? Math.ceil(ms / 1000) : 0);
    };
    tick();
    const id = setInterval(tick, 500);
    return () => clearInterval(id);
  }, [armedAt, streaming]);
  return left;
}

/// Barra superior de la revisió: nom del fitxer, compte enrere i els botons
/// «Accepta» (deixa el nou, esborra el vell) i «Rebutja» (torna el vell).
function ReviewBar({ mark }: { mark: EditMark }) {
  const { t } = useT();
  const left = useAutoAcceptCountdown(mark);
  return (
    <div className="review-bar">
      <Sparkles size={13} className="review-spark" />
      <span className="review-file">{mark.name}</span>
      <span className="review-hint">
        {t("editor.reviewHint")}
        {left !== null && (
          <b className="review-count"> {t("editor.reviewAuto", { n: left })}</b>
        )}
      </span>
      <div style={{ flex: 1 }} />
      <button
        className="btn sm review-reject"
        title={t("editor.reviewRejectHint")}
        onClick={() => void resolveMark(mark.path, "reject")}
      >
        <Undo2 size={12} /> {t("editor.reviewReject")}
      </button>
      <button
        className="btn sm review-accept"
        title={t("editor.reviewAcceptHint")}
        onClick={() => void resolveMark(mark.path, "accept")}
      >
        <Check size={12} /> {t("editor.reviewAccept")}
      </button>
    </div>
  );
}

/** Previsualització d'una imatge: la llegim via «image_preview_base64» (el
    mateix que fa el xat) i la centrem. Si falla, mostrem el nom + botons. */
function ImageViewer({ path, name }: { path: string; name: string }) {
  const [url, setUrl] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    setUrl(null);
    setErr(null);
    invoke<string>("image_preview_base64", { path })
      .then((d) => alive && setUrl(d))
      .catch((e) => alive && setErr(String(e)));
    return () => {
      alive = false;
    };
  }, [path]);

  return (
    <div className="editor-content image-viewer">
      {url ? (
        <img src={url} alt={name} />
      ) : (
        <div className="image-viewer-msg">
          <ImageOff size={18} /> {err ?? "Carregant imatge…"}
        </div>
      )}
    </div>
  );
}

export default function EditorPane() {
  const { t } = useT();
  const { openFile, updateContent, saveFile, closeFile } = useWorkspaceStore(
    useShallow((st) => ({
      openFile: st.openFile,
      updateContent: st.updateContent,
      saveFile: st.saveFile,
      closeFile: st.closeFile,
    }))
  );
  const theme = useUIStore((s) => s.theme);
  // Autocomplete IA ( sugeriments durant l'escriptura): flag persistent.
  const [acOn, setAcOn] = useState(autocompleteEnabled());
  // Diàleg «Edita la selecció amb la IA».
  const [aiSel, setAiSel] = useState(false);
  const [aiSelEmpty, setAiSelEmpty] = useState(false);
  const [aiSelBusy, setAiSelBusy] = useState(false);
  const [aiSelErr, setAiSelErr] = useState<string | null>(null);
  const openAiSel = () => {
    const ed = getEditor();
    const sel = ed?.getSelection?.();
    setAiSelEmpty(!sel || sel.isEmpty());
    setAiSelErr(null);
    setAiSel(true);
  };
  /// Aplica la instrucció: demana el fragment nou a la IA i el posa al lloc
  /// de la selecció entrant pel flux de revisió (diff + Accepta/Rebutja).
  const applyAiSel = async (instruction: string) => {
    if (!openFile) return;
    const ed = getEditor();
    const model = ed?.getModel?.();
    const sel = ed?.getSelection?.();
    if (!ed || !model || !sel || sel.isEmpty()) return;
    const lang = langFor(openFile.name);
    setAiSelBusy(true);
    setAiSelErr(null);
    const replaced = await aiReplaceSelection(openFile.name, lang, model.getValueInRange(sel), instruction);
    if (!replaced) {
      setAiSelBusy(false);
      setAiSelErr(t("aiEdit.error"));
      return;
    }
    const startOffset = model.getOffsetAt({ lineNumber: sel.startLineNumber, column: sel.startColumn });
    const endOffset = model.getOffsetAt({ lineNumber: sel.endLineNumber, column: sel.endColumn });
    const original = openFile.content;
    const newFull =
      original.slice(0, startOffset) + replaced + original.slice(endOffset);
    setAiSel(false);
    setAiSelBusy(false);
    // Pel flux de revisió: aprofegim el vell i armem el Verd/Verment.
    updateContent(newFull);
    void saveFile(openFile.path, newFull).catch(() => undefined);
    noteFileEdited(openFile.path, newFull, original, true);
  };
  // Edició de la IA pendent de revisar en aquest fitxer (verd = nou /
  // roig = vell). Mentre n'hi ha, l'editor normal es manté muntat però
  // ocult (així no es perd la instància que usa l'LSP i les diagnotificacions).
  const review = useEditReviewStore((s) => s.marks.find((m) => m.path === openFile?.path));
  const reviewing = !!review && !review.streaming;

  // Monaco es carrega ÀJUS (primera obertura d'un fitxer de codi): fins que
  // el seu chunk arribi, fem el marc de l'editor amb un placeholder de pes
  // zero — l'arrencada de l'app no el toca mai.
  const monReady = useMonacoReady();

  if (!openFile) {
    return (
      <div className="editor-pane empty-art">
        <ArtEmpty />
        <div style={{ fontSize: 14, marginBottom: 6, color: "var(--text-0)" }}>{t("editor.noFile")}</div>
        <span className="empty-hint">{t("editor.noFileHint")}</span>
      </div>
    );
  }

  // Fitxer imatge: previsualització en lloc de codi. Monaco no pinten binari.
  if (openFile.imagePath) {    return (
      <div className="editor-pane">
        <div className="editor-tabs">
          <div className="editor-tab">{openFile.name}</div>
          <div style={{ flex: 1 }} />
          <button
            className="btn sm"
            title={t("common.openExternHint")}
            onClick={() => void invoke("open_externally", { path: openFile.imagePath }).catch(() => undefined)}
          >
            <ExternalLink size={12} /> {t("common.openExtern")}
          </button>
          <button
            className="btn sm"
            title={t("common.revealHint")}
            onClick={() => void invoke("reveal_in_finder", { path: openFile.imagePath }).catch(() => undefined)}
          >
            <FolderOpen size={12} /> {t("common.reveal")}
          </button>
          <button className="icon-btn" style={{ width: 16, height: 16 }} onClick={() => closeFile()}>
            <X size={12} />
          </button>
        </div>
        <ImageViewer path={openFile.imagePath} name={openFile.name} />
      </div>
    );
  }

  // Fita de Monaco: mentre el chunk arriba (només la primera obertura),
  // mostrem l'esquelet de l'editor — instantani, sense cap megabyte.
  if (!monReady) {
    return (
      <div className="editor-pane">
        <div className="editor-tabs">
          <div className="editor-tab">
            {openFile.dirty && <span className="dirty-dot" />}
            {openFile.name}
          </div>
        </div>
        <div className="empty-hint" aria-busy="true" style={{ margin: "auto" }}>
          {t("editor.loading")}
        </div>
      </div>
    );
  }

  return (
    <div className="editor-pane">
      <div className="editor-tabs">
        <div className="editor-tab">
          {openFile.dirty && <span className="dirty-dot" />}
          {openFile.name}
          <button
            className="icon-btn"
            style={{ width: 16, height: 16 }}
            onClick={() => closeFile()}
          >
            <X size={12} />
          </button>
        </div>
        <div style={{ flex: 1 }} />
        {/* Autocomplete IA: un clic el posa/llevo (persisteix). */}
        {/* Editar amb la IA la selecció actual mitjà botó (mateix flux de
            revisió en diff verd/roig que la resta d'edicions). */}
        <button className="btn sm" onClick={openAiSel} title={t("aiEdit.title")}>
          <Sparkles size={12} /> {t("aiEdit.short")}
        </button>
        <button
          className="btn sm"
          title={acOn ? t("editor.acHintOff") : t("editor.acHintOn")}
          onClick={() => void toggleAutocomplete().then(setAcOn)}
        >
          <Sparkles size={12} style={{ opacity: acOn ? 1 : 0.35 }} />
          {acOn ? t("editor.acOn") : t("editor.acOff")}
        </button>
        <button
          className="btn sm"
          disabled={!openFile.dirty}
          onClick={() => {
            // El store ja avisa amb un toast si el desat falla; aquí només
            // evitem el rebuig sense gestionar.
            void saveFile(openFile.path, openFile.content).catch(() => undefined);
          }}
        >
          {t("common.save")}
        </button>
      </div>
      {review && <ReviewBar mark={review} />}
      <div className="editor-content">
        <div className={"editor-host" + (reviewing ? " hidden" : "")}>
          <Editor
            theme={monacoThemeFor(theme)}
            path={openFile.path}
            defaultLanguage={langFor(openFile.name)}
            language={langFor(openFile.name)}
            value={openFile.content}
            onChange={(v) => {
              // Si l'usuari escriu sobre un canvi de la IA, la seua edició
              // guanya: la revisió es tanca (evita que un fragment arribat
              // tard li aixafi el que està escrivint).
              dropMarkOnManualSave(openFile.path);
              updateContent(v ?? "");
              validateCurrentFile(openFile.name, v ?? "");
            }}
            onMount={(ed, mon) => {
              bindEditor(ed, mon);
              validateCurrentFile(openFile.name, openFile.content);
              // Autocomplete IA (suggeriments grisos en línia), amb debounce
              // i timeout propis perquè no engargoli mai el teclejat.
              registerInlineComplete(mon);
              // ⇧⌘2: edició de la selecció amb la IA (mateix flux de revisió).
              ed.addCommand(mon.KeyMod.CtrlCmd | mon.KeyMod.Shift | mon.KeyCode.Digit2, () => {
                openAiSel();
              });
              // Correccions ràpides estil Eclipse: ⌘+1 / Ctrl+1 obri el menú de
              // «quick fix» (les accions natives de Monaco + les nostres). Amb
              // ⇧⌘+1 / ⇧Ctrl+1 la IA repara la línia del cursor.
              registerQuickFixes();
              ed.addCommand(mon.KeyMod.CtrlCmd | mon.KeyCode.Digit1, () => {
                ed.getAction("editor.action.quickFix")?.run();
              });
              ed.addCommand(mon.KeyMod.CtrlCmd | mon.KeyMod.Shift | mon.KeyCode.Digit1, () => {
                void aiQuickFix();
              });
            }}
            options={{
              fontSize: 13,
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
              automaticLayout: true,
              tabSize: 2,
              inlineSuggest: { enabled: true },
            }}
          />
        </div>
        {/* Revisió estil Qoder/VS Code: diff EN LÍNIA — el codi vell tal com
            estava, barrat i en roig; el nou que proposa la IA, en verd. */}
        {reviewing && review && (
          <div className="editor-host review-diff">
            <DiffEditor
              height="100%"
              theme={monacoThemeFor(theme)}
              language={langFor(openFile.name)}
              original={review.original}
              modified={review.newContent}
              options={
                {
                  readOnly: true,
                  renderSideBySide: false,
                  renderOverviewRuler: false,
                  minimap: { enabled: false },
                  fontSize: 13,
                  scrollBeyondLastLine: false,
                  automaticLayout: true,
                  tabSize: 2,
                  originalEditable: false,
                } as object
              }
            />
          </div>
        )}
      </div>

      {/* Diàleg «Edita la selecció amb la IA»: instrucció → fragment nou →
          revisió (diff + Accepta/Rebutja) igual que la IA del xat. */}
      {aiSel && (
        <AiSelectionModal
          selected={(() => {
            const ed = getEditor();
            const sel = ed?.getSelection?.();
            return sel && !sel.isEmpty() && ed?.getModel?.()
              ? ed.getModel()!.getValueInRange(sel)
              : "";
          })()}
          empty={aiSelEmpty}
          fileName={openFile.name}
          busy={aiSelBusy}
          error={aiSelErr}
          onApply={(instruction) => void applyAiSel(instruction)}
          onClose={() => setAiSel(false)}
        />
      )}
    </div>
  );
}
