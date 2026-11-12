/// Tipus públics de l'Agent (xat, sessions, resultats). Extrets d'agentStore
/// perquè es puguen importar sense carregar tot el store (ni els seus efectes
/// de Tauri): els tests i altres mòduls només necessiten les formes.

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

/// Resultat d'una execució d'agent (text + fitxers creats + passos).
export interface AgentResult {
  text: string;
  files: string[];
  steps: { label: string; detail: string; ok: boolean }[];
}

/// Una secció CERCA/NOU d'una edició quirúrgica «@edit:».
export interface EditSection {
  find: string;
  replace: string;
}

/// Bloc «@edit: ruta» amb totes les seccions CERCA/NOU acumulades.
export interface EditBlock {
  path: string;
  edits: EditSection[];
}