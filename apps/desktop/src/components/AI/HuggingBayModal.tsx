import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Clock,
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
import { useT } from "../../i18n";
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
  /// Tipus segons el mirror («llm», «embedding», «vision»…).
  kind?: string | null;
  /// Tasca del model («text-generation», «feature-extraction»…).
  task?: string | null;
}

/// El model serveix per XATAR? Els d'EMBEDDINGS (vectors) s'engeuen igual
/// però no generen text: NoOrbit ho diu abans de baixar-los.
const esDeXat = (m: { kind?: string | null; task?: string | null }): boolean => {
  const k = (m.kind ?? "").trim().toLowerCase();
  const t = (m.task ?? "").trim().toLowerCase();
  if (k === "embedding" || k === "embeddings") return false;
  if (t === "feature-extraction" || t === "embedding" || t === "embeddings") return false;
  return true;
};

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

/// Element de la cua de descàrregues. `key` és únic (el mateix model es pot
/// encuar amb quantitzacions/modes diferents); `id` és el del mirror (la
/// cancel·lació i el progrés del backend van per id).
interface QueueItem {
  key: string;
  id: string;
  name: string;
  quant: string;
  mode: "ollama" | "native";
  size: number | null;
  status: "pending" | "active" | "done" | "error" | "cancelled";
  message?: string;
}

