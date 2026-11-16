import { useRef } from "react";

interface Props {
  /// Eix de l'arrossegament: «x» nansa vertical (canvia l'AMPLADA del panell),
  /// «y» nansa horitzontal (canvia l'ALÇADA).
  axis: "x" | "y";
  /// Direcció de creixement del panell respecte la nansa: +1 si arrossegar
  /// cap a la dreta/avall l'engrandeix (panell a l'esquerra o a dalt),
  /// -1 si l'engrandeix cap a l'esquerra/amunt (panell a la dreta o a sota).
  dir: 1 | -1;
  onResize: (deltaPx: number) => void;
  onReset?: () => void;
  tip?: string;
}

/// Nansa de redimensionat entre panells (com les de VS Code): arrossega per
/// canviar la mida del panell adjacent; doble clic restableix el valor.
/// La captura del punter (setPointerCapture) fa que l'arrossegament segueixi
/// funcionant encara que passi per sobre de l'editor o de la vista prèvia.
export default function ResizeHandle({ axis, dir, onResize, onReset, tip }: Props) {
  const dragging = useRef(false);
  const last = useRef(0);

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    dragging.current = true;
    last.current = axis === "x" ? e.clientX : e.clientY;
    e.currentTarget.setPointerCapture(e.pointerId);
    document.body.classList.add("resizing");
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!dragging.current) return;
    const cur = axis === "x" ? e.clientX : e.clientY;
    const delta = (cur - last.current) * dir;
    last.current = cur;
    if (delta !== 0) onResize(delta);
  };

  const finish = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!dragging.current) return;
    dragging.current = false;
    document.body.classList.remove("resizing");
    try {
      e.currentTarget.releasePointerCapture(e.pointerId);
    } catch {
      /* el punter ja s'ha alliberat */
    }
  };

  return (
    <div
      className={"resize-handle rh-" + axis}
      title={tip}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={finish}
      onPointerCancel={finish}
      onDoubleClick={onReset}
    />
  );
}
