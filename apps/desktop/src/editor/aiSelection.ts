/// EDICIÓ DE CODI AMB LA IA SÍ SOBRE LA SELECCIÓ.
///
/// L'usuari selecciona un fragment al seu editor, escriu WHAT se'n vol fer
/// («corregeix», «refactoritza amb tipus», «tradueix a Python»…), i la IA
/// retorna el fragment NOVET al seu lloc. El resultat entra pel mateix flux
/// de revisió que totes les edicions de la IA: la ReviewBar de muntja
/// MOSTRA el diff (verd/vermell) i es pot acceptar o rebutjar — res no es
/// fica davant sense que l'usuari ho vegi.
import { invoke } from "@tauri-apps/api/core";

/// Demana a la IA la versió nova del fragment seleccionat. Torna null si el
/// model no respon o no té res (en aquest cas ho deixem avisat a la UI).
export async function aiReplaceSelection(
  fileName: string,
  lang: string,
  code: string,
  instruction: string
): Promise<string | null> {
  const prompt =
    `LLENGUATGE: ${lang}\nARXIU: ${fileName}\n\n` +
    `INSTRUCCIÓ de l'usuari sobre el fragment: ${instruction}\n\n` +
    `FRAGMENT SELECCIONAT (codi exacte a substituir):\n${code}\n\n` +
    `Retorna NOMÉS la versió nova d'aquest fragment, sense les mateixes\n` +
    `sanglines que el seu context, sense backticks ni explicacions.`;
  try {
    const raw = await invoke<string>("send_prompt", {
      prompt,
      system:
        "Ets un editor de codi precís. Responds només amb el fragment reescrit, en el mateix llenguatge, sense backticks ni explicacions.",
      // La instrucció de l'usuari és el que s'APREN (aprenentatge local).
      question: instruction,
    });
    let out = (raw ?? "").trim();
    const fence = /```[a-zA-Z]*\n?([\s\S]*?)```/.exec(out);
    if (fence) out = fence[1].trim();
    return out.length > 0 ? out : null;
  } catch {
    return null;
  }
}
