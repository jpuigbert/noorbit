import { useEffect, useMemo, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { Plus, Trash2, Zap, Star, Pencil, Loader2, Power, Cpu } from "lucide-react";
import { useT } from "../../i18n";
import HarnessSection from "./HarnessSection";
import RemoteAiServerPanel from "./RemoteAiServerPanel";
import { allCloudModels } from "../../stores/aiStore";
import {
  useProviderStore,
  type ProviderInput,
  type AuthScheme,
  type ProviderKind,
} from "../../stores/providerStore";

const emptyForm = (): ProviderInput => ({
  name: "",
  baseUrl: "",
  model: "",
  kind: "openai",
  auth: "bearer",
  headerName: "",
  token: "",
  enabled: true,
});

/// Un servidor LOCAL (LM Studio, vLLM, DeepInfer a 127.0.0.1…) no demana cap
/// clau: allà «sense token» no és cap avís, és el normal.
const isLocal = (url: string): boolean =>
  /localhost|127\.0\.0\.1|0\.0\.0\.0|::1/.test(url.toLowerCase());

/// Preconfiguracions d'un sol clic: omplin el formulari amb la URL i el
/// model adequats perquè l'usuari només hi enganxe el token.
/// `models` és la llista de noms que el servei reconeix AVUI (nov 2026):
/// escriure'n un de vell («claude-3-5-sonnet», «deepseek-chat») és el que feia
/// que Claude i DeepSeek «no funcionessin» encara que la clau fos bona.
export type Preset = {
  key: string;
  label: string;
  data: Partial<ProviderInput>;
  /// Buit = NoOrbit no en coneix cap llista fiable; aleshores el camp de
  /// model queda com a text lliure (proposar un nom vell fa més mal que
  /// no proposar-ne cap).
  models?: string[];
};

const CLAUDE_MODELS = [
  "claude-sonnet-5-5",
  "claude-opus-5-5",
  "claude-haiku-5-5",
  "claude-sonnet-5",
  "claude-opus-4-8",
  "claude-opus-4-6",
  "claude-sonnet-4-5",
  "claude-haiku-4-5",
];

const DEEPSEEK_MODELS = ["deepseek-flash", "deepseek-v4-pro"];

export const PRESETS: Preset[] = [
  {
    key: "openai",
    label: "OpenAI",
    data: { name: "OpenAI", baseUrl: "https://api.openai.com/v1", model: "gpt-4o-mini", kind: "openai", auth: "bearer" },
  },
  {
    // Claude (Anthropic): l'API oficial NO és compatible amb OpenAI. Tres
    // detalls la fan anar o no: la base amb «/v1», el tipus «anthropic» i la
    // clau a la capçalera «x-api-key» (Anthropic rebutja «Bearer»). Abans
    // aquest preset deia «bearer» i un model antic: d'aquí el «no funciona».
    key: "claude",
    label: "Claude (Anthropic)",
    data: {
      name: "Claude (Anthropic)",
      baseUrl: "https://api.anthropic.com/v1",
      model: "claude-sonnet-5-5",
      kind: "anthropic",
      auth: "header",
      headerName: "x-api-key",
    },
    models: CLAUDE_MODELS,
  },
  {
    key: "ollamacloud",
    label: "Ollama Cloud (gratuït)",
    data: { name: "Ollama Cloud", baseUrl: "https://ollama.com/v1", model: "gpt-oss:20b", kind: "openai", auth: "bearer" },
  },
  {
    // MiniMax: API compatible amb OpenAI (https://api.minimax.io/v1) amb un
    // tier GRATUÏT potent (MiniMax-M3.1-Flash, context d'1M). Serveix com a
    // «teacher» en línia per millorar la IA nativa quan no hi ha Ollama local.
    // No queda configurat fins que l'usuari hi enganxa el seu token.
    key: "minimax",
    label: "MiniMax (gratuït)",
    data: { name: "MiniMax (gratuït)", baseUrl: "https://api.minimax.io/v1", model: "MiniMax-M3.1-Flash", kind: "openai", auth: "bearer" },
  },
  {
    // Venice AI: API compatible amb OpenAI (documentació oficial:
    // https://api.venice.ai/api/v1), inferència «sense filtres» i zero
    // retenció. La clau es crea al compte (Setup › API Keys) i comença per
    // «ven-». NO queda configurada fins que l'usuari hi enganxa el token.
    key: "venice",
    label: "Venice AI",
    data: { name: "Venice AI", baseUrl: "https://api.venice.ai/api/v1", model: "venice-uncensored", kind: "openai", auth: "bearer" },
  },
  {
    // DeepSeek: API compatible amb OpenAI. No queda configurada fins que
    // l'usuari hi enganxe el seu token (creat al seu compte de DeepSeek).
    // La base documentada ja no porta «/v1» i el model «deepseek-chat» és el
    // nom antic: avui és «deepseek-flash».
    key: "deepseek",
    label: "DeepSeek",
    data: { name: "DeepSeek", baseUrl: "https://api.deepseek.com", model: "deepseek-flash", kind: "openai", auth: "bearer" },
    models: DEEPSEEK_MODELS,
  },
  {
    // DeepSeek també exposa una entrada d'estil Anthropic (mateix token, la
    // clau del mateix compte hi serveix). Amb el format Claude funciona amb
    // tot el que fa servir Anthropic: clients, eines i l'harness «dsh».
    key: "deepseek-anthropic",
    label: "DeepSeek (format Claude)",
    data: {
      name: "DeepSeek (format Claude)",
      baseUrl: "https://api.deepseek.com/anthropic",
      model: "deepseek-flash",
      kind: "anthropic",
      auth: "header",
      headerName: "x-api-key",
    },
    models: DEEPSEEK_MODELS,
  },
  {
    // Perplexity: API oficial (compatible amb OpenAI) de cerca web amb
    // citacions. Només funciona si l'usuari registra el seu propi token.
    key: "perplexity",
    label: "Perplexity",
    data: { name: "Perplexity", baseUrl: "https://api.perplexity.ai", model: "sonar", kind: "openai", auth: "bearer" },
  },
  {
    // DeepInfer (github.com/deepseek-ai/DeepInfer): motor d'inferència de
    // DeepSeek per a servidors Linux amb GPU SM100 / Ascend 950. No demana
    // token: l'usuari només ha de posar la URL del seu servei (per defecte,
    // el port 8000 de vLLM a la mateixa màquina).
    key: "deepinfer",
    label: "DeepInfer (servidor GPU)",
    data: {
      name: "DeepInfer (servidor GPU)",
      baseUrl: "http://127.0.0.1:8000/v1",
      model: "deepseek-ai/DeepSeek-V4.1-Flash-1017",
      kind: "openai",
      auth: "bearer",
    },
  },
];

export function presetModelsFor(baseUrl: string, model = ""): string[] {
  const low = baseUrl.trim().toLowerCase();
  if (!low) return model ? [model] : [];
  const exact = PRESETS.find((x) => (x.data.baseUrl ?? "").toLowerCase() === low);
  let list: string[] = exact?.models ?? [];
  if (list.length === 0) {
    // El preset no hi és (l'usuari ha escrit la URL a mà), però el SERVEI el
    // coneixem: la llista va per domini, no pel botó premut.
    if (low.includes("anthropic.com")) list = CLAUDE_MODELS;
    else if (low.includes("deepseek.com")) list = DEEPSEEK_MODELS;
  }
  if (model && !list.includes(model)) return [model, ...list];
  return list;
}

export default function ProviderManager() {
  const { t } = useT();
  const {
    providers,
    active,
    testingId,
    lastTest,
    load,
    add,
    update,
    remove,
    test,
    select,
    checkEndpoint,
    checking,
    lastCheck,
    setEnabled,
  } = useProviderStore(
    useShallow((st) => ({
      providers: st.providers,
      active: st.active,
      testingId: st.testingId,
      lastTest: st.lastTest,
      load: st.load,
      add: st.add,
      update: st.update,
      remove: st.remove,
      test: st.test,
      select: st.select,
      checkEndpoint: st.checkEndpoint,
      checking: st.checking,
      lastCheck: st.lastCheck,
      setEnabled: st.setEnabled,
    }))
  );

  const [form, setForm] = useState<ProviderInput>(emptyForm());
  const [editingId, setEditingId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    load();
  }, [load]);

  const set = (patch: Partial<ProviderInput>) => setForm((f) => ({ ...f, ...patch }));

  // Suggiments del camp «Model»: els del servei escollit; si no en sabem res
  // però la URL és el núvol d'Ollama, la seva llista de models gratuïts.
  const modelOptions = useMemo(() => {
    const list = presetModelsFor(form.baseUrl, form.model);
    if (list.length > 0) return list;
    if (form.baseUrl.toLowerCase().includes("ollama.com")) {
      return allCloudModels().map((m) => m.name);
    }
    return [];
  }, [form.baseUrl, form.model]);

  const startEdit = (id: string) => {
    const p = providers.find((x) => x.id === id);
    if (!p) return;
    setEditingId(id);
    setForm({
      name: p.name,
      baseUrl: p.base_url,
      model: p.model,
      kind: p.kind as ProviderKind,
      auth: p.auth,
      headerName: p.header_name,
      token: "", // buit = conserva
      enabled: p.enabled,
    });
  };

  const save = async () => {
    if (!form.name || !form.baseUrl) return;
    setSaving(true);
    try {
      if (editingId) {
        await update({ ...form, id: editingId, token: form.token || undefined });
      } else {
        await add(form);
      }
      setForm(emptyForm());
      setEditingId(null);
    } finally {
      setSaving(false);
    }
  };

  const cancel = () => {
    setForm(emptyForm());
    setEditingId(null);
  };

  return (
    <div className="provider-mgr">
      <p className="lp-hint">{t("providers.intro")}</p>

      {/* Proveïdor actiu */}
      <div className="field">
        <label>{t("ai.switchProvider")}</label>
        <select
          className="lp-select"
          value={active}
          onChange={(e) => select(e.target.value)}
        >
          <option value="ollama">{t("providers.local")}</option>
          {providers.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name} · {p.model}
            </option>
          ))}
        </select>
      </div>

      {/* Llista */}
      <div className="prov-list">
        {providers.length === 0 && <div className="empty-hint">{t("providers.none")}</div>}
        {providers.map((p) => {
          const tst = lastTest[p.id];
          // IA NATIVA: integrada al binari, sense token ni descàrregues. No
          // s'edita ni s'esborra (el backend també ho protegeix); només es
          // pot activar/desactivar, provar i fer servir.
          const native = p.kind === "native";
          return (
            <div
              key={p.id}
              className={"prov-row" + (active === p.id ? " active" : "") + (p.enabled ? "" : " off")}
            >
              <div className="prov-main">
                <div className="prov-title">
                  {active === p.id && <Star size={12} />} {p.name}
                  <span className="prov-model">{p.model}</span>
                  {native && (
                    <span className="prov-native-badge" title={t("providers.nativeHint")}>
                      <Cpu size={11} /> {t("providers.native")}
                    </span>
                  )}
                  {/* ESTAT ACTIU/INACTIU amb toggle MANUAL: proveïdors com
                      DeepInfer (servidor GPU) venen desactivats de fàbrica;
                      quan l'usuari té el servidor en marxa, ho activa aquí
                      amb un clic — sense obrir el formulari ni token. */}
                  <button
                    className={"prov-toggle" + (p.enabled ? " on" : "")}
                    title={
                      p.enabled ? t("providers.deactivate") : t("providers.activate")
                    }
                    onClick={() => void setEnabled(p.id, !p.enabled)}
                  >
                    <Power size={11} /> {p.enabled ? t("providers.on") : t("providers.off")}
                  </button>
                </div>
                <div className="prov-url">{native ? t("providers.nativeUrl") : p.base_url}</div>
                <div className="prov-token">
                  {native
                    ? t("providers.nativeNoToken")
                    : p.has_token
                    ? `${p.token_masked}`
                    : // Un servidor LOCAL (LM Studio, vLLM/DeepInfer…) no demana
                      // cap clau: «sense token» no és cap problema allà.
                      isLocal(p.base_url)
                      ? "— local: no cal token"
                      : "⚠ sense token"}
                </div>
                {tst && (
                  <div className={"prov-test " + (tst.ok ? "ok" : "fail")}>
                    {tst.ok ? t("providers.testOk") : `${t("providers.testFail")}: ${tst.msg}`}
                  </div>
                )}
              </div>
              <div className="prov-actions">
                <button
                  className="btn sm"
                  onClick={() => test(p.id)}
                  disabled={testingId === p.id}
                  title={t("providers.test")}
                >
                  <Zap size={12} /> {testingId === p.id ? t("providers.testing") : t("providers.test")}
                </button>
                {active !== p.id && (
                  <button className="btn sm" onClick={() => select(p.id)} title={t("providers.use")}>
                    {t("providers.use")}
                  </button>
                )}
                {!native && (
                  <button className="icon-btn" onClick={() => startEdit(p.id)} title={t("providers.edit")}>
                    <Pencil size={13} />
                  </button>
                )}
                {!native && (
                  <button className="icon-btn" onClick={() => remove(p.id)} title={t("providers.remove")}>
                    <Trash2 size={13} />
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {/* Formulari */}
      <div className="prov-form">
        <h4>{editingId ? t("providers.edit") : t("providers.add")}</h4>
        {/* Preconfiguracions: un clic per omplir OpenAI, Claude o Ollama Cloud. */}
        <div className="prov-presets">
          {PRESETS.map((p) => (
            <button
              key={p.key}
              className="btn sm"
              onClick={() => setForm({ ...emptyForm(), ...p.data })}
            >
              {p.label}
            </button>
          ))}
        </div>
        <div className="field">
          <label>{t("providers.name")}</label>
          <input value={form.name} onChange={(e) => set({ name: e.target.value })} placeholder="OpenAI" />
        </div>
        <div className="field">
          <label>{t("providers.baseUrl")}</label>
          <input
            value={form.baseUrl}
            onChange={(e) => set({ baseUrl: e.target.value })}
            placeholder={t("providers.baseUrlHint")}
          />
        </div>
        <div className="field">
          <label>{t("providers.model")}</label>
          {/* El camp admet qualsevol nom escrit a mà; els suggeriments són els
              models que el servei d'aquesta URL reconeix avui (Claude,
              DeepSeek…) o, si és el núvol d'Ollama, els seus gratuïts. */}
          <input
            list="noorbit-model-options"
            value={form.model}
            onChange={(e) => set({ model: e.target.value })}
            placeholder="claude-sonnet-5-5"
          />
          {modelOptions.length > 0 && (
            <div className="lp-hint">{t("providers.modelOptions")}</div>
          )}
          <datalist id="noorbit-model-options">
            {modelOptions.map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
        </div>
        <div className="field">
          <label>{t("providers.kind")}</label>
          <select
            value={form.kind ?? "openai"}
            onChange={(e) => set({ kind: e.target.value as ProviderKind })}
          >
            <option value="openai">{t("providers.kindOpenai")}</option>
            <option value="anthropic">{t("providers.kindAnthropic")}</option>
          </select>
        </div>
        <div className="prov-form-2">
          <div className="field">
            <label>{t("providers.auth")}</label>
            <select
              value={form.auth}
              onChange={(e) => set({ auth: e.target.value as AuthScheme })}
            >
              <option value="bearer">{t("providers.authBearer")}</option>
              <option value="header">{t("providers.authHeader")}</option>
              <option value="none">{t("providers.authNone")}</option>
            </select>
          </div>
          {form.auth === "header" && (
            <div className="field">
              <label>{t("providers.headerName")}</label>
              <input
                value={form.headerName}
                onChange={(e) => set({ headerName: e.target.value })}
                placeholder="x-api-key"
              />
            </div>
          )}
        </div>
        <div className="field">
          <label>{t("providers.token")}</label>
          <input
            type="password"
            value={form.token}
            onChange={(e) => set({ token: e.target.value })}
            placeholder={editingId ? t("providers.tokenKeep") : "sk-…"}
          />
        </div>
        <div className="prov-form-actions">
          {/* Comprova ABANS de desar: si falla, mostra el MOTIU exacte
              (servidor aturat, clau dolenta, URL errònia…) i, si el servidor
              contesta amb una llista, els models que ofereix (clicables). */}
          <button
            className="btn sm"
            disabled={checking || !form.baseUrl}
            onClick={() => void checkEndpoint(form)}
          >
            {checking ? <Loader2 size={12} className="spin" /> : <Zap size={12} />}
            {checking ? t("providers.checking") : t("providers.check")}
          </button>
          <button className="btn primary sm" onClick={save} disabled={saving || !form.name || !form.baseUrl}>
            <Plus size={13} /> {t("providers.save")}
          </button>
          {editingId && (
            <button className="btn sm" onClick={cancel}>
              {t("common.cancel")}
            </button>
          )}
        </div>
        {/* Resultat del diagnòstic: sempre marca ✓/✗ i, si falla, DIU EL
            MOTIU (servidor aturat, clau dolenta, URL errònia, model que no
            hi és) amb les pistes per arreglar-ho. El models que ofereix el
            servei surten en xips: clicar-ne un el posa al formulari. */}
        {lastCheck && (
          <div
            className={"prov-test" + (lastCheck.ok ? " ok" : " fail")}
            style={{ marginTop: 8, whiteSpace: "pre-wrap" }}
          >
            {lastCheck.ok ? "✓ " : "✗ "}
            <b>{lastCheck.title}</b>
            {"\n"}
            {lastCheck.detail}
            {lastCheck.models.length > 0 && (
              <div className="prov-chips">
                {lastCheck.models.slice(0, 16).map((m) => (
                  <button
                    key={m}
                    className="btn sm"
                    onClick={() => set({ model: m })}
                    title={t("providers.useModel")}
                  >
                    {m}
                  </button>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Servidor d'IA remot: puja el servei per SFTP/FTP a una màquina amb
          GPU (DeepInfer/vLLM) i connecta NoOrbit a la seva API OpenAI. Les
          IAs treballen fora del Mac; respostes i canvis tornen per HTTP. */}
      <RemoteAiServerPanel />

      {/* Menú APART per a harnessos d'agents (DeepSeek Harness/dsh): no són
          proveïdors d'API i per això no figuren a la llista de dalt. Ací es
          poden instal·lar (Node.js inclòs, automàtic, sense administració),
          arrencar i obrir la seva interfície. */}
      <HarnessSection />
    </div>
  );
}
