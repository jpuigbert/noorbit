/// Ressaltat de sintaxi lleuger per als blocs de codi del xat, sense
/// dependències: un regex per llenguatge (comentaris de bloc > comentaris
/// de línia > cadenes > números > paraules clau) i estat de comentari de
/// bloc entre línies. Treballa amb elements React purs (cap HTML injectat).
///
/// També dedueix el llenguatge quan falta: per l'extensió del fitxer
/// (targetes «@file:») o per l'aparença del codi (blocs ``` sense etiqueta).

import type { ReactNode } from "react";

export interface LangDef {
  label: string;
  /// Marques de comentari de línia («//», «#»…).
  line: string[];
  /// Parells de comentari de bloc obertura/tancament («/*»…, «<!--»…).
  block: [string, string][];
  /// Cometes de cadena («"», «'», «`»).
  strings: string[];
  /// Paraules clau ressaltades (lletres i símbols de paraula).
  keywords?: string[];
}

const KW = (s: string) => s.split(" ");
const KW_JS = KW(
  "const let var function return if else for while do switch case break continue new class extends import export from default try catch finally throw async await typeof instanceof in of null undefined true false this super static get set yield delete void"
);
const KW_TS = KW_JS.concat(
  KW("interface type enum namespace declare readonly implements public private protected abstract as satisfies keyof infer never unknown any")
);

const LANGS: Record<string, LangDef> = {
  js: { label: "JavaScript", line: ["//"], block: [["/*", "*/"]], strings: ['"', "'", "`"], keywords: KW_JS },
  jsx: { label: "JSX", line: ["//"], block: [["/*", "*/"]], strings: ['"', "'", "`"], keywords: KW_JS },
  ts: { label: "TypeScript", line: ["//"], block: [["/*", "*/"]], strings: ['"', "'", "`"], keywords: KW_TS },
  tsx: { label: "TSX", line: ["//"], block: [["/*", "*/"]], strings: ['"', "'", "`"], keywords: KW_TS },
  py: {
    label: "Python",
    line: ["#"],
    block: [["'''", "'''"], ['"""', '"""']],
    strings: ['"', "'"],
    keywords: KW(
      "def return if elif else for while in not and or is None True False import from as class pass break continue try except finally with lambda raise global nonlocal yield async await del assert"
    ),
  },
  rs: {
    label: "Rust",
    line: ["//"],
    block: [["/*", "*/"]],
    strings: ['"'],
    keywords: KW(
      "fn let mut const struct enum impl trait match if else for while loop return use mod pub crate self super ref true false move in async await where type break continue Some None Ok Err"
    ),
  },
  go: {
    label: "Go",
    line: ["//"],
    block: [["/*", "*/"]],
    strings: ['"', "`"],
    keywords: KW(
      "func package import return if else for range var const type struct interface map chan go defer select switch case break continue fallthrough nil true false"
    ),
  },
  java: {
    label: "Java",
    line: ["//"],
    block: [["/*", "*/"]],
    strings: ['"'],
    keywords: KW(
      "public private protected static final void class interface extends implements new return if else for while switch case break continue try catch finally throw throws import package this super null true false abstract enum"
    ),
  },
  c: {
    label: "C",
    line: ["//"],
    block: [["/*", "*/"]],
    strings: ['"'],
    keywords: KW(
      "int char float double long short unsigned signed void struct union enum typedef const static extern return if else for while do switch case break continue sizeof"
    ),
  },
  cpp: {
    label: "C++",
    line: ["//"],
    block: [["/*", "*/"]],
    strings: ['"'],
    keywords: KW(
      "int char float double long short unsigned signed void struct union enum class namespace template typename public private protected virtual constexpr const static extern return if else for while do switch case break continue sizeof new delete true false nullptr"
    ),
  },
  cs: {
    label: "C#",
    line: ["//"],
    block: [["/*", "*/"]],
    strings: ['"'],
    keywords: KW(
      "public private protected internal static readonly const void class interface struct enum namespace using new return if else for foreach while switch case break continue try catch finally throw var async await this base null true false"
    ),
  },
  php: {
    label: "PHP",
    line: ["//", "#"],
    block: [["/*", "*/"]],
    strings: ['"', "'"],
    keywords: KW(
      "function return if else elseif for foreach while switch case break continue echo print class new public private protected static extends implements try catch finally throw require include use namespace null true false const fn"
    ),
  },
  css: { label: "CSS", line: [], block: [["/*", "*/"]], strings: ['"', "'"], keywords: [] },
  scss: { label: "SCSS", line: ["//"], block: [["/*", "*/"]], strings: ['"', "'"], keywords: [] },
  html: { label: "HTML", line: [], block: [["<!--", "-->"]], strings: ['"', "'"], keywords: [] },
  sh: {
    label: "Shell",
    line: ["#"],
    block: [],
    strings: ['"', "'"],
    keywords: KW("if then else elif fi for while do done case esac function echo export source return exit local set unset readonly in")
  },
  sql: {
    label: "SQL",
    line: ["--"],
    block: [["/*", "*/"]],
    strings: ["'"],
    keywords: KW(
      "select from where insert into values update set delete create table drop alter join left right inner outer full on group by order having limit and or not null as distinct union all primary key foreign references index view exists between like case when then else end"
    ),
  },
  yaml: {
    label: "YAML",
    line: ["#"],
    block: [],
    strings: ['"', "'"],
    keywords: KW("true false null yes no on off"),
  },
  toml: { label: "TOML", line: ["#"], block: [], strings: ['"', "'"], keywords: KW("true false") },
  json: { label: "JSON", line: [], block: [], strings: ['"'], keywords: KW("true false null") },
  xml: { label: "XML", line: [], block: [["<!--", "-->"]], strings: ['"', "'"], keywords: [] },
  md: { label: "Markdown", line: [], block: [], strings: ["`"], keywords: [] },
  dockerfile: {
    label: "Dockerfile",
    line: ["#"],
    block: [],
    strings: ['"', "'"],
    keywords: KW("FROM RUN CMD ENTRYPOINT COPY ADD WORKDIR ENV ARG EXPOSE VOLUME USER LABEL ONBUILD STOPSIGNAL HEALTHCHECK SHELL MAINTAINER"),
  },
  diff: { label: "Diff", line: [], block: [], strings: [], keywords: [] },
  graphql: {
    label: "GraphQL",
    line: ["#"],
    block: [],
    strings: ['"'],
    keywords: KW("query mutation subscription fragment on type input schema scalar enum interface union directive extend null true false"),
  },
};

