---
colors:
  background: "#f4f3f1"
  surface: "#ffffff"
  overlay: "#e4e2de"
  hairline: "#d8d5d0"
  foreground: "#1c1b1a"
  muted: "#6b6862"
  faint: "#706c66"
  accent: "#0f6e6e"
  accentText: "#0a5252"
  working: "#1f6a3a"
  blocked: "#b8332a"
  idle: "#6b6862"
  done: "#45637a"
  backgroundDark: "#14130f"
  surfaceDark: "#1e1d1a"
  overlayDark: "#2a2824"
  hairlineDark: "#383631"
  foregroundDark: "#e8e6e1"
  mutedDark: "#9a968e"
  faintDark: "#8a8680"
  accentDark: "#2dd4cf"
  accentTextDark: "#5ee5e0"
  workingDark: "#65c97b"
  blockedDark: "#ff7a72"
  idleDark: "#9a968e"
  doneDark: "#82a8c4"
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

Each status has a text color and a soft background color (for chips). Active
work is green: it means the current task is moving. A pending ask is red because
it requires the operator. Completed work moves to blue-gray so success never
competes with the active-work signal. All text variants pass AA contrast in
their theme.

| Status   | Light text  | Dark text  | Meaning                    |
|----------|-------------|------------|----------------------------|
| working  | `#1f6a3a`   | `#65c97b`  | current task is executing  |
| blocked  | `#b8332a`   | `#ff7a72`  | pending ask, needs input    |
| idle     | `#6b6862`   | `#9a968e`  | alive but not working       |
| done     | `#45637a`   | `#82a8c4`  | completed                   |
| unknown  | `#6b6862`   | `#9a968e`  | status not reported         |

Blocked is the highest-attention state and uses an attention pulse. Working
uses a slower breathing pulse. Both are opacity-only and disabled under
reduced motion.

## Workspace identity

### Problem

Workspaces churn constantly. Hardcoding identity (icon, color) to today's
names breaks the moment a workspace is added, renamed, or removed. The
system must derive identity deterministically at runtime from the workspace
name alone.

### Solution

**Icon:** Each workspace name hashes directly into a fixed vocabulary of 35
Lucide icons. There are no name-specific assignments: every current and future
workspace follows the same function. The hash is a DJB2-style multiply-and-add
over UTF-16 code units (`h = h*31 + unit`), chosen for speed, deterministic
cross-language behavior, and even distribution over the small vocabulary.

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

Pill chips remain visible even in one-tab workspaces so lifecycle controls
never disappear. The active tab gets an accent border and its own 44px close
target. New-tab and workspace-close controls follow the chips. Both destructive
actions use explicit bottom-sheet confirmation; create/close requests take
synchronous locks, disable repeat input, and show `Adding…` or `Closing…`.
Tab creation carries a stable action id: the bridge accepts once, serializes
all lifecycle mutations per workspace, and exposes status readback so a lost
POST response cannot create a duplicate tab. Tab/workspace close also refuses
while an affected pane write is active.

### Session/term header

Single compact row: back chevron (44px), workspace avatar (28px), workspace
name as the primary text, and a status dot on the right. The pane title lives
in the composer's meta row, not the header. The dot is 12px inside a 44px
tappable button (tap toasts the status word; the button carries the
aria-label): blocked = red attention pulse, working = green breathing pulse,
idle = static gray, done = static blue-gray with an inset check (non-color cue),
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

Effort options come from the exact active selector's `/api/models` entry. The
sheet shows a loading state until capabilities arrive; it never falls back to
an unrelated catalog model. Selection sends the requested level, while the
bridge advances omp's `app.thinking.cycle` one rendered state at a time using
paced raw CSI Z input. Transitions are serialized per pane. Kelpie only
confirms success after the live terminal footer matches the requested level;
an unreadable footer remains explicitly unverified.
Send disabled = overlay fill + faint text. `/` opens slash-command
autocomplete. Send is disabled when there is neither text nor an attachment,
while an upload is in flight, or while a reasoning-effort or model change is
being applied.

Composer drafts are pane-local browser state, keyed by pane id and written on
every input event. A keyed session component hydrates the correct draft before
first paint when routing between panes. Route changes, reloads, and background
SSE refreshes therefore cannot replace or clear text. Send captures the exact
submitted value, disables conflicting actions, and clears storage only when
the confirmed response returns and the current textarea still equals that
submitted value; edits made while a request is in flight win.
The action id is persisted beside the draft before submission; if that durable
write fails, no terminal input is sent. If Enter crossed the terminal boundary
but receipt readback stays ambiguous, every fresh send and
pane lifecycle action remains blocked across navigation and reload. The session
links to the raw terminal; only an explicit “I checked” acknowledgement there
releases the unresolved action, so an unchanged draft cannot silently execute
twice under a new id.
For agent panes, the bridge verifies the visible `❯` composer is present and
empty before typing. Raw shell panes do not require omp-specific chrome, but
still require the typed marker to appear before Enter crosses the submit
boundary. Kelpie never clears text entered from another control surface.

### Bottom sheets

One generic sheet primitive (scrim + bottom panel, 70dvh max, iOS drawer
curve `cubic-bezier(0.32, 0.72, 0, 1)`, `@starting-style` rise; scrim tap
dismisses).

- **Model sheet** (tap model chip): full catalog from `/api/models`. The
  bridge serves a validated persistent last-known-good immediately, tags it
  with the producing OMP version, rejects it on a known version mismatch, and
  coalesces one bounded background `omp models --json` refresh. Timed-out
  subprocesses are killed. Grouped by provider, current provider first, current
  model highlighted; provider headers stay sticky while scrolling and every
  model row repeats `provider · id`, so same-named models from Anthropic,
  Cursor, OpenRouter, etc. cannot be confused. Filter field on top; 60-row
  render cap with a "type to narrow" hint. Selecting calls
  `POST /api/pane/{id}/model`. The bridge drives omp's session-only `/switch`
  picker rather than `/model`, because `/model` mutates role assignments.
  It opens `/switch`, searches by the complete `provider/model` selector,
  confirms the selected row, and waits for omp's printed
  `Session-only model: <selector>` receipt. Empty sessions that do not persist
  a model-change receipt are confirmed against the live selector instead.
  Picker input is paced to respect omp's debounce, Nerd Font glyphs are
  stripped before screen matching, and failures unwind the picker with Escape.
  Model and effort changes share the same per-pane lock. The chip shows the
  confirmed selector as an override until the session file catches up. A
  provider without credentials fails cleanly; the catalog remains a superset
  of the providers configured in the current session. If a control-driver
  deadline expires mid-picker, the pane lock remains held while Kelpie sends
  bounded Escape cleanup, clears only its own partial `/switch` command, and
  verifies the empty composer before unlocking.
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
accept=image/* multiple>`). Each photo is POSTed raw to
`/api/pane/{id}/upload` (32 MB limit); the bridge atomically writes it to a
unique temp path and returns that absolute path. Concurrent uploads cannot
overwrite one another. Pending attachments render as removable chips above the
status row. Send and lifecycle controls remain disabled until every upload
settles. On send, the file paths are appended to the message body — omp's read
tool decodes images natively, so the agent can open them directly.

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
  theme-specific: green marks active work, red marks required operator input,
  and blue-gray marks completion without competing with either.
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
  a tier, newest activity comes first; workspace label and pane id are stable
  tie-breakers.
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
