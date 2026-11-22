import Editor, { DiffEditor } from "@monaco-editor/react";
import { useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { invoke } from "@tauri-apps/api/core";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  X,
  ExternalLink,
  FolderOpen,
  ImageOff,
  Film,
  Check,
  Undo2,
  Sparkles,
  Columns2,
  ArrowRightToLine,
} from "lucide-react";
import { useT } from "../../i18n";
import { useWorkspaceStore, type OpenFile } from "../../stores/workspaceStore";
import {
  AUTO_ACCEPT_MS,
  dropMarkOnManualSave,
  resolveMark,
  noteFileEdited,
  useEditReviewStore,
  type EditMark,
} from "../../stores/editReviewStore";
import { useUIStore } from "../../stores/uiStore";
import { useEditorStatusStore } from "../../stores/editorStatusStore";
import {
  bindEditor,
  monacoThemeFor,
  setFocusedEditorGroup,
  type EditorGroup as EditorGroupKind,
} from "../../editor/editorApi";
import { validateCurrentFile } from "../../editor/diagnostics";
import { registerQuickFixes, aiQuickFix } from "../../editor/quickFixes";
import {
  registerInlineComplete,
  autocompleteEnabled,
  toggleAutocomplete,
} from "../../editor/inlineComplete";
import { aiReplaceSelection } from "../../editor/aiSelection";
import {
  registerLspProviders,
  scheduleLspDiagnostics,
  setLspProjectRoot,
} from "../../editor/lspBridge";
import { useMonacoReady } from "../../editor/monReady";
import { ArtEmpty } from "../Illustrations";
import { getEditor } from "../../editor/editorApi";
import AiSelectionModal from "./AiSelectionModal";
import ReferencesOverlay from "./ReferencesOverlay";
import Breadcrumbs from "./Breadcrumbs";

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
  ini: "ini",
  cfg: "ini",
  yml: "yaml",
  yaml: "yaml",
  c: "c",
  cpp: "cpp",
  h: "c",
  glsl: "glsl",
  usf: "glsl",
  // PHP de primera classe: basic-languages del paquet local + lint real.
  php: "php",
  // GDScript (Godot): tokenizer monarch propi a monacoSetup.
  gd: "gdscript",
  xml: "xml",
  sh: "shell",
  sql: "sql",
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
export function ImageViewer({ path, name }: { path: string; name: string }) {
  const { t } = useT();
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
        <img
          src={url}
          alt={name}
          onError={() => {
            // Data-URL indecodificable (p. ex. text pla dins d'un .png):
            // degradem al placeholder amb el nom, com els errors d'invoke.
            setUrl(null);
            setErr(`${name} · ${t("editor.imageInvalid")}`);
          }}
        />
      ) : (
        <div className="image-viewer-msg">
          <ImageOff size={18} /> {err ?? "Carregant imatge…"}
        </div>
      )}
    </div>
  );
}

/// MIME per extensió de vídeo (per crear el Blob reproduïble).
function videoMime(path: string): string {
  const ext = (path.split(".").pop() ?? "").toLowerCase();
  switch (ext) {
    case "mp4":
    case "m4v":
    case "f4v":
      return "video/mp4";
    case "mov":
      return "video/quicktime";
    case "webm":
      return "video/webm";
    case "mkv":
      return "video/x-matroska";
    case "avi":
      return "video/x-msvideo";
    case "mts":
    case "m2ts":
      return "video/mp2t";
    default:
      return "video/*";
  }
}

/** Reproductor de vídeo dins de l'editor: llegeix els bytes amb el plugin fs
    (permís de lectura sobre tot el disc) i crea un Blob URL reproduïble. */
function VideoViewer({ path }: { path: string }) {
  const [url, setUrl] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    setUrl(null);
    setErr(null);
    let objectUrl: string | null = null;
    (async () => {
      try {
        const bytes = await readFile(path);
        if (!alive) return;
        const blob = new Blob([bytes as BlobPart], { type: videoMime(path) });
        objectUrl = URL.createObjectURL(blob);
        if (alive) setUrl(objectUrl);
      } catch (e) {
        if (alive) setErr(String(e));
      }
    })();
    return () => {
      alive = false;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [path]);

  return (
    <div className="editor-content image-viewer">
      {url ? (
        <video src={url} controls style={{ maxWidth: "100%", maxHeight: "100%" }} />
      ) : (
        <div className="image-viewer-msg">
          <Film size={18} /> {err ?? "Carregant vídeo…"}
        </div>
      )}
    </div>
  );
}

