import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ShieldQuestion } from "lucide-react";
import { useT } from "../../i18n";

type PermissionOption = { optionId: string; name: string; kind: string };
type PermissionAsk = {
  id: string;
  title: string;
  kind: string;
  options: PermissionOption[];
};

/// L'agent de GitHub Copilot (motor ACP) demana permís abans d'usar una
/// eina o d'escriure un fitxer. Aquest diàleg en mostra la petició i deixa
/// l'usuari triar: el permís NO s'aprova mai en silenci — si l'usuari no
/// contesta, el backend rebutja l'operació.
export default function AcpPermissionModal() {
  const { t } = useT();
  const [ask, setAsk] = useState<PermissionAsk | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    const p = listen<PermissionAsk>("agent://acp-permission", (e) => {
      const d = e.payload;
      if (d?.id) setAsk(d);
    })
      .then((u) => {
        unlisten = u;
      })
      .catch(() => {});
    return () => {
      void p.then(() => unlisten?.());
    };
  }, []);

  if (!ask) return null;

  const respond = (optionId: string | null) => {
    const id = ask.id;
    setAsk(null);
    void invoke("acp_permission_reply", { id, optionId }).catch(() => undefined);
  };

  const rebutja = ask.options.filter((o) => o.kind.startsWith("reject"));
  const permet = ask.options.filter((o) => o.kind.startsWith("allow"));

  return (
    <div className="modal-overlay">
      <div className="modal acp-permission">
        <div className="modal-head">
          <span>
            <ShieldQuestion size={15} style={{ verticalAlign: -2, marginRight: 6 }} />
            {t("acp.permissionTitle")}
          </span>
        </div>
        <div className="modal-body">
          <div className="acp-permission-what">
            <code>{ask.title || t("acp.permissionFallback")}</code>
          </div>
          {ask.kind && (
            <p className="empty-hint">{t("acp.permissionKind", { kind: ask.kind })}</p>
          )}
          <p className="empty-hint" style={{ margin: "6px 0 0" }}>
            {t("acp.permissionHint")}
          </p>
        </div>
        <div className="modal-foot">
          {(rebutja.length > 0 ? rebutja : [{ optionId: "", name: t("acp.permissionDeny"), kind: "reject_once" }]).map(
            (o) => (
              <button
                key={o.optionId || "deny"}
                className="btn"
                onClick={() => respond(o.optionId || null)}
              >
                {o.name}
              </button>
            )
          )}
          {permet.map((o) => (
            <button
              key={o.optionId}
              className="btn primary"
              onClick={() => respond(o.optionId)}
            >
              {o.name}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
