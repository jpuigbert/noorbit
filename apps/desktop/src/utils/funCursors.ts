/// Cursors divertits de NoOrbit: dibuixats a mà com a SVG petits (32 px,
/// com el punter del ratolí) i convertits a data-URI per al CSS `cursor`.
/// Cada zona de l'app té el seu personatge: coet per defecte, robot a
/// l'editor, ovni al terminal, àlien al xat, satèl·lit als fitxers…
/// Mentre no fas res, el cursor es disfressa (ànec, pizza, dònut, gat…)
/// i la disfressa canvia cada 5 minuts. Tots duen contorn fosc perquè
/// es llegeixin igual sobre els temes clars que sobre els foscos.

export interface FunCursor {
  /// Nom en català per a la previsualització de Configuració.
  name: string;
  /// Valor llest per a `cursor:` (url + hotspot + alternativa semàntica).
  css: string;
  /// Data-URI sol, per a les miniatures <img> de Configuració.
  img: string;
}

/// Embolcall comú: contorn «tinta espacial» gruixut i unions rodones.
const S = (body: string) =>
  `<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32' viewBox='0 0 32 32'>` +
  `<g stroke='#241f36' stroke-width='1.6' stroke-linecap='round' stroke-linejoin='round'>${body}</g></svg>`;

function make(name: string, body: string, hx: number, hy: number, fallback: string): FunCursor {
  const img = `data:image/svg+xml,${encodeURIComponent(S(body))}`;
  return { name, img, css: `url("${img}") ${hx} ${hy}, ${fallback}` };
}

// ── DISFRESSES PER ZONA ────────────────────────────────────────────────

const STAR_PATH =
  "M16 3 L19.2 11.6 L28.4 12 L21.2 17.7 L23.6 26.5 L16 21.5 L8.4 26.5 L10.8 17.7 L3.6 12 L12.8 11.6 Z";

/// Coet diagonal (punta a dalt-esquerra, com la fletxa clàssica).
const rocket = make(
  "Coet",
  `<g transform='rotate(-45 16 16)'>
    <path d='M16 2 L21.5 12 H10.5 Z' fill='#ff5d5d'/>
    <rect x='10.5' y='12' width='11' height='11' rx='3' fill='#f4f6ff'/>
    <circle cx='16' cy='16.5' r='2.6' fill='#7dd3fc'/>
    <path d='M10.5 18 L5 25 L10.5 23 Z' fill='#ff5d5d'/>
    <path d='M21.5 18 L27 25 L21.5 23 Z' fill='#ff5d5d'/>
    <path d='M13 23 Q16 30 19 23 Q16 25.5 13 23 Z' fill='#ffb020'/>
  </g>`,
  6, 6, "default"
);

const ufo = make(
  "Ovni",
  `<path d='M12 21 L7 30 H25 L20 21 Z' fill='#ffd166' opacity='0.45' stroke='none'/>
  <circle cx='16' cy='11' r='6.5' fill='#a7f3d0'/>
  <circle cx='13.5' cy='10' r='1.2' fill='#241f36' stroke='none'/>
  <circle cx='18.5' cy='10' r='1.2' fill='#241f36' stroke='none'/>
  <ellipse cx='16' cy='18' rx='13' ry='5.5' fill='#8b5cf6'/>
  <circle cx='8' cy='18.5' r='1.5' fill='#ffd166'/>
  <circle cx='16' cy='20' r='1.5' fill='#ffd166'/>
  <circle cx='24' cy='18.5' r='1.5' fill='#ffd166'/>`,
  16, 4, "default"
);

const star = make(
  "Estrella",
  `<path d='${STAR_PATH}' fill='#ffd166'/>
  <circle cx='12.5' cy='14.5' r='1.2' fill='#241f36' stroke='none'/>
  <circle cx='19.5' cy='14.5' r='1.2' fill='#241f36' stroke='none'/>
  <path d='M12.5 18.5 Q16 21.5 19.5 18.5' fill='none' stroke-width='1.5'/>`,
  16, 3, "default"
);

const planet = make(
  "Planeta",
  `<circle cx='16' cy='16' r='9' fill='#fb7185'/>
  <circle cx='13' cy='13' r='2' fill='#e11d48' stroke='none' opacity='0.6'/>
  <circle cx='19' cy='19' r='1.5' fill='#e11d48' stroke='none' opacity='0.6'/>
  <ellipse cx='16' cy='16' rx='14.5' ry='4.5' fill='none' stroke='#7dd3fc' stroke-width='2.4' transform='rotate(-18 16 16)'/>`,
  16, 7, "default"
);

