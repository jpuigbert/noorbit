import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { DEFAULT_UNREAL_CONFIG } from "../core/unreal/client";
import { useExpertStore } from "./expertStore";
import { useWorkspaceStore } from "./workspaceStore";

export type Modality = "image" | "code" | "text" | "3d";
/// Destí exterior on aplicar una ordre escrita al xat.
export type ApplyTarget = "blender" | "unreal";

export interface BackendInfo {
  id: string;
  name: string;
  modality: string;
  available: boolean;
  detail: string | null;
}

export interface TimelineStep {
  label: string;
  detail: string;
  ok: boolean;
  at: number;
}

/// Un torn del xat: una bombolla d'usuari o de la IA. Les respostes de la IA
/// poden portre el raonament (thinking), els pas de progrés i la durada.
export interface ChatMessage {
  role: "user" | "assistant";
  text: string;
  thinking?: string | null;
  steps?: TimelineStep[];
  elapsedMs?: number | null;
  at: number;
  error?: boolean;
}

/// Torn de conversa que s'envia al backend perquè la IA (o un FORK) no perda
/// el fil: rol i contingut tal com es van escriure.
export interface HistoryTurn {
  role: string;
  content: string;
}

/// UN XAT COMPLE. Cada xat treballa per la seva banda: té la seva conversa,
/// la seva cua, el seu proveïdor/model i la seva generació en curs (o cap).
/// Així es pot donar una ordre a la IA i obrir un altre xat —amb la IA local
/// o la remota configurada— per continuar el treball en paral·lel, i fer
/// forks d'una conversa per provar un altre camí sense perdre l'original.
export interface ChatSession {
  id: string;
  title: string;
  createdAt: number;
  /// Fork: xat d'origen i índex del missatge del qual va brollar.
  parentId: string | null;
  originIndex: number | null;
  /// IA D'AQUEST XAT: null = la global (el proveïdor actiu a la configuració).
  provider: string | null;
  /// Model d'Ollama per a aquest xat: null = el model global triat.
  model: string | null;
  /// Especialista d'aquest xat ("" = agent de text directe).
  expertId: string;
  /// Plataforma objectiu quan es genera codi: «desktop» (per defecte, qualsevol
  /// llenguatge d'escriptori), «android» (Kotlin + Compose natiu) o «ios»
  /// (Swift + SwiftUI natiu). Afecta SCAFFOLD_SYSTEM i les pistes de context.
  platform: "desktop" | "android" | "ios";
  /// Cert si aquest xat adjunta el codi del projecte a cada missatge.
  includeContext: boolean;
  messages: ChatMessage[];
  /// Missatges afegits mentre la IA treballa; s'executen en cua al acabar.
  queue: string[];
  running: boolean;
  stopped: boolean;
  startedAt: number | null;
  lastElapsedMs: number | null;
  /// Text rebut en directe (streaming) durant la generació d'aquest xat.
  stream: string | null;
  timeline: TimelineStep[];
  lastResult: string | null;
  lastThinking: string | null;
  error: string | null;
  contextInfo: string | null;
}

/// Xat heretat de les tasques que no venen d'un xat múltiple (autònomes,
/// ajudants de Blender…). El backend l'anomena igual: DEFAULT_SESSION.
const DEFAULT_ID = "main";

function newId(): string {
  return "x" + Date.now().toString(36) + Math.random().toString(36).slice(2, 6);
}

function defaultCtx(): boolean {
  try {
    // Per DEFECTE sempre actiu: la IA ha de veure el codi del projecte.
    // Només es desactiva si l'usuari ho demana explícit («0»).
    return localStorage.getItem("noorbit.ctx") !== "0";
  } catch {
    return true;
  }
}

function blankSession(id: string, over: Partial<ChatSession> = {}): ChatSession {
  return {
    id,
    title: "",
    createdAt: Date.now(),
    parentId: null,
    originIndex: null,
    provider: null,
    model: null,
    expertId: "",
    platform: "desktop",
    includeContext: defaultCtx(),
    messages: [],
    queue: [],
    running: false,
    stopped: false,
    startedAt: null,
    lastElapsedMs: null,
    stream: null,
    timeline: [],
    lastResult: null,
    lastThinking: null,
    error: null,
    contextInfo: null,
    ...over,
  };
}

/// La conversa precedent que acompanya cada missatge nou (per CONTINUAR el
/// treball i perquè un FORK herete el fil). S'acota per no desbordar la
/// memòria dels models: els últims torns, cadascun retallat.
function toHistory(messages: ChatMessage[]): HistoryTurn[] {
  return messages
    .filter((m) => m.text.trim() && !m.error)
    .slice(-10)
    .map((m) => ({ role: m.role, content: m.text.slice(0, 6000) }));
}

/// Persistència dels xats oberts (conversa inclosa) a localStorage.
const LS_KEY = "noorbit.chats";

/// Demana al backend l'últim raonament generat PER UN XAT (per mostrar-lo).
async function fetchThinking(sid: string): Promise<string | null> {
  try {
    return await invoke<string | null>("ai_last_thinking", { session: sid });
  } catch {
    return null;
  }
}

/// Gestió dels errors: els de memòria d'Ollama es tradueixen a un missatge
/// clar en català; la cancel·lació no es mostra com a error.
function isOOM(e: unknown): boolean {
  const s = String(e).toLowerCase();
  return (
    s.includes("out of memory") ||
    s.includes("oom") ||
    s.includes("could not allocate") ||
    s.includes("failed to allocate")
  );
}

function isCancelled(e: unknown): boolean {
  return String(e).includes("cancel·lat");
}

