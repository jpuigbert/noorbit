import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Upload, Zap, Plug, Loader2, BookOpen } from "lucide-react";
import { useT } from "../../i18n";
import {
  useProviderStore,
  type EndpointCheck,
  type AuthScheme,
} from "../../stores/providerStore";

/// Assistent «Servidor d'IA remot»: puja els fitxers del servei al servidor
/// extern per SFTP/FTPS/FTP (amb curl, via `ai_remote_deploy`), engega a
/// dins una API compatible amb OpenAI i connecta NoOrbit a ella. Així les
/// IAs treballen al servidor (GPU/SM100, DeepInfer, vLLM…) i NOMÉS les
/// respostes i els canvis tornen al Mac per HTTP: zero RAM d'inferència ací.
export default function RemoteAiServerPanel() {
  const { t } = useT();
  const add = useProviderStore((s) => s.add);
  const load = useProviderStore((s) => s.load);

  const [host, setHost] = useState("");
  const [user, setUser] = useState("");
  const [password, setPassword] = useState("");
  const [protocol, setProtocol] = useState<"sftp" | "ftps" | "ftp">("sftp");
  const [port, setPort] = useState(""); // port del SSH/FTP (buit = 22/21)
  const [remoteDir, setRemoteDir] = useState("/opt/noorbit-ia");
  const [apiPort, setApiPort] = useState("8000");
  const [backend, setBackend] = useState<"deepinfer" | "vllm">("deepinfer");
  const [model, setModel] = useState(""); // buit = model de rigor del backend
  const [busy, setBusy] = useState<"" | "deploy" | "check" | "connect">("");
  const [report, setReport] = useState("");
  const [reportOk, setReportOk] = useState(false);
  const [check, setCheck] = useState<EndpointCheck | null>(null);
  // Ajuda «pas a pas» per a gent que mai ha connectat un servidor desat.
  const [tut, setTut] = useState(false);

  const hostTrim = host.trim();
  const apiPortNum = Number(apiPort) > 0 && Number(apiPort) <= 65535 ? Number(apiPort) : 8000;
  const apiBase = `http://${hostTrim}:${apiPortNum}/v1`;

  /// Pujar els dos fitxers del servidor per FTP/SFTP.
  const deploy = async () => {
    setBusy("deploy");
    setReport("");
    setReportOk(false);
    try {
      const msg = await invoke<string>("ai_remote_deploy", {
        spec: {
          host: hostTrim,
          protocol,
          port: port.trim() ? Number(port) : null,
          user: user.trim(),
          password,
          remoteDir: remoteDir.trim() || "/opt/noorbit-ia",
          apiPort: apiPortNum,
          backend,
          model: model.trim() || null,
        },
      });
      setReport(msg);
      setReportOk(true);
    } catch (e) {
      setReport(String(e));
      setReportOk(false);
    } finally {
      setBusy("");
    }
  };

  /// Verificar la API del servidor (mateix diagnòstic que «Comprova»).
  const verify = async (): Promise<EndpointCheck | null> => {
    const result = await invoke<EndpointCheck>("ai_remote_verify", {
      host: hostTrim,
      apiPort: apiPortNum,
      model: model.trim() || null,
    });
    setCheck(result);
    return result;
  };

  const checkNow = async () => {
    setBusy("check");
    setCheck(null);
    try {
      await verify();
    } catch (e) {
      setCheck({
        ok: false,
        stage: "connexio",
        title: "No s'ha pogut fer la verificació",
        detail: String(e),
        models: [],
      });
    } finally {
      setBusy("");
    }
  };

  /// Connecta = comprova i, si va bé, afegeix el proveïdor a NoOrbit per usar
  /// la IA del servidor (el chat, els especialistes i l'equip el veuran tots).
  const connect = async () => {
    setBusy("connect");
    setCheck(null);
    try {
      const result = await verify();
      if (result!.ok) {
        const chosen = model.trim() || result!.models[0] || "";
        await add({
          name: `Servidor IA · ${hostTrim}`,
          baseUrl: apiBase,
          model: chosen,
          kind: "openai",
          auth: "none" as AuthScheme,
          enabled: true,
        });
        await load();
        setReport(`[NoOrbit] ✔ Connectat! Proveïdor afegit (${apiBase}, model «${chosen}»).\nJa pots seleccionar-lo a «Proveïdor actiu» i la IA treballarà al servidor.`);
        setReportOk(true);
      }
    } catch (e) {
      setReport(String(e));
      setReportOk(false);
    } finally {
      setBusy("");
    }
  };

  const canDeploy = Boolean(hostTrim && user.trim());

  return (
    <div className="remote-ai">
      <h4>{t("remote.title")}</h4>
      <p className="empty-hint">{t("remote.desc")}</p>

      <div className="prov-form-2">
        <div className="field" style={{ flex: 2 }}>
          <label>{t("remote.host")}</label>
          <input
            value={host}
            onChange={(e) => setHost(e.target.value)}
            placeholder={t("remote.hostHint")}
          />
        </div>
        <div className="field">
          <label>{t("remote.protocol")}</label>
          <select
            className="lp-select"
            value={protocol}
            onChange={(e) => setProtocol(e.target.value as "sftp" | "ftps" | "ftp")}
          >
            <option value="sftp">SFTP (22, xifrat)</option>
            <option value="ftps">FTPS (xifrat)</option>
            <option value="ftp">FTP</option>
          </select>
        </div>
        <div className="field" style={{ flex: 1 }}>
          <label>{t("remote.port")}</label>
          <input
            value={port}
            onChange={(e) => setPort(e.target.value)}
            placeholder={protocol === "ftp" ? "21" : "22"}
          />
        </div>
      </div>

      <div className="prov-form-2">
        <div className="field">
          <label>{t("remote.user")}</label>
          <input value={user} onChange={(e) => setUser(e.target.value)} placeholder="root" />
        </div>
        <div className="field">
          <label>{t("remote.password")}</label>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder={t("remote.passwordHint")}
          />
        </div>
      </div>
      {protocol === "ftp" && <p className="empty-hint">⚠ {t("remote.ftpWarn")}</p>}

      <div className="prov-form-2">
        <div className="field" style={{ flex: 2 }}>
          <label>{t("remote.remoteDir")}</label>
          <input value={remoteDir} onChange={(e) => setRemoteDir(e.target.value)} />
        </div>
        <div className="field">
          <label>{t("remote.apiPort")}</label>
          <input
            value={apiPort}
            onChange={(e) => setApiPort(e.target.value.replace(/[^0-9]/g, ""))}
            placeholder="8000"
          />
        </div>
      </div>

      <div className="prov-form-2">
        <div className="field">
          <label>{t("remote.backend")}</label>
          <select
            className="lp-select"
            value={backend}
            onChange={(e) => setBackend(e.target.value as "deepinfer" | "vllm")}
          >
            <option value="deepinfer">DeepInfer (DeepSeek, SM100/Ascend)</option>
            <option value="vllm">vLLM (servei OpenAI natiu)</option>
          </select>
        </div>
        <div className="field" style={{ flex: 2 }}>
          <label>{t("remote.model")}</label>
          <input
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder={backend === "deepinfer"
              ? "deepseek-ai/DeepSeek-V4.1-Flash-1017"
              : "meta-llama/Llama-3.1-8B-Instruct"}
          />
        </div>
      </div>

      <div className="prov-form-actions">
        <button className="btn sm" disabled={!canDeploy || busy !== ""} onClick={() => void deploy()}>
          {busy === "deploy" ? <Loader2 size={13} className="spin" /> : <Upload size={13} />}
          {busy === "deploy" ? t("remote.deploying") : t("remote.deploy")}
        </button>
        <button className="btn sm" disabled={!hostTrim || busy !== ""} onClick={() => void checkNow()}>
          {busy === "check" ? <Loader2 size={13} className="spin" /> : <Zap size={13} />}
          {t("remote.check")}
        </button>
        <button
          className="btn primary sm"
          disabled={!hostTrim || busy !== ""}
          onClick={() => void connect()}
          title={t("remote.connectHint")}
        >
          {busy === "connect" ? <Loader2 size={13} className="spin" /> : <Plug size={13} />}
          {t("remote.connect")}
        </button>
      </div>

      {/* Resultat de la pujada: passos fets + el que queda per SSH (amb el
          motiu exacte quan alguna cosa falla). */}
      {report && (
        <div
          className={"prov-test " + (reportOk ? "ok" : "fail")}
          style={{ whiteSpace: "pre-wrap" }}
        >
          {report}
        </div>
      )}

      <div className="remote-tut">
        <button className="btn sm" onClick={() => setTut(!tut)}>
          <BookOpen size={12} />
          {tut ? t("remote.tutHide") : t("remote.tutShow")}
        </button>

        {tut && (
          <ol className="remote-tut-steps">
            {["tut1", "tut2", "tut3", "tut4", "tut5", "tut6", "tut7"].map((k) => (
              <li key={k}>
                {k === "tut4" ? (
                  <>
                    {t("remote.tut4a")}
                    <code className="remote-tut-cmd">ssh {user.trim() || "usuari"}@{hostTrim || "EL-MEU-SERVIDOR"}</code>
                    {t("remote.tut4b")}
                  </>
                ) : (
                  t(`remote.${k}`)
                )}
              </li>
            ))}
          </ol>
        )}
        {tut && (
          <p className="empty-hint" style={{ marginTop: 4 }}>
            {t("remote.tutNote")}
          </p>
        )}
      </div>

      {/* Resultat del diagnòstic (models clicables: un clic els posa al camp) */}
      {check && (
        <div
          className={"prov-test " + (check.ok ? "ok" : "fail")}
          style={{ marginTop: 8, whiteSpace: "pre-wrap" }}
        >
          {check.ok ? "✓ " : "✗ "}
          <b>{check.title}</b>
          {"\n"}
          {check.detail}
          {check.models.length > 0 && (
            <div className="prov-chips">
              {check.models.slice(0, 16).map((m) => (
                <button key={m} className="btn sm" onClick={() => setModel(m)} title={t("remote.useModel")}>
                  {m}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