/// Capçalera auxiliar d'un grup: per a imatge/vídeo (obrir externament,
/// mostrar al Finder) i per al grup dividit (nom + tancar la divisió).
function GroupBar({ file, isSplit }: { file: OpenFile; isSplit: boolean }) {
  const { t } = useT();
  const closeFile = useWorkspaceStore((s) => s.closeFile);
  const toggleSplit = useWorkspaceStore((s) => s.toggleSplit);
  const media = file.imagePath ?? file.videoPath;
  return (
    <div className="editor-group-head">
      {file.dirty && <span className="dirty-dot" />}
      <span className="editor-group-name" title={file.path}>
        {file.name}
      </span>
      <div style={{ flex: 1 }} />
      {media && (
        <>
          <button
            className="btn sm"
            title={t("common.openExternHint")}
            onClick={() => void invoke("open_externally", { path: media }).catch(() => undefined)}
          >
            <ExternalLink size={12} /> {t("common.openExtern")}
          </button>
          <button
            className="btn sm"
            title={t("common.revealHint")}
            onClick={() => void invoke("reveal_in_finder", { path: media }).catch(() => undefined)}
          >
            <FolderOpen size={12} /> {t("common.reveal")}
          </button>
        </>
      )}
      {isSplit ? (
        <button
          className="icon-btn"
          style={{ width: 16, height: 16 }}
          title={t("editor.splitClose")}
          onClick={() => toggleSplit()}
        >
          <X size={12} />
        </button>
      ) : (
        media && (
          <button
            className="icon-btn"
            style={{ width: 16, height: 16 }}
            title={t("editor.tabClose")}
            onClick={() => closeFile(file.path)}
          >
            <X size={12} />
          </button>
        )
      )}
    </div>
  );
}

