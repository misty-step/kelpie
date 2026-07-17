---
colors:
  background: "#f4f3f1"
  surface: "#ffffff"
  overlay: "#e4e2de"
  hairline: "#d8d5d0"
  foreground: "#1c1b1a"
  muted: "#6b6862"
  faint: "#9b9893"
  accent: "#0f6e6e"
  accentText: "#0a5252"
  working: "#945500"
  blocked: "#b8332a"
  idle: "#6b6862"
  done: "#2e6b3e"
  backgroundDark: "#14130f"
  surfaceDark: "#1e1d1a"
  overlayDark: "#2a2824"
  hairlineDark: "#383631"
  foregroundDark: "#e8e6e1"
  mutedDark: "#9a968e"
  faintDark: "#6b6862"
  accentDark: "#2dd4cf"
  accentTextDark: "#5ee5e0"
  workingDark: "#f0a030"
  blockedDark: "#f0625a"
  idleDark: "#9a968e"
  doneDark: "#5eb872"
typography:
  fontFamily: "-apple-system, BlinkMacSystemFont, Segoe UI, Roboto, sans-serif"
  monoFamily: "ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, monospace"
  body: "15px / 1.4"
  title: "16px / 700 / -0.01em"
  chip: "10.5px / 600 / uppercase / 0.03em"
  metadata: "11-12px"
  mono: "12px / 1.4"
rounded:
  sm: "8px"
  md: "12px"
  lg: "16px"
  pill: "999px"
spacing: ["4px", "6px", "8px", "10px", "12px", "14px", "16px"]
---

# kelpie — Signal Deck

## Overview

kelpie is a phone-first PWA for triaging a fleet of omp coding agents running
in herdr terminal workspaces. One expert operator, one-handed iPhone use
(390×844), served over Tailscale. Views: inbox (fleet triage), agent session
(transcript + composer), terminal (raw screen + key row).

The design language is **Signal Deck**: a fleet operations console, not a chat
app. Graphite base (warm stone grays), single teal accent, semantic status
colors. Dense, information-first, professional. Every pixel earns its place by
carrying signal.

## Design thesis

Previous lab-001 produced 15 candidates across decorative lanes (anthro,
brutalist, soft, minimal, impeccable) and was rejected wholesale. The lesson:
kelpie is an operational tool, not a canvas. Signal Deck strips decoration to
zero and invests the entire visual budget in scannability and status clarity.

The operator's core loop is triage: scan the inbox, spot the pane that needs
attention, act, repeat. The design optimizes for this loop at every layer.

## Color architecture

### Base palette

Warm stone graphite. Not neutral gray — warm. The difference is felt: pure
gray reads as sterile; warm stone reads as paper, which is what a terminal
operator's eye expects after decades of reading logs on light backgrounds.

- `--bg` (page): light warm stone `#f4f3f1` / dark `#14130f`
- `--surface` (cards, headers): pure white `#ffffff` / dark `#1e1d1a`
- `--overlay` (pressed states, desktop frame): `#e4e2de` / `#2a2824`
- `--hairline` (borders): `#d8d5d0` / `#383631`
- `--text`: near-black `#1c1b1a` / `#e8e6e1`
- `--muted`: `#6b6862` / `#9a968e` (secondary text, timestamps)
- `--faint`: `#9b9893` / `#6b6862` (tertiary, section labels, empty states)

### Accent — single teal

One accent color, used for: interactive elements (send buttons, links), active
selection (tab chips, recommended ask options), and connection status (up dot).
Not used for status semantics.

- Light: `#0f6e6e` (deep teal, text-safe on white)
- Dark: `#2dd4cf` (bright teal, text-safe on dark surface)
- Soft variant: 12% mix for backgrounds (active tab, user bubbles)

Teal was chosen over the Rose Pine iris/purple because it reads as
"operational" rather than "decorative." It's the color of status LEDs,
oscilloscope traces, and terminal cursor blocks — the operator's visual
vernacular.

