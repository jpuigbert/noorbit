import Editor from "@monaco-editor/react";
import { X } from "lucide-react";
import { useT } from "../../i18n";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useUIStore } from "../../stores/uiStore";
import { bindEditor, monacoThemeFor } from "../../editor/editorApi";

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

export default function EditorPane() {
  const { t } = useT();
  const { openFile, updateContent, saveFile, closeFile } = useWorkspaceStore();
  const theme = useUIStore((s) => s.theme);

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
      <div className="editor-content">
        <Editor
          theme={monacoThemeFor(theme)}
          path={openFile.path}
          defaultLanguage={langFor(openFile.name)}
          language={langFor(openFile.name)}
          value={openFile.content}
          onChange={(v) => updateContent(v ?? "")}
          onMount={(ed, mon) => bindEditor(ed, mon)}
          options={{
            fontSize: 13,
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
            automaticLayout: true,
            tabSize: 2,
          }}
        />
      </div>
    </div>
  );
}
