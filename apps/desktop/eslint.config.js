import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

// ESLint (flat config) per a NoOrbit: caça els patrons de hooks de React que
// el grep del CI no detecta — hooks cridats condicionalment (rules-of-hooks) i
// dependències que falten o sobren als useEffect/useMemo (exhaustive-deps),
// la causa habitual dels bucles infinits de render amb selectors de Zustand.
export default tseslint.config(
  { ignores: ["dist/**", "node_modules/**", "src-tauri/**"] },
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: { ecmaFeatures: { jsx: true } },
    },
    plugins: { "react-hooks": reactHooks },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
    },
  }
);