/// Claus i18n de l'estat de cada element de la cua.
const QSTATUS: Record<QueueItem["status"], string> = {
  pending: "hfBay.qPending",
  active: "hfBay.qActive",
  done: "hfBay.qDone",
  error: "hfBay.qError",
  cancelled: "hfBay.qCancelled",
};

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
  const { t } = useT();
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
  const pulling = useAIStore((s) => s.pulling);
  const loadModels = useAIStore((s) => s.loadModels);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // On es desen els models GGUF (pot ser un disc extern triat pel usuari).
  const [ggufDir, setGgufDir] = useState("");
  // Espai al disc destí: lliure (per avisar ABANS de baixar un model enorme)
  // i total (la mida del disc/USB, per mostrar-la).
  const [freeBytes, setFreeBytes] = useState<number | null>(null);
  const [totalBytes, setTotalBytes] = useState<number | null>(null);
  /// Comptadors de generació: eviten que una resposta VELLA (pàgina
  /// anterior, model anterior) sobreEscrigui la nova (cursa clàssica en
  /// cerques ràpides: es canvia de pàgina/filtre i el llistat mostra el
  /// resultat que no correspon).
  const loadSeq = useRef(0);
  const detailsSeq = useRef(0);
  // CUA de descàrregues (seqüencial): el ref és autoritatiu i l'estat només
  // serveix per renderitzar. Viu mentre el component és muntat (sempre ho
  // està), de manera que la cua sobreviu al tancament del diàleg.
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const queueRef = useRef<QueueItem[]>([]);
  const runningRef = useRef(false);
  const queueSeq = useRef(0);

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
    // Espai al disc destí (lliure + total): per avisar si el model no hi cap
    // i per mostrar la mida del disc.
    void invoke<{ dir: string; free_bytes: number | null; total_bytes: number | null }>(
      "hf_bay_disk_space"
    )
      .then((r) => {
        setFreeBytes(r?.free_bytes ?? null);
        setTotalBytes(r?.total_bytes ?? null);
      })
      .catch(() => {
        setFreeBytes(null);
        setTotalBytes(null);
      });
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

  // Espai lliure al disc destí: es refresca en obrir, en canviar de carpeta
  // i en acabar cada descàrrega (el disc es va omplint).
  const refreshSpace = useCallback(() => {
    void invoke<{ dir: string; free_bytes: number | null; total_bytes: number | null }>(
      "hf_bay_disk_space"
    )
      .then((r) => {
        setFreeBytes(r?.free_bytes ?? null);
        setTotalBytes(r?.total_bytes ?? null);
      })
      .catch(() => {
        setFreeBytes(null);
        setTotalBytes(null);
      });
  }, []);

  /// Mutació autoritària de la cua: actualitza el ref I l'estat (render).
  const mutateQueue = useCallback((fn: (q: QueueItem[]) => QueueItem[]) => {
    queueRef.current = fn(queueRef.current);
    setQueue(queueRef.current);
  }, []);

  /// Bomba la cua: processa els elements «pending» d'un en un. Una descàrrega
  /// en curs no bloqueja la resta: s'encuen i s'executen quan toca.
  const pump = useCallback(async () => {
    if (runningRef.current) return;
    runningRef.current = true;
    try {
      for (;;) {
        const next = queueRef.current.find((it) => it.status === "pending");
        if (!next) break;
        mutateQueue((q) =>
          q.map((it) => (it.key === next.key ? { ...it, status: "active" } : it))
        );
        const cmd =
          next.mode === "native" ? "hf_bay_install_native" : "hf_bay_download";
        try {
          const res = await invoke<{
            ok: boolean;
            model: string | null;
            message: string;
          }>(cmd, { id: next.id, quant: next.quant });
          mutateQueue((q) =>
            q.map((it) =>
              it.key === next.key
                ? { ...it, status: res.ok ? "done" : "error", message: res.message }
                : it
            )
          );
          if (next.mode === "native") {
            // El proveïdor nou ha d'aparèixer al selector del xat.
            try {
              const { useProviderStore } = await import("../../stores/providerStore");
              await useProviderStore.getState().load();
            } catch {
              /* si no es pot refrescar, l'usuari pot tancar i obrir el xat */
            }
          }
          await loadModels();
        } catch (e) {
          const m = String(e);
          const cancelled = /cancel/i.test(m);
          mutateQueue((q) =>
            q.map((it) =>
              it.key === next.key
                ? { ...it, status: cancelled ? "cancelled" : "error", message: m }
                : it
            )
          );
        } finally {
          // El progrés d'aquest model ja no s'ha de mostrar.
          useAIStore.setState((s) => {
            const p = { ...s.pulling };
            delete p[`huggingbay:${next.id}`];
            return { pulling: p };
          });
          refreshSpace();
        }
      }
    } finally {
      runningRef.current = false;
    }
  }, [mutateQueue, loadModels, refreshSpace]);

  /// Afegeix el model triat a la cua (mode Ollama o natiu) i arrenca la bomba.
  const enqueue = useCallback(
    (mode: "ollama" | "native") => {
      if (!sel || !details) return;
      const dup = queueRef.current.some(
        (it) =>
          it.id === sel &&
          it.quant === quant &&
          it.mode === mode &&
          (it.status === "pending" || it.status === "active")
      );
      if (dup) return;
      const key = `${sel}:${mode}:${quant}:${++queueSeq.current}`;
      mutateQueue((q) => [
        ...q,
        {
          key,
          id: sel,
          name: details.model.name,
          quant,
          mode,
          size: details.recommended?.size ?? null,
          status: "pending",
        },
      ]);
      void pump();
    },
    [sel, details, quant, mutateQueue, pump]
  );

  /// Cancel·la un element: si és en curs, demana al backend que l'aturi (esborra
  /// el «.part»); si és en espera, el marca cancel·lat directament.
  const cancelItem = useCallback(
    (item: QueueItem) => {
      if (item.status === "active") {
        void invoke("hf_bay_cancel", { id: item.id }).catch(() => undefined);
      } else if (item.status === "pending") {
        mutateQueue((q) =>
          q.map((it) =>
            it.key === item.key
              ? { ...it, status: "cancelled", message: t("hfBay.qCancelled") }
              : it
          )
        );
      }
    },
    [mutateQueue, t]
  );

  /// Treu de la vista els elements acabats (fet/error/cancel·lat).
  const clearFinished = useCallback(() => {
    mutateQueue((q) =>
      q.filter((it) => it.status === "pending" || it.status === "active")
    );
  }, [mutateQueue]);

  /// TRIA ON ES BAIXEN ELS MODELS, DIRECTAMENT DES D'AQUÍ: l'usuari pot
  /// escollir una carpeta d'un DISC EXTERN (USB/HDD) sense haver d'anar a
  /// IA ▸ Models. La carpeta es desa a la configuració (gguf_dir) i les
  /// descàrregues següents hi van directament.
  const pickGgufDir = async () => {
    setMsg(null);
    try {
      const sel = await open({
        directory: true,
        title: t("hfBay.pickDirTitle"),
      });
      if (typeof sel !== "string") return;
      const m = await invoke<string>("llamacpp_set_dirs", {
        modelsDir: sel,
        engineDir: null,
      });
      setGgufDir(sel);
      refreshSpace();
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
      refreshSpace();
      setMsg(m);
    } catch (e) {
      setMsg(String(e));
    }
  };

  /// El model triat ja és a la cua (en espera o en curs) amb aquest mode?
  const isQueued = (mode: "ollama" | "native") =>
    queue.some(
      (it) =>
        it.id === sel &&
        it.quant === quant &&
        it.mode === mode &&
        (it.status === "pending" || it.status === "active")
    );

  if (!visible) return null;

  return (
    <div className="modal-overlay" onClick={close}>
      <div className="modal hb-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>{t("hfBay.title")}</span>
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
              placeholder={t("hfBay.searchPh")}
              onChange={(e) => setQuery(e.target.value)}
            />
            <div className="hb-sort">
              <button
                className={"btn sm" + (sort === "best" ? " primary" : "")}
                onClick={() => setSort("best")}
              >
                {t("hfBay.sortBest")}
              </button>
              <button
                className={"btn sm" + (sort === "downloads" ? " primary" : "")}
                onClick={() => setSort("downloads")}
              >
                {t("hfBay.sortDownloads")}
              </button>
            </div>
          </div>

          {/* LLISTA (5 per pàgina) */}
          <div className="hb-list">
            {loading && (
              <div className="hb-hint">
                <Loader2 size={13} className="spin" /> {t("hfBay.searching")}
              </div>
            )}
            {error && (
              <div className="hb-hint err">
                <AlertTriangle size={13} /> {error}
              </div>
            )}
            {!loading && page && page.models.length === 0 && (
              <div className="hb-hint">{t("hfBay.noResults")}</div>
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
                    {/* Els models d'EMBEDDINGS no serveixen per xatejar:
                        s'ho diem abans que l'usuari baixi 600 MB. */}
                    {!esDeXat(m) && <span className="hb-badge-embed">embeddings</span>}
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
              <ChevronLeft size={12} /> {t("hfBay.prev")}
            </button>
            <span className="hb-pager-info">
              {page ? t("hfBay.pageInfo", { page: stack.length + 1, total: num(page.total) }) : ""}
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
              {t("hfBay.next")} <ChevronRight size={12} />
            </button>
          </div>

          {/* DETALLS */}
          {sel && (
            <div className="hb-details">
              {detailsLoading && (
                <div className="hb-hint">
                  <Loader2 size={13} className="spin" /> {t("hfBay.loadingDetails")}
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
                      <span>{t("hfBay.quant")}</span>
                      <select value={quant} onChange={(e) => setQuant(e.target.value)}>
                        {["Q4_K_M", "Q5_K_M", "Q6_K", "Q8_0", "Q4_0", "F16"].map((q) => (
                          <option key={q} value={q}>
                            {q}
                          </option>
                        ))}
                      </select>
                    </div>
                    <div>
                      <span>{t("hfBay.ram")}</span>
                      <b>{details.ram_gb ? `${details.ram_gb} GB` : "—"}</b>
                    </div>
                    <div>
                      <span>{t("hfBay.status")}</span>
                      <b>{details.status ?? "—"}</b>
                    </div>
                  </div>
                  {details.recommended && (
                    <div className="hb-file">
                      <div>
                        <span className="hb-k">{t("hfBay.file")}</span> {details.recommended.path}
                      </div>
                      <div>
                        <span className="hb-k">{t("hfBay.size")}</span> {bytes(details.recommended.size)}
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
                      disabled={isQueued("ollama") || !esDeXat(details.model)}
                      onClick={() => enqueue("ollama")}
                      title={t("hfBay.downloadOllama")}
                    >
                      <Download size={12} />
                      {isQueued("ollama")
                        ? t("hfBay.alreadyQueued")
                        : t("hfBay.downloadOllama")}
                    </button>
                    {/* VIA NATIVA: el GGUF es queda al disc i s'engega amb el
                        motor local (llama.cpp): NO cal tenir Ollama. El model
                        es registra com a proveïdor i es pot triar al xat. */}
                    <button
                      className="btn sm hb-download"
                      disabled={isQueued("native") || !esDeXat(details.model)}
                      onClick={() => enqueue("native")}
                      title={t("hfBay.nativeHint")}
                    >
                      <Cpu size={12} />
                      {isQueued("native")
                        ? t("hfBay.alreadyQueued")
                        : t("hfBay.installNative")}
                    </button>
                  </div>
                  {/* AVÍS: model d'embeddings (no serveix per xatejar). */}
                  {!esDeXat(details.model) && (
                    <div className="hb-hint warn">
                      <AlertTriangle size={12} /> {t("hfBay.embeddingsWarn")}
                    </div>
                  )}
                  {/* ON ES DESA: l'usuari pot triar AQUÍ MATEIX una carpeta
                      d'un disc extern; les descàrregues hi van directament. */}
                  <div className="hb-where">
                    <HardDrive size={11} />
                    <span>
                      {t("hfBay.savedTo")}{" "}
                      <b>{ggufDir || t("hfBay.defaultDir")}</b>
                    </span>
                    {(freeBytes != null || totalBytes != null) && (
                      <span className="hb-free">
                        {totalBytes != null
                          ? t("hfBay.diskSpace", {
                              free: bytes(freeBytes),
                              total: bytes(totalBytes),
                            })
                          : t("hfBay.freeSpace", { size: bytes(freeBytes) })}
                      </span>
                    )}
                    <button
                      className="btn sm ghost hb-where-btn"
                      onClick={() => void pickGgufDir()}
                      title={t("hfBay.pickDirHint")}
                    >
                      <FolderOpen size={11} /> {t("hfBay.pickDir")}
                    </button>
                    {ggufDir && (
                      <button
                        className="btn sm ghost hb-where-btn"
                        onClick={() => void resetGgufDir()}
                        title={t("hfBay.resetDirHint")}
                      >
                        <RotateCcw size={11} /> {t("hfBay.resetDir")}
                      </button>
                    )}
                  </div>
                  {/* AVÍS D'ESPAI: el model no cap al disc destí. */}
                  {details.recommended?.size != null &&
                    freeBytes != null &&
                    details.recommended.size > freeBytes && (
                      <div className="hb-hint err">
                        <AlertTriangle size={12} />
                        {t("hfBay.notEnoughSpace", {
                          need: bytes(details.recommended.size),
                          free: bytes(freeBytes),
                        })}
                      </div>
                    )}
                </>
              )}
            </div>
          )}

          {/* CUA DE DESCÀRREGUES: una en curs, la resta en espera. Es pot
              cancel·lar qualsevol element i netejar els acabats. */}
          {queue.length > 0 && (
            <div className="hb-queue">
              <div className="hb-queue-head">
                <span>{t("hfBay.queueTitle", { n: queue.length })}</span>
                <button
                  className="btn sm ghost"
                  onClick={clearFinished}
                  disabled={
                    !queue.some(
                      (it) =>
                        it.status === "done" ||
                        it.status === "error" ||
                        it.status === "cancelled"
                    )
                  }
                >
                  {t("hfBay.clearFinished")}
                </button>
              </div>
              {queue.map((it) => {
                const prog =
                  it.status === "active" ? pulling[`huggingbay:${it.id}`] : null;
                return (
                  <div key={it.key} className={"hb-q-item " + it.status}>
                    <div className="hb-q-icon">
                      {it.status === "active" ? (
                        <Loader2 size={13} className="spin" />
                      ) : it.status === "done" ? (
                        <CheckCircle2 size={13} />
                      ) : it.status === "error" ? (
                        <AlertTriangle size={13} />
                      ) : it.status === "cancelled" ? (
                        <X size={13} />
                      ) : (
                        <Clock size={13} />
                      )}
                    </div>
                    <div className="hb-q-main">
                      <div className="hb-q-name">
                        {it.name} <span className="hb-q-quant">{it.quant}</span>
                        <span className="hb-q-mode">
                          {it.mode === "native"
                            ? t("hfBay.modeNative")
                            : t("hfBay.modeOllama")}
                        </span>
                      </div>
                      <div className="hb-q-meta">
                        <span>{t(QSTATUS[it.status])}</span>
                        {it.size ? <span>{bytes(it.size)}</span> : null}
                      </div>
                      {prog && <div className="hb-progress">{prog}</div>}
                      {it.message && <div className="hb-q-msg">{it.message}</div>}
                    </div>
                    {(it.status === "pending" || it.status === "active") && (
                      <button
                        className="icon-btn hb-q-cancel"
                        onClick={() => cancelItem(it)}
                        title={t("hfBay.cancelHint")}
                      >
                        <X size={12} />
                      </button>
                    )}
                  </div>
                );
              })}
            </div>
          )}

          {msg && (
            <div className="hb-msg">
              <CheckCircle2 size={13} /> {msg}
            </div>
          )}

          {/* ATRIBUCIÓ */}
          <div className="hb-credit">
            {t("hfBay.creditMadeBy")} <b>Joan Puigbert Carreras</b> · {t("hfBay.creditProject")}{" "}
            <b>Thepuigberts.com</b>
          </div>
        </div>
      </div>
    </div>
  );
}
