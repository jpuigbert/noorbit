/// CAPSA DE PENSAMENT de cada resposta de la IA.
///
/// Vila sempre OBERT mentre es parla (l'usuari vol veure el raonament sense
/// plegar res), i en CATALÀ quan el model pensa en altra llengua: la
/// traducció és automàtica amb una crida crua, i queda cachejada per text
/// no repetir crides. Amb el botó «Original» se segueix tenint cru.
import { useEffect, useState } from "react";
import { Brain } from "lucide-react";
import { useT } from "../../i18n";
import { looksCatalan, cachedCat, translateToCatalan } from "./reasoningCat";

export default function ChatThought({ thinking, elapsedMs }: { thinking: string; elapsedMs?: number | null }) {
  const { t } = useT();
  const [cat, setCat] = useState<string | null>(() => cachedCat(thinking));
  const [showRaw, setShowRaw] = useState(false);
  const jaEsCatala = looksCatalan(thinking);

  useEffect(() => {
    if (jaEsCatala || cat) { return; }
    setCat(cachedCat(thinking)); // potser una altra bombolla ja la va Traducció
    if (cachedCat(thinking)) { return; }
    let viu = true;
    void translateToCatalan(thinking).then((v) => { if (viu && v) { setCat(v); } });
    return () => { viu = false; };
  }, [thinking, jaEsCatala, cat]);

  const text = !jaEsCatala && cat && !showRaw ? cat : thinking;
  const subtitle =
    (elapsedMs != null ? `${(elapsedMs / 1000).toFixed(1)} s · ` : "") +
    (jaEsCatala ? t("agent.thCatalan") : cat ? t("agent.thTranslated") : t("agent.thTranslating"));

  return (
    <details className="chat-thought" open>
      <summary onClick={(e) => e.preventDefault()}>
        <Brain size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
        {t("agent.thinkingTitle")}
        <span className="chat-thought-sub"> {subtitle}</span>
        {!jaEsCatala && (
          <button
            className="chat-thought-btn"
            title={showRaw ? t("agent.thShowCat") : t("agent.thShowRaw")}
            onClick={(e) => { e.stopPropagation(); setShowRaw(!showRaw); }}
          >
            {showRaw ? "CA" : t("agent.thRaw") }
          </button>
        )}
      </summary>
      <pre className="agent-thinking-body">{text}</pre>
    </details>
  );
}
