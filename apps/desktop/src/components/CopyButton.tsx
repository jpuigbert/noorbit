import { useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { Copy, Check, Save } from "lucide-react";

/// Copia text al porta-retalls. Intenta l'API nadi (requereix context segur)
/// i, si falla o no hi és, fa servir el mètode clàssic amb un camp temporal
/// + `execCommand("copy")`, que funciona en el WKWebView de Tauri.
export async function copyText(text: string): Promise<boolean> {
  if (!text) return false;
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* cau al mètode alternatiu */
  }
  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.top = "-1000px";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(ta);
    return ok;
  } catch {
    return false;
  }
}

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
  const [copied, setCopied] = useState(false);
  return (
    <button
      className="btn sm ghost chat-copy"
      title={title ?? "Copia al porta-retalls"}
      onClick={async (e) => {
        e.stopPropagation();
        if (await copyText(text)) {
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        }
      }}
    >
      {copied ? <Check size={12} /> : <Copy size={12} />}
      {label ? <span> {copied ? "Copiat" : label}</span> : null}
    </button>
  );
}

interface SaveButtonProps {
  text: string;
  suggestedName?: string;
  label?: string;
}

/// Botó «Desa»: guarda el text a un fitxer triat per l'usuari.
export function SaveButton({ text, suggestedName, label = "Desa" }: SaveButtonProps) {
  const [saved, setSaved] = useState(false);
  return (
    <button
      className="btn sm ghost chat-copy"
      title="Desa el resultat en un fitxer"
      onClick={async (e) => {
        e.stopPropagation();
        const path = await saveTextToFile(text, suggestedName);
        if (path) {
          setSaved(true);
          setTimeout(() => setSaved(false), 1500);
        }
      }}
    >
      <Save size={12} /> <span>{saved ? "Desat" : label}</span>
    </button>
  );
}

export default CopyButton;
