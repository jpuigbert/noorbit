import { Fragment, useState, type ReactNode } from "react";
import { Check, ChevronDown, ChevronUp, Copy, FileCode, FilePlus2, FolderPlus, GitBranch, Trash2 } from "lucide-react";
import { useT } from "../../i18n";
import { copyText } from "../CopyButton";
import { tokenizeChat } from "./chatSegments";
import { highlightLines, langFromPath, langLabel, sniffLang } from "./codeHighlight";

/// Renderitzador Markdown lleuger (ínteg del projecte, sense dependències):
/// - Targetes «@file: ruta» amb el codi dins (capçalera amb ruta, còpia i
///   aplicació directa al projecte)
/// - Blocs de codi ``` amb capçalera de llenguatge, números de línia,
///   ressaltat de sintaxi i còpia amb confirmació
/// - Directrius «@dir:»/«@delete:» com a xips informatius
/// - Codi inline `…`, negreta **…**, cursiva *…*, enllaços [t](u) i URLs
/// - Encapçalaments #, llistes «-»/«•», cites, separadors «---»
/// S'usa a les bombolles de la IA (estil xats moderns d'IDE). Treballa amb
/// elements React, mai amb injecció d'HTML: el codi del model és text pla.

/// Línies que es mostren quan l'usuari PLEGA un bloc a mà. Per defecte els
/// blocs es pinten SENCERS: la informació que dona la IA no s'ha de contraure
/// ni amagar darrere d'un «desplega» — es llegeix tota d'una tirada.
const FOLD_AT = 18;

/// Callback opcional per escriure UN fitxer del xat al projecte obert.
export type FileApply = (path: string, content: string) => Promise<unknown> | void;

function InlineMD({ text }: { text: string }): ReactNode {
  // Dividim per tokens sense HTML: `code`, **bold**, *ital*, [t](u), URL nua.
  // (Cada crida rep UNA línia: les classes de caràcters no cal que excloguin
  // els salts de línia.)
  const parts: ReactNode[] = [];
  const re =
    /(`[^`]+`)|(\*\*[^*]+\*\*)|(\*[^*]+\*)|(\[[^\]]+\]\(https?:\/\/[^)\s]+\))|(https?:\/\/[^\s<>()]+)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let key = 0;
  while ((m = re.exec(text)) !== null) {
    if (m.index > last) parts.push(text.slice(last, m.index));
    const tok = m[0];
    if (tok.startsWith("`")) {
      parts.push(<code key={key++}>{tok.slice(1, -1)}</code>);
    } else if (tok.startsWith("**")) {
      parts.push(<strong key={key++}>{tok.slice(2, -2)}</strong>);
    } else if (tok.startsWith("*") && !tok.startsWith("**")) {
      parts.push(<em key={key++}>{tok.slice(1, -1)}</em>);
    } else if (tok.startsWith("[")) {
      const mm = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(tok);
      if (mm)
        parts.push(
          <a key={key++} href={mm[2]} target="_blank" rel="noreferrer">
            {mm[1]}
          </a>
        );
      else parts.push(tok);
    } else {
      parts.push(
        <a key={key++} href={tok} target="_blank" rel="noreferrer">
          {tok}
        </a>
      );
    }
    last = m.index + tok.length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return <>{parts}</>;
}

/// Cos del codi: ressaltat per línia, números al marge i plec OPCIONAL.
/// Neix DESPLEGAT (expanded = true): el codi sencer de la IA és visible sense
/// cap clic; qui vulgui compactar-lo té el botó «plega».
function CodeLines({ code, lang, live }: { code: string; lang: string; live?: boolean }) {
  const { t } = useT();
  const [expanded, setExpanded] = useState(true);
  const all = code.replace(/\n$/, "").split("\n");
  const folded = !expanded && all.length > FOLD_AT;
  // En streaming interessa veure el FINAL (el que s'està escrivint); en un
  // missatge tancat, el principi. El número de línia sempre és el real.
  const lines = folded ? (live ? all.slice(-FOLD_AT) : all.slice(0, FOLD_AT)) : all;
  const first = folded && live ? all.length - FOLD_AT + 1 : 1;
  const hl = highlightLines(lines, lang.toLowerCase());
  return (
    <>
      <pre>
        <code>
          {hl.map((row, i) => (
            <span className="cmd-line" key={i}>
              {all.length > 1 && <span className="cmd-ln">{first + i}</span>}
              <span className="cmd-line-body">{row.length ? row : "\u00a0"}</span>
            </span>
          ))}
        </code>
      </pre>
      {folded && (
        <button className="cmd-fold" onClick={() => setExpanded(true)}>
          <ChevronDown size={12} /> {t("agent.expandCode", { n: all.length - FOLD_AT })}
        </button>
      )}
      {expanded && all.length > FOLD_AT && (
        <button className="cmd-fold" onClick={() => setExpanded(false)}>
          <ChevronUp size={12} /> {t("agent.collapseCode")}
        </button>
      )}
    </>
  );
}

