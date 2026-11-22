/// Especialistes com a fitxers Markdown declaratives (patró d'OpenCode i dels
/// subagents de Qoder): un fitxer `.md` amb capçalera YAML i el cos com a
/// instruccions de sistema. Així els experts es poden versionar al repo,
/// compartir entre equips i importar/descarregar sense pantalles extra.
///
/// Format:
///   ---
///   name: Revisor
///   description: Revisa el codi cercant regressions   (≡ «rol»)
///   model: llama3.1:8b                                 (opcional)
///   mode: subagent                                     (opcional)
///   ---
///   (aquí, el system prompt complet)
///
/// El parser és tolerant: sense capçalera, TOT el text és el system prompt i
/// el nom surt de la primera línia. Accepta `role` com a alias de
/// `description` i valors en català per al mode («xat»/«equip»).

export type ExpertMode = "primary" | "subagent" | "all";

export interface ExpertMarkdown {
  name: string;
  role: string;
  systemPrompt: string;
  model: string | null;
  mode: ExpertMode | null;
}

/// Nom d'arxiu net per a l'exportació (sense caràcters molestos).
export function expertFileName(name: string): string {
  const s = name
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return `${s || "especialista"}.md`;
}

export function expertToMarkdown(e: {
  name: string;
  role: string;
  systemPrompt: string;
  model?: string | null;
  mode?: string | null;
}): string {
  const head = ["---", `name: ${e.name}`, `description: ${e.role}`];
  if (e.model) head.push(`model: ${e.model}`);
  if (e.mode && e.mode !== "all") head.push(`mode: ${e.mode}`);
  head.push("---", "");
  return head.join("\n") + "\n" + e.systemPrompt.trim() + "\n";
}

function normMode(v: string): ExpertMode | null {
  const m = v.trim().toLowerCase();
  if (!m) return null;
  if (m === "primary" || m === "xat" || m === "cap" || m === "principal") return "primary";
  if (m === "subagent" || m === "subaltern" || m === "equip" || m === "worker") return "subagent";
  if (m === "all" || m === "tots" || m === "tots dos") return "all";
  return null;
}

/// Converteix el text d'un `.md` d'especialista en dades per crear-lo.
export function expertFromMarkdown(text: string): ExpertMarkdown {
  const clean = text.replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n");
  const out: ExpertMarkdown = { name: "", role: "", systemPrompt: "", model: null, mode: null };

  const lines = clean.split("\n");
  let bodyStart = 0;
  if (lines[0]?.trim() === "---") {
    const end = lines.findIndex((l, i) => i > 0 && l.trim() === "---");
    if (end > 0) {
      for (const l of lines.slice(1, end)) {
        const m = l.match(/^\s*([A-Za-z_][\w-]*)\s*:\s*(.*)$/);
        if (!m) continue; // estructures YAML complexes: no apliquen ací
        const key = m[1].toLowerCase();
        const val = m[2].trim().replace(/^["']|["']$/g, "");
        if (key === "name") out.name = val;
        else if (key === "description" || key === "role") out.role = val;
        else if (key === "model") out.model = val || null;
        else if (key === "mode") out.mode = normMode(val);
      }
      bodyStart = end + 1;
    }
  }
  out.systemPrompt = lines.slice(bodyStart).join("\n").trim();

  // Sense nom a la capçalera: primera línia del cos (retallada), o el nom del
  // fitxer és impossible ací — que ho decideixa qui crida la funció.
  if (!out.name) {
    const first = out.systemPrompt.split("\n")[0] ?? "";
    out.name = first.replace(/^#+\s*/, "").slice(0, 60).trim();
  }
  // Si el fitxer no porta ni nom ni cos no és un especialista: buida.
  if (!out.systemPrompt && !out.name) out.systemPrompt = clean.trim();
  return out;
}
