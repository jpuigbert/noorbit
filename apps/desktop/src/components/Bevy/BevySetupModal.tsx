import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  CheckCircle2,
  Download,
  FolderOpen,
  Loader2,
  RefreshCw,
  Rocket,
  X,
} from "lucide-react";
import { useUIStore } from "../../stores/uiStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import "./BevySetupModal.css";

/**
 * ASSISTENT DE BEVY — les dues preguntes que no es poden endevinar:
 *
 *  1. **On viu el Rust**: la instal·lació per defecte és `~/.cargo`, però
 *     l'usuari pot triar una altra carpeta (un disc extern, per exemple).
 *     NoOrbit li fa la pregunta, obre l'instal·lador oficial (rustup) en un
 *     Terminal TRANSPARENT i després DESA la tria, de manera que la
 *     detecció del «cargo» la trobi sempre.
 *  2. **On treballar-hi**: carpeta i nom del projecte Bevy. El projecte es
 *     crea amb el `Cargo.toml`, el `src/main.rs` i el `.gitignore` correctes
 *     i tot seguit es VERIFICA amb el mateix detector que fa servir l'IA.
 *
 * Tots els textos són en català, com la resta de l'aplicació.
 */

interface BevyStatus {
  installed: boolean;
  version: string;
  path: string;
  cargoHome: string;
  rustupHome: string;
  toolchain: string;
}

interface BevyProject {
  dir: string;
  manifest: string;
  detected: boolean;
  bevyVersion: string;
  cargoInstalled: boolean;
  cargoPath: string;
}

interface BevySetupInfo {
  status: BevyStatus;
  defaultProjectsDir: string;
  workspaceProject: BevyProject | null;
}

