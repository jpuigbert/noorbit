// ENTRADA NOMÉS PER A LA VISTA PRÈVIA AL NAVEGADOR (no s'inclou a l'app).
// Munta l'app real i força un estat de demostració per revisar el disseny del
// panell de l'Agent amb un projecte obert i context adjunt.
import "./main";
import { useWorkspaceStore } from "./stores/workspaceStore";
import { useAgentStore } from "./stores/agentStore";

try {
  localStorage.setItem("noorbit.onboardingDone", "1");
} catch {
  /* sense persistència */
}

setTimeout(() => {
  useWorkspaceStore.setState({ root: "/Users/appleone/programes/noorbit" });
  const state = useAgentStore.getState();
  const id = state.activeId;
  if (id) {
    useAgentStore.setState((s) => ({
      sessions: s.sessions.map((x) =>
        x.id === id
          ? {
              ...x,
              includeContext: true,
              contextInfo:
                "📁 Projecte complet a l'abast: 21 fitxers (tots a l'arbre). 19 amb contingut adjunt (9836 caràcters).",
            }
          : x,
      ),
    }));
  }
}, 300);
