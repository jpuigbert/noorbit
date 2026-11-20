/// Copia text al porta-retalls. Intenta l'API nadi (requereix context segur)
/// i, si falla o no hi és, fa servir el mètode clàssic amb un camp temporal
/// + `execCommand("copy")`, que funciona en el WKWebView de Tauri.
///
/// viu en un fitxer propi (i no dins d'un component) perquè el puguin
/// importar sense arrossegar el plugin de diàlegs a l'arrencada.
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