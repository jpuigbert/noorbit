/// Component que mostra els avisos (toasts) del `toastStore`.
/// Es munta UNA vegada a App.tsx.
import { useToastStore } from "../stores/toastStore";

export default function Toaster() {
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);
  if (!toasts.length) return null;
  return (
    <div className="toaster" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`toast toast-${t.level}`} onClick={() => dismiss(t.id)}>
          <span>{t.message}</span>
        </div>
      ))}
    </div>
  );
}
