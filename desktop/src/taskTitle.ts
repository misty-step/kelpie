// OMP terminal titles often embed a braille spinner (⠋⠙⠹…) that only advances
// when herdr polls. That reads as a clunky snap in the UI. Strip those glyphs
// and let CSS own a smooth working spinner instead. Also drop decorative
// prefixes like "π !" that omp/herdr sometimes inject into the pane title.

const BRAILLE = /[\u2800-\u28FF]/g;
const LEADING_SPINNER =
  /^[\s\u25D0-\u25D3\u25F4-\u25F7\u2022\u00B7*◦○●◉◎◐◑◒◓]+/;
const LEADING_DECOR =
  /^(?:π|Π|\u03C0|\u220F)\s*[!|]?\s*/u;

/** Display label for a pane task — spinner glyphs removed, whitespace collapsed. */
export function taskTitle(raw: string | null | undefined, fallback: string): string {
  if (!raw) return fallback;
  const cleaned = raw
    .replace(BRAILLE, "")
    .replace(LEADING_SPINNER, "")
    .replace(LEADING_DECOR, "")
    .replace(/\s+/g, " ")
    .trim();
  return cleaned || fallback;
}
