import { useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import { ChevronDown } from "lucide-react";
import { useT } from "../../i18n";

/// GUIA VISUAL D'USUARI (SVG a codi): un dibuix esquemàtic per cada cosa
/// important de NoOrbit. Els SVG usen variables del tema (var(--accent)…) per
/// canviar de color segons el tema escollit. Cada tema té un número i uns
/// passos (claus i18n «guide.tN»/«guide.tNsM»).

type Topic = {
  n: number;
  titleKey: string;
  svg: () => ReactNode;
  steps: string[];
};

/* ── peces de dibuix reutilitzables ─────────────────────────────────────── */

function Frame({ w = 330, h = 190, children }: { w?: number; h?: number; children: React.ReactNode }) {
  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="vg-svg" role="img" style={{ width: "100%", height: "auto" }}>
      {children}
    </svg>
  );
}

function Chip({ x, y, label, color = "var(--accent)", w }: { x: number; y: number; label: string; color?: string; w?: number }) {
  const width = w ?? label.length * 6.2 + 18;
  return (
    <g>
      <rect x={x} y={y} width={width} height={18} rx={9} fill={color} opacity={0.16} />
      <text x={x + width / 2} y={y + 13} textAnchor="middle" fontSize="9.5" fill={color} fontFamily="var(--font-ui)">
        {label}
      </text>
    </g>
  );
}

function Arrow({ x1, y1, x2, y2 }: { x1: number; y1: number; x2: number; y2: number }) {
  const mx = (x1 + x2) / 2;
  return (
    <g stroke="var(--text-3)" strokeWidth="1.4" fill="none">
      <path d={`M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`} />
      <path d={`M ${x2 - 5} ${y2 - 3} L ${x2} ${y2} L ${x2 - 5} ${y2 + 3}`} />
    </g>
  );
}

function Win({ x, y, w, h, title, tint = "var(--bg-3)" }: { x: number; y: number; w: number; h: number; title: string; tint?: string }) {
  return (
    <g>
      <rect x={x} y={y} width={w} height={h} rx={8} fill={"var(--bg-2)"} stroke={"var(--border-2)"} strokeWidth={1.2} />
      <rect x={x} y={y} width={w} height={16} rx={8} fill={tint} opacity={0.55} />
      <circle cx={x + 10} cy={y + 8} r={2.6} fill="var(--error)" opacity={0.8} />
      <circle cx={x + 17} cy={y + 8} r={2.6} fill="var(--warning)" opacity={0.8} />
      <circle cx={x + 24} cy={y + 8} r={2.6} fill="var(--success)" opacity={0.8} />
      <text x={x + w / 2} y={y + 12} textAnchor="middle" fontSize="8.5" fill="var(--text-2)">
        {title}
      </text>
    </g>
  );
}

/* ── 1. Obre un projecte ───────────────────────────────────────────────── */
function SvgProjecte() {
  return (
    <Frame>
      <Win x={14} y={22} w={170} h={140} title="NoOrbit — Inici" />
      <rect x={26} y={48} width={146} height={26} rx={6} fill="var(--accent-soft)" stroke="var(--accent)" strokeWidth={0.8} />
      <text x={99} y={64} textAnchor="middle" fontSize="9" fill="var(--accent-hover)">
        projecte/recent.carpetes
      </text>
      <rect x={26} y={82} width={146} height={26} rx={6} fill="var(--bg-3)" stroke="var(--border)" />
      <text x={99} y={98} textAnchor="middle" fontSize="9" fill="var(--text-1)">
        Nova carpeta…
      </text>
      <text x={99} y={122} textAnchor="middle" fontSize="8.5" fill="var(--text-3)">
        ⇧⌘N nou · ⌘⌥R recents
      </text>
      <Arrow x1={190} y1={90} x2={240} y2={90} />
      <Win x={244} y={30} w={72} h={120} title="Editor" />
      <rect x={252} y={56} width={56} height={7} rx={2} fill="var(--text-3)" opacity={0.5} />
      <rect x={252} y={70} width={44} height={7} rx={2} fill="var(--text-3)" opacity={0.35} />
      <rect x={252} y={84} width={52} height={7} rx={2} fill="var(--text-3)" opacity={0.5} />
      <Chip x={252} y={120} label="fitxer obert" />
    </Frame>
  );
}

