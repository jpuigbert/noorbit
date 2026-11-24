import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Gamepad2, Square, ListChecks, AlertTriangle, CheckCircle2, Loader2, X, Trophy, Rocket, Folder, MonitorPlay, Brain, Bot, MemoryStick, Activity, Cpu, Share2, ClipboardCopy } from "lucide-react";
import { useT } from "../../i18n";
import WorkProgress from "../WorkProgress";
import { obrirEstalvi } from "../../screensaver/window";
import { customMinutes, formatDuration, type TimeUnit } from "../../utils/workTime";
import { useProviderStore, type ProviderPublic } from "../../stores/providerStore";
import { useGridStore } from "../../stores/gridStore";
import type { OllamaModel } from "../../stores/aiStore";
import { LOCAL, NATIVE, NATIVE_ID, PROV, GRID, resolAi } from "./jamAi";

interface JamTask {
  id: string;
  title: string;
  description: string;
  acceptance: string[];
}

interface JamPlan {
  title: string;
  tasks: JamTask[];
  durationSecs: number;
}

interface JamState {
  running: boolean;
  title: string;
  completed: number;
  blocked: number;
  skipped: number;
  /// Total de tasques del pla que s'està EXECUTANT (veu del backend). Sense
  /// això la barra de progrés no tenia denominador: el `plan` es buida en
  /// prémer «Comença» i el percentatge es calculava amb els ja fets.
  total: number;
  currentTask: string;
  currentDetail: string;
  startedAt: number;
  finishedAt: number | null;
  /// Durada màxima triada (segons): sostre de l'ETA i de la fin estimada.
  durationSecs: number;
  reportPath: string;
  /// Arrel del projecte on la Jam està (o ha estat) treballant. Sense això,
  /// al final l'usuari no podia saber QUIN projecte havia tocat.
  projectPath: string;
  error: string | null;
  /// Proveïdor i model d'IA que fa servir la Jam. Sense això l'usuari no
  /// sabia amb quina IA treballava de debò (Ollama, un proveïdor en línia
  /// o la IA integrada de NoOrbit).
  provider: string;
  model: string;
  /// L'aturada ja s'ha demanat però la Jam encara no s'ha aturat del tot:
  /// el backend encara està tallant el pla o el procés fill.
  stopping: boolean;
}

/// Payload que envia el backend per l'esdeveniment `jam://event`.
interface JamEventPayload {
  kind: string;
  task: string;
  detail: string;
  ok: boolean;
}

/// Esdeveniment + moment en què ha arribat (ms). L'hora permet pintar una
/// marca de temps a cada línia del registre i dir «fa X s» del darrer.
interface JamEvent extends JamEventPayload {
  at: number;
}

/// Resultat de `ai_free_ram`: quants models d'Ollama s'han descarregat,
/// quants motors locals s'aturat i quanta memòria s'ha recuperat (bytes).
interface RamFreed {
  models: number;
  engines: number;
  usedBefore: number;
  usedAfter: number;
  freed: number;
}

/// Memòria que està usant la IA LOCAL ara mateix: el pes del model que
/// Ollama té carregat a RAM i el consum TOTAL del sistema.
interface LocalMemory {
  total: number;
  used: number;
  models: { name: string; size: number }[];
  modelBytes: number;
  engines: number;
  local: boolean;
}

