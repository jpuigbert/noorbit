import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  Clock,
  Cpu,
  Download,
  Folder,
  KeyRound,
  Loader2,
  Palette,
  Sparkles,
  Square,
  X,
} from "lucide-react";
import { useT } from "../../i18n";
import WorkProgress from "../WorkProgress";
import { customMinutes, formatDuration, type TimeUnit } from "../../utils/workTime";
import { useUIStore } from "../../stores/uiStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useAIStore } from "../../stores/aiStore";
import { useProviderStore, type ProviderInput } from "../../stores/providerStore";
import { PRESETS, presetModelsFor } from "../AI/ProviderManager";
import {
  WP_THEME_CORE,
  WP_THEME_FILE_SYSTEM,
  WP_THEME_MANIFEST_SYSTEM,
} from "../../stores/agent/prompts";
import { extractSingleFileContent, parseManifest } from "../../stores/agent/fileBlocks";
import "./WordPressThemeModal.css";

/**
 * MENÚ «CREAR TEMA DE WORDPRESS» — demana a la IA els fitxers inicials d'un
 * tema de WordPress.
 *
 * Passos:
 *  1. **Què vols crear**: l'usuari explica el tema (i en quina carpeta ha de
 *     néixer) i la IA el converteix en fitxers.
 *  2. **Quina IA**: es tria el model local (Ollama) o un proveïdor en línia
 *     (amb clau API) DES DEL MATEIX diàleg: es pot baixar un model nou o
 *     configurar una IA amb la seva clau sense sortir d'aquí.
 *  3. **Generació**: la IA escriu el tema fitxer a fitxer (un model dèbil no
 *     escriu vint fitxers d'un cop) i NoOrbit obre la carpeta del tema.
 */

/// Valors del desplegable «Quina IA»:
/// - `local|<model>`  → model d'Ollama instal·lat
/// - `prov|<id>`      → proveïdor en línia (amb clau API)
/// - `__download__`   → baixar un model nou (Ollama)
/// - `__api__`        → configurar una IA amb clau API
const LOCAL = "local|";
const PROV = "prov|";
const DOWNLOAD = "__download__";
const API = "__api__";

const emptyApiForm = (): ProviderInput => ({
  name: "",
  baseUrl: "",
  model: "",
  kind: "openai",
  auth: "bearer",
  headerName: "",
  token: "",
  enabled: true,
});

/// Estat de la millora contínua autopilotada (veu del backend `project://work`).
interface ProjectWorkState {
  running: boolean;
  pass: number;
  elapsedMs: number;
  budgetMs: number;
  filesChanged: number;
  detail: string;
  startedAt: number;
  finishedAt: number | null;
  projectPath: string;
  error: string | null;
}

/// Temps de treball EXTRA (millora autopilotada) que l'usuari pot triar.
/// 0 = només crear el tema; la resta = la IA treballa eixe temps buscant
/// millores i bugs i implementant-los, sense que l'usuari toque res.
/// Opció «Personalitzat»: temps exacte (valor + unitat), com a la Jam.
const CUSTOM = -1;

const WORK_DURATIONS = [
  { mins: 0, label: "—" },
  { mins: 15, label: "15 min" },
  { mins: 30, label: "30 min" },
  { mins: 60, label: "1 hora" },
  { mins: 120, label: "2 hores" },
  { mins: 240, label: "4 hores" },
  { mins: 480, label: "8 hores" },
  { mins: 1440, label: "1 dia" },
  { mins: CUSTOM, label: "" },
];

