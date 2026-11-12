/// AUTOCOMPLETAT INTEL·LIGENT (provider d'inline completions de Monaco).
///
/// Serveix suggeriments de continuació mentre l'usuari escriu, fent servir
/// la IA ACTIVA de NoOrbit (Ollama local = gratis i ràpid; o el proveïdor
/// remot que tingui actiu). Es pot aturar amb l'interruptor de la pestanya
/// de l'editor (flag a localStorage) i té debounce + timeout perquè el
/// teclejat no se senti mai pesat ni bloquejat.
import { getMonaco, getEditor } from "./editorApi";
import { invoke } from "@tauri-apps/api/core";
import type { editor as E, languages as L } from "monaco-editor";

/// Flag persistit a localStorage: per defecte ACTIVAT (tota via local
/// Ollama és una joia per a això); el botó de l'pestanya el canvia en viu.
const KEY = "noorbit.autocomplete";

export function autocompleteEnabled(): boolean {
  try {
    return localStorage.getItem(KEY) !== "0";
  } catch {
    return true;
  }
}

export function setAutocompleteEnabled(v: boolean) {
  try {
    localStorage.setItem(KEY, v ? "1" : "0");
  } catch {
    /* mode privat: no persisteix, però la sessió ho respecta */
  }
}

/// Delays: debonce del tipeig i vàlua màxima de la resposta del model.
const DEBOUNCE_MS = 350;
const TIMEOUT_MS = 6_000;

interface Timer {
  t: ReturnType<typeof setTimeout> | null;
  gen: number;
  /// Id de la petició d'inferència EN VOL per a aquest model (per cancel·lar-la).
  inflight: string | null;
}

const timers = new Map<string, Timer>();
let requestSeq = 0;

/// Avisa el backend que ja no ens interessa la inferència en vol: al servidor
/// s'atura la generació (deixa de gastar CPU/GPU i RAM). Sense això, la
/// inferència vella continuava fins a expirar el temps límit.
function cancelRequest(requestId: string | null) {
  if (!requestId) return;
  void invoke("ai_inline_complete_cancel", { requestId }).catch(() => undefined);
}

/// Guard de registre: `registerInlineCompletionsProvider` ACUMULA providers
/// (no substitueix). L'editor es remunta (canvi de layout, StrictMode) i,
/// sense aquest guard, cada muntatge afegia 14 providers més: Monaco els
/// consultava TOTS → cada pulsació disparava N peticions d'inferència al
/// backend (2-3 vegades el cost de CPU i xarxa).
let registered = false;

/// Registra el provider als llenguatge principals. S'ha de cridar UNA vegada
/// per editor muntat (des de onMount, on el món de monaco ja és a punt).
export function registerInlineComplete(mon: typeof import("monaco-editor")) {
  if (registered) return;
  registered = true;
  const LANGS = [
    "typescript",
    "typescriptreact",
    "javascript",
    "javascriptreact",
    "python",
    "rust",
    "c",
    "cpp",
    "glsl",
    "css",
    "html",
    "ini",
    "yaml",
  ];
  for (const lang of LANGS) {
    try {
      mon.languages.registerInlineCompletionsProvider([lang], {
        provideInlineCompletions: async (
          model: E.ITextModel,
          position: import("monaco-editor").Position,
          _ctx: L.InlineCompletionContext,
          token: import("monaco-editor").CancellationToken
        ): Promise<L.InlineCompletions> => {
          const key = model.uri.toString();
          // Model tancat: netegem el seu debounce (fuita menor).
          if (model.isDisposed()) {
            timers.delete(key);
            return { items: [] };
          }
          // Debounce: cancel·la la petició anterior del mateix model.
          let timer = timers.get(key);
          if (!timer) {
            timer = { t: null, gen: 0, inflight: null };
            timers.set(key, timer);
          }
          timer.gen += 1;
          const myGen = timer.gen;
          if (timer.t) clearTimeout(timer.t);
          // L'usuari ha seguit escrivint: la inferència anterior (si encara
          // s'està generant al servidor) ja no serveix → s'atura de debò.
          cancelRequest(timer.inflight);
          timer.inflight = null;
          // Espera el debonce (o la cancel·lació de Monaco).
          const slept = new Promise<boolean>((res) => {
            timer!.t = setTimeout(() => res(false), DEBOUNCE_MS);
            token.onCancellationRequested(() => res(true));
          });
          const cancelled = await slept;
          if (cancelled || myGen !== timer.gen || !autocompleteEnabled()) {
            return { items: [] };
          }

          const offset = model.getOffsetAt(position);
          const full = model.getValue();
          // Context sempre petit: els darrers 3000 caràcters ABANS + 600 següents.
          const prefix = full.slice(Math.max(0, offset - 3000), offset);
          const suffix = full.slice(offset, offset + 600);
          if (prefix.trim().length < 3) return { items: [] };

          const langId = model.getLanguageId();
          requestSeq += 1;
          const requestId = "inline-" + requestSeq;
          timer.inflight = requestId;
          const shot = invoke<string>("ai_inline_complete", {
            prefix,
            suffix,
            language: langId,
            requestId,
          });
          // Si Monaco cancel·la el suggeriment (o l'editor es tanca), la
          // inferència s'atura al servidor igualment.
          const onCancel = token.onCancellationRequested(() => cancelRequest(requestId));
          try {
            const text = await Promise.race([
              shot,
              new Promise<string>((_, rej) =>
                setTimeout(() => rej(new Error("timeout")), TIMEOUT_MS)
              ),
            ]);
            const insight = (text ?? "").trim();
            if (!insight || myGen !== timer.gen) return { items: [] };
            return { items: [{ insertText: insight }] };
          } catch {
            // Timeout o error: es cancel·la la petició al backend perquè la
            // generació no continuï consumint CPU/GPU i RAM en segon pla.
            cancelRequest(requestId);
            return { items: [] };
          } finally {
            onCancel.dispose();
            if (timer.inflight === requestId) timer.inflight = null;
          }
        },
        handleItemDidShow() {},
        freeInlineCompletions() {},
      } as L.InlineCompletionsProvider<L.InlineCompletions>);
    } catch {
      /* llenguatge no registrat encara: ho intentariem el proper cop */
    }
  }
}

/// Interruptor de la barra de pestanyes de l'editor (activa/desactiva en viu).
export async function toggleAutocomplete(): Promise<boolean> {
  setAutocompleteEnabled(!autocompleteEnabled());
  // El suggeriment que s'estava mostrant ja no torna a aparèixer.
  const ed = getEditor();
  if (ed) {
    try {
      ed.trigger("autocomplete-toggle", "editor.action.inlineSuggest.hide", null);
    } catch {
      /* inofensiu: el proper tecleat ja tornarà a consultar el flag */
    }
  }
  return autocompleteEnabled();
}