/// Mida en bytes amb unitat llegible i coma decimal (català).
function fmtBytes(b: number): string {
  if (!b || b <= 0) return "0 MB";
  const gb = b / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(1).replace(".", ",")} GB`;
  return `${Math.round(b / 1024 / 1024)} MB`;
}

/// Presets de jocs per a nens (un clic i a jugar).
const PRESETS = [
  { emoji: "🎣", label: "Un joc de pesca", brief: "un joc de pesca on el peix rar només pica a la posta de sol" },
  { emoji: "🏎️", label: "Un joc de curses", brief: "un joc de curses de cotxes amb 3 circuits i turbo" },
  { emoji: "🚀", label: "Un joc de naus", brief: "un joc de naus que disparen a asteroides, amb vides i puntuació" },
  { emoji: "🧱", label: "Un Tetris", brief: "un Tetris clàssic amb nivells i música" },
  { emoji: "🐍", label: "Una serp", brief: "el joc de la serp (snake) amb menjar i obstacles" },
];

const ENGINES = [
  { id: "godot4", label: "🎮 Godot 4" },
  { id: "bevy", label: "🦀 Bevy (Rust)" },
  { id: "blender", label: "🧊 Blender" },
  { id: "unreal5", label: "🎬 Unreal 5" },
  { id: "unity", label: "Unity" },
  { id: "custom", label: "🌐 Web / HTML" },
];

/// Opció «Personalitzat»: l'usuari escriu el temps exacte (valor + unitat).
const CUSTOM = -1;

const DURATIONS = [
  { mins: 15, label: "⏱️ 15 min" },
  { mins: 60, label: "⏱️ 1 hora" },
  { mins: 240, label: "⏱️ 4 hores" },
  { mins: 480, label: "⏱️ 8 hores" },
  { mins: 1440, label: "📅 1 dia" },
  { mins: 2880, label: "📅 2 dies" },
  { mins: 4320, label: "📅 3 dies" },
  { mins: 10080, label: "📅 7 dies" },
  { mins: CUSTOM, label: "" },
];

/// Etiqueta humana de cada esdeveniment del registre (el backend en parla
/// internament en anglès; l'usuari ha de veure què està passant).
const KIND_LABEL: Record<string, string> = {
  task_start: "començo",
  task_done: "fet",
  task_error: "no ho aconsegueixo",
  note: "fent",
  thinking: "pensament",
  stream: "escrivint",
  memory: "memòria",
  restart: "reinici",
  compact: "neteja de memòria",
  summary: "resum",
  finished: "acabat",
  error: "error",
};

/// SELECTOR D'IA: un sol desplegable amb tres grups — IA integrada (sempre
/// hi és, sense token ni descàrrega), IA local d'Ollama (els models que
/// tinguis baixats) i proveïdors en línia.
function AiPicker({
  value,
  onChange,
  models,
  providers,
  disabled,
}: {
  value: string;
  onChange: (v: string) => void;
  models: OllamaModel[];
  providers: ProviderPublic[];
  disabled?: boolean;
}) {
  const { t } = useT();
  const cloud = providers.filter((p) => p.kind !== "native");
  return (
    <select
      className="lp-select jam-ai-pick"
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
    >
      <optgroup label={t("jam.aiNativeGroup")}>
        <option value={NATIVE + NATIVE_ID}>{t("jam.aiNativeOption")}</option>
      </optgroup>
      {models.length > 0 && (
        <optgroup label={t("jam.aiLocalGroup")}>
          {models.map((m) => (
            <option key={m.name} value={LOCAL + m.name}>
              {m.name}
            </option>
          ))}
        </optgroup>
      )}
      {cloud.length > 0 && (
        <optgroup label={t("jam.aiCloudGroup")}>
          {cloud.map((p) => (
            <option key={p.id} value={PROV + p.id}>
              {p.name} · {p.model}
              {p.has_token ? "" : ` (${t("jam.aiNoToken")})`}
            </option>
          ))}
        </optgroup>
      )}
      <optgroup label={t("jam.aiGridGroup")}>
        <option value={GRID}>{t("jam.aiGridOption")}</option>
      </optgroup>
    </select>
  );
}

/// Hora curta (hh:mm:ss) d'un esdeveniment: marca de temps del registre.
function timeText(at: number): string {
  const d = new Date(at);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/// COMPARTIR AMB LA COMUNITAT: commutador que engega el servidor P2P per a
/// la Jam actual i mostra la invitació perquè els voluntaris s'hi connectin.
function GridShareBox({
  compartint,
  invitacio,
  busy,
  onToggle,
}: {
  compartint: boolean;
  invitacio: string;
  busy: boolean;
  onToggle: (on: boolean) => void;
}) {
  const { t } = useT();
  const [copiat, setCopiat] = useState(false);
  const copia = async () => {
    try {
      await navigator.clipboard.writeText(invitacio);
      setCopiat(true);
      window.setTimeout(() => setCopiat(false), 2000);
    } catch {
      // El porta-retalls pot estar bloquejat: el text queda seleccionable.
    }
  };
  return (
    <div className="field">
      <label className="setting-check">
        <input
          type="checkbox"
          checked={compartint}
          disabled={busy}
          onChange={(e) => onToggle(e.target.checked)}
        />
        <Share2 size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
        {t("jam.gridShare")}
      </label>
      <div className="lp-hint">{t("jam.gridShareHint")}</div>
      {compartint && invitacio && (
        <div style={{ display: "flex", gap: 6, marginTop: 6 }}>
          <input
            readOnly
            style={{ flex: 1 }}
            value={invitacio}
            onFocus={(e) => e.currentTarget.select()}
            title={t("jam.gridInvitacioHint")}
          />
          <button className="btn sm" onClick={() => void copia()} title={t("jam.gridCopyHint")}>
            <ClipboardCopy size={12} /> {copiat ? t("jam.gridCopied") : t("jam.gridCopy")}
          </button>
        </div>
      )}
    </div>
  );
}

/// Registre VIU compartit entre muntatges del panell: si l'usuari canvia de
/// sub-pestanya (Agent ▸ xat/Jam/integracions) mentre la Jam treballa, el
/// panell es desmunta i tornava buit, fent pensar que «no fa res». Viure
/// fora del component (mòdul) ho conserva mentre la finestra estigui oberta.
let registreCompartit: JamEvent[] = [];

/// Text que la IA està ESCRIVINT ara mateix, compartit fora del component
/// pel mateix motiu que el registre: en canviar de sub-pestanya el text en
/// viu no es pot perdre (és la prova que la IA treballa).
let liveCompartit = "";

/// PANELL «JAM» — l'agent autònom que construeix el joc sol, en hores,
/// sense que el Mac es pengi (supervisió de memòria al backend).
/// Dissenyat perquè fins i tot un nen de 5 anys pugui fer-ho servir:
/// tria un preset, prem «Comença» i mira com el joc es construeix sol.
export default function JamPanel() {
  const { t } = useT();
  const [brief, setBrief] = useState("");
  const [engine, setEngine] = useState("godot4");
  const [mins, setMins] = useState(60);
  const [customVal, setCustomVal] = useState(2);
  const [customUnit, setCustomUnit] = useState<TimeUnit>("h");
  const [buildCmd, setBuildCmd] = useState("");
  const [plan, setPlan] = useState<JamPlan | null>(null);
  const [state, setState] = useState<JamState | null>(null);
  const [events, setEvents] = useState<JamEvent[]>(() => registreCompartit.slice());
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [reportText, setReportText] = useState<string | null>(null);
  // IA triada per a la Jam (model local, proveïdor en línia o IA integrada).
  const [ai, setAi] = useState("");
  // Els models d'Ollama ja s'han carregat? Ho cal per triar la IA per
  // defecte SENSE ficar la integrada quan en realitat hi ha un model local
  // a l'espera.
  const [modelsCarregats, setModelsCarregats] = useState(false);
  // Estat del botó «Allibera la memòria» + el que ha alliberat.
  const [ramBusy, setRamBusy] = useState(false);
  const [ramMsg, setRamMsg] = useState<string | null>(null);
  // Rellotge (ms): serveix per dir «fa X s» del darrer esdeveniment.
  const [ara, setAra] = useState(() => Date.now());
  // Text que la IA està ESCRIVINT ara mateix (streaming del model local):
  // sense això el panell semblava mort mentre la IA treballava de debò.
  const [liveText, setLiveText] = useState(liveCompartit);
  // Memòria que està usant la IA local (model carregat + consum del sistema).
  const [mem, setMem] = useState<LocalMemory | null>(null);
  // Referència al registre: hi fem autoscroll a cada esdeveniment nou.
  const logRef = useRef<HTMLDivElement | null>(null);
  // Referència al requadre «en viu»: autoscroll al final (el text nou és
  // sempre a sota).
  const liveRef = useRef<HTMLPreElement | null>(null);
  // La Jam està treballant? (el backend ho diu; «stopping» és l'aturada
  // demanada que encara no s'ha completat).
  const running = state?.running ?? false;
  const stopping = state?.stopping ?? false;
  // Proveïdors configurats: només en volem el nom llegible del que fa
  // servir la Jam («OpenAI» en lloc de «openai»).
  const providers = useProviderStore((s) => s.providers);
  const loadProviders = useProviderStore((s) => s.load);
  // Models locals d'Ollama: la Jam pot fer servir el que l'usuari tingui.
  // Es llisten DIRECTAMENT (no es crida el store, que a més canviaria el
  // model actiu global de l'app i tocaria el xat de l'usuari sense que
  // ell ho demani).
  const [models, setModels] = useState<OllamaModel[]>([]);

  // ── XARXA ALTRUISTA P2P: compartir la Jam i distribuir la còpia final ──
  const gridStatus = useGridStore((s) => s.status);
  const gridLoad = useGridStore((s) => s.load);
  const [gridBusy, setGridBusy] = useState(false);
  const [gridErr, setGridErr] = useState<string | null>(null);

  useEffect(() => {
    void gridLoad();
    const un = listen("grid://event", () => void gridLoad());
    return () => {
      void un.then((u) => u());
    };
  }, [gridLoad]);

  /// Engega/atura el servidor P2P per a la Jam actual.
  const canviaCompartir = async (on: boolean) => {
    setGridBusy(true);
    setGridErr(null);
    try {
      if (on) await invoke("grid_share_new");
      else await invoke("grid_share_stop");
      await gridLoad();
    } catch (e) {
      setGridErr(String(e));
    } finally {
      setGridBusy(false);
    }
  };

  /// El projecte ha acabat: ofereix la còpia als participants (MAI sol).
  const distribueix = async () => {
    setGridBusy(true);
    setGridErr(null);
    try {
      await invoke("grid_distribute_start");
      await gridLoad();
    } catch (e) {
      setGridErr(String(e));
    } finally {
      setGridBusy(false);
    }
  };

  /// Revoca la distribució (les còpies a mitges queden a mitges).
  const revocaDist = async () => {
    setGridBusy(true);
    setGridErr(null);
    try {
      await invoke("grid_distribute_stop");
      await gridLoad();
    } catch (e) {
      setGridErr(String(e));
    } finally {
      setGridBusy(false);
    }
  };

  // Estat inicial + esdeveniments en viu.
  useEffect(() => {
    void invoke<JamState>("jam_state").then(setState).catch(() => undefined);
    const un1 = listen<JamState>("jam://state", (e) => setState(e.payload));
    const un2 = listen<JamEventPayload>("jam://event", (e) => {
      // Marca de temps: quan ha arribat l'esdeveniment (per pintar l'hora
      // de cada línia del registre i dir «fa X s» del darrer).
      const ev: JamEvent = { ...e.payload, at: Date.now() };
      // El TEXT EN VIU va a la seva caixa i NO al registre: si hi anés,
      // inundaria el registre i es perdrían les notes que expliquen què fa
      // la IA (tasca, eines, errors).
      if (ev.kind === "stream") {
        liveCompartit = ev.detail.slice(-2500);
        setLiveText(liveCompartit);
        return;
      }
      registreCompartit = [...registreCompartit.slice(-200), ev];
      setEvents((prev) => [...prev.slice(-200), ev]);
    });
    return () => {
      void un1.then((u) => u());
      void un2.then((u) => u());
    };
  }, []);

  // Noms llegibles dels proveïdors (només si encara no tenim la llista).
  useEffect(() => {
    if (providers.length === 0) void loadProviders();
  }, [providers.length, loadProviders]);

  // Models locals d'Ollama: es llisten un sol cop (per oferir-los com a
  // opció de la Jam).
  useEffect(() => {
    let viu = true;
    void invoke<OllamaModel[]>("ollama_list_models")
      .then((m) => {
        if (viu) setModels(m ?? []);
      })
      .catch(() => {
        if (viu) setModels([]);
      })
      .finally(() => {
        if (viu) setModelsCarregats(true);
      });
    return () => {
      viu = false;
    };
  }, []);

  // IA per defecte: el model local si n'hi ha; si no, la IA INTEGRADA
  // (sempre funciona, sense token ni descàrrega). Només es tria quan
  // l'usuari no ha triat res: un cop triada, la seva tria mana.
  useEffect(() => {
    if (ai || !modelsCarregats) return;
    setAi(models.length > 0 ? LOCAL + models[0].name : NATIVE + NATIVE_ID);
  }, [ai, models, modelsCarregats]);

  // Autoscroll del registre: sense això l'usuari ha de fer scroll a mà per
  // veure què està fent la IA (el registre creix per baix).
  useEffect(() => {
    const el = logRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [events]);

  // Autoscroll del text en viu: el que escriu la IA és sempre al final.
  useEffect(() => {
    const el = liveRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [liveText]);

  // MEMÒRIA DE LA IA LOCAL: es refresca sovint mentre la Jam treballa.
  // L'usuari ha de veure quanta RAM consumeix el model (i el sistema) per
  // adivinar si li convé un model més lleuger o aturar la Jam.
  useEffect(() => {
    let viu = true;
    const refresca = async () => {
      try {
        const m = await invoke<LocalMemory>("ai_local_memory");
        if (viu) setMem(m);
      } catch {
        // Ollama aturat o error puntual: es manté el darrer valor conegut.
      }
    };
    void refresca();
    const id = setInterval(() => void refresca(), running ? 3000 : 15000);
    return () => {
      viu = false;
      clearInterval(id);
    };
  }, [running]);

  // Rellotge: un tic per segon mentre la Jam treballa (per «fa X s»).
  useEffect(() => {
    if (!running) return;
    const id = setInterval(() => setAra(Date.now()), 1000);
    return () => clearInterval(id);
  }, [running]);

  // La tria d'IA es resol en (proveïdor, model) per als arguments del
  // backend i per pintar l'etiqueta de la capçalera.
  const tria = useMemo(() => resolAi(ai, providers), [ai, providers]);

  /// Temps efectiu en minuts: el preset triat o el valor personalitzat (retallat
  /// a 1 minut..7 dies, igual que el backend).
  const effectiveMins = mins === CUSTOM ? customMinutes(customVal, customUnit) : mins;

  const doPlan = async () => {
    if (!brief.trim()) {
      setErr(t("jam.errBrief"));
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      const p = await invoke<JamPlan>("jam_plan", {
        request: {
          brief,
          engine,
          durationSecs: effectiveMins * 60,
          buildCmd: buildCmd || null,
          provider: tria.provider,
          model: tria.model,
        },
      });
      setPlan(p);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const doStart = async () => {
    setBusy(true);
    setErr(null);
    registreCompartit = [];
    setEvents([]);
    setReportText(null);
    try {
      await invoke("jam_start", {
        request: {
          brief,
          engine,
          durationSecs: effectiveMins * 60,
          buildCmd: buildCmd || null,
          provider: tria.provider,
          model: tria.model,
        },
      });
      setPlan(null);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const doStop = async () => {
    // Resposta immediata: el botó es posa a «aturant…» abans que torni el
    // backend (que també emet l'estat). Abans no hi havia cap retorn visible
    // i prémer «Atura» semblava que no feia res.
    setState((s) => (s ? { ...s, stopping: true } : s));
    try {
      await invoke("jam_stop");
    } catch (e) {
      // Si l'ordre no arriba al backend es torna enrere: el botó no es pot
      // quedar «Aturant…» per sempre sense que la Jam s'aturi.
      setState((s) => (s ? { ...s, stopping: false } : s));
      setErr(String(e));
    }
  };

  const openReport = async () => {
    // El backend llegeix el REPORT.md i, si encara no hi és, explica QUÈ
    // està passant (Jam fallida, encara en marxa, volum lent...). Abans es
    // llegia el fitxer directament i l'error cru de Rust s'escampava a
    // pantalla: «No s'ha pogut llegir /Volumes/…/REPORT.md: No such file or
    // directory (os error 2)».
    try {
      const text = await invoke<string>("jam_read_report");
      setReportText(text);
      setErr(null);
    } catch (e) {
      setErr(String(e));
    }
  };

  /// Canvia la IA de la Jam. Si la Jam ja corre, el canvi s'aplica AL
  /// MOMENT (el backend el desa i el procés fill el recull i el canvia
  /// sense aturar la Jam); si encara no ha començat, només es queda
  /// escollida per a la propera arrencada.
  const canviaIA = async (v: string) => {
    setAi(v);
    if (!running) return;
    const { provider, model } = resolAi(v, providers);
    if (!provider) return;
    try {
      await invoke("jam_set_ai", { provider, model });
      setNotice(t("jam.aiChanged"));
    } catch (e) {
      setErr(String(e));
    }
  };

  /// Allibera la memòria que el programa ja no necessita: descarrega els
  /// models d'Ollama que estan residents a la RAM i atura els motors locals
  /// (llama.cpp), que entre tots poden ocupar gigabytes.
  const lliuraRam = async () => {
    setRamBusy(true);
    setRamMsg(null);
    try {
      const r = await invoke<RamFreed>("ai_free_ram");
      if (r.models === 0 && r.engines === 0 && r.freed <= 0) {
        setRamMsg(t("jam.ramNone"));
        return;
      }
      const mb = Math.round((r.freed / (1024 * 1024)) * 10) / 10;
      setRamMsg(t("jam.ramFreed", { mb, models: r.models, engines: r.engines }));
    } catch (e) {
      setErr(String(e));
    } finally {
      setRamBusy(false);
    }
  };

  // Nom llegible del proveïdor: el backend dona l'id («ollama»,
  // «noorbit_native»…) i el store el nom de veritat («OpenAI», «Claude…»).
  // Ollama i la IA integrada NO són al store (no són proveïdors
  // configurables), així que tenen el nom aquí.
  const providerLabel = (id: string) =>
    providers.find((p) => p.id === id)?.name ??
    (id === "noorbit_native"
      ? t("jam.aiNative")
      : id === "ollama"
        ? t("jam.aiOllama")
        : id);
  // Etiqueta de la IA que fa servir la Jam: «OpenAI · gpt-4o-mini».
  // Mentre la Jam corre, la que fa servir DE VERITAT (l'envia el backend);
  // abans de començar, la que l'usuari ha triat al desplegable.
  const aiTriada = tria.provider
    ? [providerLabel(tria.provider), tria.model].filter(Boolean).join(" · ")
    : "";
  const aiLabel = running
    ? state?.provider
      ? [providerLabel(state.provider), state.model].filter(Boolean).join(" · ")
      : ""
    : aiTriada;
  // Quant fa (en segons) que ha arribat el darrer esdeveniment: sense això
  // no es pot saber si la IA s'ha quedat encallada o continua treballant.
  const darrer = events.length > 0 ? events[events.length - 1] : null;
  const fa = darrer ? Math.max(0, Math.round((ara - darrer.at) / 1000)) : null;
  // Nom de la carpeta del projecte (l'últim segment del camí): és el que
  // l'usuari reconeix de la barra lateral, no tot el camí absolut.
  const projectName = (state?.projectPath ?? "")
    .replace(/[\\/]+$/, "")
    .split(/[\\/]/)
    .pop() ?? "";
  const revealProject = () => {
    if (!state?.projectPath) return;
    void invoke("reveal_in_finder", { path: state.projectPath }).catch(() => undefined);
  };
  // Motor on què es pot obrir el projecte amb UN CLIC. Només els que tenen
  // editor o terminal propi: Blender, Unity i el web no s'hi obren així.
  const openableEngine =
    engine === "godot4"
      ? "godot"
      : engine === "unreal5"
        ? "unreal"
        : engine === "bevy"
          ? "bevy"
          : null;
  const openInEngine = async () => {
    if (!state?.projectPath || !openableEngine) return;
    setNotice(null);
    try {
      setNotice(
        await invoke<string>("open_project_in_engine", {
          projectPath: state.projectPath,
          engine: openableEngine,
        }),
      );
    } catch (e) {
      setErr(String(e));
    }
  };
  // Denominador REAL: el `total` que envia el backend (el pla que s'està
  // executant de veritat). El `plan` de la preview només serveix mentre no
  // hagi arrencat, i el backend ja el passa a `state.total`.
  const totalTasks = state?.total || plan?.tasks.length || (state?.completed ?? 0);
  // Una tasca «processada» també és la que s'ha bloquejat o s'ha saltat: si
  // no, la barra es quedava enrere quan la IA no acabava una tasca.
  const doneTasks = Math.min(
    totalTasks,
    (state?.completed ?? 0) + (state?.blocked ?? 0) + (state?.skipped ?? 0),
  );
  // Percentatge de RAM usada del sistema (barra del requadre de memòria).
  const memPct = mem && mem.total > 0 ? Math.round((mem.used / mem.total) * 100) : 0;

  return (
    <div className="jam-panel">
      <div className="jam-head">
        <Gamepad2 size={14} />
        <span>{t("jam.title")}</span>
        <span className="jam-head-right">
          {aiLabel && (
            <span className="jam-ai" title={`${t("jam.ai")}: ${aiLabel}`}>
              <Bot size={12} /> {aiLabel}
            </span>
          )}
          {running && (
            <span className="jam-pulse" title={t("jam.pulseHint")}>
              <Activity size={11} /> {t("jam.live")}
              {fa != null && <em>{t("jam.ago", { n: fa })}</em>}
            </span>
          )}
        </span>
      </div>

      {!running && (
        <>
          {/* ── PRESETS PER A NENS (un clic i a jugar) ── */}
          <div className="jam-presets">
            {PRESETS.map((p) => (
              <button
                key={p.label}
                className="jam-preset-btn"
                onClick={() => setBrief(p.brief)}
                title={p.brief}
              >
                <span className="jam-preset-emoji">{p.emoji}</span>
                <span className="jam-preset-label">{p.label}</span>
              </button>
            ))}
          </div>

          <div className="field">
            <label>{t("jam.brief")}</label>
            <textarea
              rows={4}
              value={brief}
              placeholder={t("jam.briefPh")}
              onChange={(e) => setBrief(e.target.value)}
            />
          </div>

          <div className="jam-row">
            <div className="field" style={{ flex: 1 }}>
              <label>{t("jam.engine")}</label>
              <select value={engine} onChange={(e) => setEngine(e.target.value)}>
                {ENGINES.map((e) => (
                  <option key={e.id} value={e.id}>
                    {e.label}
                  </option>
                ))}
              </select>
            </div>
            <div className="field" style={{ flex: 1 }}>
              <label>{t("jam.duration")}</label>
              <select value={mins} onChange={(e) => setMins(Number(e.target.value))}>
                {DURATIONS.map((d) => (
                  <option key={d.mins} value={d.mins}>
                    {d.mins === CUSTOM ? t("work.custom") : d.label}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {mins === CUSTOM && (
            <div className="jam-row">
              <div className="field" style={{ flex: 1 }}>
                <label>{t("work.customTime")}</label>
                <input
                  type="number"
                  min={1}
                  value={customVal}
                  onChange={(e) => setCustomVal(Number(e.target.value))}
                />
              </div>
              <div className="field" style={{ flex: 1 }}>
                <label>{t("work.unit")}</label>
                <select
                  value={customUnit}
                  onChange={(e) => setCustomUnit(e.target.value as TimeUnit)}
                >
                  <option value="min">{t("work.unitMin")}</option>
                  <option value="h">{t("work.unitH")}</option>
                  <option value="d">{t("work.unitD")}</option>
                </select>
              </div>
            </div>
          )}
          {mins === CUSTOM && (
            <div className="lp-hint" style={{ marginTop: -4 }}>
              {formatDuration(effectiveMins * 60_000)} · {t("work.customHint")}
            </div>
          )}

          <div className="field">
            <label>{t("jam.buildCmd")}</label>
            <input
              value={buildCmd}
              placeholder={t("jam.buildCmdPh")}
              onChange={(e) => setBuildCmd(e.target.value)}
            />
          </div>

          {/* ── IA QUE FARÀ LA JAM ── */}
          <div className="field">
            <label>
              <Brain size={12} style={{ marginRight: 4, verticalAlign: -2 }} />
              {t("jam.aiPick")}
            </label>
            <AiPicker
              value={ai}
              onChange={(v) => void canviaIA(v)}
              models={models}
              providers={providers}
            />
            <div className="lp-hint" style={{ marginTop: 4 }}>
              {t("jam.aiPickHint")}
            </div>
          </div>

          {/* COMPARTIR AMB LA COMUNITAT: engega el servidor P2P per a la
              Jam actual (els voluntaris responen els torns amb el seu
              model local i, si cap no respon, la IA local ho fa). */}
          <GridShareBox
            compartint={gridStatus?.compartint ?? false}
            invitacio={gridStatus?.invitacio ?? ""}
            busy={gridBusy}
            onToggle={(on) => void canviaCompartir(on)}
          />
          {gridErr && (
            <div className="si-error" style={{ marginTop: 8 }}>
              <AlertTriangle size={12} /> {gridErr}
            </div>
          )}

          <div className="jam-actions">
            <button className="btn sm" disabled={busy} onClick={() => void doPlan()}>
              <ListChecks size={12} /> {t("jam.plan")}
            </button>
            <button
              className="btn primary"
              style={{ padding: "8px 20px", fontSize: 13, fontWeight: 700 }}
              disabled={busy || !brief.trim()}
              onClick={() => void doStart()}
            >
              {busy ? <Loader2 size={14} className="spin" /> : <Rocket size={14} />} {t("jam.start")}
            </button>
          </div>
        </>
      )}

      {err && (
        <div className="si-error" style={{ marginTop: 8 }}>
          <AlertTriangle size={12} /> {err}
        </div>
      )}

      {/* ── PLA (preview) ── */}
      {plan && !running && (
        <div className="jam-plan">
          <div className="jam-plan-title">
            <ListChecks size={12} /> {plan.title}
          </div>
          <ul className="jam-plan-list">
            {plan.tasks.map((task, i) => (
              <li key={task.id}>
                <b>{i + 1}.</b> {task.title}
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* ── ESTAT EN CURS (i resum final: si no, els comptadors desapareixen
             amb la Jam i l'usuari no veu què s'ha aconseguit) ── */}
      {state && (running || state.finishedAt) && (
        <div className="jam-live-box">
          {/* Barra de progrés + temps (transcorregut, ETA i fin estimada) en
              viu. Mode «tasks»: el progrés són tasques fetes/total i l'ETA es
              retalla al sostre de durada triada (la Jam s'atura sola allí). */}
          <WorkProgress
            running={running}
            startedAtMs={(state.startedAt || 0) * 1000}
            mode="tasks"
            doneUnits={doneTasks}
            totalUnits={totalTasks}
            deadlineMs={
              state.durationSecs > 0
                ? (state.startedAt + state.durationSecs) * 1000
                : null
            }
            finishedAtMs={state.finishedAt ? state.finishedAt * 1000 : null}
          />
          <div className="jam-stats">
            <span>
              <CheckCircle2 size={11} /> {state.completed} de {totalTasks} fets
            </span>
            <span className="jam-bad">
              <AlertTriangle size={11} /> {state.blocked} fallats
            </span>
            <span className="jam-muted">{state.skipped} saltats</span>
          </div>
          {/* El projecte concret on la Jam treballa (o ha treballat): visible
              TAMBÉ després d'acabar, quan la pestanya es tanca i es reobre. */}
          {projectName && (
            <div className="jam-current" title={state.projectPath}>
              <Folder size={11} /> <b>{t("jam.project")}</b> {projectName}
              <button className="btn sm" style={{ marginLeft: 8 }} onClick={revealProject}>
                {t("jam.reveal")}
              </button>
              {openableEngine && (
                <button className="btn sm" style={{ marginLeft: 6 }} onClick={() => void openInEngine()}>
                  <MonitorPlay size={12} /> {t("jam.openInEngine")}
                </button>
              )}
            </div>
          )}
          {notice && (
            <div className="jam-current" style={{ color: "var(--accent)" }}>
              {notice}
            </div>
          )}
          {!running && state.finishedAt && (
            <div className="jam-current">
              <Trophy size={11} /> <b>{t("jam.title")}</b> {t("jam.finished")}
            </div>
          )}
          {/* DISTRIBUCIÓ ESTIL TORRENT: quan la Jam ha acabat, el
              desenvolupador principal pot donar una còpia verificada als
              participants (MAI s'activa sola). */}
          {!running && state.finishedAt && (
            !gridStatus?.distribucio?.activa ? (
              <div className="jam-current">
                <Share2 size={11} />
                <button
                  className="btn sm primary"
                  style={{ marginLeft: 6 }}
                  disabled={gridBusy}
                  onClick={() => void distribueix()}
                >
                  {gridBusy ? <Loader2 size={12} className="spin" /> : <Share2 size={12} />}{" "}
                  {t("jam.gridDistribute")}
                </button>
              </div>
            ) : (
              <div className="jam-current">
                <Share2 size={11} />
                {t("jam.gridDistActive", {
                  peces: gridStatus?.distribucio?.peces ?? 0,
                  baixades: gridStatus?.distribucio?.baixades ?? 0,
                })}
                <button className="btn sm" style={{ marginLeft: 8 }} disabled={gridBusy} onClick={() => void revocaDist()}>
                  {t("jam.gridDistStop")}
                </button>
              </div>
            )
          )}
          {running && (
            <div className="jam-activity">
              <Activity size={11} />
              <b>{state.currentTask || t("jam.workingNow")}</b>
              <span className="jam-activity-detail">{state.currentDetail}</span>
              {fa != null && <em>{t("jam.ago", { n: fa })}</em>}
            </div>
          )}
          {/* QUÈ ESTÀ FENT LA IA ARA MATEIX: el text que el model local està
              escrivint, en directe. Abans el panell només mostrava el títol
              de la tasca i l'usuari pensava que la Jam s'havia penjat. */}
          {liveText && (
            <div className="jam-live">
              <div className="jam-live-head">
                <Activity size={11} /> {t("jam.liveWriting")}
              </div>
              <pre className="jam-live-body" ref={liveRef}>
                {liveText}
              </pre>
            </div>
          )}
          {/* Canvi d'IA EN MARXA: el backend el desa i el procés fill el
              recull, de manera que la Jam continua amb la IA nova sense
              aturar-se ni perdre la feina feta. */}
          {running && (
            <div className="jam-ai-live">
              <label>
                <Bot size={11} style={{ marginRight: 4, verticalAlign: -2 }} />
                {t("jam.aiChange")}
              </label>
              <AiPicker
                value={ai}
                onChange={(v) => void canviaIA(v)}
                models={models}
                providers={providers}
              />
            </div>
          )}
          {/* COMPARTIR EN MARXA: el commutador també es pot prémer mentre
              la Jam treballa (els torns comencen a sortir a la xarxa a
              partir d'aquell moment). */}
          {running && (
            <GridShareBox
              compartint={gridStatus?.compartint ?? false}
              invitacio={gridStatus?.invitacio ?? ""}
              busy={gridBusy}
              onToggle={(on) => void canviaCompartir(on)}
            />
          )}
          {gridErr && (
            <div className="si-error" style={{ marginTop: 8 }}>
              <AlertTriangle size={12} /> {gridErr}
            </div>
          )}
          {running && (
            <button className="btn sm" disabled={stopping} onClick={() => void doStop()}>
              {stopping ? <Loader2 size={12} className="spin" /> : <Square size={12} />}{" "}
              {stopping ? t("jam.stopping") : t("jam.stop")}
            </button>
          )}
          {/* Pantalla de feina a pantalla completa: deixa el Mac/PC mostrant
              el progrés gran mentre la IA treballa sola (i keep_awake evita
              que el sistema s'adormi per inactivitat). */}
          <button
            className="btn sm"
            style={{ marginLeft: 8 }}
            onClick={() => void obrirEstalvi()}
            title={t("jam.workScreen")}
          >
            <MonitorPlay size={12} /> {t("jam.workScreen")}
          </button>
        </div>
      )}

      {/* ── MEMÒRIA DE LA IA LOCAL: quanta RAM està usant ara mateix ── */}
      {mem && (
        <div className="jam-mem" title={t("jam.memHint")}>
          <div className="jam-mem-row">
            <MemoryStick size={11} />
            <span className="jam-mem-label">{t("jam.memLocal")}</span>
            <span className="jam-mem-value">
              {mem.models.length > 0
                ? `${mem.models.map((m) => m.name).join(", ")} · ${fmtBytes(mem.modelBytes)}`
                : t("jam.memNone")}
            </span>
          </div>
          <div className="jam-mem-row">
            <Cpu size={11} />
            <span className="jam-mem-label">{t("jam.memSystem")}</span>
            <span className="jam-mem-bar">
              <i style={{ width: `${memPct}%` }} />
            </span>
            <span className="jam-mem-value">
              {fmtBytes(mem.used)} / {fmtBytes(mem.total)} · {memPct} %
            </span>
          </div>
        </div>
      )}

      {/* ── ALLIBERA LA MEMÒRIA: els models d'Ollama residents i els motors
           locals ocupen gigabytes que, quan no s'usen, no serveixen de res.
           Amb la Jam en marxa es desactiva (ara la memòria S'ESTÀ fent
           servir). ── */}
      <div className="jam-ram">
        <button
          className="btn sm"
          disabled={ramBusy || running}
          onClick={() => void lliuraRam()}
          title={running ? t("jam.ramRunning") : t("jam.ramHint")}
        >
          {ramBusy ? <Loader2 size={12} className="spin" /> : <MemoryStick size={12} />}
          {t("jam.freeRam")}
        </button>
        {ramMsg && <span className="jam-ram-msg">{ramMsg}</span>}
      </div>

      {/* ── REGISTRE ── */}
      {events.length > 0 && (
        <div className="jam-log" ref={logRef}>
          {events.map((e, i) => (
            <div key={i} className={"jam-log-row" + (e.ok ? "" : " bad") + (e.kind === "thinking" ? " thought" : "")}>
              <span className="jam-log-time">{timeText(e.at)}</span>
              {e.kind === "task_done" ? (
                <CheckCircle2 size={11} />
              ) : e.kind === "task_error" || e.kind === "error" ? (
                <AlertTriangle size={11} />
              ) : e.kind === "finished" ? (
                <Trophy size={11} />
              ) : e.kind === "thinking" ? (
                <Brain size={11} />
              ) : (
                <Loader2 size={11} className="spin-slow" />
              )}
              <span className="jam-log-kind">{KIND_LABEL[e.kind] ?? e.kind}</span>
              {e.task && e.kind !== "thinking" && e.kind !== "note" && <b>{e.task}</b>}
              <span
                className="jam-log-detail"
                style={e.kind === "thinking" ? { whiteSpace: "pre-wrap" } : undefined}
              >
                {e.detail}
              </span>
            </div>
          ))}
        </div>
      )}

      {state?.finishedAt && !running && state.reportPath && (
        <div className="jam-report">
          <Trophy size={12} /> {t("jam.report")}: {state.reportPath}
          <button className="btn sm" style={{ marginLeft: 8 }} onClick={() => void openReport()}>
            {t("jam.openReport")}
          </button>
        </div>
      )}

      {reportText && (
        <div className="jam-report-viewer">
          <div className="jam-report-viewer-head">
            <span>REPORT.md</span>
            <button className="icon-btn" onClick={() => setReportText(null)}>
              <X size={14} />
            </button>
          </div>
          <pre className="jam-report-viewer-body">{reportText}</pre>
        </div>
      )}

      {state?.error && (
        <div className="si-error" style={{ marginTop: 8 }}>
          <X size={12} /> {state.error}
        </div>
      )}
    </div>
  );
}
