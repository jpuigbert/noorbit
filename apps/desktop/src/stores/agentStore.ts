import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DEFAULT_UNREAL_CONFIG } from "../core/unreal/client";
import { useExpertStore } from "./expertStore";
import { useTaskListStore } from "./taskListStore";
import { useWorkspaceStore } from "./workspaceStore";
import { lintOffline } from "../editor/diagnostics";
import {
  armAllMarks,
  captureOriginal,
  dropMark,
  noteFileEdited,
  noteLiveWriteContent,
  noteLiveWriteStart,
} from "./editReviewStore";

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
  /// Rutes absolutes d'imatges adjuntes o generades (es mostren al xat i
  /// es poden copiar; els models amb visió d'Ollama les reben com a entrada).
  images?: string[];
  /// Comptabilització de tokens del torn (entrada/sortida i context ocupat).
  /// Especialment útil amb les APIs de núvol: s'hi veu què costa cada
  /// resposta i quant del context s'ha consumit.
  usage?: TokenUsage;
}

/// Consum de tokens d'un torn, tal com l'informa el proveïdor.
export interface TokenUsage {
  provider: string;
  model: string;
  /// Tokens d'entrada (prompt + sistema + historial).
  input: number;
  /// Tokens generats.
  output: number;
  total: number;
  /// Finestra de context del model (tokens).
  contextWindow: number;
  /// Percentatge del context ocupat pels tokens d'entrada.
  contextUsedPct: number;
}

