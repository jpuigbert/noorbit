import { useEffect, useRef, useState, type KeyboardEvent, type ClipboardEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { ListChecks, Zap, CheckCircle2, XCircle, Server, Download, RefreshCw, Cpu, UserCog, Send, Square, Brain, Boxes, X, Plus, Trash2, FolderPlus, FileDown, FolderTree, GitFork, Smartphone, Apple, Monitor, Copy, FolderOpen, ImagePlus, ImageOff } from "lucide-react";
import { useT } from "../../i18n";
import { useAgentStore, useActiveChat, extractRecommendedPaths, extractChatImagePaths } from "../../stores/agentStore";
import { useAIStore } from "../../stores/aiStore";
import { useExpertStore } from "../../stores/expertStore";
import { useProviderStore } from "../../stores/providerStore";
import { usePreviewStore } from "../../stores/previewStore";
import { useSystemStatsStore, memPercent, formatBytes } from "../../stores/systemStatsStore";
import { CopyButton, SaveButton } from "../CopyButton";

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
    setIncludeContext,
    clearChat,
    stop,
  } = useAgentStore();

  // El xat visible: cadascun té la seva conversa, la seva cua, la seva IA i
  // la seva generació en curs. Tots els botons d'aquest panell actuen SOBRE
  // AQUEST XAT; un altre xat pot estar treballant alhora sense molestar-lo.
  const sess = useActiveChat();
  const {
    messages,
    timeline,
    lastResult,
    lastThinking,
    stream,
    running,
    error,
    startedAt,
    lastElapsedMs,
    includeContext,
    contextInfo,
    queue,
  } = sess;

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

  // Proveïdors configurats (Venice, Claude…) per triar la IA d'aquest xat.
  const providers = useProviderStore((s) => s.providers);
  const loadProviders = useProviderStore((s) => s.load);

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
    usePreviewStore.getState().setRightTab("experts");
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
  }, [messages, running, timeline, lastResult, stream, activeId]);

  const hasText = prompt.trim().length > 0;
  // Es pot enviar amb text, amb imatges enganxades, o amb totes dues coses.
  const canSend = hasText || pendingImgs.length > 0;
  const disabled = running || !hasText;
  const pullBusy = Object.keys(pulling).length > 0;

  // Amb expert seleccionat en aquest xat, la tasca la resol ell.
  const activeExpert = experts.find((e) => e.id === sess.expertId) ?? null;

  // Acció principal del xat. Si la IA ja treballa AQUÍ, el text s'afegeix a la
  // cua d'aquest xat; si no, s'envia ara. Les imatges adjuntes viatgen com a
  // entrada visual pels models amb visió; a la cua es posen les seves rutes
  // al text, on «send» les reconeix com a adjunts quan els toca el torn.
  const handleSend = () => {
    const text = prompt.trim();
    const imgs = pendingImgs;
    if (!text && imgs.length === 0) return;
    setPrompt("");
    setPendingImgs([]);
    setAttachErr(null);
    if (running) {
      const queued = [text, ...imgs].filter(Boolean).join("\n");
      enqueue(queued);
    } else {
      void send(text || "(imatge adjunta)", undefined, imgs);
    }
  };

  // Resposta ràpida: consulta de text directa (o l'expert d'aquest xat).
  const handleQuick = () => {
    const text = prompt.trim();
    if (!text || running) return;
    setPrompt("");
    if (activeExpert) void send(text);
    else void quick(text);
  };

  // Crea el projecte: la IA genera els fitxers i s'escriuen al workspace.
  const handleScaffold = () => {
    const text = prompt.trim();
    if (!text || running) return;
    setPrompt("");
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
  // dades amb «image_import» i apareixen com a miniatures sota l'entrada.
  const importImageFiles = async (files: File[]) => {
    for (const f of files) {
      try {
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

  const onPromptPaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const files = Array.from(e.clipboardData?.files ?? []).filter((f) =>
      f.type.startsWith("image/")
    );
    if (files.length === 0) return; // text normal: deixem l'enganxat habitual
    e.preventDefault();
    void importImageFiles(files);
  };

  const doDownload = async () => {
    const name = modelName.trim();
    if (!name || pullBusy) return;
    setShowDownload(false);
    setModelName("");
    await pullModel(name); // es seleccionarà sol en acabar
  };

  // IA d'aquest xat: proveïdor ("" = el global de la configuració) i model
  // d'Ollama ("" = el model global). El model només té sentit en local.
  const chatProvider = sess.provider ?? "";
  const isLocalChat = chatProvider === "" || chatProvider === "ollama";

  return (
    <div className="agent-panel">
      {/* Barra de xats: cadascun treballa en paral·lel. «+» en obre un de nou
          (amb la mateixa o una altra IA) mentre la IA està ocupada en un altre. */}
      <div className="ap-chats-bar">
        {sessions.map((s) => (
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
              {s.title || `${t("agent.chat")} ${sessions.findIndex((x) => x.id === s.id) + 1}`}
            </span>
            {sessions.length > 1 && (
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

      {/* IA D'AQUEST XAT: proveïdor local (Ollama) o qualsevol remot configurat,
          independentment del que usin els altres xats oberts. */}
      <div className="ap-ia-bar">
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
              </option>
            ))}
        </select>
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
        {/* Botons de gestió de models d'Ollama: recarregar la llista i baixar
            un model nou. Formen part d'AQUEST ÚNIC selector d'IA del xat. */}
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
      </div>

      {/* Selector de PLATAFORMA: condiciona el system prompt de la IA perquè
          escriga codi NATIU (Kotlin/Compose o Swift/SwiftUI) dins l'estructura
          que Android Studio / Xcode esperen. Afecta sol AQUEST xat. */}
      <div className="ap-platform-bar" style={{ display: "flex", gap: 4, alignItems: "center" }}>
        <PlatformButton
          active={sess.platform === "desktop"}
          onClick={() => setChatPlatform(activeId, "desktop")}
          icon={<Monitor size={12} />}
          label="Escriptor"
        />
        <PlatformButton
          active={sess.platform === "android"}
          onClick={() => setChatPlatform(activeId, "android")}
          icon={<Smartphone size={12} />}
          label="Android"
        />
        <PlatformButton
          active={sess.platform === "ios"}
          onClick={() => setChatPlatform(activeId, "ios")}
          icon={<Apple size={12} />}
          label="iOS"
        />
        <span style={{ fontSize: 11, color: "var(--text-3)", marginLeft: 6 }}>
          {sess.platform === "android"
            ? "Kotlin + Jetpack Compose (natiu)"
            : sess.platform === "ios"
            ? "Swift + SwiftUI (natiu)"
            : "Lliurement: Python/JS/Rust/etc."}
        </span>
      </div>

      {/* Mode expert per xat: les tasques d'AQUEST xat les resol l'expert triat. */}
      <div className="ap-expert-bar">
        <UserCog size={13} className="ap-model-icon" />
        <select
          className="ap-model-select"
          value={sess.expertId}
          onChange={(e) => setChatExpert(activeId, e.target.value)}
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
          <button className="btn sm" onClick={goCreateExpert}>
            <Plus size={12} /> {t("agent.expertNone")}
          </button>
        )}
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

      <div className="agent-prompt">
        {/* Miniatures de les imatges adjuntes pendents (clic a la X les treu). */}
        {(pendingImgs.length > 0 || attachErr) && (
          <div className="ap-attach-row">
            {pendingImgs.map((p) => (
              <ChatThumb
                key={p}
                path={p}
                onRemove={() => setPendingImgs((x) => x.filter((y) => y !== p))}
              />
            ))}
            {attachErr && <span className="ap-attach-err">{attachErr}</span>}
          </div>
        )}
        <textarea
          placeholder={t("agent.promptPlaceholder") + " · enganxa-hi imatges amb ⌘V"}
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          onPaste={onPromptPaste}
          onKeyDown={onPromptKey}
        />
        <div className="agent-actions">
          {/* Interruptor: la IA d'aquest xat llegeix els fitxers reals del projecte. */}
          <button
            className={"btn sm ap-ctx" + (includeContext ? " active" : "")}
            onClick={() => setIncludeContext(activeId, !includeContext)}
            title={t("agent.ctxHint")}
          >
            <FolderTree size={13} /> {t("agent.ctxToggle")}
          </button>
          {/* Adjuntar imatges des del disc (mateix efecte que enganxar-les). */}
          <button
            className="btn sm ap-ctx"
            onClick={() => fileRef.current?.click()}
            title="Adjunta imatges: es mostren al xat i els models amb visió les reben"
          >
            <ImagePlus size={13} />
          </button>
          <input
            ref={fileRef}
            type="file"
            accept="image/*"
            multiple
            hidden
            onChange={(e) => {
              void importImageFiles(Array.from(e.target.files ?? []));
              e.target.value = "";
            }}
          />
          {/* Què està veient realment la IA: nre. de fitxers o avís. */}
          {includeContext && contextInfo && (
            <span className="ap-ctx-info">{contextInfo}</span>
          )}
        </div>
        <div className="agent-actions">
          {running ? (
            <>
              <button
                className="btn sm"
                disabled={!hasText}
                onClick={handleSend}
                title={t("agent.enqueueHint")}
              >
                <Send size={13} /> {t("agent.addToQueue")}
              </button>
              <button className="btn sm" onClick={() => newChat()} title={t("agent.newChatWhileRunningHint")}>
                <Plus size={13} /> {t("agent.newChatWhileRunning")}
              </button>
              <button className="btn danger sm" onClick={() => void stop(activeId)}>
                <Square size={13} /> {t("common.stop")}
              </button>
            </>
          ) : (
            <>
              <button className="btn primary sm" disabled={!hasText} onClick={handleSend}>
                <Send size={13} /> {t("agent.send")}
              </button>
              <button className="btn sm" disabled={disabled} onClick={() => plan(prompt, "text")}>
                <ListChecks size={13} /> {t("agent.plan")}
              </button>
              <button className="btn sm" disabled={disabled} onClick={handleQuick}>
                <Zap size={13} /> {t("agent.quick")}
              </button>
              <button
                className="btn sm"
                disabled={disabled}
                onClick={handleScaffold}
                title={t("agent.scaffoldHint")}
              >
                <FolderPlus size={13} /> {t("agent.scaffold")}
              </button>
            </>
          )}
        </div>
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

      <div className="agent-body" ref={bodyRef}>
        {/* Capçalera discreta del xat amb l'opció de buidar-lo (només aquest). */}
        {messages.length > 0 && (
          <div className="chat-head">
            <span className="chat-head-title">
              {sess.title || t("agent.chat")}
              {sess.parentId && (
                <span className="chat-fork-origin"> ⑂ {t("agent.forkedFrom")}</span>
              )}
            </span>
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
          return m.role === "user" ? (
            <div key={i} className="chat-row user">
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
              {(m.thinking || (m.steps && m.steps.length > 0)) && (
                <details className="chat-thought">
                  <summary>
                    <Brain size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
                    {t("agent.thinkingTitle")}
                    {m.elapsedMs != null && ` · ${(m.elapsedMs / 1000).toFixed(1)} s`}
                  </summary>
                  {m.steps && m.steps.length > 0 && (
                    <div className="chat-steps">
                      {m.steps.map((s, k) => (
                        <div key={k} className={"timeline-step " + (s.ok ? "ok" : "fail")}>
                          {s.ok ? (
                            <CheckCircle2 size={12} color="var(--success)" />
                          ) : (
                            <XCircle size={12} color="var(--error)" />
                          )}
                          <div>
                            <div style={{ color: "var(--text-0)" }}>{s.label}</div>
                            <div style={{ color: "var(--text-2)" }}>{s.detail}</div>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                  {m.thinking && (
                    <pre className="agent-thinking-body">{m.thinking}</pre>
                  )}
                </details>
              )}
              <div className={"chat-bubble assistant" + (m.error ? " err" : "")}>
                {m.text}
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
              {/* Objectiu 3: còpia del resultat (resposta, raonament) i desa
                  a fitxer, sense eixir del xat. */}
              {!m.error && m.text && (
                <div className="chat-copy-row">
                  <CopyButton text={m.text} label="Resposta" />
                  {m.thinking && <CopyButton text={m.thinking} label="Raonament" />}
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
                  {stream}
                  <span className="stream-caret">▋</span>
                  <div className="stream-meta">
                    <RefreshCw size={11} className="spin" /> {t("agent.running")}
                    {liveSeconds !== null && ` · ${liveSeconds} s`}
                    {ramChip}
                  </div>
                </div>
              ) : (
                <div className="chat-bubble assistant pending">
                  <RefreshCw size={12} className="spin" /> {t("agent.running")}
                  {liveSeconds !== null && ` · ${liveSeconds} s`}
                  {ramChip}
                </div>
              )}
            {/* Si el stream ja anuncia una imatge generada, es mostra al moment. */}
            {extractChatImagePaths(stream ?? "").length > 0 && (
              <div className="chat-imgs">
                {extractChatImagePaths(stream ?? "").map((p) => (
                  <ChatImage key={p} path={p} />
                ))}
              </div>
            )}
            {timeline.length > 0 && (
              <div className="chat-steps">
                {timeline.map((s, k) => (
                  <div key={k} className={"timeline-step " + (s.ok ? "ok" : "fail")}>
                    {s.ok ? (
                      <CheckCircle2 size={12} color="var(--success)" />
                    ) : (
                      <XCircle size={12} color="var(--error)" />
                    )}
                    <div>
                      <div style={{ color: "var(--text-0)" }}>{s.label}</div>
                      <div style={{ color: "var(--text-2)" }}>{s.detail}</div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {error && !running && <div className="si-error">{error}</div>}
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

/** Botó de selecció de plataforma (Escriptor / Android / iOS). */
function PlatformButton({
  active,
  onClick,
  icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <button
      className={"btn sm" + (active ? " primary" : "")}
      onClick={onClick}
      style={{ display: "inline-flex", alignItems: "center", gap: 4 }}
    >
      {icon} {label}
    </button>
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
