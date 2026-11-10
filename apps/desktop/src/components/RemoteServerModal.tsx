import { X, Server } from "lucide-react";
import { useT } from "../i18n";
import { useUIStore } from "../stores/uiStore";
import RemoteAiServerPanel from "./AI/RemoteAiServerPanel";
import { ArtServer } from "./Illustrations";

/// Finestra modal «Servidor d'IA remot»: puja el servei d'inferència a una
/// màquina amb GPU per SFTP/FTP i connecta-s'hi per HTTP. Embolcall simple
/// del panell que ja existia, ara accessible també amb una sola entrada de
/// menú i de paleta de comandes (IA ▸ Servidor remot).
export default function RemoteServerModal() {
  const { t } = useT();
  const show = useUIStore((s) => s.showRemoteServer);
  const setShow = useUIStore((s) => s.setShowRemoteServer);

  if (!show) return null;

  return (
    <div className="modal-overlay" onClick={(e) => e.target === e.currentTarget && setShow(false)}>
      <div className="modal" style={{ maxWidth: 680 }}>
        <div className="modal-head">
          <span>
            <Server size={15} style={{ verticalAlign: -2, marginRight: 6 }} />
            {t("remoteServer.title")}
          </span>
          <button className="icon-btn" title={t("common.close")} onClick={() => setShow(false)}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body">
          <div className="remote-server-art">
            <ArtServer size={104} />
          </div>
          <RemoteAiServerPanel />
        </div>
      </div>
    </div>
  );
}
