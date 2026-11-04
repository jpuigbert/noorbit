import { useEffect, useState } from "react";
import {
  X,
  UserPlus,
  LogIn,
  User,
  Mail,
  Lock,
  LogOut,
  Trash2,
  Pencil,
  Check,
  Users,
  Cpu,
} from "lucide-react";
import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";
import { useAuthStore, avatarColor, initialOf } from "../../stores/authStore";

/// Finestra de COMPTE de NoOrbit (estil dels IDEs amb compte, p. ex. Qoder):
///
/// - Sense sessió: fitxes «Crea el compte» / «Inicia la sessió» amb
///   validació i missatges d'error catalans.
/// - Amb sessió: fitxa del perfil (avatar, nom, correu), la secció
///   «IAs connectades» (reservada per al futur: quan el compte s'enganxi
///   amb serveis d'IA, serà el lloc on es gestionin), canviar nom/correu,
///   contrasenya, i la gestió de COMPTES DEL DISPOSITIU (entrar amb un
///   altre compte, tancar la sessió o esborrar un compte amb la clau).
///
/// Tot és opcional: sense compte, NoOrbit funciona exactament igual (les
/// IAs locals no depenen d'això).
type Pane = "signedOut" | "signedIn";
type Mode = "signup" | "login";

export default function AccountModal() {
  const show = useUIStore((s) => s.showAccount);
  const setShow = useUIStore((s) => s.setShowAccount);
  const { t } = useT();
  const user = useAuthStore((s) => s.user);
  const accounts = useAuthStore((s) => s.accounts);

  // Vista visible: per defecte depèn de la sessió; el botó d'un altre compte
  // força temporalment la d'entrada (amb el correu ja posat).
  const [forcePane, setForcePane] = useState<Pane | null>(null);
  const pane: Pane = forcePane ?? (user ? "signedIn" : "signedOut");

  const [mode, setMode] = useState<Mode>("signup");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [pass, setPass] = useState("");
  const [pass2, setPass2] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Edició de perfil
  const [editing, setEditing] = useState(false);
  const [nName, setNName] = useState("");
  const [nEmail, setNEmail] = useState("");
  const [nPass, setNPass] = useState("");

  // Esborrar compte (cal la contrasenya: res es fàcil d'eliminar per error)
  const [deleting, setDeleting] = useState(false);
  const [delPass, setDelPass] = useState("");
  const [delId, setDelId] = useState<string | null>(null);

  useEffect(() => {
    if (show) {
      setError(null);
      setEditing(false);
      setDeleting(false);
      setMode(user ? "login" : "signup");
      setNName(user?.name ?? "");
      setNEmail(user?.email ?? "");
      setForcePane(null);
    }
  }, [show, user]);

  if (!show) return null;

  const close = () => {
    setShow(false);
    setName("");
    setEmail("");
    setPass("");
    setPass2("");
    setError(null);
    setForcePane(null);
  };

  const errKeyToText = (key: string): string => t(key as never);

  const submit = async () => {
    if (busy) return;
    setError(null);
    if (mode === "signup" && pass !== pass2) {
      setError(errKeyToText("account.errors.passMatch"));
      return;
    }
    setBusy(true);
    try {
      const store = useAuthStore.getState();
      if (mode === "signup") {
        await store.signUp(name, email, pass);
      } else {
        await store.logIn(email, pass);
      }
      setForcePane(null);
      close();
    } catch (e) {
      setError(errKeyToText(String(e)));
    } finally {
      setBusy(false);
    }
  };

  const saveProfile = async () => {
    if (busy) return;
    setError(null);
    setBusy(true);
    try {
      await useAuthStore
        .getState()
        .updateProfile({ name: nName, email: nEmail }, nPass.trim() ? nPass : undefined);
      setEditing(false);
      setNPass("");
    } catch (e) {
      setError(errKeyToText(String(e)));
    } finally {
      setBusy(false);
    }
  };

  const confirmDelete = async () => {
    if (!delId || busy) return;
    setBusy(true);
    setError(null);
    try {
      await useAuthStore.getState().deleteAccount(delId, delPass);
      setDeleting(false);
      setDelId(null);
      setDelPass("");
    } catch (e) {
      setError(errKeyToText(String(e)));
    } finally {
      setBusy(false);
    }
  };

  const fieldLabelStyle: React.CSSProperties = { display: "block", marginBottom: 4, fontSize: 12, color: "var(--text-2)" };
  const inputStyle: React.CSSProperties = {
    width: "100%",
    boxSizing: "border-box",
    padding: "6px 8px",
    background: "var(--bg-1)",
    border: "1px solid var(--border)",
    borderRadius: 4,
    color: "var(--text-1)",
  };

  return (
    <div className="modal-overlay" onClick={close}>
      <div className="modal" style={{ width: 380 }} onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span>
            <User size={15} style={{ marginRight: 6, verticalAlign: -2 }} />
            {t("account.title")}
          </span>
          <button className="icon-btn" onClick={close}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body">
          {error && (
            <div style={{ color: "var(--error)", fontSize: 12, marginBottom: 10 }}>{error}</div>
          )}

          {pane === "signedOut" && (
            <>
              {/* Fitxes d'entrada, com els IDEs amb compte */}
              <div style={{ display: "flex", gap: 6, marginBottom: 12 }}>
                <button
                  className={"btn sm " + (mode === "signup" ? "primary" : "")}
                  onClick={() => {
                    setMode("signup");
                    setError(null);
                  }}
                >
                  <UserPlus size={12} /> {t("account.signup")}
                </button>
                <button
                  className={"btn sm " + (mode === "login" ? "primary" : "")}
                  onClick={() => {
                    setMode("login");
                    setError(null);
                  }}
                >
                  <LogIn size={12} /> {t("account.login")}
                </button>
              </div>

              {mode === "signup" && (
                <div className="field">
                  <label style={fieldLabelStyle}>{t("account.name")}</label>
                  <input
                    autoFocus
                    style={inputStyle}
                    placeholder={t("account.namePh")}
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                  />
                </div>
              )}
              <div className="field">
                <label style={fieldLabelStyle}>{t("account.email")}</label>
                <input
                  type="email"
                  autoFocus={mode === "login"}
                  style={inputStyle}
                  placeholder={t("account.emailPh")}
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && mode === "login" && void submit()}
                />
              </div>
              <div className="field">
                <label style={fieldLabelStyle}>{t("account.pass")}</label>
                <input
                  type="password"
                  style={inputStyle}
                  placeholder={t("account.passPh")}
                  value={pass}
                  onChange={(e) => setPass(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && mode === "login" && void submit()}
                />
              </div>
              {mode === "signup" && (
                <div className="field">
                  <label style={fieldLabelStyle}>{t("account.pass2")}</label>
                  <input
                    type="password"
                    style={inputStyle}
                    value={pass2}
                    onChange={(e) => setPass2(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && void submit()}
                  />
                  <p style={{ fontSize: 11, color: "var(--text-3)", margin: "6px 0 0" }}>
                    {t("account.intro")}
                  </p>
                </div>
              )}
              <button className="btn primary" style={{ width: "100%", marginTop: 10 }} disabled={busy} onClick={submit}>
                {mode === "signup" ? (
                  <>
                    <UserPlus size={13} /> {t("account.createBtn")}
                  </>
                ) : (
                  <>
                    <LogIn size={13} /> {t("account.loginBtn")}
                  </>
                )}
              </button>
            </>
          )}

          {pane === "signedIn" && user && (
            <>
              {/* Perfil connectat */}
              <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 12 }}>
                <div
                  style={{
                    width: 38,
                    height: 38,
                    borderRadius: "50%",
                    background: avatarColor(user.id),
                    color: "#fff",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    fontSize: 17,
                    fontWeight: 600,
                    flexShrink: 0,
                  }}
                >
                  {initialOf(user.name)}
                </div>
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontWeight: 600, fontSize: 14, color: "var(--text-1)" }}>{user.name}</div>
                  <div style={{ fontSize: 12, color: "var(--text-3)", overflow: "hidden", textOverflow: "ellipsis" }}>
                    {user.email}
                  </div>
                </div>
                <span style={{ flex: 1 }} />
                <button
                  className="icon-btn"
                  title={t("account.edit")}
                  onClick={() => {
                    setEditing(!editing);
                    setNName(user.name);
                    setNEmail(user.email);
                  }}
                >
                  <Pencil size={14} />
                </button>
              </div>

              {editing && (
                <div style={{ borderTop: "1px solid var(--border)", borderBottom: "1px solid var(--border)", padding: "10px 0", marginBottom: 10 }}>
                  <div className="field">
                    <label style={fieldLabelStyle}>{t("account.name")}</label>
                    <input style={inputStyle} value={nName} onChange={(e) => setNName(e.target.value)} />
                  </div>
                  <div className="field">
                    <label style={fieldLabelStyle}>{t("account.email")}</label>
                    <input style={inputStyle} value={nEmail} onChange={(e) => setNEmail(e.target.value)} />
                  </div>
                  <div className="field">
                    <label style={fieldLabelStyle}>{t("account.newPass")}</label>
                    <input
                      type="password"
                      style={inputStyle}
                      placeholder={t("account.newPassPh")}
                      value={nPass}
                      onChange={(e) => setNPass(e.target.value)}
                    />
                  </div>
                  <button className="btn primary sm" disabled={busy} onClick={saveProfile}>
                    <Check size={12} /> {t("account.saveBtn")}
                  </button>
                </div>
              )}

              {/* IAs connectades al compte: la secció queda SEMBRADA per al
                  futur. Quan el backend de comptes tingui serveis remots,
                  aquesta llista es farà gran sense canviar la UI. */}
              <div style={{ border: "1px solid var(--border)", borderRadius: 6, padding: "8px 10px", marginBottom: 12 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12, color: "var(--text-2)", marginBottom: 4 }}>
                  <Cpu size={13} /> {t("account.connections")}
                </div>
                {user.connections.length === 0 ? (
                  <div style={{ fontSize: 11, color: "var(--text-3)" }}>{t("account.connectionsEmpty")}</div>
                ) : (
                  <ul style={{ listStyle: "none", margin: 0, padding: 0 }}>
                    {user.connections.map((c) => (
                      <li key={c.id} style={{ fontSize: 12, color: "var(--text-2)" }}>
                        • {c.label} <span style={{ color: "var(--text-3)" }}>({c.kind})</span>
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              {/* Comptes d'aquest Mac: entrar amb un altre o esborrar-ne un */}
              {accounts.length > 1 && (
                <div style={{ borderTop: "1px solid var(--border)", paddingTop: 8, marginBottom: 10 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12, color: "var(--text-2)", marginBottom: 4 }}>
                    <Users size={13} /> {t("account.otherAccounts")}
                  </div>
                  <ul style={{ listStyle: "none", margin: 0, padding: 0 }}>
                    {accounts
                      .filter((a) => a.id !== user.id)
                      .map((a) => (
                        <li key={a.id} style={{ display: "flex", alignItems: "center", gap: 8, padding: "3px 0" }}>
                          <span style={{ fontSize: 12, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                            {initialOf(a.name)} · {a.email}
                          </span>
                          <button
                            className="btn sm"
                            onClick={() => {
                              // Força la vista d'entrada amb el correu ja posat:
                              // només cal posar la contrasenya d'aquell compte.
                              setMode("login");
                              setEmail(a.email);
                              setPass("");
                              setForcePane("signedOut");
                            }}
                            title={t("account.login")}
                          >
                            <LogIn size={11} />
                          </button>
                          {delId === a.id ? (
                            <span style={{ display: "flex", gap: 4, alignItems: "center" }}>
                              <input
                                type="password"
                                placeholder={t("account.pass")}
                                style={{ width: 110, fontSize: 11, padding: "2px 4px", background: "var(--bg-1)", border: "1px solid var(--border)", borderRadius: 3, color: "var(--text-1)" }}
                                value={delPass}
                                onChange={(e) => setDelPass(e.target.value)}
                                onKeyDown={(e) => e.key === "Enter" && void confirmDelete()}
                              />
                              <button className="btn danger sm" onClick={confirmDelete}>
                                <Trash2 size={11} />
                              </button>
                            </span>
                          ) : (
                            <button
                              className="icon-btn"
                              title={t("account.delete")}
                              onClick={() => {
                                setDelId(a.id);
                                setDelPass("");
                              }}
                            >
                              <Trash2 size={12} />
                            </button>
                          )}
                        </li>
                      ))}
                  </ul>
                </div>
              )}
            </>
          )}
        </div>
        <div className="modal-foot" style={{ borderTop: "1px solid var(--border)", display: "flex", justifyContent: "space-between" }}>
          <button className="btn sm" onClick={close}>
            <X size={12} /> {t("account.close")}
          </button>
          {user && (
            <button
              className="btn danger sm"
              onClick={() => {
                useAuthStore.getState().logOut();
                close();
              }}
            >
              <LogOut size={12} /> {t("account.logout")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
