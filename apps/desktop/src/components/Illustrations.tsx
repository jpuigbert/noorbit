/// Il·lustracions SVG pròpies de NoOrbit (vector plató, ~1 KB cada una):
/// amenitzen els buits («no hi ha fitxers obert», benvinguda, servidor)
/// sense cap imatge binària que engregar el paquet. El colors venen del
/// TEMA ACTIU:Accent «currentColor» + accent soft, així que changen de
/// colors amb el tema que es trii (verd i daurat included).
export function ArtEmpty({ size = 120 }: { size?: number }) {
  return (
    <svg width={size} height={size * 0.72} viewBox="0 0 160 116" fill="none" aria-hidden>
      <defs>
        <linearGradient id="noa-grad" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="var(--accent)" />
          <stop offset="100%" stopColor="var(--accent-2, var(--accent))" />
        </linearGradient>
      </defs>
      {/* carpeta */}
      <path
        d="M18 34h36l8 10h56a8 8 0 0 1 8 8v40a8 8 0 0 1-8 8H18a8 8 0 0 1-8-8V42a8 8 0 0 1 8-8Z"
        fill="var(--bg-3)"
        stroke="var(--border-2)"
        strokeWidth="2"
      />
      <path d="M18 34h36l8 10H18Z" fill="var(--bg-4)" />
      {/* eines que pugen de dins: codi + resplendor */}
      <g stroke="url(#noa-grad)" strokeWidth="3" strokeLinecap="round">
        <path d="M60 62l-9 9 9 9" />
        <path d="M84 62l9 9-9 9" />
        <path d="M76 58l-8 28" />
      </g>
      {/* estrelles suaus */}
      <g fill="url(#noa-grad)" opacity="0.55">
        <circle cx="136" cy="30" r="3.2" />
        <circle cx="122" cy="18" r="2" />
        <circle cx="146" cy="18" r="1.4" />
      </g>
      <text x="80" y="106" textAnchor="middle" fontSize="9" fill="var(--text-3)" fontFamily="var(--font-ui)">
      </text>
    </svg>
  );
}

export function ArtWelcome({ size = 150 }: { size?: number }) {
  return (
    <svg width={size} height={size * 0.8} viewBox="0 0 150 120" fill="none" aria-hidden>
      <defs>
        <linearGradient id="now-grad" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="var(--accent)" />
          <stop offset="100%" stopColor="var(--accent-2, var(--accent))" />
        </linearGradient>
      </defs>
      {/* òrbita */}
      <ellipse cx="75" cy="62" rx="52" ry="26" stroke="var(--border-2)" strokeWidth="1.6" transform="rotate(-16 75 62)" />
      {/* nucli: la IA */}
      <circle cx="75" cy="58" r="17" fill="url(#now-grad)" opacity="0.92" />
      <circle cx="75" cy="58" r="23" stroke="url(#now-grad)" strokeWidth="1.4" opacity="0.5" />
      {/* lluentons en òrbita (els teus projectes) */}
      <circle cx="118" cy="47" r="4.5" fill="var(--accent)" />
      <circle cx="36" cy="72" r="3.4" fill="var(--accent-2, var(--accent))" />
      {/* raig de vida */}
      <g stroke="url(#now-grad)" strokeWidth="2.4" strokeLinecap="round" opacity="0.85">
        <path d="M75 30v-12" />
        <path d="M96 38l7-9" />
        <path d="M54 38l-7-9" />
      </g>
    </svg>
  );
}

export function ArtServer({ size = 120 }: { size?: number }) {
  return (
    <svg width={size} height={size * 0.75} viewBox="0 0 160 120" fill="none" aria-hidden>
      <defs>
        <linearGradient id="nog-grad" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="var(--accent)" />
          <stop offset="100%" stopColor="var(--accent-2, var(--accent))" />
        </linearGradient>
      </defs>
      {/* caixó del servidor */}
      <rect x="26" y="30" width="88" height="26" rx="7" fill="var(--bg-3)" stroke="var(--border-2)" strokeWidth="2" />
      <rect x="26" y="62" width="88" height="26" rx="7" fill="var(--bg-3)" stroke="var(--border-2)" strokeWidth="2" />
      <circle cx="40" cy="43" r="3.4" fill="url(#nog-grad)" />
      <circle cx="40" cy="75" r="3.4" fill="url(#nog-grad)" />
      <rect x="52" y="40" width="44" height="5" rx="2.5" fill="var(--bg-4)" />
      <rect x="52" y="72" width="44" height="5" rx="2.5" fill="var(--bg-4)" />
      {/* lléiga: treball ALLÀ, resultat ací */}
      <g stroke="url(#nog-grad)" strokeWidth="2.6" strokeLinecap="round">
        <path d="M120 50c14-3 24-3 30 6" strokeDasharray="4 5" />
        <path d="M145 48l7 8-10 2" fill="none" />
      </g>
      {/* llamp: GPU */}
      <path d="M138 78l-9 14h6l-3 12 12-17h-7l4-9Z" fill="url(#nog-grad)" opacity="0.9" />
    </svg>
  );
}