/* ── 2. Parlar amb la IA ───────────────────────────────────────────────── */
function SvgXatIA() {
  return (
    <Frame>
      <Win x={14} y={22} w={190} h={140} title="Agent · xat" />
      <rect x={26} y={52} width={80} height={18} rx={9} fill="var(--accent)" opacity={0.85} />
      <text x={66} y={64} textAnchor="middle" fontSize="8" fill="#fff">
        la teva pregunta
      </text>
      <rect x={40} y={78} width={150} height={34} rx={8} fill="var(--bg-3)" stroke="var(--border-2)" />
      <text x={48} y={90} fontSize="8" fill="var(--text-2)">
        ⚙ passos visibles en viu
      </text>
      <text x={48} y={102} fontSize="8.5" fill="var(--text-0)">
        escriu la resposta ▋
      </text>
      <rect x={40} y={120} width={120} height={12} rx={4} fill="var(--bg-1)" stroke="var(--border)" />
      <text x={46} y={129} fontSize="7.5" fill="var(--text-3)">
        ```python · Copia
      </text>
      <Arrow x1={210} y1={80} x2={252} y2={64} />
      <Chip x={232} y={54} label="context del codi" />
      <Arrow x1={210} y1={104} x2={252} y2={118} />
      <Chip x={226} y={112} label="fitxers @file:" color="var(--success)" />
    </Frame>
  );
}

/* ── 3. Especialistes: crea i delega ──────────────────────────────────── */
function SvgAgents() {
  return (
    <Frame>
      <circle cx={90} cy={92} r={30} fill="var(--accent-soft)" stroke="var(--accent)" strokeWidth={1.4} />
      <text x={90} y={89} textAnchor="middle" fontSize="9" fill="var(--text-0)">
        IA principal
      </text>
      <text x={90} y={101} textAnchor="middle" fontSize="8" fill="var(--text-3)">
        del xat
      </text>
      <Arrow x1={124} y1={78} x2={214} y2={56} />
      <Arrow x1={124} y1={92} x2={214} y2={92} />
      <Arrow x1={124} y1={106} x2={214} y2={128} />
      <rect x={220} y={44} width={92} height={24} rx={7} fill="var(--bg-3)" stroke="var(--border-2)" />
      <text x={266} y={53} textAnchor="middle" fontSize="7.5" fill="var(--text-3)">
        @agent:
      </text>
      <text x={266} y={64} textAnchor="middle" fontSize="8.5" fill="var(--text-0)">
        crea especialista
      </text>
      <rect x={220} y={80} width={92} height={24} rx={7} fill="var(--bg-3)" stroke="var(--border-2)" />
      <text x={266} y={89} textAnchor="middle" fontSize="7.5" fill="var(--text-3)">
        @use:
      </text>
      <text x={266} y={100} textAnchor="middle" fontSize="8.5" fill="var(--text-0)">
        delega la subtasca
      </text>
      <rect x={220} y={116} width={92} height={24} rx={7} fill="var(--bg-3)" stroke="var(--border-2)" />
      <text x={266} y={125} textAnchor="middle" fontSize="7.5" fill="var(--text-3)">
        consolidació
      </text>
      <text x={266} y={136} textAnchor="middle" fontSize="8.5" fill="var(--text-0)">
        resposta integrada
      </text>
      <Chip x={20} y={152} label="els agents treballen EN PARAL·LEL (un per subtasca)" color="var(--accent-2)" w={200} />
    </Frame>
  );
}

/* ── 4. Llista de tasques compartida ─────────────────────────────────── */
function SvgTasques() {
  return (
    <Frame>
      <Win x={30} y={28} w={150} h={108} title="Checklist del xat" />
      <circle cx={46} cy={62} r={5} fill="none" stroke="var(--accent)" strokeWidth={1.4} />
      <text x={58} y={65} fontSize="8.5" fill="var(--text-0)">
        tasca pendent
      </text>
      <circle cx={46} cy={82} r={5} fill="var(--success)" opacity={0.85} />
      <text x={58} y={85} fontSize="8.5" fill="var(--text-3)">
        tasca feta ✓
      </text>
      <circle cx={46} cy={102} r={5} fill="none" stroke="var(--accent)" strokeWidth={1.4} />
      <text x={58} y={105} fontSize="8.5" fill="var(--text-0)">
        tasca pendent
      </text>
      <Arrow x1={186} y1={62} x2={228} y2={62} />
      <Chip x={214} y={54} label="«@todo: ...»" />
      <Arrow x1={186} y1={82} x2={228} y2={98} />
      <Chip x={210} y={90} label="«@done: ...»" color="var(--success)" />
      <text x={230} y={128} fontSize="8" fill="var(--text-2)">
        la IA i tu la
      </text>
      <text x={230} y={139} fontSize="8" fill="var(--text-2)">
        veieu igual
      </text>
      <Chip x={30} y={152} label="es injecta al context de TOTS els agents" color="var(--accent-2)" w={210} />
    </Frame>
  );
}

/* ── 5. IA de web: pregunta, sessió i codi aplicat ────────────────────── */
function SvgIaWeb() {
  return (
    <Frame w={330} h={200}>
      <Chip x={16} y={16} label="TU" color="var(--accent)" w={40} />
      <Arrow x1={62} y1={25} x2={108} y2={25} />
      <Win x={112} y={8} w={100} h={34} title="xat web visible" tint="var(--accent-soft)" />
      <Arrow x1={218} y1={25} x2={262} y2={25} />
      <Win x={266} y={8} w={50} h={34} title="IA" />
      <text x={291} y={29} textAnchor="middle" fontSize="8" fill="var(--text-0)">
        web
      </text>
      <Arrow x1={291} y1={46} x2={291} y2={68} />
      <text x={291} y={82} textAnchor="middle" fontSize="8" fill="var(--text-3)">
        resposta +
      </text>
      <text x={291} y={93} textAnchor="middle" fontSize="8" fill="var(--text-3)">
        blocs de codi
      </text>
      <text x={30} y={74} fontSize="9" fill="var(--warning)">
        🔑 entra UNA vegada
      </text>
      <text x={30} y={86} fontSize="8" fill="var(--text-3)">
        amb el teu compte:
      </text>
      <text x={30} y={97} fontSize="8" fill="var(--text-3)">
        queda logejat
      </text>
      <rect x={16} y={108} width={298} height={62} rx={10} fill="var(--bg-2)" stroke="var(--border-2)" />
      <text x={26} y={124} fontSize="9" fill="var(--text-0)">
        🧩 INTEGRACIÓ AUTOMÀTICA
      </text>
      <text x={26} y={137} fontSize="8" fill="var(--text-2)">
        el model LOCAL revisa el codi de la web,
      </text>
      <text x={26} y={149} fontSize="8" fill="var(--text-2)">
        l'adapta i l'escriu amb «@file:» —
      </text>
      <text x={26} y={161} fontSize="8" fill="var(--text-2)">
        els fitxers apareixen al projecte i al diff
      </text>
    </Frame>
  );
}

/* ── 6. Revisar el codi de la IA ─────────────────────────────────────── */
function SvgRevisio() {
  return (
    <Frame>
      <Win x={14} y={26} w={196} h={136} title="editor · revisió diff" />
      <rect x={26} y={56} width={84} height={12} rx={3} fill="rgba(248,113,113,0.22)" />
      <rect x={26} y={76} width={104} height={12} rx={3} fill="rgba(74,222,128,0.22)" />
      <rect x={26} y={96} width={70} height={12} rx={3} fill="rgba(74,222,128,0.22)" />
      <Arrow x1={216} y1={92} x2={246} y2={64} />
      <Chip x={228} y={56} label="⌘↵ accepta" color="var(--success)" />
      <Arrow x1={216} y1={104} x2={246} y2={128} />
      <Chip x={228} y={120} label="⌘⌫ rebutja" color="var(--error)" />
      <text x={214} y={152} fontSize="8" fill="var(--text-3)">
        cap línia entra sense el teu vist i plau
      </text>
    </Frame>
  );
}

/* ── 7. Compte d'usuari ──────────────────────────────────────────────── */
function SvgCompte() {
  return (
    <Frame>
      <rect x={10} y={70} width={310} height={18} rx={9} fill="var(--bg-2)" stroke="var(--border)" />
      <circle cx={280} cy={79} r={6} fill="var(--accent)" />
      <text x={280} y={82} textAnchor="middle" fontSize="7" fill="#fff">
        J
      </text>
      <text x={290} y={82} fontSize="8" fill="var(--text-1)">
        tu@correu.cat
      </text>
      <Arrow x1={278} y1={92} x2={252} y2={112} />
      <Win x={120} y={56} w={130} h={100} title="Compte de NoOrbit" tint="var(--accent-soft)" />
      <text x={185} y={82} textAnchor="middle" fontSize="9" fill="var(--text-0)">
        Crea el compte
      </text>
      <text x={185} y={96} textAnchor="middle" fontSize="8" fill="var(--text-3)">
        correu + contrasenya
      </text>
      <text x={185} y={112} textAnchor="middle" fontSize="8" fill="var(--text-3)">
        (SHA-256 + sal, al teu Mac)
      </text>
      <Chip x={150} y={138} label="IAs connectades (per al futur)" color="var(--accent-2)" w={150} />
    </Frame>
  );
}

/* ── catàleg final ────────────────────────────────────────────────────── */
const TOPICS: Topic[] = [
  { n: 1, titleKey: "guide.t1", svg: SvgProjecte, steps: ["guide.t1s1", "guide.t1s2", "guide.t1s3"] },
  { n: 2, titleKey: "guide.t2", svg: SvgXatIA, steps: ["guide.t2s1", "guide.t2s2", "guide.t2s3"] },
  { n: 3, titleKey: "guide.t3", svg: SvgAgents, steps: ["guide.t3s1", "guide.t3s2", "guide.t3s3"] },
  { n: 4, titleKey: "guide.t4", svg: SvgTasques, steps: ["guide.t4s1", "guide.t4s2", "guide.t4s3"] },
  { n: 5, titleKey: "guide.t5", svg: SvgIaWeb, steps: ["guide.t5s1", "guide.t5s2", "guide.t5s3"] },
  { n: 6, titleKey: "guide.t6", svg: SvgRevisio, steps: ["guide.t6s1", "guide.t6s2", "guide.t6s3"] },
  { n: 7, titleKey: "guide.t7", svg: SvgCompte, steps: ["guide.t7s1", "guide.t7s2", "guide.t7s3"] },
];

export default function VisualGuide() {
  const { t } = useT();
  const [open, setOpen] = useState(true);

  return (
    <div className="vg">
      <button className="vg-toggle" onClick={() => setOpen(!open)}>
        <span className="vg-toggle-title">{t("guide.title")}</span>
        <ChevronDown size={14} style={{ transform: open ? "rotate(0deg)" : "rotate(-90deg)", transition: "transform .15s" }} />
      </button>
      {open && (
        <p className="vg-intro">{t("guide.intro")}</p>
      )}
      {open && (
        <div className="vg-grid">
          {TOPICS.map((topic) => (
            <div key={topic.n} className="vg-card">
              <div className="vg-card-title">
                <span className="vg-num">{topic.n}</span> {t(topic.titleKey)}
              </div>
              {topic.svg()}
              <ol className="vg-steps">
                {topic.steps.map((k) => (
                  <li key={k}>{t(k)}</li>
                ))}
              </ol>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