const comet = make(
  "Cometa",
  `<path d='M3 29 Q8 22 12 18' fill='none' stroke='#7dd3fc' stroke-width='3' opacity='0.8'/>
  <path d='M6 29 Q10 24 13 21' fill='none' stroke='#a7f3d0' stroke-width='1.8' opacity='0.8'/>
  <circle cx='19' cy='13' r='7.5' fill='#ffd166'/>
  <circle cx='16.5' cy='11.5' r='1.1' fill='#241f36' stroke='none'/>
  <circle cx='21.5' cy='11.5' r='1.1' fill='#241f36' stroke='none'/>
  <path d='M16.5 15.5 Q19 17.5 21.5 15.5' fill='none' stroke-width='1.4'/>`,
  19, 5, "default"
);

/// Robot simpàtic: el cursor de l'editor de codi.
const robot = make(
  "Robot",
  `<line x1='16' y1='8' x2='16' y2='4'/>
  <circle cx='16' cy='3.5' r='2' fill='#ff5d5d'/>
  <rect x='4' y='13' width='3.5' height='6' rx='1.5' fill='#60a5fa'/>
  <rect x='24.5' y='13' width='3.5' height='6' rx='1.5' fill='#60a5fa'/>
  <rect x='7' y='8' width='18' height='15' rx='4' fill='#93c5fd'/>
  <circle cx='12' cy='14.5' r='2.2' fill='#241f36' stroke='none'/>
  <circle cx='20' cy='14.5' r='2.2' fill='#241f36' stroke='none'/>
  <circle cx='12.8' cy='13.7' r='0.7' fill='#f4f6ff' stroke='none'/>
  <circle cx='20.8' cy='13.7' r='0.7' fill='#f4f6ff' stroke='none'/>
  <rect x='12' y='18.5' width='8' height='2.4' rx='1.2' fill='#241f36' stroke='none'/>`,
  16, 2, "text"
);

/// Invasor pixelat: el cursor del terminal.
const invader = make(
  "Invasor",
  `<path d='M10 4 H14 V8 H18 V4 H22 V8 H26 V12 H28 V20 H24 V24 H20 V20 H12 V24 H8 V20 H4 V12 H6 V8 H10 Z' fill='#4ade80'/>
  <rect x='10' y='12' width='4' height='4' fill='#241f36' stroke='none'/>
  <rect x='18' y='12' width='4' height='4' fill='#241f36' stroke='none'/>`,
  16, 4, "text"
);

/// Àlien del xat: ulls grossos i somriure tímid.
const alien = make(
  "Àlien",
  `<ellipse cx='16' cy='15' rx='10' ry='11.5' fill='#c4b5fd'/>
  <ellipse cx='11.5' cy='14' rx='2.8' ry='4' fill='#241f36' stroke='none' transform='rotate(-12 11.5 14)'/>
  <ellipse cx='20.5' cy='14' rx='2.8' ry='4' fill='#241f36' stroke='none' transform='rotate(12 20.5 14)'/>
  <circle cx='12.3' cy='12.6' r='0.8' fill='#f4f6ff' stroke='none'/>
  <circle cx='21.3' cy='12.6' r='0.8' fill='#f4f6ff' stroke='none'/>
  <path d='M13 21.5 Q16 23.5 19 21.5' fill='none' stroke-width='1.4'/>`,
  16, 3, "default"
);

/// Satèl·lit: el cursor de l'arbre de fitxers.
const satellite = make(
  "Satèl·lit",
  `<line x1='10' y1='16' x2='14' y2='16'/>
  <line x1='18' y1='16' x2='22' y2='16'/>
  <rect x='2' y='12.5' width='8' height='7' rx='1' fill='#60a5fa'/>
  <line x1='6' y1='12.5' x2='6' y2='19.5' stroke-width='1'/>
  <rect x='22' y='12.5' width='8' height='7' rx='1' fill='#60a5fa'/>
  <line x1='26' y1='12.5' x2='26' y2='19.5' stroke-width='1'/>
  <rect x='12.5' y='12.5' width='7' height='7' rx='1.5' fill='#fbbf24'/>
  <line x1='16' y1='12.5' x2='16' y2='8'/>
  <circle cx='16' cy='6.5' r='2.5' fill='#f4f6ff'/>`,
  16, 4, "pointer"
);

