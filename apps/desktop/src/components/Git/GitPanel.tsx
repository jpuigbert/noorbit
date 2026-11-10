import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DiffEditor } from "@monaco-editor/react";
import { useMonacoReady } from "../../editor/monReady";
import {
  GitBranch, RefreshCw, ArrowDown, ArrowUp, Check, Archive, History, Undo2,
  Cloud, CloudUpload, Repeat,
} from "lucide-react";
import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";

interface Change {
  path: string;
  status: string;
  staged: boolean;
}
interface GitStatus {
  repo: boolean;
  branch: string;
  changes: Change[];
}
interface FileDiff {
  path: string;
  patch: string;
  head_content: string;
  working_content: string;
  /// Controla si es mostra també el pedaç en text pla (diff unificat de git),
  /// a més del diff visual costat a costat de Monaco. No ve del backend.
  showPatch?: boolean;
}
interface StashEntry {
  index: number;
  message: string;
}
/// On apunta el projecte al núvol (remot `origin`).
interface RemoteInfo {
  remote: boolean;
  url: string | null;
}
/// Quant queda per sincronitzar amb l'equip (altres dispositius).
interface SyncStatus {
  remote: boolean;
  ahead: number;
  behind: number;
}

/// Dedueix el llenguatge de Monaco per a l'extensió del fitxer (per al color
/// del diff, com fa VS Code).
function langForPath(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    ts: "typescript",
    tsx: "typescript",
    js: "javascript",
    jsx: "javascript",
    json: "json",
    py: "python",
    rs: "rust",
    css: "css",
    scss: "scss",
    html: "html",
    md: "markdown",
    toml: "ini",
    yml: "yaml",
    yaml: "yaml",
    sh: "shell",
    bash: "shell",
    c: "c",
    h: "c",
    cpp: "cpp",
  };
  return map[ext] ?? "plaintext";
}

const STATUS_LABEL: Record<string, string> = {
  M: "M",
  A: "A",
  D: "D",
  U: "U",
  R: "R",
  C: "C",
};

