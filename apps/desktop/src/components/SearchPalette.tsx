import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../i18n";
import { useUIStore } from "../stores/uiStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import { revealAt, setSearchMatches } from "../editor/editorApi";

interface SearchHit {
  path: string;
  line: number;
  column: number;
  preview: string;
  match_len: number;
}

/// Cerca de text a TOT el projecte (⇧⌘F), com VS Code. L'usuari escriu i,
/// amb una petita pausa, el backend retorna coincidències; al clicar-ne una,
/// s'obre el fitxer i el cursor salta a la línia i selecciona el text trobat.
export default function SearchPalette() {
  const show = useUIStore((s) => s.showSearch);
  const setShow = useUIStore((s) => s.setShowSearch);
  const root = useWorkspaceStore((s) => s.root);
  const loadFile = useWorkspaceStore((s) => s.loadFile);
  const { t } = useT();

  const [q, setQ] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [sel, setSel] = useState(0);
  const [searching, setSearching] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (show) {
      setSel(0);
      const id = setTimeout(() => inputRef.current?.focus(), 30);
      return () => clearTimeout(id);
    }
    return undefined;
  }, [show]);

  // Cerca amb retard (debounce) mentre s'escriu: 2 caràcters mínim.
  useEffect(() => {
    if (!show || !root) return undefined;
    const needle = q.trim();
    if (needle.length < 2) {
      setHits([]);
      return undefined;
    }
    const timer = setTimeout(() => {
      setSearching(true);
      invoke<SearchHit[]>("search_files", { query: needle, limit: 120 })
        .then((h) => {
          setHits(h);
          setSel(0);
        })
        .catch(() => setHits([]))
        .finally(() => setSearching(false));
    }, 350);
    return () => clearTimeout(timer);
  }, [q, show, root]);

  if (!show) return null;

  const openHit = (i: number) => {
    const h = hits[i];
    if (!h || !root) return;
    const abs = `${root.replace(/\/+$/, "")}/${h.path}`;
    void loadFile(abs);
    setShow(false);
    // Registrem la SESSIÓ de cerca: totes les posicions de la paraula dins
    // aquest fitxer. Així queda ressaltada i F3/⇧F3 hi pot navegar.
    const needle = q.trim().toLowerCase();
    const content = useWorkspaceStore.getState().openFile?.content ?? h.preview;
    const ranges: {
      startLineNumber: number;
      startColumn: number;
      endLineNumber: number;
      endColumn: number;
    }[] = [];
    content.split("\n").forEach((ln, idx) => {
      const low = ln.toLowerCase();
      let p = low.indexOf(needle);
      while (p !== -1) {
        ranges.push({
          startLineNumber: idx + 1,
          startColumn: p + 1,
          endLineNumber: idx + 1,
          endColumn: p + 1 + needle.length,
        });
        p = low.indexOf(needle, p + Math.max(1, needle.length));
      }
    });
    // Donem temps al carregament del fitxer i al muntatge de Monaco abans
    // de saltar a la línia i començar el ressaltat.
    setTimeout(() => {
      setSearchMatches(ranges);
      revealAt(h.line, h.column, h.match_len);
    }, 350);
  };

  return (
    <div className="modal-overlay" onClick={() => setShow(false)}>
      <div className="modal cp-palette" onClick={(e) => e.stopPropagation()}>
        <input
          ref={inputRef}
          className="cp-palette-input"
          placeholder={t("search.placeholder")}
          value={q}
          onChange={(e) => {
            setQ(e.target.value);
            setSel(0);
          }}
          onKeyDown={(e) => {
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
          }}
        />
        <div className="cp-palette-list">
          {!root && <div className="cp-empty">{t("search.noWorkspace")}</div>}
          {root && q.trim().length < 2 && !searching && (
            <div className="cp-empty">{t("search.minChars")}</div>
          )}
          {searching && <div className="cp-empty">{t("search.searching")}</div>}
          {!searching && root && q.trim().length >= 2 && hits.length === 0 && (
            <div className="cp-empty">{t("search.empty")}</div>
          )}
          {hits.map((h, i) => (
            <button
              key={`${h.path}:${h.line}:${i}`}
              className={"cp-palette-row" + (i === sel ? " selected" : "")}
              onMouseEnter={() => setSel(i)}
              onClick={() => openHit(i)}
            >
              <span style={{ opacity: 0.6, fontSize: 11, marginRight: 8 }}>
                {h.path}:{h.line}
              </span>
              <span className="search-preview">{h.preview}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
