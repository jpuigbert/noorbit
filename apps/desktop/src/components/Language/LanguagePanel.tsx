import { useRef, useState } from "react";
import { Download, Upload, Trash2, Languages } from "lucide-react";
import { useI18n, BASE_LOCALE } from "../../i18n/store";
import type { Catalog } from "../../i18n/store";

/**
 * Panell de gestió d'idiomes.
 *
 * Permet a qualsevol (traductor/a) sense saber programar:
 *  1. Baixar el catàleg de referència en català (ca.json).
 *  2. Traduir-lo (mantenint les claus) i pujar-lo aquí.
 *  3. Canviar l'idioma actiu a l'instant.
 */
export default function LanguagePanel() {
  const { locale, t, languages, setLocale, addLocale, removeLocale, exportCatalog } =
    useI18n();
  const fileRef = useRef<HTMLInputElement>(null);
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [msg, setMsg] = useState<string | null>(null);

  const langs = languages();

  const downloadCatalog = () => {
    const json = exportCatalog(BASE_LOCALE);
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${BASE_LOCALE}.json`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  };

  const onPickFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      const catalog = JSON.parse(text) as Catalog;
      // dedueix el codi del nom del fitxer (p.ex. "en.json" -> "en")
      const derived = code || file.name.replace(/\.json$/i, "").toLowerCase();
      if (!derived) {
        setMsg(t("language.importError"));
        return;
      }
      addLocale(derived, catalog, name || undefined);
      setLocale(derived);
      setMsg(`${t("language.imported")}: ${derived}`);
      setCode("");
      setName("");
    } catch {
      setMsg(t("language.importError"));
    } finally {
      if (fileRef.current) fileRef.current.value = "";
    }
  };

  return (
    <div className="language-panel">
      <div className="lp-section">
        <h4>
          <Languages size={14} /> {t("language.current")}
        </h4>
        <select
          className="lp-select"
          value={locale}
          onChange={(e) => setLocale(e.target.value)}
        >
          {langs.map((l) => (
            <option key={l.code} value={l.code}>
              {l.name} ({l.code}){l.code === BASE_LOCALE ? " — base" : ""}
            </option>
          ))}
        </select>
        {locale !== BASE_LOCALE && (
          <button
            className="lp-btn danger"
            onClick={() => removeLocale(locale)}
            title={t("language.remove")}
          >
            <Trash2 size={13} /> {t("language.remove")}
          </button>
        )}
      </div>

      <div className="lp-section">
        <h4>{t("language.exportBase")}</h4>
        <p className="lp-hint">{t("language.exportHint")}</p>
        <button className="lp-btn" onClick={downloadCatalog}>
          <Download size={13} /> {BASE_LOCALE}.json
        </button>
      </div>

      <div className="lp-section">
        <h4>{t("language.importFile")}</h4>
        <p className="lp-hint">{t("language.importHint")}</p>
        <div className="lp-import">
          <input
            className="lp-input"
            placeholder={t("language.code")}
            value={code}
            onChange={(e) => setCode(e.target.value)}
          />
          <input
            className="lp-input"
            placeholder={t("language.name")}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </div>
        <input
          ref={fileRef}
          type="file"
          accept="application/json,.json"
          hidden
          onChange={onPickFile}
        />
        <button className="lp-btn" onClick={() => fileRef.current?.click()}>
          <Upload size={13} /> {t("language.addFrom")}
        </button>
        {msg && <div className="lp-msg">{msg}</div>}
      </div>
    </div>
  );
}
