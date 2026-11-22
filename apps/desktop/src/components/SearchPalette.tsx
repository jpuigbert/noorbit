import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { Regex, Replace, ReplaceAll } from "lucide-react";
import { useT } from "../i18n";
import { useUIStore } from "../stores/uiStore";
import {
  isImagePath,
  isVideoPath,
  useWorkspaceStore,
  type FileNode,
} from "../stores/workspaceStore";
import { dropMarkOnManualSave } from "../stores/editReviewStore";
import { toastError, toastInfo, toastSuccess } from "../stores/toastStore";
import { revealAt, setSearchMatches } from "../editor/editorApi";
import {
  buildSearchRegex,
  countMatches,
  findLineMatches,
  replaceAllInFile,
} from "../utils/searchReplace";

interface SearchHit {
  /// Ruta RELATIVA al projecte (com la del backend `search_files`).
  path: string;
  line: number;
  column: number;
  preview: string;
  match_len: number;
}

/// Límit de coincidències mostrades: projects enormes no poden congelar la
/// UI. Si se supera, avís explícit perquè l'usuari afine la cerca.
const MAX_HITS = 200;
/// Màxim de fitxers als quals es llegeix el contingut per al recompte de
/// substitucions (cada lectura és una crida al backend).
const MAX_COUNT_FILES = 60;
/// Escombrada client-side del mode regex (el backend cerca literal).
const SKIP_DIRS = new Set(["node_modules", ".git", "target", "dist", "build", "__pycache__"]);
const MAX_SCAN_FILES = 1200;

async function scanDir(dir: string, out: string[], depth: number) {
  if (out.length >= MAX_SCAN_FILES || depth > 8) return;
  let nodes: FileNode[];
  try {
    nodes = await invoke<FileNode[]>("list_dir", { path: dir });
  } catch {
    return;
  }
  for (const n of nodes) {
    if (out.length >= MAX_SCAN_FILES) return;
    if (n.is_dir) {
      if (SKIP_DIRS.has(n.name) || n.name.startsWith(".")) continue;
      await scanDir(n.path, out, depth + 1);
    } else if (!isImagePath(n.path) && !isVideoPath(n.path)) {
      out.push(n.path);
    }
  }
}

