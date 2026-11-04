/// CERCA DE SKILLS — punt de partida amb repositoris coneguts i certesa
/// («catàleg») més recerca en viu a GitHub.
export interface SkillSearchResult {
  repo: string;
  name: string;
  description: string;
  stars?: number;
  source: "catalog" | "github";
}

/// Repositoris amb skills d'agent en format SKILL.md (compatible amb
/// NoOrbit, Claude Code i OpenCode). Escull-ne una i prem «Instal·la».
export const SKILL_CATALOG: SkillSearchResult[] = [
  {
    repo: "anthropics/skills",
    name: "Skills oficials (PDF, XLSX, DOCX, PPTX)",
    description:
      "La col·lecció oficial: llegir i generar documents, fulls de càlcul i presentacions.",
    source: "catalog",
  },
  {
    repo: "obra/superpowers",
    name: "Superpowers (col·lecció popular)",
    description:
      "Depuració sistemàtica, TDD, planificació de codi, revisió de codi i més bons costums per als agents.",
    source: "catalog",
  },
  {
    repo: "anthropics/claude-code",
    name: "Recursos format Agent Skills",
    description:
      "Exemples i docs de referència sobre el format Agent Skills (SKILL.md).",
    source: "catalog",
  },
  {
    repo: "docker/awesome-compose",
    name: "Exemples de Compose",
    description:
      "Piles docker-compose ja fetes (frontend + backend + bbdd) per començar projectes amb les eines a punt.",
    source: "catalog",
  },
];

/// Recerca de repos de skills a GitHub (API pública, sense token):
/// cerca repositoris motiu del terme posat. Si la xarxa no va, la UI cau
/// al catàleg de dalt.
export async function searchSkillsGitHub(query: string): Promise<SkillSearchResult[]> {
  const q = encodeURIComponent(`${query.trim()} skill agent SKILL.md`);
  const res = await fetch(
    `https://api.github.com/search/repositories?q=${q}&sort=stars&per_page=10`,
    { headers: { Accept: "application/vnd.github+json" } },
  );
  if (!res.ok) throw new Error(`GitHub ${res.status}`);
  const data = await res.json();
  return (data.items ?? []).map((r: any) => ({
    repo: r.full_name,
    name: r.full_name,
    description: r.description ?? "",
    stars: r.stargazers_count ?? 0,
    source: "github" as const,
  }));
}