/// Vareta màgica: el cursor dels botons i enllaços (la punta és l'estel).
const wand = make(
  "Vareta",
  `<line x1='6' y1='27' x2='17' y2='16' stroke='#8b5cf6' stroke-width='3.5'/>
  <path d='M21 4 L22.6 8.4 L27 10 L22.6 11.6 L21 16 L19.4 11.6 L15 10 L19.4 8.4 Z' fill='#ffd166'/>
  <path d='M27 3 V7 M25 5 H29' stroke='#ff5d5d' stroke-width='1.5'/>
  <path d='M12 5 V9 M10 7 H14' stroke='#7dd3fc' stroke-width='1.5'/>
  <path d='M26 18 V22 M24 20 H28' stroke='#4ade80' stroke-width='1.5'/>`,
  21, 10, "pointer"
);

/// Llapis amb la punta a baix-esquerra: camps de text.
const pencil = make(
  "Llapis",
  `<path d='M5 27 L7.5 20.5 L21 7 L25 11 L11.5 24.5 Z' fill='#fbbf24'/>
  <path d='M5 27 L7.5 20.5 L11.5 24.5 Z' fill='#fde68a'/>
  <path d='M5 27 L6.4 23.4 L8.6 25.6 Z' fill='#241f36' stroke='none'/>
  <path d='M21 7 L23.5 4.5 Q25 3 26.5 4.5 L27.5 5.5 Q29 7 27.5 8.5 L25 11 Z' fill='#fb7185'/>
  <line x1='9' y1='22' x2='22.5' y2='8.5' stroke-width='1' opacity='0.5'/>`,
  5, 27, "text"
);

const ARROWS_BODY =
  `<line x1='7' y1='16' x2='25' y2='16' stroke-width='2.2'/>
  <path d='M7 16 L13 10.5 V21.5 Z' fill='#7dd3fc'/>
  <path d='M25 16 L19 10.5 V21.5 Z' fill='#7dd3fc'/>
  <circle cx='16' cy='16' r='5.5' fill='#ffd166'/>
  <circle cx='14.2' cy='14.8' r='0.9' fill='#241f36' stroke='none'/>
  <circle cx='17.8' cy='14.8' r='0.9' fill='#241f36' stroke='none'/>
  <path d='M14 17.8 Q16 19.4 18 17.8' fill='none' stroke-width='1.2'/>`;

const arrowsX = make("Fletxes", ARROWS_BODY, 16, 16, "ew-resize");
const arrowsY = make(
  "Fletxes verticals",
  `<g transform='rotate(90 16 16)'>${ARROWS_BODY}</g>`,
  16, 16, "ns-resize"
);

/// Urpa de fira: mentre arrossegues una nansa de redimensionat.
const claw = make(
  "Urpa",
  `<rect x='11' y='2' width='10' height='6' rx='2' fill='#ff5d5d'/>
  <line x1='16' y1='8' x2='16' y2='12' stroke-width='2'/>
  <path d='M16 12 L8 17 L10 24' fill='none' stroke='#94a3b8' stroke-width='3'/>
  <path d='M16 12 L24 17 L22 24' fill='none' stroke='#94a3b8' stroke-width='3'/>
  <path d='M16 12 V22' fill='none' stroke='#94a3b8' stroke-width='3'/>
  <circle cx='16' cy='12' r='2.5' fill='#fbbf24'/>`,
  16, 14, "grabbing"
);

/// Rellotge de sorra amb estels: la IA està treballant.
const hourglass = make(
  "Rellotge",
  `<line x1='8' y1='4' x2='24' y2='4' stroke-width='2.4'/>
  <line x1='8' y1='28' x2='24' y2='28' stroke-width='2.4'/>
  <path d='M10 5 H22 L16.8 15 Q16 16 15.2 15 Z' fill='#7dd3fc'/>
  <path d='M10 27 H22 L16.8 17 Q16 16 15.2 17 Z' fill='#7dd3fc'/>
  <path d='M12.5 26.5 H19.5 L16 20 Z' fill='#ffd166' stroke='none'/>
  <circle cx='16' cy='13' r='1' fill='#ffd166' stroke='none'/>
  <path d='M26 8 V12 M24 10 H28' stroke='#ffd166' stroke-width='1.4'/>
  <path d='M4 20 V24 M2 22 H6' stroke='#fb7185' stroke-width='1.4'/>`,
  16, 16, "progress"
);

/// Lluna amb cara: barres de menú i d'estat.
const moon = make(
  "Lluna",
  `<path d='M21 3.5 A12.5 12.5 0 1 0 21 28.5 A10 10 0 1 1 21 3.5 Z' fill='#fde68a'/>
  <circle cx='12' cy='14' r='1.1' fill='#241f36' stroke='none'/>
  <path d='M10.5 19 Q13 21 15.5 19' fill='none' stroke-width='1.3'/>
  <path d='M25 8 V12 M23 10 H27' stroke='#7dd3fc' stroke-width='1.4'/>`,
  14, 16, "default"
);

