import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  CloudUpload,
  FolderOpen,
  Loader2,
  Lock,
  LogIn,
  Package,
  UserPlus,
  X,
} from "lucide-react";
import { useAuthStore } from "../../stores/authStore";
import { useUIStore } from "../../stores/uiStore";
import { useT } from "../../i18n";
import "./PublishThemeModal.css";

/**
 * MENÚ «PUJAR» — publica un tema WordPress al marketplace
 * (themes.thepuigberts.com).
 *
 * Passos:
 *  1. **Tria la carpeta del tema** → NoOrbit la revisa (capçalera de
 *     `style.css`, `screenshot.png`, fitxers que no s'han de pujar).
 *  2. **Empaqueta** el ZIP (WordPress demana una sola carpeta a dins).
 *  3. **Identifica't**: amb el compte de NoOrbit (el registre que ja existia)
 *     i amb les credencials del lloc (usuari WordPress + contrasenya
 *     d'aplicació) → pujada via l'API REST.
 *
 * Les credencials del lloc es desen al compte de l'usuari (mai al codi).
 */

interface ThemeInfo {
  name: string;
  version: string;
  description: string;
  requiresPhp: string;
  requiresAtLeast: string;
  author: string;
  hasStyleCss: boolean;
  hasScreenshot: boolean;
  screenshotSize: [number, number] | null;
  files: string[];
  warnings: string[];
  errors: string[];
  dir: string;
}

const SITE_DEFECTE = "https://themes.thepuigberts.com";

