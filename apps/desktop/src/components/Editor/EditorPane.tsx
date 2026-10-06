import Editor, { DiffEditor } from "@monaco-editor/react";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { X, ExternalLink, FolderOpen, ImageOff, Check, Undo2, Sparkles } from "lucide-react";
import { useT } from "../../i18n";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import {
  AUTO_ACCEPT_MS,
  dropMarkOnManualSave,
  resolveMark,
  useEditReviewStore,
  type EditMark,
} from "../../stores/editReviewStore";
import { useUIStore } from "../../stores/uiStore";
import { bindEditor, monacoThemeFor } from "../../editor/editorApi";
import { validateCurrentFile } from "../../editor/diagnostics";
import { registerQuickFixes, aiQuickFix } from "../../editor/quickFixes";

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
  useEffect(() => {
    if (!mark || mark.streaming) {
      setLeft(null);
      return;
    }
    const tick = () => {
      const ms = AUTO_ACCEPT_MS - (Date.now() - mark.armedAt);
      setLeft(ms > 0 ? Math.ceil(ms / 1000) : 0);
    };
    tick();
    const id = setInterval(tick, 500);
    return () => clearInterval(id);
  }, [mark?.armedAt, mark?.streaming]);
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
  const { openFile, updateContent, saveFile, closeFile } = useWorkspaceStore();
  const theme = useUIStore((s) => s.theme);
  // Edició de la IA pendent de revisar en aquest fitxer (verd = nou /
  // roig = vell). Mentre n'hi ha, l'editor normal es manté muntat però
  // ocult (així no es perd la instància que usa l'LSP i les diagnotificacions).
  const review = useEditReviewStore((s) => s.marks.find((m) => m.path === openFile?.path));
  const reviewing = !!review && !review.streaming;

  if (!openFile) {
    return (
      <div className="editor-pane">
        <div className="empty-hint" style={{ margin: "auto" }}>
          <div style={{ fontSize: 14, marginBottom: 6 }}>{t("editor.noFile")}</div>
          {t("editor.noFileHint")}
        </div>
      </div>
    );
  }

  // Fitxer imatge: previsualització en lloc de codi. Monaco no pinten binari.
  if (openFile.imagePath) {
    return (
      <div className="editor-pane">
        <div className="editor-tabs">
          <div className="editor-tab">{openFile.name}</div>
          <div style={{ flex: 1 }} />
          <button
            className="btn sm"
            title="Obri amb l'aplicació per defecte del sistema"
            onClick={() => void invoke("open_externally", { path: openFile.imagePath }).catch(() => undefined)}
          >
            <ExternalLink size={12} /> Obri
          </button>
          <button
            className="btn sm"
            title="Mostra al Finder / explorador"
            onClick={() => void invoke("reveal_in_finder", { path: openFile.imagePath }).catch(() => undefined)}
          >
            <FolderOpen size={12} /> Mostra
          </button>
          <button className="icon-btn" style={{ width: 16, height: 16 }} onClick={() => closeFile()}>
            <X size={12} />
          </button>
        </div>
        <ImageViewer path={openFile.imagePath} name={openFile.name} />
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
        <button
          className="btn sm"
          disabled={!openFile.dirty}
          onClick={() => saveFile(openFile.path, openFile.content)}
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
    </div>
  );
}
