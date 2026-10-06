// Component «Actualització» per a la BARRA D'ESTAT. Uns segons després de
// l'arrancada pregunta a GitHub si existeix una versió nova (comanda
// «check_update»). Si NO n'hi ha, no mostra res: mai no ocupa ni molesta.
// Si n'hi ha, apareix un avís amb la versió nova; fer clic obri la
// pàgina del release amb el navegador per a descarregar l'instal·ador.
// La mateixa informació, amb botó de comprovació MANUAL, viu a Preferències
// › Informació.
import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Download } from "lucide-react";
import { useUpdateStore } from "../../stores/updateStore";

export default function UpdateBadge() {
  const info = useUpdateStore((s) => s.info);
  const check = useUpdateStore((s) => s.check);

  useEffect(() => {
    // Deixem 4 s: la xarxa no ha de frenar l'arrancada de la finestra.
    const id = setTimeout(() => void check(), 4000);
    return () => clearTimeout(id);
  }, [check]);

  if (!info?.available) return null;
  return (
    <button
      className="status-update"
      title={
        "Hi ha una versió nova de NoOrbit: " +
        info.latest +
        ". Fes clic per a obrir la pàgina del release i descarregar-la."
      }
      onClick={() =>
        void invoke("open_externally", { path: info.url }).catch(() => undefined)
      }
    >
      <Download size={12} /> Actualització {info.latest}
    </button>
  );
}
