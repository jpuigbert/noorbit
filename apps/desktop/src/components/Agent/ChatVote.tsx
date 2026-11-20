// VOT DEL XAT (👍 / 👎): el que fa que el registre d'aprenentatge serveixi
// per a alguna cosa.
//
// Una conversa sense vot no diu si la resposta anava bé. Amb el vot:
//  · 👍 esdevé una PROVA — la porta d'avaluació del cervell (`ai/brain_gate`)
//    la fa servir per desfer un aprenentatge que empitjora el que ja aprovaves,
//    i una fila de trenament del paquet de Soup (`.soup-loops/`);
//  · 👎 es descarta: no s'ensenya mai a les lliçons ni al dataset.
//
// El vot es desa al backend amb `learning_vote`, que busca el torn per la
// PREGUNTA (el xat no coneix l'`id` de l'intercanvi). Si el registre no el té
// —aprenentatge desactivat o torn ja fora del límit— ho diem amb un avís:
// no fem passar un vot no desat per vot desat.
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ThumbsDown, ThumbsUp } from "lucide-react";
import { useT } from "../../i18n";
import { toastError } from "../../stores/toastStore";

/// El que retorna `learning_vote` (camelCase, com el `VoteInfo` del backend).
interface VoteInfo {
  thumbsUp: number;
  thumbsDown: number;
}

interface Props {
  /// Pregunta del torn al qual pertany aquesta resposta.
  question: string;
  /// Vot ja marcat en aquest missatge: 1 👍, -1 👎, `null` cap.
  vote: number | null;
  /// Només s'anuncia quan el backend ho ha confirmat.
  onVoted: (vote: number | null) => void;
}

export default function ChatVote({ question, vote, onVoted }: Props) {
  const { t } = useT();
  const [busy, setBusy] = useState(false);

  // Sense pregunta no hi ha torn que votar (p. ex. una resposta brollada
  // d'un equip d'especialistes, no d'un missatge de l'usuari).
  if (!question.trim()) return null;

  const click = async (next: number) => {
    if (busy) return;
    // Tornar a prémer el mateix botó RETIRA el vot. Al feedback de Soup
    // s'escriu com a `value: null` —els registres són només-afegits i hi
    // guanya l'última línia—, no pas esborrant res.
    const value = vote === next ? 0 : next;
    setBusy(true);
    try {
      await invoke<VoteInfo>("learning_vote", { vote: value, question });
      onVoted(value === 0 ? null : value);
    } catch (e) {
      toastError(t("agent.voteFailed"), e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <span className="chat-vote" role="group" aria-label={t("agent.voteTitle")}>
      <span className="chat-vote-label">{t("agent.voteTitle")}</span>
      <button
        className={"btn sm ghost chat-vote-btn" + (vote === 1 ? " up" : "")}
        title={vote === 1 ? t("agent.voteUpOn") : t("agent.voteUpHint")}
        aria-pressed={vote === 1}
        disabled={busy}
        onClick={() => void click(1)}
      >
        <ThumbsUp size={11} />
      </button>
      <button
        className={"btn sm ghost chat-vote-btn" + (vote === -1 ? " down" : "")}
        title={vote === -1 ? t("agent.voteDownOn") : t("agent.voteDownHint")}
        aria-pressed={vote === -1}
        disabled={busy}
        onClick={() => void click(-1)}
      >
        <ThumbsDown size={11} />
      </button>
    </span>
  );
}