/// Botó de còpia amb confirmació breu (reutilitza el copyText de NoOrbit,
/// que sap caure al mètode clàssic dins del WKWebView de Tauri).
function CopyCode({ code }: { code: string }) {
  const { t } = useT();
  const [copied, setCopied] = useState(false);
  return (
    <button
      className="cmd-code-copy"
      title={t("common.copyCode")}
      onClick={async () => {
        if (await copyText(code)) {
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        }
      }}
    >
      {copied ? <Check size={11} /> : <Copy size={11} />} {copied ? t("common.copied") : t("common.copy")}
    </button>
  );
}

/// Bloc de codi amb tanca (```): capçalera de llenguatge + còpia + cos.
function CodeBlock({ lang, code, live }: { lang: string; code: string; live?: boolean }) {
  const { t } = useT();
  const key = lang.toLowerCase();
  const label = langLabel(key) || langLabel(sniffLang(code) ?? "") || t("common.code");
  return (
    <div className="cmd-code">
      <div className="cmd-code-head">
        <span className="cmd-code-lang">{label}</span>
        <span className="cmd-code-actions">
          <CopyCode code={code} />
        </span>
      </div>
      <CodeLines code={code} lang={lang} live={live} />
    </div>
  );
}

/// Targeta d'un fitxer «@file: ruta»: ruta a la capçalera (carpeta atenuada,
/// nom destacat), còpia i aplicació directa al projecte si el panell ho
/// permet. El llenguatge es dedueix de l'extensió o de l'aparença del codi.
function FileCard({
  path,
  code,
  live,
  onApplyFile,
}: {
  path: string;
  code: string;
  live?: boolean;
  onApplyFile?: FileApply;
}) {
  const { t } = useT();
  const [applyState, setApplyState] = useState<"idle" | "busy" | "done">("idle");
  const segs = path.replace(/\\/g, "/").split("/");
  const base = segs.pop() ?? path;
  const folder = segs.join("/");
  const langKey = langFromPath(path) ?? sniffLang(code) ?? "";
  const label = langLabel(langKey);

  const apply = async () => {
    if (!onApplyFile || applyState !== "idle") return;
    setApplyState("busy");
    let ok = false;
    try {
      ok = Boolean(await onApplyFile(path, code));
    } catch {
      ok = false;
    }
    setApplyState(ok ? "done" : "idle");
    if (ok) setTimeout(() => setApplyState("idle"), 2000);
  };

  return (
    <div className="cmd-code cmd-file">
      <div className="cmd-code-head">
        <span className="cmd-file-path" title={path}>
          <FileCode size={12} />
          {folder && <span className="cmd-file-dir">{folder}/</span>}
          <span className="cmd-file-name">{base}</span>
          {label && <span className="cmd-file-lang">{label}</span>}
        </span>
        <span className="cmd-code-actions">
          <CopyCode code={code} />
          {onApplyFile && (
            <button
              className={"cmd-code-copy cmd-apply" + (applyState === "done" ? " ok" : "")}
              title={t("agent.applyFileHint")}
              disabled={applyState === "busy"}
              onClick={() => void apply()}
            >
              {applyState === "done" ? (
                <Check size={11} />
              ) : (
                <FilePlus2 size={11} className={applyState === "busy" ? "spin" : ""} />
              )}{" "}
              {applyState === "done" ? t("agent.applyFileDone") : t("agent.applyFile")}
            </button>
          )}
        </span>
      </div>
      <CodeLines code={code} lang={langKey} live={live} />
    </div>
  );
}

