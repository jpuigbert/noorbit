/// EL PENSAMENT DE LA IA, SEMPRE EN CATALÀ.
///
/// Un model local anglòfon pensa en anglès i l'usuari es perd «el procés».
/// Ara el raonament es detecta per marca lingüística i, si no és català,
/// es tradueix AUTOMÀTICAMENT amb UNA crida crua del proveïdor actiu
/// (`ai_provider_chat`: sense eines, sense events, un sol missatge). Si el
/// model ja pensa en català, no se'n fa cap: es mostra tal qual.
import { invoke } from "@tauri-apps/api/core";

const cache = new Map<string, string>();

export function cachedCat(thinking: string): string | null {
  return cache.get(thinking) ?? null;
}

/// Heurística ràpida (molt més barata que un traductor): el català i el
/// castellà comparteixen molt; les marques pròpies del català decideixen.
const CAT_MARKS = [
  " això", "també", "però", " és ", " està", " fitxer", " codi", " pel ", "al ",
  "d'un", "d'una", " així", " hem ", " fas ", " per ", " més ", " ara ",
  "hem de", "vull ", "necessit", " comprova", "escriu", "llegeix",
];
const EN_MARKS = [
  "the ", " and ", " to ", " of ", " with ", " you ", " for ", " is ", " will ",
  " need", " code ", " file ", " this ", " that ", " should ",
];

export function looksCatalan(thinking: string): boolean {
  const s = thinking.toLowerCase();
  if (s.length < 20) return true; // massa curt per fer-ne res
  const cat = CAT_MARKS.reduce((n, m) => n + (s.includes(m) ? 1 : 0), 0);
  const en = EN_MARKS.reduce((n, m) => n + (s.includes(m) ? 1 : 0), 0);
  return cat >= en;
}

/// Tradueix el raonament al català (crida crua; sense xat ni eines).
export async function translateToCatalan(thinking: string): Promise<string | null> {
  const hit = cache.get(thinking);
  if (hit) return hit;
  try {
    const provider = await invoke<string>("ai_current_provider");
    const raw = await invoke<string>("ai_provider_chat", {
      prompt:
        "Tradueix aquest raonament (procés de pensament d'una IA) al català.\n" +
        "Respon NOMÉS la traducció en text pla, mateixa estructura, sense " +
        "respinats ni res més:\n\n" +
        thinking.slice(0, 6000),
      system:
        "Ets un traductor català precís. Responds només amb la traducció al català, res més.",
    });
    let out = (raw ?? "").trim();
    // Si el model s'ha tirat de gola (capçaleres, preàmbuls), retall.
    if (out.toLowerCase().startsWith("traducció:")) { out = out.slice(10).trim(); }
    if (!out || out.length < 5) return null;
    cache.set(thinking, out);
    return out;
  } catch {
    return null; // sense traducció: es mostra l'original (vegeu toggle «CA»)
  }
}
