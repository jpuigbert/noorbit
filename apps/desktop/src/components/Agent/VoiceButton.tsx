//! Botó de VEUI del composer: dicta l'ordre i l'escriu al xat.
//!
//! Per què un component a part? Perquè la gravació té el seu propi cicle de
//! vida (permís, temporitzador, MediaRecorder, transcripció) i el panell del
//! xat ja és prou gran. Ací es queda tot tancat: el panell només rep el text
//! final, com si l'usuari l'haguera teclejat.
//!
//! Flux: clic → demana el micròfon → grava → segon clic (o 2 min) → envia
//! l'àudio a `voice_transcribe` (motor del sistema / whisper / proveïdor) →
//! el text s'afig al composer perquè es puga revisar abans d'enviar.
//! «Esc» mentres es grava ho cancel·la sense transcriure res.

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Loader2, Mic, Square } from "lucide-react";
import { useT } from "../../i18n";
import { toastError, toastInfo } from "../../stores/toastStore";

interface VoiceEngine {
  id: string;
  label: string;
  ready: boolean;
  detail: string;
}

interface VoiceStatus {
  engines: VoiceEngine[];
  locale: string;
  chosen: string | null;
}

interface Transcript {
  text: string;
  engine: string;
}

/// Formats que sap gravar el WebView, del més compatible amb els motors de
/// transcripció al menys. `audio/mp4` el llig el reconeixement de macOS.
const MIMES = ["audio/mp4", "audio/webm;codecs=opus", "audio/webm", "audio/ogg;codecs=opus"];

/// Tope de seguretat: dos minuts ja són una ordre llarga; més seria un fitxer
/// enorme i una espera incòmoda.
const MAX_SECONDS = 120;

function pickMime(): string {
  if (typeof MediaRecorder === "undefined") return "";
  return MIMES.find((m) => MediaRecorder.isTypeSupported(m)) ?? "";
}