### Status semantics — four states

Each status has a text color and a soft background color (for chips). The
text colors are darkened from their saturated counterparts to pass AA contrast
on the light cream background. Raw amber/orange fails AA on light; a darkened
amber (`#945500`) passes AA at 4.5:1 on white.

| Status   | Light text  | Dark text  | Meaning                    |
|----------|-------------|------------|----------------------------|
| working  | `#945500`   | `#f0a030`  | agent actively executing   |
| blocked  | `#b8332a`   | `#f0625a`  | pending ask, needs input    |
| idle     | `#6b6862`   | `#9a968e`  | alive but not working       |
| done     | `#2e6b3e`   | `#5eb872`  | completed                   |
| unknown  | `#6b6862`   | `#9a968e`  | status not reported         |

Blocked is the highest-attention state and its chip dot pulses (1.4s
ease-in-out) to draw the eye. Working is static — it doesn't need to pulse
because the inbox sort already puts it at the top. This is the only ambient
motion in the system.

## Workspace identity

### Problem

Workspaces churn constantly. Hardcoding identity (icon, color) to today's
names breaks the moment a workspace is added, renamed, or removed. The
system must derive identity deterministically at runtime from the workspace
name alone.

### Solution

**Icon:** Each workspace name hashes into a fixed vocabulary of 35 Lucide
icons. 17 known workspaces have hand-assigned semantic icons (mint→leaf,
overmind→brain, canary→bird, etc.). Unknown names hash into an 18-icon pool
of generic-but-distinct shapes (rocket, anchor, compass, box, cpu, ghost,
globe, key, map, moon, mountain, origami, palette, puzzle, radar, sailboat,
telescope, turtle). The hash is a simple DJB2-style multiply-and-add
(`h = h*31 + charCode`), chosen for speed and even distribution over a small
modulus.

**Color:** Each workspace name also hashes into an 18-step hue vocabulary:
`[210, 25, 340, 160, 45, 280, 190, 0, 130, 320, 75, 245, 15, 195, 55, 295,
110, 230]`. These hues are spread across the color wheel at perceptually
distinct intervals. The avatar background is `hsl(hue, 45%, 92%)` in light
mode and `hsl(hue, 30%, 20%)` in dark mode — low saturation, high/low
lightness, so the color identifies without competing with status colors.

The avatar is a rounded square (8px radius, 36×36px inbox, 28×28px header)
containing a 18px Lucide icon. Never an emoji, never bare initials.

### Why deterministic

The same workspace name always produces the same icon+hue, across sessions,
across devices, without storage. A new workspace gets a stable identity the
moment it appears. No configuration, no state, no drift.

## Typography

System sans for UI, system mono for terminal screens, code, and slash
commands. No web fonts — zero build step, zero network dependency.

- Body: 15px / 1.4 line-height
- Title: 16px / 700 weight / -0.01em tracking
- Card title: 15px / 700 / -0.01em
- Chips: 10.5px / 600 / uppercase / 0.03em tracking
- Metadata: 11-12px / muted color
- Mono (terminal, code, tool names): 12px / 1.4

The uppercase chip with tracking is the signature typographic move: it makes
status labels read as operational indicators, not prose.

## Layout

Single-column phone layout. Fixed header (44px+ touch rows), scrollable
content region (`.scroll`). When the iOS keyboard opens, `#app` shrinks to
the visual viewport height (`height: calc(100dvh - var(--kb-offset))`,
measured via `visualViewport` in `frontend/src/viewport.rs`) — the transcript, ask box, and
composer all stay fully visible above the keyboard; nothing is ever
overlapped. Safe-area insets respected on all edges.

At ≥700px the app centers in a 640px column with hairline borders, so it
holds up on tablet/desktop without redesigning.

## Components

### Fleet card (inbox)

```
┌──────────────────────────────────────────┐
│ [avatar]  workspace-name     ● WORKING   │
└──────────────────────────────────────────┘
```