const ALIAS: Record<string, string> = {
  javascript: "js", node: "js", mjs: "js", cjs: "js",
  typescript: "ts",
  python: "py", py3: "py",
  bash: "sh", shell: "sh", zsh: "sh", console: "sh", cmd: "sh",
  "c++": "cpp", cc: "cpp", cxx: "cpp", "c#": "cs", csharp: "cs",
  golang: "go",
  markdown: "md", text: "txt",
  yml: "yaml",
  less: "css", styl: "css",
  docker: "dockerfile", make: "makefile",
  gql: "graphql",
  html5: "html", vue: "html", svelte: "html", twig: "html", blade: "html", phtml: "html",
};

const reCache = new Map<string, RegExp>();

export function getLang(key: string): LangDef | null {
  const k = key.toLowerCase().replace(/^\./, "").trim();
  const real = ALIAS[k] ?? k;
  return LANGS[real] ?? null;
}

/// Etiqueta llegible per a la capçalera del bloc («TS», «Python»…).
export function langLabel(key: string): string | null {
  return getLang(key)?.label ?? null;
}

const EXT_LANG: Record<string, string> = {
  js: "js", mjs: "js", cjs: "js", jsx: "jsx", ts: "ts", tsx: "tsx",
  py: "py", pyw: "py", json: "json", html: "html", htm: "html",
  css: "css", scss: "scss", less: "css", sh: "sh", bash: "sh", zsh: "sh",
  sql: "sql", md: "md", markdown: "md", rs: "rs", go: "go", java: "java",
  c: "c", h: "c", cpp: "cpp", hpp: "cpp", cc: "cpp", cxx: "cpp", cs: "cs",
  php: "php", yml: "yaml", yaml: "yaml", toml: "toml", xml: "xml",
  svg: "xml", dockerfile: "dockerfile", makefile: "makefile", diff: "diff",
  patch: "diff", vue: "html", svelte: "html", twig: "html", blade: "html",
  phtml: "html", graphql: "graphql", gql: "graphql",
};