/// Un grup d'editor (principal o dividit): mostra UN fitxer amb el seu
/// Monaco, la revisió verd/roig de la IA o el visor d'imatge/vídeo.
function EditorGroup({
  group,
  file,
  onOpenAiSel,
}: {
  group: EditorGroupKind;
  file: OpenFile;
  onOpenAiSel: () => void;
}) {
  const { t } = useT();
  const theme = useUIStore((s) => s.theme);
  const updateContent = useWorkspaceStore((s) => s.updateContent);
  const setFocusedGroup = useWorkspaceStore((s) => s.setFocusedGroup);
  const root = useWorkspaceStore((s) => s.root);
  const isSplit = group === "split";
  // Edició de la IA pendent de revisar en aquest fitxer (verd = nou /
  // roig = vell). Mentre n'hi ha, l'editor normal es manté muntat però
  // ocult (així no es perd la instància que usa l'LSP i els diagnòstics).
  const review = useEditReviewStore((s) => s.marks.find((m) => m.path === file.path));
  const reviewing = !!review && !review.streaming;
  const monReady = useMonacoReady();

  // En desmuntar el grup (divisió tancada) alliberem el binding: les
  // accions globals no han d'apuntar mai a un editor mort.
  useEffect(() => {
    return () => {
      bindEditor(null, undefined, group);
    };
  }, [group]);

  const focusHere = () => {
    setFocusedEditorGroup(group);
    setFocusedGroup(group);
  };

  if (file.imagePath) {
    return (
      <div className="editor-group" onFocusCapture={focusHere} onMouseDownCapture={focusHere}>
        <GroupBar file={file} isSplit={isSplit} />
        <ImageViewer path={file.imagePath} name={file.name} />
      </div>
    );
  }
  if (file.videoPath) {
    return (
      <div className="editor-group" onFocusCapture={focusHere} onMouseDownCapture={focusHere}>
        <GroupBar file={file} isSplit={isSplit} />
        <VideoViewer path={file.videoPath} />
      </div>
    );
  }

  // Fita de Monaco: mentre el chunk arriba (només la primera obertura),
  // mostrem l'esquelet de l'editor — instantani, sense cap megabyte.
  if (!monReady) {
    return (
      <div className="editor-group">
        {isSplit && <GroupBar file={file} isSplit />}
        <div className="empty-hint" aria-busy="true" style={{ margin: "auto" }}>
          {t("editor.loading")}
        </div>
      </div>
    );
  }

  return (
    <div className="editor-group" onFocusCapture={focusHere} onMouseDownCapture={focusHere}>
      {isSplit && <GroupBar file={file} isSplit />}
      {review && <ReviewBar mark={review} />}
      <div className="editor-content">
        <div className={"editor-host" + (reviewing ? " hidden" : "")}>
          <Editor
            theme={monacoThemeFor(theme)}
            path={file.path}
            defaultLanguage={langFor(file.name)}
            language={langFor(file.name)}
            value={file.content}
            onChange={(v) => {
              // Si l'usuari escriu sobre un canvi de la IA, la seua edició
              // guanya: la revisió es tanca (evita que un fragment arribat
              // tard li aixafi el que està escrivint).
              dropMarkOnManualSave(file.path);
              updateContent(v ?? "", file.path);
              validateCurrentFile(file.name, v ?? "", file.path);
              // Diagnòstics LSP reals (debounce + cancel·lació propis).
              scheduleLspDiagnostics(root, file.path);
            }}
            onMount={(ed, mon) => {
              bindEditor(ed, mon, group);
              validateCurrentFile(file.name, file.content, file.path);
              // Autocomplete IA (suggeriments grisos en línia), amb debounce
              // i timeout propis perquè no alenteixi mai el teclejat.
              registerInlineComplete(mon);
              // LSP persistent (Fase 3c): completion/hover per a llenguatges
              // amb servidor detectat + diagnòstics de projecte del tsserver.
              registerLspProviders(mon);
              scheduleLspDiagnostics(root, file.path);
              // El focus del text mana: les dreceres globals (⌘W, IA…)
              // actuen sobre ESTE grup mentre s'hi escriu.
              ed.onDidFocusEditorText(() => {
                focusHere();
                // StatusBar d'IDE: el llenguatge i la posició segueixen
                // l'editor que l'usuari està tocant.
                useEditorStatusStore.getState().setLanguage(langFor(file.name));
                const p = ed.getPosition();
                if (p) useEditorStatusStore.getState().setPosition(p.lineNumber, p.column);
              });
              // Ln/Col en viu per a la barra d'estat (Fase 4).
              ed.onDidChangeCursorPosition((e) => {
                useEditorStatusStore.getState().setPosition(e.position.lineNumber, e.position.column);
              });
              // ⇧⌘2: edició de la selecció amb la IA (mateix flux de revisió).
              ed.addCommand(mon.KeyMod.CtrlCmd | mon.KeyMod.Shift | mon.KeyCode.Digit2, () => {
                focusHere();
                onOpenAiSel();
              });
              // Correccions ràpides: ⌘. obri el menú de «quick fix» (les
              // accions natives de Monaco + les nostres); ⇧⌘. la IA repara
              // la línia del cursor. (⌘1..9 ara salten a la pestanya n.)
              registerQuickFixes();
              ed.addCommand(mon.KeyMod.CtrlCmd | mon.KeyCode.Period, () => {
                ed.getAction("editor.action.quickFix")?.run();
              });
              ed.addCommand(mon.KeyMod.CtrlCmd | mon.KeyMod.Shift | mon.KeyCode.Period, () => {
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
              language={langFor(file.name)}
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

/// Barra de PESTANYES (com VS Code): nom + punt de «sense desar», tancar
/// amb creu o clic del mig, menú contextual (obri al costat / tanca…),
/// scroll horitzontal quan no caben i les accions del fitxer amb focus.
function TabBar({
  onOpenAiSel,
  onMenu,
}: {
  onOpenAiSel: () => void;
  onMenu: (m: { x: number; y: number; path: string }) => void;
}) {
  const { t } = useT();
  const { openFiles, activePath, splitPath, focusedGroup } = useWorkspaceStore(
    useShallow((st) => ({
      openFiles: st.openFiles,
      activePath: st.activePath,
      splitPath: st.splitPath,
      focusedGroup: st.focusedGroup,
    }))
  );
  const setActive = useWorkspaceStore((s) => s.setActive);
  const closeFile = useWorkspaceStore((s) => s.closeFile);
  const saveFile = useWorkspaceStore((s) => s.saveFile);
  const toggleSplit = useWorkspaceStore((s) => s.toggleSplit);
  const [acOn, setAcOn] = useState(autocompleteEnabled());
  const scrollRef = useRef<HTMLDivElement>(null);

  const focusedFile =
    focusedGroup === "split" && splitPath
      ? openFiles.find((f) => f.path === splitPath)
      : openFiles.find((f) => f.path === activePath);

  // La pestanya activa sempre visible (scroll horitzontal automàtic).
  useEffect(() => {
    scrollRef.current
      ?.querySelector(".editor-tab.active")
      ?.scrollIntoView({ inline: "nearest", block: "nearest" });
  }, [activePath]);

  return (
    <div className="editor-tabs">
      <div className="editor-tabs-scroll" ref={scrollRef} role="tablist">
        {openFiles.map((f) => (
          <div
            key={f.path}
            role="tab"
            aria-selected={f.path === activePath}
            className={
              "editor-tab" +
              (f.path === activePath ? " active" : "") +
              (f.path === splitPath ? " in-split" : "")
            }
            title={f.path + (f.dirty ? " — " + t("editor.dirty") : "")}
            onClick={() => setActive(f.path)}
            onAuxClick={(e) => {
              // Clic del mig: tanca la pestanya (com VS Code).
              if (e.button === 1) {
                e.preventDefault();
                closeFile(f.path);
              }
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              onMenu({ x: e.clientX, y: e.clientY, path: f.path });
            }}
          >
            {f.dirty && <span className="dirty-dot" />}
            <span className="tab-name">{f.name}</span>
            <button
              className="tab-close"
              title={t("editor.tabClose")}
              onClick={(e) => {
                e.stopPropagation();
                closeFile(f.path);
              }}
            >
              <X size={11} />
            </button>
          </div>
        ))}
      </div>
      <div className="editor-tabs-actions">
        <button
          className="icon-btn"
          title={t("editor.splitToggle")}
          onClick={() => toggleSplit()}
        >
          <Columns2 size={13} style={{ opacity: splitPath ? 1 : 0.55 }} />
        </button>
        {/* Editar amb la IA la selecció actual (mateix flux de revisió en
            diff verd/roig que la resta d'edicions). */}
        <button className="btn sm" onClick={onOpenAiSel} title={t("aiEdit.title")}>
          <Sparkles size={12} /> {t("aiEdit.short")}
        </button>
        {/* Autocomplete IA: un clic el posa/lleva (persisteix). */}
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
          disabled={!focusedFile?.dirty}
          onClick={() => {
            if (!focusedFile) return;
            if (focusedFile.path.startsWith("untitled:")) {
              // Sense títol: cal el diàleg «Desa com a…» (sobre l'actiu).
              setActive(focusedFile.path);
              void useWorkspaceStore.getState().saveAs();
              return;
            }
            // El store ja avisa amb un toast si el desat falla; aquí només
            // evitem el rebuig sense gestionar.
            void saveFile(focusedFile.path, focusedFile.content).catch(() => undefined);
          }}
        >
          {t("common.save")}
        </button>
      </div>
    </div>
  );
}

export default function EditorPane() {
  const { t } = useT();
  const { openFiles, activePath, splitPath, focusedGroup } = useWorkspaceStore(
    useShallow((st) => ({
      openFiles: st.openFiles,
      activePath: st.activePath,
      splitPath: st.splitPath,
      focusedGroup: st.focusedGroup,
    }))
  );
  const root = useWorkspaceStore((s) => s.root);
  // El pont d'accions globals (menús, dreceres) segueix el grup amb focus.
  useEffect(() => {
    setFocusedEditorGroup(focusedGroup);
  }, [focusedGroup]);
  // Els providers LSP consulten l'arrel del projecte en cada petició.
  useEffect(() => {
    setLspProjectRoot(root);
  }, [root]);

  // Menú contextual d'una pestanya (obri al costat, tanca, tanca les altres…).
  const [menu, setMenu] = useState<{ x: number; y: number; path: string } | null>(null);
  // Diàleg «Edita la selecció amb la IA» (sobre l'editor amb focus).
  const [aiSel, setAiSel] = useState(false);
  const [aiSelEmpty, setAiSelEmpty] = useState(false);
  const [aiSelBusy, setAiSelBusy] = useState(false);
  const [aiSelErr, setAiSelErr] = useState<string | null>(null);

  const mainFile = openFiles.find((f) => f.path === activePath);
  const splitFile = splitPath ? openFiles.find((f) => f.path === splitPath) : undefined;
  const focusedFile = focusedGroup === "split" && splitFile ? splitFile : mainFile;

  // StatusBar d'IDE: el llenguatge mostrat és el del fitxer amb focus
  // (també quan es canvia de pestanya sense tocar l'editor).
  const focusedName = focusedFile?.name;
  useEffect(() => {
    const st = useEditorStatusStore.getState();
    st.setLanguage(focusedName ? langFor(focusedName) : null);
    const p = getEditor()?.getPosition?.();
    if (p) st.setPosition(p.lineNumber, p.column);
  }, [focusedName]);

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
    // Estat EN VIU: el diàleg pot haver-se obert sobre un grup i el focus
    // canviar mentre s'escriu la instrucció.
    const s = useWorkspaceStore.getState();
    const file =
      s.focusedGroup === "split" && s.splitPath
        ? s.openFiles.find((f) => f.path === s.splitPath)
        : s.openFile;
    if (!file) return;
    const ed = getEditor();
    const model = ed?.getModel?.();
    const sel = ed?.getSelection?.();
    if (!ed || !model || !sel || sel.isEmpty()) return;
    const lang = langFor(file.name);
    setAiSelBusy(true);
    setAiSelErr(null);
    const replaced = await aiReplaceSelection(file.name, lang, model.getValueInRange(sel), instruction);
    if (!replaced) {
      setAiSelBusy(false);
      setAiSelErr(t("aiEdit.error"));
      return;
    }
    const startOffset = model.getOffsetAt({ lineNumber: sel.startLineNumber, column: sel.startColumn });
    const endOffset = model.getOffsetAt({ lineNumber: sel.endLineNumber, column: sel.endColumn });
    const original = file.content;
    const newFull = original.slice(0, startOffset) + replaced + original.slice(endOffset);
    setAiSel(false);
    setAiSelBusy(false);
    // Pel flux de revisió: capturem el vell i armem el verd/roig.
    s.updateContent(newFull, file.path);
    void s.saveFile(file.path, newFull).catch(() => undefined);
    noteFileEdited(file.path, newFull, original, true);
  };

  if (!mainFile) {
    return (
      <div className="editor-pane empty-art">
        <ArtEmpty />
        <div style={{ fontSize: 14, marginBottom: 6, color: "var(--text-0)" }}>{t("editor.noFile")}</div>
        <span className="empty-hint">{t("editor.noFileHint")}</span>
      </div>
    );
  }

  return (
    <div className="editor-pane">
      <TabBar onOpenAiSel={openAiSel} onMenu={setMenu} />
      {/* Ruta del fitxer actiu (carpeta > … > fitxer), segments clicables. */}
      <Breadcrumbs />
      <div className={"editor-groups" + (splitFile ? " split" : "")}>
        <EditorGroup group="main" file={mainFile} onOpenAiSel={openAiSel} />
        {splitFile && <EditorGroup group="split" file={splitFile} onOpenAiSel={openAiSel} />}
      </div>

      {/* Menú contextual de pestanya: «Obre al costat» i tancaments. */}
      {menu && (
        <div
          className="ctx-overlay"
          onClick={() => setMenu(null)}
          onContextMenu={(e) => {
            e.preventDefault();
            setMenu(null);
          }}
        >
          <div
            className="ctx-menu"
            style={{
              left: Math.min(menu.x, window.innerWidth - 240),
              top: Math.min(menu.y, window.innerHeight - 170),
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              className="ctx-row"
              onClick={() => {
                void useWorkspaceStore.getState().openSide(menu.path);
                setMenu(null);
              }}
            >
              <ArrowRightToLine size={13} /> {t("editor.openSide")}
            </button>
            <div className="ctx-sep" />
            <button
              className="ctx-row"
              onClick={() => {
                useWorkspaceStore.getState().closeFile(menu.path);
                setMenu(null);
              }}
            >
              <X size={13} /> {t("editor.tabClose")}
            </button>
            <button
              className="ctx-row"
              onClick={() => {
                useWorkspaceStore.getState().closeOthers(menu.path);
                setMenu(null);
              }}
            >
              <X size={13} /> {t("editor.tabCloseOthers")}
            </button>
            <button
              className="ctx-row"
              onClick={() => {
                useWorkspaceStore.getState().closeAll();
                setMenu(null);
              }}
            >
              <X size={13} /> {t("editor.tabCloseAll")}
            </button>
          </div>
        </div>
      )}

      {/* Referències LSP (⌥F12): llista navegable amb salt al fitxer. */}
      <ReferencesOverlay />

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
          fileName={focusedFile?.name ?? ""}
          busy={aiSelBusy}
          error={aiSelErr}
          onApply={(instruction) => void applyAiSel(instruction)}
          onClose={() => setAiSel(false)}
        />
      )}
    </div>
  );
}