export default function BevySetupModal() {
  const visible = useUIStore((s) => s.showBevySetup);
  const setVisible = useUIStore((s) => s.setShowBevySetup);
  const root = useWorkspaceStore((s) => s.root);

  const [info, setInfo] = useState<BevySetupInfo | null>(null);
  const [project, setProject] = useState<BevyProject | null>(null);
  /// carpeta on es crea el projecte Bevy
  const [parent, setParent] = useState("");
  /// nom del projecte Bevy
  const [name, setName] = useState("meu-joc");
  /// «default» = la carpeta del sistema; «custom» = les que tria l'usuari
  const [lloc, setLloc] = useState<"default" | "custom">("default");
  const [cargoHome, setCargoHome] = useState("");
  const [rustupHome, setRustupHome] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const revisar = async () => {
    try {
      const data = await invoke<BevySetupInfo>("bevy_setup_info", { workspace: root ?? "" });
      setInfo(data);
      setParent((p) => p || data.defaultProjectsDir || "");
      if (data.workspaceProject) setProject(data.workspaceProject);
    } catch (e) {
      setErr(String(e));
    }
  };

  useEffect(() => {
    if (!visible) return;
    setMsg(null);
    setErr(null);
    void revisar();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible, root]);

  const status = info?.status;
  const instalat = !!status?.installed;

  const triarCarpeta = async (destino: "projecte" | "cargo" | "rustup") => {
    try {
      const triat = await open({
        directory: true,
        multiple: false,
        title:
          destino === "projecte"
            ? "Tria la carpeta on crear el projecte Bevy"
            : destino === "cargo"
              ? "Tria la carpeta del «cargo»"
              : "Tria la carpeta del «rustup»",
      });
      if (typeof triat !== "string" || !triat) return;
      if (destino === "projecte") setParent(triat);
      else if (destino === "cargo") {
        setCargoHome(triat);
        setLloc("custom");
      } else {
        setRustupHome(triat);
        setLloc("custom");
      }
    } catch (e) {
      setErr(String(e));
    }
  };

  const instalar = async () => {
    setBusy("instal·la");
    setErr(null);
    setMsg(null);
    try {
      const resposta = await invoke<string>("bevy_install_rust", {
        cargoHome: lloc === "custom" ? cargoHome : "",
        rustupHome: lloc === "custom" ? rustupHome : "",
      });
      setMsg(resposta);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
      await revisar();
    }
  };

  const crear = async () => {
    setBusy("crea");
    setErr(null);
    setMsg(null);
    try {
      const p = await invoke<BevyProject>("bevy_create_project", { parent, name });
      setProject(p);
      setMsg(
        `Projecte Bevy creat a «${p.dir}»${
          p.bevyVersion ? ` (Bevy ${p.bevyVersion})` : ""
        }. Ja el detectem com a projecte Bevy.`,
      );
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  const detectar = async () => {
    setBusy("detecta");
    setErr(null);
    setMsg(null);
    try {
      const triat = await open({
        directory: true,
        multiple: false,
        title: "Tria la carpeta del projecte Bevy",
      });
      if (typeof triat !== "string" || !triat) return;
      const p = await invoke<BevyProject>("bevy_detect_project", { path: triat });
      if (!p.detected) {
        setErr(
          `a «${p.dir}» no hi ha cap projecte Bevy: cal un «Cargo.toml» amb la dependència «bevy».`,
        );
        return;
      }
      setProject(p);
      setMsg(`Projecte Bevy detectat a «${p.dir}».`);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  const obrir = async () => {
    if (!project) return;
    setBusy("obre");
    setErr(null);
    setMsg(null);
    try {
      const resposta = await invoke<string>("open_project_in_engine", {
        projectPath: project.dir,
        engine: "bevy",
      });
      setMsg(resposta);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  if (!visible) return null;

  return (
    <div className="modal-overlay" onClick={() => setVisible(false)}>
      <div className="modal bs-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>Bevy (Rust) — posa’t en marxa</span>
          <button className="icon-btn" onClick={() => setVisible(false)}>
            <X size={14} />
          </button>
        </div>
        <div className="modal-body">
          <p className="bs-intro">
            Treballar en Bevy vol dir dues coses: <strong>on viu el Rust</strong> (el
            «cargo») i <strong>on treballar el projecte</strong>. Aquí les preguntes
            i les resposeses.
          </p>

          {/* ── 1. DETECCIÓ ─────────────────────────────────────────── */}
          <section className="bs-step">
            <div className="bs-step-head">
              <span className="bs-step-no">1</span>
              <span className="bs-step-title">Què hi ha instal·lat</span>
            </div>
            {!status ? (
              <div className="bs-desc">Comprovant…</div>
            ) : instalat ? (
              <div className="bs-ok">
                <CheckCircle2 size={14} />
                <div>
                  <div>
                    <strong>Rust detectat</strong>
                    {status.version ? ` · ${status.version}` : ""}
                  </div>
                  <div className="bs-path">{status.path}</div>
                  {status.toolchain ? (
                    <div className="bs-path">toolchain: {status.toolchain}</div>
                  ) : null}
                  <div className="bs-path">
                    CARGO_HOME: {status.cargoHome || "—"} · RUSTUP_HOME:{" "}
                    {status.rustupHome || "—"}
                  </div>
                </div>
              </div>
            ) : (
              <div className="bs-warn">
                <AlertCircle size={14} />
                <div>
                  <strong>Rust (cargo) no està instal·lat</strong>
                  <div className="bs-desc">
                    Cal el «cargo» per compilar Bevy. Tria on instal·lar-lo.
                  </div>
                </div>
              </div>
            )}
          </section>

          {/* ── 2. ON INSTAL·LAR RUST ──────────────────────────────── */}
          {!instalat && (
            <section className="bs-step">
              <div className="bs-step-head">
                <span className="bs-step-no">2</span>
                <span className="bs-step-title">On instal·lar Rust</span>
              </div>
              <label className="bs-radio">
                <input
                  type="radio"
                  name="bs-lloc"
                  checked={lloc === "default"}
                  onChange={() => setLloc("default")}
                />
                <span>
                  Per defecte (recomanat)
                  <em className="bs-hint">
                    {status?.cargoHome
                      ? `${status.cargoHome} i ${status.rustupHome}`
                      : "~/.cargo i ~/.rustup"}
                  </em>
                </span>
              </label>
              <label className="bs-radio">
                <input
                  type="radio"
                  name="bs-lloc"
                  checked={lloc === "custom"}
                  onChange={() => setLloc("custom")}
                />
                <span>
                  Altra carpeta (per exemple, un disc extern)
                  <em className="bs-hint">
                    NoOrbit hi buscarà el «cargo» cada vegada que el detecti.
                  </em>
                </span>
              </label>
              {lloc === "custom" && (
                <div className="bs-fields">
                  <div className="field">
                    <label>Carpeta del «cargo» (CARGO_HOME)</label>
                    <div className="bs-row">
                      <input
                        value={cargoHome}
                        placeholder="/Users/…/.cargo"
                        onChange={(e) => setCargoHome(e.target.value)}
                      />
                      <button
                        className="btn sm"
                        onClick={() => void triarCarpeta("cargo")}
                        type="button"
                      >
                        <FolderOpen size={12} /> Tria
                      </button>
                    </div>
                  </div>
                  <div className="field">
                    <label>Carpeta del «rustup» (RUSTUP_HOME)</label>
                    <div className="bs-row">
                      <input
                        value={rustupHome}
                        placeholder="/Users/…/.rustup"
                        onChange={(e) => setRustupHome(e.target.value)}
                      />
                      <button
                        className="btn sm"
                        onClick={() => void triarCarpeta("rustup")}
                        type="button"
                      >
                        <FolderOpen size={12} /> Tria
                      </button>
                    </div>
                  </div>
                </div>
              )}
              <div className="bs-actions">
                <button
                  className="btn primary sm"
                  disabled={busy !== null}
                  onClick={() => void instalar()}
                >
                  {busy === "instal·la" ? (
                    <Loader2 size={12} className="bs-spin" />
                  ) : (
                    <Download size={12} />
                  )}
                  {busy === "instal·la" ? "Instal·lant…" : "Instal·la Rust"}
                </button>
                <span className="bs-hint">
                  S’obre el Terminal amb l’instal·lador oficial (rustup): hi veus
                  tot el que s’executa.
                </span>
              </div>
            </section>
          )}

          {/* ── 3. ON TREBALLAR-HI ─────────────────────────────────── */}
          <section className="bs-step">
            <div className="bs-step-head">
              <span className="bs-step-no">{instalat ? 2 : 3}</span>
              <span className="bs-step-title">On treballar el projecte Bevy</span>
            </div>
            <div className="field">
              <label>Nom del projecte</label>
              <input value={name} onChange={(e) => setName(e.target.value)} />
            </div>
            <div className="field">
              <label>Carpeta on es crea</label>
              <div className="bs-row">
                <input value={parent} onChange={(e) => setParent(e.target.value)} />
                <button
                  className="btn sm"
                  type="button"
                  onClick={() => void triarCarpeta("projecte")}
                >
                  <FolderOpen size={12} /> Tria
                </button>
              </div>
            </div>
            <div className="bs-actions">
              <button
                className="btn primary sm"
                disabled={busy !== null || !parent || !name}
                onClick={() => void crear()}
              >
                {busy === "crea" ? (
                  <Loader2 size={12} className="bs-spin" />
                ) : (
                  <Rocket size={12} />
                )}
                {busy === "crea" ? "Creant…" : "Crea el projecte"}
              </button>
              <button
                className="btn sm"
                disabled={busy !== null}
                type="button"
                onClick={() => void detectar()}
              >
                {busy === "detecta" ? (
                  <Loader2 size={12} className="bs-spin" />
                ) : (
                  <FolderOpen size={12} />
                )}
                Detecta un projecte existent
              </button>
            </div>
          </section>

          {/* ── 4. RESULTAT ────────────────────────────────────────── */}
          {project && (
            <section className="bs-step">
              <div className="bs-step-head">
                <span className="bs-step-no">{instalat ? 3 : 4}</span>
                <span className="bs-step-title">Projecte</span>
              </div>
              <div className="bs-ok">
                <CheckCircle2 size={14} />
                <div>
                  <div>
                    <strong>Projecte Bevy detectat</strong>
                    {project.bevyVersion ? ` · Bevy ${project.bevyVersion}` : ""}
                  </div>
                  <div className="bs-path">{project.dir}</div>
                  {!project.cargoInstalled && (
                    <div className="bs-path bs-warn-txt">
                      ⚠︎ sense «cargo»: compila’t el Rust abans de fer res.
                    </div>
                  )}
                </div>
              </div>
              <div className="bs-actions">
                <button
                  className="btn primary sm"
                  disabled={busy !== null}
                  onClick={() => void obrir()}
                >
                  {busy === "obre" ? (
                    <Loader2 size={12} className="bs-spin" />
                  ) : (
                    <Rocket size={12} />
                  )}
                  Obre el projecte (cargo run)
                </button>
                <button
                  className="btn sm"
                  type="button"
                  onClick={() => void invoke("open_externally", { path: project.dir })}
                >
                  <FolderOpen size={12} /> Mostra la carpeta
                </button>
              </div>
            </section>
          )}

          {msg && <div className="bs-msg">{msg}</div>}
          {err && <div className="bs-msg bs-msg-err">{err}</div>}

          <div className="bs-foot">
            <button
              className="btn sm ghost"
              disabled={busy !== null}
              onClick={() => void revisar()}
            >
              <RefreshCw size={12} /> Revisa
            </button>
            <button className="btn primary sm" onClick={() => setVisible(false)}>
              Tanca
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
