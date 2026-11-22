/// Cerca i substitució GLOBAL (SearchPalette): funcions PURES, testejables
/// sense Monaco ni Tauri. Mode literal (el del backend `search_files`) i
/// mode regex amb validació d'errors de patró i grups de substitució ($1…).

export interface LineMatch {
  /// Línia 1-based.
  line: number;
  /// Columna 1-based de l'inici de la coincidència.
  column: number;
  matchLen: number;
  /// Línia sencera (preview al panell de resultats).
  preview: string;
}

/// Escapa un text literal perquè es pugui usar com a patró regex.
export function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/// Construeix la regex de cerca (sempre global). En mode literal escapa el
/// text; en mode regex valida el patró i llança l'error de sintaxi perquè
/// la UI el mostri (mai una cerca silenciosament buida per patró invàlid).
export function buildSearchRegex(pattern: string, regexMode: boolean): RegExp {
  const source = regexMode ? pattern : escapeRegExp(pattern);
  return new RegExp(source, "gi");
}

/// Coincidències per línia (posicions per al ressaltat i la navegació F3).
/// `limit` talla la cerca quan el projecte és enorme (avís a la UI).
export function findLineMatches(content: string, re: RegExp, limit = 200): LineMatch[] {
  const out: LineMatch[] = [];
  const lines = content.split("\n");
  for (let i = 0; i < lines.length && out.length < limit; i++) {
    for (const m of lines[i].matchAll(re)) {
      if (out.length >= limit) break;
      out.push({
        line: i + 1,
        column: (m.index ?? 0) + 1,
        matchLen: m[0].length,
        preview: lines[i],
      });
    }
  }
  return out;
}

/// Nombre de coincidències d'un contingut (recompte per fitxer abans de
/// substituir). Les coincidències de longitud zero no compten com a canvi.
export function countMatches(content: string, re: RegExp): number {
  let n = 0;
  for (const m of content.matchAll(re)) {
    if (m[0].length > 0) n++;
  }
  return n;
}

/// Substitueix TOTES les coincidències. En mode literal el text de
/// substitució és literal («$&» no s'expandeix); en mode regex els grups
/// ($1, $&, $`…) funcionen com a JavaScript estàndard.
export function replaceAll(
  content: string,
  re: RegExp,
  replacement: string,
  regexMode: boolean
): { text: string; count: number } {
  const count = countMatches(content, re);
  const repl = regexMode ? replacement : replacement.replace(/\$/g, "$$$$");
  const text = count > 0 ? content.replace(re, repl) : content;
  return { text, count };
}

/// Dependències injectades del «Substitueix tot» per fitxer: la funció és
/// testejable sense Tauri ni Monaco (la carrera amb l'escriptura es simula).
export interface ReplaceFileDeps {
  readFile: (path: string) => Promise<string>;
  writeFile: (path: string, content: string) => Promise<void>;
  /// Pestanya VIUA (pot ser bruta), si el fitxer és obert a l'editor.
  getTab: (path: string) => { content: string } | undefined;
  /// Escriu el contingut a la pestanya de forma SÍNCRONA (sense brutar).
  setTab: (path: string, content: string) => void;
}

/// «Substitueix tot» en UN fitxer sense perdre el que l'usuari tecleja
/// durant l'escriptura: després del write es rellegeix l'estat viu de la
/// pestanya i, si ha canviat, el reemplaçament es reaplica sobre el
/// contingut viu (i es reescriu al disc). Retorna el nombre de canvis.
export async function replaceAllInFile(
  path: string,
  re: RegExp,
  replacement: string,
  regexMode: boolean,
  deps: ReplaceFileDeps
): Promise<number> {
  const tab = deps.getTab(path);
  const content = tab ? tab.content : await deps.readFile(path);
  const r = replaceAll(content, re, replacement, regexMode);
  if (r.count === 0) return 0;
  await deps.writeFile(path, r.text);
  if (tab) {
    const live = deps.getTab(path);
    if (live) {
      // Carrera: l'usuari pot haver teclejat mentre s'escrivia. L'estat viu
      // mana; entre rellegir-lo i escriure'l NO hi ha cap await (ningú no
      // pot intercalar edicions en eixe buit síncron).
      const final =
        live.content !== content ? replaceAll(live.content, re, replacement, regexMode) : r;
      deps.setTab(path, final.text);
      // Disc alineat amb l'editor. Si l'usuari torna a teclejar durant esta
      // escriptura, la pestanya queda bruta i ⌘S desa el seu text: no es perd.
      if (final.text !== r.text) await deps.writeFile(path, final.text);
      return final.count;
    }
  }
  return r.count;
}