/// Estrella de festa: surt uns segons quan una tasca acaba.
const party = make(
  "Festa",
  `<path d='${STAR_PATH}' fill='#ffd166'/>
  <path d='M16 0.5 L20 7 H12 Z' fill='#ff5d5d'/>
  <circle cx='16' cy='1' r='1.6' fill='#7dd3fc'/>
  <circle cx='12.5' cy='15' r='1.2' fill='#241f36' stroke='none'/>
  <circle cx='19.5' cy='15' r='1.2' fill='#241f36' stroke='none'/>
  <path d='M12 19 Q16 23 20 19' fill='none' stroke-width='1.5'/>
  <circle cx='5' cy='5' r='1.4' fill='#4ade80' stroke='none'/>
  <circle cx='28' cy='5' r='1.4' fill='#fb7185' stroke='none'/>
  <circle cx='29' cy='22' r='1.4' fill='#7dd3fc' stroke='none'/>
  <circle cx='3' cy='22' r='1.4' fill='#fbbf24' stroke='none'/>`,
  16, 3, "default"
);

// ── DISFRESSES DE «NO FAIG RES» (canvien cada 5 minuts) ────────────────

const duck = make(
  "Ànec",
  `<ellipse cx='13' cy='20' rx='10' ry='8' fill='#ffd166'/>
  <path d='M8 19 Q13 15 17 19 Q13 23 8 19 Z' fill='#fbbf24' stroke-width='1.2'/>
  <circle cx='22' cy='10' r='6.5' fill='#ffd166'/>
  <path d='M28 9 L31.5 11 L28 13 Z' fill='#fb923c'/>
  <circle cx='23.5' cy='8.5' r='1.2' fill='#241f36' stroke='none'/>`,
  22, 3, "default"
);

const pizza = make(
  "Pizza",
  `<path d='M16 3 L28 25 Q16 30 4 25 Z' fill='#fbbf24'/>
  <path d='M4 25 Q16 30 28 25 L26.8 22.8 Q16 27 5.2 22.8 Z' fill='#f59e0b'/>
  <circle cx='15' cy='13' r='2' fill='#ef4444'/>
  <circle cx='19.5' cy='18.5' r='2' fill='#ef4444'/>
  <circle cx='11.5' cy='19.5' r='1.8' fill='#ef4444'/>`,
  16, 3, "default"
);

const donut = make(
  "Dònut",
  `<path fill-rule='evenodd' d='M16 3.5 A12.5 12.5 0 1 0 16 28.5 A12.5 12.5 0 1 0 16 3.5 Z M16 11.5 A4.5 4.5 0 1 1 16 20.5 A4.5 4.5 0 1 1 16 11.5 Z' fill='#f472b6'/>
  <line x1='9' y1='9' x2='11' y2='10.5' stroke='#7dd3fc' stroke-width='1.8'/>
  <line x1='21' y1='8' x2='23' y2='9.5' stroke='#ffd166' stroke-width='1.8'/>
  <line x1='24' y1='17' x2='26' y2='18.5' stroke='#4ade80' stroke-width='1.8'/>
  <line x1='7' y1='18' x2='9' y2='19.5' stroke='#fbbf24' stroke-width='1.8'/>
  <line x1='13' y1='24' x2='15' y2='25.5' stroke='#7dd3fc' stroke-width='1.8'/>
  <line x1='20' y1='23' x2='22' y2='24.5' stroke='#f4f6ff' stroke-width='1.8'/>`,
  16, 3, "default"
);

const cat = make(
  "Gat",
  `<path d='M8 12 L6 3 L14 8.5 Z' fill='#fb923c'/>
  <path d='M24 12 L26 3 L18 8.5 Z' fill='#fb923c'/>
  <circle cx='16' cy='17' r='10.5' fill='#fb923c'/>
  <circle cx='12' cy='15' r='1.4' fill='#241f36' stroke='none'/>
  <circle cx='20' cy='15' r='1.4' fill='#241f36' stroke='none'/>
  <path d='M14.5 19 L17.5 19 L16 21 Z' fill='#fb7185' stroke-width='1'/>
  <path d='M16 21 Q14 23 12 22 M16 21 Q18 23 20 22' fill='none' stroke-width='1.2'/>
  <line x1='4' y1='17' x2='9' y2='18' stroke-width='1'/>
  <line x1='4' y1='21' x2='9' y2='20.5' stroke-width='1'/>
  <line x1='28' y1='17' x2='23' y2='18' stroke-width='1'/>
  <line x1='28' y1='21' x2='23' y2='20.5' stroke-width='1'/>`,
  16, 3, "default"
);

