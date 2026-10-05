import { useEffect, useState } from "react";
import { Plus, Trash2, Zap, Star, Pencil } from "lucide-react";
import { useT } from "../../i18n";
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

/// Preconfiguracions d'un sol clic: omplin el formulari amb la URL i el
/// model adequats perquè l'usuari només hi enganxe el token.
const PRESETS: { key: string; label: string; data: Partial<ProviderInput> }[] = [
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
  } = useProviderStore();

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
            <div key={p.id} className={"prov-row" + (active === p.id ? " active" : "")}>
              <div className="prov-main">
                <div className="prov-title">
                  {active === p.id && <Star size={12} />} {p.name}
                  <span className="prov-model">{p.model}</span>
                </div>
                <div className="prov-url">{p.base_url}</div>
                <div className="prov-token">
                  {p.has_token ? `${p.token_masked}` : "⚠ sense token"}
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
          <input
            value={form.model}
            onChange={(e) => set({ model: e.target.value })}
            placeholder="gpt-4o-mini"
          />
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
          <button className="btn primary sm" onClick={save} disabled={saving || !form.name || !form.baseUrl}>
            <Plus size={13} /> {t("providers.save")}
          </button>
          {editingId && (
            <button className="btn sm" onClick={cancel}>
              {t("common.cancel")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