/// Xip d'una directriu d'estructura: «@dir:» crea una carpeta, «@delete:»
/// esborra una ruta. Informatius: l'escriptura real la fa el materialitzador.
function DirectiveChip({ kind, path }: { kind: "dir" | "del"; path: string }) {
  const { t } = useT();
  const isDir = kind === "dir";
  return (
    <div
      className={"cmd-dir" + (isDir ? "" : " del")}
      title={isDir ? t("agent.dirNewHint") : t("agent.delPathHint")}
    >
      {isDir ? <FolderPlus size={12} /> : <Trash2 size={12} />}
      <span className="cmd-dir-path">{path}</span>
      <span className="cmd-dir-tag">{isDir ? t("agent.dirNew") : t("agent.delPath")}</span>
    </div>
  );
}

/// Xip d'una directriu de BRANCA (Git): «@branch: create feature/x».
/// Informatiu: l'operació real la fa el materialitzador. L'acció ja ve
/// NORMALITZADA de `tokenizeChat` («crea» → «create»): un xip mai no pot
/// mostrar-se com a esborrat quan el que es fa és crear.
function BranchChip({ action, name }: { action: string; name: string }) {
  const { t } = useT();
  const label =
    action === "create"
      ? t("agent.branchCreate")
      : action === "checkout"
        ? t("agent.branchCheckout")
        : t("agent.branchDelete");
  return (
    <div className="cmd-dir branch" title={t("agent.branchHint")}>
      <GitBranch size={12} />
      <span className="cmd-dir-path">{name}</span>
      <span className="cmd-dir-tag">{label}</span>
    </div>
  );
}

/// Converteix un segment SENSE blocs de codi en pàgrafs (+ llistes/cites).
function Prose({ text }: { text: string }): ReactNode {
  const lines = text.split(/\r?\n/);
  const out: ReactNode[] = [];
  let list: string[] = [];
  let listOrdered = false;
  const flush = (key: string | number) => {
    if (list.length === 0) return;
    const Li = listOrdered ? "ol" : "ul";
    out.push(
      <Li key={key}>
        {list.map((li, k) => (
          <li key={k}>
            <InlineMD text={li} />
          </li>
        ))}
      </Li>
    );
    list = [];
  };
  lines.forEach((raw, i) => {
    const line = raw.replace(/\s+$/, "");
    // Llistes: «- », «* », «• » o «1. »
    const li = /^\s*[-*•]\s+(.*)$/.exec(line);
    const ol = /^\s*(\d+)[.)]\s+(.*)$/.exec(line);
    if (li) {
      if (listOrdered) flush(`f${i}`);
      listOrdered = false;
      list.push(li[1]);
      return;
    }
    if (ol) {
      if (!listOrdered) flush(`f${i}`);
      listOrdered = true;
      list.push(ol[2]);
      return;
    }
    flush(`f${i}`);
    if (!line.trim()) return;
    // Encapçalaments i cites
    const h = /^(#{1,4})\s+(.*)$/.exec(line);
    if (h) {
      const Lvl = `h${Math.min(h[1].length + 3, 6)}` as "h4" | "h5" | "h6";
      out.push(<Lvl key={i}>{<InlineMD text={h[2]} />}</Lvl>);
      return;
    }
    if (/^>\s?/.test(line)) {
      out.push(
        <blockquote key={i}>
          <InlineMD text={line.replace(/^>\s?/, "")} />
        </blockquote>
      );
      return;
    }
    if (/^\s*(---|===|___)\s*$/.test(line)) {
      out.push(<hr key={i} />);
      return;
    }
    out.push(
      <p key={i}>
        <InlineMD text={line} />
      </p>
    );
  });
  flush("f-final");
  return <Fragment>{out}</Fragment>;
}

export function ChatMarkdown({
  text,
  onApplyFile,
  live,
}: {
  text: string;
  onApplyFile?: FileApply;
  /// True a la bombolla en streaming: els blocs llargs mostren el final.
  live?: boolean;
}) {
  const segs = tokenizeChat(text);
  return (
    <div className="chat-md">
      {segs.map((s, i) => {
        switch (s.kind) {
          case "prose":
            return <Prose key={i} text={s.text} />;
          case "code":
            return <CodeBlock key={i} lang={s.lang} code={s.code} live={live} />;
          case "file":
            return <FileCard key={i} path={s.path} code={s.code} live={live} onApplyFile={onApplyFile} />;
          case "dir":
            return <DirectiveChip key={i} kind="dir" path={s.path} />;
          case "del":
            return <DirectiveChip key={i} kind="del" path={s.path} />;
          case "branch":
            return <BranchChip key={i} action={s.action} name={s.name} />;
        }
      })}
    </div>
  );
}

export default ChatMarkdown;