/// Llenguatge pel nom del fitxer (targetes «@file:» sense tanca ```).
export function langFromPath(path: string): string | null {
  const base = path.split(/[/\\]/).pop() ?? "";
  if (/^dockerfile$/i.test(base)) return "dockerfile";
  if (/^makefile$/i.test(base)) return "makefile";
  const ext = base.split(".").pop()?.toLowerCase() ?? "";
  return EXT_LANG[ext] ?? null;
}

/// Llenguatge per l'aparença del codi (blocs ``` sense etiqueta).
export function sniffLang(code: string): string | null {
  const c = code.replace(/^\s+/, "");
  if (/^<\?php/i.test(c)) return "php";
  if (/^<!--/.test(c) || /^<!doctype html>/i.test(c) || /^<html[\s>]/i.test(c) || /^<[a-z][^>]*>/i.test(c))
    return "html";
  if (/^#!/.test(c)) return "sh";
  if (/^(import |from |def |class |@)/.test(c) || /^if __name__/.test(c)) return "py";
  if (/^(fn |impl |use |pub |mod |#\[)/.test(c)) return "rs";
  if (/^(package |func |import \()/.test(c)) return "go";
  if (/^(const |let |var |function |import |export |interface |type |async )/.test(c) || /^(['"]use strict)/.test(c))
    return "js";
  if (/^\{/.test(c) || /^\[/.test(c)) return "json";
  if (/^(select |insert |update |delete |create |with |alter )/i.test(c)) return "sql";
  if (/^FROM\b/im.test(code.slice(0, 40))) return "dockerfile";
  return null;
}

const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

function buildRe(def: LangDef): RegExp {
  const parts: string[] = [];
  for (const [o] of def.block) parts.push(esc(o));
  for (const c of def.line) parts.push(`${esc(c)}[^\n]*`);
  for (const q of def.strings) parts.push(`${esc(q)}(?:\\\\.|[^${esc(q)}\\\\])*${esc(q)}`);
  parts.push("\\b\\d[\\d_]*(?:\\.\\d+)?\\b");
  if (def.keywords && def.keywords.length) parts.push(`\\b(?:${def.keywords.join("|")})\\b`);
  return new RegExp(parts.join("|"), "g");
}

function getRe(langKey: string): RegExp {
  let re = reCache.get(langKey);
  if (!re) {
    const def = getLang(langKey)!;
    re = buildRe(def);
    reCache.set(langKey, re);
  }
  return re;
}

interface HlState {
  block: string | null;
}

function highlightLine(line: string, re: RegExp, def: LangDef, st: HlState): ReactNode[] {
  const out: ReactNode[] = [];
  const closes: Record<string, string> = {};
  for (const [o, c] of def.block) closes[o] = c;
  let rest = line;
  let key = 0;
  while (rest.length > 0) {
    // Dins d'un comentari de bloc: tot és comentari fins al tancament.
    if (st.block) {
      const idx = rest.indexOf(st.block);
      if (idx < 0) {
        out.push(<span key={key++} className="tok-c">{rest}</span>);
        return out;
      }
      const end = idx + st.block.length;
      out.push(<span key={key++} className="tok-c">{rest.slice(0, end)}</span>);
      rest = rest.slice(end);
      st.block = null;
      continue;
    }
    re.lastIndex = 0;
    const m = re.exec(rest);
    if (!m) {
      out.push(rest);
      return out;
    }
    if (m.index > 0) out.push(rest.slice(0, m.index));
    const tok = m[0];
    let cls = "tok-k";
    if (closes[tok] !== undefined) {
      st.block = closes[tok];
      cls = "tok-c";
    } else if (def.line.some((c) => tok.startsWith(c))) cls = "tok-c";
    else if (def.strings.some((q) => tok.startsWith(q))) cls = "tok-s";
    else if (/^\d/.test(tok)) cls = "tok-n";
    out.push(<span key={key++} className={cls}>{tok}</span>);
    rest = rest.slice(m.index + tok.length);
  }
  return out;
}

/// Ressalta línia a línia (estat de comentari de bloc compartit entre
/// línies) i torna una llista de nodes per línia. Sense llenguatge
/// reconegut, cada línia és text pla (cap cost).
export function highlightLines(lines: string[], langKey: string): ReactNode[][] {
  const def = getLang(langKey);
  if (!def) return lines.map((l) => [l]);
  const re = getRe(langKey);
  const st: HlState = { block: null };
  return lines.map((l) => highlightLine(l, re, def, st));
}
