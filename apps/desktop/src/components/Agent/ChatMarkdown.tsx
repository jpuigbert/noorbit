import { Fragment, type ReactNode } from "react";
import { Copy } from "lucide-react";
import { useT } from "../../i18n";

/// Renderitzador Markdown lleuger (ínteg del projecte, sense dependències):
/// - Blocs de codi ``` amb CAPÇALERA de llenguatge + botó de còpia
/// - Codi inline `…`, negreta **…**, cursiva *…*, enllaços [t](u) i URLs
/// - Encapçalaments #, llistes «-»/«•», cites, separadors «---»
/// S'usa a les bombolles de la IA (estil xats moderns d'IDE). Treballa amb
/// elements React (cap dangerouslySetInnerHTML: no injecta HTML extern).

const FENCE = /```([a-zA-Z0-9_+\-]*)[ \t]*\n?([\s\S]*?)(```|$)/g;

function InlineMD({ text }: { text: string }): ReactNode {
  // Dividim per tokens sense HTML: `code`, **bold**, *ital*, [t](u), URL nua.
  const parts: ReactNode[] = [];
  const re =
    /(`[^`\n]+`)|(\*\*[^*\n]+\*\*)|(\*[^*\n]+\*)|(\[[^\]\n]+\]\(https?:\/\/[^)\s]+\))|(https?:\/\/[^\s<>()]+)/g;
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

function CodeBlock({ lang, code }: { lang: string; code: string }): ReactNode {
  const { t } = useT();
  return (
    <div className="cmd-code">
      <div className="cmd-code-head">
        <span className="cmd-code-lang">{lang || t("common.code")}</span>
        <button
          className="cmd-code-copy"
          title={t("common.copyCode")}
          onClick={() => void navigator.clipboard.writeText(code)}
        >
          <Copy size={11} /> {t("common.copy")}
        </button>
      </div>
      <pre>
        <code>{code}</code>
      </pre>
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

export function ChatMarkdown({ text }: { text: string }) {
  const nodes: ReactNode[] = [];
  let last = 0;
  let key = 0;
  FENCE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = FENCE.exec(text)) !== null) {
    if (m.index > last) nodes.push(<Prose key={`p${key++}`} text={text.slice(last, m.index)} />);
    nodes.push(<CodeBlock key={`c${key++}`} lang={m[1]} code={m[2].replace(/\n$/, "")} />);
    last = m.index + m[0].length;
    if (m[3] !== "```") break; // bloc sense tancar (streaming): aquest és l'últim
  }
  if (last < text.length) nodes.push(<Prose key={`p-ult`} text={text.slice(last)} />);
  return <div className="chat-md">{nodes}</div>;
}

export default ChatMarkdown;