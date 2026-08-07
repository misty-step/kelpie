# Lane B — The Session Story

## Direction ID and title
**B — The Session Story**: a guided-writing surface with an indigo identity rail, where the session relationship is stated as a sentence instead of a chip bar.

## One-sentence thesis
Kelpie's composer should read as a deliberate writing surface for steering the agent — the draft is a *direction*, and the session story ("guiding *powder* with *model* at *effort* reasoning") frames it as a relationship, not a terminal prompt.

## Lead lens and skill read
- Lens: product identity and narrative rhythm.
- Skill read: `skill://improve-ui` — read its contract (surface selection, rendered evidence, proof-before-finding). Applied as: every claim below is a design decision for a scratch prototype, evidenced by the rendered preview in this session (desktop 1100px + mobile 390px captures). No production source was traced or modified; the prototype route is `desktop/scratch/prototypes/composer/b-identity/` per the brief.

## Host route or standalone-route reason
Standalone scratch preview: `desktop/scratch/prototypes/composer/b-identity/preview.html`. No host route exists for this design question — the brief's desktop-first chat column with Codex-aligned tokens does not match the phone-first PWA tokens in `DESIGN.md` (The Well), so the preview is deliberately token-standalone until the chief folds the direction into the catalog.

## Information hierarchy and primary interaction
Top → bottom inside the chat column:
1. Thin header stub — session title + explicit note that Interrupt/Esc live *there*, never in the composer (locked rule made visible).
2. Transcript — 1 user bubble, 1 short assistant line, and an Ask card that holds the pending-ask choices (choices never live in the composer).
3. Composer card (identity device: 4px indigo rail running the full left edge + indigo caption dot):
   - **Caption**: `Follow-up to powder` (wordmark-adjacent, uppercase; workspace = quiet read-only mono label).
   - **Session story sentence**: `Guiding with [model] at [thinking] reasoning` — model and thinking are *native selects styled as sentence words* (editable fixture pickers, mono chip for model, indigo underlined word for thinking). They read as prose, not controls.
   - **Pending-ask**: thin one-line green banner (dot + text + Review affordance) above the stage; resolves to the transcript Ask card.
   - **Stage**: recessed card-in-card writing surface (the only outlined box, indigo focus ring).
   - **Action row**: deliberately quiet — `+ Commands` (slash-only) left, `Send →` right, one filled indigo action. Nothing else: no attach, no interrupt, no Esc.

Primary interaction: type a direction, adjust who you're guiding (model/thinking) inline, insert slash commands, send. The story sentence is the mental model for every session control.

## Layout and responsive behavior
- Desktop-first: centered 960px chat column on `#f3f3f3`, composer card `#ffffff` with 16px radius, 4px indigo rail, hairline borders.
- The rhythm is vertical and airy (caption → sentence → stage → action row), explicitly *not* a horizontal toolbar.
- <560px: padding shrinks, the sentence wraps gracefully (verified at 390px), the story chips go full-width, rail thins to 20px inset; composer bottom (Send) stays reachable in-viewport after scroll (verified 390×844, sendTop 789px).
- States: quiet empty · slash menu open (inline popover list) · pending-ask (thin banner + expanded Ask card) · filled (Send enabled) · busy (spinner + contained bottom shimmer, draft retained during send, cleared after). All toggled from the preview shell.

