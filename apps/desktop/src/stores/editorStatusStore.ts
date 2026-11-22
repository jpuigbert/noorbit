/// Estat de l'editor per a la StatusBar d'IDE (Fase 4): posició del cursor
/// (Ln, Col) i llenguatge del fitxer amb focus. L'alimenta EditorPane amb
/// `onDidChangeCursorPosition` de Monaco i el llenguatge del grup actiu;
/// la StatusBar només el llig (selectors primitius, re-render mínim).
import { create } from "zustand";

interface EditorStatusState {
  line: number;
  column: number;
  /** Llenguatge Monaco del fitxer actiu (null = cap editor amb focus). */
  language: string | null;
  setPosition: (line: number, column: number) => void;
  setLanguage: (lang: string | null) => void;
}

export const useEditorStatusStore = create<EditorStatusState>((set, get) => ({
  line: 1,
  column: 1,
  language: null,
  setPosition: (line, column) => {
    const s = get();
    // Monaco dispara l'esdeveniment sovint (tecleig, selecció): només
    // escrivim a l'store quan la posició realment canvia.
    if (s.line !== line || s.column !== column) set({ line, column });
  },
  setLanguage: (lang) => {
    if (get().language !== lang) set({ language: lang });
  },
}));