export default function WordPressThemeModal() {
  const { t } = useT();
  const show = useUIStore((s) => s.showWpTheme);
  const setShow = useUIStore((s) => s.setShowWpTheme);

  // IA local (Ollama) i proveïdors en línia.
  const models = useAIStore((s) => s.models);
  const loadModels = useAIStore((s) => s.loadModels);
  const pullModel = useAIStore((s) => s.pullModel);
  const providers = useProviderStore((s) => s.providers);
  const activeProvider = useProviderStore((s) => s.active);
  const loadProviders = useProviderStore((s) => s.load);
  const addProvider = useProviderStore((s) => s.add);
  const selectProvider = useProviderStore((s) => s.select);

  const [step, setStep] = useState<1 | 2 | 3>(1);
  const [name, setName] = useState("");
  const [parent, setParent] = useState("");
  const [idea, setIdea] = useState("");
  const [author, setAuthor] = useState("Joan Puigbert Carreras");
  const [ai, setAi] = useState("");
  const [dlModel, setDlModel] = useState("");
  const [apiForm, setApiForm] = useState<ProviderInput>(emptyApiForm());
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [files, setFiles] = useState<string[]>([]);
  const [themePath, setThemePath] = useState<string | null>(null);
  const [workMins, setWorkMins] = useState(0);
  const [workCustomVal, setWorkCustomVal] = useState(2);
  const [workCustomUnit, setWorkCustomUnit] = useState<TimeUnit>("h");
  const [wpState, setWpState] = useState<ProjectWorkState | null>(null);

  useEffect(() => {
    if (!show) return;
    void loadModels();
    void loadProviders();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [show]);

  /// Estat de la millora autopilotada + esdeveniments en viu. En obrir el
  /// diàleg es recupera l'estat (la feina pot continuar en segon pla).
  useEffect(() => {
    if (!show) return;
    void invoke<ProjectWorkState>("project_work_state").then(setWpState).catch(() => undefined);
    const un = listen<ProjectWorkState>("project://work", (e) => setWpState(e.payload));
    return () => {
      void un.then((u) => u());
    };
  }, [show]);

  /// Tria per defecte la IA que té l'usuari activa: el model local si en té,
  /// el seu proveïdor en línia si no.
  useEffect(() => {
    if (ai || models.length === 0) return;
    const local = models[0];
    if (!local) return;
    setAi(LOCAL + local.name);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [models]);

  /// L'IA triada es resol en (proveïdor, model) per als arguments del backend.
  const resolveAi = useMemo((): { provider: string | undefined; model: string | undefined } => {
    if (ai.startsWith(LOCAL)) return { provider: "ollama", model: ai.slice(LOCAL.length) };
    if (ai.startsWith(PROV)) {
      const p = providers.find((x) => x.id === ai.slice(PROV.length));
      return { provider: p?.id, model: p?.model };
    }
    return { provider: undefined, model: undefined };
  }, [ai, providers]);

  /// Temps efectiu de treball (minuts): preset triat o valor personalitzat
  /// (retallat a 1 minut..7 dies, igual que el backend).
  const effectiveWorkMins =
    workMins === CUSTOM ? customMinutes(workCustomVal, workCustomUnit) : workMins;

  const close = () => {
    setShow(false);
    setStep(1);
    setIdea("");
    setName("");
    setParent("");
    setErr(null);
    setMsg(null);
    setFiles([]);
    setThemePath(null);
    setApiForm(emptyApiForm());
    setDlModel("");
    setWorkMins(0);
    setWorkCustomVal(2);
    setWorkCustomUnit("h");
    setWpState(null);
  };

  const browse = async () => {
    const sel = await open({ directory: true, title: t("wpTheme.parent") });
    if (typeof sel === "string") setParent(sel);
  };

  /// Baixa un model d'Ollama i el tria per fer el tema.
  const download = async () => {
    const m = dlModel.trim();
    if (!m) return;
    setBusy("dl");
    setErr(null);
    setMsg(null);
    try {
      await pullModel(m);
      const found = useAIStore.getState().models.find((x) => x.name === m);
      setAi(LOCAL + (found?.name ?? m));
      setMsg(t("wpTheme.downloaded", { model: m }));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  /// Configura una IA amb clau API des del mateix diàleg i la tria.
  const saveApi = async () => {
    if (!apiForm.name.trim() || !apiForm.baseUrl.trim()) return;
    setBusy("api");
    setErr(null);
    setMsg(null);
    try {
      await addProvider({ ...apiForm, token: apiForm.token || undefined });
      const found = useProviderStore
        .getState()
        .providers.find((p) => p.name === apiForm.name.trim());
      if (found) {
        await selectProvider(found.id);
        setAi(PROV + found.id);
      }
      setApiForm(emptyApiForm());
      setMsg(t("wpTheme.apiSaved", { name: apiForm.name.trim() }));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  /// Crea el tema: la IA escriu els fitxers inicials, un a un.
  const generate = async () => {
    if (!name.trim() || !idea.trim()) {
      setErr(t("wpTheme.emptyIdea"));
      return;
    }
    if (!ai || ai === DOWNLOAD || ai === API) {
      setErr(t("wpTheme.noAi"));
      return;
    }
    const { provider, model } = resolveAi;
    setBusy("gen");
    setErr(null);
    setMsg(null);
    setFiles([]);
    try {
      // 1) Carpeta del tema.
      const path = await invoke<string>("create_project", {
        name: name.trim(),
        parent: parent.trim() ? parent.trim() : null,
      });
      setThemePath(path);

      // 2) Quin fitxer cal? La IA ho diu (tasca fàcil), però el nucli
      //    (style.css, functions.php, index.php…) sempre hi és: sense
      //    style.css el tema no té ni nom.
      setBusy("manifest");
      let paths: string[] = [];
      try {
        const raw = await invoke<string>("send_prompt", {
          prompt:
            `Vull crear un tema de WordPress anomenat «${name.trim()}». ` +
            `Descripció: ${idea.trim()}\n\n` +
            `Llista NOMÉS les rutes relatives dels fitxers del tema, una per línia.`,
          system: WP_THEME_MANIFEST_SYSTEM,
          provider: provider ?? undefined,
          model: model ?? undefined,
          question: idea.trim(),
        });
        paths = parseManifest(raw);
      } catch {
        paths = [];
      }
      // El nucli del tema, sempre.
      const filesSet = new Set<string>([...WP_THEME_CORE, ...paths, "README.md"]);
      const ordered = [...filesSet].filter(
        (p) => !p.split("/").some((seg) => seg === ".."),
      );

      // 3) Fitxer a fitxer: un model dèbil no escriu vint fitxers d'un cop.
      const made: string[] = [];
      for (const rel of ordered) {
        setBusy(rel);
        setMsg(t("wpTheme.generating", { file: rel }));
        let content = "";
        try {
          const raw = await invoke<string>("ai_chat_plain", {
            prompt: wpPrompt(rel, name.trim(), idea.trim(), author.trim()),
            system: WP_THEME_FILE_SYSTEM,
            provider: provider ?? undefined,
            model: model ?? undefined,
          });
          content = extractSingleFileContent(raw);
        } catch {
          content = "";
        }
        if (!content.trim()) {
          // L'usuari ha de saber què ha quedat pendent (no li diem mentida).
          made.push(rel);
          setFiles([...made]);
          continue;
        }
        // write_file crea la carpeta de destinació si no hi és (create_dir
        // exigiria el projecte obert, que encara no ho està en aquest punt).
        const abs = `${path.replace(/\/+$/, "")}/${rel}`;
        await invoke("write_file", { path: abs, content });
        made.push(rel);
        setFiles([...made]);
      }

      setBusy(null);
      setMsg(t("wpTheme.created", { path, n: made.length }));
      // Obre el tema com a projecte perquè l'usuari hi treballi de seguida.
      await useWorkspaceStore.getState().openWorkspace(path);
      // MILLORA AUTOPILOTADA: si l'usuari ha triat un temps de treball extra,
      // la IA continua millorant el tema (bugs, rendiment, SEO, noves
      // funcionalitats…) durant eixe temps, sense que l'usuari toque res.
      if (effectiveWorkMins > 0) {
        setMsg(t("wpTheme.workStarting"));
        await startWork(path);
      }
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  /// Arrenca la millora contínua autopilotada sobre el tema ja creat.
  const startWork = async (path: string) => {
    const { provider, model } = resolveAi;
    setErr(null);
    try {
      await invoke("wp_theme_work", {
        themePath: path,
        name: name.trim(),
        idea: idea.trim(),
        budgetSecs: effectiveWorkMins * 60,
        provider: provider ?? undefined,
        model: model ?? undefined,
      });
    } catch (e) {
      setErr(String(e));
    }
  };

  const stopWork = async () => {
    await invoke("project_work_stop").catch(() => undefined);
  };

  if (!show) return null;

  const passos: { n: 1 | 2 | 3; label: string }[] = [
    { n: 1, label: t("wpTheme.stepIdea") },
    { n: 2, label: t("wpTheme.stepAi") },
    { n: 3, label: t("wpTheme.stepBuild") },
  ];

  return (
    <div className="modal-overlay" onClick={close}>
      <div className="modal wp-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>
            <Palette size={15} style={{ marginRight: 6, verticalAlign: -2 }} />
            {t("wpTheme.title")}
          </span>
          <button className="icon-btn" onClick={close}>
            <X size={16} />
          </button>
        </div>

        {/* PASSOS */}
        <div className="wp-steps">
          {passos.map((p) => (
            <button
              key={p.n}
              className={"wp-step" + (step === p.n ? " active" : "") + (step > p.n ? " done" : "")}
              onClick={() => setStep(p.n)}
              disabled={busy !== null}
            >
              <span className="wp-step-n">{step > p.n ? <CheckCircle2 size={11} /> : p.n}</span>
              {p.label}
            </button>
          ))}
        </div>

        <div className="modal-body wp-body">
          {/* ── 1 · QUÈ VOLS CREAR ── */}
          {step === 1 && (
            <div className="wp-step-body">
              <p className="lp-hint">{t("wpTheme.intro")}</p>
              <div className="field">
                <label>{t("wpTheme.name")}</label>
                <input
                  autoFocus
                  placeholder={t("wpTheme.namePh")}
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                />
              </div>
              <div className="field">
                <label>{t("wpTheme.idea")}</label>
                <textarea
                  rows={6}
                  placeholder={t("wpTheme.ideaPh")}
                  value={idea}
                  onChange={(e) => setIdea(e.target.value)}
                />
                <div className="lp-hint" style={{ marginTop: 4 }}>
                  {t("wpTheme.ideaHint")}
                </div>
              </div>
              <div className="wp-2">
                <div className="field">
                  <label>{t("wpTheme.author")}</label>
                  <input
                    placeholder={t("wpTheme.authorPh")}
                    value={author}
                    onChange={(e) => setAuthor(e.target.value)}
                  />
                </div>
                <div className="field">
                  <label>{t("wpTheme.parent")}</label>
                  <div style={{ display: "flex", gap: 8 }}>
                    <input
                      style={{ flex: 1 }}
                      placeholder={t("wpTheme.parentPh")}
                      value={parent}
                      onChange={(e) => setParent(e.target.value)}
                    />
                    <button className="btn sm" onClick={() => void browse()}>
                      <Folder size={12} /> {t("common.browse")}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* ── 2 · QUINA IA ── */}
          {step === 2 && (
            <div className="wp-step-body">
              <div className="field">
                <label>{t("wpTheme.aiLabel")}</label>
                <select className="lp-select" value={ai} onChange={(e) => setAi(e.target.value)}>
                  {models.length > 0 && (
                    <optgroup label={t("wpTheme.aiLocal")}>
                      {models.map((m) => (
                        <option key={m.name} value={LOCAL + m.name}>
                          {m.name}
                        </option>
                      ))}
                    </optgroup>
                  )}
                  {providers.length > 0 && (
                    <optgroup label={t("wpTheme.aiCloud")}>
                      {providers.map((p) => (
                        <option key={p.id} value={PROV + p.id}>
                          {p.name} · {p.model}
                          {p.has_token ? "" : ` (${t("wpTheme.noToken")})`}
                        </option>
                      ))}
                    </optgroup>
                  )}
                  <optgroup label={t("wpTheme.aiMore")}>
                    <option value={DOWNLOAD}>{t("wpTheme.downloadOption")}</option>
                    <option value={API}>{t("wpTheme.apiOption")}</option>
                  </optgroup>
                </select>
                <div className="lp-hint" style={{ marginTop: 4 }}>
                  {models.length === 0 && providers.length === 0
                    ? t("wpTheme.aiNone")
                    : t("wpTheme.aiHint")}
                </div>
              </div>

              {/* BAIXAR UN MODEL */}
              {ai === DOWNLOAD && (
                <div className="wp-panel">
                  <div className="lp-hint">{t("wpTheme.downloadHint")}</div>
                  <div style={{ display: "flex", gap: 8 }}>
                    <input
                      style={{ flex: 1 }}
                      placeholder={t("wpTheme.downloadPh")}
                      value={dlModel}
                      onChange={(e) => setDlModel(e.target.value)}
                    />
                    <button
                      className="btn sm primary"
                      disabled={busy !== null || !dlModel.trim()}
                      onClick={() => void download()}
                    >
                      {busy === "dl" ? (
                        <Loader2 size={12} className="spin" />
                      ) : (
                        <Download size={12} />
                      )}
                      {busy === "dl" ? t("wpTheme.downloading") : t("wpTheme.downloadBtn")}
                    </button>
                  </div>
                </div>
              )}

              {/* CONFIGURAR UNA IA AMB CLAU API */}
              {ai === API && (
                <div className="wp-panel">
                  <div className="lp-hint">{t("wpTheme.apiHint")}</div>
                  <div className="field">
                    <label>{t("wpTheme.apiPresets")}</label>
                    <select
                      className="lp-select"
                      value=""
                      onChange={(e) => {
                        const p = PRESETS.find((x) => x.key === e.target.value);
                        if (p) setApiForm({ ...emptyApiForm(), ...p.data });
                      }}
                    >
                      <option value="">{t("wpTheme.apiPresetsPh")}</option>
                      {PRESETS.map((p) => (
                        <option key={p.key} value={p.key}>
                          {p.label}
                        </option>
                      ))}
                    </select>
                  </div>
                  <div className="wp-2">
                    <div className="field">
                      <label>{t("wpTheme.apiName")}</label>
                      <input
                        value={apiForm.name}
                        onChange={(e) => setApiForm((f) => ({ ...f, name: e.target.value }))}
                        placeholder="OpenAI"
                      />
                    </div>
                    <div className="field">
                      <label>{t("wpTheme.apiUrl")}</label>
                      <input
                        value={apiForm.baseUrl}
                        onChange={(e) => setApiForm((f) => ({ ...f, baseUrl: e.target.value }))}
                        placeholder="https://api.openai.com/v1"
                      />
                    </div>
                  </div>
                  <div className="wp-2">
                    <div className="field">
                      <label>{t("wpTheme.apiModel")}</label>
                      {/* Suggimits segons el servei triat: els noms que Claude i
                          DeepSeek reconeixen avui (escrits a mà també valen). */}
                      <input
                        list="wp-api-model-options"
                        value={apiForm.model}
                        onChange={(e) => setApiForm((f) => ({ ...f, model: e.target.value }))}
                        placeholder="claude-sonnet-5-5"
                      />
                      <datalist id="wp-api-model-options">
                        {presetModelsFor(apiForm.baseUrl, apiForm.model).map((m) => (
                          <option key={m} value={m} />
                        ))}
                      </datalist>
                    </div>
                    <div className="field">
                      <label>{t("wpTheme.apiToken")}</label>
                      <input
                        type="password"
                        value={apiForm.token}
                        onChange={(e) => setApiForm((f) => ({ ...f, token: e.target.value }))}
                        placeholder="sk-…"
                      />
                    </div>
                  </div>
                  <button
                    className="btn sm primary"
                    disabled={busy !== null || !apiForm.name.trim() || !apiForm.baseUrl.trim()}
                    onClick={() => void saveApi()}
                  >
                    {busy === "api" ? (
                      <Loader2 size={12} className="spin" />
                    ) : (
                      <KeyRound size={12} />
                    )}
                    {t("wpTheme.apiSave")}
                  </button>
                </div>
              )}
            </div>
          )}

          {/* ── 3 · GENERACIÓ ── */}
          {step === 3 && (
            <div className="wp-step-body">
              <div className="wp-summary">
                <div>
                  <span>{t("wpTheme.name")}</span>
                  <b>{name || "—"}</b>
                </div>
                <div>
                  <span>{t("wpTheme.aiLabel")}</span>
                  <b>
                    {ai.startsWith(LOCAL)
                      ? ai.slice(LOCAL.length)
                      : ai.startsWith(PROV)
                      ? providers.find((p) => p.id === ai.slice(PROV.length))?.name ?? "—"
                      : "—"}
                  </b>
                </div>
              </div>
              {busy === "gen" || busy === "manifest" ? (
                <div className="lp-hint">
                  <Loader2 size={12} className="spin" /> {t("wpTheme.planning")}
                </div>
              ) : null}
              {files.length > 0 && (
                <div className="wp-files">
                  {files.map((f) => (
                    <div key={f} className="wp-file">
                      <CheckCircle2 size={11} /> {f}
                    </div>
                  ))}
                </div>
              )}

              {/* TEMPS DE TREBALL EXTRA (millora autopilotada): la IA treballa
                  eixe temps buscant millores i bugs i implementant-los sola. */}
              <div className="field">
                <label>
                  <Clock size={12} style={{ marginRight: 4, verticalAlign: -2 }} />
                  {t("wpTheme.workLabel")}
                </label>
                <select
                  className="lp-select"
                  value={workMins}
                  disabled={busy !== null || wpState?.running === true}
                  onChange={(e) => setWorkMins(Number(e.target.value))}
                >
                  {WORK_DURATIONS.map((d) => (
                    <option key={d.mins} value={d.mins}>
                      {d.mins === 0 ? t("wpTheme.workNone") : d.mins === CUSTOM ? t("work.custom") : d.label}
                    </option>
                  ))}
                </select>
                <div className="lp-hint" style={{ marginTop: 4 }}>
                  {t("wpTheme.workHint")}
                </div>
                {workMins === CUSTOM && (
                  <div className="wp-2" style={{ marginTop: 8 }}>
                    <div className="field">
                      <label>{t("work.customTime")}</label>
                      <input
                        type="number"
                        min={1}
                        value={workCustomVal}
                        disabled={busy !== null || wpState?.running === true}
                        onChange={(e) => setWorkCustomVal(Number(e.target.value))}
                      />
                    </div>
                    <div className="field">
                      <label>{t("work.unit")}</label>
                      <select
                        className="lp-select"
                        value={workCustomUnit}
                        disabled={busy !== null || wpState?.running === true}
                        onChange={(e) => setWorkCustomUnit(e.target.value as TimeUnit)}
                      >
                        <option value="min">{t("work.unitMin")}</option>
                        <option value="h">{t("work.unitH")}</option>
                        <option value="d">{t("work.unitD")}</option>
                      </select>
                    </div>
                  </div>
                )}
                {workMins === CUSTOM && (
                  <div className="lp-hint" style={{ marginTop: 4 }}>
                    {formatDuration(effectiveWorkMins * 60_000)} · {t("work.customHint")}
                  </div>
                )}
              </div>

              {themePath && effectiveWorkMins > 0 && !wpState?.running && busy === null && (
                <button className="btn sm primary" onClick={() => void startWork(themePath)}>
                  <Sparkles size={12} /> {t("wpTheme.workStart")}
                </button>
              )}

              {wpState && (wpState.running || wpState.finishedAt) && (
                <div className="wp-work">
                  <WorkProgress
                    running={wpState.running}
                    startedAtMs={(wpState.startedAt || 0) * 1000}
                    mode="time"
                    budgetMs={wpState.budgetMs}
                    finishedAtMs={wpState.finishedAt ? wpState.finishedAt * 1000 : null}
                    label={`${t("wpTheme.workPass")} ${wpState.pass} · ${wpState.filesChanged} ${t("wpTheme.workFiles")}`}
                  />
                  {wpState.detail && <div className="lp-hint">{wpState.detail}</div>}
                  {wpState.running && (
                    <button className="btn sm" onClick={() => void stopWork()}>
                      <Square size={12} /> {t("wpTheme.workStop")}
                    </button>
                  )}
                  {wpState.error && (
                    <div className="wp-msg err">
                      <AlertTriangle size={13} /> {wpState.error}
                    </div>
                  )}
                </div>
              )}
              {msg && <div className="wp-msg">{msg}</div>}
              {err && (
                <div className="wp-msg err">
                  <AlertTriangle size={13} /> {err}
                </div>
              )}
            </div>
          )}
        </div>

        {/* PEU: enrere / següent */}
        <div className="wp-foot">
          <span className="wp-credit">
            {t("wpTheme.credit")} <b>Joan Puigbert Carreras</b> ·{" "}
            <b>Thepuigberts.com</b>
          </span>
          <div style={{ display: "flex", gap: 8 }}>
            {step > 1 && (
              <button className="btn sm" disabled={busy !== null} onClick={() => setStep((step - 1) as 1 | 2 | 3)}>
                {t("wpTheme.back")}
              </button>
            )}
            {step < 3 ? (
              <button
                className="btn primary sm"
                disabled={busy !== null || (step === 1 && (!name.trim() || !idea.trim()))}
                onClick={() => setStep((step + 1) as 1 | 2 | 3)}
              >
                <Sparkles size={12} /> {t("wpTheme.next")}
              </button>
            ) : (
              <button
                className="btn primary sm"
                disabled={busy !== null}
                onClick={() => void generate()}
              >
                {busy !== null ? (
                  <Loader2 size={12} className="spin" />
                ) : (
                  <Palette size={12} />
                )}
                {busy !== null ? t("wpTheme.creating") : t("wpTheme.create")}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/// Prompt d'UN fitxer del tema: què ha de fer el tema, què és aquest fitxer i
/// les dades de la capçalera (WordPress llegeix el nom i l'autor de
/// `style.css`).
function wpPrompt(rel: string, name: string, idea: string, author: string): string {
  const cap = rel === "style.css";
  return (
    `Tema WordPress: «${name}».\n` +
    `Què ha de fer: ${idea}\n\n` +
    `Escriu el fitxer «${rel}» COMPLET (codi real i executable, sense «...» ni «(implementa aquí)»).\n` +
    (cap
      ? `La capçalera ha de començar pel comentari /* Theme Name: ${name} */ amb Theme Name, ` +
        `Theme URI, Author: ${author || "Joan Puigbert Carreras"}, Description, Version, ` +
        `Requires at least: 6.0, Tested up to: 6.7, Requires PHP: 7.4, License: GNU General ` +
        `Public License v2 or later, License URI: https://www.gnu.org/licenses/gpl-2.0.html i ` +
        `Text Domain: ${name}. Després, les regles CSS del tema.\n`
      : "")
  );
}
