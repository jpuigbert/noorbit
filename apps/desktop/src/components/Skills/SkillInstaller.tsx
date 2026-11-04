import { useEffect, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import {
  Github,
  Link2,
  FileArchive,
  FolderOpen,
  Trash2,
  RefreshCw,
  RotateCcw,
  Download,
  Search,
  Code2,
  FolderPlus,
} from "lucide-react";
import { useT } from "../../i18n";
import { useSkillStore } from "../../stores/skillStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { SKILL_CATALOG, searchSkillsGitHub, type SkillSearchResult } from "./skillCatalog";

type Source = "github" | "url" | "zip" | "local";

export default function SkillInstaller() {
  const { t } = useT();
  const {
    skills,
    busy,
    lastError,
    load,
    reload,
    installGithub,
    installUrl,
    installZip,
    installLocal,
    uninstall,
    restoreBuiltins,
    exportExternal,
  } = useSkillStore();
  const workspaceRoot = useWorkspaceStore((s) => s.root);

  const [source, setSource] = useState<Source>("github");
  const [repo, setRepo] = useState("");
  const [value, setValue] = useState("");
  const [info, setInfo] = useState<string | null>(null);
  // ── Cercador de skills: recerca en viu a GitHub + catàleg fix ──
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SkillSearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  // ── Importació de VS Code (comandes + fragments de les extensions) ──
  const [vscodeBusy, setVscodeBusy] = useState(false);

  useEffect(() => {
    load();
  }, [load]);

  const doInstall = async () => {
    if (source === "github") {
      if (!repo.trim()) return;
      const ok = await installGithub(repo.trim());
      if (ok) setRepo("");
    } else {
      if (!value.trim()) return;
      const ok =
        source === "url"
          ? await installUrl(value.trim())
          : source === "zip"
            ? await installZip(value.trim())
            : await installLocal(value.trim());
      if (ok) setValue("");
    }
  };

  const pickZip = async () => {
    const sel = await open({
      multiple: false,
      filters: [{ name: "ZIP", extensions: ["zip"] }],
    });
    if (typeof sel === "string") setValue(sel);
  };

  const pickFolder = async () => {
    const sel = await open({ directory: true });
    if (typeof sel === "string") setValue(sel);
  };

  const doRestore = async () => {
    setInfo(null);
    const ok = await restoreBuiltins();
    setInfo(ok ? t("skills.restoreOk") : null);
  };

  // Recerca de skills: GitHub en viu amb replieg al catàleg curat.
  const doSearch = async (raw?: string) => {
    const q = (raw ?? query).trim();
    if (!q) return;
    setSearching(true);
    setResults([]);
    try {
      const found = await searchSkillsGitHub(q);
      setResults(found.length > 0 ? found : SKILL_CATALOG);
    } catch {
      // Sense xarxa o límit d'API: el catàleg curt sempre respon alguna cosa.
      const hay = q.toLowerCase();
      setResults(SKILL_CATALOG.filter((c) => (c.name + c.description + c.repo).toLowerCase().includes(hay)));
    } finally {
      setSearching(false);
    }
  };

  const installResult = async (r: SkillSearchResult) => {
    setInfo(null);
    const ok = await installGithub(r.repo);
    setInfo(ok ? t("skills.installed_ok") : null);
  };

  // Importaquí les extensions instal·lades de VS Code com a skills: comandes
  // i fragments dins `vscode/<ext>/SKILL.md`, disponibles a la IA immediat.
  const importVscode = async () => {
    setVscodeBusy(true);
    setInfo(t("skills.vscodeBusy"));
    try {
      const rep = await invoke<{ scanned: number; imported: number; details: string[] }>(
        "vscode_import_skills",
      );
      setInfo(
        rep.imported > 0
          ? t("skills.vscodeOk", { n: rep.imported })
          : t("skills.vscodeEmpty"),
      );
      await reload();
    } catch {
      setInfo(t("skills.vscodeEmpty"));
    } finally {
      setVscodeBusy(false);
    }
  };

  const doExport = async (dir: string, okKey: string) => {
    setInfo(null);
    const n = await exportExternal(dir);
    if (n !== null) setInfo(t(okKey, { n }));
  };

  const exportTargets: { key: string; label: string; dir: string; disabled?: boolean }[] = [
    { key: "opencode", label: t("skills.exportOpencode"), dir: "~/.config/opencode/skills" },
    { key: "claude", label: t("skills.exportClaude"), dir: "~/.claude/skills" },
    { key: "agents", label: t("skills.exportAgents"), dir: "~/.agents/skills" },
    {
      key: "copilot",
      label: t("skills.exportCopilot"),
      dir: workspaceRoot ? `${workspaceRoot}/.github/skills` : "",
      disabled: !workspaceRoot,
    },
  ];

  const tabs: { key: Source; label: string; icon: ReactNode }[] = [
    { key: "github", label: t("skills.github"), icon: <Github size={13} /> },
    { key: "url", label: t("skills.url"), icon: <Link2 size={13} /> },
    { key: "zip", label: t("skills.zip"), icon: <FileArchive size={13} /> },
    { key: "local", label: t("skills.local"), icon: <FolderOpen size={13} /> },
  ];

  return (
    <div className="skill-installer">
      <div className="si-tabs">
        {tabs.map((tab) => (
          <button
            key={tab.key}
            className={"si-tab" + (source === tab.key ? " active" : "")}
            onClick={() => setSource(tab.key)}
          >
            {tab.icon} {tab.label}
          </button>
        ))}
      </div>

      {/* ── CERCADOR: GitHub en viu + catàleg d'arrencada ── */}
      <div className="field">
        <label>{t("skills.searchTitle")}</label>
        <div className="si-pick">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("skills.searchHint")}
            onKeyDown={(e) => e.key === "Enter" && void doSearch()}
          />
          <button className="btn sm" disabled={searching} onClick={() => void doSearch()}>
            <Search size={12} /> {searching ? t("common.loading") : t("skills.searchTitle")}
          </button>
        </div>
      </div>
      {results.length > 0 && (
        <div className="si-results">
          {results.map((r) => (
            <div key={r.repo} className="si-result">
              <div style={{ minWidth: 0, flex: 1 }}>
                <div className="si-result-name">
                  {r.name}
                  {r.stars != null && r.stars > 0 ? ` · ★ ${r.stars}` : ""}
                </div>
                <div className="si-result-desc">{r.description}</div>
              </div>
              <button className="btn sm" disabled={busy} onClick={() => void installResult(r)}>
                <FolderPlus size={12} /> {t("skills.install")}
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="si-form">
        {source === "github" && (
          <div className="field">
            <label>{t("skills.repo")}</label>
            <input
              value={repo}
              onChange={(e) => setRepo(e.target.value)}
              placeholder={t("skills.repoHint")}
              onKeyDown={(e) => e.key === "Enter" && doInstall()}
            />
          </div>
        )}
        {source === "url" && (
          <div className="field">
            <label>{t("skills.pathOrUrl")}</label>
            <input
              value={value}
              onChange={(e) => setValue(e.target.value)}
              placeholder="https://…/skill.zip"
              onKeyDown={(e) => e.key === "Enter" && doInstall()}
            />
          </div>
        )}
        {source === "zip" && (
          <div className="field">
            <label>{t("skills.zip")}</label>
            <div className="si-pick">
              <input value={value} onChange={(e) => setValue(e.target.value)} placeholder="/ruta/al/skill.zip" />
              <button className="btn sm" onClick={pickZip}>
                {t("common.browse")}
              </button>
            </div>
          </div>
        )}
        {source === "local" && (
          <div className="field">
            <label>{t("skills.local")}</label>
            <div className="si-pick">
              <input value={value} onChange={(e) => setValue(e.target.value)} placeholder="/ruta/a/la/carpeta" />
              <button className="btn sm" onClick={pickFolder}>
                {t("common.browse")}
              </button>
            </div>
          </div>
        )}
        <button
          className="btn primary sm"
          onClick={doInstall}
          disabled={busy || (source === "github" ? !repo.trim() : !value.trim())}
        >
          {busy ? t("common.loading") : t("skills.install")}
        </button>
        {lastError && (
          <div className="si-error">
            {lastError === "export"
              ? t("skills.exportFail")
              : lastError === "restore"
                ? t("skills.restoreFail")
                : t("skills.installFail")}
          </div>
        )}
        {info && <div className="si-info">{info}</div>}
      </div>

      <div className="si-builtin">
        <div className="si-builtin-head">
          <span>{t("skills.builtinTitle")}</span>
          <button className="btn sm" onClick={doRestore} disabled={busy} title={t("skills.restore")}>
            <RotateCcw size={12} /> {t("skills.restore")}
          </button>
        </div>
        <div className="si-export">
          <Download size={12} />
          <span>{t("skills.export")}</span>
          {exportTargets.map((tg) => (
            <button
              key={tg.key}
              className="btn sm"
              disabled={busy || tg.disabled}
              title={tg.dir}
              onClick={() => doExport(tg.dir, "skills.exportOk")}
            >
              {tg.label}
            </button>
          ))}
        </div>
        {/* COMPATIBILITAT VS CODE: porta les comandes i fragments de les seves
            extensions cap a skills de NoOrbit (carpeta vscode/). */}
        <div className="si-export">
          <Code2 size={12} />
          <span>VS Code · Qoder</span>
          <button className="btn sm" disabled={vscodeBusy} onClick={() => void importVscode()}>
            {vscodeBusy ? t("skills.vscodeBusy") : t("skills.vscodeImport")}
          </button>
        </div>
      </div>

      <div className="si-list-head">
        <span>{t("skills.installed")}</span>
        <button className="icon-btn" title={t("skills.reload")} onClick={() => reload()}>
          <RefreshCw size={13} />
        </button>
      </div>
      <div className="si-list">
        {skills.length === 0 ? (
          <div className="empty-hint">{t("skills.empty")}</div>
        ) : (
          skills.map((s) => (
            <div key={s.id} className="si-row">
              <div>
                <div className="si-name">
                  {s.name}
                  {s.builtin && <span className="si-badge">{t("skills.builtinBadge")}</span>}
                </div>
                {s.description && <div className="si-desc">{s.description}</div>}
              </div>
              <button className="icon-btn" title={t("skills.uninstall")} onClick={() => uninstall(s.id)}>
                <Trash2 size={13} />
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
