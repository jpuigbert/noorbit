import { useEffect, useState } from "react";
import { Globe, MessageSquare, LogIn, Search, Code2 } from "lucide-react";
import { useT } from "../../i18n";
import { useWebAiStore } from "../../stores/webAiStore";
import { useAgentStore } from "../../stores/agentStore";
import { usePreviewStore } from "../../stores/previewStore";

/// Panell «IAs de la web» — PUNT D'ENTRADA dels xats amb IAs web.
///
/// AMB ELS XATS UNIFICATS aquest panell NO té la seua pròpia conversa: el
/// botó «Xatat-hi» obre un xat DELS DE SEMPRE (pestanya com les altres, amb
/// historial, forks i delegacions) però amb proveïdor «web:<ia>», així que la
/// pregunta viatja directes al navegador. La conversa web viu al lloc de
/// totes: el xat de l'agent.
export default function WebAiPanel() {
  const { t } = useT();
  const ias = useWebAiStore((s) => s.ias);
  const init = useWebAiStore((s) => s.init);
  const openWebChat = useWebAiStore((s) => s.openWebChat);
  const newChat = useAgentStore((s) => s.newChat);
  const setActive = useAgentStore((s) => s.setActive);
  const setRightTab = usePreviewStore((s) => s.setRightTab);
  const [starting, setStarting] = useState<string | null>(null);

  useEffect(() => {
    void init();
  }, [init]);

  const startChatWith = (id: string) => {
    const ia = ias.find((x) => x.id === id);
    if (!ia || starting) return;
    setStarting(id);
    // Un xat «normal» amb proveïdor web: tot el pipeline compartin és el mateix
    // (historial persistit, passos, @file d'autoMaterialize, llista de tasques).
    const sid = newChat({ provider: `web:${ia.id}`, title: ia.name, model: null, expertId: "" });
    setActive(sid);
    setRightTab("agent");
    // La finestra del navegador ja queda oberta a la IA (visible): si cal,
    // l'usuari iniciarà sessió amb el seu compte mentre s'hi và passant.
    // S'ESPERA l'obertura abans de deixar el flag: abans es feia alhora
    // (React agrupava els dos estats i el botó mai no quedava «bloquejat»:
    // un doble clic obria dues finestres del navegador).
    void openWebChat(ia.id)
      .catch(() => undefined)
      .finally(() => setStarting(null));
  };

  return (
    <div className="webai-launch">
      <div style={{ padding: "10px 12px 6px", color: "var(--text-2)", fontSize: 12 }}>
        {t("webAi.launchIntro")}
      </div>
      <div className="webai-grid">
        {ias.map((ia) => (
          <div key={ia.id} className="webai-card">
            <div className="webai-card-head">
              <Globe size={14} style={{ color: "var(--accent)" }} />
              <strong>{ia.name}</strong>
              <span className="webai-card-url">{ia.url.replace(/^https?:\/\//, "").replace(/\/$/, "")}</span>
            </div>
            <div className="webai-card-actions">
              <button
                className={"btn sm primary"}
                onClick={() => startChatWith(ia.id)}
                disabled={starting !== null}
              >
                <MessageSquare size={12} /> {t("webAi.startChat")}
              </button>
              {ia.id !== "dsh" && (
                <button
                  className="btn sm"
                  title={t("webAi.open")}
                  onClick={() => void openWebChat(ia.id)}
                >
                  <LogIn size={11} /> {t("webAi.loginFirst")}
                </button>
              )}
            </div>
          </div>
        ))}
        {ias.length === 0 && (
          <div className="empty-hint" style={{ padding: 20 }}>
            {t("webAi.loading")}
          </div>
        )}
      </div>
      <div style={{ padding: "6px 12px 12px", fontSize: 11, color: "var(--text-3)", display: "flex", gap: 6, alignItems: "flex-start" }}>
        <Search size={12} style={{ flexShrink: 0, marginTop: 1 }} />
        <span>{t("webAi.searchNote")}</span>
      </div>
      <div style={{ padding: "0 12px 12px", fontSize: 11, color: "var(--text-3)", display: "flex", gap: 6, alignItems: "flex-start" }}>
        <Code2 size={12} style={{ flexShrink: 0, marginTop: 1 }} />
        <span>{t("webAi.codeNote")}</span>
      </div>
    </div>
  );
}
