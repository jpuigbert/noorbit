// Catàleg d'IAs del menú IA: informa ABANS de descarregar — mida aprox.,
// si és sense censura, si accepta imatges (visió), si en genera (imatges o
// vídeo) i l'estat d'instal·lació. Les dades venen de la comanda Rust
// «ia_catalog» (taula curated local + proveïdors en línia registrats).
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { X, Download, Check, RefreshCw } from "lucide-react";
import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";
import { useAIStore } from "../../stores/aiStore";

interface IaCatalogEntry {
  name: string;
  source: string;
  approx_size_gb: number;
  uncensored: boolean;
  vision: boolean;
  image_gen: boolean;
  video_gen: boolean;
  installed: boolean;
  note: string;
}

type Filter = "all" | "uncensored" | "vision" | "image_gen" | "video_gen" | "installed";

export default function IaCatalogModal() {
  const show = useUIStore((s) => s.showIaCatalog);
  const setShow = useUIStore((s) => s.setShowIaCatalog);
  const { t } = useT();
  const pullModel = useAIStore((s) => s.pullModel);
  const pulling = useAIStore((s) => s.pulling);

  const [entries, setEntries] = useState<IaCatalogEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>("all");

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      setEntries(await invoke<IaCatalogEntry[]>("ia_catalog"));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (show) void load();
  }, [show]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!show) return null;

  const visible = entries.filter((e) => {
    switch (filter) {
      case "uncensored":
        return e.uncensored;
      case "vision":
        return e.vision;
      case "image_gen":
        return e.image_gen;
      case "video_gen":
        return e.video_gen;
      case "installed":
        return e.installed;
      default:
        return true;
    }
  });

  const yes = <span className="ic-yes">✓</span>;
  const no = <span className="ic-no">—</span>;

  const filters: { id: Filter; label: string }[] = [
    { id: "all", label: "Totes" },
    { id: "uncensored", label: "Sense censura" },
    { id: "vision", label: "Accepten imatges" },
    { id: "image_gen", label: "Generen imatges" },
    { id: "video_gen", label: "Generen vídeo" },
    { id: "installed", label: "Ja instal·lades" },
  ];

  return (
    <div className="modal-overlay" onClick={() => setShow(false)}>
      <div className="modal ic-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>{t("ai.catalog")}</span>
          <button className="icon-btn" title="Actualitza" onClick={() => void load()}>
            <RefreshCw size={13} />
          </button>
          <button className="icon-btn" onClick={() => setShow(false)}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body">
          <p className="lp-hint">
            Mides aproximades de l'etiqueta estàndard (les variants «:3b» o q4
            pesen molt menys). «Núvol» = no cal baixar res. Cap model d'aquesta
            llista genera vídeo avui; per a vídeo a parts locals caldria un
            model dedicat (p. ex. via ComfyUI) encara no integrat.
          </p>
          <div className="ic-filters">
            {filters.map((f) => (
              <button
                key={f.id}
                className={"btn sm" + (filter === f.id ? " primary" : "")}
                onClick={() => setFilter(f.id)}
              >
                {f.label}
              </button>
            ))}
          </div>
          {error && <div className="mm-warn">{error}</div>}
          {loading && visible.length === 0 ? (
            <div className="empty-hint">{t("common.loading")}</div>
          ) : (
            <table className="ic-table">
              <thead>
                <tr>
                  <th>Model</th>
                  <th>Origen</th>
                  <th title="Mida aprox. abans de descarregar">Mida</th>
                  <th title="Sense filtres de contingut">Censura</th>
                  <th title="Rep imatges com a entrada (visió)">Imatges in.</th>
                  <th title="Genera imatges noves">Imatges out.</th>
                  <th title="Genera vídeo">Vídeo</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {visible.map((e) => {
                  const prog = pulling[e.name];
                  return (
                    <tr key={e.source + "/" + e.name} title={e.note}>
                      <td className="ic-name">{e.name}</td>
                      <td>{e.source}</td>
                      <td>{e.approx_size_gb > 0 ? `~${e.approx_size_gb} GB` : "núvol"}</td>
                      <td>{e.uncensored ? yes : no}</td>
                      <td>{e.vision ? yes : no}</td>
                      <td>{e.image_gen ? yes : no}</td>
                      <td>{e.video_gen ? yes : no}</td>
                      <td>
                        {e.source !== "ollama" ? (
                          e.installed ? (
                            <span className="mm-installed">clau lista</span>
                          ) : (
                            <span className="ic-no" title="Cal registrar el token al Gestor de proveïdors">
                              sense clau
                            </span>
                          )
                        ) : prog ? (
                          <span className="mm-prog">{prog}</span>
                        ) : e.installed ? (
                          <span className="mm-installed">
                            <Check size={12} /> instal·lat
                          </span>
                        ) : (
                          <button className="btn sm" onClick={() => void pullModel(e.name)}>
                            <Download size={12} /> {t("ai.pull")}
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}