## Token and accessibility notes
- Tokens (brief-locked, Codex-aligned): surfaces `#f3f3f3`/`#ffffff`; borders `#e0e0e0`; seam `#efefef`; text `#2d2e30`/`#666`/`#898989`; brand indigo `#8891e1`; awaiting green `#c1e2ce`/`#3c9663`; mono for model chips and slash names.
- Off-token justifications: `--brand-ink #5b66c8` (AA action fill/text — `#8891e1` white-on fails at 2.9:1); `--await-fg #1f6838` (`#3c9663` on `#c1e2ce` measures 2.6:1 — fails AA; #1f6838 = 4.8:1, same hue family); `--brand-tint #eeeffc` (focus ring wash).
- AA: body/controls pass (ink2 `#666` 5.7:1 on white). `#898989` (3.5:1) is used only on the preview-scaffold note in the header stub and decorative dots — noted as the brief's faint token; every meaningful label was bumped to `#666`.
- Keyboard: native selects + native buttons throughout; visible `:focus-visible` rings on all controls; `aria-expanded` on the + Commands toggle; textarea labeled.
- Touch targets: Send / + Commands 44px min-height, slash rows 44px, Ask choices 42px.
- Reduced motion: banner dot pulse, send spinner, and busy shimmer all disabled under `prefers-reduced-motion` (plus a global transition kill).
- Semantics: no color-only state — pending banner pairs icon/dot + text; state toggles are text buttons.

## Preview and screenshot paths
- Preview: `desktop/scratch/prototypes/composer/b-identity/preview.html` (self-contained, zero build; inline CSS/JS only, no deps, no CDN).
- Session captures (desktop 1100×900 and mobile 390×844 verified this session; paths are the render session's temp files):
  - Quiet (desktop): `/tmp/omp-sshots-154c4c2caf4328f0.webp`
  - Pending-ask (desktop, post-contrast pass): `/tmp/omp-sshots-154c4c2c53c328ef.webp`
  - Slash open (desktop): `/tmp/omp-sshots-154c4b76bf0328e3.webp`
  - Filled (desktop): `/tmp/omp-sshots-154c4b774fc328e5.webp`
  - Busy (desktop, after shimmer fix): `/tmp/omp-sshots-154c4b9fa0c328e7.webp`
  - Mobile quiet / slash / pending (390px): `/tmp/omp-sshots-154c4bb6204328e8.webp`, `/tmp/omp-sshots-154c4bb65ac328e9.webp`, `/tmp/omp-sshots-154c4bb6954328ea.webp`

## Tradeoffs
- Gains: session controls become legible prose (model/thinking are self-explanatory in context); strong identity device (indigo rail + caption) differentiates the composer from any dense chip bar; one clear primary action keeps the surface calm; the pending ask stays thin and out of the way, matching the locked rule.
- Costs: sentence-style selects sacrifice a conventional picker look (users must discover the underline affordance); the airy vertical rhythm takes ~15% more vertical space than a dense footer; inline model id can wrap on narrow columns (mitigated: wraps as a unit, verified at 390px).
- Risks: if operators expect terminal-speed control density, the sentence framing reads as decorative; "Follow-up to powder" caption assumes a single active workspace framing (multi-workspace sessions would need the caption to adapt); native select popups render OS-styled and cannot carry the indigo identity inside the dropdown itself.
- Rejected defaults: dense chip toolbar above the textarea (the terminal-prompt default — rejected: no identity, no narrative, controls read as bare chrome); putting ask choices in the composer (locked out of scope); attach/permissions/interrupt/Esc anywhere in the composer (locked out of scope); putting model/thinking in a separate settings sheet (rejected: breaks the session-story sentence and adds a round trip).

## Assumptions and unresolved questions
- Assumed: one active session per composer (caption "Follow-up to powder"); the workspace label stays read-only and quiet; pending asks always arrive with an Ask card in the transcript to point to; the composer sits inside a chat column that already has a header (interrupt/Esc home).
- Unresolved: should the story sentence also show *workspace* as an editable word once multi-workspace sessions exist (brief says read-only today — kept read-only)? Does the narrative caption scale when the model id is long on a 900px column? Should the slash menu's source badges carry the green/indigo/gray coding into production, or is a single neutral badge enough? (Prototype shows the coding as the fixture brief specifies.)

## One-command run instruction
```bash
python3 -m http.server 8766 --directory desktop/scratch/prototypes/composer/b-identity
```
Then open `http://127.0.0.1:8766/preview.html` (works via `file://` too — zero build).