/// Panell de Git integrat, com la vista Source Control de VS Code:
/// llista de fitxers canviats, diff d'un fitxer, commit, push i pull.
export default function GitPanel() {
  // El DiffEditor de Monaco es carrega ÀJUS: sense cap fitxer de codi obert
  // mai, el seu chunk (3,9 MB) no arriba a descarregar-se a l'arrencada.
  const monReady = useMonacoReady();
  const { t } = useT();
  const root = useWorkspaceStore((s) => s.root);
  const [status, setStatus] = useState<GitStatus | null>(null);
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [diff, setDiff] = useState<FileDiff | null>(null);
  // Stash + gràfic de branques (funcions estil VS Code).
  const [stashes, setStashes] = useState<StashEntry[]>([]);
  const [showGraph, setShowGraph] = useState(false);
  const [log, setLog] = useState("");
  // ── Núvol d'equip: més d'un NoOrbit (dispositius diferents) editant el
  // mateix projecte. Sincronització manual d'1 clic O automàtica, a decisió
  // de cada un (el flag es recorda per projecte).
  const [remote, setRemote] = useState<RemoteInfo | null>(null);
  const [sync, setSync] = useState<SyncStatus | null>(null);
  const [cloudUrl, setCloudUrl] = useState("");
  const [auto, setAuto] = useState(() => {
    try {
      return root ? localStorage.getItem(`noorbit.cloudAuto:${root}`) === "1" : false;
    } catch {
      return false;
    }
  });
  const [lastSync, setLastSync] = useState<number>(0);
  const busyRef = useRef(false);

  const refresh = useCallback(async () => {
    if (!root) {
      setStatus(null);
      return;
    }
    try {
      const st = await invoke<GitStatus>("git_status");
      setStatus(st);
      // El llistat de stash només té sentit dins d'un repositori.
      setStashes(st.repo ? await invoke<StashEntry[]>("git_stash_list") : []);
      if (st.repo) {
        setRemote(await invoke<RemoteInfo>("git_remote_get"));
        setSync(await invoke<SyncStatus>("git_sync_status"));
      } else {
        setRemote(null);
        setSync(null);
      }
    } catch (e) {
      setStatus(null);
      setFeedback(String(e));
    }
  }, [root]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const act = async (fn: () => Promise<unknown>, done: string): Promise<boolean> => {
    setBusy(true);
    busyRef.current = true;
    setFeedback(null);
    try {
      await fn();
      setFeedback(done);
      await refresh();
      return true;
    } catch (e) {
      setFeedback(`✗ ${String(e)}`);
      await refresh();
      return false;
    } finally {
      setBusy(false);
      busyRef.current = false;
    }
  };

  // MODE AUTOMÀTIC d'equip: cada 90 s fa una passada completa (commit + baixa
  // + pujada) mentre el projecte tingui remot i ningú estigui fent res més.
  // La decisió és de cada un: el flag es recorda per projecte.
  useEffect(() => {
    try {
      if (root) localStorage.setItem(`noorbit.cloudAuto:${root}`, auto ? "1" : "0");
    } catch {
      /* sense persistència: es queda a la sessió */
    }
    if (!auto) return;
    const tick = window.setInterval(() => {
      if (busyRef.current || !remote?.remote) return;
      void act(() => invoke("git_sync", { message: null }), t("git.cloudDone")).then((ok) => {
        if (ok) setLastSync(Date.now());
      });
    }, 90_000);
    return () => window.clearInterval(tick);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [auto, remote?.remote, refresh]);

  if (!root) {
    return <div className="empty-hint" style={{ margin: 16 }}>{t("git.noProject")}</div>;
  }
  if (status && !status.repo) {
    return (
      <div className="git-panel">
        <div className="empty-hint" style={{ margin: "24px 16px" }}>
          {t("git.notRepo")}
        </div>
        <div style={{ padding: "0 16px 16px" }}>
          <button
            className="btn sm"
            disabled={busy}
            onClick={() => act(() => invoke("git_init"), t("git.inited"))}
          >
            <GitBranch size={12} /> {t("git.init")}
          </button>
        </div>
        {feedback && <div className="git-feedback">{feedback}</div>}
      </div>
    );
  }

  const openDiff = (c: Change) =>
    act(async () => {
      const abs = `${root.replace(/\/+$/, "")}/${c.path}`;
      setDiff(await invoke<FileDiff>("git_diff_file", { path: abs }));
    }, "");

  /// Carrega (o amaga) el gràfic ASCII de branques, com la vista de història
  /// de VS Code. Es demana a `git log --graph` i es mostra tal com ve.
  const toggleGraph = async () => {
    if (showGraph) {
      setShowGraph(false);
      return;
    }
    setShowGraph(true);
    try {
      setLog(await invoke<string>("git_log", { limit: 80 }));
    } catch (e) {
      setLog(String(e));
    }
  };

  return (
    <div className="git-panel">
      <div className="git-head">
        <span className="git-branch">
          <GitBranch size={12} /> {status?.branch ?? "?"}
        </span>
        <div className="git-tools">
          <button className="btn sm" title={t("git.refresh")} onClick={() => void refresh()}>
            <RefreshCw size={12} />
          </button>
          <button
            className="btn sm"
            title={t("git.pull")}
            disabled={busy}
            onClick={() => act(() => invoke<string>("git_pull"), t("git.pulled"))}
          >
            <ArrowDown size={12} />
          </button>
          <button
            className="btn sm"
            title={t("git.push")}
            disabled={busy}
            onClick={() => act(() => invoke<string>("git_push"), t("git.pushed"))}
          >
            <ArrowUp size={12} />
          </button>
          <button
            className="btn sm"
            title={t("git.stash")}
            disabled={busy}
            onClick={() =>
              act(() => invoke<string>("git_stash", { message: msg || null }), t("git.stashed"))
            }
          >
            <Archive size={12} />
          </button>
          <button
            className="btn sm"
            title={t("git.stashPop")}
            disabled={busy || stashes.length === 0}
            onClick={() => act(() => invoke<string>("git_stash_pop", { index: 0 }), t("git.popped"))}
          >
            <Undo2 size={12} />
          </button>
          <button
            className={"btn sm" + (showGraph ? " active" : "")}
            title={t("git.graph")}
            onClick={() => void toggleGraph()}
          >
            <History size={12} />
          </button>
        </div>
      </div>

      {/* ── NÚVOL D'EQUIP: més d'un NoOrbit (dispositius diferents) editant
          el MATEIX projecte. Connectes el repositori una vegada; després
          sincronitzes amb 1 clic o en automàtic — a decisió de cada un. ── */}
      <div className="git-cloud">
        {remote?.remote ? (
          <div className="git-cloud-line">
            <Cloud size={12} />
            <span className="git-cloud-url" title={remote.url ?? ""}>
              {remote.url?.replace(/^https?:\/\//, "")}
            </span>
            {(sync?.ahead ?? 0) > 0 && (
              <span className="git-cloud-badge up" title={t("git.cloudUpHint")}>
                ↑ {sync!.ahead}
              </span>
            )}
            {(sync?.behind ?? 0) > 0 && (
              <span className="git-cloud-badge down" title={t("git.cloudDownHint")}>
                ↓ {sync!.behind}
              </span>
            )}
            <span className="git-cloud-spacer" />
            <button
              className="btn primary sm"
              disabled={busy}
              title={t("git.cloudSyncHint")}
              onClick={() =>
                void act(() => invoke("git_sync", { message: msg || null }), t("git.cloudDone")).then(
                  (ok) => {
                    if (ok) setLastSync(Date.now());
                  }
                )
              }
            >
              <CloudUpload size={12} /> {t("git.cloudSync")}
            </button>
            <button
              className={"btn sm" + (auto ? " active" : "")}
              title={t("git.cloudAutoHint")}
              onClick={() => setAuto(!auto)}
            >
              <Repeat size={12} /> {t("git.cloudAuto")}
            </button>
            {lastSync > 0 && (
              <span className="git-cloud-ago" title={t("git.cloudLast")}>
                {t("git.cloudLast")} {new Date(lastSync).toLocaleTimeString()}
              </span>
            )}
          </div>
        ) : (
          <div className="git-cloud-line">
            <Cloud size={12} />
            <input
              className="input sm git-cloud-input"
              placeholder={t("git.cloudUrlPh")}
              value={cloudUrl}
              onChange={(e) => setCloudUrl(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && cloudUrl.trim() && !busy) {
                  void act(() => invoke("git_remote_set", { url: cloudUrl.trim() }), t("git.cloudConnected"));
                }
              }}
            />
            <button
              className="btn primary sm"
              disabled={busy || !cloudUrl.trim()}
              title={t("git.cloudConnectHint")}
              onClick={() =>
                void act(
                  () => invoke("git_remote_set", { url: cloudUrl.trim() }),
                  t("git.cloudConnected")
                )
              }
            >
              <Check size={12} /> {t("git.cloudConnect")}
            </button>
          </div>
        )}
      </div>

      {diff && !monReady ? (
        <div className="git-diff">
          <div className="git-diff-head">
            <span className="git-diff-path">{diff.path}</span>
            <button className="btn sm" onClick={() => setDiff(null)}>
              {t("common.close")}
            </button>
          </div>
        </div>
      ) : diff ? (
        <div className="git-diff">
          <div className="git-diff-head">
            <span className="git-diff-path">{diff.path}</span>
            <button className="btn sm" onClick={() => setDiff(null)}>
              {t("common.close")}
            </button>
          </div>
          <div className="git-diff-editor">
            <DiffEditor
              height="100%"
              language={langForPath(diff.path)}
              original={diff.head_content}
              modified={diff.working_content}
              theme={useUIStore.getState().theme === "light" ? "light" : "vs-dark"}
              options={{
                readOnly: true,
                renderSideBySide: true,
                renderOverviewRuler: false,
                minimap: { enabled: false },
                fontSize: 12,
                scrollBeyondLastLine: false,
                automaticLayout: true,
              }}
            />
          </div>
          <button
            className="btn sm"
            style={{ margin: "6px 10px" }}
            onClick={() => setDiff({ ...diff, showPatch: !diff.showPatch })}
          >
            {t("git.showPatch")}
          </button>
          {diff.showPatch ? (
            <pre className="git-patch">
              {diff.patch === "" ? t("git.untrackedDiff") : diff.patch}
            </pre>
          ) : null}
        </div>
      ) : (
        <div className="git-changes">
          {(status?.changes.length ?? 0) === 0 && (
            <div className="empty-hint" style={{ margin: 16 }}>{t("git.clean")}</div>
          )}
          {status?.changes.map((c) => (
            <button key={c.path} className="git-row" onClick={() => void openDiff(c)}>
              <span className={"git-badge git-" + c.status}>{STATUS_LABEL[c.status] ?? "?"}</span>
              <span className="git-path">{c.path}</span>
            </button>
          ))}
        </div>
      )}

      {showGraph && !diff && (
        <div className="git-graph">
          <pre className="git-graph-pre">{log}</pre>
        </div>
      )}

      {stashes.length > 0 && (
        <div className="git-stashes">
          <div className="git-stashes-title">
            <Archive size={11} /> {t("git.stashCount", { n: stashes.length })}
          </div>
          {stashes.map((s) => (
            <div key={s.index} className="git-stash-row" title={s.message}>
              <span className="git-stash-msg">stash@{`{${s.index}}`}: {s.message}</span>
              <button
                className="btn sm"
                disabled={busy}
                onClick={() =>
                  act(() => invoke<string>("git_stash_pop", { index: s.index }), t("git.popped"))
                }
              >
                <Undo2 size={11} /> {t("git.popThis")}
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="git-commit">
        <textarea
          className="git-msg"
          rows={2}
          placeholder={t("git.message")}
          value={msg}
          onChange={(e) => setMsg(e.target.value)}
        />
        <button
          className="btn sm primary"
          disabled={busy || !msg.trim() || (status?.changes.length ?? 0) === 0}
          onClick={() =>
            act(async () => {
              await invoke("git_commit", { message: msg });
              setMsg("");
            }, t("git.committed"))
          }
        >
          <Check size={12} /> {t("git.commit")}
        </button>
      </div>

      {feedback && <div className="git-feedback">{feedback}</div>}
    </div>
  );
}
