import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { X, Download, Trash2, RefreshCw, Check, Search, Cloud, Play, HardDrive, Zap, Globe } from "lucide-react";
import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";
import { useAIStore, OLLAMA_CLOUD_MODELS, OLLAMA_CLOUD_URL } from "../../stores/aiStore";
import { useProviderStore } from "../../stores/providerStore";

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
    deleteModel,
    selectModel,
    modelsDir,
    volumes,
    loadModelsDir,
    setModelsDir,
  } = useAIStore();

  const [custom, setCustom] = useState("");
  const [cloudQuery, setCloudQuery] = useState("");
  // Cerca a INTERNET (Hugging Face) per a models que NO són a Ollama.
  const [internetQuery, setInternetQuery] = useState("");
  // Connexió a Ollama Cloud (models gratuïts al núvol, sense descàrrega).
  const [cloudKey, setCloudKey] = useState("");
  const providers = useProviderStore((s) => s.providers);
  const activeProvider = useProviderStore((s) => s.active);
  const loadProviders = useProviderStore((s) => s.load);
  const addProvider = useProviderStore((s) => s.add);
  const updateProvider = useProviderStore((s) => s.update);
  const selectProvider = useProviderStore((s) => s.select);
  // El model concret que ara mateix serveix el proveïdor «ollama-cloud»:
  // així només es marca aquell model, no tots a la vegada.
  const cloudProvider = providers.find((p) => p.id === "ollama-cloud");

  useEffect(() => {
    if (show) {
      checkOllama();
      checkInstalled();
      checkComfy();
      loadModels();
      loadModelsDir();
      loadProviders();
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
  const pullCloud = async (name: string) => {
    if (!ollamaRunning) {
      if (ollamaInstalled) await startOllama();
      else {
        await installOllama();
        return;
      }
    }
    await pullModel(name);
  };

  // Tria on viuran els models (p. ex. un USB extern) i l'aplica.
  const pickModelsDir = async () => {
    const selected = await open({ directory: true });
    if (typeof selected === "string") await setModelsDir(selected);
  };

  // Connecta/actualitza el proveïdor «ollama-cloud» amb el model triat i
  // l'activa. Reutilitza el sistema de proveïdors (compatible amb OpenAI).
  const connectOllamaCloud = async (model: string) => {
    const existing = providers.find((p) => p.id === "ollama-cloud");
    const token = cloudKey.trim();
    if (existing) {
      await updateProvider({
        id: "ollama-cloud",
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
                <span className="lp-hint">{t("ai.modelsDirApplied")}</span>
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
            <div className="mm-cloud-list">
              {OLLAMA_CLOUD_MODELS.map((m) => {
                // Només actiu si el núvol està seleccionat I aquest n'és el
                // model. Clicar-lo de nou el desactiva (torna a Ollama local).
                const isActive =
                  activeProvider === "ollama-cloud" && cloudProvider?.model === m.name;
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
            {activeProvider === "ollama-cloud" && (
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
                  const prog = pulling[s.name];
                  return (
                    <div key={s.name} className="mm-row">
                      <div className="mm-cloud-info">
                        <div className="mm-name-static">{s.name}</div>
                        {s.description && <div className="mm-desc">{s.description}</div>}
                      </div>
                      {installed ? (
                        <span className="mm-installed">{t("ai.cloudInstalled")}</span>
                      ) : prog ? (
                        <span className="mm-prog">{prog}</span>
                      ) : (
                        <button
                          className="btn sm"
                          onClick={() => void pullCloud(s.name)}
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
                        <div className="mm-name-static">{c.name}</div>
                        {c.description && <div className="mm-desc">{c.description}</div>}
                      </div>
                      {installed ? (
                        <span className="mm-installed">{t("ai.cloudInstalled")}</span>
                      ) : prog ? (
                        <span className="mm-prog">{prog}</span>
                      ) : (
                        <button
                          className="btn sm"
                          onClick={() => void pullCloud(c.name)}
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
            <div className="mm-custom">
              <input
                placeholder={t("ai.internetSearchPh")}
                value={internetQuery}
                onChange={(e) => setInternetQuery(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && searchInternet(internetQuery)}
              />
              <button
                className="btn primary sm"
                onClick={() => searchInternet(internetQuery)}
                disabled={internetLoading || !internetQuery.trim()}
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
                  const prog = pulling[c.name];
                  return (
                    <div key={c.name} className="mm-row">
                      <div className="mm-cloud-info">
                        <div className="mm-name-static">{c.name}</div>
                        {c.description && <div className="mm-desc">{c.description}</div>}
                      </div>
                      {prog ? (
                        <span className="mm-prog">{prog}</span>
                      ) : (
                        <button
                          className="btn sm"
                          onClick={() => void pullCloud(c.name)}
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
