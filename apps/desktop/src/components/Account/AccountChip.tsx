import { useT } from "../../i18n";
import { useUIStore } from "../../stores/uiStore";
import { useAuthStore, avatarColor, initialOf } from "../../stores/authStore";
import { UserPlus } from "lucide-react";

/// Chip del compte a la barra d'estat (com els IDEs amb compte):
/// - Amb sessió iniciada: cercle acolorit amb la inicial + el correu.
/// - Sense sessió: botó «Registra't / Inicia la sessió», discret.
/// Si es clica, obre la finestra de compte (registre, perfil, IAs…).
export default function AccountChip() {
  const { t } = useT();
  const user = useAuthStore((s) => s.user);
  const openAccount = useUIStore((s) => s.setShowAccount);

  if (!user) {
    return (
      <button className="status-item as-btn" onClick={() => openAccount(true)} title={t("account.title")}>
        <UserPlus size={12} /> {t("account.chipOff")}
      </button>
    );
  }

  return (
    <button
      className="status-item as-btn"
      onClick={() => openAccount(true)}
      title={t("account.title")}
      style={{ display: "inline-flex", alignItems: "center", gap: 6, maxWidth: 220 }}
    >
      <span
        style={{
          width: 15,
          height: 15,
          borderRadius: "50%",
          background: avatarColor(user.id),
          color: "#fff",
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
          fontSize: 9,
          fontWeight: 700,
          flexShrink: 0,
        }}
      >
        {initialOf(user.name)}
      </span>
      <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{user.email}</span>
    </button>
  );
}
