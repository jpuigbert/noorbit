import { useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { Copy, Check, Save } from "lucide-react";
import { useT } from "../i18n";
import { copyText } from "../utils/clipboard";

/// Reexportat per comoditat: el text es pot copiar des de molts llocs.
export { copyText };

/// Obre el diàleg «Desa com a…» del sistema i escriu el contingut al camí
/// triat (reutilitzant la comanda `write_file` ja registrada a NoOrbit).
export async function saveTextToFile(content: string, suggestedName = "resposta.md"): Promise<string | null> {
  try {
    const path = await save({
      defaultPath: suggestedName,
      filters: [{ name: "Text", extensions: ["md", "txt", "json", "py", "js", "ts", "tsx"] }],
    });
    if (!path) return null;
    await invoke("write_file", { path, content });
    return path;
  } catch {
    return null;
  }
}

interface CopyButtonProps {
  text: string;
  label?: string;
  title?: string;
}

/// Botó petit que copia `text` al porta-retalls i confirma-ho breument.
export function CopyButton({ text, label, title }: CopyButtonProps) {
  const { t } = useT();
  const [copied, setCopied] = useState(false);
  return (
    <button
      className="btn sm ghost chat-copy"
      title={title ?? t("common.copyHint")}
      onClick={async (e) => {
        e.stopPropagation();
        if (await copyText(text)) {
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        }
      }}
    >
      {copied ? <Check size={12} /> : <Copy size={12} />}
      {label ? <span> {copied ? t("common.copied") : label}</span> : null}
    </button>
  );
}

interface SaveButtonProps {
  text: string;
  suggestedName?: string;
  label?: string;
}

/// Botó «Desa»: guarda el text a un fitxer triat per l'usuari.
export function SaveButton({ text, suggestedName, label }: SaveButtonProps) {
  const { t } = useT();
  const [saved, setSaved] = useState(false);
  return (
    <button
      className="btn sm ghost chat-copy"
      title={t("common.saveHint")}
      onClick={async (e) => {
        e.stopPropagation();
        const path = await saveTextToFile(text, suggestedName);
        if (path) {
          setSaved(true);
          setTimeout(() => setSaved(false), 1500);
        }
      }}
    >
      <Save size={12} /> <span>{saved ? t("common.saved") : (label ?? t("common.save"))}</span>
    </button>
  );
}

export default CopyButton;
