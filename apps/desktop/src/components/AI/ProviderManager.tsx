import { useEffect, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { Plus, Trash2, Zap, Star, Pencil, Loader2, Power } from "lucide-react";
import { useT } from "../../i18n";
import HarnessSection from "./HarnessSection";
import RemoteAiServerPanel from "./RemoteAiServerPanel";
import { OLLAMA_CLOUD_MODELS } from "../../stores/aiStore";
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
export const PRESETS: { key: string; label: string; data: Partial<ProviderInput> }[] = [
  {
    key: "openai",
    label: "OpenAI",
    data: { name: "OpenAI", baseUrl: "https://api.openai.com/v1", model: "gpt-4o-mini", kind: "openai", auth: "bearer" },
  },
  {
    key: "claude",
    label: "Claude (Anthropic)",
    data: { name: "Claude", baseUrl: "https://api.anthropic.com", model: "claude-sonnet-4-5", kind: "anthropic", auth: "bearer" },
  },
  {
    key: "ollamacloud",
    label: "Ollama Cloud (gratuït)",
    data: { name: "Ollama Cloud", baseUrl: "https://ollama.com/v1", model: "gpt-oss:20b", kind: "openai", auth: "bearer" },
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
    key: "deepseek",
    label: "DeepSeek",
    data: { name: "DeepSeek", baseUrl: "https://api.deepseek.com", model: "deepseek-chat", kind: "openai", auth: "bearer" },
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

  const startEdit = (id: string) => {
    const p = providers.find((x) => x.id === id);
    if (!p) return;
    setEditingId(id);
    setForm({
      name: p.name,
      baseUrl: p.base_url,
      model: p.model,
      kind: (p.kind === "anthropic" ? "anthropic" : "openai") as ProviderKind,
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
          return (
            <div
              key={p.id}
              className={"prov-row" + (active === p.id ? " active" : "") + (p.enabled ? "" : " off")}
            >
              <div className="prov-main">
                <div className="prov-title">
                  {active === p.id && <Star size={12} />} {p.name}
                  <span className="prov-model">{p.model}</span>
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
                <div className="prov-url">{p.base_url}</div>
                <div className="prov-token">
                  {p.has_token
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
                <button className="icon-btn" onClick={() => startEdit(p.id)} title={t("providers.edit")}>
                  <Pencil size={13} />
                </button>
                <button className="icon-btn" onClick={() => remove(p.id)} title={t("providers.remove")}>
                  <Trash2 size={13} />
                </button>
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
          {/* El camp admet qualsevol nom escrit a mà; el «datalist» ofereix els
              models gratuïts del núvol d'Ollama com a suggeriments. */}
          <input
            list="noorbit-cloud-models"
            value={form.model}
            onChange={(e) => set({ model: e.target.value })}
            placeholder="gpt-4o-mini"
          />
          <datalist id="noorbit-cloud-models">
            {OLLAMA_CLOUD_MODELS.map((m) => (
              <option key={m.name} value={m.name} />
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
