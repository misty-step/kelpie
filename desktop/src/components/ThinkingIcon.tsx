/**
 * Thinking-level icon ladder, matching the omp TUI glyphs (theme.ts):
 *   off ⊘ · minimal ○ · low ◔ · medium ◑ · high ◒ · xhigh ● · max ◉ · auto dashed ○
 * Fill fraction rises with level; max is ring + dot so it stays distinct from
 * xhigh's solid fill without needing a background color to punch a hole.
 */
export function ThinkingIcon({
  level,
  size = 12,
}: {
  level: string;
  size?: number;
}) {
  const normalized = normalizeLevel(level);
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      className={`thinking-ico level-${normalized}`}
      aria-hidden="true"
    >
      {shape(normalized)}
    </svg>
  );
}

function normalizeLevel(level: string): string {
  const t = level.trim().toLowerCase();
  if (t === "extra high" || t === "extra-high" || t === "extra_high") return "xhigh";
  if (t === "med") return "medium";
  if (t === "min") return "minimal";
  if (t === "none") return "off";
  switch (t) {
    case "off":
    case "minimal":
    case "low":
    case "medium":
    case "high":
    case "xhigh":
    case "max":
    case "auto":
      return t;
    default:
      return "unknown";
  }
}

const RING = <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1.5" />;

function shape(level: string) {
  switch (level) {
    case "off":
      return (
        <>
          {RING}
          <line x1="4.5" y1="11.5" x2="11.5" y2="4.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </>
      );
    case "low":
      return (
        <>
          {RING}
          <path d="M8 8 L8 2 A6 6 0 0 1 14 8 Z" fill="currentColor" />
        </>
      );
    case "medium":
      return (
        <>
          {RING}
          <path d="M8 2 A6 6 0 0 1 8 14 Z" fill="currentColor" />
        </>
      );
    case "high":
      return (
        <>
          {RING}
          <path d="M8 8 L8 2 A6 6 0 1 1 2 8 Z" fill="currentColor" />
        </>
      );
    case "xhigh":
      return <circle cx="8" cy="8" r="6" fill="currentColor" stroke="currentColor" strokeWidth="1.5" />;
    case "max":
      return (
        <>
          {RING}
          <circle cx="8" cy="8" r="3" fill="currentColor" />
        </>
      );
    case "auto":
      return (
        <circle
          cx="8"
          cy="8"
          r="6"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeDasharray="2.5 2"
        />
      );
    default:
      return RING;
  }
}
