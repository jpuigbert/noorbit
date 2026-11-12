import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Cpu,
  Download,
  FileCode2,
  FolderOpen,
  HardDrive,
  Loader2,
  RotateCcw,
  Search,
  ShieldCheck,
  X,
} from "lucide-react";
import { useAIStore } from "../../stores/aiStore";
import { useUIStore } from "../../stores/uiStore";
import "./HuggingBayModal.css";

/**
 * MENÚ «HUGGING BAY» — el mirror verificat de Hugging Face, dins de NoOrbit.
 *
 * - Cercador de models GGUF (filtre format=gguf del mirror).
 * - «Els 5 més descarregats» de cada pàgina (amb paginació pel cursor).
 * - Detalls: fitxer recomanat (Q4_K_M), mida, SHA-256, llicència i RAM.
 * - Descàrrega amb progrés en viu i registre a Ollama (Modelfile «FROM»).
 *
 * Mirror: huggingbay.xyz (enllaços signats de 5 min, SHA-256 verificable).
 */

interface BayModel {
  id: string;
  repo: string;
  owner: string;
  name: string;
  description: string;
  license: string;
  size: number | null;
  downloads: number | null;
  url: string | null;
}

interface BayFile {
  path: string;
  size: number | null;
  sha256: string | null;
  download_url: string | null;
  quant: string | null;
}

interface BayDetails {
  model: BayModel;
  recommended: BayFile | null;
  files: BayFile[];
  status: string | null;
  downloadable: boolean;
  ram_gb: number | null;
  requested_quant: string | null;
}

interface BayPage {
  models: BayModel[];
  next_cursor: string | null;
  total: number;
}

const PAGE_SIZE = 5;