export default function PublishThemeModal() {
  const { t } = useT();
  const [visible, setVisible] = useState(false);
  const showMenu = useUIStore((s) => s.showPublishTheme);
  const setShowMenu = useUIStore((s) => s.setShowPublishTheme);
  const [info, setInfo] = useState<ThemeInfo | null>(null);
  const [zip, setZip] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  // Comptes de NoOrbit (el sistema de registre ja existent).
  const user = useAuthStore((s) => s.user);
  const signUp = useAuthStore((s) => s.signUp);
  const logIn = useAuthStore((s) => s.logIn);
  const [mode, setMode] = useState<"login" | "signup">("login");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [pass, setPass] = useState("");

  // Credencials del lloc.
  const [site, setSite] = useState(SITE_DEFECTE);
  const [wpUser, setWpUser] = useState("");
  const [wpPass, setWpPass] = useState("");

  useEffect(() => {
    if (!showMenu) return;
    setVisible(true);
    // Obertura: recupera la connexió desada del compte (si n'hi ha).
    const c = user?.connections.find((x) => x.kind === "wordpress");
    if (c) {
      if (c.url) setSite(c.url);
      if (c.username) setWpUser(c.username);
      if (c.secret) setWpPass(c.secret);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showMenu]);

  const close = () => {
    setVisible(false);
    setShowMenu(false);
  };

  const pickTheme = async () => {
    setErr(null);
    setMsg(null);
    try {
      const dir = await open({
        directory: true,
        multiple: false,
        title: t("publish.pickDir"),
      });
      if (typeof dir !== "string") return;
      setBusy("inspect");
      const i = await invoke<ThemeInfo>("theme_inspect", { dir });
      setInfo(i);
      setZip(null);
      if (i.errors.length === 0) {
        setBusy("zip");
        const [z] = await invoke<[string, string]>("theme_build_zip", { dir });
        setZip(z);
        setMsg(t("publish.zipPrepared", { name: z.split("/").pop() ?? "" }));
      }
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  const doAuth = async () => {
    setErr(null);
    try {
      if (mode === "signup") await signUp(name, email, pass);
      else await logIn(email, pass);
      setMsg(t("publish.accountVerified"));
    } catch (e) {
      setErr(String(e));
    }
  };

  const upload = async () => {
    setErr(null);
    setMsg(null);
    if (!zip) {
      setErr(t("publish.needZip"));
      return;
    }
    setBusy("upload");
    try {
      // Amb credencials: API REST (autenticació bàsica). Sense: la SESSIÓ del
      // navegador (el que ha iniciat l'usuari al login del lloc).
      const res = await invoke<{
        ok: boolean;
        message: string;
        attachmentId: number | null;
        url: string | null;
      }>(
        wpUser.trim() && wpPass.trim() ? "theme_upload" : "theme_upload_session",
        {
          zip,
          site,
          user: wpUser,
          appPassword: wpPass,
        }
      );
      setMsg(res.message);
      // Desa la connexió al compte per no tornar-ho a posar.
      if (wpUser.trim() && wpPass.trim()) {
        useAuthStore
          .getState()
          .addConnection({
            kind: "wordpress",
            label: t("publish.connectionLabel"),
            url: site,
            username: wpUser,
            secret: wpPass,
          });
      }
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  /// Obre la pàgina de LOGIN del lloc al navegador intern de NoOrbit: hi
  /// entra l'usuari amb el seu compte i, quan torni, la pujada es fa amb
  /// aquesta sessió.
  const openLogin = async () => {
    setErr(null);
    setMsg(null);
    setBusy("login");
    try {
      setMsg(await invoke<string>("theme_open_login", { site }));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  const openSales = async () => {
    try {
      setMsg(await invoke<string>("theme_open_sales_page", { site }));
    } catch (e) {
      setErr(String(e));
    }
  };

  if (!visible) return null;

  return (
    <div className="modal-overlay" onClick={close}>
      <div className="modal pb-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>{t("publish.title")}</span>
          <button className="icon-btn" onClick={close}>
            <X size={14} />
          </button>
        </div>
        <div className="modal-body pb-body">
          {/* 1 · TEMA */}
          <section className="pb-step">
            <div className="pb-step-head">
              <span className="pb-num">1</span> {t("publish.step1")}
              <button className="btn sm" onClick={() => void pickTheme()} disabled={busy !== null}>
                <FolderOpen size={12} /> {t("publish.chooseFolder")}
              </button>
            </div>
            {busy === "inspect" && (
              <div className="pb-hint">
                <Loader2 size={12} className="spin" /> {t("publish.inspecting")}
              </div>
            )}
            {info && (
              <div className="pb-theme">
                <div className="pb-theme-name">
                  <Package size={13} /> {info.name || t("publish.noName")}
                  {info.version ? <span className="pb-ver">v{info.version}</span> : null}
                </div>
                {info.description ? (
                  <div className="pb-theme-desc">{info.description}</div>
                ) : null}
                <div className="pb-checks">
                  <span className={info.hasStyleCss ? "ok" : "ko"}>
                    {info.hasStyleCss ? <CheckCircle2 size={11} /> : <AlertTriangle size={11} />}
                    style.css
                  </span>
                  <span className={info.hasScreenshot ? "ok" : "warn"}>
                    {info.hasScreenshot ? <CheckCircle2 size={11} /> : <AlertTriangle size={11} />}
                    screenshot.png
                    {info.screenshotSize
                      ? ` (${info.screenshotSize[0]}×${info.screenshotSize[1]})`
                      : ""}
                  </span>
                  <span className="info">{t("publish.filesCount", { n: info.files.length })}</span>
                  {info.requiresPhp ? <span className="info">PHP {info.requiresPhp}</span> : null}
                </div>
                {info.errors.map((e, i) => (
                  <div className="pb-line ko" key={`e${i}`}>
                    <AlertTriangle size={11} /> {e}
                  </div>
                ))}
                {info.warnings.slice(0, 4).map((w, i) => (
                  <div className="pb-line warn" key={`w${i}`}>
                    <AlertTriangle size={11} /> {w}
                  </div>
                ))}
                {zip && (
                  <div className="pb-line ok">
                    <CheckCircle2 size={11} /> {t("publish.zipReady", { name: zip.split("/").pop() ?? "" })}
                  </div>
                )}
              </div>
            )}
          </section>

          {/* 2 · COMTE DE NOORBIT */}
          <section className="pb-step">
            <div className="pb-step-head">
              <span className="pb-num">2</span> {t("publish.step2")}
              {user ? (
                <span className="pb-user">
                  <CheckCircle2 size={12} /> {user.name} · {user.email}
                </span>
              ) : (
                <div className="pb-auth">
                  <div className="pb-auth-tabs">
                    <button
                      className={"btn sm" + (mode === "login" ? " primary" : "")}
                      onClick={() => setMode("login")}
                    >
                      <LogIn size={11} /> {t("publish.login")}
                    </button>
                    <button
                      className={"btn sm" + (mode === "signup" ? " primary" : "")}
                      onClick={() => setMode("signup")}
                    >
                      <UserPlus size={11} /> {t("publish.signup")}
                    </button>
                  </div>
                  {mode === "signup" && (
                    <input
                      className="pb-input"
                      placeholder={t("publish.namePh")}
                      value={name}
                      onChange={(e) => setName(e.target.value)}
                    />
                  )}
                  <input
                    className="pb-input"
                    placeholder={t("publish.emailPh")}
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                  />
                  <input
                    className="pb-input"
                    type="password"
                    placeholder={t("publish.passPh")}
                    value={pass}
                    onChange={(e) => setPass(e.target.value)}
                  />
                  <button className="btn sm primary" onClick={() => void doAuth()}>
                    {mode === "signup" ? t("publish.createAccount") : t("publish.enter")}
                  </button>
                </div>
              )}
            </div>
          </section>

          {/* 3 · PUJADA */}
          <section className="pb-step">
            <div className="pb-step-head">
              <span className="pb-num">3</span> {t("publish.step3")}
            </div>
            <div className="pb-site">
              <input
                className="pb-input wide"
                placeholder={t("publish.sitePh")}
                value={site}
                onChange={(e) => setSite(e.target.value)}
              />
              <input
                className="pb-input"
                placeholder={t("publish.wpUserPh")}
                value={wpUser}
                onChange={(e) => setWpUser(e.target.value)}
              />
              <input
                className="pb-input"
                type="password"
                placeholder={t("publish.wpPassPh")}
                value={wpPass}
                onChange={(e) => setWpPass(e.target.value)}
              />
            </div>
            <div className="pb-note">
              <Lock size={11} /> {t("publish.notePre")}<b>{t("publish.noteBold")}</b>{t("publish.notePost")}
            </div>
            <div className="pb-actions">
              <button
                className="btn sm"
                disabled={busy !== null}
                onClick={() => void openLogin()}
              >
                <LogIn size={12} /> {t("publish.openLogin")}
              </button>
              <button
                className="btn primary sm"
                disabled={busy !== null || !zip}
                onClick={() => void upload()}
              >
                {busy === "upload" ? (
                  <Loader2 size={12} className="spin" />
                ) : (
                  <CloudUpload size={12} />
                )}
                {busy === "upload" ? t("publish.uploading") : t("publish.uploadZip")}
              </button>
              <button className="btn sm" onClick={() => void openSales()}>
                {t("publish.openSales")}
              </button>
            </div>
          </section>

          {msg && (
            <div className="pb-msg ok">
              <CheckCircle2 size={13} /> {msg}
            </div>
          )}
          {err && (
            <div className="pb-msg err">
              <AlertTriangle size={13} /> {err}
            </div>
          )}

          <div className="pb-credit">
            {t("publish.creditMadeBy")} <b>Joan Puigbert Carreras</b> · {t("publish.creditProject")}{" "}
            <b>Thepuigberts.com</b>
          </div>
        </div>
      </div>
    </div>
  );
}
