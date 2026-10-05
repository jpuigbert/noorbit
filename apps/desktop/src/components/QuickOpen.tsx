import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../i18n";
import { useUIStore } from "../stores/uiStore";
import { useWorkspaceStore, type FileNode } from "../stores/workspaceStore";

const SKIP_DIRS = new Set(["node_modules", ".git", "target", "dist", "build", "__pycache__"]);
const MAX_FILES = 3000;

async function scanDir(dir: string, out: string[], depth: number) {
  if (out.length >= MAX_FILES || depth > 8) return;
  let nodes: FileNode[];
  try {
    nodes = await invoke<FileNode[]>("list_dir", { path: dir });
  } catch {
    return;
  }
  for (const n of nodes) {
    if (out.length >= MAX_FILES) return;
    if (n.is_dir) {
      if (SKIP_DIRS.has(n.name) || n.name.startsWith(".")) continue;
      await scanDir(n.path, out, depth + 1);
    } else {
      out.push(n.path);
    }
  }
}

/// Obertura ràpida de fitxers (⌘P) amb cerca per nom, com VS Code.
export default function QuickOpen() {
  const show = useUIStore((s) => s.showQuickOpen);
  const setShow = useUIStore((s) => s.setShowQuickOpen);
  const root = useWorkspaceStore((s) => s.root);
  const loadFile = useWorkspaceStore((s) => s.loadFile);
  const { t } = useT();

  const [files, setFiles] = useState<string[]>([]);
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!show) return;
    setQ("");
    setSel(0);
    setTimeout(() => inputRef.current?.focus(), 30);
    if (root && files.length === 0) {
      setLoading(true);
      const out: string[] = [];
      void scanDir(root, out, 0).then(() => {
        setFiles(out);
        setLoading(false);
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [show, root]);

  const matches = useMemo(() => {
    if (!q.trim()) return files.slice(0, 100);
    const needle = q.toLowerCase();
    // cerca tipus "fuzzy lleuger": toquen el nom de fitxer i la ruta
    return files
      .filter((f) => f.toLowerCase().includes(needle))
      .slice(0, 100);
  }, [q, files]);

  if (!show) return null;

  const openAt = (i: number) => {
    const f = matches[i];
    if (f) void loadFile(f);
    setShow(false);
  };

  const rel = (p: string) => (root && p.startsWith(root) ? p.slice(root.length + 1) : p);

  return (
    <div className="modal-overlay" onClick={() => setShow(false)}>
      <div className="modal cp-palette" onClick={(e) => e.stopPropagation()}>
        <input
          ref={inputRef}
          className="cp-palette-input"
          placeholder={t("quickOpen.placeholder")}
          value={q}
          onChange={(e) => {
            setQ(e.target.value);
            setSel(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setSel((s) => Math.min(matches.length - 1, s + 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setSel((s) => Math.max(0, s - 1));
            } else if (e.key === "Enter") {
              e.preventDefault();
              openAt(sel);
            } else if (e.key === "Escape") {
              setShow(false);
            }
          }}
        />
        <div className="cp-palette-list">
          {loading && <div className="cp-empty">{t("quickOpen.loading")}</div>}
          {!root && <div className="cp-empty">{t("quickOpen.noWorkspace")}</div>}
          {!loading &&
            matches.map((f, i) => (
              <button
                key={f}
                className={"cp-palette-row" + (i === sel ? " selected" : "")}
                onMouseEnter={() => setSel(i)}
                onClick={() => openAt(i)}
              >
                <span>{f.split(/[/\\]/).pop()}</span>
                <span style={{ opacity: 0.5, fontSize: 11, marginLeft: 8 }}>{rel(f)}</span>
              </button>
            ))}
          {!loading && !root && matches.length === 0 && (
            <div className="cp-empty">{t("quickOpen.empty")}</div>
          )}
        </div>
      </div>
    </div>
  );
}