function bytes(n: number | null | undefined): string {
  if (!n || n <= 0) return "—";
  const u = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1000 && i < u.length - 1) {
    v /= 1000;
    i++;
  }
  return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${u[i]}`;
}

function num(n: number | null | undefined): string {
  if (!n) return "—";
  return n.toLocaleString("ca");
}

export default function HuggingBayModal() {
  const [visible, setVisible] = useState(false);
  const showMenu = useUIStore((s) => s.showHuggingBay);
  const setShowMenu = useUIStore((s) => s.setShowHuggingBay);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<"best" | "downloads">("best");
  const [cursor, setCursor] = useState<string | null>(null);
  const [stack, setStack] = useState<string[]>([]); // historial de cursors
  const [page, setPage] = useState<BayPage | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sel, setSel] = useState<string | null>(null);
  const [details, setDetails] = useState<BayDetails | null>(null);
  const [detailsLoading, setDetailsLoading] = useState(false);
  const [quant, setQuant] = useState<string>("Q4_K_M");
  const [msg, setMsg] = useState<string | null>(null);
  const [native, setNative] = useState(false);
  const pulling = useAIStore((s) => s.pulling);
  const loadModels = useAIStore((s) => s.loadModels);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // On es desen els models GGUF (pot ser un disc extern triat pel usuari).
  const [ggufDir, setGgufDir] = useState("");
  /// Comptadors de generació: eviten que una resposta VELLA (pàgina
  /// anterior, model anterior) sobreEscrigui la nova (cursa clàssica en
  /// cerques ràpides: es canvia de pàgina/filtre i el llistat mostra el
  /// resultat que no correspon).
  const loadSeq = useRef(0);
  const detailsSeq = useRef(0);

  const load = useCallback(
    async (q: string, s: "best" | "downloads", c: string | null) => {
      const my = ++loadSeq.current;
      setLoading(true);
      setError(null);
      try {
        const p = await invoke<BayPage>("hf_bay_page", {
          query: q,
          sort: s,
          cursor: c,
          limit: PAGE_SIZE,
        });
        if (my !== loadSeq.current) return; // ha arribat tard: descarta
        setPage(p);
      } catch (e) {
        if (my !== loadSeq.current) return;
        setError(String(e));
        setPage(null);
      } finally {
        if (my === loadSeq.current) setLoading(false);
      }
    },
    []
  );

  // Obertura: carrega la pàgina inicial (els més descarregats).
  useEffect(() => {
    if (!showMenu) return;
    setVisible(true);
    setStack([]);
    setCursor(null);
    setSel(null);
    void load("", "downloads", null);
    // On es desen els models (disc extern o dades de NoOrbit).
    void invoke<{ modelsDir: string }>("llamacpp_status")
      .then((st) => setGgufDir(st?.modelsDir ?? ""))
      .catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showMenu]);

  const close = () => {
    setVisible(false);
    setShowMenu(false);
  };

  // Cerca amb retard (per no saturar el mirror). Només amb el modal obert:
  // abans es llançava la consulta fins i tot amb el diàleg tancat.
  useEffect(() => {
    if (!visible) return;
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      setStack([]);
      setCursor(null);
      void load(query, sort, null);
    }, 350);
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, sort, visible]);

  // Detalls del model triat.
  useEffect(() => {
    if (!sel) {
      setDetails(null);
      return;
    }
    const my = ++detailsSeq.current;
    setDetailsLoading(true);
    invoke<BayDetails>("hf_bay_details", { id: sel, quant })
      .then((d) => {
        if (my !== detailsSeq.current) return; // ha arribat tard: descarta
        setDetails(d);
      })
      .catch((e) => {
        if (my !== detailsSeq.current) return;
        setError(String(e));
      })
      .finally(() => {
        if (my === detailsSeq.current) setDetailsLoading(false);
      });
  }, [sel, quant]);

  // Progrés de la descàrrega (event del backend).
  const progress = useMemo(() => {
    const e = Object.entries(pulling).find(([k]) => k.startsWith("huggingbay:"));
    return e ? e[1] : null;
  }, [pulling]);

  const download = async () => {
    if (!sel) return;
    setMsg(null);
    try {
      const res = await invoke<{ ok: boolean; model: string | null; message: string }>(
        "hf_bay_download",
        { id: sel, quant }
      );
      setMsg(res.message);
      await loadModels();
    } catch (e) {
      setMsg(String(e));
    }
  };

  /// Instal·lació NATIVA (sense Ollama): el GGUF es guarda i s'engega amb el
  /// motor local (llama.cpp). El model queda registrat com a proveïdor, així
  /// que es pot triar al xat com qualsevol altra IA.
  const installNative = async () => {
    if (!sel) return;
    setMsg(null);
    setNative(true);
    try {
      const res = await invoke<{ ok: boolean; model: string | null; message: string }>(
        "hf_bay_install_native",
        { id: sel, quant }
      );
      setMsg(res.message);
      await loadModels();
      // El proveïdor nou ha d'aparèixer al selector del xat.
      try {
        const { useProviderStore } = await import("../../stores/providerStore");
        await useProviderStore.getState().load();
      } catch {
        /* si no es pot refrescar, l'usuari pot tancar i obrir el xat */
      }
    } catch (e) {
      setMsg(String(e));
    } finally {
      setNative(false);
    }
  };

  /// TRIA ON ES BAIXEN ELS MODELS, DIRECTAMENT DES D'AQUÍ: l'usuari pot
  /// escollir una carpeta d'un DISC EXTERN (USB/HDD) sense haver d'anar a
  /// IA ▸ Models. La carpeta es desa a la configuració (gguf_dir) i les
  /// descàrregues següents hi van directament.
  const pickGgufDir = async () => {
    setMsg(null);
    try {
      const sel = await open({
        directory: true,
        title: "Tria la carpeta (p. ex. d'un disc extern) on es baixaran els models GGUF",
      });
      if (typeof sel !== "string") return;
      const m = await invoke<string>("llamacpp_set_dirs", {
        modelsDir: sel,
        engineDir: null,
      });
      setGgufDir(sel);
      setMsg(m);
    } catch (e) {
      setMsg(String(e));
    }
  };

  /// Torna al comportament per defecte: els models es desen a les dades de
  /// NoOrbit (carpeta buida = sense disc extern).
  const resetGgufDir = async () => {
    setMsg(null);
    try {
      const m = await invoke<string>("llamacpp_set_dirs", {
        modelsDir: "",
        engineDir: null,
      });
      setGgufDir("");
      setMsg(m);
    } catch (e) {
      setMsg(String(e));
    }
  };

  if (!visible) return null;

  return (
    <div className="modal-overlay" onClick={close}>
      <div className="modal hb-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>Hugging Bay · models GGUF</span>
          <button className="icon-btn" onClick={close}>
            <X size={14} />
          </button>
        </div>
        <div className="modal-body hb-body">
          {/* CERCADOR */}
          <div className="hb-search">
            <Search size={13} />
            <input
              autoFocus
              value={query}
              placeholder="Cerca models GGUF (p. ex. «qwen3 coder 4b»)…"
              onChange={(e) => setQuery(e.target.value)}
            />
            <div className="hb-sort">
              <button
                className={"btn sm" + (sort === "best" ? " primary" : "")}
                onClick={() => setSort("best")}
              >
                Relevància
              </button>
              <button
                className={"btn sm" + (sort === "downloads" ? " primary" : "")}
                onClick={() => setSort("downloads")}
              >
                Més descarregats
              </button>
            </div>
          </div>

          {/* LLISTA (5 per pàgina) */}
          <div className="hb-list">
            {loading && (
              <div className="hb-hint">
                <Loader2 size={13} className="spin" /> Buscant al mirror…
              </div>
            )}
            {error && (
              <div className="hb-hint err">
                <AlertTriangle size={13} /> {error}
              </div>
            )}
            {!loading && page && page.models.length === 0 && (
              <div className="hb-hint">Cap model trobat amb aquesta cerca.</div>
            )}
            {page?.models.map((m) => (
              <button
                key={m.id}
                className={"hb-item" + (sel === m.id ? " active" : "")}
                onClick={() => setSel(m.id)}
              >
                <div className="hb-item-main">
                  <div className="hb-item-name">{m.name}</div>
                  <div className="hb-item-meta">
                    {m.owner ? <span>{m.owner}</span> : null}
                    <span>{bytes(m.size)}</span>
                    {m.downloads ? <span>⬇ {num(m.downloads)}</span> : null}
                    {m.license ? <span className="hb-lic">{m.license}</span> : null}
                  </div>
                </div>
              </button>
            ))}
          </div>

          {/* PAGINACIÓ */}
          <div className="hb-pager">
            <button
              className="btn sm ghost"
              disabled={stack.length === 0}
              onClick={() => {
                const s = stack.slice();
                const prev = s.pop() ?? null;
                setStack(s);
                setCursor(prev);
                void load(query, sort, prev);
              }}
            >
              <ChevronLeft size={12} /> Anterior
            </button>
            <span className="hb-pager-info">
              {page ? `Pàgina ${stack.length + 1} · ${num(page.total)} models al mirror` : ""}
            </span>
            <button
              className="btn sm ghost"
              disabled={!page?.next_cursor}
              onClick={() => {
                const c = page?.next_cursor ?? null;
                if (!c) return;
                setStack([...stack, cursor ?? ""]);
                setCursor(c);
                void load(query, sort, c);
              }}
            >
              Següent <ChevronRight size={12} />
            </button>
          </div>

          {/* DETALLS */}
          {sel && (
            <div className="hb-details">
              {detailsLoading && (
                <div className="hb-hint">
                  <Loader2 size={13} className="spin" /> Llegint els detalls…
                </div>
              )}
              {details && (
                <>
                  <div className="hb-details-title">
                    <FileCode2 size={13} />
                    {details.model.name}
                    {details.model.license ? (
                      <span className="hb-lic">{details.model.license}</span>
                    ) : null}
                  </div>
                  <div className="hb-fields">
                    <div>
                      <span>Quantització</span>
                      <select value={quant} onChange={(e) => setQuant(e.target.value)}>
                        {["Q4_K_M", "Q5_K_M", "Q6_K", "Q8_0", "Q4_0", "F16"].map((q) => (
                          <option key={q} value={q}>
                            {q}
                          </option>
                        ))}
                      </select>
                    </div>
                    <div>
                      <span>RAM estimada</span>
                      <b>{details.ram_gb ? `${details.ram_gb} GB` : "—"}</b>
                    </div>
                    <div>
                      <span>Estat</span>
                      <b>{details.status ?? "—"}</b>
                    </div>
                  </div>
                  {details.recommended && (
                    <div className="hb-file">
                      <div>
                        <span className="hb-k">Fitxer</span> {details.recommended.path}
                      </div>
                      <div>
                        <span className="hb-k">Mida</span> {bytes(details.recommended.size)}
                      </div>
                      <div className="hb-sha">
                        <span className="hb-k">
                          <ShieldCheck size={11} /> SHA-256
                        </span>{" "}
                        <code>{details.recommended.sha256 ?? "—"}</code>
                      </div>
                    </div>
                  )}
                  <div className="hb-actions">
                    <button
                      className="btn primary sm hb-download"
                      disabled={!!progress || native}
                      onClick={() => void download()}
                    >
                      {progress ? (
                        <Loader2 size={12} className="spin" />
                      ) : (
                        <Download size={12} />
                      )}
                      {progress ? "Baixant…" : "Baixa i instal·la a Ollama"}
                    </button>
                    {/* VIA NATIVA: el GGUF es queda al disc i s'engega amb el
                        motor local (llama.cpp): NO cal tenir Ollama. El model
                        es registra com a proveïdor i es pot triar al xat. */}
                    <button
                      className="btn sm hb-download"
                      disabled={!!progress || native}
                      onClick={() => void installNative()}
                      title="Descarrega el GGUF i l'engega amb el motor local (llama.cpp), sense Ollama. Queda disponible com a IA al xat."
                    >
                      {native ? (
                        <Loader2 size={12} className="spin" />
                      ) : (
                        <Cpu size={12} />
                      )}
                      {native ? "Instal·lant…" : "Instal·la nativament (sense Ollama)"}
                    </button>
                  </div>
                  {/* ON ES DESA: l'usuari pot triar AQUÍ MATEIX una carpeta
                      d'un disc extern; les descàrregues hi van directament. */}
                  <div className="hb-where">
                    <HardDrive size={11} />
                    <span>
                      Es desa a:{" "}
                      <b>{ggufDir || "les dades de NoOrbit"}</b>
                    </span>
                    <button
                      className="btn sm ghost hb-where-btn"
                      onClick={() => void pickGgufDir()}
                      title="Tria una carpeta d'un disc extern (USB/HDD): els models GGUF s'hi baixaran directament."
                    >
                      <FolderOpen size={11} /> Tria carpeta / disc extern…
                    </button>
                    {ggufDir && (
                      <button
                        className="btn sm ghost hb-where-btn"
                        onClick={() => void resetGgufDir()}
                        title="Torna a desar els models a les dades de NoOrbit."
                      >
                        <RotateCcw size={11} /> Per defecte
                      </button>
                    )}
                  </div>
                  {progress && <div className="hb-progress">{progress}</div>}
                </>
              )}
            </div>
          )}

          {msg && (
            <div className="hb-msg">
              <CheckCircle2 size={13} /> {msg}
            </div>
          )}

          {/* ATRIBUCIÓ */}
          <div className="hb-credit">
            Fet per <b>Joan Puigbert Carreras</b> · Projecte de{" "}
            <b>Thepuigberts.com</b>
          </div>
        </div>
      </div>
    </div>
  );
}