One row, nothing else — the inbox is a scannable roster, not a feed.
Avatar (36px, deterministic) on the left, workspace name (15px/700) fills
the middle, status chip on the right. A pane with a pending ask shows a
"needs input" chip in the blocked color. Min-height 52px for touch.

### Status chip

Pill with leading dot and a status glyph (question/activity/clock/check).
Uppercase, 10.5px, 600 weight. Color-coded by status. Blocked dot pulses; all
others static. The dot+glyph+text combination is redundant (color + shape)
for colorblind accessibility. Chips appear on inbox cards only — session and
term headers use the status dot instead.

### Tab strip

Pill chips; close requires a second confirming tap (tap shows "confirm?",
3s timeout resets). Active tab gets accent border + soft accent background.
"+" button at the end. The strip only renders with 2+ tabs — a lone tab is
noise; "New tab" lives in the composer's ⋯ sheet, so nothing is lost.

### Session/term header

Single compact row: back chevron (44px), workspace avatar (28px), workspace
name as the primary text, and a status dot on the right. The pane title lives
in the composer's meta row, not the header. The dot is 12px inside a 44px
tappable button (tap toasts the status word; the button carries the
aria-label): blocked = red attention pulse, working = amber breathing pulse,
idle = static gray, done = static green with an inset check (non-color cue),
unknown = hollow ring. Each state adds a faint status-tinted ring. Pulses are
the sanctioned ambient motion and are disabled under reduced motion. A 1px
workspace-hue edge underlines the header. When SSE drops, a tappable amber
"Reconnecting" pill appears beside the dot (nothing is shown while
connected — calm default; tap explains "data may be stale").

### Agent composer

Three stacked strata, clearly separated from the transcript (surface shift +
hairline):

1. **Meta row** — horizontally scrollable (edge-fade mask, no scrollbar),
   hairline underneath: the model chip (cpu icon + full model id — never
   ellipsized, the whole id is readable at 390px; opens the model picker),
   the thinking chip (brain icon; opens an exact effort picker), and a
   non-tappable pane-title chip (max-width 140px) when the title adds info
   beyond the workspace name.
2. **Actions row** — tight (4px gaps, 44px targets): attach, back-to-inbox,
   terminal toggle, Ctrl+C, Esc (text-only red — quiet, reads "careful"),
   ⋯ (rare actions; currently just New tab), spacer, Send — the ONE filled
   accent action, anchored right.
3. **Input row** — the textarea alone, full width; the only outlined box.

Effort options come from `/api/models`; the current level is checked.
Selection updates the chip immediately, then the bridge applies omp's
`app.thinking.cycle` the required number of times using paced raw CSI Z
input. Transitions are serialized per pane. Kelpie only confirms success
after the live terminal footer matches the requested level; an unreadable
footer remains explicitly unverified.
Send disabled = overlay fill + faint text. `/` opens slash-command
autocomplete. Send is disabled when there is neither text nor an attachment,
while an upload is in flight, or while a reasoning-effort or model change is
being applied.

### Bottom sheets

One generic sheet primitive (scrim + bottom panel, 70dvh max, iOS drawer
curve `cubic-bezier(0.32, 0.72, 0, 1)`, `@starting-style` rise; scrim tap
dismisses).