/// Extreu les rutes absolutes d'imatge (png/jpg/webp/gif/bmp) que apareixen
/// en un text: serveix perquè la IA (i l'usuari) vega les imatges que el
/// backend genera i desa a disc sense haver d'afegir cap format nou.
export function extractChatImagePaths(text: string): string[] {
  const re =
    /(\/[^\s"'`()<>]+?\.(?:png|jpe?g|webp|gif|bmp))|(?:[A-Za-z]:\\E[^\s"'`<>]+?\.(?:png|jpe?g|webp|gif|bmp))/gi;
  const found = text.match(re) ?? [];
  return Array.from(new Set(found));
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
  /// Projecte (bucket) a l'ambit d'aquest xat: els historials d'altres
  /// projectes no es mostren així aquí. Es posa al crear el xat i es
  /// reescriu en canviar de projecte.
  bucket: string;
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
    bucket: useWorkspaceStore.getState().root ?? "",
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
/// memòria dels models: els últims torns, cadascun retallat. Finestra àmplia
/// (16 torns) perquè el xat RECORDERA les consultes anteriors i responga en
/// conseqüència; cada torn conserva 12.000 caràcters —abans 6.000— perquè un
/// fitxer llegit en un torn anterior no arribe MEITAT tallat al següent; el
/// backend encara compacta més si la RAM del model no abasta.
function toHistory(messages: ChatMessage[]): HistoryTurn[] {
  return messages
    .filter((m) => m.text.trim() && !m.error)
    .slice(-16)
    .map((m) => ({ role: m.role, content: m.text.slice(0, 12000) }));
}

/// FIL MÍNIM per a l'últim recurs de memòria: abans el nivell 3 BORRAVA tota
/// la conversa — canviaves de IA, pujava el context, entrava el recurs… i el
/// model ja no sabia res: dir «continua» era inútil. Ara conserva els últims
/// 6 torns retallats: poc RAM, però el model SAB què s'estava fent.
function threadOf(history: HistoryTurn[]): HistoryTurn[] {
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
const LS_KEY = "noorbit.chats";

/// Clau del projecte obert ara mateix; sense projecte es fa servir un calaix
/// propi (ja no és el clau global antic, que era compartit per a TODO).
function chatLsKey(root: string | null): string {
  return root ? `${LS_KEY}::${root}` : `${LS_KEY}::__no_project__`;
}

/// Bucket (projecte) dels xats que tenim en memòria. Només es canvia amb
/// «switchChatBucket» — mai a mà.
let currentBucket: string | null = null;

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
    return "La memòria no ha abastat tot el context i la resposta s'ha tallat. NoOrbit ho ha reintentat AUTOMÀTICAMENT alliberant RAM i COMPACTANT el context (resum dens que conserva l'objectiu, el pla i l'esquelet del codi) → sense codi → sense historial, i ha tornat a fallar. Desactiva «Inclou el codi» o usa un model més lleuger.";
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
  "README.md) i, si el projecte té lògica, inclou proves reals dins tests/ " +
  "(o la carpeta de proves del llenguatge). No escriguis res més enllà de " +
  "blocs «@file:» i línies «@dir:».";

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

/// ORDRES CLARes D'ORDINADOR, en llenguatge natural (català d'aquest Mac):
/// comencen amb un verb del sistema, no tenen cap signe de pregunta i no
/// s'enfilan massa. Tota altra frase segueix al model de text com sempre.
/// El límit `@computer:? qualsevol cosa` engloba el cas general.
const COMPUTER_ORDER_RE =
  /^(?:obri?m?\b|obre\b|arrenca\b|engega\b|tanca\b|atura\b|mou\b|renomena\b|copia\b|esborra\b|elimina\b|mata\b|llista\b|repàs\b|fes\b[^?]{0,40}\bcaptura\b|crea\b[^?]{0,60}\bcarpeta\b)[^?]{0,160}$/i;

/// Nota que ensenya a la IA la directriu «@agent:» — el mateix mecanisme que
/// «@file:» però per CREAR agents especialistes (com els subagents de Qoder).
/// S'inclou en tots els system prompts de xat perquè el model sàpigua que pot
/// fer-ho i com ha de formatar la línia.
const AGENT_CREATION_SYSTEM =
  "AGENTS ESPECIALISTES QUE POTS CREAR: si la tasca és prou complexa o barreja " +
  "dominis (p. ex. frontend + base de dades + proves), pots DEMANAR que es cree " +
  "un agent especialista per a cada part, com un expert real. Emet una línia " +
  "per agent amb aquest format exacte:\n" +
  "@agent: Nom | Rol curt | Instruccions de sistema (què ha de fer, com, i en " +
  "quin llenguatge/format)\n" +
  "NoOrbit el crearà automaticament com un especialista persistent (panell " +
  "«Especialistes») que l'usuari podrà triar, llançar en solitari o posar en " +
  "equip. Crea'n NOMÉS si apporta valor real; reutilitza els que ja existeixen " +
  "(NoOrbit no en duplica cap pel nom). Pots omplir les 3 parts o només el " +
  "nom: «@agent: Revisor de seguretat».\n" +
  "DELEGAR EN ELS AGENTS (AJUDA'T D'ELLS): quan ja hi hagi especialistes " +
  "(els creats per tu o els de l'usuari), NO ho resolges tot tu sol. Emet una " +
  "línia per cada subtasca que volguessis encarregar a un agent amb aquest " +
  "format exacte:\n" +
  "@use: NomAgent | tasca concreta que ha de resoldre\n" +
  "NoOrbit llançarà ESE agent (un model local) perquè treballe en la seua " +
  "part ARA MATEIX, i et retornarà la seua solució ací mateix perquè la " +
  "CONSOLIDIS amb la resta del teu treball. Usa el NOM exacte de l'especialista. " +
  "Pots delegar diverses subtasques en diversos agents al mateix missatge.\n\n" +
  "LLISTA DE TASQUES (per no perdre't ni tu ni els agents): manté un checklist. " +
  "Quan identifiques una tasca pendient, afegeix-la amb una línia:\n" +
  "@todo: descripció curta de la tasca\n" +
  "Quan COMENCES a treballar-ne una, marca-la EN CURS amb una línia:\n" +
  "@doing: text de la tasca que fas ara (l'usuari veurà el progrés en viu a la llista)\n" +
  "Quan en completes una, marca-la amb:\n" +
  "@done: text exacte de la tasca completada\n" +
  "NoOrbit mostra eixa llista a tot el xat i la manté visible per a tots els " +
  "agents. Compleix-les abans de donar la tasca per acabada.\n\n" +
  "Respon sempre en català.";

/// Sistema base MÍNIM que s'aplica SEMPRE, encara que «Inclou el codi» estiga
/// tancat: dos regles que l'usuari ha demanat expressament —
/// 1) MEMÒRIA: si el missatge continua el treball anterior, usa la conversa;
///    si és una consulta nova sense relació, respon-ne només la nova.
/// 2) FITXERS REALS: el codi generat s'ha d'emetre amb «@file:» perquè
///    NoOrbit l'escriga al projecte (i sobreesciga la versió modificada).
const MEMORY_SYSTEM =
  AGENT_CREATION_SYSTEM +
  "Tens el historial complet d'aquest xat. Si el missatge actual CONTINUA " +
  "una tasca o consulta anterior, respon en conseqüència servant-la com a " +
  "base. Si és una consulta NOVA sense cap relació, respon NOMÉS a la nova. " +
  "Quan generes o modifiquis codi d'un projecte, emet cada fitxer amb una " +
  "línia «@file: ruta/real» seguida del contingut COMPLET: NoOrbit el crea o " +
  "sobreescriu SOL al projecte obert. Per modificar un fitxer existent usa " +
  "la seua ruta EXACTA.\n" +
  "PER EDITAR només un tros d'un fitxer (millor que reemetre'l sencer): " +
  "«@edit: ruta» + marcador «<<<<<<< CERCA» + el text EXACTE que vols canviar " +
  "+ «=======» + el text NOU (buit = ESBORRAR-lo) + «>>>>>>>».\n" +
  "PER ESBORRAR un fitxer sencer: una línia «@delete: ruta». " +
  "Si la petició és FER codi («fes…», «implementa…», «crea…», «afegeix…», " +
  "«arregla…») o PROVES («tests», «proves»), la resposta HA de portar els " +
  "fitxers amb «@file:» i el codi COMPLET; les proves són fitxers reals " +
  "(p. ex. tests/test_*.py o la carpeta de proves del projecte), mai una " +
  "explicació, i si cal executar-les afegeix «RUN|<comanda>». No diguis que " +
  "ho has fet si no has emès cap «@file:». " +
  "Respon sempre en català.";

/// Nota curta que acompanya el context del projecte: demana al model que
/// s'hi basi i responga en català. S'usa només quan «Inclou el codi» és actiu.
const CONTEXT_SYSTEM =
  AGENT_CREATION_SYSTEM +
  "T'hem passat els fitxers reals del projecte obert com a context. Respon " +
  "en català i BASANT-TE EN AQUEST CODI (no inventes estructures ni fitxers " +
  "que no hi apareixen). QUAN GENERIS O MODIFIQUIS CODI, emet cada fitxer en " +
  "format «@file: ruta_relativa» amb la RUTA COMPLETA de les seves carpetes " +
  "(p. ex. @file: src/components/Boto.tsx), perquè NoOrbit creï les carpetes " +
  "i apliqui el fitxer AUTOMÀTICAMENT. Si cal una carpeta sense fitxer, " +
  "afegeix una línia «@dir: <ruta>». Si no pots generar un fitxer sencer, " +
  "marca-ho amb una línia «TODO: …».\n\n" +
  "PER MODIFICAR UN FITXER QUE JA EXISTEIX: primer LOCALITZAL al context " +
  "(rutes i contingut que t'hem donat), llegeix-ne el contingut actual i " +
  "reemet el MATEIX camí amb «@file:» i el codi COMPLET ja amb els canvis " +
  "aplicats (NoOrbit el sobreescriurà). No inventes un nom nou per a una " +
  "versió modificada: usa la ruta exacta del fitxer existent.\n\n" +
  "MÉS FÀCIL I MILLOR per a canvis PETITS: «@edit: ruta» + «<<<<<<< CERCA» " +
  "+ el text EXACTE del fitxer a canviar + «=======» + el text NOU (buit = " +
  "esborrar-lo) + «>>>>>>>». NoOrbit aplica el canvi al fitxer real.\n" +
  "PER ESBORRAR UN FITXER SENCER (o codi que falla): una línia «@delete: " +
  "ruta». Ho esborra de veritat al projecte.\n\n" +
  "SI ET DEMANEN FER CODI o PROVES (tests): no ho expliquis en prosa — emet " +
  "els fitxers amb «@file:» i el codi COMPLET. Les proves són fitxers reals " +
  "(p. ex. tests/test_*.py o la carpeta de proves del projecte), no una " +
  "explicació, i si cal executar-les afegeix «RUN|<comanda>».\n\n" +
  "MEMÒRIA DE LA CONVERSA: tens el historial de les consultes anteriors " +
  "d'aquest xat. Si el missatge actual CONTINUA una tasca prèvia (p. ex. " +
  "«ara afegeix…», «millora-ho», «nova versió»), usa'l com a base. Si en " +
  "canvi és una consulta NOVA sense relació, respon NOMÉS a la nova consulta " +
  "sense arrossegar el tema anterior.";

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
/// HEURÍSTICA NOVA (important): si el bloc usa tanques ``` , el contingut és
/// NOMÉS el que hi ha DINS DE LES TANQUES. Així la prosa posterior, les
/// directrius «RUN|» i els altres anuncis NO s'arreneguen dins del fitxer
/// (abans «python3 x.py» fallava amb SyntaxError per culpa d'eixa brossa).
function parseFileBlocks(raw: string): { path: string; content: string }[] {
  const lines = raw.split(/\r?\n/);
  type Cur = { path: string; fenced: string[]; bare: string[]; inFence: boolean; hadFence: boolean };
  const acc: Cur[] = [];
  let cur: Cur | null = null;
  const flush = () => {
    if (cur) {
      acc.push(cur);
      cur = null;
    }
  };
  for (const line of lines) {
    const m = line.match(/^[ \t>#*-]*(?:\d+[.)]\s+)?@?file:\s*(.+?)\s*$/i);
    if (m) {
      flush();
      cur = {
        path: m[1].replace(/^[`'"*«]+|[`'"*»;:.,\s]+$/g, "").trim(),
        fenced: [],
        bare: [],
        inFence: false,
        hadFence: false,
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
      flush();
      continue;
    }
    if (cur.inFence) cur.fenced.push(line);
    else cur.bare.push(line);
  }
  flush();
  return acc
    .filter((f) => f.path.length > 0)
    .map((f) => {
      // Amb tanques: només el codi de dins. Sense tanques: les línies nues
      // (com abans), però tallades a la primera directriu.
      const body = f.hadFence && f.fenced.length > 0 ? f.fenced : f.bare;
      return {
        path: f.path,
        content: body.join("\n").replace(/^\s*\n+/, "").replace(/\s+$/, "") + "\n",
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
/// el nom de fitxer. És el camí de RESCAT quan la IA no usa el format
/// estricte @file: — i ha de ser TOLERANT perquè els models no responen tots
/// igual: tanques CRLF, `~~~`, bloc sense tancar (resposta tallada), nom del
/// fitxer escrit DESPRÉS del bloc o com a comentari a la primera línia…
/// Així un tutorial explicat es pot convertir en fitxers reals amb codi.
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
    let p = path.replace(/^\/+/, "").replace(/\\/g, "/");
    if (p.split("/").some((s) => s === "..")) return;
    // Rutes duplicades: afegeix un sufix numèric abans de l'extensió.
    let uniq = p;
    let k = 2;
    while (used.has(uniq)) {
      uniq = /\.[^.]+$/.test(p) ? p.replace(/(\.[^.]+)$/, `_${k}$1`) : `${p}_${k}`;
      k++;
    }
    used.add(uniq);
    out.push({ path: uniq, content });
  };

  /// Dedueix el nom del fitxer d'un bloc: 1) un comentari a la PRIMERA línia
  /// del codi («// src/App.tsx», «# main.py», «<!-- index.html -->»); 2) el
  /// text JUST ABANS del bloc; 3) el text JUST DESPRÉS; 4) nom automàtic
  /// pel llenguatge. Els models barregen totes les maneres.
  const inferPath = (lang: string, content: string, before: string, after: string, i: number): string => {
    const first = content.split("\n", 1)[0] ?? "";
    const cm = first.match(/^\s*(?:\/\/|#|--|<!--|\*|\/\*)\s*([\w][\w./\\-]*\.[A-Za-z0-9]{1,6})\s*(?:-->)?$/);
    if (cm) {
      const ext = cm[1].split(".").pop()!.toLowerCase();
      if (knownExts.has(ext)) return cm[1];
    }
    for (const side of [before, after]) {
      const named = side.match(/[A-Za-z0-9_./\\-]+\.[A-Za-z0-9]{1,6}/g);
      if (!named) continue;
      // Candidat amb extensió RECONEGUDA (evita «e.g», «3.4»), el més proper.
      const cand = [...named].reverse().find((c) => {
        const ext = c.includes(".") ? c.split(".").pop()!.toLowerCase() : "";
        return knownExts.has(ext) && !/^[\d.]+$/.test(c);
      });
      if (cand) return cand;
    }
    return `bloc_${i}.${LANG_EXT[lang] || "txt"}`;
  };

  // Blocs TANCATS: obertura ```lang + contingut + tancament ``` (tolerant a
  // espais/trailers a la línia de tancament).
  const fenceRe = /```([A-Za-z0-9_-]*)\n([\s\S]*?)\n?[ \t]*```/g;
  let m: RegExpExecArray | null;
  let i = 0;
  while ((m = fenceRe.exec(norm)) !== null) {
    i++;
    const lang = (m[1] || "").toLowerCase();
    const content = m[2].replace(/\s+$/, "") + "\n";
    if (!content.trim()) continue;
    const before = norm.slice(Math.max(0, m.index - 320), m.index);
    const after = norm.slice(m.index + m[0].length, m.index + m[0].length + 320);
    const path = inferPath(lang, content, before, after, i);
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
      const content = after.replace(/\s+$/, "") + "\n";
      const before = norm.slice(0, lastOp.index ?? 0).slice(-320);
      dedupe(inferPath(lang, content, before, "", fences.length + 1), content);
    }
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

/// Directrius d'ESBORRAT que la IA emet per treure fitxers o carpetes del
/// projecte. Formes tolerades: «@delete: ruta», «@esborra: ruta»,
/// «@borra: ruta», «@elimina: ruta», «@remove: ruta». Abans d'això, dir-li
/// a la IA «esborra X» era impossible: un @file: buit CREAVA el fitxer buit
/// en lloc d'esborrar-lo. Les rutes són sempre relatives al projecte.
function parseDeleteDirectives(raw: string): string[] {
  const out: string[] = [];
  for (const line of raw.split(/\r?\n/)) {
    const m = line.match(/^\s*@?\s*(?:delete|esborra|borra|elimina|remove|rm)\w*:\s*(.+?)\s*$/i);
    if (!m) continue;
    let d = m[1]
      .replace(/^[`'"]+|[`'"]+$/g, "")
      .trim()
      .replace(/^\/+/, "")
      .replace(/\\/g, "/");
    if (!d || d.split("/").includes("..")) continue;
    if (!out.includes(d)) out.push(d);
  }
  return out;
}

interface AgentResult {
  text: string;
  files: string[];
  steps: { label: string; detail: string; ok: boolean }[];
}

// ── AGENTS ESPECIALISTES CREATS PER LA IA ───────────────────────────────────
// Mateix mecanisme que «@file:» o «@delete:»: la IA emet una directriu en una
// línia i NoOrbit l'executa de veritat. Ací la IA pot CREAR agents
// especialitzats (com subagents de Qoder) quan la tasca ho demane. Els agents
// es guarden com a «Especialistes» persistents (panell Especialistes), amb el
// seu rol i les seues instruccions, i l'usuari ja els podrà triar o posar en
// equip. Format tolerat (separador «|», també s'accepta «·»):
//   @agent: Nom | Rol curt | Instruccions de sistema (què ha de fer i com)
function parseAgentDirectives(raw: string): { name: string; role: string; systemPrompt: string }[] {
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
function stripAgentDirectiveLines(raw: string): string {
  return raw
    .split(/\r?\n/)
    .filter(
      (l) =>
        !/^\s*@\s*(?:agent|subagent|expert|especialista)s?\s*:/i.test(l) &&
        !/^\s*@\s*(?:use|delega|delegate|todo|done|feta|pendient)\w*\s*:/i.test(l)
    )
    .join("\n");
}

/// Crea (o detecta com a existents) els agents que la IA ha demanat amb
/// «@agent:». Retorna un resum llegible per al xat o null si no n'ha emés cap.
/// Si el xat que els ha demanat treballa sense agent actiu, el primer agent
/// creat es VINCULA a ell (a l'estil dels subagents especialitzats): així la
/// directriu no acaba en un nom guardat — el següent missatge ja el fa servir.
async function applyAgentDirectives(raw: string, sid?: string): Promise<string | null> {
  const reqs = parseAgentDirectives(raw);
  if (reqs.length === 0) return null;
  const store = useExpertStore.getState();
  // El panell d'Especialistes pot no haver-se muntat mai: carreguem la llista
  // des del disc abans de deduplicar, si no podria crear duplicats.
  if (store.experts.length === 0) await store.load();
  const created: string[] = [];
  const createdNames: string[] = [];
  const existed: string[] = [];
  for (const r of reqs) {
    const norm = r.name.trim().toLowerCase();
    // Deduplica pel nom: la IA no hauria de crear dos agents iguals. Si ja
    // existeix, es respecta (l'usuari el pot editar al panell).
    if (useExpertStore.getState().experts.some((e) => e.name.trim().toLowerCase() === norm)) {
      existed.push(r.name);
      continue;
    }
    await store.create({ name: r.name, role: r.role, systemPrompt: r.systemPrompt, model: null });
    // «create» no llença: confirma que ara és a la llista (després del «load»).
    if (useExpertStore.getState().experts.some((e) => e.name.trim().toLowerCase() === norm)) {
      created.push(`${r.name}${r.role ? ` (${r.role})` : ""}`);
      createdNames.push(norm);
    }
  }
  if (created.length === 0 && existed.length === 0) return null;
  // Vinculació automàtica (punt b): si aquest xat encara no té cap especialista
  // actiu, agafa el primer agent acabat de crear. Si en crea diversos, l'usuari
  // pot triar-ne un altre al selector o llançar l'equip.
  let boundNote = "";
  if (sid && createdNames.length > 0) {
    const sess = useAgentStore.getState().sessions.find((s) => s.id === sid);
    if (sess && !sess.expertId) {
      const first = useExpertStore
        .getState()
        .experts.find((e) => createdNames.includes(e.name.trim().toLowerCase()));
      if (first) {
        useAgentStore.getState().setChatExpert(sid, first.id);
        boundNote = `\n\n🎯 Ara aquest xat treballa amb «${first.name}» com a agent actiu (al selector de dalt del xat pots canviar-lo o treure'l).`;
      }
    }
  }
  const lines: string[] = [];
  if (created.length > 0)
    lines.push(
      `🤖 Creats ${created.length} agent(s) especialista(s) — els trobaràs al panell «Especialistes» (i a la llista d'IA del xat):
` +
        created.map((n) => `• ${n}`).join("\n") +
        boundNote
    );
  if (existed.length > 0)
    lines.push(`↩️ Ja existien, no s'han duplicat (${existed.length}): ` + existed.join(", "));
  return lines.join("\n\n");
}

// ── DELEGACIÓ SÍNCRONA EN AGENTS LOCALS («@use:») ────────────────────────────
// La IA principal emet «@use: NomAgent | tasca». NoOrbit llança ESE especialista
// (un model local) perresoldre la subtasca ARA MATEIX, recull la seua solució i la
// torna a la IA principal (segona passada de consolidació) perquè l'integre en la
// seua resposta. Tot dins del MATXEIX torn: cap esperà a un procés en segon pla.
const CONSOLIDATION_SYSTEM =
  "Ets la IA PRINCIPAL de NoOrbit. T'hem passat les solucions que els teus " +
  "agents especialistes han produït per a les subtasques delegades. La teua " +
  "feina és CONSOLIDAR-LES: integra-les en una única resposta final coherent, " +
  "resol incoherències entre agents i completa el que els falti. No delegues " +
  "de nou. Respon en català i, si generes codi, usa el format «@file: ruta».";

function parseUseDirectives(
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

/// Executa les delegacions «@use:» i torna un bloc llegible amb la solució de
/// cada agent + la consolidació de la IA principal. Null si no n'hi havia cap.
async function applyDelegateDirectives(raw: string, sid: string): Promise<string | null> {
  const reqs = parseUseDirectives(raw);
  if (reqs.length === 0) return null;
  const store = useExpertStore.getState();
  if (store.experts.length === 0) await store.load();
  const sess = useAgentStore.getState().sessions.find((x) => x.id === sid);
  // Els xats WEB («web:<ia>») no poden delegar: el qui parla és una IA de
  // navegador, que no té accès a l'orquestració interna. Avisem amb claredat.
  if (sess?.provider?.startsWith("web:")) {
    return "(les IAs web del navegador no poden delegar subtasques en agents locals: els agents només treballen amb la IA principal de NoOrbit)";
  }
  const ia = { session: sid, provider: sess?.provider ?? null, model: sess?.model ?? null };

  type Deleg = { agent: string; task: string; solution: string; ok: boolean };
  const results: Deleg[] = [];
  for (const r of reqs) {
    const norm = r.agent.trim().toLowerCase();
    const expert = useExpertStore
      .getState()
      .experts.find((e) => e.name.trim().toLowerCase() === norm);
    if (!expert) {
      results.push({
        agent: r.agent,
        task: r.task,
        solution: `(no existeix cap especialista anomenat «${r.agent}»; crea'l amb «@agent:» o usa'n un altre)`,
        ok: false,
      });
      continue;
    }
    useAgentStore.getState().pushStep(sid, {
      label: `Delegació · ${expert.name}`,
      detail: r.task ? `Treballant la subtasca: ${r.task.slice(0, 80)}…` : "Resolent la subtasca…",
      ok: true,
    });
    // Se si la IA no va escriure tasca, li reenviem la petició original.
    const baseTask =
      r.task ||
      (sess?.messages.filter((m: ChatMessage) => m.role === "user" && !m.error).slice(-1)[0]?.text ?? "");
    // El agent també veu la llista de tasques: així NI la principal NI ell es
    // perden cap punt del treball en comú.
    const agentTaskCtx = useTaskListStore.getState().asContext();
    const subTask = agentTaskCtx ? `${agentTaskCtx}TASCA QUE SE T'HA DELEGAT:\n${baseTask}` : baseTask;
    // Sesión sintética: la solució de l'agent NO sobreescriu el raonament del xat.
    const solution = await useExpertStore
      .getState()
      .runSolo(expert.id, subTask, { session: `${sid}#delegate` })
      .catch(() => null);
    if (solution && solution.trim()) {
      results.push({ agent: expert.name, task: r.task, solution: solution.trim(), ok: true });
    } else {
      results.push({
        agent: expert.name,
        task: r.task,
        solution: `(l'agent no ha pogut respondre: ${useExpertStore.getState().error ?? "error"})`,
        ok: false,
      });
    }
  }

  const okOnes = results.filter((x) => x.ok);
  const solsBlock = results
    .map((x) => `### ${x.agent}${x.task ? ` — ${x.task}` : ""}\n${x.solution}`)
    .join("\n\n");

  // Consolida: torna les solucions a la IA principal perquè les integre.
  let consolidated = "";
  if (okOnes.length > 0) {
    const originalUser =
      sess?.messages.filter((m: ChatMessage) => m.role === "user" && !m.error).slice(-1)[0]?.text ?? "";
    const hist = toHistory(sess?.messages ?? []);
    try {
      consolidated = await invoke<string>("send_prompt", {
        prompt:
          `TASCA DE L'USUARI:\n${originalUser}\n\n--- SOLUCIONS DELS AGENTS DELEGATS ---\n${solsBlock}\n\n` +
          "Integra aquestes solucions en una RESPOSTA FINAL coherent en català. Si cal codi, " +
          "emet cada fitxer amb «@file:». No tornes a delegar; només consolida.",
        system: CONSOLIDATION_SYSTEM,
        ...ia,
        history: hist,
        // La pregunta REAL de l'usuari (per a l'aprenentatge local): el prompt
        // aquí és una instrucció de consolidació, no el que ha demanat l'usuari.
        question: originalUser,
      });
    } catch {
      consolidated = ""; // model incapaç de consolidar: ens quedem amb les solucions en brut
    }
  }

  const head = `🤝 Delegades ${results.length} subtasca(es) en agents locals:`;
  const status = results
    .map((x) => `${x.ok ? "✅" : "⚠️"} ${x.agent}${x.task ? ` — ${x.task}` : ""}`)
    .join("\n");
  const body = consolidated && consolidated.trim()
    ? consolidated.trim()
    : solsBlock;
  return `${head}\n${status}\n\n${body}`;
}

// ── LLISTA DE TASQUES («@todo:» / «@done:») ──────────────────────────────────
function parseTodoDirectives(raw: string): { add: string[]; done: string[]; doing: string[] } {
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

/// Aplica les directrius de checklist als «taskListStore». Retorna un resum o null.
function applyTodoDirectives(raw: string): string | null {
  const { add, done, doing } = parseTodoDirectives(raw);
  if (add.length === 0 && done.length === 0 && doing.length === 0) return null;
  const store = useTaskListStore.getState();
  const added: string[] = [];
  for (const t of add) {
    const before = useTaskListStore.getState().items.length;
    store.add(t, "ia");
    if (useTaskListStore.getState().items.length > before) added.push(t);
  }
  const completed: string[] = [];
  for (const t of done) {
    const pendBefore = useTaskListStore.getState().items.filter((i) => !i.done).length;
    store.complete(t);
    if (useTaskListStore.getState().items.filter((i) => !i.done).length < pendBefore) completed.push(t);
  }
  // EN CURS: la darrera directriu «@doing:» guanya (només n'hi ha una a la vegada).
  const doingNow: string[] = [];
  for (const t of doing) {
    const before = useTaskListStore.getState().items.find((i) => i.doing);
    store.markDoing(t);
    const after = useTaskListStore.getState().items.find((i) => i.doing);
    if (after && after.id !== before?.id) doingNow.push(after.text);
  }
  const lines: string[] = [];
  if (added.length > 0) lines.push("📌 Tasques afegides a la llista:\n" + added.map((x) => `• ${x}`).join("\n"));
  if (doingNow.length > 0)
    lines.push("⏳ La IA està treballant ara mateix en:\n" + doingNow.map((x) => `• ${x}`).join("\n"));
  if (completed.length > 0)
    lines.push("✔️ Tasques marcades com a fetes:\n" + completed.map((x) => `• ${x}`).join("\n"));
  return lines.length > 0 ? lines.join("\n\n") : null;
}

/// Automaterialització: si la resposta de la IA conté codi amb NOMS de fitxers
/// reals (blocs @file: o ``` amb nom), els escriu SOLA al projecte obert
/// (creant també l'estructura de carpetes). Sense clics de l'usuari.
/// Retorna un resum llegible (què ha canviat + tasques pendents) o null.
/// Extensions de fitxers de codi/document que reconeixem quan l'usuari les
/// esmenta al xat («modifica src/app.py»).
const REF_EXTS = new Set([
  "py", "js", "jsx", "ts", "tsx", "json", "md", "txt", "css", "scss", "html",
  "toml", "yaml", "yml", "ini", "cfg", "rs", "go", "java", "c", "h", "cpp",
  "hpp", "sh", "sql", "csv", "xml", "svg", "swift", "kt", "env",
]);

/// Detecta rutes de fitxers del projecte esmentades al text de l'usuari i
/// n'adjunta el contingut (màx 3 fitxers, 8 KB cadascun): és el que permet
/// que un model LOCAL editi codi real sense activar «Inclou el codi».
/// També resol NOMS SENSE RUTA («app.py», «index.html») cercant-los al
/// projecte: l'usuari no sempre escriu la ruta completa.
async function attachReferencedFiles(
  userText: string,
  root: string
): Promise<string | null> {
  const cand = new Set<string>();
  for (const m of userText.matchAll(/[A-Za-z0-9_./\\-]+\.[A-Za-z0-9]{1,6}/g)) {
    let t = m[0]
      .replace(/^[./\\]+/, "")
      .replace(/[.,;:)»"']+$/, "")
      .replace(/\\/g, "/");
    const ext = t.split(".").pop()?.toLowerCase() ?? "";
    if (!REF_EXTS.has(ext)) continue;
    if (t.split("/").includes("..")) continue;
    cand.add(t);
  }
  if (cand.size === 0) return null;
  const parts: string[] = [];
  let n = 0;
  for (const rel of cand) {
    if (n >= 3) break;
    let resolved = rel;
    if (!rel.includes("/")) {
      // Nom sense ruta: el BUSQUEM al projecte (primer resultati amb el
      // mateix nom de fitxer; evita node_modules/target/build).
      try {
        const hits = await invoke<{ path: string }[]>("search_files", {
          query: rel,
          limit: 20,
        });
        const hit =
          hits.find((h) => h.path.endsWith(`/${rel}`) || h.path === rel) ??
          hits.find((h) => h.path.split("/").pop() === rel);
        if (hit) resolved = hit.path.replace(/^\/+/, "");
      } catch {
        /* sense coincidències: provem la ruta tal qual */
      }
    }
    try {
      const content = await invoke<string>("read_file", {
        path: `${root.replace(/\/+$/, "")}/${resolved}`,
      });
      if (content.length > 8000) continue; // massa gran per al context local
      parts.push(
        `FITXER DEL PROJECTE «${resolved}» (contingut actual):\n\`\`\`\n${content}\n\`\`\``
      );
      n++;
    } catch {
      /* la ruta no existeix al projecte: la ignorem */
    }
  }
  return parts.length > 0 ? parts.join("\n\n") : null;
}

/// Extreu els blocs «@edit: ruta» amb seccions CERCA/NOU (edicions
/// quirúrgiques). Tolerant amb marcadors >>> (guerra de conflict marks),
/// talls de resposta i seccions sense tancar. NOU buit = esborrar codi.
interface EditSection {
  find: string;
  replace: string;
}
interface EditBlock {
  path: string;
  edits: EditSection[];
}
function parseEditBlocks(raw: string): EditBlock[] {
  const lines = raw.split(/\r?\n/);
  const out: EditBlock[] = [];
  let cur: EditBlock | null = null;
  let sec: { find: string[]; rep: string[]; phase: 0 | 1 } | null = null;
  const flushSec = () => {
    if (cur && sec) {
      const find = sec.find.join("\n").replace(/^\n+|\s+$/g, "");
      const replace = sec.rep.join("\n").replace(/^\n+|\s+$/g, "");
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

/// Aplica els blocs «@edit:» al fitxer REAL: cerca el text CERCA (exacte o
/// per línies retallades —tolera indentació—) i el substitueix per NOU.
/// Retorna un informe per al xat: què s'ha aplicat i què no s'ha trobat.
async function applyEditBlocks(
  base: string,
  blocks: EditBlock[]
): Promise<{ edited: string[]; failed: string[] }> {
  const edited: string[] = [];
  const failed: string[] = [];
  for (const block of blocks) {
    const rel = block.path.replace(/^\/+/, "").replace(/\\/g, "/");
    if (rel.split("/").some((s) => s === "..")) continue;
    const abs = `${base}/${rel}`;
    let old: string;
    try {
      old = await invoke<string>("read_file", { path: abs });
    } catch {
      failed.push(`${rel}: el fitxer no existeix (no es pot editar)`);
      continue;
    }
    let updated = old;
    let ok = 0;
    let fail = 0;
    for (const ed of block.edits) {
      if (updated.includes(ed.find)) {
        updated = updated.replace(ed.find, ed.replace);
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
      }
    }
    if (ok > 0) {
      try {
        const prev = await captureOriginal(abs);
        await invoke("write_file", { path: abs, content: updated });
        noteFileEdited(abs, updated, prev.original, prev.existed);
        edited.push(`${rel} (${ok} edició${ok === 1 ? "" : "s"}${fail ? `, ${fail} no trobades` : ""})`);
      } catch {
        failed.push(`${rel}: no s'ha pogut escriure`);
      }
    } else if (fail > 0) {
      failed.push(`${rel}: el text CERCA no coincideix amb el fitxer real — cal el text EXACTE`);
    }
  }
  return { edited, failed };
}

async function autoMaterialize(raw: string): Promise<string | null> {
  const ws = useWorkspaceStore.getState();
  const root = ws.root;
  if (!root || !raw) {
    // Sense projecte obert NO es pot escriure res: ho DIEM al xat (abans
    // callàvem i semblava que la IA «no editava codi» per sí sola).
    if (raw && /```/.test(raw)) {
      return (
        "⚠️ He generat codi, però NO HI HA CAP PROJECTE OBERT on escriure'l. " +
        "Obre una carpeta (Fitxer → Obre carpeta) i torna a demanar-m'ho: " +
        "aleshores els fitxers es crearan i modificaran de veritat."
      );
    }
    return null;
  }

  let files = parseFileBlocks(raw);
  if (files.length === 0) {
    files = extractFromProse(raw);
  }
  // Blocs de codi SENSE nom de fitxer (els models locals sovint no anomenen
  // res): abans es descartaven i NOMÉS sobrevivien els .md, així que «no crea
  // res de codi». Ara els blocs de CODI real (.py/.js/.ts/…) es desin a la
  // carpeta «codi_ia/» del projecte (els .md/.txt sense nom es descarten:
  // són prosa explicativa, no codi).
  const isUnnamed = (p: string) => /^bloc_\d+\./.test(p.split("/").pop() || "");
  files = files
    .filter((f) => !isUnnamed(f.path) || !/\.(md|txt)$/i.test(f.path))
    .map((f) => (isUnnamed(f.path) ? { ...f, path: `codi_ia/${f.path}` } : f));
  // EDICIÓ D'UN SOL FITXER: si la resposta porta UN sol bloc sense nom i la
  // prosa esmenta UNA única ruta coneguda (p. ex. «he modificat src/app.py»),
  // aquest bloc ÉS aquell fitxer: l'escrivim a la seva ruta REAL en lloc de
  // deixar-lo a codi_ia/ amb un nom automàtic (l'usuari demanava editar).
  if (files.length === 1 && isUnnamed(files[0].path.split("/").pop() || "")) {
    const mentioned = new Set<string>();
    for (const m of raw.matchAll(/[A-Za-z0-9_./\\-]+\.[A-Za-z0-9]{1,6}/g)) {
      let t = m[0]
        .replace(/^[./\\]+/, "")
        .replace(/[.,;:)»"']+$/, "")
        .replace(/\\/g, "/");
      const ext = t.split(".").pop()?.toLowerCase() ?? "";
      if (!REF_EXTS.has(ext) || t.startsWith("codi_ia/")) continue;
      mentioned.add(t);
    }
    if (mentioned.size === 1) {
      files = [{ ...files[0], path: [...mentioned][0] }];
    }
  }
  // Carpetes que la IA ha demanat explícitament (estructura buida, sense fitxer).
  const dirReqs = parseDirDirectives(raw);
  // Fitxers/carpetes que la IA ha demanat ESBORRAR (l'opció contraria a @file:).
  const delReqs = parseDeleteDirectives(raw);
  // Edicions QUIRÚRGIQUES «@edit:» (CERCA/NOU): modificar o esborrar codi
  // existent sense reemetre fitxers sencers (els models locals els tallen).
  const editBlocks = parseEditBlocks(raw);
  if (
    files.length === 0 &&
    dirReqs.length === 0 &&
    delReqs.length === 0 &&
    editBlocks.length === 0
  )
    return null;

  const base = root.replace(/\/+$/, "");
  const written: string[] = [];
  const createdDirs: string[] = [];
  const pending: string[] = [];
  let lintErrors = 0; //errors detectats en el codi escrit per la IA

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
      const abs = `${base}/${rel}`;
      // El codi VELL es llegeix ABANS d'aixafar-lo: és el que es mostrarà en
      // roig al diff de revisió (verd = nou). Vegeu «editReviewStore».
      const prev = await captureOriginal(abs);
      await invoke("write_file", { path: abs, content: f.content });
      noteFileEdited(abs, f.content, prev.original, prev.existed);
      // Revisió lleugera del codi escrit (encara que el fitxer NO estiga obert):
      // balanceig de delimiters i JSON vàlid. Els problemes van al panell.
      lintErrors += lintOffline(abs, rel, f.content);
      written.push(rel);
      const segs = rel.split("/");
      segs.pop(); // descarta el nom del fitxer: la resta són carpetes
      if (segs.length > 0) structure.add(segs.join("/"));
      if (!f.content.trim() || /pendent d'implementar/i.test(f.content)) pending.push(rel);
    } catch {
      /* si un fitxer falla, continua amb la resta */
    }
  }
  if (written.length === 0 && createdDirs.length === 0 && delReqs.length === 0 && editBlocks.length === 0)
    return null;

  // 3) Esborra el que la IA ha demanat amb «@delete:/@esborra:». Va DESPRÉS
  //    de les escriptures: si un fitxer s'ha reescrit i alhora s'ha demanat
  //    d'esborrar-lo, mana l'esborrat (l'usuari vol netejar el projecte).
  const deleted: string[] = [];
  for (const d of delReqs) {
    try {
      await invoke("delete_path", { path: `${base}/${d}` });
      dropMark(`${base}/${d}`);
      deleted.push(d);
      // Si era també un fitxer escrit aquest torn, ix de la llista: ja no hi és.
      const i = written.indexOf(d);
      if (i >= 0) written.splice(i, 1);
    } catch {
      /* si falla (p. ex. no existeix), continua */
    }
  }
  // 4) Edicions quirúrgiques «@edit:» (CERCA/NOU): modificar codi existent
  //    o ESBORRAR-ne fragments (NOU buit). Van DESPRÉS de les escriptures:
  //    un @file: complet mana sobre un @edit del mateix fitxer.
  let editReport: { edited: string[]; failed: string[] } | null = null;
  if (editBlocks.length > 0) {
    editReport = await applyEditBlocks(base, editBlocks);
  }
  await ws.refreshTree();
  // La materialització definitiva ha acabat: els fitxers escrits en directe
  // deixen de ser una previsualització i passen a revisió pendent (compte
  // enrere de 35 s fins a acceptar-se sols).
  armAllMarks();

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
  // Si el motor lleuger ha detectat errors (claus sense tancar, JSON invàlid…),
  // avisem: es veuran al panell «Problemes» i la IA els pot corregir si se li
  // demana. És el mateix que fa Eclipse en marcar línies en vermell en escriure.
  if (lintErrors > 0) {
    parts.push(
      `⚠️ Detectats ${lintErrors} error${lintErrors === 1 ? "" : "s"} de sintaxi en el codi escrit.` +
        ` Revisa'l al panell «Problemes» o demana a la IA que el corregeix.`
    );
  }
  if (deleted.length > 0) {
    parts.push(
      `🗑 Esborrats del projecte (${deleted.length}):\n` +
        deleted.map((p) => `• ${p}`).join("\n")
    );
  }
  if (editReport && (editReport.edited.length > 0 || editReport.failed.length > 0)) {
    if (editReport.edited.length > 0) {
      parts.push(
        `✏️ Edicions aplicades al codi existent (${editReport.edited.length}):\n` +
          editReport.edited.map((p) => `• ${p}`).join("\n")
      );
    }
    if (editReport.failed.length > 0) {
      parts.push(
        `⚠️ Edicions NO aplicades (el text CERCA no coincideix):\n` +
          editReport.failed.map((p) => `• ${p}`).join("\n")
      );
    }
  }
  // El que s'ha esborrat ja no és cap tasca pendent.
  const pend = Array.from(new Set([...pending, ...todos])).filter((p) => !deleted.includes(p));
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
  // Les revisions pendents del torn anterior NO es netegen: poden ser encara
  // dins dels 35 s de gràcia i s'han d'acabar d'acceptar/rebutjar normalment.
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
      const abs = `${base}/${rel}`;
      try {
        // Primer contacte: guardem el contingut VELL abans d'escriure (si
        // ho féssim després, el disc ja tindria el nou i no tindríem amb
        // què comparar-lo) i escrivim el nou: disc + editor en directe.
        const first = !liveWritten.has(rel);
        const prev = await captureOriginal(abs);
        await useWorkspaceStore.getState().writeLive(abs, f.content);
        if (first) noteLiveWriteStart(abs, f.content, prev.original, prev.existed);
        else noteLiveWriteContent(abs, f.content);
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
  /// Projecte (bucket) de l'últim xat obert: només es mostren els del projecte actual.
  bucket: string | null;
  activeId: string;
  /// Últim consum de tokens per xat (event «ai://usage»): s'ensenya sota la
  /// resposta. Especialment útil amb les APIs de núvol (què costa cada torn).
  usageBySession: Record<string, TokenUsage>;

  refreshBackends: () => Promise<void>;

  // ── Gestió de xats ─────────────────────────────────────────────────────
  /// Obre un xat NOU (pot tenir la seva IA des del primer moment).
  newChat: (over?: Partial<ChatSession>) => string;
  /// FORK: duplica la conversa fins al missatge `upToIndex` en un xat nou,
  /// que pot continuar el treball amb una altra IA sense tocar l'original.
  forkChat: (id: string, upToIndex: number) => string;
  closeChat: (id: string) => Promise<void>;
  setActive: (id: string) => void;
  /// Crea el xat amb l'id EXACT de la sessió de l'equip d'especialistes
  /// (sense canviar l'actiu) perquè els fragments del stream s'acumulin.
  ensureChat: (id: string, title: string) => void;
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
  send: (prompt: string, id?: string, images?: string[]) => Promise<string | null>;
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
  /// Esborra el stream acumulat d'un xat (el fa servir l'equip d'especialistes:
  /// el backend condueix les sessions «team:*» i ningú més les tanca).
  clearStream: (sessionId: string) => void;
  buildContext: (id?: string, limit?: number, taskText?: string) => Promise<string>;
  beginTurn: (id: string, text: string, images?: string[]) => void;
  finalizeTurn: (id: string) => void;
  /// Afegeix al xat un missatge sense preguntar a cap model: notes del propi
  /// NoOrbit i respostes obtingudes del xat web d'una altra IA (consulta
  /// manual amb el compte de l'usuari — l'origen sempre queda etiquetat).
  addNote: (text: string, opts?: { role?: "user" | "assistant"; images?: string[]; id?: string }) => void;
  clearChat: (id?: string) => void;
  /// Atura la generació d'un xat (o de tots, si no se n'indica cap).
  stop: (id?: string) => Promise<void>;
  clearTimeline: (id?: string) => void;
}

/// Carrega els xats d'UN PROJECTE (`key`); si no n'hi ha cap encara, obre el
/// xat «main».
function loadChats(key: string): { sessions: ChatSession[]; activeId: string } {
  try {
    const raw = localStorage.getItem(key);
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

const initialChats = (() => {
  // MIGRACIÓ (una sola vegada): els xats globals antics passen al calaix
  // «sense projecte». Així cap projecte futur hereta els xats d'un altre.
  try {
    const legacy = localStorage.getItem(LS_KEY);
    if (legacy && !localStorage.getItem(chatLsKey(null))) {
      localStorage.setItem(chatLsKey(null), legacy);
      localStorage.removeItem(LS_KEY);
    }
  } catch {
    /* si falla, els xats antics simplement es perdran d'estat visible */
  }
  return loadChats(chatLsKey(useWorkspaceStore.getState().root));
})();

/// Projecte del calaix de xats que es crearà just després amb «initialChats».
currentBucket = useWorkspaceStore.getState().root;

export const useAgentStore = create<AgentState>((set, get) => {
  /// Aplicar un canvi al xat `sid` (si ja no existeix, es ignora: pot haver
  /// estat tancat mentre la IA responia).
  const patch = (sid: string, p: Partial<ChatSession> | ((s: ChatSession) => Partial<ChatSession>)) =>
    set((st) => ({
      sessions: st.sessions.map((s) => (s.id === sid ? { ...s, ...(typeof p === "function" ? p(s) : p) } : s)),
    }));

  // ── COMPTABILITZACIÓ DE TOKENS ────────────────────────────────────────
  // El backend emet «ai://usage» al final de cada torn (Ollama i APIs de
  // núvol). Es registra UN COP i es desa per xat: així la resposta mostra
  // quant ha costat (entrada/sortida) i quin percentatge del context s'ha
  // ocupat — la dada que fa falta per saber si una API de núvol val la pena.
  if (!(globalThis as Record<string, unknown>).__noorbitUsageListener) {
    (globalThis as Record<string, unknown>).__noorbitUsageListener = true;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<TokenUsage & { session: string }>("ai://usage", (evt) => {
        const p = evt.payload;
        if (!p?.session) return;
        const usage: TokenUsage = {
          provider: p.provider ?? "",
          model: p.model ?? "",
          input: p.input ?? 0,
          output: p.output ?? 0,
          total: p.total ?? (p.input ?? 0) + (p.output ?? 0),
          contextWindow: p.contextWindow ?? 0,
          contextUsedPct: p.contextUsedPct ?? 0,
        };
        set((st) => ({
          usageBySession: { ...st.usageBySession, [p.session]: usage },
        }));
        // Si el xat ja té el missatge final, hi enganxem l'ús (el comptador
        // ha de quedar SOTA la resposta corresponent).
        const sess = get().sessions.find((s) => s.id === p.session);
        if (sess && sess.messages.length > 0) {
          const last = sess.messages[sess.messages.length - 1];
          if (last.role === "assistant") {
            set((st2) => ({
              sessions: st2.sessions.map((s) =>
                s.id === p.session
                  ? {
                      ...s,
                      messages: s.messages.map((m, i) =>
                        i === s.messages.length - 1 ? { ...m, usage } : m
                      ),
                    }
                  : s
              ),
            }));
          }
        }
      })
    );
  }

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
    // El raonament que el model ja havia emès ABANS del faliment (o de premere
    // «Atura») també es conserva a la bombolla del xat: demanar-lo al backend
    // és instantani i després es tanca el torn.
    void fetchThinking(sid).then((t) => {
      if (t) patch(sid, { lastThinking: t });
      get().finalizeTurn(sid);
    });
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
    invokeFn: (level: number) => Promise<T>,
    startLevel = 0,
  ): Promise<{ level: number; value: T }> => {
    let lastErr: unknown = null;
    for (let level = startLevel; level <= 3; level++) {
      try {
        if (level > 0) {
          // Netegem el stream acumulat i afegim un pas explicatiu.
          patch(sid, { stream: null });
          get().pushStep(sid, {
            label: "memòria",
            detail:
              level === 1
                ? "⚠️ El context era massa gran: ALLIBERANT RAM i COMPACTANT la conversa (resum dens que conserva l'objectiu, el pla i l'esquelet del codi) perquè la IA CONTINUI programant a trossos…"
                : level === 2
                ? "⚠️ Continua sense prou memòria: reintentant SENSE adjuntar el codi del projecte…"
                : "⚠️ Últim recurs: reintentant amb NOMÉS el fil bàsic de la conversa (últims torns retallats)…",
            ok: true,
          });
        }
        const value = await invokeFn(level);
        return { level, value };
      } catch (e) {
        lastErr = e;
        if (!isRetriableError(e) || isCancelled(e)) throw e;
        // Si l'usuari ha premut «Atura» mentrestant, ixem del bucle.
        if (sessOf(sid)?.stopped) throw e;
      }
    }
    throw lastErr ?? new Error("Reintents esgotats");
  };

  /// Nota que s'afegeix al resultat quan la resposta ha vingut amb el context
  /// COMPACTAT: sense ella, l'usuari no sabia per què la IA «no recordava» el
  /// que estaven fent (sobretot en CANVIAR de IA i pujar el context).
  const oomNote = (level: number): string =>
    level >= 3
      ? "\n\n⚠️ La IA ha anat justa de memòria i ha respost amb NOMÉS el fil bàsic de la conversa. Si ha perdut el detall, digues-li «repren el pla anterior»."
      : level > 0
      ? "\n\n⚠️ La IA ha anat justa de memòria: ha respost amb el context COMPACTAT (un resum dens que conserva l'objectiu, el pla i l'esquelet del codi — no és una retallada). Si nota que ha perdut algun detall, digues-li «repren el pla anterior»."
      : "";

  return {
    backends: [],
    checking: false,
    sessions: initialChats.sessions,
    activeId: initialChats.activeId,
    bucket: currentBucket,
    usageBySession: {},

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

    /// Garanteix que existeix un xat amb EXACTAMENT aquest id (títol donat),
    /// SENSE canviar l'actiu. El fan servir els especialistes de l'equip:
    /// el backend emet els fragments «ai://chunk» amb la sessió «team:<id>»
    /// i appendStream només els acumula si la sessió ja existeix.
    ensureChat: (id, title) => {
      if (get().sessions.some((x) => x.id === id)) return;
      set((st) => ({ sessions: [...st.sessions, blankSession(id, { title })] }));
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
          // podria excedir la quota del magatzem local. Els límits són generosos
          // (16.000 / 10.000 caràcters) perquè una resposta llarga no isca
          // TRENCADA en reiniciar l'app; si la quota no dóna, el catch evita
          // perdre la sessió sencera.
          messages: s.messages.slice(-30).map((m) => ({
            role: m.role,
            text: m.text.slice(0, 16000),
            thinking: m.thinking ? String(m.thinking).slice(0, 10000) : null,
            steps: (m.steps ?? []).slice(0, 8),
            elapsedMs: m.elapsedMs ?? null,
            at: m.at,
            error: m.error ?? false,
          })),
        }));
        localStorage.setItem(chatLsKey(currentBucket), JSON.stringify({ activeId: st.activeId, sessions }));
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
        // També ací: si l'agent ha generat codi o proves amb «@file:» (o en
        // blocs de codi), s'han d'escriure AL PROJECTE. Abans aquest camí es
        // quedava en text i l'usuari veia la feina «feta» sense cap fitxer.
        resetLiveMaterialize();
        const extra = await autoMaterialize(res.text);
        const agents = await applyAgentDirectives(res.text, sid);
        const delegated = await applyDelegateDirectives(res.text, sid);
        const todos = applyTodoDirectives(res.text);
        set((st) => ({
          sessions: st.sessions.map((s) =>
            s.id === sid
              ? {
                  ...s,
                  running: false,
                  startedAt: null,
                  lastElapsedMs: Date.now() - t0,
                  lastResult:
                    [stripAgentDirectiveLines(res.text), delegated, extra, agents, todos]
                      .filter(Boolean)
                      .join("\n\n") || stripAgentDirectiveLines(res.text),
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
        const ctx = sessOf(sid)?.includeContext ? await get().buildContext(sid, undefined, prompt) : "";
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
        // AUTOMÀTICAMENT: alliberar RAM + context COMPACTAT → sense context → sense històrial.
        const quickStart =
          (sessOf(sid)?.provider ?? "").toLowerCase() === "" ||
          (sessOf(sid)?.provider ?? "").toLowerCase() === "ollama"
            ? finalPrompt.length > 1500
              ? 1
              : 0
            : 0;
          const oomLevel = await runWithOOMRetry(sid, async (level) => {
          let m = finalPrompt;
          let h = history;
          let sy: string | null = ctx ? CONTEXT_SYSTEM : MEMORY_SYSTEM;
          if (level === 1) {
            const small = sessOf(sid)?.includeContext ? await get().buildContext(sid, 1500, prompt) : "";
            m = small ? `${small}\n\n---\nPREGUNTA / TASCA:\n${prompt}` : prompt;
            sy = small ? CONTEXT_SYSTEM : MEMORY_SYSTEM;
          } else if (level === 2) {
            m = prompt;
            sy = MEMORY_SYSTEM;
          } else if (level === 3) {
            // Últim recurs: ABANS es borrava TOTA la conversa i el model
            // perdia el fil («continua» no sabia què continuar). Ara es
            // conserva un fil MÍNIM: últims torns retallats.
            m = prompt;
            h = threadOf(history);
            sy = MEMORY_SYSTEM;
          }
          return await invoke<string>("send_prompt_stream", {
            prompt: m,
            system: sy,
            session: sid,
            provider: s0.provider,
            model: s0.model,
            history: h,
            // Pregunta real de l'usuari (aprenentatge local): «m» pot ser el
            // context del projecte + la pregunta.
            question: prompt,
          });
        }, quickStart);
        // Memòria: si s'ha hagut de RETALLAR el context, el resultat ho diu
        // al propi xat (abans només es veia al visor de procés i l'usuari no
        // entenia per què la IA «no recordava» el que estaven fent).
        const text = oomLevel.value;
        const oom = oomNote(oomLevel.level);
        const thinking = await fetchThinking(sid);
        get().pushStep(sid, {
          label: "ia",
          detail: "Resposta rebuda; comprovant si cal crear o desar fitxers al projecte…",
          ok: true,
        });
        // Automaterialització: si la resposta conté fitxers amb nom, s'escriuen
        // sols al projecte i afegim el resum del que ha canviat + pendents.
        // I si ha emés «@agent:», NoOrbit crea els especialistes demanats.
        resetLiveMaterialize();
        const extra = await autoMaterialize(text);
        const agents = await applyAgentDirectives(text, sid);
        // Delegació «@use:» (després de crear-los, perquè els agents existiscan)
        // i checklist «@todo/@done».
        const delegated = await applyDelegateDirectives(text, sid);
        const todos = applyTodoDirectives(text);
        patch(sid, {
          running: false,
          startedAt: null,
          lastElapsedMs: Date.now() - t0,
          // Les línies «@agent:» ja s'han executat: no cal mostrar-les al xat
          // (el resum d'agents creats informa de la mateixa feina).
          lastResult:
            [stripAgentDirectiveLines(text), delegated, extra, agents, todos, oom]
              .filter(Boolean)
              .join("\n\n") || stripAgentDirectiveLines(text),
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
        const oomLevel = await runWithOOMRetry(sid, async (level) => {
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
            // Pregunta real de l'usuari (aprenentatge local).
            question: user,
          });
        });
        const raw = oomLevel.value;
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
            // Pregunta real de l'usuari (aprenentatge local).
            question: text,
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
        let lintErrors = 0;
        for (const f of files) {
          const rel = f.path.replace(/^\/+/, "").replace(/\\/g, "/");
          // Seguretat: mai eixir de l'arrel del projecte.
          if (rel.split("/").some((seg) => seg === "..")) continue;
          const abs = `${base}/${rel}`;
          const prev = await captureOriginal(abs);
          await invoke("write_file", { path: abs, content: f.content });
          noteFileEdited(abs, f.content, prev.original, prev.existed);
          lintErrors += lintOffline(abs, rel, f.content);
          created.push(rel);
        }
        await ws.refreshTree();
        armAllMarks();
        const head = skeleton
          ? `🗂️ Esquelet creat a «${folder}»: ${created.length} fitxers/carpetes (buits, per omplir — el model local és dèbil):\n`
          : `✅ Creats ${created.length} fitxers a «${folder}»:\n`;
        const summary =
          head +
          created.map((c) => `• ${c}`).join("\n") +
          (lintErrors > 0
            ? `\n\n⚠️ ${lintErrors} error${
                lintErrors === 1 ? "" : "s"
              } de sintaxi detectats. Mira'l al panell «Problemes».`
            : "");
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
        // El missatge també pot portar peticions d'esborrat («@delete: …»).
        const dels = parseDeleteDirectives(raw);
        if (files.length === 0 && dels.length === 0) {
          throw new Error(
            "Aquest missatge no té cap bloc de codi que es pugui crear. Prova una d'aquestes:\n" +
              "• Demana a la IA «reemete els fitxers amb @file:» i torna-ho a prémer.\n" +
              "• Si el codi ja el tens a mà, prem «Crear a mà» i enganxa-hi les rutes.\n" +
              "• Si la resposta es va TALLAR (codi a mitges), demana «continua»."
          );
        }
        get().pushStep(sid, {
          label: "projecte",
          detail: `Escrivint ${files.length} fitxers (i les seves carpetes) al projecte…`,
          ok: true,
        });
        const created: string[] = [];
        let lintErrors = 0;
        for (const f of files) {
          const rel = f.path.replace(/^\/+/, "").replace(/\\/g, "/");
          if (rel.split("/").some((seg) => seg === "..")) continue;
          const abs = `${base}/${rel}`;
          const prev = await captureOriginal(abs);
          await invoke("write_file", { path: abs, content: f.content });
          noteFileEdited(abs, f.content, prev.original, prev.existed);
          lintErrors += lintOffline(abs, rel, f.content);
          created.push(rel);
        }
        // Esborras demanats: van després de les escriptures (mana l'esborrat).
        const erased: string[] = [];
        for (const d of dels) {
          try {
            await invoke("delete_path", { path: `${base}/${d}` });
            dropMark(`${base}/${d}`);
            erased.push(d);
            const i = created.indexOf(d);
            if (i >= 0) created.splice(i, 1);
          } catch {
            /* si falla (p. ex. no existeix), continua */
          }
        }
        await ws.refreshTree();
        armAllMarks();
        const summary =
          `✅ Creats ${created.length} fitxers a «${folder}» des del missatge:\n` +
          created.map((c) => `• ${c}`).join("\n") +
          (erased.length > 0
            ? `\n\n🗑 Esborrats (${erased.length}):\n` + erased.map((d) => `• ${d}`).join("\n")
            : "") +
          (lintErrors > 0
            ? `\n\n⚠️ ${lintErrors} error${
                lintErrors === 1 ? "" : "s"
              } de sintaxi detectats. Mira'l al panell «Problemes».`
            : "");
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

    // Neteja el stream d'un xat (l'equip d'especialistes ho fa al principi de
    // cada «run»: el backend condueix eixes sessions i ningú més les tanca,
    // així sense eixò el text de runs anteriors s'hi acumularia a l'infinit).
    clearStream: (sessionId) => {
      if (!sessOf(sessionId)) return;
      set((s) => ({
        sessions: s.sessions.map((x) => (x.id === sessionId ? { ...x, stream: null } : x)),
      }));
    },

    // Llegeix els fitxers reals del projecte obert i en forma un bloc de context
    // per a la IA. Buit si no hi ha workspace o cap fitxer llegible.
    // `limit` (caràcters): 4.500 per defecte; 1.200-2.000 en reintents.
    buildContext: async (id, limit, taskText) => {
      const sid = target(id);
      const ws = useWorkspaceStore.getState();
      if (!ws.root) {
        patch(sid, { contextInfo: "⚠️ No hi ha cap projecte obert: la IA no està veient codi." });
        return "";
      }
      // Rutes a les quals l'usuari ES REFERIX en el seu missatge: aquestes van
      // PRIMERS i SENS TALLAR al context (argument «focus» del backend),
      // perquè la IA puga REVISAR i MODIFICAR el codi real, no inventar-lo.
      const focus = taskText ? extractRecommendedPaths(taskText) : [];
      try {
        // Pressupost de CONTINGUT. Abans només hi cabien ~3 fitxers (4500 ÷
        // 1500/caràcters per fitxer); ara n'adjuntem molts més. L'ARBRE de
        // rutes, però, és SEMPRE el projecte COMPLET (el backend ja emet cada
        // fitxer, encara que el seu cos no hi capga). En un equip lent la
        // primera resposta pot trigar més; «runWithOOMRetry» el redueix si cal.
        const files = await invoke<{ path: string; content: string }[]>(
          "collect_project_files",
          { limit: limit ?? 16000, focus }
        );
        if (!files || files.length === 0) {
          patch(sid, { contextInfo: "⚠️ No he trobat fitxers de codi a la carpeta oberta." });
          return "";
        }
        // Els fitxers «només-ruta» (content buit) igualment eixen a l'arbre,
        // però no generen un bloc de CONTINGUT buit.
        const withBody = files.filter((f) => f.content.trim().length > 0);
        const chars = withBody.reduce((n, f) => n + f.content.length, 0);
        patch(sid, {
          contextInfo:
            `📁 Projecte complet a l'abast: ${files.length} fitxers (tots a l'arbre). ` +
            `${withBody.length} amb contingut adjunt (${chars} caràcters). Els altres ` +
            `els veus pel nom i pots modificar-los reenviant la seua ruta amb «@file:».`,
        });
        // Arbre acotat (400 rutes) perquè un monorepo enorme no inflle el
        // prompt; si en sobresurten, s'indica quants falten.
        const MAX_TREE = 400;
        const more = files.length - MAX_TREE;
        const tree =
          files.slice(0, MAX_TREE).map((f) => f.path).join("\n") +
          (more > 0 ? `\n… i ${more} fitxers més.` : "");
        const body = withBody.map((f) => `===== ${f.path} =====\n${f.content}`).join("\n\n");
        return (
          `A continuació tens TOT EL PROJECTE obert (workspace). La llista de\n` +
          `RUTES inclou CADA fitxer del projecte; el CONTINGUT adjunt cobreix els\n` +
          `que hi capen (els altres els veus pel nom: per modificar-ne un, reemet\n` +
          `la MATEIXA ruta amb «@file:» i el codi complet).\n` +
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
    send: async (prompt, id, images) => {
      const sid = target(id);
      const sess = sessOf(sid);
      const text = prompt.trim();
      if (!sess || !text) return null;
      // El fil precedent viatja amb cada missatge: el model sap què s'ha dit
      // (essencial per als forks i per continuar el treball).
      const history = toHistory(sess.messages);
      // Imatges adjuntes: les passades per la UI + les rutes que aparegin al
      // text (una ruta .png al prompt és tractada com a imatge per llegir).
      const imgs = Array.from(new Set([...(images ?? []), ...extractChatImagePaths(text)]));

      // ── RUTES AUTOMÀTIQUES abans de res ────────────────────────────────
      // El xat interpreta TRES tipus de missatge especials abans d'enviar
      // la frase al model de text:
      //  1. ORDRES D'ORDINADOR (@computer: o «obre Blender», «crea una
      //     carpeta X»…): van a l'agent d'ordinador, que demana permís
      //     cada comanda (panell Ordinador).
      //  2. ORDRES DE BLENDER (@blender: o un verb d'edició +
      //     menció explícita de Blender): van al pont 3D (`agent_apply_
      //     blender`) — genera Python, ho aplica i arrenca Blender sol
      //     si falta (amb el permís d'Ordinador).
      //  3. Tot altre missatge segueix el flux normal de l'agent.
      const dirComputer = text.match(/^@(?:computer|ordinador):\s*([\s\S]+)/i);
      const goalComputer = dirComputer
        ? dirComputer[1].trim()
        : COMPUTER_ORDER_RE.test(text.trim())
          ? text.trim()
          : null;
      const dirBlender = text.match(/^@(?:blender|b3d):\s*([\s\S]+)/i);
      const mentionsBlender = /\bblender\b/i.test(text.trim());
      const isAppOSVerb = /^(?:obri?m?\b|obre\b|arrenca\b|engega\b|tanca\b)/.test(text.trim());
      // Les PREGUNTES («per què Blender es queda penjat?», «com ho faig?»)
      // continuen al xat normal: només les ORDRES sense interrogació van al 3D.
      const looksLikeQuestion = /\?|^\s*(?:per què|per què no|que és|què és|com\s)/i.test(text.trim());
      if (goalComputer === null && (dirBlender || (mentionsBlender && !isAppOSVerb && !looksLikeQuestion))) {
        // ── BLENDER: aplicar el demanat al pont 3D, sense retocs al xat ──
        const goalBlender = (
          dirBlender?.[1] ??
          text.replace(/\b(?:in|a|en|dins|amb)\s+blender\b/gi, "")
        )
          .replace(/\bblender\b\s*/gi, "")
          .replace(/\s{2,}/g, " ")
          .replace(/^[\s,.]+/g, "")
          .trim();
        return get().apply("blender", goalBlender || text, id);
      }
      get().beginTurn(sid, text, imgs);

      // ── ORDRES D'ORDINADOR: interpreta · pregunta permís · actua ──────
      // Si el missatge és una ordre del PC («obre Blender», «crea una carpeta
      // X», «tanca el navegador») o porta el prefix @computer:, no va al
      // model de text: va a l'AGENT D'ORDINADOR, que tradueix l'objectiu en
      // comandes reals i demana CONFIRMACIÓ cada vegada (panell Ordinador).
      // Sense permís activat, el xat explica com activar-lo i no toca res.
      if (goalComputer) {
        const t0c = Date.now();
        patch(sid, {
          running: true,
          error: null,
          stopped: false,
          lastThinking: null,
          startedAt: t0c,
          lastElapsedMs: null,
          stream: null,
        });
        let perms: { enabled: boolean } | null = null;
        try {
          perms = await invoke<{ enabled: boolean }>("computer_permissions");
        } catch {
          perms = null;
        }
        if (!perms?.enabled) {
          patch(sid, { running: false, lastElapsedMs: Date.now() - t0c });
          get().addNote(
            "Per executar ordres a l'ordinador necessito el teu permís: activa «Permet que la IA controli l'ordinador» al panell **Treball → Integracions → Ordinador**. Amb «Confirma cada comanda» activat et demanaré permís una a una a pantalla. Quan estigui activat, torna'm a fer l'ordre i jo la faré.",
            { id: sid },
          );
          get().finalizeTurn(sid);
          return goalComputer;
        }
        get().pushStep(sid, {
          label: "Ordre d'ordinador",
          detail: `Interpretant: «${goalComputer.slice(0, 80)}»…`,
          ok: true,
        });
        let answer = "";
        let unlistenAgent: (() => void) | null = null;
        try {
          unlistenAgent = await listen("computer://agent", (ev) => {
            const d = (ev.payload ?? {}) as {
              action?: string;
              command?: string;
              reason?: string;
              answer?: string;
            };
            if (d.action === "run" && d.command) {
              get().pushStep(sid, {
                label: "Ordre d'ordinador",
                detail: `$ ${d.command}`,
                ok: true,
              });
            } else if (d.action === "done" && d.answer) {
              answer = d.answer;
            }
          });
          const out = await invoke<string>("computer_agent_run", { goal: goalComputer });
          if (!answer && out) answer = out;
        } catch (e) {
          answer = `❌ ${String(e)}`;
        } finally {
          try {
            unlistenAgent?.();
          } catch {
            /* ignore */
          }
        }
        patch(sid, { running: false, lastElapsedMs: Date.now() - t0c });
        get().addNote(answer || "Tasca finalitzada.", { id: sid });
        get().finalizeTurn(sid);
        return goalComputer;
      }

      // ── XAT WEB (proveïdor «web:<ia>»): parlar directes amb una IA de navegador ──
      // El provider «web:deepseek»/«web:chatgpt»/… fa que aquest xat escrigui la
      // pregunta al xat web oficial (finestra VISIBLE, compte de l'usuari) i llegeixi
      // la resposta de la pantalla. Un sol camí, el mateix historial i la mateixa
      // llista de xats que la resta: tots els xats queden UNIFICATS.
      const webId = sess.provider?.startsWith("web:") ? sess.provider.slice(4) : null;
      if (webId) {
        const t0w = Date.now();
        patch(sid, {
          running: true,
          error: null,
          stopped: false,
          lastThinking: null,
          startedAt: t0w,
          lastElapsedMs: null,
          stream: null,
        });
        get().pushStep(sid, {
          label: `Xat web · ${webId}`,
          detail: "Escrivint la pregunta al navegador intern (finestra visible)…",
          ok: true,
        });
        try {
          const out = await invoke<string>("web_ia_ask", {
            name: webId,
            question: text,
            waitSecs: 300,
          });
          get().pushStep(sid, {
            label: `Xat web · ${webId}`,
            detail: "Resposta rebuda; comprovant si cal crear fitxers al projecte…",
            ok: true,
          });
          resetLiveMaterialize();
          // INTEGRACIÓ AUTOMÀTICA DEL CODI WEB: si la resposta de la IA web porta
          // blocs de codi però cap fitxer «@file:», la IA LOCAL del projecte fa una
          // passada d'integració (revisa, adapta i emet «@file: ruta») i després
          // autoMaterialize els escriu SOLS a l'estructura real del workspace.
          // Resultat: el codi del xat web S'APLIQUA A LLOC, sense còpia i enganxa.
          let integrated: string | null = null;
          if (parseFileBlocks(out).length === 0 && /```/.test(out)) {
            get().pushStep(sid, {
              label: `Integració · codi web`,
              detail: "Adaptant el codi rebut de la web a l'estructura del projecte…",
              ok: true,
            });
            try {
              const adapted = await invoke<string>("send_prompt", {
                prompt:
                  "AQUESTA ÉS LA RESPOSTA D'UNA IA WEB (ChatGPT, Gemini, DeepSeek…) sobre " +
                  "una tasca de programació. INTEGRA-LA AL PROJECTE OBERT.\n\n" +
                  `TASCA ORIGINAL DE L'USUARI:\n${text}\n\n` +
                  `RESPUESTA AMB CODI DE LA IA WEB:\n${out.slice(0, 24000)}\n\n` +
                  "Revisa el codi rebut (no el creguis cec): adapta-lo (imports, noms, " +
                  "estructura i llenguatge del workspace) i després emet CADA fitxer com una " +
                  "línia «@file: ruta/relativa» seguida del seu contingut COMPLET — igual que " +
                  "en «Crea al projecte». No expliquis res més que una frase de resum per fitxer. " +
                  "Si el codi de la web no serveix per al projecte actual, diu-ho i no escriguis res.",
                system:
                  "Ets l'instrument d'integració de NoOrbit: contingut de fitxers com a blocs " +
                  "«@file: ruta» completes. Sense més prosa que el resum mínim.",
                session: `${sid}#webintegrate`,
                provider: null,
                model: sess.model,
                // Pregunta real de l'usuari (aprenentatge local).
                question: text,
              });
              if (adapted && parseFileBlocks(adapted).length > 0) {
                integrated = adapted;
              }
            } catch {
              /* sense integració: ens quedem amb la resposta original */
            }
          }
          const finalText = integrated ?? out;
          const integratedNote = integrated
            ? "🧩 Codi de la IA web integrat al projecte (revisat i adaptat pel model local)."
            : null;
          // La IA web pot haver generat codi: la integració és la mateixa que la
          // de la IA local (blocs ``` extrets per autoMaterialize + @file:).
          const extra = await autoMaterialize(finalText);
          const agents = await applyAgentDirectives(out, sid);
          const todos = applyTodoDirectives(out);
          patch(sid, {
            running: false,
            startedAt: null,
            lastElapsedMs: Date.now() - t0w,
            lastResult:
              [stripAgentDirectiveLines(out), integratedNote, extra, agents, todos]
                .filter(Boolean)
                .join("\n\n") ||
              stripAgentDirectiveLines(out),
            lastThinking: null,
          });
        } catch (e) {
          patch(sid, {
            running: false,
            stopped: false,
            startedAt: null,
            lastElapsedMs: Date.now() - t0w,
            error: toErrorText(e),
          });
        }
        // Tanca del torn + cua, idèntic al camí normal (unificació real).
        const curW = sessOf(sid);
        const lastW = curW?.messages[curW.messages.length - 1];
        if (!lastW || lastW.role !== "assistant") get().finalizeTurn(sid);
        get().persistChats();
        await drainQueue(sid);
        return sessOf(sid)?.lastResult ?? null;
      }

      let msg = text;
      let sys: string | null = null;
      // Sense «Inclou el codi», un model local NO veu el fitxer que l'usuari
      // demana editar: el detectem al missatge («modifica src/app.py», «al
      // fitxer index.html…») i n'adjuntem el contingut COMPLET al prompt.
      // Sense això, la IA local «no edita codi» — només pot inventar-lo.
      if (!sess.includeContext) {
        const wsRoot = useWorkspaceStore.getState().root;
        if (wsRoot) {
          const attached = await attachReferencedFiles(text, wsRoot);
          if (attached) {
            msg = `${attached}\n\n---\nPREGUNTA / TASCA:\n${text}`;
            get().pushStep(sid, {
              label: "ia",
              detail: "Fitxer esmentat adjuntat com a context per poder-lo editar.",
              ok: true,
            });
          }
        }
      }
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
        const ctx = await get().buildContext(sid, undefined, text);
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
      // la pista de plataforma perquè el xat continue escrivint codi natiu;
      // si tampoc n'hi ha, la base de memòria + fitxers (MEMORY_SYSTEM).
      if (!sys && platformPreamble) sys = platformPreamble + CONTEXT_SYSTEM;
      if (!sys) sys = MEMORY_SYSTEM;

      // Checklist compartida: si hi ha tasques a la llista, les posem DAMUNT
      // del system prompt perquè la IA principal les veja i no se'n perga cap.
      const taskCtx = useTaskListStore.getState().asContext();
      const withTasks = (s: string | null): string | null => (taskCtx ? taskCtx + (s ?? "") : s);
      if (taskCtx) sys = taskCtx + sys;

      // Millora de consum de memòria/tokens: si el proveïdor és local i el
      // context (amb codi del projecte) és gran, COMMENÇA amb el context COMPACTAT
      // per això no esperar al primer intent que falli per OOM.
      const sendStart = (() => {
        const sp = sessOf(sid);
        const prov = (sp?.provider ?? "").toLowerCase();
        const local = prov === "" || prov === "ollama";
        // Context adjunt actiu (l'usuari va activar «inclou el codi») →
        // per a models locals COMMENÇA amb el context COMPACTAT (alliberant RAM) per
        // ahorrar memòria sense perdre l'esperit del programa.
        const bigCtx = sess.includeContext ?? false;
        return local && bigCtx ? 1 : 0;
      })();

      // Reconstruïble: per cada nivell de reintent, munte el «prompt final»
      // amb MENYS context (o sense). S'usa dins «runWithOOMRetry».
      const rebuildForLevel = async (level: number): Promise<{ m: string; h: HistoryTurn[]; s: string | null }> => {
        if (level === 0) return { m: msg, h: history, s: sys };
        const sysBase = platformPreamble ? platformPreamble + CONTEXT_SYSTEM : (sess.includeContext ? CONTEXT_SYSTEM : MEMORY_SYSTEM);
        if (level === 1) {
          // Context COMPACTAT (no reduït): el backend condensa el fil en un
          // «fitxer de memòria» DENS que conserva l'objectiu, el pla,
          // l'esquelet del codi, les rutes i les tasques — i mana la IA
          // CONTINUAR la feina a trossos en lloc de començar de zero.
          // Abans, si la RAM no donava, es retallava tot a ~1.500 caràcters i
          // la IA perdia el fil del programa.
          try {
            // 1) Alliberem RAM: descarreguem els models que Ollama té carregats.
            await invoke<number>("ollama_unload_models", { model: null }).catch(() => 0);
          } catch {
            /* sense permís/servei: es continua igual */
          }
          const memoria = await invoke<string>("agent_compact_context", {
            input: { history, task: text },
          }).catch(() => "");
          if (memoria) {
            return { m: memoria, h: [], s: withTasks(sysBase) };
          }
          // Si la compactació no és possible, es fa el que es podia: reduït.
          const small = sess.includeContext ? await get().buildContext(sid, 1500, text) : "";
          const m = small ? `${small}\n\n---\nPREGUNTA / TASCA:\n${text}` : text;
          return { m, h: history, s: withTasks(sysBase) };
        }
        if (level === 2) {
          // Sense context adjunt: només el text de l'usuari.
          return { m: text, h: history, s: withTasks(platformPreamble ? platformPreamble + CONTEXT_SYSTEM : MEMORY_SYSTEM) };
        }
        // Level 3: sense context I amb NOMÉS el fil bàsic (abans es BORRAVA
        // tota la conversa: en canviar de IA i pujar el context, el model
        // perdia el fil i «continua» no sabia què continuar).
        return {
          m: text,
          h: threadOf(history),
          s: withTasks(platformPreamble ? platformPreamble + CONTEXT_SYSTEM : MEMORY_SYSTEM),
        };
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
            const agents = await applyAgentDirectives(out, sid);
            const delegated = await applyDelegateDirectives(out, sid);
            const todos = applyTodoDirectives(out);
            patch(sid, {
              running: false,
              startedAt: null,
              lastElapsedMs: Date.now() - t0,
              lastResult:
                [stripAgentDirectiveLines(out), delegated, extra, agents, todos]
                  .filter(Boolean)
                  .join("\n\n") || stripAgentDirectiveLines(out),
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
          const oomLevelSend = await runWithOOMRetry(sid, async (level) => {
            const built = await rebuildForLevel(level);
            return await invoke<string>("send_prompt_stream", {
              prompt: built.m,
              system: built.s,
              session: sid,
              provider: sess.provider,
              model: sess.model,
              history: built.h,
              images: imgs.length > 0 ? imgs : undefined,
              // Pregunta real de l'usuari (aprenentatge local): «built.m» pot
              // ser el context del projecte + la pregunta.
              question: text,
            });
          }, sendStart);
          const out = oomLevelSend.value;
          const thinking = await fetchThinking(sid);
          const oom = oomNote(oomLevelSend.level);
          get().pushStep(sid, {
            label: "ia",
            detail: "Resposta rebuda; comprovant si cal crear o desar fitxers al projecte…",
            ok: true,
          });
          resetLiveMaterialize();
          const extra = await autoMaterialize(out);
          const agents = await applyAgentDirectives(out, sid);
          const delegated = await applyDelegateDirectives(out, sid);
          const todos = applyTodoDirectives(out);
          patch(sid, {
            running: false,
            startedAt: null,
            lastElapsedMs: Date.now() - t0,
            lastResult:
              [stripAgentDirectiveLines(out), delegated, extra, agents, todos, oom]
                .filter(Boolean)
                .join("\n\n") || stripAgentDirectiveLines(out),
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

    beginTurn: (id, text, images) => {
      // Torn nou: llença l'estat de la materialització en viu (mapa de
      // fitxers ja escrits i reprogramacions pendents del torn anterior).
      resetLiveMaterialize();
      set((s) => ({
        sessions: s.sessions.map((x) =>
          x.id === id
            ? {
                ...x,
                messages: [
                  ...x.messages,
                  {
                    role: "user" as const,
                    text,
                    at: Date.now(),
                    ...(images && images.length > 0 ? { images } : {}),
                  },
                ],
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
      // Comptador de tokens del torn (si el backend l'ha emès).
      const usage = get().usageBySession[id];
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
                    usage,
                  },
                ],
              }
            : x
        ),
      }));
    },

    addNote: (text, opts) => {
      const sid = target(opts?.id);
      const role = opts?.role ?? "assistant";
      set((s) => ({
        sessions: s.sessions.map((x) =>
          x.id === sid
            ? {
                ...x,
                messages: [
                  ...x.messages,
                  {
                    role,
                    text,
                    at: Date.now(),
                    ...(opts?.images && opts.images.length > 0 ? { images: opts.images } : {}),
                  },
                ],
                title: x.title || text.slice(0, 48),
              }
            : x
        ),
      }));
      get().persistChats();
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

/// Canvia el calaix de xats al projecte `root`: desen els xats actuals al
/// calaix del projecte d'origen i carreguen els del projecte nou (o un xat
/// «main» buit si és el primer treball en aquesta carpeta).
function switchChatBucket(root: string | null) {
  if (root === currentBucket) return;
  const store = useAgentStore.getState();
  try {
    localStorage.setItem(
      chatLsKey(currentBucket),
      JSON.stringify({
        activeId: store.activeId,
        sessions: store.sessions.map((s) => ({
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
          messages: s.messages.slice(-30).map((m) => ({
            role: m.role,
            text: m.text.slice(0, 16000),
            thinking: m.thinking ? String(m.thinking).slice(0, 10000) : null,
            steps: (m.steps ?? []).slice(0, 8),
            elapsedMs: m.elapsedMs ?? null,
            at: m.at,
            error: m.error ?? false,
          })),
        })),
      })
    );
  } catch {
    /* sense persistència: el canvi de projecte igualment ha de funcionar */
  }
  currentBucket = root;
  const loaded = loadChats(chatLsKey(root));
  useAgentStore.setState(loaded);
  store.persistChats();
}

// Cada vegada que s'obre un altre projecte (o se'n tanca un), els xats que
// es mostren són els D'AQUELL projecte: ja no queden els xats antics
// barrejats ni en obrir un de nou ni en reiniciar l'app.
useWorkspaceStore.subscribe((state, prev) => {
  if (state.root === prev.root) return;
  switchChatBucket(state.root);
});

/// Veritat si QUALSEVOL xat està generant ara mateix (per a indicadors
/// globals com la barra d'estat).
export function useAnyChatRunning(): boolean {
  return useAgentStore((s) => s.sessions.some((x) => x.running));
}
