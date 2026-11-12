/// Utilitats PURES del xat de l'Agent: historial, errors, identificadors i
/// preferències. Sense Tauri ni stores: es poden testejar sols (Vitest).

import type { ChatMessage, HistoryTurn } from "./types";

/// Extreu les rutes absolutes d'imatge (png/jpg/webp/gif/bmp) que apareixen
/// en un text: serveix perquè la IA (i l'usuari) vega les imatges que el
/// backend genera i desa a disc sense haver d'afegir cap format nou.
export function extractChatImagePaths(text: string): string[] {
  const re =
    /(\/[^\s"'`()<>]+?\.(?:png|jpe?g|webp|gif|bmp))|(?:[A-Za-z]:\\E[^\s"'`<>]+?\.(?:png|jpe?g|webp|gif|bmp))/gi;
  const found = text.match(re) ?? [];
  return Array.from(new Set(found));
}

export function newId(): string {
  return "x" + Date.now().toString(36) + Math.random().toString(36).slice(2, 6);
}

/// El raonament es mostra per defecte; l'usuari el pot amagar (preferència
/// global, es recorda entre sessions).
export function defaultShowThinking(): boolean {
  try {
    return localStorage.getItem("noorbit.showThinking") !== "0";
  } catch {
    return true;
  }
}

export function defaultCtx(): boolean {
  // PER NORMA: la IA de l'Agent treballa SEMPRE amb el codi real del projecte
  // obert. Abans es podia desactivar i es recordava entre sessions; això feia
  // que la IA responguera «a cegues» i no creara els fitxers correctes.
  // Si un projecte és enorme, el context es retalla sol (buildContext) i els
  // reintents per falta de memòria el redueixen encara més.
  return true;
}

/// La conversa precedent que acompanya cada missatge nou (per CONTINUAR el
/// treball i perquè un FORK herete el fil). S'acota per no desbordar la
/// memòria dels models: els últims torns, cadascun retallat. Finestra àmplia
/// (16 torns) perquè el xat RECORDERA les consultes anteriors i responga en
/// conseqüència; cada torn conserva 12.000 caràcters —abans 6.000— perquè un
/// fitxer llegit en un torn anterior no arribe MEITAT tallat al següent; el
/// backend encara compacta més si la RAM del model no abasta.
export function toHistory(messages: ChatMessage[]): HistoryTurn[] {
  return messages
    .filter((m) => m.text.trim() && !m.error)
    .slice(-16)
    .map((m) => ({ role: m.role, content: m.text.slice(0, 12000) }));
}

/// FIL MÍNIM per a l'últim recurs de memòria: abans el nivell 3 BORRAVA tota
/// la conversa — canviaves de IA, pujava el context, entrava el recurs… i el
/// model ja no sabia res: dir «continua» era inútil. Ara conserva els últims
/// 6 torns retallats: poc RAM, però el model SAB què s'estava fent.
export function threadOf(history: HistoryTurn[]): HistoryTurn[] {
  return history.slice(-6).map((t) => ({
    role: t.role,
    content: t.content.slice(0, 800),
  }));
}

/// Persistència dels xats oberts (conversa inclosa) a localStorage.
/// ABANS era un calaix global («noorbit.chats»): en obrir un projecte nou
/// apareixien els xats antics. Ara cada projecte té el seu propi calaix
/// («noorbit.chats::<arrel>») i, en canviar de projecte, es desen i es
/// carreguen els xats D'AQUELL projecte.
export const LS_KEY = "noorbit.chats";

/// Clau del projecte obert ara mateix; sense projecte es fa servir un calaix
/// propi (ja no és el clau global antic, que era compartit per a TODO).
export function chatLsKey(root: string | null): string {
  return root ? `${LS_KEY}::${root}` : `${LS_KEY}::__no_project__`;
}

/// Gestió dels errors: els de memòria d'Ollama es tradueixen a un missatge
/// clar en català; la cancel·lació no es mostra com a error.
export function isOOM(e: unknown): boolean {
  const s = String(e).toLowerCase();
  return (
    s.includes("out of memory") ||
    s.includes("oom") ||
    s.includes("could not allocate") ||
    s.includes("failed to allocate")
  );
}

export function isCancelled(e: unknown): boolean {
  return String(e).includes("cancel·lat");
}

/// El model ha tallat la descàrrega de la resposta (manca de memòria per a un
/// context gran o temps màxim exhaurit). El backend ja comprimeix el context i
/// reintenta; aquest missatge només es veu si també falla el segon intent.
export function isBodyCut(e: unknown): boolean {
  const s = String(e).toLowerCase();
  return (
    s.includes("error decoding response body") ||
    s.includes("stream tallat") ||
    s.includes("payload was not fully received")
  );
}

/// Errors que Justifiquen un reintent AUTOMÀTIC amb MENYS context: OOM dur
/// (el model no cap) o «body cut» (s'ha tallat a mitanda). En ambdós casos,
/// NoOrbit torna a intentar-ho retallant el context; si continua fallant,
/// ho fa Sense context; i com a últim recurs, Sense històrial.
export function isRetriableError(e: unknown): boolean {
  return isOOM(e) || isBodyCut(e);
}

export function toErrorText(e: unknown): string {
  if (isOOM(e))
    return "Memòria insuficient: el model és massa gran per a aquest ordinador. NoOrbit ho ha tornat a provar amb MENYS context i ha continuat fallant. Prova un model més lleuger (p. ex. :3b o quantitzat q4).";
  if (isBodyCut(e))
    return "La memòria no ha abastat tot el context i la resposta s'ha tallat. NoOrbit ho ha reintentat AUTOMÀTICAMENT alliberant RAM i COMPACTANT el context (resum dens que conserva l'objectiu, el pla i l'esquelet del codi) → sense codi → sense historial, i ha tornat a fallar. Desactiva «Inclou el codi» o usa un model més lleuger.";
  return String(e);
}

/// El model triat és PETIT (≤3,5B)? Aleshores el prompt i el codi adjunt
/// s'han de retallar o el model no té espai per respondre.
export function isSmallModel(model: string | null | undefined): boolean {
  const tag = (model ?? "").trim().toLowerCase().split(":").pop() ?? "";
  const m = tag.match(/^([\d.]+)\s*b$/);
  return m ? parseFloat(m[1]) <= 3.5 : false;
}