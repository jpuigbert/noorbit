import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Sparkles, Terminal, Zap, Server, Check, ChevronRight, Bot,
} from "lucide-react";
import { useT } from "../i18n";
import { useUIStore } from "../stores/uiStore";
import { ArtWelcome } from "./Illustrations";

/// Assistent de 3 passos que surt UNA sola vegada (primera arrencada):
///   1) IA amb la qual treballar  2) tema  3) resum + començar.
/// Pensat perquè un usuari NOU entengui el valor en 30 segons: sense compte,
/// sense clau (Ollama local si està instal·lat), i amb les vies avançades a
/// l'abast (núvol amb la seua clau, servidor remot amb la seua GPU).
const LS_DONE = "noorbit.onboardingDone";

export function shouldShowOnboarding(): boolean {
  try {
    return localStorage.getItem(LS_DONE) !== "1";
  } catch {
    return false; // sense persistència, no el fem ressorgir cada cop
  }
}

export default function OnboardingModal() {
  const { t } = useT();

  const [step, setStep] = useState(0);
  const [choice, setChoice] = useState<null | "ollama" | "cloud" | "remote">(null);
  const [theme, setTheme] = useState<"dark" | "light" | "gray" | "gold">(() =>
    useUIStore.getState().theme as "dark" | "light" | "gray" | "gold"
  );
  const [checking, setChecking] = useState(false);
  const [ollamaOk, setOllamaOk] = useState<boolean | null>(null);
  const [remote, setRemote] = useState("");
  const [remoteResult, setRemoteResult] = useState<string | null>(null);

  const finish = () => {
    try {
      localStorage.setItem(LS_DONE, "1");
    } catch {
      /* sense persistència: tornaria a sortir; no és greu */
    }
    if (useUIStore.getState().theme !== theme) {
      useUIStore.setState({ theme });
      window.dispatchEvent(new CustomEvent("noorbit:theme", { detail: theme }));
    }
    useUIStore.setState({ showOnboarding: false });
  };

  /** Comprova Ollama local (breu) per dir si ja el tens llistat al pas 1. */
  const checkOllama = async () => {
    setChecking(true);
    setOllamaOk(null);
    try {
      await invoke("ollama_list_models");
      setOllamaOk(true);
    } catch {
      setOllamaOk(false);
    } finally {
      setChecking(false);
    }
  };

  /** Prova la URL d'un servei OpenAI-compatible (núvol o servidor remot). */
  const checkRemote = async () => {
    setRemoteResult(null);
    try {
      const c = await invoke<{ ok: boolean; title: string }>("ai_endpoint_check", {
        baseUrl: remote.trim(),
        model: "",
        token: null,
        kind: "openai",
      });
      setRemoteResult(`${c.ok ? "✓" : "✗"} ${c.title}`);
    } catch (e) {
      setRemoteResult(`✗ ${String(e)}`);
    }
  };

  const steps: string[] = [t("onb.stepModel"), t("onb.stepTheme"), t("onb.stepReady")];

  return (
    <div className="modal-overlay">
      <div className="modal onboarding" style={{ maxWidth: 620 }}>
        <div className="modal-head">
          <span>
            <Sparkles size={15} style={{ verticalAlign: -2, marginRight: 6 }} />
            {t("onb.title")}
          </span>
          <button className="icon-btn" title={t("onb.skip")} onClick={finish}>
            ✕
          </button>
        </div>
        <div className="modal-body">
          <div className="onb-steps">
            {steps.map((s, i) => (
              <div key={s} className={"onb-step" + (i === step ? " active" : "")}>
                {i < step ? <Check size={12} /> : <span>{i + 1}</span>} {s}
                {i < steps.length - 1 && (
                  <ChevronRight size={12} style={{ opacity: 0.4, margin: "0 4px" }} />
                )}
              </div>
            ))}
          </div>

          {step === 0 && (
            <div>
              <p className="onb-lead">{t("onb.intro")}</p>
              <div className="onb-cards">
                <button
                  className={"onb-card" + (choice === "ollama" ? " picked" : "")}
                  onClick={() => {
                    setChoice("ollama");
                    void checkOllama();
                  }}
                >
                  <div className="onb-card-ic"><Terminal size={18} /></div>
                  <b>{t("onb.cardLocal")}</b>
                  <span>{t("onb.cardLocalDesc")}</span>
                  <em>
                    {checking && <Bot size={12} style={{ verticalAlign: -2 }} />}{" "}
                    {ollamaOk === true ? t("onb.localOk") : ""}
                    {ollamaOk === false ? t("onb.localNo") : ""}
                  </em>
                </button>
                <button className="onb-card" onClick={() => setChoice("cloud")}>
                  <div className="onb-card-ic"><Zap size={18} /></div>
                  <b>{t("onb.cardCloud")}</b>
                  <span>{t("onb.cardCloudDesc")}</span>
                  <em>{t("onb.cardCloudNote")}</em>
                </button>
                <button
                  className={"onb-card" + (choice === "remote" ? " picked" : "")}
                  onClick={() => setChoice("remote")}
                >
                  <div className="onb-card-ic"><Server size={18} /></div>
                  <b>{t("onb.cardRemote")}</b>
                  <span>{t("onb.cardRemoteDesc")}</span>
                  <em>{t("onb.advanced")}</em>
                </button>
              </div>
              {choice === "remote" && (
                <div className="onb-remote">
                  <input
                    style={{ flex: 1 }}
                    value={remote}
                    placeholder="http://192.168.1.20:8000/v1"
                    onChange={(e) => setRemote(e.target.value)}
                  />
                  <button className="btn sm" onClick={() => void checkRemote()}>
                    {t("onb.check")}
                  </button>
                  {remoteResult && <span className="empty-hint">{remoteResult}</span>}
                  <p className="empty-hint" style={{ margin: 0 }}>
                    {t("onb.remoteHint")}
                  </p>
                </div>
              )}
            </div>
          )}

          {step === 1 && (
            <div>
              <p className="onb-lead">{t("onb.themeLead")}</p>
              <div className="onb-themes">
                {(["dark", "light", "gray", "gold"] as const).map((th) => (
                  <button
                    key={th}
                    className={"onb-theme" + (theme === th ? " picked" : "")}
                    onClick={() => {
                      setTheme(th);
                      useUIStore.setState({ theme: th });
                      window.dispatchEvent(new CustomEvent("noorbit:theme", { detail: th }));
                    }}
                  >
                    <i className={"onb-swatch onb-" + th} />
                    {t(`onb.theme.${th}`)}
                  </button>
                ))}
              </div>
            </div>
          )}

          {step === 2 && (
            <div>
              <div className="onb-art"><ArtWelcome size={140} /></div>
              <p className="onb-lead">{t("onb.readyLead")}</p>
              <div className="onb-summary">
                <div>
                  <b>{t("onb.stepModel")}:</b>{" "}
                  {choice === "ollama"
                    ? t("onb.cardLocal")
                    : choice === "cloud"
                      ? t("onb.cardCloud")
                      : choice === "remote"
                        ? `${t("onb.cardRemote")} (${remote})`
                        : "—"}
                </div>
                <div>
                  <b>{t("onb.stepTheme")}:</b> {t(`onb.theme.${theme}`)}
                </div>
              </div>
              <p className="empty-hint">{t("onb.readyHint")}</p>
            </div>
          )}
        </div>
        <div className="modal-foot">
          <span style={{ flex: 1 }} />
          {step > 0 && (
            <button className="btn" onClick={() => setStep(step - 1)}>
              ← {t("onb.back")}
            </button>
          )}
          {step < 2 ? (
            <button
              className="btn primary"
              disabled={step === 0 && choice === null}
              onClick={() => setStep(step + 1)}
            >
              <ChevronRight size={13} /> {t("onb.next")}
            </button>
          ) : (
            <button className="btn primary" onClick={finish}>
              <Sparkles size={13} /> {t("onb.done")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
