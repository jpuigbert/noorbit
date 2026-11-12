import { useEffect, useRef, useState, type KeyboardEvent, type ClipboardEvent, type DragEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ListChecks, Zap, CheckCircle2, XCircle, Server, Download, RefreshCw, Cpu, UserCog, Send, Square, Brain, Boxes, X, Plus, Trash2, FolderPlus, FileDown, FolderTree, GitFork, Smartphone, Apple, Monitor, Copy, FolderOpen, ImageOff, Paperclip, ExternalLink, ChevronDown, Users, Gauge } from "lucide-react";
import { useT } from "../../i18n";
import { useAgentStore, useActiveChat, extractRecommendedPaths, extractChatImagePaths } from "../../stores/agentStore";
import { useAIStore, OLLAMA_CLOUD_MODELS } from "../../stores/aiStore";
import { useExpertStore } from "../../stores/expertStore";
import {
  useProviderStore,
  isOllamaCloudProvider,
  type ProviderKind,
} from "../../stores/providerStore";
import { usePreviewStore } from "../../stores/previewStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useSystemStatsStore, memPercent, formatBytes } from "../../stores/systemStatsStore";
import { useProcessStore } from "../../stores/processStore";
import TaskChecklist from "./TaskChecklist";
import { CopyButton, SaveButton } from "../CopyButton";
import ChatMarkdown from "./ChatMarkdown";
import { useWebAiStore } from "../../stores/webAiStore";
import ChatThought from "./ChatThought";
import LocalAIStatus from "../AI/LocalAIStatus";

