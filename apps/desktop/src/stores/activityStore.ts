import { useMemo } from "react";
import { useAgentStore } from "./agentStore";
import { useComputerStore } from "./computerStore";
import { useAIStore } from "./aiStore";

export interface AIActivity {
  working: boolean;
  // clau i18n descriptiva del que s'està fent ara mateix
  labelKey: "status.workingAgent" | "status.workingComputer" | "status.workingPull" | "status.ready";
  detail: string | null; // per exemple, el model que es baixa
}

/// Agrega tots els indicadors de "la IA està treballant": l'agent multimodal,
/// el control de l'ordinador i la descàrrega de models.
export function useAIActivity(): AIActivity {
  // Ara hi ha diversos xats possibles; «treballant» si QUALSEVOL xat està generant.
  const agentRunning = useAgentStore((s) => s.sessions.some((x) => x.running));
  const computerBusy = useComputerStore((s) => s.busy || s.agentRunning);
  const pulling = useAIStore((s) => s.pulling);

  return useMemo(() => {
    const pullEntries = Object.entries(pulling);
    if (agentRunning) return { working: true, labelKey: "status.workingAgent", detail: null };
    if (computerBusy) return { working: true, labelKey: "status.workingComputer", detail: null };
    if (pullEntries.length > 0) {
      const [model, msg] = pullEntries[0];
      return { working: true, labelKey: "status.workingPull", detail: `${model}: ${msg}` };
    }
    return { working: false, labelKey: "status.ready", detail: null };
  }, [agentRunning, computerBusy, pulling]);
}
