# ECC de sèrie (vendoritzat)

Còpia local del coneixement d'**ECC** — <https://github.com/affaan-m/ECC>
(llicència MIT, vegeu `LICENSE`), inclosa a l'instal·lador de NoOrbit perquè
funcioni **sense xarxa i només instal·lant**:

- `skills/` — 329 skills en format estàndard Agent Skills (`SKILL.md`), amb
  les seves `references/` i `scripts/`. Inclou `ecc-conventions`, que al
  repositori original viu a `.agents/skills/`.
- `agents/` — 72 agents especialistes (`.md` amb frontmatter) que es
  converteixen en Especialistes de NoOrbit («ECC · nom»).
- `rules/common/` — 10 regles comunes que esdevenen skills a `ecc-rules/`.

En arrencar, NoOrbit sembra aquest arbre al directori de skills de l'usuari
(marcardor `ecc/.noorbit-ecc-sembrat`: només el primer cop, mai se sobreescriu
res que l'usuari haja editat o esborrat després). El botó «ECC» del panell
Skills torna a importar des d'aquesta còpia local quan existeix.

LIMITACIÓ HONESTA: els hooks, servidors MCP i AgentShield d'ECC són
específics d'altres harnessos (Claude Code…) i NO s'inclouen ni s'executen:
ací només hi ha el coneixement (skills, agents i regles).