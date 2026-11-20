import { useEffect, useMemo, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  AlertTriangle,
  Clapperboard,
  Download,
  ExternalLink,
  Eye,
  FileVideo,
  Folder,
  Images,
  Loader2,
  Play,
  Plus,
  RefreshCw,
  Scissors,
  Sparkles,
  Square,
  Wrench,
  X,
} from "lucide-react";
import { useT } from "../../i18n";
import WorkProgress from "../WorkProgress";
import { customMinutes, formatDuration, type TimeUnit } from "../../utils/workTime";
import { useUIStore } from "../../stores/uiStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useAIStore } from "../../stores/aiStore";
import { useProviderStore } from "../../stores/providerStore";
import {
  imageAsDataUrl,
  useVideoStudioStore,
  type SourceInfo,
} from "../../stores/videoStudioStore";
import "./VideoStudioModal.css";

/**
 * MENÚ «VÍDEO»: muntatge professional Blender → Kdenlive/ffmpeg dirigit per IA.
 *
 * Quatre pestanyes, cadascuna amb la seva feina real:
 *  - «Director» : la IA ho fa TOT (mira els clips amb visió, escriu el guió,
 *    es critica amb rúbrica professional i renderitza). Aquí és on es tria el
 *    TEMPS DE TREBALL — els 40 minuts de criteri que abans eren humans.
 *  - «Orígens»  : inventari mesurat (durada, mida, lluentor, moviment, plans
 *    morts, silencis) amb la graella de fotogrames que la IA ha vist.
 *  - «Muntatge» : l'EDL clip a clip, editable a mà o en JSON.
 *  - «Sortida»  : previsualització, render final, ordre `ffmpeg` real,
 *    comprovació i export `.kdenlive` per polir-ho a la GUI.
 *
 * El panell NO és el propietari del treball: render i director moren en el
 * backend i segueixen en segon pla amb el menú tancat (vegeu
 * `stores/videoStudioStore.ts`).
 */

const LOCAL = "local|";
const PROV = "prov|";
const NATIVE = "nat|";
const NATIVE_ID = "noorbit_native";
const CUSTOM = -1;

/// Pressupost de treball de la IA. 40 min és el preset per defecte: prou per
/// mirar els clips, escriure el guió, criticar-lo dues vegades i renderitzar.
const BUDGETS = [10, 20, 40, 60, 120, 240, 480, 1440, CUSTOM];

/// Durada OBJECTIU del vídeo (no del treball): 0 = la que decideixi la IA.
const TARGETS = [0, 15, 30, 45, 60, 90, 120, 300, 600, CUSTOM];

const RESOLUTIONS: [string, number, number][] = [
  ["auto", 0, 0],
  ["1920×1080", 1920, 1080],
  ["1080×1920 (vertical)", 1080, 1920],
  ["1280×720", 1280, 720],
  ["3840×2160 (4K)", 3840, 2160],
];

const FPS = [24, 25, 30, 50, 60];
const PRESETS = ["ultrafast", "veryfast", "medium", "slow"];
const CONTAINERS = ["mp4", "mov", "webm"];

type Tab = "director" | "sources" | "edl" | "out";

/// Un número 0..1 en percentatge pintable (les mesures del `ff` ja venen
/// normalitzades; el moviment és molt petit i s'amplifica per veure'l).
const pct = (v: number, gain = 1) =>
  `${Math.max(0, Math.min(100, Math.round(v * 100 * gain)))}%`;

const mb = (bytes: number) => (bytes > 0 ? `${(bytes / 1048576).toFixed(1)} MB` : "—");