/// Codi d'idioma de la UI → variant que entenen els motors de veu.
function speechLocale(code: string): string {
  const map: Record<string, string> = {
    ca: "ca-ES",
    es: "es-ES",
    en: "en-US",
    fr: "fr-FR",
    de: "de-DE",
    it: "it-IT",
    pt: "pt-PT",
    gl: "gl-ES",
    eu: "eu-ES",
  };
  return map[code] ?? code;
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const raw = String(reader.result ?? "");
      resolve(raw.includes("base64,") ? raw.split("base64,")[1] : raw);
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

function mmss(total: number): string {
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

/// Nom llegible del motor que ha reconegut la veu (per a l'avís).
function engineLabel(engine: string): string {
  if (engine === "macos") return "Veu del sistema";
  if (engine === "whisper") return "whisper.cpp";
  if (engine.startsWith("openai:")) return engine.slice("openai:".length);
  return engine;
}

interface Props {
  /// Rep el text reconegut (i el motor) perquè el panell l'afija al composer.
  onText: (text: string, engine: string) => void;
  /// El xat està treballant: no es dicta mentres la IA respon.
  disabled?: boolean;
}

export default function VoiceButton({ onText, disabled }: Props) {
  const { t, locale } = useT();
  const [status, setStatus] = useState<VoiceStatus | null>(null);
  const [phase, setPhase] = useState<"idle" | "recording" | "working">("idle");
  const [seconds, setSeconds] = useState(0);
  const recRef = useRef<MediaRecorder | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const cancelledRef = useRef(false);
  const startedRef = useRef(0);

  const speechLang = speechLocale(locale);

  // Estat dels motors (tooltip): es refresca en muntar i en acabar cada dictat.
  const refreshStatus = () => {
    void invoke<VoiceStatus>("voice_status", { locale: speechLang })
      .then(setStatus)
      .catch(() => setStatus(null));
  };
  useEffect(() => {
    refreshStatus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [speechLang]);

  // Cronòmetre mentres es grava (250 ms: el comptador no ha de fer salts).
  useEffect(() => {
    if (phase !== "recording") return;
    const id = window.setInterval(() => {
      const elapsed = Math.floor((Date.now() - startedRef.current) / 1000);
      setSeconds(elapsed);
      if (elapsed >= MAX_SECONDS) stop();
    }, 250);
    return () => window.clearInterval(id);
  }, [phase]);

  // «Esc» cancel·la la gravació (no es transcriu res).
  useEffect(() => {
    if (phase !== "recording") return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        cancelledRef.current = true;
        stop();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [phase]);

  // Si el panell es desmunta (canvi de pestanya, xat tancat…), s'atura el
  // micròfon: mai no ha de quedar gravant en segon pla.
  useEffect(() => {
    return () => {
      cancelledRef.current = true;
      if (recRef.current && recRef.current.state === "recording") recRef.current.stop();
      streamRef.current?.getTracks().forEach((tr) => tr.stop());
    };
  }, []);

  const cleanup = () => {
    streamRef.current?.getTracks().forEach((tr) => tr.stop());
    streamRef.current = null;
    recRef.current = null;
    chunksRef.current = [];
  };

  function stop() {
    const rec = recRef.current;
    if (rec && rec.state !== "inactive") rec.stop();
  }

  const start = async () => {
    if (typeof MediaRecorder === "undefined") {
      toastError(t("agent.voiceUnsupported"));
      return;
    }
    if (!navigator.mediaDevices?.getUserMedia) {
      toastError(t("agent.voiceUnsupported"));
      return;
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
      });
      streamRef.current = stream;
      const mime = pickMime();
      const rec = new MediaRecorder(stream, mime ? { mimeType: mime } : undefined);
      recRef.current = rec;
      chunksRef.current = [];
      cancelledRef.current = false;

      rec.ondataavailable = (e) => {
        if (e.data && e.data.size > 0) chunksRef.current.push(e.data);
      };
      rec.onstop = () => {
        const chunks = chunksRef.current;
        const type = rec.mimeType || mime || "audio/mp4";
        cleanup();
        if (cancelledRef.current) {
          setPhase("idle");
          setSeconds(0);
          return;
        }
        void finish(chunks, type);
      };

      rec.start(500);
      startedRef.current = Date.now();
      setSeconds(0);
      setPhase("recording");
    } catch (e) {
      cleanup();
      setPhase("idle");
      const name = (e as { name?: string })?.name ?? "";
      if (name === "NotAllowedError" || name === "SecurityError") {
        toastError(t("agent.voiceDenied"));
      } else if (name === "NotFoundError" || name === "OverconstrainedError") {
        toastError(t("agent.voiceNoMic"));
      } else {
        toastError(t("agent.voiceFailed"), e);
      }
    }
  };

  const finish = async (chunks: Blob[], type: string) => {
    if (chunks.length === 0) {
      setPhase("idle");
      toastInfo(t("agent.voiceEmpty"));
      return;
    }
    setPhase("working");
    try {
      const blob = new Blob(chunks, { type });
      // Alguns WebView diuen «audio/mp4;codecs=…»: el backend només vol el tipus.
      const mime = type.split(";")[0];
      const audioBase64 = await blobToBase64(blob);
      const res = await invoke<Transcript>("voice_transcribe", {
        audioBase64,
        mime,
        locale: speechLang,
      });
      const text = (res.text ?? "").trim();
      if (!text) {
        toastInfo(t("agent.voiceEmpty"));
      } else {
        onText(text, res.engine);
        toastInfo(t("agent.voiceAdded", { engine: engineLabel(res.engine) }));
      }
    } catch (e) {
      toastError(t("agent.voiceFailed"), e);
    } finally {
      setPhase("idle");
      setSeconds(0);
      refreshStatus();
    }
  };

  const toggle = () => {
    if (phase === "recording") {
      stop();
      return;
    }
    if (phase === "working" || disabled) return;
    void start();
  };

  const readyEngine = status?.engines.find((e) => e.ready);
  const title =
    phase === "recording"
      ? t("agent.voiceStopHint")
      : phase === "working"
      ? t("agent.voiceWorking")
      : readyEngine
      ? `${t("agent.voiceHint")}\n${readyEngine.label}: ${readyEngine.detail}`
      : `${t("agent.voiceHint")}\n${t("agent.voiceNoEngine")}`;

  return (
    <span className={"ap-voice" + (phase === "recording" ? " recording" : "")}>
      {phase === "recording" && (
        <span className="ap-voice-timer" aria-live="polite">
          {mmss(seconds)}
        </span>
      )}
      <button
        type="button"
        className={"icon-btn ap-voice-btn" + (phase === "recording" ? " recording" : "")}
        onClick={toggle}
        disabled={phase === "working" || (disabled && phase !== "recording")}
        title={title}
        aria-label={title}
        aria-pressed={phase === "recording"}
      >
        {phase === "working" ? (
          <Loader2 size={13} className="spin" />
        ) : phase === "recording" ? (
          <Square size={11} />
        ) : (
          <Mic size={13} />
        )}
      </button>
    </span>
  );
}
