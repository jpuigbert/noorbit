import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { X, Download, Trash2, RefreshCw, Check, Search, Cloud, Play, HardDrive, Zap, Globe, MemoryStick, Cpu, Film } from "lucide-react";
import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";
import { useAIStore, OLLAMA_CLOUD_MODELS, OLLAMA_CLOUD_URL } from "../../stores/aiStore";
import { useProviderStore, isOllamaCloudProvider } from "../../stores/providerStore";

function fmtSize(bytes: number): string {
  if (!bytes) return "—";
  const gb = bytes / 1e9;
  return gb >= 1 ? `${gb.toFixed(1)} GB` : `${(bytes / 1e6).toFixed(0)} MB`;
}

export default function ModelManagerModal() {
  const show = useUIStore((s) => s.showModelManager);
  const setShow = useUIStore((s) => s.setShowModelManager);
  const { t } = useT();

  const {
    models,
    cloud,
    suggested,
    cloudLoading,
    cloudError,
    internet,
    internetLoading,
    internetError,
    ollamaRunning,
    ollamaInstalled,
    comfyRunning,
    comfyInstalled,
    imageCpu,
    imageCpuProgress,
    loadImageCpu,
    installImageCpu,
    downloadCpuModel,
    selectedModel,
    pulling,
    lastMessage,
    loadModels,
    refreshProviders,
    checkOllama,
    checkInstalled,
    checkComfy,
    searchCloud,
    searchInternet,
    installOllama,
    startOllama,
    installComfy,
    startComfy,
    pullModel,
    pullBayModel,
    deleteModel,
    selectModel,
    modelsDir,
    volumes,
    migrating,
    loaded,
    keepAlive,
    loadModelsDir,
    setModelsDir,
    migrateModels,
    internetSource,
    setInternetSource,
    installLocalAi,
    loadLoaded,
    unloadModels,
    setKeepAlive,
  } = useAIStore();

  const [custom, setCustom] = useState("");
  const [ramMsg, setRamMsg] = useState("");
  const [cloudQuery, setCloudQuery] = useState("");
  // Cerca a INTERNET (Hugging Face) per a models que NO són a Ollama.
  const [internetQuery, setInternetQuery] = useState("");
  // Connexió a Ollama Cloud (models gratuïts al núvol, sense descàrrega).
  const [cloudKey, setCloudKey] = useState("");
  // Model gran pendent de descarregar quan cal decidir ON (disc extern o
  // disc intern): ≥ 8 GB i cap carpeta de models triada encara.
  const [askDisk, setAskDisk] = useState<{ name: string; size: number | null } | null>(null);
  // Avís de la instala·lació d'editors de vídeo (per si el Terminal falla).
  const [videoMsg, setVideoMsg] = useState("");
  // Motor local SENSE Ollama (llama.cpp): estat i models GGUF ja baixats.
  const [llama, setLlama] = useState<{
    installed: boolean;
    binary: string | null;
    models: [string, number][];
    running: { port: number; model: string; path: string; pid: number }[];
    /// Carpeta dels models GGUF (pot ser un disc extern).
    modelsDir: string;
    /// Carpeta del motor local (pot ser un disc extern).
    engineDir: string;
  } | null>(null);
  const [llamaMsg, setLlamaMsg] = useState("");

  const loadLlama = async () => {
    const st = await invoke<{
      installed: boolean;
      binary: string | null;
      models: [string, number][];
      running: { port: number; model: string; path: string; pid: number }[];
      modelsDir: string;
      engineDir: string;
    }>("llamacpp_status").catch(() => null);
    if (st) setLlama(st);
  };

  /// Tria on es desen els models GGUF i on viu el motor local: pot ser un
  /// DISC EXTERN (USB/HDD) i així el model s'hi queda i s'hi pot treballar
  /// des d'allà (fins i tot endurant-lo a una altra màquina).
  const setLlamaDirs = async (modelsDir?: string, engineDir?: string) => {
    try {
      const msg = await invoke<string>("llamacpp_set_dirs", {
        modelsDir: modelsDir ?? null,
        engineDir: engineDir ?? null,
      });
      setLlamaMsg(msg);
      await loadLlama();
    } catch (e) {
      setLlamaMsg(String(e));
    }
  };
  const pickLlamaModelsDir = async () => {
    const sel = await open({ directory: true });
    if (typeof sel === "string") await setLlamaDirs(sel, undefined);
  };
  const pickLlamaEngineDir = async () => {
    const sel = await open({ directory: true });
    if (typeof sel === "string") await setLlamaDirs(undefined, sel);
  };
  // El catàleg del núvol és molt més llarg que la llista base: qualsevol nom
  // allotjat a ollama.com es pot usar sense baixar-lo res al disc.
  const [cloudCustom, setCloudCustom] = useState("");
  const providers = useProviderStore((s) => s.providers);
  const activeProvider = useProviderStore((s) => s.active);
  const loadProviders = useProviderStore((s) => s.load);
  const addProvider = useProviderStore((s) => s.add);
  const updateProvider = useProviderStore((s) => s.update);
  const selectProvider = useProviderStore((s) => s.select);
  // El model concret que ara mateix serveix el núvol d'Ollama: així només es
  // marca aquell model, no tots a la vegada. Pot tenir un id automàtic si
  // l'usuari el va afegir a mà des del gestor de proveïdors.
  const cloudProvider = providers.find(isOllamaCloudProvider);
  // El proveïdor global actiu és del núvol d'Ollama?
  const cloudIsActive = isOllamaCloudProvider(
    providers.find((p) => p.id === activeProvider),
  );

  useEffect(() => {
    if (show) {
      checkOllama();
      checkInstalled();
      checkComfy();
      loadImageCpu();
      loadModels();
      loadModelsDir();
      loadLoaded();
      loadProviders();
      void loadLlama();
      // Carrega el catàleg de models recomanats (opcions instal·lables).
      refreshProviders();
      // El navegador del núvol funciona sempre, tingues o no Ollama.
      searchCloud("");
    }
  }, [show]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!show) return null;

  const doCustomPull = () => {
    const name = custom.trim();
    if (!name) return;
    pullModel(name);
    setCustom("");
  };

  // Baixa un model del núvol. Si Ollama no roda, intenta arrencar-lo
  // (o instal·lar-lo si no existeix) abans de descarregar.
  // ABANS DE DESCARREGAR-HO TOT: si el model pesa molt (≥ 8 GB) i encara no
  // has triat cap disc extern, PREGUNTEM on el vols — els models grans no
  // caben sempre al disc intern, i Ollama pot treballar des d'un USB/SSD.
  const pullCloud = async (name: string, size?: number | null) => {
    if (!ollamaRunning) {
      if (ollamaInstalled) await startOllama();
      else {
        await installOllama();
        return;
      }
    }
    if ((size ?? 0) >= 8_000_000_000 && !modelsDir) {
      setAskDisk({ name, size: size ?? null });
      return;
    }
    await pullModel(name);
  };

  // La decisió de disc presa: baixa al destí triat.
  const pullTo = (dir: string | null) => {
    const ask = askDisk;
    setAskDisk(null);
    if (!ask) return;
    if (dir) {
      void setModelsDir(dir).then(() => pullModel(ask.name));
    } else {
      void pullModel(ask.name);
    }
  };

  // Tria on viuran els models (p. ex. un USB extern) i l'aplica.
  const pickModelsDir = async () => {
    const selected = await open({ directory: true });
    if (typeof selected === "string") await setModelsDir(selected);
  };

  // Instal·la un editor de vídeo IA descarregable (Kdenlive, OpenShot,
  // DaVinci, Shotcut) amb el mateix camí que ComfyUI: Terminal + cask.
  const installEditor = async (id: string) => {
    try {
      const msg = await invoke<string>("install_video_editor", { editor: id });
      setVideoMsg(msg);
    } catch (e) {
      setVideoMsg(String(e));
    }
    setTimeout(() => setVideoMsg(""), 15_000);
  };

  // Connecta/actualitza el proveïdor del núvol d'Ollama amb el model triat i
  // l'activa. Reutilitza el sistema de proveïdors (compatible amb OpenAI).
  const connectOllamaCloud = async (model: string) => {
    const existing = cloudProvider;
    const token = cloudKey.trim();
    if (existing) {
      await updateProvider({
        id: existing.id,
        name: "Ollama Cloud (gratuït)",
        baseUrl: OLLAMA_CLOUD_URL,
        model,
        kind: "openai",
        auth: "bearer",
        // Si no s'ha escrit clau nova, conserva la desada.
        token: token || undefined,
        enabled: true,
      });
    } else {
      await addProvider({
        id: "ollama-cloud",
        name: "Ollama Cloud (gratuït)",
        baseUrl: OLLAMA_CLOUD_URL,
        model,
        kind: "openai",
        auth: "bearer",
        token,
        enabled: true,
      });
    }
    await selectProvider("ollama-cloud");
  };

  return (
    <div className="modal-overlay" onClick={() => setShow(false)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>{t("ai.manager")}</span>
          <button className="icon-btn" onClick={() => setShow(false)}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body">
          {/* Estat d'Ollama: si no hi és, oferim instal·lar-lo; si hi és però
              no roda, oferim arrencar-lo. El núvol es pot navegar igualment. */}
          {!ollamaRunning &&
            (ollamaInstalled ? (
              <div className="mm-warn mm-ollama-actions">
                <span>{t("ai.ollamaDown")}</span>
                <button className="btn sm primary" onClick={() => void startOllama()}>
                  <Play size={12} /> {t("ai.startOllama")}
                </button>
              </div>
            ) : (
              <div className="mm-warn mm-ollama-actions">
                <span>{t("ai.ollamaMissing")}</span>
                <button className="btn sm primary" onClick={() => void installOllama()}>
                  <Download size={12} /> {t("ai.installOllama")}
                </button>
              </div>
            ))}

          {/* Estat de ComfyUI (generació d'imatge): mateixa lògica que amb
              Ollama — si no hi és, oferim instal·lar-lo; si hi és però no
              roda, oferim arrencar-lo. */}
          {!comfyRunning && (
            <div className="mm-warn mm-ollama-actions">
              <span>
                {comfyInstalled ? t("ai.comfyDown") : t("ai.comfyMissing")}
              </span>
              {comfyInstalled ? (
                <button className="btn sm primary" onClick={() => void startComfy()}>
                  <Play size={12} /> {t("ai.startComfy")}
                </button>
              ) : (
                <button className="btn sm primary" onClick={() => void installComfy()}>
                  <Download size={12} /> {t("ai.installComfy")}
                </button>
              )}
            </div>
          )}

          {/* Rodet d'imatges en CPU (stable-diffusion.cpp): genera amb el
              processador. L'única via LOCAL en Mac Intel (ComfyUI oficial
              només Apple Silicon), però útil a qualsevol ordinador. */}
          <div className="field">
            <label>
              <Cpu size={13} style={{ verticalAlign: -2, marginRight: 5 }} />
              {t("ai.cpuTitle")}
            </label>
            <p className="lp-hint">{t("ai.cpuHint")}</p>
            <div className="mm-custom">
              <input
                readOnly
                value={imageCpu?.installed ? t("ai.cpuInstalled") : t("ai.cpuMissing")}
              />
              <button className="btn sm" onClick={() => void installImageCpu()}>
                <Download size={12} /> {t("ai.cpuInstall")}
              </button>
            </div>
            <div className="mm-custom" style={{ marginTop: 6 }}>
              <input readOnly value={imageCpu?.model ?? t("ai.cpuNoModel")} />
              <button
                className="btn primary sm"
                disabled={!!imageCpuProgress}
                onClick={() => void downloadCpuModel()}
              >
                <Download size={12} /> {imageCpuProgress || t("ai.cpuGetModel")}
              </button>
            </div>
          </div>

          {/* PREGUNTA DE DISC: si el model pesa ≥ 8 GB i no hi ha cap carpeta
              de models triada, demanem ON desar-lo abans de baixar-lo — un
              model gran no sempre cap al disc intern. */}
          {askDisk && (
            <div
              className="mm-warn"
              style={{ flexDirection: "column", alignItems: "stretch", gap: 8 }}
            >
              <span>
                {t("ai.askDisk").replace(
                  "{size}",
                  askDisk.size ? fmtSize(askDisk.size) : "?"
                )}{" "}
                <b>{askDisk.name}</b>
              </span>
              {volumes.length > 0 && (
                <div className="mm-volumes">
                  {volumes.map((v) => (
                    <button
                      key={v}
                      className="btn sm primary"
                      onClick={() => pullTo(v + "/ollama-models")}
                    >
                      <HardDrive size={11} />{" "}
                      {v.replace("/Volumes/", "").replace("/media/", "")}
                    </button>
                  ))}
                </div>
              )}
              <div className="mm-ollama-actions">
                <button
                  className="btn sm"
                  onClick={async () => {
                    const ask = askDisk;
                    const sel = await open({ directory: true });
                    if (typeof sel === "string" && ask) {
                      setAskDisk(null);
                      await setModelsDir(sel);
                      void pullModel(ask.name);
                    }
                  }}
                >
                  <Search size={12} /> {t("ai.askDiskFolder")}
                </button>
                <button className="btn sm" onClick={() => pullTo(null)}>
                  {t("ai.askDiskInternal")}
                </button>
              </div>
            </div>
          )}

          {/* Editors de vídeo IA descarregables: el material s'obté amb
              «IMG|»/rodets i es munta amb un editor REAL, en 1 clic. */}
          <div className="field">
            <label>
              <Film size={13} style={{ verticalAlign: -2, marginRight: 5 }} />
              {t("ai.videoEditorsTitle")}
            </label>
            <p className="lp-hint">{t("ai.videoEditorsHint")}</p>
            <div className="mm-volumes">
              {[
                { id: "kdenlive", k: "ai.videoKdenlive" },
                { id: "openshot", k: "ai.videoOpenshot" },
                { id: "davinci", k: "ai.videoDavinci" },
                { id: "shotcut", k: "ai.videoShotcut" },
              ].map(({ id, k }) => (
                <button key={id} className="btn sm" onClick={() => void installEditor(id)}>
                  <Download size={12} /> {t(k)}
                </button>
              ))}
            </div>
            {videoMsg && (
              <p className="lp-hint" style={{ marginTop: 6 }}>
                {videoMsg}
              </p>
            )}
          </div>

          {/* ALTRES IA locals, apart d'Ollama: en 1 clic (Homebrew en macOS,
              pàgina de descàrrega a la resta). Un cop instal·lades, NoOrbit
              les detecta soles al descobridor «IA locals externes». */}
          <div className="field">
            <label>
              <Cpu size={13} style={{ verticalAlign: -2, marginRight: 5 }} />
              {t("ai.localAppsTitle")}
            </label>
            <p className="lp-hint">{t("ai.localAppsHint")}</p>
            <div className="mm-volumes">
              {[
                { id: "lmstudio", k: "ai.localLmstudio" },
                { id: "jan", k: "ai.localJan" },
                { id: "gpt4all", k: "ai.localGpt4all" },
                { id: "llamacpp", k: "ai.localLlamacpp" },
              ].map(({ id, k }) => (
                <button key={id} className="btn sm" onClick={() => void installLocalAi(id)}>
                  <Download size={12} /> {t(k)}
                </button>
              ))}
            </div>
          </div>

          {/* Carpeta dels models: per posar-los en un USB extern quan el disc
              va curt d'espai. Cal reiniciar Ollama perquè ho tinga en compte. */}
          <div className="field">
            <label>
              <HardDrive size={13} style={{ verticalAlign: -2, marginRight: 5 }} />
              {t("ai.modelsDirTitle")}
            </label>
            <p className="lp-hint">{t("ai.modelsDirHint")}</p>
            <div className="mm-custom">
              <input
                readOnly
                value={modelsDir || t("ai.modelsDirDefault")}
                title={modelsDir || undefined}
              />
              <button className="btn sm" onClick={() => void pickModelsDir()}>
                <Search size={12} /> {t("ai.chooseFolder")}
              </button>
            </div>
            {volumes.length > 0 && (
              <div className="mm-volumes">
                {volumes.map((v) => (
                  <button
                    key={v}
                    className={"btn sm" + (modelsDir.startsWith(v) ? " primary" : "")}
                    onClick={() => void setModelsDir(v + "/ollama-models")}
                  >
                    <HardDrive size={11} /> {v.replace("/Volumes/", "").replace("/media/", "")}
                  </button>
                ))}
              </div>
            )}
            {modelsDir && (
              <div className="mm-ollama-actions" style={{ marginTop: 8 }}>
                <span className="lp-hint">{migrating || t("ai.modelsDirApplied")}</span>
                <button
                  className="btn sm"
                  title={t("ai.migrateModelsHint")}
                  disabled={!!migrating}
                  onClick={() => void migrateModels()}
                >
                  <HardDrive size={12} /> {t("ai.migrateModels")}
                </button>
                <button className="btn sm primary" onClick={() => void startOllama()}>
                  <Play size={12} /> {t("ai.startOllama")}
                </button>
              </div>
            )}
          </div>

          {/* Ollama Cloud: models alliberats que s'executen al núvol i són
              gratuïts (amb un token d'ollama.com). No cal baixar-los. */}
          <div className="field">
            <label>
              <Zap size={13} style={{ verticalAlign: -2, marginRight: 5 }} />
              {t("ai.cloudFreeTitle")}
            </label>
            <p className="lp-hint">{t("ai.cloudFreeHint")}</p>
            <div className="mm-custom">
              <input
                type="password"
                placeholder={t("ai.cloudKeyPh")}
                value={cloudKey}
                onChange={(e) => setCloudKey(e.target.value)}
              />
            </div>
            {/* Qualsevol model allotjat a ollama.com, no només els de la llista:
                s'escriu el nom i «Usa» el connecta sense descarregar-lo. */}
            <div className="mm-custom" style={{ marginTop: 6 }}>
              <input
                placeholder={t("ai.cloudFreeCustomPh")}
                value={cloudCustom}
                onChange={(e) => setCloudCustom(e.target.value)}
                onKeyDown={(e) =>
                  e.key === "Enter" && cloudCustom.trim() && void connectOllamaCloud(cloudCustom.trim())
                }
              />
              <button
                className="btn sm primary"
                disabled={!cloudCustom.trim()}
                onClick={() => void connectOllamaCloud(cloudCustom.trim())}
              >
                <Zap size={12} /> {t("ai.cloudFreeUse")}
              </button>
            </div>
            <div className="mm-cloud-list">
              {OLLAMA_CLOUD_MODELS.map((m) => {
                // Només actiu si el núvol està seleccionat I aquest n'és el
                // model. Clicar-lo de nou el desactiva (torna a Ollama local).
                const isActive =
                  cloudIsActive && cloudProvider?.model === m.name;
                return (
                  <div key={m.name} className="mm-row">
                    <div className="mm-cloud-info">
                      <div className="mm-name-static">{m.name}</div>
                      {m.description && <div className="mm-desc">{m.description}</div>}
                    </div>
                    <button
                      className={"btn sm" + (isActive ? " primary" : "")}
                      onClick={() =>
                        isActive
                          ? void selectProvider("ollama")
                          : void connectOllamaCloud(m.name)
                      }
                    >
                      {isActive ? <Check size={12} /> : <Zap size={12} />}{" "}
                      {isActive ? t("ai.cloudFreeStop") : t("ai.cloudFreeUse")}
                    </button>
                  </div>
                );
              })}
            </div>
            {cloudIsActive && (
              <p className="lp-hint" style={{ marginTop: 6 }}>
                {t("ai.cloudFreeActive")}
              </p>
            )}
          </div>

          <div className="field">
            <label>{t("ai.downloadByName")}</label>
            <p className="lp-hint">{t("ai.downloadByNameHint")}</p>
            <div className="mm-custom">
              <input
                placeholder={t("ai.modelName") + " (p. ex. llama3.1:8b)"}
                value={custom}
                onChange={(e) => setCustom(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && doCustomPull()}
              />
              <button className="btn primary sm" onClick={doCustomPull} disabled={!custom.trim()}>
                <Download size={13} /> {t("common.start")}
              </button>
            </div>
          </div>

          <div className="field">
            <label>
              {t("ai.models")}
              <button
                className="icon-btn"
                style={{ marginLeft: 8 }}
                title={t("common.refresh")}
                onClick={() => loadModels()}
              >
                <RefreshCw size={12} />
              </button>
            </label>
            {lastMessage && (
              <p className="lp-hint" style={{ marginBottom: 6 }}>
                {lastMessage}
              </p>
            )}
            {models.length === 0 ? (
              <div className="empty-hint">{t("ai.noModels")}</div>
            ) : (
              <div className="mm-list">
                {models.map((m) => {
                  const prog = pulling[m.name];
                  const active = selectedModel === m.name;
                  return (
                    <div key={m.name} className="mm-row">
                      <button
                        className="mm-name"
                        onClick={() => selectModel(m.name)}
                        title={t("ai.chooseModel")}
                      >
                        {active && <Check size={13} />} {m.name}
                      </button>
                      <span className="mm-size">{fmtSize(m.size)}</span>
                      {prog ? (
                        <span className="mm-prog">{prog}</span>
                      ) : (
                        <button
                          className="icon-btn"
                          title={t("ai.delete")}
                          onClick={() => deleteModel(m.name)}
                        >
                          <Trash2 size={13} />
                        </button>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </div>

          {/* Memòria RAM: quins models té Ollama carregats ARA mateix i quanta
              memòria ocupen. Es poden alliberar d'un clic, i es pot triar quant
              de temps romanen carregats després de respondre (menys = menys
              RAM en repòs, útil en equips de 8 GB). */}
          <div className="field">
            <label>
              <MemoryStick size={13} style={{ verticalAlign: -2, marginRight: 5 }} />
              {t("ai.ramTitle")}
            </label>
            <p className="lp-hint">{t("ai.ramHint")}</p>
            {loaded.length === 0 ? (
              <div className="empty-hint">{t("ai.ramNone")}</div>
            ) : (
              <div className="mm-list">
                {loaded.map((m) => (
                  <div key={m.name} className="mm-row">
                    <span className="mm-name-static">{m.name}</span>
                    <span className="mm-size">{fmtSize(m.size)}</span>
                    <button
                      className="btn sm"
                      title={t("ai.ramUnloadOne")}
                      onClick={() => void unloadModels(m.name)}
                    >
                      {t("ai.ramUnloadOne")}
                    </button>
                  </div>
                ))}
              </div>
            )}
            {loaded.length > 0 && (
              <div className="mm-ollama-actions" style={{ marginTop: 8 }}>
                <span className="lp-hint">
                  {fmtSize(loaded.reduce((a, m) => a + m.size, 0))} {t("ai.ramInUse")}
                </span>
                <button
                  className="btn sm primary"
                  onClick={async () => {
                    const n = await unloadModels(null);
                    setRamMsg(
                      n > 0
                        ? t("ai.ramUnloaded").replace("{n}", String(n))
                        : t("ai.ramNone")
                    );
                  }}
                >
                  {t("ai.ramUnloadAll")}
                </button>
              </div>
            )}
            {ramMsg && (
              <p className="lp-hint" style={{ marginTop: 6 }}>
                {ramMsg}
              </p>
            )}
            <div style={{ marginTop: 10 }}>
              <label>{t("ai.keepAliveTitle")}</label>
              <p className="lp-hint">{t("ai.keepAliveHint")}</p>
              <select
                className="ex-input"
                value={keepAlive}
                onChange={(e) => void setKeepAlive(e.target.value)}
              >
                <option value="">{t("ai.keepAliveDefault")}</option>
                <option value="5m">5 min</option>
                <option value="2m">2 min</option>
                <option value="1m">1 min</option>
                <option value="0">{t("ai.keepAliveZero")}</option>
              </select>
            </div>
          </div>

          {/* Catàleg recomanat: opcions instal·lables (NO es baixen sols).
              Cada fila té un botó «Baixa»; si ja és al disc, ho indica. */}
          <div className="field">
            <label>{t("ai.featuredTitle")}</label>
            <p className="lp-hint">{t("ai.featuredHint")}</p>
            {suggested.length === 0 ? (
              <div className="empty-hint">{t("common.loading")}</div>
            ) : (
              <div className="mm-list">
                {suggested.map((s) => {
                  const installed = models.some(
                    (m) => m.name === s.name || m.name.startsWith(s.name + ":")
                  );
                  // Hugging Bay: la clau de progrés és «huggingbay:<id>» i el
                  // model NO es baixa amb Ollama (el mirror dóna el GGUF i
                  // NoOrbit el registra). La resta de fonts: com abans.
                  const bayId =
                    internetSource === "hf-bay" ? (s.url ?? "").split("/").pop() ?? "" : "";
                  const prog = pulling[bayId ? `huggingbay:${bayId}` : s.name];
                  return (
                    <div key={s.name} className="mm-row">
                      <div className="mm-cloud-info">
                        <div className="mm-name-static">
                          {s.name}
                          {/* Quant pesa ABANS de baixar-lo (s'indica amb ≈
                              perquè el catàleg és una aproximació). */}
                          {s.size ? (
                            <span className="mm-size" style={{ marginLeft: 8 }}>
                              ≈ {fmtSize(s.size)}
                            </span>
                          ) : null}
                        </div>
                        {s.description && <div className="mm-desc">{s.description}</div>}
                      </div>
                      {installed ? (
                        <span className="mm-installed">{t("ai.cloudInstalled")}</span>
                      ) : prog ? (
                        <span className="mm-prog">{prog}</span>
                      ) : (
                        <button
                          className="btn sm"
                          onClick={() =>
                            bayId ? void pullBayModel(bayId) : void pullCloud(s.name, s.size)
                          }
                        >
                          <Download size={12} /> {t("ai.pull")}
                        </button>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </div>

          {/* MOTOR LOCAL SENSE OLLAMA (llama.cpp): aquí es poden fer córrer
              els models GGUF baixats (Hugging Bay / Hugging Face) sense Ollama:
              s'engega «llama-server» i el servei es registra com a proveïdor
              local, per xatejar-hi com si fos una IA qualsevol. */}
          <div className="field">
            <label>
              <Cpu size={13} style={{ verticalAlign: -2, marginRight: 5 }} />
              {t("ai.llamacppTitle")}
            </label>
            <p className="lp-hint">{t("ai.llamacppHint")}</p>

            {!llama?.installed ? (
              <div className="mm-custom" style={{ display: "flex", gap: 8, alignItems: "center" }}>
                <button
                  className="btn sm primary"
                  onClick={async () => {
                    // Binaris oficials de GitHub: funciona a tothom (Homebrew
                    // no en té binaris en els macOS vells).
                    setLlamaMsg(t("ai.llamacppInstalling"));
                    const r = await invoke<string>("llamacpp_install").catch((e) => String(e));
                    setLlamaMsg(r);
                    void loadLlama();
                  }}
                >
                  <Download size={12} /> {t("ai.llamacppInstall")}
                </button>
                {llamaMsg && <span className="mm-desc">{llamaMsg}</span>}
              </div>
            ) : (
              <>
                {llama.running.length > 0 && (
                  <div className="mm-custom" style={{ marginBottom: 6 }}>
                    {llama.running.map((r) => (
                      <div key={r.port} className="mm-row">
                        <div className="mm-cloud-info">
                          <div className="mm-name-static">
                            <span className="mm-installed">{t("ai.llamacppRunning")}</span> {r.model}
                          </div>
                          <div className="mm-desc">
                            127.0.0.1:{r.port} · {t("ai.llamacppProvider")}
                          </div>
                        </div>
                        <button
                          className="btn sm"
                          onClick={async () => {
                            await invoke("llamacpp_stop", { port: r.port }).catch(() => undefined);
                            void loadLlama();
                            void loadProviders();
                          }}
                        >
                          {t("ai.llamacppStop")}
                        </button>
                      </div>
                    ))}
                  </div>
                )}

                {llama.models.length === 0 ? (
                  <p className="lp-hint">{t("ai.llamacppNoModels")}</p>
                ) : (
                  <div className="mm-custom">
                    {llama.models.map(([path, size]) => (
                      <div key={path} className="mm-row">
                        <div className="mm-cloud-info">
                          <div className="mm-name-static">
                            {path.split("/").pop()}
                            <span className="mm-size" style={{ marginLeft: 8 }}>
                              {fmtSize(size)}
                            </span>
                          </div>
                        </div>
                        <button
                          className="btn sm"
                          onClick={async () => {
                            setLlamaMsg(t("ai.llamacppStarting"));
                            const r = await invoke<string>("llamacpp_start", { modelPath: path }).catch(
                              (e) => String(e),
                            );
                            setLlamaMsg(r);
                            void loadLlama();
                            void loadProviders();
                          }}
                        >
                          <Play size={12} /> {t("ai.llamacppRun")}
                        </button>
                      </div>
                    ))}
                  </div>
                )}
                {llamaMsg && <p className="lp-hint">{llamaMsg}</p>}
              </>
            )}

            {/* DISC EXTERN: on es desen els models GGUF i on viu el motor
                local. Si els posem en un USB/HDD, el model hi queda i
                NoOrbit hi treballa des d'allà. */}
            <div className="mm-llama-dirs">
              <div className="mm-custom">
                <span className="lp-hint" style={{ minWidth: 190 }}>
                  Models GGUF:
                </span>
                <input
                  readOnly
                  value={llama?.modelsDir || "Dades de NoOrbit"}
                  title={llama?.modelsDir || undefined}
                />
                <button className="btn sm" onClick={() => void pickLlamaModelsDir()}>
                  <Search size={12} /> {t("ai.chooseFolder")}
                </button>
                {volumes.map((v) => (
                  <button
                    key={`m${v}`}
                    className={
                      "btn sm" + ((llama?.modelsDir ?? "").startsWith(v) ? " primary" : "")
                    }
                    onClick={() => void setLlamaDirs(v + "/noorbit-models", undefined)}
                  >
                    <HardDrive size={11} />{" "}
                    {v.replace("/Volumes/", "").replace("/media/", "")}
                  </button>
                ))}
                {llama?.modelsDir && (
                  <button
                    className="btn sm ghost"
                    onClick={() => void setLlamaDirs("", undefined)}
                  >
                    {t("ai.modelsDirDefault")}
                  </button>
                )}
              </div>
              <div className="mm-custom">
                <span className="lp-hint" style={{ minWidth: 190 }}>
                  Motor local:
                </span>
                <input
                  readOnly
                  value={llama?.engineDir || "Dades de NoOrbit"}
                  title={llama?.engineDir || undefined}
                />
                <button className="btn sm" onClick={() => void pickLlamaEngineDir()}>
                  <Search size={12} /> {t("ai.chooseFolder")}
                </button>
                {volumes.map((v) => (
                  <button
                    key={`e${v}`}
                    className={
                      "btn sm" + ((llama?.engineDir ?? "").startsWith(v) ? " primary" : "")
                    }
                    onClick={() => void setLlamaDirs(undefined, v + "/noorbit-engine")}
                  >
                    <HardDrive size={11} />{" "}
                    {v.replace("/Volumes/", "").replace("/media/", "")}
                  </button>
                ))}
              </div>
              <p className="lp-hint">
                Amb el model en un disc extern, arrenca'l des d'aquí i NoOrbit
                hi treballa igual: si el disc no hi és muntat, el model
                simplement no apareix (no es per cap dada).
              </p>
            </div>
          </div>

          {/* Models del NÚVOL d'Ollama: cerca a ollama.com i baixa'n qualsevol.
              Funciona encara que Ollama no estigui en marxa (no genera tokens). */}
          <div className="field">
            <label>
              <Cloud size={13} style={{ verticalAlign: -2, marginRight: 5 }} />
              {t("ai.cloudTitle")}
            </label>
            <p className="lp-hint">{t("ai.cloudHint")}</p>
            <div className="mm-custom">
              <input
                placeholder={t("ai.cloudSearchPh")}
                value={cloudQuery}
                onChange={(e) => setCloudQuery(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && searchCloud(cloudQuery)}
              />
              <button
                className="btn primary sm"
                onClick={() => searchCloud(cloudQuery)}
                disabled={cloudLoading}
              >
                <Search size={13} /> {t("ai.cloudSearch")}
              </button>
            </div>
            {cloudError && (
              <p className="lp-hint" style={{ marginTop: 6 }}>
                {t("ai.cloudOffline")}
              </p>
            )}
            {cloud.length === 0 ? (
              <div className="empty-hint">
                {cloudLoading ? t("common.loading") : t("ai.cloudEmpty")}
              </div>
            ) : (
              <div className="mm-list">
                {cloud.map((c) => {
                  const installed = models.some(
                    (m) => m.name === c.name || m.name.startsWith(c.name + ":")
                  );
                  const prog = pulling[c.name];
                  return (
                    <div key={c.name} className="mm-row">
                      <div className="mm-cloud-info">
                        <div className="mm-name-static">
                          {c.name}
                          {/* Quant pesa ABANS de baixar-lo: suma dels .gguf
                              (HF) o la fitxa d'ollama.com. Si no se sap,
                              no es mostra res. */}
                          {c.size ? (
                            <span className="mm-size" style={{ marginLeft: 8 }}>
                              {fmtSize(c.size)}
                            </span>
                          ) : null}
                        </div>
                        {c.description && <div className="mm-desc">{c.description}</div>}
                      </div>
                      {installed ? (
                        <span className="mm-installed">{t("ai.cloudInstalled")}</span>
                      ) : prog ? (
                        <span className="mm-prog">{prog}</span>
                      ) : (
                        <button
                          className="btn sm"
                          onClick={() => void pullCloud(c.name, c.size)}
                        >
                          <Download size={12} /> {t("ai.pull")}
                        </button>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </div>

          {/* Models a INTERNET (Hugging Face): per si el que busques NO és a
              la biblioteca d'Ollama. Ollama baixa en natiu els noms «hf.co/…»
              (GGUF), així que el botó «Baixa» usa la mateixa descàrrega. */}
          <div className="field">
            <label>
              <Globe size={13} style={{ verticalAlign: -2, marginRight: 5 }} />
              {t("ai.internetTitle")}
            </label>
            <p className="lp-hint">{t("ai.internetHint")}</p>
            {/* Font de la cerca: Hugging Face per a LLM (GGUF que Ollama
                baixa natiu) o CivitAI per a models d'imatge (URL directa cap
                al rodet de CPU «sd-models/», útil fins i tot en Mac Intel). */}
            <select
              className="ex-input"
              value={internetSource}
              onChange={(e) => void setInternetSource(e.target.value as "hf" | "hf-bay" | "civitai")}
              style={{ marginBottom: 6 }}
            >
              <option value="hf">{t("ai.sourceHf")}</option>
              <option value="hf-bay">{t("ai.sourceHfBay")}</option>
              <option value="civitai">{t("ai.sourceCivitai")}</option>
            </select>
            <div className="mm-custom">
              <input
                placeholder={internetSource === "hf" ? t("ai.internetSearchPh") : t("ai.civitaiSearchPh")}
                value={internetQuery}
                onChange={(e) => setInternetQuery(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && searchInternet(internetQuery)}
              />
              <button
                className="btn primary sm"
                onClick={() => searchInternet(internetQuery)}
                disabled={internetLoading || (internetSource === "hf" ? false : !internetQuery.trim())}
              >
                <Search size={13} /> {t("ai.internetSearch")}
              </button>
            </div>
            {internetError && (
              <p className="lp-hint" style={{ marginTop: 6 }}>
                {t("ai.internetOffline")}
              </p>
            )}
            {internet.length === 0 ? (
              <div className="empty-hint">
                {internetLoading
                  ? t("common.loading")
                  : internetQuery.trim()
                    ? t("ai.internetEmpty")
                    : t("ai.internetIdle")}
              </div>
            ) : (
              <div className="mm-list">
                {internet.map((c) => {
                  const prog = pulling[c.name] || (imageCpuProgress && c.url ? imageCpuProgress : null);
                  return (
                    <div key={c.name} className="mm-row">
                      <div className="mm-cloud-info">
                        <div className="mm-name-static">
                          {c.name}
                          {/* Quant pesa ABANS de baixar-lo: suma dels .gguf
                              (HF) o el fitxer del model (CivitAI). Si no se
                              sap, no es mostra res. */}
                          {c.size ? (
                            <span className="mm-size" style={{ marginLeft: 8 }}>
                              {fmtSize(c.size)}
                            </span>
                          ) : null}
                        </div>
                        {c.description && <div className="mm-desc">{c.description}</div>}
                      </div>
                      {prog ? (
                        <span className="mm-prog">{prog}</span>
                      ) : c.url ? (
                        <button
                          className="btn sm"
                          title={t("ai.civitaiDownloadHint")}
                          onClick={() =>
                            void downloadCpuModel(
                              c.url ?? undefined,
                              c.name.split(/[\\/]/).pop() ?? undefined
                            )
                          }
                        >
                          <Download size={12} /> {t("ai.civitaiDownload")}
                        </button>
                      ) : (
                        <button
                          className="btn sm"
                          onClick={() => void pullCloud(c.name, c.size)}
                        >
                          <Download size={12} /> {t("ai.pull")}
                        </button>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