export default function VideoStudioModal() {
  const { t } = useT();
  const show = useUIStore((s) => s.showVideoStudio);
  const setShow = useUIStore((s) => s.setShowVideoStudio);
  const wsRoot = useWorkspaceStore((s) => s.root);

  const models = useAIStore((s) => s.models);
  const loadModels = useAIStore((s) => s.loadModels);
  const providers = useProviderStore((s) => s.providers);
  const loadProviders = useProviderStore((s) => s.load);

  // Estat CRU del store (referències estables: res de llistes noves en pintar).
  const tools = useVideoStudioStore((s) => s.tools);
  const scan = useVideoStudioStore((s) => s.scan);
  const edl = useVideoStudioStore((s) => s.edl);
  const director = useVideoStudioStore((s) => s.director);
  const render = useVideoStudioStore((s) => s.render);
  const plan = useVideoStudioStore((s) => s.plan);
  const notes = useVideoStudioStore((s) => s.notes);
  const transitions = useVideoStudioStore((s) => s.transitions);
  const busy = useVideoStudioStore((s) => s.busy);
  const scanning = useVideoStudioStore((s) => s.scanning);

  const loadTools = useVideoStudioStore((s) => s.loadTools);
  const hydrate = useVideoStudioStore((s) => s.hydrate);
  const scanProject = useVideoStudioStore((s) => s.scanProject);
  const startDirector = useVideoStudioStore((s) => s.startDirector);
  const stopDirector = useVideoStudioStore((s) => s.stopDirector);
  const renderEdl = useVideoStudioStore((s) => s.renderEdl);
  const stopRender = useVideoStudioStore((s) => s.stopRender);
  const buildPlan = useVideoStudioStore((s) => s.buildPlan);
  const saveEdl = useVideoStudioStore((s) => s.saveEdl);
  const parseEdl = useVideoStudioStore((s) => s.parseEdl);
  const makeAutoEdl = useVideoStudioStore((s) => s.makeAutoEdl);
  const addSource = useVideoStudioStore((s) => s.addSource);
  const exportProject = useVideoStudioStore((s) => s.exportProject);
  const openFile = useVideoStudioStore((s) => s.openFile);
  const openKdenlive = useVideoStudioStore((s) => s.openKdenlive);
  const installEditor = useVideoStudioStore((s) => s.installEditor);
  const verifyLast = useVideoStudioStore((s) => s.verifyLast);

  const [tab, setTab] = useState<Tab>("director");
  /// El projecte és el del workspace, però es pot canviar sense tancar res:
  /// els muntatges sovint es fan sobre una carpeta de renders de Blender.
  const [dir, setDir] = useState<string | null>(null);
  const projectPath = dir ?? wsRoot ?? "";

  const [name, setName] = useState("");
  const [brief, setBrief] = useState("");
  const [ai, setAi] = useState("");
  const [budgetMin, setBudgetMin] = useState(40);
  const [customVal, setCustomVal] = useState(40);
  const [customUnit, setCustomUnit] = useState<TimeUnit>("min");
  const [targetOpt, setTargetOpt] = useState(0);
  const [targetCustom, setTargetCustom] = useState(60);
  const [resIdx, setResIdx] = useState(0);
  const [fps, setFps] = useState(25);
  const [crf, setCrf] = useState(20);
  const [preset, setPreset] = useState("medium");
  const [container, setContainer] = useState("mp4");
  const [critiques, setCritiques] = useState(2);
  const [silent, setSilent] = useState(false);
  const [openAfter, setOpenAfter] = useState(true);

  const [raw, setRaw] = useState("");
  const [verifyRep, setVerifyRep] = useState<unknown>(null);
  const [sheets, setSheets] = useState<Record<string, string>>({});
  const [err, setErr] = useState<string | null>(null);
  const [playing, setPlaying] = useState<string | null>(null);

  const working = director?.running === true;
  const rendering = render?.running === true;

  /// En obrir: eines + estat del que ja està corrent en segon pla + inventari.
  useEffect(() => {
    if (!show) return;
    useVideoStudioStore.getState().init();
    void loadModels();
    void loadProviders();
    void hydrate();
    void loadTools();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [show]);

  /// El títol del muntatge, la primera vegada, és el de la carpeta oberta.
  useEffect(() => {
    if (!name && projectPath) setName(projectPath.split("/").filter(Boolean).pop() ?? "");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectPath]);

  /// L'IA per defecte: una de local si n'hi ha; si no, la nativa (sempre
  /// funciona); els proveïdors en línia queden triables.
  useEffect(() => {
    if (ai) return;
    if (models.length > 0) {
      setAi(LOCAL + models[0].name);
      return;
    }
    setAi(NATIVE + NATIVE_ID);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [models]);

  const resolveAi = useMemo(() => {
    if (ai.startsWith(LOCAL)) return { provider: "ollama", model: ai.slice(LOCAL.length) };
    if (ai.startsWith(NATIVE)) {
      const nat = providers.find((x) => x.kind === "native");
      return { provider: NATIVE_ID, model: nat?.model };
    }
    if (ai.startsWith(PROV)) {
      const p = providers.find((x) => x.id === ai.slice(PROV.length));
      return { provider: p?.id, model: p?.model };
    }
    return { provider: undefined, model: undefined };
  }, [ai, providers]);

  const budgetSecs =
    (budgetMin === CUSTOM ? customMinutes(customVal, customUnit) : budgetMin) * 60;
  const targetS = targetOpt === CUSTOM ? targetCustom : targetOpt;

  /// En anar a «Orígens» es carreguen les graelles de fotogrames (PNG del
  /// projecte → `data:` URL): el que la IA ha vist per decidir els talls.
  useEffect(() => {
    if (!show || tab !== "sources") return;
    const todo = (scan?.sources ?? []).filter((s) => s.sheet && !sheets[s.sheet]);
    if (todo.length === 0) return;
    let live = true;
    void Promise.all(
      todo.map(async (s) => ({ key: s.sheet as string, url: await imageAsDataUrl(s.sheet as string) }))
    ).then((list) => {
      if (!live) return;
      setSheets((prev) => {
        const next = { ...prev };
        list.forEach((x) => (next[x.key] = x.url ?? ""));
        return next;
      });
    });
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [show, tab, scan]);

  /// L'editor JSON reflecteix sempre l'EDL desat (mentre no s'estigui escrivint).
  useEffect(() => {
    if (tab === "edl" && edl) setRaw(JSON.stringify(edl, null, 2));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab, edl?.name, edl?.clips.length]);

  const close = () => {
    setShow(false);
    setErr(null);
    // NO s'atura res: el director i el render segueixen en segon pla.
  };

  const browse = async () => {
    const sel = await open({ directory: true, multiple: false, defaultPath: wsRoot ?? undefined });
    if (typeof sel === "string") {
      setDir(sel);
      setSheets({});
      await scanProject(sel, true);
    }
  };

  const pickSource = async () => {
    if (!projectPath) {
      setErr(t("video.needProject"));
      return;
    }
    const sel = await open({
      multiple: false,
      directory: false,
      defaultPath: projectPath,
      filters: [{ name: "Vídeo", extensions: ["mp4", "mov", "webm", "mkv", "avi", "gif"] }],
    });
    if (typeof sel === "string") await addSource(projectPath, sel);
  };

  const doStart = async () => {
    setErr(null);
    if (!projectPath) {
      setErr(t("video.needProject"));
      return;
    }
    const [ , w, h] = RESOLUTIONS[resIdx];
    await startDirector({
      projectPath,
      name: name.trim() || "Muntatge",
      brief: brief.trim(),
      // Buit = que el director descobreixi els orígens del projecte.
      sources: [],
      budgetSecs,
      targetS,
      provider: resolveAi.provider,
      model: resolveAi.model,
      critiques,
      width: w || undefined,
      height: h || undefined,
      fps,
      crf,
      preset,
      container,
      silent,
    });
  };

  const doVerify = async () => {
    const out = render?.outPath ?? scan?.lastRender ?? null;
    if (!out || !edl) {
      setErr(t("video.needVerify"));
      return;
    }
    setVerifyRep(await verifyLast(out, edl));
  };

  const toolsChip = (label: string, ok: boolean, version?: string | null) => (
    <span className={"vs-chip " + (ok ? "ok" : "bad")} title={version ?? undefined}>
      <Wrench size={10} /> {label}
      {version ? ` ${version}` : ""}
    </span>
  );

  return (
    <div className="modal-overlay" onClick={close}>
      <div className="modal vs-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>
            <Clapperboard size={15} style={{ marginRight: 6, verticalAlign: -2 }} />
            {t("video.title")}
          </span>
          <button className="icon-btn" onClick={close}>
            <X size={16} />
          </button>
        </div>

        <div className="modal-body vs-body">
          {/* ── EINES + PROJECTE ──────────────────────────────────────────── */}
          <div className="vs-tools">
            {tools ? (
              <>
                {toolsChip("ffmpeg", tools.ffmpeg.ok, tools.ffmpeg.version)}
                {toolsChip("ffprobe", tools.ffprobe.ok, tools.ffprobe.version)}
                {toolsChip("melt", tools.melt.ok, tools.melt.version)}
                {toolsChip("Kdenlive", tools.kdenlive.ok, tools.kdenlive.version)}
                {tools.bundle && (
                  <span className="vs-chip" title={tools.bundle}>
                    <FileVideo size={10} /> {t("video.bundle")}
                  </span>
                )}
                {!tools.ready && (
                  <button
                    className="btn sm"
                    disabled={busy === "install"}
                    onClick={() => void installEditor("kdenlive")}
                  >
                    <Download size={12} /> {t("video.install")}
                  </button>
                )}
              </>
            ) : (
              <span>
                <Loader2 size={11} className="spin" /> {t("video.detecting")}
              </span>
            )}
          </div>

          <div className="vs-row" style={{ marginTop: 0 }}>
            <input
              readOnly
              value={projectPath}
              placeholder={t("video.dirPh")}
              style={{ flex: 1, minWidth: 220 }}
            />
            <button className="btn sm" onClick={() => void browse()}>
              <Folder size={12} /> {t("common.browse")}
            </button>
            <button
              className="btn sm"
              disabled={scanning || !projectPath}
              onClick={() => void scanProject(projectPath, true)}
            >
              {scanning ? <Loader2 size={12} className="spin" /> : <RefreshCw size={12} />}{" "}
              {t("video.scan")}
            </button>
          </div>

          {/* ── PESTANYES ─────────────────────────────────────────────────── */}
          <div className="vs-tabs" style={{ marginTop: 12 }}>
            {(
              [
                ["director", <Sparkles size={12} key="d" />, t("video.tabDirector")],
                ["sources", <Images size={12} key="s" />, t("video.tabSources")],
                ["edl", <Scissors size={12} key="e" />, t("video.tabEdl")],
                ["out", <Play size={12} key="o" />, t("video.tabOut")],
              ] as [Tab, ReactNode, string][]
            ).map(([k, icon, label]) => (
              <button
                key={k}
                className={"vs-tab" + (tab === k ? " on" : "")}
                onClick={() => setTab(k)}
              >
                {icon} {label}
              </button>
            ))}
          </div>

          {working && tab !== "director" && (
            <div className="vs-msg">
              <Loader2 size={11} className="spin" /> {t("video.directorRunning")}
            </div>
          )}

          {/* ══ DIRECTOR: la IA ho fa tot, amb pressupost de temps ══════════ */}
          {tab === "director" && (
            <>
              <p className="lp-hint">{t("video.intro")}</p>

              <div className="field">
                <label>{t("video.brief")}</label>
                <textarea
                  rows={4}
                  disabled={working}
                  value={brief}
                  placeholder={t("video.briefPh")}
                  onChange={(e) => setBrief(e.target.value)}
                />
                <div className="lp-hint" style={{ marginTop: 4 }}>
                  {t("video.briefHint")}
                </div>
              </div>

              <div className="vs-3">
                <div className="field">
                  <label>{t("video.name")}</label>
                  <input
                    value={name}
                    disabled={working}
                    onChange={(e) => setName(e.target.value)}
                  />
                </div>
                <div className="field">
                  <label>{t("video.time")}</label>
                  <select
                    className="lp-select"
                    value={budgetMin}
                    disabled={working || busy === "director"}
                    onChange={(e) => setBudgetMin(Number(e.target.value))}
                  >
                    {BUDGETS.map((m) => (
                      <option key={m} value={m}>
                        {m === CUSTOM ? t("work.custom") : `${m} ${t("work.unitMin")}`}
                      </option>
                    ))}
                  </select>
                </div>
                <div className="field">
                  <label>{t("video.target")}</label>
                  <select
                    className="lp-select"
                    value={targetOpt}
                    disabled={working || busy === "director"}
                    onChange={(e) => setTargetOpt(Number(e.target.value))}
                  >
                    {TARGETS.map((s) => (
                      <option key={s} value={s}>
                        {s === CUSTOM
                          ? t("work.custom")
                          : s === 0
                            ? t("video.targetAuto")
                            : `${s} s`}
                      </option>
                    ))}
                  </select>
                </div>
              </div>

              {budgetMin === CUSTOM && (
                <div className="vs-2">
                  <div className="field">
                    <label>{t("work.customTime")}</label>
                    <input
                      type="number"
                      min={1}
                      value={customVal}
                      disabled={working}
                      onChange={(e) => setCustomVal(Number(e.target.value))}
                    />
                  </div>
                  <div className="field">
                    <label>{t("work.unit")}</label>
                    <select
                      className="lp-select"
                      value={customUnit}
                      disabled={working}
                      onChange={(e) => setCustomUnit(e.target.value as TimeUnit)}
                    >
                      <option value="min">{t("work.unitMin")}</option>
                      <option value="h">{t("work.unitH")}</option>
                      <option value="d">{t("work.unitD")}</option>
                    </select>
                  </div>
                </div>
              )}
              {targetOpt === CUSTOM && (
                <div className="field" style={{ maxWidth: 220 }}>
                  <label>{t("video.target")}</label>
                  <input
                    type="number"
                    min={1}
                    max={14400}
                    value={targetCustom}
                    disabled={working}
                    onChange={(e) => setTargetCustom(Number(e.target.value))}
                  />
                </div>
              )}

              <div className="lp-hint" style={{ marginTop: 6 }}>
                {formatDuration(budgetSecs * 1000)} · {t("video.timeHint")}
              </div>

              <div className="vs-3" style={{ marginTop: 10 }}>
                <div className="field">
                  <label>{t("video.ai")}</label>
                  <select
                    className="lp-select"
                    value={ai}
                    disabled={working}
                    onChange={(e) => setAi(e.target.value)}
                  >
                    <optgroup label={t("projectWork.aiNativeGroup")}>
                      <option value={NATIVE + NATIVE_ID}>{t("projectWork.aiNativeOption")}</option>
                    </optgroup>
                    {models.length > 0 && (
                      <optgroup label={t("wpTheme.aiLocal")}>
                        {models.map((m) => (
                          <option key={m.name} value={LOCAL + m.name}>
                            {m.name}
                          </option>
                        ))}
                      </optgroup>
                    )}
                    {providers.filter((p) => p.kind !== "native").length > 0 && (
                      <optgroup label={t("wpTheme.aiCloud")}>
                        {providers
                          .filter((p) => p.kind !== "native")
                          .map((p) => (
                            <option key={p.id} value={PROV + p.id}>
                              {p.name} · {p.model}
                              {p.has_token ? "" : ` (${t("wpTheme.noToken")})`}
                            </option>
                          ))}
                      </optgroup>
                    )}
                  </select>
                </div>
                <div className="field">
                  <label>{t("video.critiques")}</label>
                  <input
                    type="number"
                    min={0}
                    max={8}
                    value={critiques}
                    disabled={working}
                    onChange={(e) => setCritiques(Number(e.target.value))}
                  />
                  <div className="lp-hint" style={{ marginTop: 4 }}>
                    {t("video.critiquesHint")}
                  </div>
                </div>
                <div className="field">
                  <label>{t("video.res")}</label>
                  <select
                    className="lp-select"
                    value={resIdx}
                    disabled={working}
                    onChange={(e) => setResIdx(Number(e.target.value))}
                  >
                    {RESOLUTIONS.map(([l], i) => (
                      <option key={l} value={i}>
                        {l}
                      </option>
                    ))}
                  </select>
                </div>
              </div>

              <div className="vs-3" style={{ marginTop: 10 }}>
                <div className="field">
                  <label>{t("video.fps")}</label>
                  <select
                    className="lp-select"
                    value={fps}
                    disabled={working}
                    onChange={(e) => setFps(Number(e.target.value))}
                  >
                    {FPS.map((f) => (
                      <option key={f} value={f}>
                        {f}
                      </option>
                    ))}
                  </select>
                </div>
                <div className="field">
                  <label>{t("video.crf")}</label>
                  <input
                    type="number"
                    min={10}
                    max={32}
                    value={crf}
                    disabled={working}
                    onChange={(e) => setCrf(Number(e.target.value))}
                  />
                </div>
                <div className="field">
                  <label>{t("video.preset")}</label>
                  <select
                    className="lp-select"
                    value={preset}
                    disabled={working}
                    onChange={(e) => setPreset(e.target.value)}
                  >
                    {PRESETS.map((p) => (
                      <option key={p} value={p}>
                        {p}
                      </option>
                    ))}
                  </select>
                </div>
              </div>

              <div className="vs-row">
                <label className="vs-check">
                  <input
                    type="checkbox"
                    checked={silent}
                    disabled={working}
                    onChange={(e) => setSilent(e.target.checked)}
                  />
                  {t("video.silent")}
                </label>
                <span className="field" style={{ marginLeft: "auto" }}>
                  <label>{t("video.container")}</label>
                  <select
                    className="lp-select"
                    value={container}
                    disabled={working}
                    onChange={(e) => setContainer(e.target.value)}
                  >
                    {CONTAINERS.map((c) => (
                      <option key={c} value={c}>
                        {c}
                      </option>
                    ))}
                  </select>
                </span>
              </div>

              {!working ? (
                <div className="vs-row">
                  <button
                    className="btn primary"
                    disabled={busy === "director" || !projectPath || !ai}
                    onClick={() => void doStart()}
                  >
                    {busy === "director" ? (
                      <Loader2 size={14} className="spin" />
                    ) : (
                      <Sparkles size={14} />
                    )}{" "}
                    {t("video.start")}
                  </button>
                  <span className="lp-hint">{scan?.sources.length ?? 0} {t("video.sourcesFound")}</span>
                </div>
              ) : (
                <div className="vs-row">
                  <button className="btn sm" onClick={() => void stopDirector()}>
                    <Square size={12} /> {t("video.stop")}
                  </button>
                </div>
              )}

              {director && (director.running || director.outPath || director.error) && (
                <div className="vs-work" style={{ marginTop: 12 }}>
                  <WorkProgress
                    running={director.running}
                    startedAtMs={(director.startedAt || 0) * 1000}
                    mode="time"
                    budgetMs={director.budgetMs}
                    finishedAtMs={director.finishedAt ? director.finishedAt * 1000 : null}
                    label={`${t("video.stage")} ${director.stage} · ${t("video.pass")} ${director.pass} · ${director.critique} ${t("video.critiqueN")}`}
                  />
                  {director.detail && <div className="lp-hint">{director.detail}</div>}
                  <div className="vs-meters" style={{ marginTop: 6 }}>
                    <span className="vs-meter">
                      {t("video.sourcesFound")}: <b>{director.sourcesFound}</b>
                    </span>
                    <span className="vs-meter">
                      {t("video.clipsN")}: <b>{director.clips}</b>
                    </span>
                    <span className="vs-meter">
                      {t("video.renderProgress")}: <b>{Math.round(director.progress * 100)}%</b>
                    </span>
                  </div>
                  {director.outPath && (
                    <div className="vs-row">
                      <ExternalLink size={12} />
                      <span className="vs-cmd" style={{ flex: 1 }}>
                        {director.outPath}
                      </span>
                      <button
                        className="btn sm"
                        onClick={() => void openFile(director.outPath as string)}
                      >
                        <Play size={12} /> {t("common.open")}
                      </button>
                    </div>
                  )}
                  {director.error && (
                    <div className="vs-msg err">
                      <AlertTriangle size={13} /> {director.error}
                    </div>
                  )}
                </div>
              )}

              {director?.log.length ? (
                <div className="vs-log" style={{ marginTop: 10 }}>
                  {director.log.join("\n")}
                </div>
              ) : null}
            </>
          )}

          {/* ══ ORÍGENS: l'inventari mesurat ═══════════════════════════════ */}
          {tab === "sources" && (
            <>
              <p className="lp-hint">{t("video.sourcesIntro")}</p>
              <div className="vs-row" style={{ marginTop: 0 }}>
                <button className="btn sm" onClick={() => void pickSource()}>
                  <Plus size={12} /> {t("video.addSource")}
                </button>
                <span className="lp-hint">
                  {t("video.workDir")}{" "}
                  <code style={{ wordBreak: "break-all" }}>{scan?.workDir ?? "—"}</code>
                </span>
              </div>

              {(scan?.sources ?? []).length === 0 && (
                <div className="vs-msg">{t("video.noSources")}</div>
              )}

              {(scan?.sources ?? []).map((s: SourceInfo, i) => (
                <div className="vs-src" key={s.path + i}>
                  <span>{i + 1}</span>
                  <div>
                    <h4>{s.name}</h4>
                    <div className="vs-meters">
                      <span className="vs-meter">{s.duration.toFixed(2)} s</span>
                      <span className="vs-meter">
                        {s.width}×{s.height}
                      </span>
                      <span className="vs-meter">{s.fps.toFixed(2)} fps</span>
                      <span className="vs-meter">{s.vcodec}</span>
                      <span className="vs-meter">{s.hasAudio ? `àudio ${s.acodec}` : "mut"}</span>
                      <span className="vs-meter">{mb(s.sizeBytes)}</span>
                    </div>
                    <div className="vs-meters" style={{ marginTop: 4 }}>
                      <span className="vs-meter" title={t("video.mLuma")}>
                        {t("video.mLuma")}
                        <span className="vs-bar">
                          <i style={{ width: pct(s.luma) }} />
                        </span>
                      </span>
                      <span className="vs-meter" title={t("video.mSat")}>
                        {t("video.mSat")}
                        <span className="vs-bar">
                          <i style={{ width: pct(s.saturation) }} />
                        </span>
                      </span>
                      <span className="vs-meter" title={t("video.mMotion")}>
                        {t("video.mMotion")}
                        <span className="vs-bar">
                          <i style={{ width: pct(s.motion, 300) }} />
                        </span>
                      </span>
                      <span className="vs-meter" title={t("video.mBlack")}>
                        {t("video.mBlack")}
                        <span className="vs-bar">
                          <i style={{ width: pct(s.black) }} />
                        </span>
                      </span>
                      <span className="vs-meter">
                        {t("video.mScenes")}: <b>{s.sceneChanges}</b>
                      </span>
                      <span className="vs-meter">
                        {t("video.mSilence")}: <b>{s.silenceBlocks}</b> · {s.totalSilenceS.toFixed(1)} s
                      </span>
                    </div>
                    {!s.readable && <div className="vs-msg err">{t("video.unreadable")}</div>}
                  </div>
                  <div style={{ display: "flex", gap: 6 }}>
                    <button className="btn sm" onClick={() => setPlaying(s.path)} title={t("common.openExternHint")}>
                      <Play size={12} />
                    </button>
                    <button className="btn sm" onClick={() => void openFile(s.path)} title={t("common.openExternHint")}>
                      <ExternalLink size={12} />
                    </button>
                  </div>
                  {s.sheet && sheets[s.sheet] && (
                    <div className="vs-sheet">
                      <div className="lp-hint" style={{ marginBottom: 4 }}>
                        <Eye size={11} /> {t("video.sheet")}
                      </div>
                      <img src={sheets[s.sheet]} alt={s.name} />
                    </div>
                  )}
                </div>
              ))}
            </>
          )}

          {/* ══ MUNTATGE: l'EDL, clip a clip ═══════════════════════════════ */}
          {tab === "edl" && (
            <>
              <p className="lp-hint">{t("video.edlIntro")}</p>
              <div className="vs-row" style={{ marginTop: 0 }}>
                <button
                  className="btn sm"
                  disabled={!projectPath || busy === "auto"}
                  onClick={() => void makeAutoEdl(projectPath, name || "Muntatge", targetS)}
                >
                  {busy === "auto" ? <Loader2 size={12} className="spin" /> : <Scissors size={12} />}{" "}
                  {t("video.edlAuto")}
                </button>
                <button
                  className="btn sm"
                  disabled={!projectPath || !raw.trim() || busy === "parse"}
                  onClick={() => void parseEdl(projectPath, raw)}
                >
                  <Download size={12} /> {t("video.edlLoad")}
                </button>
                <button
                  className="btn sm"
                  disabled={!edl || busy === "save"}
                  onClick={() => edl && void saveEdl(projectPath, edl)}
                >
                  {t("video.edlSave")}
                </button>
              </div>

              {!edl && <div className="vs-msg">{t("video.edlEmpty")}</div>}

              {edl && (
                <>
                  <div className="vs-meters" style={{ marginTop: 10 }}>
                    <span className="vs-meter">
                      «{edl.name}» · {edl.clips.length} {t("video.clipsN")}
                    </span>
                    <span className="vs-meter">
                      {edl.profile.width}×{edl.profile.height} @ {edl.profile.fps} fps · CRF{" "}
                      {edl.profile.crf} · {edl.profile.preset}
                    </span>
                    <span className="vs-meter">
                      {edl.texts.length} {t("video.textsN")}
                    </span>
                    <span className="vs-meter">
                      {edl.normalizeAudio ? `${t("video.lufs")} ${edl.targetLufs}` : t("video.noNorm")}
                    </span>
                  </div>

                  {edl.clips.map((c, i) => (
                    <div className="vs-clip" key={c.source + i}>
                      <b>{i + 1}. {c.label || c.source.split("/").pop()}</b>{" "}
                      <span className="t">
                        {c.inS.toFixed(2)}–{c.outS.toFixed(2)} s
                        {c.speed !== 1 ? ` · ×${c.speed}` : ""}
                        {c.mute ? " · mut" : c.gain !== 1 ? ` · ${c.gain.toFixed(2)}×` : ""}
                        {c.fadeInS > 0 || c.fadeOutS > 0
                          ? ` · fons ${c.fadeInS.toFixed(2)}/${c.fadeOutS.toFixed(2)}`
                          : ""}
                        {c.transition ? ` · ${c.transition.kind} ${c.transition.durS}s` : ""}
                        {c.grade?.stabilize ? " · estabilitzat" : ""}
                      </span>
                      <div className="t" style={{ wordBreak: "break-all", marginTop: 2 }}>
                        {c.source}
                      </div>
                    </div>
                  ))}

                  {edl.texts.length > 0 && (
                    <div className="vs-clip">
                      {edl.texts.map((x, i) => (
                        <div key={i} className="t">
                          <b>{x.kind}</b> «{x.text}» · {x.atS.toFixed(2)} s +{x.durS.toFixed(2)} s
                        </div>
                      ))}
                    </div>
                  )}

                  {edl.note && <div className="vs-msg">{edl.note}</div>}
                </>
              )}

              <div className="field" style={{ marginTop: 12 }}>
                <label>{t("video.edlJson")}</label>
                <textarea
                  rows={10}
                  value={raw}
                  onChange={(e) => setRaw(e.target.value)}
                  spellCheck={false}
                  style={{ fontFamily: "var(--mono, ui-monospace, monospace)", fontSize: 11 }}
                />
                <div className="lp-hint" style={{ marginTop: 4 }}>
                  {t("video.edlJsonHint")}
                </div>
              </div>
            </>
          )}

          {/* ══ SORTIDA: previsualitzar, renderitzar, comprovar, exportar ══ */}
          {tab === "out" && (
            <>
              <p className="lp-hint">{t("video.outIntro")}</p>
              <div className="vs-row" style={{ marginTop: 0 }}>
                <button
                  className="btn sm"
                  disabled={!projectPath || rendering || working || busy === "render"}
                  onClick={() => void renderEdl(projectPath, true, container, false)}
                >
                  <Play size={12} /> {t("video.previewBtn")}
                </button>
                <button
                  className="btn primary"
                  disabled={!projectPath || rendering || working || busy === "render"}
                  onClick={() => void renderEdl(projectPath, false, container, openAfter)}
                >
                  {busy === "render" ? <Loader2 size={12} className="spin" /> : <Clapperboard size={12} />}{" "}
                  {t("video.renderBtn")}
                </button>
                {rendering && (
                  <button className="btn sm" onClick={() => void stopRender()}>
                    <Square size={12} /> {t("video.stopRender")}
                  </button>
                )}
                <label className="vs-check">
                  <input
                    type="checkbox"
                    checked={openAfter}
                    onChange={(e) => setOpenAfter(e.target.checked)}
                  />
                  {t("video.openAfter")}
                </label>
              </div>

              <div className="vs-row">
                <button
                  className="btn sm"
                  disabled={!projectPath || busy === "plan"}
                  onClick={() => void buildPlan(projectPath, false, container)}
                >
                  {t("video.planBtn")}
                </button>
                <button
                  className="btn sm"
                  disabled={!projectPath || !edl || busy === "verify"}
                  onClick={() => void doVerify()}
                >
                  {t("video.verifyBtn")}
                </button>
                <button
                  className="btn sm"
                  disabled={!projectPath || busy === "export"}
                  onClick={() => void exportProject(projectPath)}
                >
                  <Download size={12} /> {t("video.exportBtn")}
                </button>
              </div>

              {render && (render.running || render.outPath || render.error) && (
                <div className="vs-work" style={{ marginTop: 12 }}>
                  <WorkProgress
                    running={render.running}
                    startedAtMs={(render.startedAt || 0) * 1000}
                    mode="tasks"
                    doneUnits={Math.round(render.progress * 100)}
                    totalUnits={100}
                    label={`${t("video.stage")} ${render.stage}${render.preview ? ` · ${t("video.previewTag")}` : ""}`}
                  />
                  {render.detail && <div className="lp-hint">{render.detail}</div>}
                  {render.error && (
                    <div className="vs-msg err">
                      <AlertTriangle size={13} /> {render.error}
                    </div>
                  )}
                </div>
              )}

              {(plan?.command || render?.commandLine) && (
                <>
                  <div className="lp-hint" style={{ marginTop: 10 }}>
                    {t("video.cmd")}
                  </div>
                  <div className="vs-cmd">{render?.commandLine ?? plan?.command}</div>
                </>
              )}

              {scan?.lastRender && (
                <div className="vs-row">
                  <span className="lp-hint">{t("video.lastRender")}</span>
                  <span className="vs-cmd" style={{ flex: 1 }}>
                    {scan.lastRender}
                  </span>
                  <button className="btn sm" onClick={() => void openFile(scan.lastRender as string)}>
                    <Play size={12} /> {t("common.open")}
                  </button>
                </div>
              )}

              {verifyRep != null && (
                <>
                  <div className="lp-hint" style={{ marginTop: 10 }}>
                    {t("video.verifyReport")}
                  </div>
                  <div className="vs-log">{JSON.stringify(verifyRep, null, 2)}</div>
                </>
              )}

              <div className="vs-row">
                <button
                  className="btn sm"
                  disabled={!tools?.kdenlive.ok}
                  title={tools?.kdenlive.ok ? "" : t("video.kdenliveMissing")}
                  onClick={() => {
                    const p = scan?.lastRender?.replace(/\.(mp4|mov|webm)$/i, ".kdenlive");
                    if (p) void openKdenlive(p);
                    else setErr(t("video.noKdenliveProject"));
                  }}
                >
                  <ExternalLink size={12} /> {t("video.openKdenlive")}
                </button>
              </div>
            </>
          )}

          {/* ── Registre comú (avisos del sanejament i del motor) ─────────── */}
          {notes.length > 0 && (
            <>
              <div className="lp-hint" style={{ marginTop: 12 }}>
                {t("video.log")}
              </div>
              <div className="vs-log">{notes.join("\n")}</div>
            </>
          )}

          {transitions.length > 0 && tab === "edl" && (
            <div className="vs-msg">{t("video.transitionsHint")} {transitions.join(" · ")}</div>
          )}

          {err && (
            <div className="vs-msg err">
              <AlertTriangle size={13} /> {err}
            </div>
          )}
          {playing && (
            <div style={{ marginTop: 12 }}>
              <SourcePlayer path={playing} onClose={() => setPlaying(null)} />
            </div>
          )}
        </div>

        <div className="vs-foot">
          <span className="vs-credit">
            {t("wpTheme.credit")} <b>Joan Puigbert Carreras</b> · <b>Thepuigberts.com</b>
          </span>
          <button className="btn sm" onClick={close}>
            {t("common.close")}
          </button>
        </div>
      </div>
    </div>
  );
}

function vsVideoMime(path: string): string {
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

function SourcePlayer({ path, onClose }: { path: string; onClose: () => void }) {
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
        const blob = new Blob([bytes as BlobPart], { type: vsVideoMime(path) });
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
    <div className="vs-row" style={{ flexDirection: "column", alignItems: "stretch" }}>
      <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 6 }}>
        <span className="lp-hint" style={{ wordBreak: "break-all" }}>{path.split("/").pop()}</span>
        <button className="btn sm" onClick={onClose}>
          <X size={12} />
        </button>
      </div>
      {url ? (
        <video src={url} controls style={{ width: "100%", maxHeight: 360, background: "#000", borderRadius: 4 }} />
      ) : (
        <div className="vs-msg">{err ?? "Carregant vídeo…"}</div>
      )}
    </div>
  );
}
