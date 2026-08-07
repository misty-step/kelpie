import { useMemo } from "react";
import { marked, type Token, type Tokens } from "marked";
import DOMPurify from "dompurify";
import hljs from "highlight.js";
import { Check, Copy } from "lucide-react";
import { useState } from "react";
import { api } from "./api";
import { Mermaid } from "./Mermaid";
import { Lightbox } from "./Lightbox";
import type { Theme } from "./theme";

// Block-level markdown renderer. Code fences, tables, and headings become
// real React nodes; inline content renders through marked → DOMPurify (the
// transcript is agent-controlled, so raw HTML must never execute). Mermaid
// fences render as diagrams.

function inlineHtml(src: string): string {
  const html = marked.parseInline(src) as string;
  return DOMPurify.sanitize(html);
}

function escapeHtml(src: string): string {
  return src
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function codeHtml(code: string, lang: string | undefined): string {
  const trimmed = code.replace(/\n$/, "");
  if (lang && hljs.getLanguage(lang)) {
    try {
      return hljs.highlight(trimmed, { language: lang }).value;
    } catch {
      // fall through to auto-detect
    }
  }
  try {
    return hljs.highlightAuto(trimmed).value;
  } catch {
    return escapeHtml(trimmed);
  }
}

function CopyButton({ text }: { text: string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      className="copy-btn"
      title="Copy"
      onClick={(e) => {
        e.stopPropagation();
        void navigator.clipboard?.writeText(text).then(() => {
          setDone(true);
          window.setTimeout(() => setDone(false), 1200);
        });
      }}
    >
      {done ? <Check size={12} /> : <Copy size={12} />}
      <span>{done ? "Copied" : "Copy"}</span>
    </button>
  );
}

function renderBlock(tok: Token, theme: Theme, key: number): React.ReactNode {
  switch (tok.type) {
    case "paragraph": {
      const p = tok as Tokens.Paragraph;
      return <p key={key} dangerouslySetInnerHTML={{ __html: inlineHtml(p.text) }} />;
    }
    case "heading": {
      const h = tok as Tokens.Heading;
      const Tag = `h${h.depth}` as "h1" | "h2" | "h3" | "h4";
      return <Tag key={key} dangerouslySetInnerHTML={{ __html: inlineHtml(h.text) }} />;
    }
    case "list": {
      const list = tok as Tokens.List;
      const items = list.items.map((item: Tokens.ListItem, i: number) => (
        <li key={i} dangerouslySetInnerHTML={{ __html: inlineHtml(item.text) }} />
      ));
      return list.ordered ? <ol key={key}>{items}</ol> : <ul key={key}>{items}</ul>;
    }
    case "blockquote": {
      const quote = tok as Tokens.Blockquote;
      return (
        <blockquote key={key} dangerouslySetInnerHTML={{ __html: inlineHtml(quote.text) }} />
      );
    }
    case "table": {
      const table = tok as Tokens.Table;
      return (
        <div className="table-wrap" key={key}>
          <table>
            <thead>
              <tr>
                {table.header.map((cell: Tokens.TableCell, i: number) => (
                  <th key={i} dangerouslySetInnerHTML={{ __html: inlineHtml(cell.text) }} />
                ))}
              </tr>
            </thead>
            <tbody>
              {table.rows.map((row: Tokens.TableCell[], ri: number) => (
                <tr key={ri}>
                  {row.map((cell: Tokens.TableCell, ci: number) => (
                    <td key={ci} dangerouslySetInnerHTML={{ __html: inlineHtml(cell.text) }} />
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    }
    case "code": {
      const code = tok as Tokens.Code;
      const lang = code.lang ?? "";
      if (lang.toLowerCase() === "mermaid") {
        return <Mermaid key={key} code={code.text} theme={theme} />;
      }
      return (
        <div className="code-block" key={key}>
          <div className="code-head">
            <span className="code-lang">{lang || "text"}</span>
            <CopyButton text={code.text} />
          </div>
          <pre>
            <code
              className={lang ? `language-${lang}` : undefined}
              dangerouslySetInnerHTML={{ __html: codeHtml(code.text, lang || undefined) }}
            />
          </pre>
        </div>
      );
    }
    case "hr":
      return <hr key={key} />;
    case "space":
      return null;
    case "text": {
      const text = tok as Tokens.Text;
      if (!text.text.trim()) return null;
      return <p key={key} dangerouslySetInnerHTML={{ __html: inlineHtml(text.text) }} />;
    }
    default:
      return null; // html and unknown tokens are dropped, never executed
  }
}

function handleLinkClick(e: React.MouseEvent, onImage: (src: string) => void) {
  const target = e.target as HTMLElement;
  const anchor = target.closest("a");
  if (anchor) {
    e.preventDefault();
    api.openUrl(anchor.href);
    return;
  }
  if (target.tagName === "IMG") {
    e.preventDefault();
    onImage((target as HTMLImageElement).src);
  }
}

export function Markdown({ text, theme }: { text: string; theme: Theme }) {
  const [zoomSrc, setZoomSrc] = useState<string | null>(null);
  const nodes = useMemo(() => {
    const tokens = marked.lexer(text, { gfm: true });
    return tokens.map((tok, i) => renderBlock(tok, theme, i));
  }, [text, theme]);
  return (
    <>
      <div className="md" onClick={(e) => handleLinkClick(e, setZoomSrc)}>
        {nodes}
      </div>
      <Lightbox
        open={zoomSrc !== null}
        onClose={() => setZoomSrc(null)}
        label="Enlarged image"
      >
        {zoomSrc && <img className="zoom-img" src={zoomSrc} alt="" />}
      </Lightbox>
    </>
  );
}