- **Model sheet** (tap model chip): full catalog from `/api/models` (the
  bridge shells `omp models --json` once and caches — the catalog is static
  per omp version). Grouped by provider, current provider first, current
  model highlighted; provider headers stay sticky while scrolling and every
  model row repeats `provider · id`, so same-named models from Anthropic,
  Cursor, OpenRouter, etc. cannot be confused. Filter field on top; 60-row
  render cap with a "type to narrow" hint. Selecting calls
  `POST /api/pane/{id}/model` — omp does not execute arged slash commands
  submitted as composer text (the palette closes once arguments follow the
  name and Enter sends the line as a chat prompt, verified live), so the
  bridge drives omp's own interactive picker: `/model` ⏎ → search the full
  selector → ⏎ role menu (identity-checked against the footer) → ⏎ assigns
  `default` → ⏎ confirms the level menu → Esc until closed. Every stage is
  screen-verified with staged unwind on failure; picker keys pace at 800ms
  (omp debounces faster input); Nerd Font glyphs are stripped before
  matching; an already-default target is detected and never re-entered
  (Enter would toggle the role off); transitions share the per-pane drive
  lock with reasoning changes; and success requires omp's printed
  `Default model: <selector>` receipt within the last screen lines (older
  receipts linger in scrollback). Fresh role assignments reset effort to
  `auto`, so after a confirmed switch the client re-applies the pane's
  prior level through the verified reasoning flow. The chip shows the
  confirmed target as an override until the session file catches up. A
  model whose provider has no credentials fails cleanly ("not available in
  this session") — the catalog is the full `omp models` list, a superset
  of the session's configured providers.
- **Actions sheet** (tap ⋯): Open terminal, New tab, Jump to latest,
  Send Enter, Send Ctrl+C, Back to inbox. Every action has a Lucide icon.

### Keyboard-open compaction

While the keyboard is up (`kb-open` class, driven by `visualViewport`), the
app height becomes `100dvh - keyboardHeight`. Safari also pans its layout
viewport to reveal focused controls; `--vv-top` mirrors `visualViewport.offsetTop`
so the shrunken app stays glued to the visible rectangle instead of leaving
the composer behind the keyboard. Focus settle retries cover Safari's missing
final resize event. The tab strip disappears and the header collapses to one
thin row (~49px vs 97px of chrome): sub-label and avatar hidden, title 14px.
The transcript keeps the reclaimed space.

### Markdown rendering

Assistant and thinking text render a whitelisted markdown subset: fenced
code, tables (block-scroll sideways on overflow, never crush columns),
headings (h1–h4 cap), lists, blockquotes, hr, bold/italic/inline
code/links. Everything passes through `escapeHtml()` before any tag is
introduced — raw HTML in transcripts never executes. User bubbles stay
plain text.

### Photo attachments

The attach button opens the system photo picker (`<input type=file
accept=image/*>`). Each photo is POSTed raw to
`/api/pane/{id}/upload` (32 MB limit); the bridge writes it to a temp
uploads dir and returns the absolute path. Pending attachments render as
removable chips above the status row. On send, the file paths are appended
to the message body — omp's read tool decodes images natively, so the agent
can open them directly.

### Terminal composer

Text input + Send, then a key row with icons (Enter, Esc, Ctrl+C, Up, Down,
Tab). Terminal screen is plain text, `pre-wrap` + `overflow-wrap: anywhere`
(pane PTYs are ~160 cols; soft wrap beats horizontal scroll on a phone). For
agent panes the header carries a chat toggle back to the session view — the
terminal is never a one-way trap.

### Kelpie mark

A custom line-art mark — a horse head cresting out of a wave, the two halves
of the mythological water horse — drawn dark-on-transparent and inverted via
CSS in dark mode (`assets/kelpie-icon-source.png` is the master). It appears
in a soft accent tile beside the inbox wordmark (`static/kelpie-mark.png`),
as the favicon (`static/favicon.png`), and as the iOS home-screen icon
(`static/apple-touch-icon.png`, white tile).

### Avatar system

Rounded square (8px radius) with deterministic icon + hue. Three sizes:
- `avatar-sm` (28px, header): 15px icon
- `avatar` (36px, inbox cards): 18px icon
- `avatar-lg` (44px, reserved): 22px icon

## Motion

Interaction-only. Never animate idle data. Tokens (`:root`):

- `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)` — entrances, presses; strong
  curve because built-in CSS easings are too weak
- `--ease-in-out: cubic-bezier(0.77, 0, 0.175, 1)` — on-screen movement
- `--press: scale(0.97)` — universal press feedback

Inventory:

- Press feedback: every pressable scales to 0.97 on `:active`
  (cards: 0.985 — large surfaces need less), 160ms `--ease-out`. Feedback on
  pointer-down, not release.
- FLIP resort (inbox reorder): 250ms cubic-bezier(0.22, 1, 0.36, 1)
- Card enter: 220ms ease-out (fade + translateY)
- Card exit: 180ms ease (fade + translateY)
- Toast enter: 200ms `--ease-out` fade + 8px rise via `@starting-style`
- Keyboard pin: 150ms ease
- Status dot pulses (session/term header): blocked = attention pulse,
  working = 2.4s breathing pulse — opacity-only, compositor-safe, the only
  ambient motion
- Skeleton shimmer: 1.4s ease infinite

Under `prefers-reduced-motion: reduce`, movement is removed (press scale,
toast rise, FLIP, pulses, shimmer); opacity/color feedback stays.

Destructive affordances stay quiet until armed: tab-close and
attachment-remove ×'s are faint grey; red is reserved for the confirm state
and the Esc interrupt.

Rendering discipline (feel = speed): the transcript renders via keyed
diff/patch (entries are cached nodes; markdown re-parses only on changed
text — never a full teardown on poll/SSE ticks), scroll and visualViewport
handlers are rAF-coalesced, `#app` has no height transition (keyboard resize
lands instantly), the term screen skips writes when unchanged, and sheet
group headers are opaque (no backdrop blur). The bridge polls herdr every
600ms and pushes SSE; all views also refresh on focus and SSE reconnect.

## Accessibility

- **WCAG AA contrast** in both light and dark themes. Status text colors are
  darkened from their saturated counterparts to pass AA on the light
  background (e.g., working text is `#945500`, not `#ea9d34`).
- **44px touch targets** minimum on all interactive elements.
- **Color + shape redundancy:** status uses both color (chip color) and shape
  (dot, uppercase label) so colorblind users can distinguish states.
- **prefers-reduced-motion:** all animations and transitions disabled.
- **Focus visibility:** inputs and buttons show accent-colored focus states.

## Empty states

Every surface handles 0..N workspaces/panes gracefully:

- **Inbox empty:** centered inbox icon + "No agents running" + "Tap + to open
  a workspace" hint.
- **Session empty:** "No messages yet." in the transcript area.
- **Terminal empty:** blank screen (the pane has no content yet).
- **Tab strip:** always visible for sessions — one tab still shows its chip
  plus the add affordance, so tab management is never hidden.

## Do's and Don'ts

- Sort the inbox attention-first: pending ask > working > idle > done. Within
  a tier, order is ALPHABETICAL by workspace name — stable, so cards do not
  churn position as agents emit activity. A card only moves when its
  attention tier changes.
- Workspace identity is deterministic (hash into fixed icon+hue vocabulary),
  never hardcoded, never an emoji, never bare initials.
- Motion only on interaction (FLIP resort, keyboard pin, card enter/exit);
  never animate idle data. Honor `prefers-reduced-motion`.
- Contrast: WCAG AA in both themes (watch status-on-light; use darkened text
  variants).
- Touch targets ≥ 44px.
- No gradient text, glassmorphism, decorative blobs, or side-stripe cards.
- Implementation is Rust end to end: axum bridge + Yew (WASM) frontend in
  `frontend/`, built via `./build-frontend.sh` into `static/wasm/`. CSS stays
  a single hand-written `static/style.css` — tokens in `:root`, dark via
  media query, no preprocessor.

## What was rejected (lab-001)

The lab produced 15 candidates across six decorative lanes (anthro, taste,
minimal, brutalist, soft, impeccable) plus the shipped Rose Pine baseline.
All were rejected. The lesson taken: kelpie doesn't need a personality contest;
it needs operational clarity. Signal Deck is the anti-lab: one coherent voice,
no decoration, every decision in service of the triage loop.
