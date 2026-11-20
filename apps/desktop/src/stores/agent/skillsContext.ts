import { invoke } from "@tauri-apps/api/core";
import type { Skill } from "../skillStore";

/// Catàleg compacte de skills per al system prompt de la IA.
///
/// Les skills instal·lades (incloses les d'ECC) només serveixen si la IA sap
/// que existeixen: ací se li passa un índex curt (nom + descripció retallada +
/// ruta del SKILL.md) perquè, quan la tasca encaixi amb una skill, la lllege
/// amb «read_file» i la seguisca. És «progressive disclosure»: l'índex sempre
/// hi és, el contingut complet només es carrega quan cal.
///
/// Límit de mida: amb centenars de skills (ECC en porta 328) l'índex pot ser
/// enorme; es retalla a `maxChars` perquè els models locals petits no es
/// queden sense context.
export function buildSkillsCatalog(skills: Skill[], maxChars = 6000): string {
  if (skills.length === 0) return "";
  const head =
    "\n\nSKILLS INSTAL·LADES: tens instruccions especialitzades (skills). " +
    "Quan la tasca encaixi amb una skill, llegeix el seu SKILL.md (ruta indicada) " +
    "i segueix-lo. Índex:\n";
  const lines: string[] = [];
  let used = head.length;
  let tallades = 0;
  for (const s of skills) {
    const desc = s.description.length > 70 ? s.description.slice(0, 70) + "…" : s.description;
    const line = `- ${s.name}${desc ? ` — ${desc}` : ""} (${s.path})\n`;
    if (used + line.length > maxChars) {
      tallades = skills.length - lines.length;
      break;
    }
    lines.push(line);
    used += line.length;
  }
  let out = head + lines.join("");
  if (tallades > 0) {
    out += `… i ${tallades} skills més (l'usuari les veu al panell Skills i connectors).\n`;
  }
  return out;
}

/// Demana la llista de skills al backend i en construeix l'índex. Si el
/// comandament falla (p. ex. proves sense Tauri), retorna cadena buida: el
/// xat continua igual, simplement sense índex de skills.
export async function skillsCatalogContext(): Promise<string> {
  try {
    const skills = await invoke<Skill[]>("list_skills");
    return buildSkillsCatalog(skills);
  } catch {
    return "";
  }
}