export default function AgentPanel() {
  const { t } = useT();
  const [prompt, setPrompt] = useState("");
  const [showDownload, setShowDownload] = useState(false);
  const [modelName, setModelName] = useState("");
  // Imatges adjuntes pendents d'enviar (enganxades amb ⌘V o triades al disc):
  // rutes absolutes dins la carpeta de dades de NoOrbit.
  const [pendingImgs, setPendingImgs] = useState<string[]>([]);
  const [attachErr, setAttachErr] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const promptRef = useRef<HTMLTextAreaElement>(null);
  // Fitxers NO-imatge adjuntats com a CONTEXT (codi, docs…): nom + text.
  const [pendingFiles, setPendingFiles] = useState<{ name: string; text: string }[]>([]);
  // Imatges generades pel xat que arriben per «runtools://image»: es mostren
  // en el moment, sense esperar que el model acabe la resposta.
  const [liveImgs, setLiveImgs] = useState<string[]>([]);
  // Arrossegament d'OS/arbres: posa l'entrada en estat «solta ací».
  const [dragOver, setDragOver] = useState(false);
  // Botó «salt al final» del xat: apareix si l'usuari ha navegat amunt a llegir.
  const [showJump, setShowJump] = useState(false);

  const {
    backends,
    sessions,
    activeId,
    refreshBackends,
    newChat,
    forkChat,
    closeChat,
    setActive,
    setChatProvider,
    setChatModel,
    setChatExpert,
    setChatPlatform,
    send,
    enqueue,
    dequeue,
    plan,
    quick,
    apply,
    scaffold,
    materialize,
    createStructure,
    pushStep,
    appendStream,
    clearChat,
    stop,
  } = useAgentStore();

  // ── Història de xats: NOMÉS es mostren els del projecte actual ─────────
  // Els chats d'altres projectes no es barregen aquí (simplicitat: el calaix
  // és per projecte, i així el panell d'Agent no es mostra res d'altres).
  const bucket = useWorkspaceStore((s) => s.root);
  const visibleSessions = sessions.filter((s) => s.bucket === bucket);

  // El xat visible: cadascun té la seva conversa, la seva cua, la seva IA i
  // la seva generació en curs. Tots els botons d'aquest panell actuen SOBRE
  // AQUEST XAT; un altre xat pot estar treballant alhora sense molestar-lo.
  const sess = useActiveChat();
  const showThinking = useAgentStore((s) => s.showThinking);
  const setShowThinking = useAgentStore((s) => s.setShowThinking);
  const {
    messages,
    lastResult,
    lastThinking,
    stream,
    running,
    error,
    startedAt,
    lastElapsedMs,
    contextInfo,
    queue,
  } = sess;

  // El codi del projecte és SEMPRE context (per norma). Al composer només
  // cal una etiqueta curta; el detall complet viu al tooltip del badge.
  const ctxCount = contextInfo?.match(/(\d+)\s+fitxers/)?.[1];
  const ctxLabel = contextInfo?.startsWith("⚠️")
    ? "Sense codi del projecte"
    : ctxCount
    ? `Codi del projecte · ${ctxCount} fitxers`
    : null;

  const {
    models,
    selectedModel,
    pulling,
    ollamaRunning,
    selectModel,
    pullModel,
    loadModels,
    checkOllama,
  } = useAIStore();

  // Mode expert PER XAT: si en triem un, les tasques d'aquest xat les fa
  // l'expert (rol + model propis). Cada xat pot treballar amb un d'diferent.
  const experts = useExpertStore((s) => s.experts);
  const loadExperts = useExpertStore((s) => s.load);
  // MODE EQUIP (per xat): el text que envies es reparteix entre TOTS els
  // especialistes (plurals/àrbitre del backend) i veus el progrés de cadascun.
  const [teamMode, setTeamMode] = useState(false);

  // Proveïdors configurats (Venice, Claude, Ollama Cloud…) per triar la IA
  // d'aquest xat. `active` és el proveïdor global (el de la configuració).
  const providers = useProviderStore((s) => s.providers);
  // Catàleg d'IAs de web (del backend: DeepSeek, ChatGPT, Claude, Gemini…).
  const webIas = useWebAiStore((s) => s.ias);
  useEffect(() => {
    void useWebAiStore.getState().init();
  }, []);
  const activeProvider = useProviderStore((s) => s.active);
  const loadProviders = useProviderStore((s) => s.load);
  const updateProvider = useProviderStore((s) => s.update);

  // RAM en temps real: es mostra junt al cronòmetre «Treballant…» perquè
  // l'usuari veja si el model està omplint la memòria (causa de bucles/lentitud).
  const mem = useSystemStatsStore((s) => s.mem);
  useEffect(() => {
    useSystemStatsStore.getState().start();
  }, []);

  // No hi ha especialistes: anem al panell «Especialistes» i obrim directament
  // el formulari de creació en muntar-se.
  const goCreateExpert = () => {
    useExpertStore.getState().requestCreate();
    usePreviewStore.getState().setRightTab("agent");
    usePreviewStore.getState().setAgentSub("experts");
  };

  useEffect(() => {
    refreshBackends();
    checkOllama();
    loadModels();
    loadExperts();
    loadProviders();
  }, [refreshBackends, checkOllama, loadModels, loadExperts, loadProviders]);

  // Rep els passos de progrés del backend en temps real (apply Blender/Unreal…).
  // L'event ve ETIQUETAT amb el xat que el va generar: l'afegim a aquell xat,
  // encara que l'usuari n'estigui mirant un altre.
  useEffect(() => {
    const un = listen<{ label: string; detail: string; ok: boolean; session?: string }>(
      "agent://progress",
      (e) => pushStep(e.payload.session ?? "main", {
        label: e.payload.label,
        detail: e.payload.detail,
        ok: e.payload.ok,
      }),
    );
    return () => {
      void un.then((f) => f());
    };
  }, [pushStep]);

  // Rep els fragments de text que la IA escriu en directe (streaming) i els
  // acumula AL XAT que correspon, perquè diversos xats poden generar alhora.
  useEffect(() => {
    const un = listen<{ text: string; session?: string }>("ai://chunk", (e) =>
      appendStream(e.payload.session ?? "main", e.payload.text)
    );
    return () => {
      void un.then((f) => f());
    };
  }, [appendStream]);

  // Cronòmetre en viu: refresca la UI cada segon mentre AQUEST xat treballa.
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!running) return;
    const id = setInterval(() => setTick((x) => x + 1), 1000);
    return () => clearInterval(id);
  }, [running]);
  const liveSeconds = startedAt ? Math.floor((Date.now() - startedAt) / 1000) : null;

  // ── QUÈ ESTÀ FENT ARA MATEIX (en viu), per al xat ────────────────────────
  // Tres fonts, de la més concreta a la més genèrica:
  //  1. L'últim PAS del motor («Blender: Executant l'script…», «ia: La IA
  //     està escrivint…», «Ordre d'ordinador: $ …»): el pas real, no una
  //     estimació.
  //  2. La fase del procés de la IA amb un fragment del RAONAMENT.
  //  3. La fase del procés amb l'última línia del TEXT que ja està escrivint.
  const procSnap = useProcessStore((s) => s.per[activeId]);
  // Només ens serveix el pas si és del TORN ACTUAL (la llista de passos no es
  // buida a cada torn: un pas vell no pot explicar què fa la IA ara mateix).
  const rawStep = sess.timeline.length ? sess.timeline[sess.timeline.length - 1] : null;
  const lastStep =
    rawStep && (!startedAt || rawStep.at >= startedAt - 2000) ? rawStep : null;
  const liveWhat = (() => {
    if (lastStep && lastStep.detail) {
      return lastStep.label ? `${lastStep.label}: ${lastStep.detail}` : lastStep.detail;
    }
    const fase = procSnap?.phase;
    const text = (fase === "thinking" ? procSnap?.thinking : procSnap?.text) ?? "";
    const línia = text
      .split(/\r?\n/)
      .map((l) => l.trim())
      .filter(Boolean)
      .pop();
    if (fase === "thinking" && línia) return `Pensant… ${línia}`;
    if (fase === "streaming" && línia) return `Generant… ${línia}`;
    return "";
  })();
  const liveWhatShort =
    liveWhat.length > 160 ? liveWhat.slice(0, 160) + "…" : liveWhat;

  // RAM en temps real per al cronòmetre: percentatge + detall «usada/total».
  const ramPct = memPercent(mem);
  const ramChip =
    ramPct === null || !mem ? null : (
      <span className="ap-ram" title={`RAM usada ${formatBytes(mem.used)} de ${formatBytes(mem.total)}`}>
        · RAM {ramPct}%
      </span>
    );

  // El xat s'autodesplaça al final quan arriben missatges o passos nous.
  const bodyRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = bodyRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages, running, lastResult, stream, activeId]);

  const hasText = prompt.trim().length > 0;
  // Es pot enviar amb text, amb imatges enganxades, amb fitxers adjunts, o
  // amb qualsevol combinació d'elles.
  const canSend = hasText || pendingImgs.length > 0 || pendingFiles.length > 0;
  const disabled = running || !hasText;
  const pullBusy = Object.keys(pulling).length > 0;

  // Amb expert seleccionat en aquest xat, la tasca la resol ell.
  const activeExpert = experts.find((e) => e.id === sess.expertId) ?? null;

  // Auto-resize: el composer es reinicia a min-height quan s'envia.
  const resetGrow = () => {
    const el = promptRef.current;
    if (el) el.style.height = "auto";
  };

  // Acció principal del xat. Si la IA ja treballa AQUÍ, el text s'afegeix a la
  // cua d'aquest xat; si no, s'envia ara. Les imatges adjuntes viatgen com a
  // entrada visual pels models amb visió; a la cua es posen les seves rutes
  // al text, on «send» les reconeix com a adjunts quan els toca el torn.
  const handleSend = () => {
    const text = prompt.trim();
    const imgs = pendingImgs;
    const files = pendingFiles;
    if (!text && imgs.length === 0 && files.length === 0) return;
    setPrompt("");
    resetGrow();
    // MODE EQUIP: si hi ha especialistes, l'ordre va a l'equip de cop (i el
    // detall del treball es veu a la sub-pestanya Especialistes).
    if (teamMode && experts.length > 0) {
      setPendingImgs([]);
      setPendingFiles([]);
      void useExpertStore.getState().runTeam(text, experts.map((e) => e.id));
      usePreviewStore.getState().setRightTab("agent");
      usePreviewStore.getState().setAgentSub("experts");
      return;
    }
    setPendingImgs([]);
    setPendingFiles([]);
    setAttachErr(null);
    // Els fitxers adjuntats (codi, docs…) es fiquen DINS del missatge com a
    // context real, perquè QUALSEVOL model —sense eina read_file— el puga veure.
    const attachBlock = files
      .map((f) => `\n\n===== ADJUNT: ${f.name} =====\n\`\`\`\n${f.text}\n\`\`\``)
      .join("");
    setLiveImgs([]);
    if (running) {
      const queued = [text, ...imgs].filter(Boolean).join("\n") + attachBlock;
      enqueue(queued);
    } else {
      const base = text || (imgs.length ? "(imatge adjunta)" : "(fitxer adjunt)");
      void send(base + attachBlock, undefined, imgs);
    }
  };

  // Resposta ràpida: consulta de text directa (o l'expert d'aquest xat).
  const handleQuick = () => {
    const text = prompt.trim();
    if (!text || running) return;
    setPrompt("");
    resetGrow();
    if (activeExpert) void send(text);
    else void quick(text);
  };

  // Crea el projecte: la IA genera els fitxers i s'escriuen al workspace.
  const handleScaffold = () => {
    const text = prompt.trim();
    if (!text || running) return;
    setPrompt("");
    resetGrow();
    void scaffold(text);
  };

  // Converteix un missatge JA generat (amb codi) en fitxers reals, sense
  // tornar a preguntar a la IA.
  const handleMaterialize = (text: string) => {
    if (running) return;
    void materialize(text);
  };

  // Forcar: duplica la conversa fins a aquest missatge en un xat NOU i canvia
  // a ell. L'original queda intacte; al fork pots triar una altra IA i
  // continuar el treball per un altre camí.
  const handleFork = (upToIndex: number) => {
    forkChat(activeId, upToIndex);
  };

  // Creació MANUAL d'estructura: un diàleg amb una línia per ruta. Es
  // pre-emplena amb els fitxers/carpetes que la IA RECOMANA al missatge
  // (encara que només n'hagi parlat en prosa); l'usuari l'edita a plaer.
  const [structText, setStructText] = useState<string | null>(null);
  const openStructDialog = (text: string) => {
    const rec = extractRecommendedPaths(text);
    setStructText(rec.join("\n"));
  };
  const submitStructure = async () => {
    if (structText === null) return;
    const lines = structText
      .split(/\r?\n/)
      .map((l) => l.trim())
      .filter((l) => l && !l.startsWith("#"));
    setStructText(null);
    await createStructure(lines);
  };

  // Enter envia el missatge (com un xat); ⇧Enter fa salt de línia.
  const onPromptKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      handleSend();
    }
  };

  // Enganxar (⌘V) o triar fitxers: les imatges es desen a la carpeta de
  // dades amb «image_import» i apareixen com a miniatures; la resta de
  // fitxers (codi, documents…) es llegeixen i s'adjunten com a CONTEXT de text.
  const isImageName = (name: string) => /\.(png|jpe?g|jpeg|gif|bmp|webp|ico|tiff?|heic|avif)$/i.test(name);

  const importImageFiles = async (files: File[]) => {
    for (const f of files) {
      try {
        const isImg = f.type.startsWith("image/") || isImageName(f.name);
        if (!isImg) {
          // Fitxer de text: el llegim i el guardem com a context adjunt.
          const text = await new Promise<string>((resolve, reject) => {
            const r = new FileReader();
            r.onload = () => resolve(String(r.result ?? ""));
            r.onerror = () => reject(r.error);
            r.readAsText(f);
          });
          setPendingFiles((p) => [...p, { name: f.name || "fitxer", text }]);
          continue;
        }
        const dataUrl = await new Promise<string>((resolve, reject) => {
          const r = new FileReader();
          r.onload = () => resolve(String(r.result));
          r.onerror = () => reject(r.error);
          r.readAsDataURL(f);
        });
        const path = await invoke<string>("image_import", {
          dataBase64: dataUrl,
          name: f.name || "enganxat.png",
        });
        setPendingImgs((p) => (p.includes(path) ? p : [...p, path]));
      } catch (e) {
        setAttachErr(String(e));
      }
    }
  };

  // Adjunta RUTES NATIVES (arrossegades des del Finder o des de l'arbre de
  // fitxers). Les imatges van a «pendingImgs»; la resta es llegeix amb
  // «file_context_read» i va al context. Esta funció rep rutes ABSOLUTES reals.
  const attachNativePaths = async (paths: string[]) => {
    for (const path of paths) {
      try {
        if (isImageName(path)) {
          setPendingImgs((p) => (p.includes(path) ? p : [...p, path]));
        } else {
          const ctx = await invoke<{ name: string; text: string }>("file_context_read", { path });
          setPendingFiles((p) => [...p, { name: ctx.name, text: ctx.text }]);
        }
      } catch (e) {
        setAttachErr(String(e));
      }
    }
  };

  const onPromptPaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const files = Array.from(e.clipboardData?.files ?? []);
    if (files.length === 0) return; // text normal: deixem l'enganxat habitual
    e.preventDefault();
    void importImageFiles(files);
  };

  // Arrossegament DES DE L'ARBRE DE FITXERS (HTML5 intern): cada node posa la
  // seua ruta a «application/noorbit-path»; ací la recollim i l'adjuntem.
  const onDropInternal = (e: DragEvent) => {
    e.preventDefault();
    setDragOver(false);
    const p = e.dataTransfer?.getData("application/noorbit-path");
    if (p) void attachNativePaths([p]);
  };

  // Escoltadors globals: imatges generades en directe i fitxers soltats des de
  // el sistema operatiu (Tauri dona les rutes absolutes reals del disc).
  useEffect(() => {
    const unImg = listen<{ path: string; session?: string }>("runtools://image", (e) => {
      if ((e.payload.session ?? "main") === activeId) {
        setLiveImgs((p) => (p.includes(e.payload.path) ? p : [...p, e.payload.path]));
      }
    });
    return () => {
      void unImg.then((f) => f());
    };
  }, [activeId]);

  useEffect(() => {
    let disposed = false;
    let un: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((e) => {
        if (disposed) return;
        if (e.payload.type === "over") setDragOver(true);
        else if (e.payload.type === "leave") setDragOver(false);
        else if (e.payload.type === "drop") {
          setDragOver(false);
          void attachNativePaths(e.payload.paths);
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else un = fn;
      });
    return () => {
      disposed = true;
      un?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const doDownload = async () => {
    const name = modelName.trim();
    if (!name || pullBusy) return;
    setShowDownload(false);
    setModelName("");
    await pullModel(name); // es seleccionarà sol en acabar
  };

  // IA d'aquest xat: proveïdor ("" = el global de la configuració) i model
  // d'Ollama ("" = el model global). El model local només té sentit en local.
  const chatProvider = sess.provider ?? "";
  // Proveïdor REAL que s'usarà: el triat al xat o, si no n'hi ha, el global.
  const effectiveProvider = chatProvider || activeProvider;
  const isLocalChat = effectiveProvider === "" || effectiveProvider === "ollama";
  // Xat WEB: el proveïdor és una IA de navegador («web:deepseek»…). En aquest
  // mode no hi ha models locals ni núvols: la pregunta viatja al navegador.
  const isWebChat = chatProvider.startsWith("web:");
  // Ollama Cloud: un (o més) proveïdors remots que serveixen models gratuïts al
  // núvol. Cal mostrar-ne el model actiu i deixar triar-lo: els proveïdors
  // remots ignoren el model per xat (el porten escrit a sobre), així que el
  // canvi s'aplica al proveïdor.
  const chatProviderEntry = providers.find((p) => p.id === effectiveProvider);
  const isCloudChat = isOllamaCloudProvider(chatProviderEntry);
  const cloudProvider = isCloudChat ? chatProviderEntry : undefined;
  const pickCloudModel = (model: string) => {
    if (!cloudProvider || cloudProvider.model === model) return;
    void updateProvider({
      id: cloudProvider.id,
      name: cloudProvider.name,
      baseUrl: cloudProvider.base_url,
      model,
      kind: (cloudProvider.kind || "openai") as ProviderKind,
      auth: cloudProvider.auth,
      enabled: true,
    });
  };

  return (
    <div className="agent-panel">
      {/* Barra de xats: cadascun treballa en paral·lel. «+» en obre un de nou
          (amb la mateixa o una altra IA) mentre la IA està ocupada en un altre. */}
      <div className="ap-chats-bar">
        {visibleSessions.map((s) => (
          <div
            key={s.id}
            className={
              "ap-chat-tab" +
              (s.id === activeId ? " active" : "") +
              (s.running ? " running" : "") +
              (s.parentId ? " fork" : "")
            }
            onClick={() => setActive(s.id)}
            title={s.title || t("agent.chat")}
          >
            {s.running ? (
              <RefreshCw size={10} className="spin" />
            ) : s.parentId ? (
              <GitFork size={10} />
            ) : null}
            <span className="ap-chat-tab-title">
              {s.title || `${t("agent.chat")} ${visibleSessions.findIndex((x) => x.id === s.id) + 1}`}
            </span>
            {visibleSessions.length > 1 && (
              <button
                className="ap-chat-tab-close"
                onClick={(e) => {
                  e.stopPropagation();
                  void closeChat(s.id);
                }}
                title={t("agent.closeChat")}
              >
                <X size={10} />
              </button>
            )}
          </div>
        ))}
        <button
          className="icon-btn ap-chat-add"
          onClick={() => newChat()}
          title={t("agent.newChat")}
        >
          <Plus size={13} />
        </button>
      </div>

      {/* ══ CONFIGURACIÓ DEL XAT: una sola targeta coherent ═══════════════
          QUINA IA respon (proveïdor + model), QUI treballa (especialista)
          i PER A QUINA PLATAFORMA (escriptori/Android/iOS). Abans eren tres
          barres separades amb el mateix pes visual que omplien el panell. */}
      <div className="ap-toolbar">
        {/* Fila 1: IA i model d'aquest xat. */}
        <div className="ap-toolbar-row">
          <Cpu size={13} className="ap-model-icon" />
          <select
            className="ap-model-select"
            value={chatProvider}
            onChange={(e) => setChatProvider(activeId, e.target.value || null)}
            title={t("agent.providerHint")}
          >
            <option value="">{t("agent.providerGlobal")}</option>
            <option value="ollama">Ollama (local)</option>
            {providers
              .filter((p) => p.enabled && p.id !== "ollama")
              .map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                  {p.model ? ` · ${p.model}` : ""}
                </option>
              ))}
            {/* IAs de WEB: parlen per Internet via el navegador intern. */}
            {webIas.length > 0 && (
              <optgroup label={t("webAi.optgroup")}>
                {webIas.map((w) => (
                  <option key={w.id} value={`web:${w.id}`}>
                    {w.name} · web
                  </option>
                ))}
              </optgroup>
            )}
          </select>
          {isWebChat && (
            <span className="ap-tool-note">{t("webAi.webChatNote")}</span>
          )}
          {/* Ollama Cloud: selector del model del núvol. */}
          {isCloudChat && (
            <select
              className="ap-model-select ap-model-perchat"
              value={cloudProvider?.model ?? ""}
              disabled={!cloudProvider}
              onChange={(e) => pickCloudModel(e.target.value)}
              title={t("agent.cloudModelHint")}
            >
              {!cloudProvider?.model && (
                <option value="">{t("agent.cloudModelNone")}</option>
              )}
              {cloudProvider?.model &&
              !OLLAMA_CLOUD_MODELS.some((m) => m.name === cloudProvider.model) ? (
                <option value={cloudProvider.model}>{cloudProvider.model}</option>
              ) : null}
              {OLLAMA_CLOUD_MODELS.map((m) => (
                <option key={m.name} value={m.name}>
                  {m.name}
                </option>
              ))}
            </select>
          )}
          {/* Model local d'Ollama i gestió (recarregar/baixar). */}
          {isLocalChat && (
            <select
              className="ap-model-select ap-model-perchat"
              value={sess.model ?? ""}
              disabled={models.length === 0}
              onChange={(e) => setChatModel(activeId, e.target.value || null)}
              title={t("agent.modelHint")}
            >
              <option value="">
                {models.length === 0
                  ? ollamaRunning
                    ? t("agent.noModels")
                    : t("agent.ollamaOffline")
                  : t("agent.modelGlobal")}
              </option>
              {models.map((m) => (
                <option key={m.name} value={m.name}>
                  {m.name}
                </option>
              ))}
            </select>
          )}
          {isLocalChat && (
            <>
              <button
                className="icon-btn"
                title={t("common.refresh")}
                onClick={() => loadModels()}
              >
                <RefreshCw size={13} />
              </button>
              <button
                className="icon-btn"
                title={t("agent.downloadModel")}
                onClick={() => setShowDownload((v) => !v)}
              >
                <Download size={13} />
              </button>
            </>
          )}
        </div>

        {/* Fila 2: ROL (especialista) i PLATAFORMA objectiu del codi. */}
        <div className="ap-toolbar-row ap-toolbar-row-role">
          <div className="ap-inline-group">
            <UserCog size={13} className="ap-model-icon" />
            <select
              className="ap-model-select ap-model-role"
              value={sess.expertId}
              onChange={(e) => setChatExpert(activeId, e.target.value)}
              title={t("agent.expertDirect")}
            >
              <option value="">{t("agent.expertDirect")}</option>
              {experts.map((e) => (
                <option key={e.id} value={e.id}>
                  {e.name} · {e.role}
                </option>
              ))}
            </select>
            <span className="ap-expert-count">({experts.length})</span>
            {experts.length === 0 && (
              <button
                className="icon-btn"
                onClick={goCreateExpert}
                title={t("agent.expertNone")}
              >
                <Plus size={12} />
              </button>
            )}
          </div>

          <div
            className="ap-platform"
            title={
              sess.platform === "android"
                ? "Android natiu: Kotlin + Jetpack Compose"
                : sess.platform === "ios"
                ? "iOS natiu: Swift + SwiftUI"
                : "Escriptori: Python, JS, Rust i qualsevol altre llenguatge"
            }
          >
            <button
              className={"ap-plat" + (sess.platform === "desktop" ? " active" : "")}
              onClick={() => setChatPlatform(activeId, "desktop")}
            >
              <Monitor size={12} /> Escriptori
            </button>
            <button
              className={"ap-plat" + (sess.platform === "android" ? " active" : "")}
              onClick={() => setChatPlatform(activeId, "android")}
            >
              <Smartphone size={12} /> Android
            </button>
            <button
              className={"ap-plat" + (sess.platform === "ios" ? " active" : "")}
              onClick={() => setChatPlatform(activeId, "ios")}
            >
              <Apple size={12} /> iOS
            </button>
          </div>
        </div>
      </div>

      {showDownload && (
        <div className="ap-download">
          <input
            className="ap-download-input"
            placeholder={t("agent.downloadPlaceholder")}
            value={modelName}
            autoFocus
            onChange={(e) => setModelName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && doDownload()}
          />
          <button className="btn sm primary" disabled={!modelName.trim() || pullBusy} onClick={doDownload}>
            <Download size={12} /> {t("agent.downloadBtn")}
          </button>
        </div>
      )}

      {pullBusy &&
        Object.entries(pulling).map(([m, msg]) => (
          <div key={m} className="ap-pull-progress">
            <RefreshCw size={11} className="spin" /> {m}: {msg}
          </div>
        ))}

      <div
        className={"agent-prompt" + (dragOver ? " drag-over" : "")}
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={onDropInternal}
      >
        {dragOver && (
          <div className="ap-dropzone">
            <Paperclip size={14} /> Solta ací: imatges com a adjunts, fitxers com a context
          </div>
        )}
        {/* Miniatures de les imatges + xips dels fitxers adjunts pendents. */}
        {(pendingImgs.length > 0 || pendingFiles.length > 0 || attachErr) && (
          <div className="ap-attach-row">
            {pendingImgs.map((p) => (
              <ChatThumb
                key={p}
                path={p}
                onRemove={() => setPendingImgs((x) => x.filter((y) => y !== p))}
              />
            ))}
            {pendingFiles.map((f, i) => (
              <span key={f.name + i} className="ap-file-chip" title={f.name}>
                <Paperclip size={11} /> {f.name}
                <button
                  className="ap-thumb-x"
                  onClick={() => setPendingFiles((x) => x.filter((_, k) => k !== i))}
                  title="Treu aquest fitxer"
                >
                  <X size={10} />
                </button>
              </span>
            ))}
            {attachErr && <span className="ap-attach-err">{attachErr}</span>}
          </div>
        )}
        <textarea
          ref={promptRef}
          placeholder={t("agent.promptPlaceholder") + " · ⌘V imatges · arrossega fitxers ací"}
          value={prompt}
          onChange={(e) => {
            setPrompt(e.target.value);
            // Auto-resize: el composer creix amb el text (màxim 140 px).
            const el = e.target;
            el.style.height = "auto";
            el.style.height = Math.min(el.scrollHeight, 140) + "px";
          }}
          onPaste={onPromptPaste}
          onKeyDown={onPromptKey}
        />
        {/* Peu del composer: a l’esquerra, QUÈ veu la IA (el codi del
            projecte va SEMPRE inclòs) i els adjunts; a la dreta, l’acció
            principal. Les accions secundàries viuen fora, en una fila pròpia,
            perquè no competisquen amb «Envia». */}
        <div className="agent-actions ap-composer-actions">
          <div className="ap-composer-tools">
            <span
              className={
                "ap-ctx-badge" +
                (contextInfo && contextInfo.startsWith("⚠️") ? " warn" : "")
              }
              title={
                contextInfo ??
                "La IA rep sempre el codi del projecte obert: les rutes de tots els fitxers i el contingut dels que hi caben."
              }
            >
              <FolderTree size={12} />
              {ctxLabel || t("agent.ctxAlways")}
            </span>
            {/* Adjuntar QUALSEVOL fitxer: imatges com a entrada visual;
                codi/docs com a context. També es pot arrossegar del Finder. */}
            <button
              className="icon-btn"
              onClick={() => fileRef.current?.click()}
              title="Adjunta un fitxer: imatges es mostren al xat; codi/documents s'envien com a context"
            >
              <Paperclip size={13} />
            </button>
            <input
              ref={fileRef}
              type="file"
              multiple
              hidden
              onChange={(e) => {
                void importImageFiles(Array.from(e.target.files ?? []));
                e.target.value = "";
              }}
            />
            <button
              className="icon-btn"
              onClick={() => {
                const root = useWorkspaceStore.getState().root;
                if (root) void invoke("open_externally", { path: root }).catch(() => undefined);
                else setAttachErr("No hi ha cap projecte obert.");
              }}
              title="Obri la carpeta del projecte amb l'aplicació per defecte"
            >
              <FolderOpen size={13} />
            </button>
          </div>
          <div className="ap-composer-primary">
            {running ? (
              <>
                <button
                  className="btn sm"
                  disabled={!canSend}
                  onClick={handleSend}
                  title={t("agent.enqueueHint")}
                >
                  <Send size={13} /> {t("agent.addToQueue")}
                </button>
                <button
                  className="icon-btn"
                  onClick={() => newChat()}
                  title={t("agent.newChatWhileRunningHint")}
                >
                  <Plus size={13} />
                </button>
                <button className="btn danger sm" onClick={() => void stop(activeId)}>
                  <Square size={13} /> {t("common.stop")}
                </button>
              </>
            ) : (
              <button className="btn primary sm" disabled={!canSend} onClick={handleSend}>
                <Send size={13} /> {t("agent.send")}
              </button>
            )}
          </div>
        </div>
      </div>

      {/* ACCIONS SECUNDÀRIES: planificar, resposta ràpida, equip i crear el
          projecte. Fila pròpia perquè no tapen l'acció principal. */}
      {!running && (
        <div className="ap-secondary-actions">
          <button className="btn sm" disabled={disabled} onClick={() => plan(prompt, "text")}>
            <ListChecks size={13} /> {t("agent.plan")}
          </button>
          <button className="btn sm" disabled={disabled} onClick={handleQuick}>
            <Zap size={13} /> {t("agent.quick")}
          </button>
          {experts.length > 0 ? (
            <button
              className={"btn sm" + (teamMode ? " primary" : "")}
              disabled={disabled}
              title={t("agent.teamToggleHint")}
              onClick={() => setTeamMode(!teamMode)}
            >
              <Users size={13} /> {t("agent.teamToggle")} ({experts.length})
            </button>
          ) : (
            <button
              className="btn sm"
              disabled={disabled}
              title={t("agent.teamCreateHint")}
              onClick={goCreateExpert}
            >
              <Users size={13} /> {t("agent.teamCreate")}
            </button>
          )}
          <button
            className="btn sm"
            disabled={disabled}
            onClick={handleScaffold}
            title={t("agent.scaffoldHint")}
          >
            <FolderPlus size={13} /> {t("agent.scaffold")}
          </button>
        </div>
      )}
      </div>

      {/* Cua de missatges afegits mentre LA IA d'aquest xat treballa. */}
      {queue.length > 0 && (
        <div className="ap-queue">
          <span className="ap-queue-title">
            {t("agent.queueTitle")} · {queue.length}
          </span>
          {queue.map((q, i) => (
            <div key={i} className="ap-queue-item">
              <span className="ap-queue-text">{q}</span>
              <button
                className="icon-btn"
                onClick={() => dequeue(i)}
                title={t("agent.queueRemove")}
              >
                <X size={11} />
              </button>
            </div>
          ))}
        </div>
      )}

      {/* Aplica el text al destí 3D triat: la IA genera l'script i l'executa
          dins de Blender o Unreal. Habilitat només si hi ha text escrit. */}
      <div className="ap-apply-bar">
        <Boxes size={13} className="ap-model-icon" />
        <span className="ap-apply-label">{t("agent.applyTo")}</span>
        <button
          className="btn sm"
          disabled={disabled || running}
          onClick={() => void apply("blender", prompt)}
          title={t("agent.applyBlenderHint")}
        >
          {t("agent.applyBlender")}
        </button>
        <button
          className="btn sm"
          disabled={disabled || running}
          onClick={() => void apply("unreal", prompt)}
          title={t("agent.applyUnrealHint")}
        >
          {t("agent.applyUnreal")}
        </button>
      </div>

      {/* Checklist compartida FIXADA A DAMUNT DEL XAT: tasques de la IA i de
          l'usuari que no s'han de perdre amb l'scroll. S'injecta al context de
          la principal i dels agents perquè ningú es perda cap punt del treball.
          Quan la IA treballa, mostra EN VIU la tasca «@doing:» i l'últim pas.
          L'usuari la pot amagar amb el botó ▲ (es recorda entre sessions). */}
      <TaskChecklist
        running={running}
        liveStep={
          sess.timeline.length > 0
            ? sess.timeline[sess.timeline.length - 1]
            : null
        }
      />

      <div
        className="agent-body"
        ref={bodyRef}
        onScroll={(e) => {
          const el = e.currentTarget;
          const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 90;
          setShowJump(!nearBottom);
        }}
      >
        {/* Capçalera discreta del xat amb l'opció de buidar-lo (només aquest). */}
        {messages.length > 0 && (
          <div className="chat-head">
            <span className="chat-head-title">
              {sess.title || t("agent.chat")}
              {sess.parentId && (
                <span className="chat-fork-origin"> ⑂ {t("agent.forkedFrom")}</span>
              )}
            </span>
            {/* Amagar el text del raonament: per a qui el vol llegir i per a
                qui l'estorba. Es recorda entre sessions. */}
            <button
              className="icon-btn"
              onClick={() => setShowThinking(!showThinking)}
              title={showThinking ? t("agent.hideThinking") : t("agent.showThinking")}
              style={{ opacity: showThinking ? 0.85 : 0.45 }}
            >
              <Brain size={12} />
            </button>
            <button className="icon-btn" onClick={() => clearChat(activeId)} title={t("agent.chatClear")}>
              <Trash2 size={12} />
            </button>
          </div>
        )}

        {/* L'estat dels servidors es mostra discretament només quan no hi ha
            conversa; el xat és el protagonista. */}
        {messages.length === 0 && (
          <>
            <div className="agent-section-head">
              <span>
                <Server size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
                {t("agent.backends")}
              </span>
              <button className="btn sm" onClick={refreshBackends}>
                {t("agent.checkBackends")}
              </button>
            </div>
            {backends.map((b) => (
              <div key={b.id} className="backend-row">
                <span>
                  {b.name}{" "}
                  <span style={{ color: "var(--text-3)" }}>({b.modality})</span>
                </span>
                <span style={{ color: b.available ? "var(--success)" : "var(--text-3)" }}>
                  {b.available ? t("agent.available") : t("agent.unavailable")}
                </span>
              </div>
            ))}
          </>
        )}

        {/* Historial del xat: bombolles d'usuari i de la IA, amb el
            raonament plegable i la durada, com els clients d'IA moderns. */}
        {messages.map((m, i) => {
          // Imatges del missatge: les adjuntes explícitament + les rutes que
          // apareixen al text (p. ex. generades per la IA amb image_generate).
          const msgImgs = Array.from(
            new Set([...(m.images ?? []), ...extractChatImagePaths(m.text)])
          );
          // Xats d'IA moderns: si dos missatges d'usuari van per sèrie (sense
          // resposta de la IA entre ells), es fusionen en UN sol bubble.
          if (m.role === "user" && i > 0 && messages[i - 1]?.role === "user") {
            return null;
          }
          return m.role === "user" ? (            <div key={i} className="chat-row user">
              <div className="chat-bubble user">{m.text}</div>
              {msgImgs.length > 0 && (
                <div className="chat-imgs">
                  {msgImgs.map((p) => (
                    <ChatImage key={p} path={p} />
                  ))}
                </div>
              )}
              {/* Fork des de la pregunta: prova un altre camí (altra IA, altre
                  especialista) sense perdre la conversa original. */}
              <button
                className="btn sm ghost chat-fork-btn"
                onClick={() => handleFork(i)}
                title={t("agent.forkHint")}
              >
                <GitFork size={11} /> {t("agent.fork")}
              </button>
            </div>
          ) : (
            <div key={i} className="chat-row assistant">
              {m.thinking && showThinking && (
                /* Sempre OBERT i en CATALÀ (ChatThought): el raonament és
                   la part que més es vol seguir — amb traducció automàtica
                   si el model pensa en anglès, i «Original» a un clic.
                   Si l'usuari l'ha amagat (botó 🧠 del capçal), no es mostra. */
                <ChatThought thinking={m.thinking} elapsedMs={m.elapsedMs} />
              )}
              <div className={"chat-bubble assistant" + (m.error ? " err" : "")}>
                <ChatMarkdown text={m.text} />
              </div>
              {/* Imatges esmentades a la resposta: previsualització amb botons
                  de copiar al porta-retalls i mostrar al Finder. */}
              {msgImgs.length > 0 && (
                <div className="chat-imgs">
                  {msgImgs.map((p) => (
                    <ChatImage key={p} path={p} />
                  ))}
                </div>
              )}
              {/* CONTEXT I TOKENS: quant ha ocupat el torn i quant del context
                  del model s'ha consumit. Indispensable amb les APIs de núvol
                  (per saber què costa cada resposta). */}
              {m.usage && (m.usage.input > 0 || m.usage.output > 0) && (
                <div
                  className="chat-usage"
                  title={
                    m.usage.contextWindow > 0
                      ? `Context del model: ${m.usage.contextWindow.toLocaleString("ca")} tokens`
                      : undefined
                  }
                >
                  <Gauge size={11} />
                  <span>
                    {m.usage.input.toLocaleString("ca")} entrada ·{" "}
                    {m.usage.output.toLocaleString("ca")} sortida
                  </span>
                  {m.usage.contextWindow > 0 && (
                    <>
                      <span className="chat-usage-sep">·</span>
                      <span>
                        {m.usage.contextUsedPct.toFixed(0)}% del context (
                        {m.usage.contextWindow.toLocaleString("ca")})
                      </span>
                      <span className="chat-usage-bar" aria-hidden>
                        <span
                          className="chat-usage-fill"
                          style={{
                            width: `${Math.max(2, Math.min(100, m.usage.contextUsedPct))}%`,
                          }}
                        />
                      </span>
                    </>
                  )}
                  {m.usage.provider && (
                    <>
                      <span className="chat-usage-sep">·</span>
                      <span className="chat-usage-model">{m.usage.model || m.usage.provider}</span>
                    </>
                  )}
                </div>
              )}
              {/* Objectiu 3: còpia del resultat (resposta, raonament) i desa
                  a fitxer, sense eixir del xat. */}
              {!m.error && m.text && (
                <div className="chat-copy-row">
                  <CopyButton text={m.text} label="Resposta" />
                  {m.thinking && showThinking && (
                    <CopyButton text={m.thinking} label="Raonament" />
                  )}
                  <SaveButton text={m.text} suggestedName="resposta-ia.md" />
                  {/* Fork des de la resposta: continua a partir d'aquí amb una
                      altra IA en un xat nou. */}
                  <button
                    className="btn sm ghost chat-fork-btn"
                    onClick={() => handleFork(i)}
                    title={t("agent.forkHint")}
                  >
                    <GitFork size={11} /> {t("agent.fork")}
                  </button>
                </div>
              )}
              {/* Si el missatge conté codi o fitxers @file:, un clic els escriu
                  al projecte obert (sense tornar a consultar la IA). */}
              {!m.error && (m.text.includes("```") || /@?file:/i.test(m.text)) && (
                <button
                  className="btn sm chat-materialize"
                  disabled={running}
                  onClick={() => handleMaterialize(m.text)}
                  title={t("agent.materializeHint")}
                >
                  <FileDown size={12} /> {t("agent.materialize")}
                </button>
              )}
              {/* Creació a mà: parteix de les rutes que la IA recomana (també
                  en prosa) i l'usuari decideix quines es creen de veritat. */}
              {!m.error && (
                <button
                  className="btn sm chat-materialize"
                  disabled={running}
                  onClick={() => openStructDialog(m.text)}
                  title={t("agent.structHint")}
                >
                  <FolderTree size={12} /> {t("agent.structBtn")}
                </button>
              )}
              {m.elapsedMs != null && (
                <span className="agent-elapsed">
                  {t("agent.elapsed")} {(m.elapsedMs / 1000).toFixed(1)} s
                </span>
              )}
            </div>
          );
        })}

        {/* Torn en curs: bombolla pendent amb els passos que van arribant. */}
        {running && (
          <div className="chat-row assistant">
              {stream ? (
                <div className="chat-bubble assistant streaming">
                  <ChatMarkdown text={stream} />
                  <span className="stream-caret">▋</span>
                  <div className="stream-meta">
                    <RefreshCw size={11} className="spin" /> {t("agent.running")}
                    {liveSeconds !== null && ` · ${liveSeconds} s`}
                    {ramChip}
                    {/* El text ja generat es pot copiar AL MOMENT (no cal
                        esperar que acabi la resposta). */}
                    {stream && <CopyButton text={stream} label={t("agent.copyNow")} />}
                  </div>
                  {/* QUÈ ESTÀ FENT ARA MATEIX: el pas real del motor o la fase
                      de la IA amb un fragment del que escriu. */}
                  {liveWhatShort && (
                    <div className="stream-what" title={liveWhat}>
                      {liveWhatShort}
                    </div>
                  )}
                </div>
              ) : (
                <div className="chat-bubble assistant pending">
                  <RefreshCw size={12} className="spin" /> {t("agent.running")}
                  {liveSeconds !== null && ` · ${liveSeconds} s`}
                  {ramChip}
                  {liveWhatShort && (
                    <div className="stream-what" title={liveWhat}>
                      {liveWhatShort}
                    </div>
                  )}
                </div>
              )}
            {/* Imatges generades en este torn: les anunciades pel stream MÉS
                les que arriben per «runtools://image» (apareixen a l'instant,
                sense esperar que el model acabe la resposta). */}
            {Array.from(new Set([...extractChatImagePaths(stream ?? ""), ...liveImgs])).length > 0 && (
              <div className="chat-imgs">
                {Array.from(new Set([...extractChatImagePaths(stream ?? ""), ...liveImgs])).map((p) => (
                  <ChatImage key={p} path={p} />
                ))}
              </div>
            )}
          </div>
        )}

        {error && !running && <div className="si-error">{error}</div>}

        {/* Botó «salt al final»: apareix només si l'usuari ha scrollat amunt. */}
        {showJump && (
          <button
            className="chat-jump"
            onClick={() => {
              const el = bodyRef.current;
              if (el) el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
            }}
          >
            <ChevronDown size={13} /> {t("agent.jumpToBottom")}
          </button>
        )}
      </div>

      {/* Diàleg de creació manual d'estructura (rutes editables línia a línia). */}
      {structText !== null && (
        <div className="modal-overlay" onClick={() => setStructText(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-head">
              <span>{t("agent.structTitle")}</span>
              <button className="icon-btn" onClick={() => setStructText(null)}>
                <X size={16} />
              </button>
            </div>
            <div className="modal-body">
              <p className="ap-struct-help">{t("agent.structHelp")}</p>
              <textarea
                className="ap-struct-list"
                autoFocus
                spellCheck={false}
                value={structText}
                placeholder={t("agent.structPh")}
                onChange={(e) => setStructText(e.target.value)}
              />
              <div className="tree-dialog-actions">
                <button className="btn sm" onClick={() => setStructText(null)}>
                  {t("common.cancel")}
                </button>
                <button
                  className="btn sm primary"
                  disabled={!structText.trim()}
                  onClick={() => void submitStructure()}
                >
                  <FolderPlus size={12} /> {t("agent.structCreate")}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/** Cau de data-URLs de les imatges ja carregades: evita rellegir-les del
    disc cada vegada que es repinta el xat. */
const imgCache = new Map<string, string>();

/** Llegeix una imatge local (Rust: «image_preview_base64») com a data-URL. */
function useImageDataUrl(path: string): string | null {
  const [url, setUrl] = useState<string | null>(() => imgCache.get(path) ?? null);
  useEffect(() => {
    const hit = imgCache.get(path);
    if (hit) {
      setUrl(hit);
      return;
    }
    let alive = true;
    invoke<string>("image_preview_base64", { path })
      .then((d) => {
        imgCache.set(path, d);
        if (alive) setUrl(d);
      })
      .catch(() => {
        if (alive) setUrl(null);
      });
    return () => {
      alive = false;
    };
  }, [path]);
  return url;
}

/** Imatge dins del xat: previsualització + copiar al porta-retalls + mostrar
    al Finder/explorador. Els models amb visió la reben si l'usuari l'adjunta. */
function ChatImage({ path }: { path: string }) {
  const url = useImageDataUrl(path);
  const name = path.split(/[\\/]/).pop() ?? path;

  // Copiar la imatge (no només el text): si el format no és PNG es converteix
  // amb un canvas, perquè el porta-retalls només accepta PNG de forma fiable.
  const copyImage = async () => {
    try {
      if (!url) throw new Error("sense dades");
      const blob = await (await fetch(url)).blob();
      let png = blob;
      if (blob.type !== "image/png") {
        const bmp = await createImageBitmap(blob);
        const canvas = new OffscreenCanvas(bmp.width, bmp.height);
        canvas.getContext("2d")?.drawImage(bmp, 0, 0);
        png = await canvas.convertToBlob({ type: "image/png" });
      }
      await navigator.clipboard.write([new ClipboardItem({ "image/png": png })]);
    } catch {
      // Si no es pot copiar la imatge, almenys la ruta al porta-retalls.
      await navigator.clipboard.writeText(path).catch(() => undefined);
    }
  };

  return (
    <figure className="chat-img" title={path}>
      {url ? (
        <img src={url} alt={name} loading="lazy" />
      ) : (
        <span className="chat-img-broken">
          <ImageOff size={14} /> {name}
        </span>
      )}
      <figcaption className="chat-img-actions">
        <button
          className="btn sm ghost"
          onClick={() => void copyImage()}
          title="Copia la imatge al porta-retalls"
        >
          <Copy size={11} /> Copiar
        </button>
        <button
          className="btn sm ghost"
          onClick={() => void invoke("reveal_in_finder", { path }).catch(() => undefined)}
          title="Mostra al Finder / explorador de fitxers"
        >
          <FolderOpen size={11} /> Mostra
        </button>
        <button
          className="btn sm ghost"
          onClick={() => void invoke("open_externally", { path }).catch(() => undefined)}
          title="Obri amb l'aplicació per defecte (exportar a un altre programa)"
        >
          <ExternalLink size={11} /> Obri
        </button>
      </figcaption>
    </figure>
  );
}

/** Miniatura d'una imatge adjunta pendent d'enviar, amb botó per treure-la. */
function ChatThumb({ path, onRemove }: { path: string; onRemove: () => void }) {
  const url = useImageDataUrl(path);
  return (
    <span className="ap-thumb" title={path}>
      {url ? <img src={url} alt="" /> : <ImageOff size={14} />}
      <button className="ap-thumb-x" onClick={onRemove} title="Treu aquesta imatge">
        <X size={10} />
      </button>
    </span>
  );
}