/// Cerca de text a TOT el projecte (⇧⌘F), com VS Code: mode literal (backend
/// `search_files`) o REGEX amb validació del patró, i SUBSTITUCIÓ global amb
/// recompte per fitxer, preview de les línies i confirmació abans d'escriure.
export default function SearchPalette() {
  const show = useUIStore((s) => s.showSearch);
  const setShow = useUIStore((s) => s.setShowSearch);
  const root = useWorkspaceStore((s) => s.root);
  const loadFile = useWorkspaceStore((s) => s.loadFile);
  const { t } = useT();

  const [q, setQ] = useState("");
  const [regexOn, setRegexOn] = useState(false);
  const [replaceOn, setReplaceOn] = useState(false);
  const [repl, setRepl] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [sel, setSel] = useState(0);
  const [searching, setSearching] = useState(false);
  const [truncated, setTruncated] = useState(false);
  const [regexError, setRegexError] = useState<string | null>(null);
  /// Recompte REAL de coincidències per fitxer (llegint el contingut sencer,
  /// no només les files mostrades) per al «Substitueix tot».
  const [counts, setCounts] = useState<Record<string, number>>({});
  const [replacing, setReplacing] = useState(false);
  const [refreshTick, setRefreshTick] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const seqRef = useRef(0);

  const base = (root ?? "").replace(/\/+$/, "");
  const minLen = regexOn ? 1 : 2;
  const needle = q.trim();

  useEffect(() => {
    if (show) {
      setSel(0);
      const id = setTimeout(() => inputRef.current?.focus(), 30);
      return () => clearTimeout(id);
    }
    return undefined;
  }, [show]);

  // ── Cerca (debounce 350 ms) ──────────────────────────────────────────────
  useEffect(() => {
    if (!show || !root) return undefined;
    if (needle.length < minLen) {
      setHits([]);
      setRegexError(null);
      setTruncated(false);
      return undefined;
    }
    let re: RegExp | null = null;
    if (regexOn) {
      try {
        re = buildSearchRegex(needle, true);
        setRegexError(null);
      } catch (err) {
        setRegexError(err instanceof Error ? err.message : String(err));
        setHits([]);
        setTruncated(false);
        return undefined;
      }
    }
    const seq = ++seqRef.current;
    const timer = setTimeout(() => {
      setSearching(true);
      const run = async (): Promise<{ hits: SearchHit[]; truncated: boolean }> => {
        if (!regexOn) {
          // Mode literal: cerca del backend (ràpida, indexada al servidor).
          const h = await invoke<SearchHit[]>("search_files", {
            query: needle,
            limit: MAX_HITS,
          });
          return { hits: h, truncated: h.length >= MAX_HITS };
        }
        // Mode REGEX: escombrada client-side acotada + lectura per fitxer.
        const files: string[] = [];
        await scanDir(base, files, 0);
        const out: SearchHit[] = [];
        let cut = files.length >= MAX_SCAN_FILES;
        for (const abs of files) {
          if (out.length >= MAX_HITS) {
            cut = true;
            break;
          }
          let content: string;
          try {
            content = await invoke<string>("read_file", { path: abs });
          } catch {
            continue; // binari o il·legible: fora
          }
          if (content.length > 2_000_000) continue;
          const ms = findLineMatches(content, re!, MAX_HITS - out.length);
          const rel = abs.startsWith(base + "/") ? abs.slice(base.length + 1) : abs;
          for (const m of ms) {
            out.push({
              path: rel,
              line: m.line,
              column: m.column,
              preview: m.preview,
              match_len: m.matchLen,
            });
          }
        }
        return { hits: out, truncated: cut };
      };
      void run()
        .then((r) => {
          if (seqRef.current !== seq) return; // cerca més nova en curs
          setHits(r.hits);
          setTruncated(r.truncated);
          setSel(0);
        })
        .catch(() => {
          if (seqRef.current !== seq) return;
          setHits([]);
          setTruncated(false);
        })
        .finally(() => {
          if (seqRef.current === seq) setSearching(false);
        });
    }, 350);
    return () => clearTimeout(timer);
  }, [q, regexOn, show, root, refreshTick, needle, minLen, base]);

  // ── Recompte per fitxer (mode substitució) ──────────────────────────────
  const hitFiles = useMemo(() => {
    const seen: string[] = [];
    for (const h of hits) if (!seen.includes(h.path)) seen.push(h.path);
    return seen;
  }, [hits]);

  useEffect(() => {
    if (!show || !replaceOn || !root || needle.length < minLen || hitFiles.length === 0) {
      setCounts({});
      return undefined;
    }
    let re: RegExp;
    try {
      re = buildSearchRegex(needle, regexOn);
    } catch {
      setCounts({});
      return undefined;
    }
    let alive = true;
    void (async () => {
      const out: Record<string, number> = {};
      for (const rel of hitFiles.slice(0, MAX_COUNT_FILES)) {
        const abs = `${base}/${rel}`;
        // Si la pestanya és oberta, el seu contingut mana (pot ser brut).
        const tab = useWorkspaceStore.getState().openFiles.find((f) => f.path === abs);
        let content = tab?.content;
        if (content == null) {
          try {
            content = await invoke<string>("read_file", { path: abs });
          } catch {
            continue;
          }
        }
        out[rel] = countMatches(content, re);
      }
      if (alive) setCounts(out);
    })();
    return () => {
      alive = false;
    };
  }, [show, replaceOn, root, needle, regexOn, hitFiles, minLen, base, refreshTick]);

  const totalRepl = useMemo(
    () => Object.values(counts).reduce((a, b) => a + b, 0),
    [counts]
  );
  const replFiles = useMemo(
    () => Object.values(counts).filter((n) => n > 0).length,
    [counts]
  );

  // Agrupació visual per fitxer (l'índex pla de selecció es conserva).
  const groups = useMemo(() => {
    const out: { path: string; items: { hit: SearchHit; index: number }[] }[] = [];
    hits.forEach((hit, index) => {
      const last = out[out.length - 1];
      if (last && last.path === hit.path) last.items.push({ hit, index });
      else out.push({ path: hit.path, items: [{ hit, index }] });
    });
    return out;
  }, [hits]);

  if (!show) return null;

  // ── Obrir una coincidència ───────────────────────────────────────────────
  const openHit = (i: number) => {
    const h = hits[i];
    if (!h || !root) return;
    const abs = `${base}/${h.path}`;
    setShow(false);
    void (async () => {
      await loadFile(abs);
      const content =
        useWorkspaceStore.getState().openFiles.find((f) => f.path === abs)?.content ??
        h.preview;
      // Sessió de cerca: TOTES les posicions dins del fitxer (F3/⇧F3).
      const ranges: {
        startLineNumber: number;
        startColumn: number;
        endLineNumber: number;
        endColumn: number;
      }[] = [];
      try {
        const re = buildSearchRegex(needle, regexOn);
        for (const m of findLineMatches(content, re, 2000)) {
          ranges.push({
            startLineNumber: m.line,
            startColumn: m.column,
            endLineNumber: m.line,
            endColumn: m.column + m.matchLen,
          });
        }
      } catch {
        /* patró invàlid: sense ressaltat, però el salt a la línia sí */
      }
      setTimeout(() => {
        setSearchMatches(ranges);
        revealAt(h.line, h.column, h.match_len);
      }, 350);
    })();
  };

  // ── Substitueix TOT (amb confirmació) ────────────────────────────────────
  const doReplaceAll = () => {
    if (!root || replacing || totalRepl === 0) return;
    let re: RegExp;
    try {
      re = buildSearchRegex(needle, regexOn);
    } catch {
      return;
    }
    void (async () => {
      const ok = await ask(t("search.replaceConfirm", { n: totalRepl, m: replFiles }), {
        title: t("search.replaceAll"),
        kind: "warning",
      });
      if (!ok) return;
      setReplacing(true);
      let doneN = 0;
      let doneM = 0;
      try {
        for (const [rel, n] of Object.entries(counts)) {
          if (n <= 0) continue;
          const abs = `${base}/${rel}`;
          // El helper rellegeix l'estat VIU de la pestanya després d'escriure:
          // el que l'usuari tecleja durant el write no s'aixafa.
          const nDone = await replaceAllInFile(abs, re, repl, regexOn, {
            readFile: (p) => invoke<string>("read_file", { path: p }),
            writeFile: async (p, c) => {
              await invoke("write_file", { path: p, content: c });
            },
            getTab: (p) => useWorkspaceStore.getState().openFiles.find((f) => f.path === p),
            setTab: (p, text) => {
              // Pestanya oberta: el text nou també a l'editor (sense brutar),
              // amb les fields vives de la pestanya (no una captura antiga).
              dropMarkOnManualSave(p);
              const live = useWorkspaceStore.getState().openFiles.find((f) => f.path === p);
              if (live) {
                useWorkspaceStore.getState().setOpenFile({ ...live, content: text, dirty: false });
              }
            },
          });
          if (nDone > 0) {
            doneN += nDone;
            doneM += 1;
          }
        }
        if (doneN > 0) toastSuccess(t("search.replaceDone", { n: doneN, m: doneM }));
        else toastInfo(t("search.replaceNone"));
        setRefreshTick((x) => x + 1); // re-cerca: els resultats han canviat
      } catch (e) {
        toastError(t("search.replaceError"), e);
      } finally {
        setReplacing(false);
      }
    })();
  };

  const onListKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSel((s) => Math.min(hits.length - 1, s + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSel((s) => Math.max(0, s - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      openHit(sel);
    } else if (e.key === "Escape") {
      setShow(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={() => setShow(false)}>
      <div className="modal cp-palette" onClick={(e) => e.stopPropagation()}>
        <div className="sp-query-row">
          <input
            ref={inputRef}
            className="cp-palette-input"
            placeholder={t("search.placeholder")}
            value={q}
            onChange={(e) => {
              setQ(e.target.value);
              setSel(0);
            }}
            onKeyDown={onListKey}
          />
          <button
            className={"sp-toggle" + (regexOn ? " on" : "")}
            title={t("search.regex")}
            aria-label={t("search.regex")}
            aria-pressed={regexOn}
            onClick={() => setRegexOn((v) => !v)}
          >
            <Regex size={14} />
          </button>
          <button
            className={"sp-toggle" + (replaceOn ? " on" : "")}
            title={t("search.replace")}
            aria-label={t("search.replace")}
            aria-pressed={replaceOn}
            onClick={() => setReplaceOn((v) => !v)}
          >
            <Replace size={14} />
          </button>
        </div>
        {regexError && (
          <div className="sp-regex-error">{t("search.regexInvalid", { msg: regexError })}</div>
        )}
        {replaceOn && (
          <div className="sp-query-row">
            <input
              className="cp-palette-input"
              placeholder={t("search.replacePh")}
              value={repl}
              onChange={(e) => setRepl(e.target.value)}
              onKeyDown={onListKey}
            />
            <button
              className="sp-replace-all"
              title={t("search.replaceAll")}
              disabled={replacing || totalRepl === 0}
              onClick={doReplaceAll}
            >
              <ReplaceAll size={14} />
              <span>
                {replacing
                  ? t("search.replacing")
                  : t("search.replaceAllN", { n: totalRepl })}
              </span>
            </button>
          </div>
        )}
        {truncated && <div className="sp-truncated">{t("search.truncated", { n: MAX_HITS })}</div>}
        <div className="cp-palette-list">
          {!root && <div className="cp-empty">{t("search.noWorkspace")}</div>}
          {root && needle.length < minLen && !searching && (
            <div className="cp-empty">{t("search.minChars")}</div>
          )}
          {searching && <div className="cp-empty">{t("search.searching")}</div>}
          {!searching && !regexError && root && needle.length >= minLen && hits.length === 0 && (
            <div className="cp-empty">{t("search.empty")}</div>
          )}
          {groups.map((g) => (
            <div key={g.path} className="sp-file-group">
              <div className="sp-file-head">
                <span className="sp-file-name">{g.path}</span>
                <span className="sp-file-count">
                  {t("search.matchesInFile", { n: counts[g.path] ?? g.items.length })}
                </span>
              </div>
              {g.items.map(({ hit, index }) => (
                <button
                  key={`${hit.path}:${hit.line}:${index}`}
                  className={"cp-palette-row" + (index === sel ? " selected" : "")}
                  onMouseEnter={() => setSel(index)}
                  onClick={() => openHit(index)}
                >
                  <span className="sp-hit-line">{hit.line}</span>
                  <span className="search-preview">{hit.preview}</span>
                </button>
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}