const ghost = make(
  "Fantasma",
  `<path d='M7 28 V14 A9 9 0 0 1 25 14 V28 L22 25.5 L19 28 L16 25.5 L13 28 L10 25.5 Z' fill='#f4f6ff'/>
  <ellipse cx='12.5' cy='14' rx='1.6' ry='2.4' fill='#241f36' stroke='none'/>
  <ellipse cx='19.5' cy='14' rx='1.6' ry='2.4' fill='#241f36' stroke='none'/>
  <ellipse cx='16' cy='19.5' rx='1.8' ry='2.2' fill='#241f36' stroke='none'/>
  <circle cx='9.5' cy='18' r='1.2' fill='#f9a8d4' stroke='none'/>
  <circle cx='22.5' cy='18' r='1.2' fill='#f9a8d4' stroke='none'/>`,
  16, 5, "default"
);

const rainbow = make(
  "Arc de Sant Martí",
  `<path d='M4 25 A12 12 0 0 1 28 25' fill='none' stroke='#ef4444' stroke-width='3.4'/>
  <path d='M7.4 25 A8.6 8.6 0 0 1 24.6 25' fill='none' stroke='#fbbf24' stroke-width='3.4'/>
  <path d='M10.8 25 A5.2 5.2 0 0 1 21.2 25' fill='none' stroke='#4ade80' stroke-width='3.4'/>
  <circle cx='5' cy='25' r='3.5' fill='#f4f6ff'/>
  <circle cx='27' cy='25' r='3.5' fill='#f4f6ff'/>`,
  16, 13, "default"
);

const icecream = make(
  "Gelat",
  `<path d='M9.5 15 H22.5 L16 29 Z' fill='#f59e0b'/>
  <line x1='12' y1='17' x2='17.5' y2='24' stroke-width='0.9' opacity='0.6'/>
  <line x1='20' y1='17' x2='14.5' y2='24' stroke-width='0.9' opacity='0.6'/>
  <circle cx='16' cy='10.5' r='7' fill='#f9a8d4'/>
  <circle cx='16' cy='3.5' r='2' fill='#ef4444'/>
  <circle cx='13.5' cy='9' r='0.9' fill='#241f36' stroke='none'/>
  <circle cx='18.5' cy='9' r='0.9' fill='#241f36' stroke='none'/>
  <path d='M13.5 12.5 Q16 14.5 18.5 12.5' fill='none' stroke-width='1.2'/>`,
  16, 2, "default"
);

const coolstar = make(
  "Estrella guai",
  `<path d='${STAR_PATH}' fill='#ffd166'/>
  <path d='M9 12.5 H23 L21.5 17 Q19 18.5 16.8 16.5 L16 15.5 L15.2 16.5 Q13 18.5 10.5 17 Z' fill='#241f36' stroke='none'/>
  <line x1='9' y1='12.5' x2='6' y2='11' stroke-width='1.4'/>
  <line x1='23' y1='12.5' x2='26' y2='11' stroke-width='1.4'/>
  <path d='M12 20 Q16 23 20 20' fill='none' stroke-width='1.5'/>`,
  16, 3, "default"
);

// ── COL·LECCIONS ───────────────────────────────────────────────────────

/// Cursors «normals» de la zona per defecte: roten cada 5 minuts.
export const DEFAULT_SET: FunCursor[] = [rocket, ufo, star, planet, comet];

/// Disfresses quan l'usuari no fa res (1 min sense tocar res): cada 5 minuts
/// en surt una de nova.
export const IDLE_SET: FunCursor[] = [duck, pizza, donut, cat, ghost, rainbow, icecream, coolstar];

export type FunZone =
  | "default"
  | "code"
  | "terminal"
  | "chat"
  | "files"
  | "pointer"
  | "text"
  | "resizeX"
  | "resizeY"
  | "grab"
  | "busy"
  | "nav"
  | "party";

/// Personatge fix de cada zona de la interfície.
export const ZONE_CURSORS: Record<Exclude<FunZone, "default">, FunCursor> = {
  code: robot,
  terminal: invader,
  chat: alien,
  files: satellite,
  pointer: wand,
  text: pencil,
  resizeX: arrowsX,
  resizeY: arrowsY,
  grab: claw,
  busy: hourglass,
  nav: moon,
  party,
};

/// Miniatures per a la previsualització de Configuració.
export const FUN_PREVIEW: FunCursor[] = [...DEFAULT_SET, ...IDLE_SET];