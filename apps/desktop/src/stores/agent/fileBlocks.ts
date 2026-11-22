/// Parsers PURS de la resposta de la IA: blocs «@file:», carpetes «@dir:»,
/// esborrat «@delete:», edicions «@edit:», rescat de codi en prosa i arbres
/// de projecte. Sense Tauri ni stores: es poden testejar sols (Vitest).

import type { EditBlock, EditSection } from "./types";

/// Extreu el contingut d'UN fitxer de la resposta del model: si ve en
/// «@file: ruta» agafem el bloc; si ve en codi «```» agafem el codi; si no,
/// el text net (un model pot escriure un .php sencer sense tanques).
export function extractSingleFileContent(raw: string): string {
  const r = (raw || "").trim();
  if (!r) return "";
  const blocs = parseFileBlocks(r);
  if (blocs.length > 0) {
    const b = blocs.reduce((a, c) => (c.content.length > a.content.length ? c : a));
    return b.content.trim();
  }
  const fence = r.match(/```[\w+#.-]*\n([\s\S]*?)```/);
  if (fence) return fence[1].trim();
  // Sense codi identificable: què queda és el text sencer (p. ex. un fitxer
  // .php sense codi fences).
  return r;
}

/// Detecta el placeholder que abans s'escrivia quan el model no retornava
/// contingut («(Pendent: el model no ha retornat contingut…)"). PROHIBIT a
/// disc: qui genera fitxers ha de regenerar-los o deixar-los sense escriure.
export function esContingutPlaceholder(content: string): boolean {
  return /\(pendent:\s*el model no ha retornat contingut/i.test(content);
}

/// Els fitxers «bloc_N.md» són fragments de documentació que els models locals
/// inventen en lloc de codi: NO són codi del projecte i no s'han de crear.
export function esBlocDeDocumentacio(path: string): boolean {
  return /^bloc[_-]?\d+\.(md|txt|markdown)$/i.test((path.split("/").pop() || "").trim());
}

/// Una resposta que només és documentació (.md) quan l'usuari demana CODI.
export function nomesDocumentacio(text: string): boolean {
  const blocs = parseFileBlocks(text);
  if (blocs.length === 0) return false;
  return blocs.every((b) => /\.(md|markdown|txt|rst)$/i.test(b.path));
}

/// Normalitza UNA vegada la ruta que un fitxer ha d'ocupar AL DISC.
///
/// La IA copia el «Projecte (cwd)» del seu system prompt i reemet rutes
/// ABSOLUTES («/Volumes/Toshi/programes/forestin the sky/main.py»); abans
/// només se'ls treia la barra inicial, així que acabaven enganxades sota
/// l'arrel i el projecte s'omplia de carpetes falses («Volumes/Toshi/…»
/// dins del propi projecte). Ara: si la ruta absoluta ÉS dins de l'arrel,
/// se'n treu l'arrel (queda relativa, que és el que el protocol demana); si
/// és realment FORA, retorna null — NoOrbit no escriu fora del projecte.
///
/// És el punt ÚNIC de normalització d'escriptura (frontend): tots els
/// consumidors (fitxers, carpetes, esborrats, edicions) l'han d'usar abans
/// de muntar la ruta absoluta. PUR: sense Tauri ni stores.
export function relForWrite(root: string, rawPath: string): string | null {
  let t = (rawPath || "").trim().replace(/\\/g, "/");
  if (!t) return null;
  const segs = t.split("/").filter((s) => s !== "");
  if (segs.some((s) => s === "..")) return null;
  if (t.startsWith("/")) {
    const r = (root || "").trim().replace(/\\/g, "/").replace(/\/+$/, "");
    if (!r.startsWith("/")) return null;
    const rsegs = r.split("/").filter((s) => s !== "");
    // L'arrel pot aparèixer duplicada dins la mateixa ruta (la IA repeteix el
    // cwd abans del nom del fitxer, i pot tornar-ho a fer): es descarta
    // tantes vegades com apareixi al capdamunt, mai a l'inrevés.
    let i = 0;
    for (;;) {
      let cops = 0;
      // Avança l'arrel (`cops`) sobre la ruta a partir del punt `i`.
      while (
        cops < rsegs.length &&
        i + cops < segs.length &&
        rsegs[cops].toLowerCase() === segs[i + cops].toLowerCase()
      )
        cops++;
      // Cal que l'arrel sigui REALMENT un prefix de la ruta i que en quedi res.
      if (cops < rsegs.length || i + cops >= segs.length) break;
      i += cops;
    }
    if (i === 0) return null; // fora del projecte obert
    return segs.slice(i).join("/") || null;
  }
  return t.replace(/^\/+/, "") || null;
}

/// RUTA ABSOLUTA FORA del projecte: només serveix per avisar l'usuari (el
/// fitxer existeix, però NoOrbit no hi pot escriure). Les rutes relatives
/// mai es marquen: són la forma correcta del protocol.
export function esRutaForaDeRelatiu(root: string, rawPath: string): boolean {
  const t = (rawPath || "").trim().replace(/\\/g, "/");
  return t.startsWith("/") && relForWrite(root, t) === null;
}

/// Converteix una llista de rutes (del pla de reserva) en camins nets.
export function parseManifest(raw: string): string[] {
  const out: string[] = [];
  for (const line of raw.split(/\r?\n/)) {
    let t = line
      .trim()
      .replace(/^[-*]\s+/, "")
      .replace(/^\d+[.)]\s+/, "")
      .replace(/^[`'"]+|[`'"]+$/g, "")
      .trim();
    if (!t) continue;
    // Ha de semblar una ruta: conté «/» o acaba en extensió (.py, .md…).
    if (t.includes("/") || /\.[A-Za-z0-9]{1,6}$/.test(t)) {
      t = t.replace(/^\.?\//, "");
      if (!t.split("/").includes("..")) out.push(t);
    }
  }
  return Array.from(new Set(out));
}

/// Converteix la resposta en una llista de (ruta, contingut). Tolera tanques
/// de codi (` ``` `) i l'optional `@`. Si no troba cap `@file:`, torna buit.
/// HEURÍSTICA NOVA (important): si el bloc usa tanques ``` , el contingut és
/// NOMÉS el que hi ha DINS DE LES TANQUES. Així la prosa posterior, les
/// directrius «RUN|» i els altres anuncis NO s'arreneguen dins del fitxer
/// (abans «python3 x.py» fallava amb SyntaxError per culpa d'eixa brossa).
/// Neteja els ARTEFACTES DE LA INTERFÍCIE que el model pot copiar del xat:
/// l'etiqueta del llenguatge («PHP», «Python»…) i el botó «Copia» que hi ha
/// just damunt del codi. No són codi: al projecte hi quedaven al davant del
/// fitxer i, sobretot, feien que cap «@edit:» coincidís mai amb el text real.
export function stripUiArtifacts(content: string): string {
  const lines = content.split("\n");
  const lang = /^(php|python|javascript|typescript|css|scss|html|json|bash|shell|sql|yaml|yml|toml|rust|go|java|c|cpp|csharp|cs|diff|markdown|text|plaintext|xml|ini|swift|kt|kotlin|ruby|rb|dart|lua|vue|svelte|astro|graphql|dockerfile|makefile|code)$/i;
  let i = 0;
  while (i < lines.length) {
    const t = lines[i].trim();
    if (!t) {
      i++;
      continue;
    }
    if (/^(copia|còpia|copy)$/i.test(t) || lang.test(t)) {
      i++;
      continue;
    }
    break;
  }
  return lines.slice(i).join("\n");
}

export function parseFileBlocks(
  raw: string,
  opts?: { onlyClosed?: boolean },
): { path: string; content: string }[] {
  const lines = raw.split(/\r?\n/);
  type Cur = { path: string; fenced: string[]; bare: string[]; inFence: boolean; hadFence: boolean; closed: boolean };
  const acc: Cur[] = [];
  let cur: Cur | null = null;
  // `closed`: el bloc ja no pot créixer (una directriu posterior l'ha tallat
  // o la seva tanca ``` s'ha tancat). Durant l'STREAMING només s'escriuen al
  // disc els blocs TANCATS: un fence obert és una preview parcial i no es
  // materialitza fins que el model el tanca (opció `onlyClosed`).
  const flush = (closed: boolean) => {
    if (cur) {
      cur.closed = closed;
      acc.push(cur);
      cur = null;
    }
  };
  for (const line of lines) {
    const m = line.match(/^[ \t>#*-]*(?:\d+[.)]\s+)?@?file:\s*(.+?)\s*$/i);
    if (m) {
      flush(true);
      cur = {
        path: m[1].replace(/^[`'"*«]+|[`'"*»;:.,\s]+$/g, "").trim(),
        fenced: [],
        bare: [],
        inFence: false,
        hadFence: false,
        closed: false,
      };
      continue;
    }
    if (!cur) continue;
    if (/^\s*```/.test(line)) {
      // Obri/tanca la tanca: el que passa DINS és codi real.
      cur.inFence = !cur.inFence;
      if (cur.inFence) cur.hadFence = true;
      continue;
    }
    // Una directriu d'execució o d'estructura INTERROMP el bloc: tot el que
    // ve després ja no és contingut d'aquest fitxer.
    if (
      /^[ \t>#*-]*(?:\d+[.)]\s+)?@?(dir|directori|carpeta|folder|delete|esborra|borra|elimina|remove|rm)\w*:/i.test(line) ||
      /^[ \t>#*-]*(?:\d+[.)]\s+)?@?(edit|modifica|substitueix|reemplaça)\w*:/i.test(line) ||
      /^[ \t>#*-]*(RUN|IMG|NB)\|/i.test(line)
    ) {
      flush(true);
      continue;
    }
    if (cur.inFence) cur.fenced.push(line);
    else cur.bare.push(line);
  }
  // Al final del text: un bloc amb tanca TANCADA és definitiu; un de sense
  // tanca (o amb la tanca oberta) encara pot créixer → no tancat.
  flush(cur ? cur.hadFence && !cur.inFence : false);
  return acc
    .filter((f) => f.path.length > 0)
    .filter((f) => !opts?.onlyClosed || f.closed)
    .map((f) => {
      // Amb tanques: només el codi de dins. Sense tanques: les línies nues
      // (com abans), però tallades a la primera directriu.
      const body = f.hadFence && f.fenced.length > 0 ? f.fenced : f.bare;
      return {
        path: f.path,
        content:
          stripUiArtifacts(body.join("\n").replace(/^\s*\n+/, "").replace(/\s+$/, "")) + "\n",
      };
    })
    // Un bloc sense contingut útil s'ignora: FILE_NUDGE ja encarrega de
    // recordar al model que el reemeta complet.
    .filter((f) => f.content.trim().length > 0);
}

/// Extreu les directrius de CARPETA que la IA emet per crear estructura buida
/// (carpetes sense fitxer). Formes tolerades: «@dir: ruta», «@carpeta: ruta»,
/// «@folder: ruta», «@directori: ruta». S'usen quan la IA vol la gestió de
/// carpetes minuciosa: crea la carpeta encara que no contingui cap fitxer.
export function parseDirDirectives(raw: string): string[] {
  const out: string[] = [];
  for (const line of raw.split(/\r?\n/)) {
    const m = line.match(/^\s*@?\s*(?:dir|directori|carpeta|folder)\w*:\s*(.+?)\s*$/i);
    if (!m) continue;
    let d = m[1]
      .replace(/^[`'"]+|[`'"]+$/g, "")
      .trim()
      .replace(/\\/g, "/");
    if (!d) continue;
    // Si té extensió de fitxer coneguda NO pot ser una carpeta (ni tan sols
    // amb «/» al mig: «admin/index.php» és un FITXER, no un directori).
    // Abans es creava com a carpeta i fallava («admin/» no existia).
    const ext = (d.split("/").pop() ?? "").split(".").pop()?.toLowerCase() ?? "";
    if (d.includes(".") && REC_EXT.has(ext)) continue;
    if (!d.split("/").includes("..")) out.push(d);
  }
  return Array.from(new Set(out));
}

/// Bloc de codi en prosa que NO s'ha materialitzat: la ruta no s'ha pogut
/// deduir (o era ambigua). Es reporta al xat amb avís visible — MAI s'escriu
/// a una ruta inventada tipus «bloc_N.ext».
export interface ProseSkip {
  lang: string;
  /// Primera línia útil del bloc (perquè l'usuari sàpiga quin codi és).
  preview: string;
  reason: "sense-nom" | "ambigu";
}

/// Rescata els blocs de codi (``` ...) d'una resposta en prosa i en dedueix
/// el nom de fitxer. És el camí de RESCAT quan la IA no usa el format
/// estricte @file: — i ha de ser TOLERANT perquè els models no responen tots
/// igual: tanques CRLF, `~~~`, bloc sense tancar (resposta tallada), nom del
/// fitxer escrit DESPRÉS del bloc o com a comentari a la primera línia…
/// Així un tutorial explicat es pot convertir en fitxers reals amb codi.
///
/// REGLES DE RUTES (important):
///  • Si hi ha MANIFEST (llista de rutes del pla), les rutes del manifest
///    manen: un bloc «style.css» va a «tema-x/style.css» si el pla ho diu.
///  • Si la ruta no es pot deduir (o és ambigua), el bloc S'OMET i es
///    registra a `opts.skipped` — mai s'inventa «bloc_N.ext».
export function extractFromProse(
  raw: string,
  opts?: { manifest?: string[]; skipped?: ProseSkip[] },
): { path: string; content: string }[] {
  const LANG_EXT: Record<string, string> = {
    python: "py", py: "py", javascript: "js", js: "js", typescript: "ts", ts: "ts",
    json: "json", html: "html", css: "css", markdown: "md", md: "md",
    bash: "sh", sh: "sh", shell: "sh", sql: "sql", yaml: "yaml", yml: "yml", toml: "toml",
    rust: "rs", go: "go", java: "java", c: "c", cpp: "cpp",
  };
  const out: { path: string; content: string }[] = [];
  const used = new Set<string>();
  // Extensions vàlides: evita falsos positius com «e.g», «v2.0», «i.e».
  const knownExts = new Set(
    Object.values(LANG_EXT).concat(["md", "txt", "cfg", "ini", "toml", "yml", "yaml", "json", "lock", "env", "xml", "svg", "jsx", "tsx", "scss", "hpp", "h"])
  );
  // Normalitzem NOMÉS per a l'extracció: CRLF → LF i línies de tanca `~~~lang`
  // → ```lang (els models mixten estils). La línia es manté sencera: només
  // canvia el marcador, el salt de línia queda intacte.
  const norm = raw
    .replace(/\r\n/g, "\n")
    .replace(/\r/g, "\n")
    .replace(/^([ \t]*)~~~[ \t]*([A-Za-z0-9]*)[ \t]*$/gm, "$1```$2");

  const dedupe = (path: string, content: string) => {
    let p = path.replace(/\\/g, "/");
    if (p.split("/").some((s) => s === "..")) return;
    // Si el model REPETEIX el mateix fitxer (bucle genseratiu), NO en creem
    // còpies «_2», «_3»…: ens quedem amb l'última versió (la més completa).
    // Abans sortien 14 fitxers idèntics i el projecte quedava ple de brossa.
    const i = out.findIndex((f) => f.path === p);
    if (i >= 0) {
      out[i] = { path: p, content };
      return;
    }
    used.add(p);
    out.push({ path: p, content });
  };

  /// ARBRE DEL PROJECTE que la IA dibuixa a la resposta (ASCII: «├──», «└──»,
  /// «│»). Serveix per posar cada fitxer a la CARPETA CORRECTA: si la
  /// resposta diu «store-wp-one/style.css» però el codi només diu
  /// «style.css», el fitxer va a la carpeta de l'arbre, no a l'arrel.
  const treeMap = extractTreePaths(norm);

  /// Dedueix el nom del fitxer d'un bloc: 1) un comentari a la PRIMERA línia
  /// del codi («// src/App.tsx», «# main.py», «<!-- index.html -->»); 2) el
  /// text JUST ABANS del bloc; 3) el text JUST DESPRÉS; 4) l'ARBRE del
  /// projecte (si el fitxer hi apareix, va a la seva carpeta); 5) el MANIFEST
  /// del pla. Si res no és clar, torna NULL: el bloc s'omet amb avís (abans
  /// s'inventava «bloc_N.ext» i s'escrivia brossa al projecte).
  const manifest = opts?.manifest ?? [];
  const inferPath = (lang: string, content: string, before: string, after: string): string | null => {
    let inferred: string | null = null;
    const first = content.split("\n", 1)[0] ?? "";
    const cm = first.match(/^\s*(?:\/\/|#|--|<!--|\*|\/\*)\s*([\w][\w./\\-]*\.[A-Za-z0-9]{1,6})\s*(?:-->)?$/);
    if (cm) {
      const ext = cm[1].split(".").pop()!.toLowerCase();
      if (knownExts.has(ext)) inferred = cm[1];
    }
    if (!inferred) {
      for (const side of [before, after]) {
        const named = side.match(/[A-Za-z0-9_./\\-]+\.[A-Za-z0-9]{1,6}/g);
        if (!named) continue;
        // Candidat amb extensió RECONEGUDA (evita «e.g», «3.4»), el més proper.
        const cand = [...named].reverse().find((c) => {
          const ext = c.includes(".") ? c.split(".").pop()!.toLowerCase() : "";
          return knownExts.has(ext) && !/^[\d.]+$/.test(c);
        });
        if (cand) {
          inferred = cand;
          break;
        }
      }
    }
    // L'arbre del projecte: «style.css» → «store-wp-one/style.css» (el lloc
    // on la IA ha dit que ha de ser). sense això tot acaba a l'arrel.
    if (!inferred) {
      const basic = `${before} ${after}`.match(/[A-Za-z0-9_.\-]+\.[A-Za-z0-9]{1,6}/g);
      if (basic) {
        for (const c of [...basic].reverse()) {
          const ruta = treeMap.get(c.toLowerCase());
          if (ruta) {
            inferred = ruta;
            break;
          }
        }
      }
    }
    // MANIFEST: si el pla dona la ruta completa del mateix fitxer, mana.
    if (manifest.length > 0) {
      if (inferred) {
        const base = inferred.split("/").pop()!.toLowerCase();
        const delPla = manifest.find((p) => p.split("/").pop()!.toLowerCase() === base);
        if (delPla) return delPla;
        return inferred;
      }
      // Sense nom: només acceptem el manifest si hi ha UN ÚNIC candidat
      // amb l'extensió del llenguatge del bloc (si n'hi ha dos, és ambigu).
      const ext = LANG_EXT[lang];
      if (ext) {
        const cands = manifest.filter((p) => p.toLowerCase().endsWith(`.${ext}`));
        if (cands.length === 1) return cands[0];
      }
      return null;
    }
    return inferred;
  };

  const skip = (lang: string, content: string, ambiguous: boolean) => {
    const preview = (content.split("\n").find((l) => l.trim()) ?? "").trim().slice(0, 80);
    opts?.skipped?.push({ lang, preview, reason: ambiguous ? "ambigu" : "sense-nom" });
  };

  // Blocs TANCATS: obertura ```lang + contingut + tancament ``` (tolerant a
  // espais/trailers a la línia de tancament).
  const fenceRe = /```([A-Za-z0-9_-]*)\n([\s\S]*?)\n?[ \t]*```/g;
  let m: RegExpExecArray | null;
  while ((m = fenceRe.exec(norm)) !== null) {
    const lang = (m[1] || "").toLowerCase();
    const content = stripUiArtifacts(m[2].replace(/\s+$/, "")) + "\n";
    if (!content.trim()) continue;
    const before = norm.slice(Math.max(0, m.index - 320), m.index);
    const after = norm.slice(m.index + m[0].length, m.index + m[0].length + 320);
    const path = inferPath(lang, content, before, after);
    if (!path) {
      skip(lang, content, manifest.length > 0);
      continue;
    }
    dedupe(path, content);
  }
  // ÚLTIM bloc SENSE TANCAR: respostes tallades a mitja generació. Sense
  // aquest rescat, el codi més valuós (el que venia al final) es perdia.
  // NOMÉS si hi ha un nombre IMPARELL de línies de tanca: aleshores la darrera
  // és una obertura sense parella (bloc tallat). Si són parells, tot està
  // tancat i no hi ha res a rescatar (així no engolim prosa per error).
  const fences = [...norm.matchAll(/^```[A-Za-z0-9_-]*[ \t]*$/gm)];
  if (fences.length % 2 === 1) {
    const lastOp = fences[fences.length - 1];
    const after = norm.slice((lastOp.index ?? 0) + lastOp[0].length).replace(/^\n/, "");
    if (after.trim()) {
      const lang = (lastOp[0].match(/```([A-Za-z0-9_-]*)/)?.[1] ?? "").toLowerCase();
      const content = stripUiArtifacts(after.replace(/\s+$/, "")) + "\n";
      const before = norm.slice(0, lastOp.index ?? 0).slice(-320);
      const path = inferPath(lang, content, before, "");
      if (path) dedupe(path, content);
      else skip(lang, content, manifest.length > 0);
    }
  }
  return out;
}

/// Extensions considerades «de fitxer» a l'hora de reconèixer rutes en prosa.
/// IMPORTANT: si una extensió NO hi és, la ruta es classifica com a CARPETA i
/// es crea com a carpeta (i falla). Per això hi ha totes les habituals:
/// WordPress/PHP, Twig/Blade, Laravel, Django, Rails, Go, Kotlin, Swift…
export const REC_EXT = new Set([
  "py", "js", "jsx", "ts", "tsx", "json", "md", "txt", "css", "scss", "html",
  "toml", "yaml", "yml", "ini", "cfg", "env", "lock", "rs", "go", "java",
  "c", "h", "cpp", "hpp", "sh", "sql", "csv", "xml", "svg", "glb", "fbx",
  // WordPress / PHP i el seu ecosistema
  "php", "php3", "php4", "php5", "phtml", "twig", "blade", "po", "pot", "mo",
  // Altres llenguatges i formats habituals
  "kt", "kts", "swift", "m", "mm", "cs", "rb", "phpunit", "vue", "svelte",
  "astro", "graphql", "gql", "proto", "dockerfile", "makefile", "conf",
  "htaccess", "htpasswd", "webp", "jpg", "jpeg", "png", "gif", "ico", "pdf",
  "woff", "woff2", "ttf", "eot", "mp4", "webm", "zip", "tar", "gz",
]);

/// Reconeix rutes que la IA RECOMANA en prosa (fora dels blocs de codi):
/// «hauries de crear src/models/order.py», «afegeix una carpeta assets/», etc.
/// També hi inclou els noms dels @file:/@dir: ja presentats en format estricte.
/// S'usa per pre-emplenar el diàleg de creació manual: l'usuari només ha
/// d'afegir/treure línies i prémer «Crear-les».
export function extractRecommendedPaths(raw: string): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  const add = (p: string, asDir = false) => {
    let t = p.trim().replace(/^[`'"«<(]+/, "").replace(/[`'"»>,)\].:;]+$/, "");
    t = t.replace(/^\.?\/+/, "");
    if (!t || /[\s$]/.test(t)) return;
    if (t.split("/").includes("..")) return;
    if (t.includes("://") || /^www\./i.test(t)) return; // descarta URLs
    if (/(^|\/)(node_modules|target|\.git)(\/|$)/.test(t)) return;
    if (!/[A-Za-z]/.test(t)) return; // sense lletres (p. ex. «10/5») no és ruta
    if (!t.endsWith("/")) t += asDir ? "/" : "";
    if (seen.has(t)) return;
    seen.add(t);
    out.push(t);
  };
  // 1) Directrius i fitxers ja en format estricte.
  for (const d of parseDirDirectives(raw)) add(d, true);
  for (const f of parseFileBlocks(raw)) add(f.path);
  // 2) Prosa: ignores els blocs ``` (el seu contingut no són recomanacions).
  const text = raw.replace(/```[\s\S]*?(```|$)/g, " ");
  // Rutes amb almenys un «/» (carpeta o fitxer dins d'una carpeta).
  for (const m of text.matchAll(/[A-Za-z0-9_.\-]+(?:\/[A-Za-z0-9_.\-]+)+\/?/g)) {
    let tok = m[0];
    if (tok.endsWith("/")) add(tok);
    else {
      const last = tok.split("/").pop() ?? "";
      const ext = last.includes(".") ? last.split(".").pop()!.toLowerCase() : "";
      add(tok, !REC_EXT.has(ext)); // sense extensió coneguda → carpeta
    }
  }
  // Fitxers d'un sol nivell amb extensió coneguda (p. ex. «requirements.txt»).
  for (const m of text.matchAll(/\b[A-Za-z0-9_.\-]+\.([A-Za-z0-9]{1,6})\b/g)) {
    if (REC_EXT.has(m[1].toLowerCase())) add(m[0]);
  }
  return out.slice(0, 60);
}

/// Directrius d'ESBORRAT que la IA emet per treure fitxers o carpetes del
/// projecte. Formes tolerades: «@delete: ruta», «@esborra: ruta»,
/// «@borra: ruta», «@elimina: ruta», «@remove: ruta». Abans d'això, dir-li
/// a la IA «esborra X» era impossible: un @file: buit CREAVA el fitxer buit
/// en lloc d'esborrar-lo. Les rutes són sempre relatives al projecte.
export function parseDeleteDirectives(raw: string): string[] {
  const out: string[] = [];
  for (const line of raw.split(/\r?\n/)) {
    const m = line.match(/^\s*@?\s*(?:delete|esborra|borra|elimina|remove|rm)\w*:\s*(.+?)\s*$/i);
    if (!m) continue;
    let d = m[1]
      .replace(/^[`'"]+|[`'"]+$/g, "")
      .trim()
      .replace(/\\/g, "/");
    if (!d || d.split("/").includes("..")) continue;
    if (!out.includes(d)) out.push(d);
  }
  return out;
}

/// Llegeix l'ARBRE de fitxers que la IA dibuixa a la resposta (ASCII amb
/// «├──», «└──», «│») i en treu un mapa «nom del fitxer» → «ruta completa».
///
/// Per exemple, si la resposta diu:
/// ```
/// store-wp-one/          ← tema
/// ├── style.css
/// └── functions.php
/// ```
/// el mapa serà { "style.css" → "store-wp-one/style.css", … } i els fitxers
/// es posaran a la CARPETA CORRECTA (no a l'arrel del projecte).
export function extractTreePaths(raw: string): Map<string, string> {
  const map = new Map<string, string>();
  const lines = raw.split(/\r?\n/);
  // Pila de carpetes. Cada línia de l'arbre i la seva indentació decideixen
  // el nivell (cada «│   » és un nivell). Les carpetes «arrel» (línia sense
  // ramal, com «store-wp-one/») es desen amb indentació -1: així els seus
  // fills (indentació 0) no les expulsen de la pila.
  const stack: { indent: number; name: string }[] = [];
  for (const line of lines) {
    // Línia de l'arbre: «├── style.css» / «│   └── options.php».
    const m = line.match(/^([\s│]*)(?:├──|└──|\+--|`--|├--)\s*([^\s`][^`]*?)\s*$/);
    if (m) {
      const indent = m[1].length;
      const name = m[2].trim();
      // Talla els germans de nivell superior (i el mateix nivell).
      while (stack.length > 0 && stack[stack.length - 1].indent >= indent) stack.pop();
      if (name.endsWith("/")) {
        stack.push({ indent, name: name.replace(/\/+$/, "") });
      } else if (/\.[A-Za-z0-9]{1,6}$/.test(name)) {
        const ruta = [...stack.map((s) => s.name), name].join("/");
        const basic = name.split("/").pop()!.toLowerCase();
        // El PRIMER que apareix és el bon (si el nom es repeteix en dos
        // arbres, ens quedem amb el primer).
        if (!map.has(basic)) map.set(basic, ruta);
      }
      continue;
    }
    // Línia de carpeta d'arrel: «store-wp-one/   ← tema».
    const dir = line.match(/^([\s│]*)([A-Za-z0-9_.\-]+\/)\s*(?:←.*)?$/);
    if (dir && !/[│├└]/.test(dir[1])) {
      stack.length = 0;
      stack.push({ indent: -1, name: dir[2].replace(/\/+$/, "") });
    }
  }
  return map;
}

/// Extreu els blocs «@edit: ruta» amb seccions CERCA/NOU (edicions
/// quirúrgiques). Tolerant amb marcadors >>> (guerra de conflict marks),
/// talls de resposta i seccions sense tancar. NOU buit = esborrar codi.
export function parseEditBlocks(raw: string): EditBlock[] {
  const lines = raw.split(/\r?\n/);
  const out: EditBlock[] = [];
  let cur: EditBlock | null = null;
  let sec: { find: string[]; rep: string[]; phase: 0 | 1 } | null = null;
  const flushSec = () => {
    if (cur && sec) {
      const find = stripUiArtifacts(sec.find.join("\n").replace(/^\n+|\s+$/g, ""));
      const replace = stripUiArtifacts(sec.rep.join("\n").replace(/^\n+|\s+$/g, ""));
      if (find.trim()) cur.edits.push({ find, replace });
    }
    sec = null;
  };
  for (const line of lines) {
    const m = line.match(/^[ \t>*-]*@?(?:edit|modifica|substitueix|reempla[çc]a)\w*:\s*(.+?)\s*$/i);
    if (m) {
      flushSec();
      const newPath = m[1]
        .replace(/^[`'"*«]+|[`'"*»;:.,\s]+$/g, "")
        .trim();
      // Rutes repetides (dos @edit: del mateix fitxer): s'accumulen al
      // mateix bloc (una sola lectura/escriptura i un sol informe).
      if (!(cur && newPath && cur.path === newPath)) {
        if (cur) out.push(cur);
        cur = { path: newPath, edits: [] };
      }
      continue;
    }
    const t = line.trim();
    if (t.startsWith("<<<<<<<")) {
      flushSec();
      sec = { find: [], rep: [], phase: 0 };
      continue;
    }
    if (t.startsWith("=======")) {
      if (sec) sec.phase = 1;
      continue;
    }
    if (t.startsWith(">>>>>>>")) {
      flushSec();
      continue;
    }
    if (!cur || !sec) continue;
    // Les tanques de codi NO interrompen una secció (el codi pot contenir-les).
    if (sec.phase === 0) sec.find.push(line);
    else sec.rep.push(line);
  }
  flushSec();
  if (cur) out.push(cur);
  return out.filter((b) => b.path && b.edits.length > 0);
}

/// Fragment REAL del fitxer més semblant al text CERCA d'una edició que no
/// ha coincidit: finestra de línies (de la mida del CERCA) amb la màxima
/// coincidència normalitzada (espais i majúscules ignorats). Es mostra al
/// xat amb números de línia perquè l'usuari/model corregeixi el CERCA en un
/// reintent. PUR: sense Tauri ni stores.
export function fragmentMesSemblant(
  contingut: string,
  cerca: string,
): { startLine: number; endLine: number; lines: string[]; score: number } | null {
  const norm = (l: string) => l.trim().replace(/\s+/g, " ").toLowerCase();
  const findLines = cerca.split("\n").map(norm);
  const contentLines = contingut.split("\n");
  if (findLines.length === 0 || contentLines.length === 0 || !cerca.trim()) return null;
  const win = Math.min(findLines.length, contentLines.length);
  let best = -1;
  let bestScore = 0;
  for (let i = 0; i + win <= contentLines.length; i++) {
    let hits = 0;
    for (let k = 0; k < win; k++) {
      if (norm(contentLines[i + k]) === findLines[k] && findLines[k] !== "") hits++;
    }
    const score = hits / findLines.length;
    if (score > bestScore) {
      bestScore = score;
      best = i;
    }
  }
  if (best < 0 || bestScore === 0) return null;
  // Màxim 16 línies al missatge del xat (un CERCA de 200 línies no hi cap).
  const shown = Math.min(win, 16);
  return {
    startLine: best + 1,
    endLine: best + shown,
    lines: contentLines.slice(best, best + shown),
    score: bestScore,
  };
}

/// Una edició CERCA/NOU que no ha coincidit: el fragment real més semblant
/// (null si no n'hi ha cap) perquè el cridaner el mostri al xat.
export interface EditFailure {
  fragment: { startLine: number; endLine: number; lines: string[]; score: number } | null;
}

export interface EditApplyOutcome {
  updated: string;
  ok: number;
  fail: number;
  noop: number;
  failures: EditFailure[];
}

/// Aplica les seccions CERCA/NOU al contingut d'un fitxer (PUR: la lectura/
/// escriptura i les marques de revisió les fa el store). Coincidència exacta
/// o per línies retallades (tolera indentació). Els no-ops (CERCA == NOU) es
/// REBUTGEN. Quan un CERCA no coincideix, calcula el fragment real més
/// semblant amb números de línia (feedback per al reintent).
export function applyEditsToContent(old: string, edits: EditSection[]): EditApplyOutcome {
  let updated = old;
  let ok = 0;
  let fail = 0;
  let noop = 0;
  const failures: EditFailure[] = [];
  for (const ed of edits) {
    // CERCA i NOU idèntics: no cal tocar el fitxer (ni obrir un diff fals).
    if (ed.find.trimEnd() === (ed.replace ?? "").trimEnd()) {
      noop++;
      continue;
    }
    if (updated.includes(ed.find)) {
      // Substitució LITERAL per índex: String.replace interpretaria $&/$'/$`
      // del text NOU (corrupció de codi JS/regex) i un replace absent eixiria
      // com a «undefined». Alineat amb el backend Rust.
      const at = updated.indexOf(ed.find);
      updated =
        updated.slice(0, at) + (ed.replace ?? "") + updated.slice(at + ed.find.length);
      ok++;
      continue;
    }
    // Coincidència per línies retallades (models que canvien indentació):
    const findLines = ed.find.split("\n").map((l) => l.trim());
    const contentLines = updated.split("\n");
    let hit = -1;
    outer: for (let i = 0; i + findLines.length <= contentLines.length; i++) {
      for (let k = 0; k < findLines.length; k++) {
        if (contentLines[i + k].trim() !== findLines[k]) continue outer;
      }
      hit = i;
      break;
    }
    if (hit >= 0) {
      const replLines = ed.replace ? ed.replace.split("\n") : [];
      contentLines.splice(hit, findLines.length, ...replLines);
      updated = contentLines.join("\n");
      ok++;
    } else {
      fail++;
      failures.push({ fragment: fragmentMesSemblant(updated, ed.find) });
    }
  }
  return { updated, ok, fail, noop, failures };
}