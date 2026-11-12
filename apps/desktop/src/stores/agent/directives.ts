/// Parsers PURS de les directrius d'agents i tasques: «@agent:» (crear
/// especialistes), «@use:» (delegar subtasques) i «@todo:/@done:/@doing:»
/// (checklist). L'APLICACIÓ (invoke, stores) es queda a agentStore; ací només
/// hi ha el parseig, que es pot testejar sol (Vitest).

// ── AGENTS ESPECIALISTES CREATS PER LA IA ───────────────────────────────────
// Mateix mecanisme que «@file:» o «@delete:»: la IA emet una directriu en una
// línia i NoOrbit l'executa de veritat. Ací la IA pot CREAR agents
// especialitzats (com subagents de Qoder) quan la tasca ho demane. Els agents
// es guarden com a «Especialistes» persistents (panell Especialistes), amb el
// seu rol i les seues instruccions, i l'usuari ja els podrà triar o posar en
// equip. Format tolerat (separador «|», també s'accepta «·»):
//   @agent: Nom | Rol curt | Instruccions de sistema (què ha de fer i com)
export function parseAgentDirectives(raw: string): { name: string; role: string; systemPrompt: string }[] {
  const out: { name: string; role: string; systemPrompt: string }[] = [];
  const seen = new Set<string>();
  for (const line of raw.split(/\r?\n/)) {
    const m = line.match(
      /^\s*@\s*(?:agent|subagent|expert|especialista)s?\s*:\s*(.+?)\s*$/i
    );
    if (!m) continue;
    const v = m[1].replace(/^[`'"\s[]+/, "").trim();
    if (!v) continue;
    // Separadors tolerats: un o dos pipes, «·», o tabulador.
    const parts = v
      .split(/\s*(?:\|{1,2}|·|\t)\s*/)
      .map((p) => p.trim())
      .filter((p) => p.length > 0);
    let name = "";
    let role = "";
    let systemPrompt = "";
    if (parts.length >= 3) {
      name = parts[0];
      role = parts[1];
      systemPrompt = parts.slice(2).join(" · ");
    } else if (parts.length === 2) {
      name = parts[0];
      role = parts[1];
      systemPrompt =
        `Ets un agent especialitzat («${name}»: ${role}). Quan rebs una tasca, ` +
        `treballa NOMÉS dins de la teva especialitat, resol-la amb codi real i ` +
        `complete (format «@file:») i explica els passos en català.`;
    } else {
      name = v.slice(0, 48);
      systemPrompt =
        `Ets un agent especialitzat anomenat «${v}». Ajuda l'usuari en aquesta ` +
        `tasca concreta amb codi real, verificable, i respostes en català.`;
    }
    name = name.slice(0, 60).trim();
    if (!name || seen.has(name.toLowerCase())) continue;
    seen.add(name.toLowerCase());
    out.push({
      name,
      role: role.slice(0, 120),
      systemPrompt: systemPrompt.trim() || `Agent especialitzat: ${name}.`,
    });
  }
  return out;
}

/// L'acció «Materialitzar» del xat reenvia el text del missatge als parsers de
/// directrius; si no treiem primer les línies «@agent:», prémer el botó tornaria
/// a crear els mateixos agents (el nom els deduplica, però mostraria un avís
/// «ja existien» innecessari). Aquest filtre els deixa fora sense tocar res més.
export function stripAgentDirectiveLines(raw: string): string {
  return raw
    .split(/\r?\n/)
    .filter(
      (l) =>
        !/^\s*@\s*(?:agent|subagent|expert|especialista)s?\s*:/i.test(l) &&
        !/^\s*@\s*(?:use|delega|delegate|todo|done|feta|pendient)\w*\s*:/i.test(l)
    )
    .join("\n");
}

export function parseUseDirectives(
  raw: string
): { agent: string; task: string }[] {
  const out: { agent: string; task: string }[] = [];
  const seen = new Set<string>();
  for (const line of raw.split(/\r?\n/)) {
    const m = line.match(/^\s*@\s*(?:use|delega(?:te)?)\s*:\s*(.+?)\s*$/i);
    if (!m) continue;
    const v = m[1].replace(/^[`'"\s[]+/, "").trim();
    if (!v) continue;
    const parts = v.split(/\s*(?:\|{1,2}|·|\t)\s*/).map((p) => p.trim()).filter(Boolean);
    const agent = parts[0] ?? "";
    const task = parts.slice(1).join(" · ").trim();
    if (!agent) continue;
    const key = `${agent.toLowerCase()}::${task.toLowerCase()}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({ agent, task });
  }
  return out;
}

// ── LLISTA DE TASQUES («@todo:» / «@done:») ──────────────────────────────────
export function parseTodoDirectives(raw: string): { add: string[]; done: string[]; doing: string[] } {
  const add: string[] = [];
  const done: string[] = [];
  const doing: string[] = [];
  for (const line of raw.split(/\r?\n/)) {
    const a = line.match(/^\s*@\s*(?:todo|pendent|pendient|task)\s*:\s*(.+?)\s*$/i);
    if (a) {
      const t = a[1].replace(/^[`'"]+|[`'"]+$/g, "").trim();
      if (t) add.push(t);
      continue;
    }
    const d = line.match(/^\s*@\s*(?:done|feta|fet|completada)\s*:\s*(.+?)\s*$/i);
    if (d) {
      const t = d[1].replace(/^[`'"]+|[`'"]+$/g, "").trim();
      if (t) done.push(t);
      continue;
    }
    // «@doing:» (o «@fent:»/«@en curs:»): la tasca que la IA té ENTRE MANS
    // ara mateix — es veu EN VIU a la llista fixada («~ En curs»).
    const w = line.match(/^\s*@\s*(?:doing|fent|en\s+curs|in\s+progress)\s*:\s*(.+?)\s*$/i);
    if (w) {
      const t = w[1].replace(/^[`'"]+|[`'"]+$/g, "").trim();
      if (t) doing.push(t);
    }
  }
  return { add, done, doing };
}