/// El model ha tallat la descàrrega de la resposta (manca de memòria per a un
/// context gran o temps màxim exhaurit). El backend ja comprimeix el context i
/// reintenta; aquest missatge només es veu si també falla el segon intent.
function isBodyCut(e: unknown): boolean {
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
function isRetriableError(e: unknown): boolean {
  return isOOM(e) || isBodyCut(e);
}

function toErrorText(e: unknown): string {
  if (isOOM(e))
    return "Memòria insuficient: el model és massa gran per a aquest ordinador. NoOrbit ho ha tornat a provar amb MENYS context i ha continuat fallant. Prova un model més lleuger (p. ex. :3b o quantitzat q4).";
  if (isBodyCut(e))
    return "La memòria no ha abastat tot el context i la resposta s'ha tallat. NoOrbit ho ha reintentat AUTOMÀTICAMENT amb context REDUÏT → SENSE codi → SENSE historial, i ha tornat a fallar. Desactiva «Inclou el codi» o usa un model més lleuger.";
  return String(e);
}

/// Instruccions per a l'agent «crea-fitxers»: forcen un format estricte
/// `@file: <ruta>` perquè es puga parçar i escriure cada fitxer de veritat.
/// Inclou un exemple concret (few-shot): els models petits obeyen molt
/// millor un exemple copiable que instruccions abstractes.
const SCAFFOLD_SYSTEM =
  "ETS UN GENERADOR DE PROJECTES. Has d'emetre NOMÉS fitxers, en aquest " +
  "format exacte, sense cap explicació, títol ni text al voltant.\n" +
  "Cada fitxer comença amb una línia «@file: <ruta_relativa>» i continua amb " +
  "el seu contingut fins al següent «@file:».\n\n" +
  "EXEMPLE de resposta CORRECTA:\n" +
  "@file: app.py\n" +
  "from flask import Flask\n" +
  "app = Flask(__name__)\n\n" +
  "@file: db.py\n" +
  "from flask_sqlalchemy import SQLAlchemy\n\n" +
  "@file: README.md\n" +
  "# Projecte\nInstal·lació: pip install -r requirements.txt\n\n" +
  "Regles: rutes relatives sense «/» inicial; usa SEMPRE la ruta completa amb " +
  "les subcarpetes (p. ex. src/models/order.py) perquè NoOrbit creï també les " +
  "carpetes; si vols una carpeta buida (estructura), emet una línia " +
  "«@dir: <ruta>» (p. ex. @dir: assets/textures). Genera TOTS els fitxers " +
  "perquè el projecte funcioni (codi, configuració, requirements.txt i " +
  "README.md). No escriguis res més enllà de blocs «@file:» i línies «@dir:».";

/// System prompt del pla de reserva: només la LLISTA DE RUTES (una per línia),
/// tasca molt més senzilla que un model dèbil sí que pot resoldre.
const MANIFEST_SYSTEM =
  "Respon NOMÉS amb una llista de rutes de fitxers, una per línia, sense " +
  "números, guions ni explicacions.\nExemple:\napp.py\ndb.py\nmodels/order.py\nREADME.md";

/// ── Platform hints: s'ANTOPOSEN al SCAFFOLD_SYSTEM quan el xat té una
/// plataforma mòbil seleccionada. Així la IA escriu codi NATIU (Kotlin/Compose
/// per a Android; Swift/SwiftUI per a iOS) dins l'estructura oficial que
/// Android Studio / Xcode esperen. Sense embolcalls web (capacitor/cordova).
const PLATFORM_ANDROID =
  "PLATAFORMA OBJECTIU: Android NATIU (Kotlin + Jetpack Compose, minSdk 24).\n" +
  "Genera un projecte Gradle-Kotlin complet amb AQUESTA estructura:\n" +
  "  settings.gradle.kts, build.gradle.kts (arrel), gradle.properties\n" +
  "  app/build.gradle.kts, app/proguard-rules.pro\n" +
  "  app/src/main/AndroidManifest.xml\n" +
  "  app/src/main/res/values/strings.xml, themes.xml\n" +
  "  app/src/main/kotlin/<package>/MainActivity.kt  (composable @Composable)\n" +
  "  app/src/main/kotlin/<package>/ui/… (pantalles, components)\n" +
  "  app/src/main/kotlin/<package>/data/… (models i repositoris)\n" +
  "  README.md (instruccions per obrir amb Android Studio)\n" +
  "Regles EXTROGES:\n" +
  "  • Usa Material 3 (androidx.compose.material3) i Compose BOM.\n" +
  "  • Totes les rutes han de ser vàlides RELATIVES a l'arrel del projecte.\n" +
  "  • NO generes .aar ni binaris. NOMÉS codi font, .kts/.kt/.xml/.md.\n" +
  "  • Inclou un MainActivity amb «setContent { … }» executable.\n" +
  "  • No usis Flutter, React Native, Capacitor, Ionic ni cap altre embolcall.";

const PLATFORM_IOS =
  "PLATAFORMA OBJECTIU: iOS NATIU (Swift 5.10 + SwiftUI, iOS 17).\n" +
  "Genera un projecte Swift Package executable (estructura que Xcode pot obrir i\n" +
  "compilar) amb AQUESTS fitxers:\n" +
  "  Package.swift  (o Project.xcodeproj/project.pbxproj si ho saps fer)\n" +
  "  Sources/<App>/<App>App.swift  (@main struct … : App)\n" +
  "  Sources/<App>/ContentView.swift\n" +
  "  Sources/<App>/Views/… (pantalles i components)\n" +
  "  Sources/<App>/Models/… (structs + ObservableObject)\n" +
  "  Resources/Assets.xcassets/AppIcon.appiconset/Contents.json\n" +
  "  Resources/Info.plist\n" +
  "  README.md (com obrir-lo a Xcode)\n" +
  "Regles EXTROGES:\n" +
  "  • SwiftUI: res de Storyboard, res d'UIKit excepte UISViewControllerRepresentable puntual.\n" +
  "  • Rutes relatives a l'arrel del workspace (Package.swift a dalt).\n" +
  "  • No generes .ipa, .xcarchive ni binaris. NOMÉS .swift/.plist/.json/.md.\n" +
  "  • No usis Flutter, React Native, Capacitor, Ionic ni cap altre embolcall.";

/// Nota curta que acompanya el context del projecte: demana al model que
/// s'hi basi i responga en català. S'usa només quan «Inclou el codi» és actiu.
const CONTEXT_SYSTEM =
  "T'hem passat els fitxers reals del projecte obert com a context. Respon " +
  "en català i BASANT-TE EN AQUEST CODI (no inventes estructures ni fitxers " +
  "que no hi apareixen). QUAN GENERIS O MODIFIQUIS CODI, emet cada fitxer en " +
  "format «@file: ruta_relativa» amb la RUTA COMPLETA de les seves carpetes " +
  "(p. ex. @file: src/components/Boto.tsx), perquè NoOrbit creï les carpetes " +
  "i apliqui el fitxer AUTOMÀTICAMENT. Si cal una carpeta sense fitxer, " +
  "afegeix una línia «@dir: <ruta>». Si no pots generar un fitxer sencer, " +
  "marca-ho amb una línia «TODO: …».";

/// Converteix una llista de rutes (del pla de reserva) en camins nets.
function parseManifest(raw: string): string[] {
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
function parseFileBlocks(raw: string): { path: string; content: string }[] {
  const lines = raw.split(/\r?\n/);
  const acc: { path: string; lines: string[] }[] = [];
  let cur: { path: string; lines: string[] } | null = null;
  for (const line of lines) {
    const m = line.match(/^\s*@?file:\s*(.+?)\s*$/i);
    if (m) {
      if (cur) acc.push(cur);
      cur = { path: m[1].replace(/^[`'"]+|[`'"]+$/g, "").trim(), lines: [] };
      continue;
    }
    if (cur) {
      if (/^\s*```/.test(line)) continue; // ignora les tanques de codi
      cur.lines.push(line);
    }
  }
  if (cur) acc.push(cur);
  return acc
    .filter((f) => f.path.length > 0)
    .map((f) => ({
      path: f.path,
      content: f.lines.join("\n").replace(/^\s*\n+/, "").replace(/\s+$/, "") + "\n",
    }));
}

/// Extreu les directrius de CARPETA que la IA emet per crear estructura buida
/// (carpetes sense fitxer). Formes tolerades: «@dir: ruta», «@carpeta: ruta»,
/// «@folder: ruta», «@directori: ruta». S'usen quan la IA vol la gestió de
/// carpetes minuciosa: crea la carpeta encara que no contingui cap fitxer.
function parseDirDirectives(raw: string): string[] {
  const out: string[] = [];
  for (const line of raw.split(/\r?\n/)) {
    const m = line.match(/^\s*@?\s*(?:dir|directori|carpeta|folder)\w*:\s*(.+?)\s*$/i);
    if (!m) continue;
    let d = m[1]
      .replace(/^[`'"]+|[`'"]+$/g, "")
      .trim()
      .replace(/^\/+/, "")
      .replace(/\\/g, "/");
    if (!d) continue;
    // Si per error posa un fitxer (sense «/») amb extensió, no és una carpeta.
    if (/\.[A-Za-z0-9]{1,6}$/.test(d) && !d.includes("/")) continue;
    if (!d.split("/").includes("..")) out.push(d);
  }
  return Array.from(new Set(out));
}

/// Rescata els blocs de codi (``` ...) d'una resposta en prosa i en dedueix
/// el nom de fitxer: primer un «xxx.py» mencionat just abans del bloc; si no,
/// un nom automàtic segons el llenguatge. Així un tutorial explicat es pot
/// convertir en fitxers reals amb codi, ni que la IA no use el format @file:.
function extractFromProse(raw: string): { path: string; content: string }[] {
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
    Object.values(LANG_EXT).concat(["md", "txt", "cfg", "ini", "toml", "yml", "yaml", "json", "lock", "env"])
  );
  const fenceRe = /```([A-Za-z0-9]*)\n([\s\S]*?)```/g;
  let m: RegExpExecArray | null;
  let i = 0;
  while ((m = fenceRe.exec(raw)) !== null) {
    i++;
    const lang = (m[1] || "").toLowerCase();
    const content = m[2].replace(/\s+$/, "") + "\n";
    if (!content.trim()) continue;
    // Text just abans del bloc: pot contenir el nom del fitxer.
    const before = raw.slice(Math.max(0, m.index - 320), m.index);
    const named = before.match(/[A-Za-z0-9_./-]+\.[A-Za-z0-9]{1,6}/g);
    let path = "";
    if (named) {
      // Agafa l'últim candidat amb una extensió RECONEGUDA (evita «e.g», «3.4»).
      const cand = [...named].reverse().find((c) => {
        const ext = c.includes(".") ? c.split(".").pop()!.toLowerCase() : "";
        return ext.length >= 1 && knownExts.has(ext) && !/^[\d.]+$/.test(c);
      });
      if (cand) path = cand;
    }
    if (!path) path = `bloc_${i}.${LANG_EXT[lang] || "txt"}`;
    path = path.replace(/^\/+/, "").replace(/\\/g, "/");
    if (path.split("/").some((s) => s === "..")) continue;
    // Rutes duplicades: afegeix un sufix numèric abans de l'extensió.
    let uniq = path;
    let k = 2;
    while (used.has(uniq)) {
      uniq = /\.[^.]+$/.test(path)
        ? path.replace(/(\.[^.]+)$/, `_${k}$1`)
        : `${path}_${k}`;
      k++;
    }
    used.add(uniq);
    out.push({ path: uniq, content });
  }
  return out;
}

/// Extensions considerades «de fitxer» a l'hora de reconèixer rutes en prosa.
const REC_EXT = new Set([
  "py", "js", "jsx", "ts", "tsx", "json", "md", "txt", "css", "scss", "html",
  "toml", "yaml", "yml", "ini", "cfg", "env", "lock", "rs", "go", "java",
  "c", "h", "cpp", "hpp", "sh", "sql", "csv", "xml", "svg", "glb", "fbx",
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

interface AgentResult {
  text: string;
  files: string[];
  steps: { label: string; detail: string; ok: boolean }[];
}

/// Automaterialització: si la resposta de la IA conté codi amb NOMS de fitxers
/// reals (blocs @file: o ``` amb nom), els escriu SOLA al projecte obert
/// (creant també l'estructura de carpetes). Sense clics de l'usuari.
/// Retorna un resum llegible (què ha canviat + tasques pendents) o null.
async function autoMaterialize(raw: string): Promise<string | null> {
  const ws = useWorkspaceStore.getState();
  const root = ws.root;
  if (!root || !raw) return null;

  let files = parseFileBlocks(raw);
  if (files.length === 0) {
    // Accepta només blocs amb nom de fitxer real; els «bloc_N» automàtics
    // es queden per al botó manual (evita fitxers brossa en respostes normals).
    files = extractFromProse(raw).filter((f) => !/^bloc_\d+\./.test(f.path));
  }
  // Carpetes que la IA ha demanat explícitament (estructura buida, sense fitxer).
  const dirReqs = parseDirDirectives(raw);
  if (files.length === 0 && dirReqs.length === 0) return null;

  const base = root.replace(/\/+$/, "");
  const written: string[] = [];
  const createdDirs: string[] = [];
  const pending: string[] = [];

  // 1) Crea primer les carpetes declarades (fins i tot si van buides).
  for (const d of dirReqs) {
    try {
      await invoke("create_dir", { path: `${base}/${d}` });
      createdDirs.push(d);
    } catch {
      /* si falla, continua */
    }
  }

  // 2) Escriu els fitxers; «write_file» ja fa «create_dir_all» del pare, així
  //    que qualsevol subcarpeta implícita en la ruta (p. ex. src/components/)
  //    es crea també. Recollim aquestes carpetes-estructura per al resum.
  const structure = new Set<string>(createdDirs);
  for (const f of files) {
    const rel = f.path.replace(/^\/+/, "").replace(/\\/g, "/");
    if (rel.split("/").some((s) => s === "..")) continue;
    try {
      await invoke("write_file", { path: `${base}/${rel}`, content: f.content });
      written.push(rel);
      const segs = rel.split("/");
      segs.pop(); // descarta el nom del fitxer: la resta són carpetes
      if (segs.length > 0) structure.add(segs.join("/"));
      if (!f.content.trim() || /pendent d'implementar/i.test(f.content)) pending.push(rel);
    } catch {
      /* si un fitxer falla, continua amb la resta */
    }
  }
  if (written.length === 0 && createdDirs.length === 0) return null;
  await ws.refreshTree();

  // Tasques que la pròpia IA ha marcat com a pendents al text (TODO, etc.).
  const todos = raw
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter(
      (l) => /\b(TODO|FIXME|pendent|per a?fer|cal implementar|to-?do)\b/i.test(l) && l.length <= 160
    )
    .slice(0, 6);

  const parts: string[] = [];
  const dirsList = Array.from(structure).sort();
  if (dirsList.length > 0) {
    parts.push(
      `🗂 Estructura de carpetes creada (${dirsList.length}):\n` +
        dirsList.map((p) => `• ${p}/`).join("\n")
    );
  }
  if (written.length > 0) {
    parts.push(
      `📁 Aplicats automàticament al projecte (${written.length} fitxers):\n` +
        written.map((p) => `• ${p}`).join("\n")
    );
  }
  const pend = Array.from(new Set([...pending, ...todos]));
  if (pend.length > 0) {
    parts.push(`⏳ Tasques pendents:\n` + pend.map((p) => `• ${p}`).join("\n"));
  }
  return parts.join("\n\n");
}

// ── MATERIALITZACIÓ «EN VIU» DURANT EL STREAMING ────────────────────────────
// Mentre la IA encara està escrivint la resposta (fragments «ai://chunk»),
// ja anem materialitzant els blocs @file: que han quedat COMPLETS: cada fitxer
// s'escriu al disc i s'obre a l'editor, així l'usuari veu néixer —i/modificar-
// se— el codi en directe, no només quan la IA acaba. L'últim bloc (encara sense
// tancar) s'espera al pas següent; al final del torn, «autoMaterialize» reescriu
// el contingut definitiu i recull els blocs en prosa que aquí ignorem.

/// Últim contingut escrit en viu per ruta relativa: evita reescriure el
/// mateix fitxer a cada fragment i detecta què ha canviat.
const liveWritten = new Map<string, string>();
let liveTimer: ReturnType<typeof setTimeout> | null = null;
let liveBusy = false;
let livePendingRaw: string | null = null;

/// Restaura l'estat d'un torn NOU (cridat des de «beginTurn»).
export function resetLiveMaterialize() {
  liveWritten.clear();
  if (liveTimer) clearTimeout(liveTimer);
  liveTimer = null;
  liveBusy = false;
  livePendingRaw = null;
}

/// Escriu al disc + editor els blocs @file: JA tancats que hagin canviat.
/// Throttled: si ja hi ha una escriptura en curs, ajorna la nova passada.
function scheduleLiveMaterialize(raw: string) {
  livePendingRaw = raw;
  if (liveBusy || liveTimer) return;
  liveTimer = setTimeout(() => {
    liveTimer = null;
    void runLiveMaterialize();
  }, 350);
}

async function runLiveMaterialize() {
  const raw = livePendingRaw;
  livePendingRaw = null;
  if (!raw) return;
  const ws = useWorkspaceStore.getState();
  const root = ws.root;
  if (!root) return;
  const base = root.replace(/\/+$/, "");
  // Només blocs estrictes @file: (els de prosa podrien ser prosa incompleta).
  const blocks = parseFileBlocks(raw);
  if (blocks.length === 0) return;
  liveBusy = true;
  let anyNew = false;
  try {
    for (const f of blocks) {
      const rel = f.path.replace(/^\/+/, "").replace(/\\/g, "/");
      if (!rel || rel.split("/").some((s) => s === "..")) continue;
      if (liveWritten.get(rel) === f.content) continue; // sense canvis
      try {
        await useWorkspaceStore.getState().writeLive(`${base}/${rel}`, f.content);
        liveWritten.set(rel, f.content);
        anyNew = true;
      } catch {
        /* si un fitxer falla, continua amb la resta */
      }
    }
    if (anyNew) await useWorkspaceStore.getState().refreshTree();
  } finally {
    liveBusy = false;
  }
  // Si han arribat fragments mentre escrivíem, refar amb l'últim text.
  if (livePendingRaw) scheduleLiveMaterialize(livePendingRaw);
}

interface PersistedChat {
  id?: string;
  title?: string;
  createdAt?: number;
  parentId?: string | null;
  originIndex?: number | null;
  provider?: string | null;
  model?: string | null;
  expertId?: string;
  platform?: "desktop" | "android" | "ios";
  includeContext?: boolean;
  messages?: ChatMessage[];
}

interface AgentState {
  backends: BackendInfo[];
  checking: boolean;
  /// Tots els xats oberts: cadascun treballa en paral·lel, amb la seva IA.
  sessions: ChatSession[];
  activeId: string;

  refreshBackends: () => Promise<void>;

  // ── Gestió de xats ─────────────────────────────────────────────────────
  /// Obre un xat NOU (pot tenir la seva IA des del primer moment).
  newChat: (over?: Partial<ChatSession>) => string;
  /// FORK: duplica la conversa fins al missatge `upToIndex` en un xat nou,
  /// que pot continuar el treball amb una altra IA sense tocar l'original.
  forkChat: (id: string, upToIndex: number) => string;
  closeChat: (id: string) => Promise<void>;
  setActive: (id: string) => void;
  setChatProvider: (id: string, provider: string | null) => void;
  setChatModel: (id: string, model: string | null) => void;
  setChatExpert: (id: string, expertId: string) => void;
  setChatPlatform: (id: string, platform: "desktop" | "android" | "ios") => void;
  setIncludeContext: (id: string, v: boolean) => void;
  persistChats: () => void;

  // ── Accions de treball (id = xat actiu si s'omet) ──────────────────────
  run: (prompt: string, modality: Modality, id?: string) => Promise<string | null>;
  plan: (prompt: string, modality: Modality, id?: string) => Promise<string | null>;
  quick: (prompt: string, id?: string) => Promise<string | null>;
  /// Acció principal del xat: enruta a l'expert actiu o a l'agent de text,
  /// i processa la cua de missatges pendents d'AQUELL xat al acabar.
  send: (prompt: string, id?: string) => Promise<string | null>;
  enqueue: (prompt: string, id?: string) => void;
  dequeue: (index: number, id?: string) => void;
  apply: (target: ApplyTarget, prompt: string, id?: string) => Promise<string | null>;
  scaffold: (prompt: string, id?: string) => Promise<string | null>;
  materialize: (text: string, id?: string) => Promise<string | null>;
  createStructure: (lines: string[], id?: string) => Promise<string | null>;
  /// Pas de progrés rebut en temps real (event «agent://progress» d'un xat).
  pushStep: (sessionId: string, step: { label: string; detail: string; ok: boolean }) => void;
  /// Fragment de text en directe (event «ai://chunk» etiquetat amb el xat).
  /// A més d'acumular-lo al xat, materialitza EN VIU els fitxers ja completes.
  appendStream: (sessionId: string, chunk: string) => void;
  buildContext: (id?: string, limit?: number) => Promise<string>;
  beginTurn: (id: string, text: string) => void;
  finalizeTurn: (id: string) => void;
  clearChat: (id?: string) => void;
  /// Atura la generació d'un xat (o de tots, si no se n'indica cap).
  stop: (id?: string) => Promise<void>;
  clearTimeline: (id?: string) => void;
}

/// Carrega els xats desats; si no n'hi ha cap encara, obre el xat «main».
function loadChats(): { sessions: ChatSession[]; activeId: string } {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (raw) {
      const data = JSON.parse(raw) as { activeId?: string; sessions?: PersistedChat[] };
      const list = (data.sessions ?? [])
        .filter((s) => !!s.id)
        .map((s) =>
          blankSession(String(s.id), {
            ...s,
            title: s.title ?? "",
            createdAt: s.createdAt ?? Date.now(),
            includeContext: s.includeContext ?? defaultCtx(),
            messages: (s.messages ?? []).map((m) => ({ ...m })),
            // L'estat transitori no es restaura: en reobrir l'app res no genera.
            running: false,
            stopped: false,
            startedAt: null,
            stream: null,
          })
        );
      if (list.length > 0) {
        const activeId = list.some((s) => s.id === data.activeId) ? String(data.activeId) : list[0].id;
        return { sessions: list, activeId };
      }
    }
  } catch {
    /* sense persistència: comencem amb un xat nou */
  }
  return { sessions: [blankSession(DEFAULT_ID)], activeId: DEFAULT_ID };
}

const initialChats = loadChats();

export const useAgentStore = create<AgentState>((set, get) => {
  /// Aplicar un canvi al xat `sid` (si ja no existeix, es ignora: pot haver
  /// estat tancat mentre la IA responia).
  const patch = (sid: string, p: Partial<ChatSession> | ((s: ChatSession) => Partial<ChatSession>)) =>
    set((st) => ({
      sessions: st.sessions.map((s) => (s.id === sid ? { ...s, ...(typeof p === "function" ? p(s) : p) } : s)),
    }));

  const sessOf = (sid: string): ChatSession | undefined =>
    get().sessions.find((s) => s.id === sid);

  /// Xat destinatari quan el cridaner no n'indica cap: l'actiu.
  const target = (id?: string): string => id ?? get().activeId;

  const failTurn = (sid: string, e: unknown) => {
    const cancelled = isCancelled(e);
    const s = sessOf(sid);
    patch(sid, {
      running: false,
      stopped: false,
      startedAt: null,
      error: cancelled ? null : toErrorText(e),
      lastResult: cancelled
        ? s?.stream
          ? s.stream + "\n\n_(generació aturada)_"
          : "(generació aturada)"
        : null,
    });
    get().finalizeTurn(sid);
  };

  /// Després de tancar un torn, engega el següent missatge en cua del xat.
  const drainQueue = async (sid: string) => {
    const s = sessOf(sid);
    if (!s || s.running || s.queue.length === 0) return;
    const next = s.queue[0];
    patch(sid, { queue: s.queue.slice(1) });
    await get().send(next, sid);
  };

  // ── Reintent AUTOMÀTIC quan la IA es queda sense memòria/context ─────────
  // «invokeFn(level)» rep un ENTER de 0 (ple) a 3 (mínim). El helper prova
  // 0 → 1 → 2 → 3 fins que un funcione o s'acaben els nivells. Abans de
  // cada reintent: NETEJA el stream del xat (perquè l'usuari no veja el text
  // mig de l'intent anterior barrejat) i emet un pas visible a la línia de
  // temps. Si tots fallen, es rellança l'últim error perquè el cridaner el
  /// el traduisca amb «toErrorText».
  const runWithOOMRetry = async <T,>(
    sid: string,
    invokeFn: (level: number) => Promise<T>
  ): Promise<T> => {
    let lastErr: unknown = null;
    for (let level = 0; level <= 3; level++) {
      try {
        if (level > 0) {
          // Netegem el stream acumulat i afegim un pas explicatiu.
          patch(sid, { stream: null });
          get().pushStep(sid, {
            label: "memòria",
            detail:
              level === 1
                ? "⚠️ El context era massa gran: reintentant amb context REDUÏT (≈ 1.500 caràcters)…"
                : level === 2
                ? "⚠️ Continua sense prou memòria: reintentant SENSE adjuntar el codi del projecte…"
                : "⚠️ Últim recurs: reintentant SENSE HISTORIAL de conversa…",
            ok: true,
          });
        }
        return await invokeFn(level);
      } catch (e) {
        lastErr = e;
        if (!isRetriableError(e) || isCancelled(e)) throw e;
        // Si l'usuari ha premut «Atura» mentrestant, ixem del bucle.
        if (sessOf(sid)?.stopped) throw e;
      }
    }
    throw lastErr ?? new Error("Reintents esgotats");
  };

  return {
    backends: [],
    checking: false,
    sessions: initialChats.sessions,
    activeId: initialChats.activeId,

    refreshBackends: async () => {
      set({ checking: true });
      try {
        const backends = await invoke<BackendInfo[]>("agent_check_backends");
        set({ backends, checking: false });
      } catch {
        set({ checking: false });
      }
    },

    // ── Gestió de xats ───────────────────────────────────────────────────

    newChat: (over) => {
      const id = newId();
      const base = sessOf(get().activeId);
      // Un xat nou hereta l'IA triada al xat des d'on s'ha premel «+»:
      // així es pot obrir un altre front de treball amb la mateixa configuració.
      const s = blankSession(id, {
        provider: base?.provider ?? null,
        model: base?.model ?? null,
        expertId: base?.expertId ?? "",
        includeContext: base?.includeContext ?? defaultCtx(),
        ...over,
      });
      set((st) => ({ sessions: [...st.sessions, s], activeId: id }));
      get().persistChats();
      return id;
    },

    forkChat: (id, upToIndex) => {
      const src = sessOf(id);
      if (!src) return id;
      const nid = newId();
      const head = src.messages.slice(0, Math.max(0, upToIndex + 1));
      const s = blankSession(nid, {
        title: `⑂ ${src.title || "Xat"}`,
        parentId: src.id,
        originIndex: upToIndex,
        provider: src.provider,
        model: src.model,
        expertId: src.expertId,
        includeContext: src.includeContext,
        messages: head.map((m) => ({ ...m })),
      });
      set((st) => ({ sessions: [...st.sessions, s], activeId: nid }));
      get().persistChats();
      return nid;
    },

    closeChat: async (id) => {
      const s = sessOf(id);
      if (!s) return;
      if (s.running) {
        try {
          await invoke("agent_stop", { session: id });
        } catch {
          /* si no hi ha res en curs, no passa res */
        }
      }
      set((st) => {
        const rest = st.sessions.filter((x) => x.id !== id);
        if (rest.length === 0) rest.push(blankSession(DEFAULT_ID));
        const activeId = st.activeId === id ? rest[rest.length - 1].id : st.activeId;
        return { sessions: rest, activeId };
      });
      get().persistChats();
    },

    setActive: (id) => {
      set({ activeId: id });
      get().persistChats();
    },

    setChatProvider: (id, provider) => {
      patch(id, { provider: provider && provider !== "global" ? provider : null });
      get().persistChats();
    },

    setChatModel: (id, model) => {
      patch(id, { model: model && model.trim() ? model : null });
      get().persistChats();
    },

    setChatExpert: (id, expertId) => {
      patch(id, { expertId });
      get().persistChats();
    },

    setChatPlatform: (id, platform) => {
      patch(id, { platform });
      get().persistChats();
    },

    setIncludeContext: (id, v) => {
      patch(id, { includeContext: v });
      try {
        localStorage.setItem("noorbit.ctx", v ? "1" : "0");
      } catch {
        /* sense persistència */
      }
      get().persistChats();
    },

    persistChats: () => {
      try {
        const st = get();
        const sessions = st.sessions.map((s) => ({
          id: s.id,
          title: s.title,
          createdAt: s.createdAt,
          parentId: s.parentId,
          originIndex: s.originIndex,
          provider: s.provider,
          model: s.model,
          expertId: s.expertId,
          platform: s.platform,
          includeContext: s.includeContext,
          // Només els últims missatges, cada un acotat: la conversa completa
          // podria excedir la quota del magatzem local.
          messages: s.messages.slice(-30).map((m) => ({
            role: m.role,
            text: m.text.slice(0, 8000),
            thinking: m.thinking ? String(m.thinking).slice(0, 2000) : null,
            steps: (m.steps ?? []).slice(0, 8),
            elapsedMs: m.elapsedMs ?? null,
            at: m.at,
            error: m.error ?? false,
          })),
        }));
        localStorage.setItem(LS_KEY, JSON.stringify({ activeId: st.activeId, sessions }));
      } catch {
        /* sense persistència */
      }
    },

    // ── Accions de treball ───────────────────────────────────────────────

    run: async (prompt, modality, id) => {
      const sid = target(id);
      const t0 = Date.now();
      patch(sid, {
        running: true,
        error: null,
        stopped: false,
        lastThinking: null,
        startedAt: t0,
        lastElapsedMs: null,
      });
      try {
        get().pushStep(sid, { label: "agent", detail: "Rep l'ordre i planifica què fer…", ok: true });
        const res = await invoke<AgentResult>("agent_run", {
          input: { prompt, modality, workspace: null, session: sid },
        });
        const thinking = await fetchThinking(sid);
        set((st) => ({
          sessions: st.sessions.map((s) =>
            s.id === sid
              ? {
                  ...s,
                  running: false,
                  startedAt: null,
                  lastElapsedMs: Date.now() - t0,
                  lastResult: res.text,
                  lastThinking: thinking,
                  timeline: [...s.timeline, ...res.steps.map((st2) => ({ ...st2, at: Date.now() }))],
                }
              : s
          ),
        }));
        return res.text;
      } catch (e) {
        const cancelled = isCancelled(e);
        const s = sessOf(sid);
        patch(sid, {
          running: false,
          stopped: false,
          startedAt: null,
          lastElapsedMs: Date.now() - t0,
          error: cancelled ? null : toErrorText(e),
          lastResult: cancelled
            ? s?.stream
              ? s.stream + "\n\n_(generació aturada)_"
              : "(generació aturada)"
            : null,
        });
        return null;
      }
    },

    plan: async (prompt, modality, id) => {
      const sid = target(id);
      get().beginTurn(sid, prompt);
      const t0 = Date.now();
      patch(sid, {
        running: true,
        error: null,
        stopped: false,
        lastThinking: null,
        startedAt: t0,
        lastElapsedMs: null,
      });
      try {
        get().pushStep(sid, { label: "agent", detail: "Preparant el pla de treball…", ok: true });
        const res = await invoke<AgentResult>("agent_plan", {
          input: { prompt, modality, workspace: null, session: sid },
        });
        const thinking = await fetchThinking(sid);
        set((st) => ({
          sessions: st.sessions.map((s) =>
            s.id === sid
              ? {
                  ...s,
                  running: false,
                  startedAt: null,
                  lastElapsedMs: Date.now() - t0,
                  lastResult: res.text,
                  lastThinking: thinking,
                  timeline: [
                    ...s.timeline,
                    { label: "plan", detail: "Pla generat", ok: true, at: Date.now() },
                  ],
                }
              : s
          ),
        }));
        get().finalizeTurn(sid);
        return res.text;
      } catch (e) {
        failTurn(sid, e);
        return null;
      }
    },

    quick: async (prompt, id) => {
      const sid = target(id);
      const s0 = sessOf(sid);
      if (!s0) return null;
      const history = toHistory(s0.messages);
      get().beginTurn(sid, prompt);
      const t0 = Date.now();
      patch(sid, {
        running: true,
        error: null,
        stopped: false,
        lastThinking: null,
        startedAt: t0,
        lastElapsedMs: null,
        stream: null,
      });
      try {
        // Passos visibles: l'usuari veu QUÈ està fent la IA en cada fase
        // (mateix tracte que tenen Blender/Unreal amb el seu progrés).
        if (sessOf(sid)?.includeContext) {
          get().pushStep(sid, {
            label: "ia",
            detail: "Recol·lectant el codi del projecte per adjuntar-lo com a context…",
            ok: true,
          });
        }
        const ctx = sessOf(sid)?.includeContext ? await get().buildContext(sid) : "";
        get().pushStep(sid, {
          label: "ia",
          detail:
            "La IA llegeix el missatge i prepara la resposta (en un equip lent pot trigar 1–3 minuts)…",
          ok: true,
        });
        const finalPrompt = ctx ? `${ctx}\n\n---\nPREGUNTA / TASCA:\n${prompt}` : prompt;
        // Primer pas en viu de la materialització: els fragments arriben per
        // «ai://chunk» (gestiona «appendStream»); aquí només cal l'escriptura
        // DEFINITIVA un cop acabat el text (netega blocs sense tancar).
        // Si l'intent falla per OOM/tall de context, NOORBIT reintenta
        // AUTOMÀTICAMENT: context REDUÏT → sense context → sense històrial.
        const text = await runWithOOMRetry(sid, async (level) => {
          let m = finalPrompt;
          let h = history;
          let sy: string | null = ctx ? CONTEXT_SYSTEM : null;
          if (level === 1) {
            const small = sessOf(sid)?.includeContext ? await get().buildContext(sid, 1500) : "";
            m = small ? `${small}\n\n---\nPREGUNTA / TASCA:\n${prompt}` : prompt;
            sy = small ? CONTEXT_SYSTEM : null;
          } else if (level === 2) {
            m = prompt;
            sy = null;
          } else if (level === 3) {
            m = prompt;
            h = [];
            sy = null;
          }
          return await invoke<string>("send_prompt_stream", {
            prompt: m,
            system: sy,
            session: sid,
            provider: s0.provider,
            model: s0.model,
            history: h,
          });
        });
        const thinking = await fetchThinking(sid);
        get().pushStep(sid, {
          label: "ia",
          detail: "Resposta rebuda; comprovant si cal crear o desar fitxers al projecte…",
          ok: true,
        });
        // Automaterialització: si la resposta conté fitxers amb nom, s'escriuen
        // sols al projecte i afegim el resum del que ha canviat + pendents.
        resetLiveMaterialize();
        const extra = await autoMaterialize(text);
        patch(sid, {
          running: false,
          startedAt: null,
          lastElapsedMs: Date.now() - t0,
          lastResult: extra ? text + "\n\n" + extra : text,
          lastThinking: thinking,
        });
        get().finalizeTurn(sid);
        return text;
      } catch (e) {
        failTurn(sid, e);
        return null;
      }
    },

    /// Atura la generació en curs D'AQUEST XAT: els altres segueixen.
    stop: async (id) => {
      const sid = target(id);
      patch(sid, { stopped: true, running: false, startedAt: null });
      try {
        await invoke("agent_stop", { session: sid });
      } catch {
        /* si no hi ha res en curs, no passa res */
      }
    },

    apply: async (targetName, prompt, id) => {
      const sid = target(id);
      get().beginTurn(
        sid,
        `${targetName === "blender" ? "Aplica a Blender" : "Aplica a Unreal"}: ${prompt}`
      );
      const t0 = Date.now();
      patch(sid, {
        running: true,
        error: null,
        stopped: false,
        lastThinking: null,
        startedAt: t0,
        lastElapsedMs: null,
        timeline: [], // el progrés d'aquesta tasca comença de zero
      });
      try {
        const cmd = targetName === "blender" ? "agent_apply_blender" : "agent_apply_unreal";
        const payload =
          targetName === "blender"
            ? { input: { prompt, session: sid } }
            : { input: { prompt, session: sid }, config: DEFAULT_UNREAL_CONFIG };
        const msg = await invoke<string>(cmd, payload);
        const thinking = await fetchThinking(sid);
        // Si el pont/Unreal no era obert, refresquem l'estat del panell.
        if (targetName === "blender") {
          import("./blenderLiveStore")
            .then((m) => m.useBlenderLiveStore.getState().refreshStatus())
            .catch(() => undefined);
        }
        patch(sid, {
          running: false,
          startedAt: null,
          lastElapsedMs: Date.now() - t0,
          lastResult: msg,
          lastThinking: thinking,
        });
        get().finalizeTurn(sid);
        return msg;
      } catch (e) {
        failTurn(sid, e);
        return null;
      }
    },

    /// Crea un projecte de veritat: demana a la IA tots els fitxers en format
    /// `@file:` i els escriu (creant carpetes) dins l'arrel del workspace.
    scaffold: async (prompt, id) => {
      const sid = target(id);
      const text = prompt.trim();
      if (!text) return null;
      const ws = useWorkspaceStore.getState();
      const root = ws.root;
      get().beginTurn(sid, `🗂️ Crea al projecte: ${text}`);
      const t0 = Date.now();
      patch(sid, {
        running: true,
        error: null,
        stopped: false,
        lastThinking: null,
        startedAt: t0,
        lastElapsedMs: null,
        timeline: [],
      });
      if (!root) {
        patch(sid, {
          running: false,
          startedAt: null,
          lastElapsedMs: Date.now() - t0,
          error:
            "Obre primer una carpeta de projecte (barra lateral ▸ «Obre carpeta») perquè l'agent hi cree els fitxers.",
        });
        get().finalizeTurn(sid);
        return null;
      }
      const s0 = sessOf(sid);
      const ia = {
        session: sid,
        provider: s0?.provider ?? null,
        model: s0?.model ?? null,
      };
      try {
        const base = root.replace(/\/+$/, "");
        const folder = root.split(/[/\\]/).pop() ?? root;
        const platform = s0?.platform ?? "desktop";
        const platformHint =
          platform === "android"
            ? "\n\nPLATAFORMA: Android NATIU (Kotlin + Jetpack Compose)."
            : platform === "ios"
            ? "\n\nPLATAFORMA: iOS NATIU (Swift + SwiftUI)."
            : "";
        const user = `${text}${platformHint}\n\nGenera tots els fitxers del projecte en el format @file: indicat.`;
        const systemForPlatform =
          platform === "android"
            ? SCAFFOLD_SYSTEM + "\n\n" + PLATFORM_ANDROID
            : platform === "ios"
            ? SCAFFOLD_SYSTEM + "\n\n" + PLATFORM_IOS
            : SCAFFOLD_SYSTEM;
        get().pushStep(sid, {
          label: "ia",
          detail:
            platform === "android"
              ? "Generant projecte Android NATIU (Kotlin/Compose) en directe…"
              : platform === "ios"
              ? "Generant projecte iOS NATIU (Swift/SwiftUI) en directe…"
              : "Generant tots els fitxers del projecte (en un equip lent pot trigar 1–3 minuts)…",
          ok: true,
        });
        const raw = await runWithOOMRetry(sid, async (level) => {
          // En el scaffold NO s'adjuntja el codi del projecte (no n'hi ha
          // encara); si falla per OOM, reduïm la llargada de la resposta
          // que exigim i, com a últim recurs, demanem un esquelet mínim.
          let promptForLevel = user;
          if (level === 1) {
            promptForLevel = user + "\n\nIMPORTANT: resposta BREU (màxim 3 fitxers petits) per no quedar sense memòria.";
          } else if (level === 2) {
            promptForLevel = user + "\n\nIMPORTANT: resposta MOLT BREU (només README.md + 1 fitxer mínim).";
          } else if (level === 3) {
            promptForLevel = user + "\n\nIMPORTANT: només un fitxer README.md amb el pla.";
          }
          return await invoke<string>("send_prompt_stream", {
            prompt: promptForLevel,
            system: systemForPlatform,
            ...ia,
            session: sid,
          });
        });
        get().pushStep(sid, { label: "ia", detail: "Resposta rebuda; n'extrec els fitxers…", ok: true });
        let files = parseFileBlocks(raw);
        let skeleton = false;

        // Capa 2: si el model no usa @file: però SÍ escriu codi en blocs ```
        // (tutorials en prosa), resquem els blocs i en fem fitxers reals.
        if (files.length === 0) {
          files = extractFromProse(raw);
        }

        // Pla de reserva: si el model no respecta «@file:», demana-li NOMÉS la
        // llista de rutes (tasca més senzilla) i en crea un esquelet real:
        // carpetes + fitxers buits + README.md. Així el projecte queda estructurat
        // fins i tot amb un model dèbil com el 1.5b.
        if (files.length === 0) {
          get().pushStep(sid, {
            label: "ia",
            detail: "El model no usa el format @file:; li demano només la llista de rutes…",
            ok: true,
          });
          const manifestRaw = await invoke<string>("send_prompt", {
            prompt: `${text}\n\nLlista NOMÉS les rutes relatives dels fitxers que cal crear, una per línia. Inclou sempre README.md.`,
            system: MANIFEST_SYSTEM,
            ...ia,
          });
          const paths = parseManifest(manifestRaw);
          if (paths.length > 0) {
            skeleton = true;
            if (!paths.some((p) => /README\.md$/i.test(p))) paths.push("README.md");
            files = paths.map((p) => ({
              path: p,
              content: /README\.md$/i.test(p)
                ? `# ${folder}\n\n${text}\n\n> Esquelet generat per NoOrbit. Omple el contingut de cada fitxer.\n`
                : `${p}\n\n(Generat per NoOrbit — pendent d'implementar)\n`,
            }));
          }
        }

        if (files.length === 0) {
          throw new Error(
            "El model actual no genera ni les rutes dels fitxers. Amb un model tan lleuger (1.5b) això és habitual: connecta un model més potent o un proveïdor remot amb token (p. ex. Venice AI)."
          );
        }

        get().pushStep(sid, {
          label: "projecte",
          detail: `Escrivint ${files.length} fitxers (i les seves carpetes) al projecte…`,
          ok: true,
        });
        const created: string[] = [];
        for (const f of files) {
          const rel = f.path.replace(/^\/+/, "").replace(/\\/g, "/");
          // Seguretat: mai eixir de l'arrel del projecte.
          if (rel.split("/").some((seg) => seg === "..")) continue;
          const abs = `${base}/${rel}`;
          await invoke("write_file", { path: abs, content: f.content });
          created.push(rel);
        }
        await ws.refreshTree();
        const head = skeleton
          ? `🗂️ Esquelet creat a «${folder}»: ${created.length} fitxers/carpetes (buits, per omplir — el model local és dèbil):\n`
          : `✅ Creats ${created.length} fitxers a «${folder}»:\n`;
        const summary = head + created.map((c) => `• ${c}`).join("\n");
        const thinking = await fetchThinking(sid);
        patch(sid, {
          running: false,
          startedAt: null,
          lastElapsedMs: Date.now() - t0,
          lastResult: summary,
          lastThinking: thinking,
        });
        get().finalizeTurn(sid);
        return summary;
      } catch (e) {
        failTurn(sid, e);
        return null;
      } finally {
        // Processa la cua d'aquest xat si encara queden missatges pendents.
        await drainQueue(sid);
      }
    },

    /// Agafa un text JA present al xat (amb codi en blocs ``` o @file:) i en
    /// crea fitxers reals al workspace, sense tornar a consultar la IA.
    materialize: async (text, id) => {
      const sid = target(id);
      const raw = (text || "").trim();
      if (!raw) return null;
      const ws = useWorkspaceStore.getState();
      const root = ws.root;
      get().beginTurn(sid, "📁 Convertir aquest missatge en fitxers");
      if (!root) {
        patch(sid, {
          error:
            "Obre primer una carpeta de projecte (barra lateral ▸ «Obre carpeta») perquè es puguin crear els fitxers.",
        });
        get().finalizeTurn(sid);
        return null;
      }
      try {
        const base = root.replace(/\/+$/, "");
        const folder = root.split(/[/\\]/).pop() ?? root;
        // Primer intentem el format estricte @file:; si no, rescatem blocs de codi.
        let files = parseFileBlocks(raw);
        if (files.length === 0) files = extractFromProse(raw);
        if (files.length === 0) {
          throw new Error(
            "Aquest missatge no conté cap bloc de codi (```) ni fitxers «@file:» que es puguin crear."
          );
        }
        get().pushStep(sid, {
          label: "projecte",
          detail: `Escrivint ${files.length} fitxers (i les seves carpetes) al projecte…`,
          ok: true,
        });
        const created: string[] = [];
        for (const f of files) {
          const rel = f.path.replace(/^\/+/, "").replace(/\\/g, "/");
          if (rel.split("/").some((seg) => seg === "..")) continue;
          const abs = `${base}/${rel}`;
          await invoke("write_file", { path: abs, content: f.content });
          created.push(rel);
        }
        await ws.refreshTree();
        const summary =
          `✅ Creats ${created.length} fitxers a «${folder}» des del missatge:\n` +
          created.map((c) => `• ${c}`).join("\n");
        patch(sid, { lastResult: summary, error: null });
        get().finalizeTurn(sid);
        return summary;
      } catch (e) {
        patch(sid, { error: toErrorText(e), lastResult: null });
        get().finalizeTurn(sid);
        return null;
      }
    },

    /// Creació manual d'estructura: l'usuari escriu (o edita de les
    /// recomanacions de la IA) una llista de rutes i les crea al projecte.
    /// Les línies acaben en «/» són CARPETES; la resta, FITXERS buits.
    /// Mai aixafa un fitxer existent: si ja hi és, el respecta i passa al següent.
    createStructure: async (lines, id) => {
      const sid = target(id);
      const ws = useWorkspaceStore.getState();
      const root = ws.root;
      get().beginTurn(sid, "🗂 Creant a mà fitxers i carpetes");
      if (!root) {
        patch(sid, {
          error:
            "Obre primer una carpeta de projecte (barra lateral ▸ «Obre carpeta») perquè es puguin crear els fitxers.",
        });
        get().finalizeTurn(sid);
        return null;
      }
      const base = root.replace(/\/+$/, "");
      const newFiles: string[] = [];
      const newDirs: string[] = [];
      const existed: string[] = [];
      const failed: string[] = [];
      for (const rawLine of lines) {
        const p = rawLine.trim().replace(/^\/+/, "").replace(/\\/g, "/");
        if (!p || p.split("/").includes("..")) continue;
        const abs = `${base}/${p}`;
        try {
          if (p.endsWith("/")) {
            await invoke("create_dir", { path: abs });
            newDirs.push(p);
          } else {
            // Si el fitxer ja existeix, NO l'aixafem (creació manual = completar).
            let exists = false;
            try {
              await invoke<string>("read_file", { path: abs });
              exists = true;
            } catch {
              /* no existeix: el podrem crear */
            }
            if (exists) {
              existed.push(p);
            } else {
              await invoke("write_file", { path: abs, content: "" });
              newFiles.push(p);
            }
          }
        } catch {
          failed.push(p);
        }
      }
      await ws.refreshTree();
      const parts: string[] = [];
      if (newDirs.length > 0)
        parts.push(`🗂 Carpetes creades (${newDirs.length}):\n` + newDirs.map((d) => `• ${d}`).join("\n"));
      if (newFiles.length > 0)
        parts.push(`📁 Fitxers creats (${newFiles.length}):\n` + newFiles.map((f) => `• ${f}`).join("\n"));
      if (existed.length > 0)
        parts.push(`↩️ Ja existien, no s'han tocat (${existed.length}):\n` + existed.map((f) => `• ${f}`).join("\n"));
      if (failed.length > 0)
        parts.push(`⚠️ No s'han pogut crear (${failed.length}):\n` + failed.map((f) => `• ${f}`).join("\n"));
      const summary =
        parts.join("\n\n") ||
        "No hi havia cap ruta vàlida a la llista (usa «carpeta/sub/>» per a carpetes i «ruta/fitxer.py» per a fitxers).";
      patch(sid, { lastResult: summary, error: null, lastElapsedMs: 0 });
      get().finalizeTurn(sid);
      return summary;
    },

    // Pas de progrés rebut via event "agent://progress" (etiquetat amb el xat).
    pushStep: (sessionId, step) => {
      if (!sessOf(sessionId)) return; // tasques d'altres orígens: ignorem
      set((s) => ({
        sessions: s.sessions.map((x) =>
          x.id === sessionId ? { ...x, timeline: [...x.timeline, { ...step, at: Date.now() }] } : x
        ),
      }));
    },

    // Fragment de text rebut via event "ai://chunk" del xat que genera.
    // Acumula AL XAT i, de pas, va escrivint els fitxers @file: ja acabats
    // perquè l'editor els mostre en directe (materialització «en viu»).
    appendStream: (sessionId, chunk) => {
      if (!sessOf(sessionId)) return;
      set((s) => ({
        sessions: s.sessions.map((x) =>
          x.id === sessionId ? { ...x, stream: (x.stream ?? "") + chunk } : x
        ),
      }));
      // Materialització en viu: només si hi ha projecte obert; ignora errors
      // (el torn final amb «autoMaterialize» torna a escriure el definitiu).
      if (useWorkspaceStore.getState().root) {
        const stream = sessOf(sessionId)?.stream ?? "";
        scheduleLiveMaterialize(stream);
      }
    },

    // Llegeix els fitxers reals del projecte obert i en forma un bloc de context
    // per a la IA. Buit si no hi ha workspace o cap fitxer llegible.
    // `limit` (caràcters): 4.500 per defecte; 1.200-2.000 en reintents.
    buildContext: async (id, limit) => {
      const sid = target(id);
      const ws = useWorkspaceStore.getState();
      if (!ws.root) {
        patch(sid, { contextInfo: "⚠️ No hi ha cap projecte obert: la IA no està veient codi." });
        return "";
      }
      try {
        // Límit ajustable: en equips lleugers (CPU, 8 GB) un prompt gran fa que
        // la PRIMERA resposta trigue minuts. 4.500 caràcters ≈ 1.500 tokens.
        const files = await invoke<{ path: string; content: string }[]>(
          "collect_project_files",
          { limit: limit ?? 4500 }
        );
        if (!files || files.length === 0) {
          patch(sid, { contextInfo: "⚠️ No he trobat fitxers de codi a la carpeta oberta." });
          return "";
        }
        const chars = files.reduce((n, f) => n + f.content.length, 0);
        patch(sid, {
          contextInfo: `📁 ${files.length} fitxers (${chars} caràcters) s'adjunten a cada missatge. En un equip lent, la primera resposta pot trigar 1–3 minuts.`,
        });
        const tree = files.map((f) => f.path).join("\n");
        const body = files.map((f) => `===== ${f.path} =====\n${f.content}`).join("\n\n");
        return (
          `A continuació tens ELS FITXERS REALS del projecte obert (workspace).\n` +
          `RUTES:\n${tree}\n\nCONTINGUT:\n${body}`
        );
      } catch (e) {
        patch(sid, { contextInfo: `⚠️ Error llegint el projecte: ${String(e)}` });
        return "";
      }
    },

    enqueue: (prompt, id) => {
      const sid = target(id);
      const text = prompt.trim();
      if (!text) return;
      set((s) => ({
        sessions: s.sessions.map((x) => (x.id === sid ? { ...x, queue: [...x.queue, text] } : x)),
      }));
    },

    dequeue: (index, id) => {
      const sid = target(id);
      set((s) => ({
        sessions: s.sessions.map((x) =>
          x.id === sid ? { ...x, queue: x.queue.filter((_, i) => i !== index) } : x
        ),
      }));
    },

    // Acció principal del xat. Amb un especialista seleccionat, la tasca la
    // resol ell (rol + model propis); si no, l'agent de text. Cada xat porta
    // la seva IA (proveïdor/model propis) i el seu fil (history): així dos
    // xats poden treballar en paral·lel i un fork continua la feina.
    send: async (prompt, id) => {
      const sid = target(id);
      const sess = sessOf(sid);
      const text = prompt.trim();
      if (!sess || !text) return null;
      // El fil precedent viatja amb cada missatge: el model sap què s'ha dit
      // (essencial per als forks i per continuar el treball).
      const history = toHistory(sess.messages);
      get().beginTurn(sid, text);

      let msg = text;
      let sys: string | null = null;
      // Pista de plataforma: s'afegeix al system prompt perquè els xats
      // normals (no només «Crea al projecte») també generin codi NATIU.
      const platformPreamble =
        sess.platform === "android"
          ? "PLATAFORMA OBJECTIU: Android NATIU. Escriu exclusivament Kotlin + Jetpack Compose dins l'estructura Gradle oficial (app/src/main/…). " +
            PLATFORM_ANDROID +
            "\n\n"
          : sess.platform === "ios"
          ? "PLATAFORMA OBJECTIU: iOS NATIU. Escriu exclusivament Swift + SwiftUI dins l'estructura de Xcode/SwiftPM (Sources/…, Resources/…). " +
            PLATFORM_IOS +
            "\n\n"
          : "";
      if (sess.includeContext) {
        get().pushStep(sid, {
          label: "ia",
          detail: "Recol·lectant el codi del projecte per adjuntar-lo com a context…",
          ok: true,
        });
        const ctx = await get().buildContext(sid);
        if (ctx) {
          msg = `${ctx}\n\n---\nPREGUNTA / TASCA:\n${text}`;
          sys = platformPreamble + CONTEXT_SYSTEM;
          get().pushStep(sid, {
            label: "ia",
            detail: sessOf(sid)?.contextInfo ?? "Context del projecte preparat.",
            ok: true,
          });
        }
      }

      // Si no hi havia context adjunt, `sys` encara és null: apliquem almenys
      // la pista de plataforma perquè el xat continue escrivint codi natiu.
      if (!sys && platformPreamble) sys = platformPreamble + CONTEXT_SYSTEM;

      // Reconstruïble: per cada nivell de reintent, munte el «prompt final»
      // amb MENYS context (o sense). S'usa dins «runWithOOMRetry».
      const rebuildForLevel = async (level: number): Promise<{ m: string; h: HistoryTurn[]; s: string | null }> => {
        if (level === 0) return { m: msg, h: history, s: sys };
        const sysBase = platformPreamble ? platformPreamble + CONTEXT_SYSTEM : (sess.includeContext ? CONTEXT_SYSTEM : null);
        if (level === 1) {
          // Context REDUÏT: limitem a ≈ 1.500 caràcters.
          const small = sess.includeContext ? await get().buildContext(sid, 1500) : "";
          const m = small ? `${small}\n\n---\nPREGUNTA / TASCA:\n${text}` : text;
          return { m, h: history, s: sysBase };
        }
        if (level === 2) {
          // Sense context adjunt: només el text de l'usuari.
          return { m: text, h: history, s: platformPreamble ? platformPreamble + CONTEXT_SYSTEM : null };
        }
        // Level 3: sense context I sense històrial.
        return { m: text, h: [], s: platformPreamble ? platformPreamble + CONTEXT_SYSTEM : null };
      };

      const expert = sess.expertId
        ? useExpertStore.getState().experts.find((e) => e.id === sess.expertId) ?? null
        : null;

      const t0 = Date.now();
      patch(sid, {
        running: true,
        error: null,
        stopped: false,
        lastThinking: null,
        startedAt: t0,
        lastElapsedMs: null,
        stream: null,
      });

      try {
        if (expert) {
          get().pushStep(sid, {
            label: `Especialista · ${expert.name}`,
            detail: "Està escrivint la resposta (el primer tram pot trigar mins en CPU)…",
            ok: true,
          });
          const out = await useExpertStore
            .getState()
            .runSolo(expert.id, msg, { session: sid, history });
          if (out !== null) {
            const thinking = await fetchThinking(sid);
            get().pushStep(sid, {
              label: `Especialista · ${expert.name}`,
              detail: "Resposta rebuda; comprovant si cal crear o desar fitxers…",
              ok: true,
            });
            resetLiveMaterialize();
            const extra = await autoMaterialize(out);
            patch(sid, {
              running: false,
              startedAt: null,
              lastElapsedMs: Date.now() - t0,
              lastResult: extra ? out + "\n\n" + extra : out,
              lastThinking: thinking,
            });
          } else {
            const err = useExpertStore.getState().error ?? "";
            if (err.includes("cancel·lat"))
              patch(sid, {
                running: false,
                startedAt: null,
                lastElapsedMs: Date.now() - t0,
                error: null,
                lastResult: "(generació aturada)",
              });
            else
              patch(sid, {
                running: false,
                startedAt: null,
                lastElapsedMs: Date.now() - t0,
                error: err,
              });
          }
        } else {
          // Cas normal sense expert: xat de text AMB streaming, perquè la UI
          // mostre el que la IA escriu en directe (event «ai://chunk»).
          get().pushStep(sid, {
            label: "ia",
            detail: "La IA processa el missatge i el context (en un equip lent pot trigar 1–3 minuts)…",
            ok: true,
          });
          const out = await runWithOOMRetry(sid, async (level) => {
            const built = await rebuildForLevel(level);
            return await invoke<string>("send_prompt_stream", {
              prompt: built.m,
              system: built.s,
              session: sid,
              provider: sess.provider,
              model: sess.model,
              history: built.h,
            });
          });
          const thinking = await fetchThinking(sid);
          get().pushStep(sid, {
            label: "ia",
            detail: "Resposta rebuda; comprovant si cal crear o desar fitxers al projecte…",
            ok: true,
          });
          resetLiveMaterialize();
          const extra = await autoMaterialize(out);
          patch(sid, {
            running: false,
            startedAt: null,
            lastElapsedMs: Date.now() - t0,
            lastResult: extra ? out + "\n\n" + extra : out,
            lastThinking: thinking,
          });
        }
      } catch (e) {
        const cancelled = isCancelled(e);
        const s = sessOf(sid);
        patch(sid, {
          running: false,
          stopped: false,
          startedAt: null,
          lastElapsedMs: Date.now() - t0,
          error: cancelled ? null : toErrorText(e),
          lastResult: cancelled
            ? s?.stream
              ? s.stream + "\n\n_(generació aturada)_"
              : "(generació aturada)"
            : null,
        });
      }

      // Tanca aquest torn abans de processar la cua (que en obrirà un de nou).
      const cur = sessOf(sid);
      const last = cur?.messages[cur.messages.length - 1];
      if (!last || last.role !== "assistant") get().finalizeTurn(sid);
      get().persistChats();

      // Cua: si aquest xat ja no està generant i li queden missatges, engega'n el següent.
      await drainQueue(sid);
      return sessOf(sid)?.lastResult ?? null;
    },

    beginTurn: (id, text) => {
      // Torn nou: llença l'estat de la materialització en viu (mapa de
      // fitxers ja escrits i reprogramacions pendents del torn anterior).
      resetLiveMaterialize();
      set((s) => ({
        sessions: s.sessions.map((x) =>
          x.id === id
            ? {
                ...x,
                messages: [...x.messages, { role: "user" as const, text, at: Date.now() }],
                // El primer missatge dona nom al xat (per a la barra de xats).
                title: x.title || text.slice(0, 48),
                timeline: [],
                stream: null,
                lastResult: null,
                lastThinking: null,
                lastElapsedMs: null,
                error: null,
              }
            : x
        ),
      }));
      get().persistChats();
    },

    finalizeTurn: (id) => {
      const g = sessOf(id);
      if (!g) return;
      const text = g.lastResult ?? g.error ?? "";
      if (!text && !g.lastThinking) return;
      set((s) => ({
        sessions: s.sessions.map((x) =>
          x.id === id
            ? {
                ...x,
                messages: [
                  ...x.messages,
                  {
                    role: "assistant" as const,
                    text,
                    thinking: g.lastThinking ?? null,
                    steps: [...g.timeline],
                    elapsedMs: g.lastElapsedMs,
                    at: Date.now(),
                    error: !g.lastResult && !!g.error,
                  },
                ],
              }
            : x
        ),
      }));
    },

    clearChat: (id) => {
      const sid = target(id);
      patch(sid, { messages: [], title: "" });
      get().persistChats();
    },

    clearTimeline: (id) => {
      const sid = target(id);
      patch(sid, { timeline: [] });
    },
  };
});

/// El xat actiu (o el primer si l'identificador encara no ha arribat).
export function useActiveChat(): ChatSession {
  return useAgentStore((s) => s.sessions.find((x) => x.id === s.activeId) ?? s.sessions[0]);
}

/// Veritat si QUALSEVOL xat està generant ara mateix (per a indicadors
/// globals com la barra d'estat).
export function useAnyChatRunning(): boolean {
  return useAgentStore((s) => s.sessions.some((x) => x.running));
}
