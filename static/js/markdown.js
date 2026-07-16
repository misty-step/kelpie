import { escapeHtml } from './dom.js';

// ------------------------------------------------------------
// Minimal markdown rendering for assistant text.
// Escapes HTML first, then applies a small whitelist of
// markdown constructs, line-by-line for block structure
// (headings, lists, fenced code) and inline for the rest
// (bold/italic/inline-code/links). Never renders raw HTML
// from the source text — everything passes through
// escapeHtml() before any tag is introduced.
// ------------------------------------------------------------
function renderInline(line) {
  let s = line;
  s = s.replace(/`([^`\n]+)`/g, '<code>$1</code>');
  s = s.replace(/\*\*([^*\n]+)\*\*/g, '<strong>$1</strong>');
  s = s.replace(/(?<!\*)\*([^*\n]+)\*(?!\*)/g, '<em>$1</em>');
  s = s.replace(/\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)/g,
    '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>');
  return s;
}

function renderMarkdown(text) {
  if (!text) return '';
  const escaped = escapeHtml(text);

  // fenced code blocks ```lang\n...\n``` — extracted before any other
  // processing so their contents are never touched by inline rules.
  const fenced = [];
  const withFences = escaped.replace(/```[^\n]*\n([\s\S]*?)```/g, (_, code) => {
    fenced.push(code);
    return '\u0000FENCE' + (fenced.length - 1) + '\u0000';
  });

  const lines = withFences.split('\n');
  const htmlParts = [];
  let para = [];
  let list = null; // { type: 'ul' | 'ol', items: [] }
  let quote = null; // string[]

  const flushPara = () => {
    if (para.length) {
      htmlParts.push(para.join('\n'));
      para = [];
    }
  };
  const flushList = () => {
    if (list) {
      const tag = list.type;
      htmlParts.push('<' + tag + '>' + list.items.map((i) => '<li>' + i + '</li>').join('') + '</' + tag + '>');
      list = null;
    }
  };
  const flushQuote = () => {
    if (quote) {
      htmlParts.push('<blockquote>' + quote.join('\n') + '</blockquote>');
      quote = null;
    }
  };
  const flushAll = () => { flushPara(); flushList(); flushQuote(); };

  const isTableRow = (s) => /^\s*\|.*\|\s*$/.test(s);
  const splitRow = (s) => s.trim().replace(/^\|/, '').replace(/\|$/, '').split('|').map((c) => c.trim());

  for (let li = 0; li < lines.length; li++) {
    const rawLine = lines[li];
    const fenceMatch = rawLine.trim().match(/^\u0000FENCE(\d+)\u0000$/);
    if (fenceMatch) {
      flushAll();
      htmlParts.push('<pre><code>' + fenced[Number(fenceMatch[1])] + '</code></pre>');
      continue;
    }
    // table: header row + |---| separator on the next line
    if (isTableRow(rawLine) && li + 1 < lines.length && /^\s*\|[\s:|-]+\|\s*$/.test(lines[li + 1])) {
      flushAll();
      const head = splitRow(rawLine);
      const rows = [];
      let j = li + 2;
      while (j < lines.length && isTableRow(lines[j])) { rows.push(splitRow(lines[j])); j++; }
      htmlParts.push(
        '<table><thead><tr>' + head.map((c) => '<th>' + renderInline(c) + '</th>').join('') + '</tr></thead>'
        + '<tbody>' + rows.map((r) => '<tr>' + r.map((c) => '<td>' + renderInline(c) + '</td>').join('') + '</tr>').join('') + '</tbody></table>'
      );
      li = j - 1;
      continue;
    }
    const heading = rawLine.match(/^(#{1,6})\s+(.*)$/);
    if (heading) {
      flushAll();
      const level = Math.min(heading[1].length, 4);
      htmlParts.push('<h' + level + '>' + renderInline(heading[2]) + '</h' + level + '>');
      continue;
    }
    if (/^\s*(-{3,}|\*{3,}|_{3,})\s*$/.test(rawLine)) {
      flushAll();
      htmlParts.push('<hr>');
      continue;
    }
    const bq = rawLine.match(/^\s*&gt;\s?(.*)$/);
    if (bq) {
      flushPara();
      flushList();
      if (!quote) quote = [];
      quote.push(renderInline(bq[1]));
      continue;
    }
    flushQuote();
    const ul = rawLine.match(/^\s*[-*]\s+(.+)$/);
    const ol = rawLine.match(/^\s*\d+\.\s+(.+)$/);
    if (ul || ol) {
      flushPara();
      const type = ul ? 'ul' : 'ol';
      if (list && list.type !== type) flushList();
      if (!list) list = { type, items: [] };
      list.items.push(renderInline((ul || ol)[1]));
      continue;
    }
    flushList();
    para.push(renderInline(rawLine));
  }
  flushAll();

  return htmlParts.join('\n');
}

export { renderMarkdown, renderInline };
