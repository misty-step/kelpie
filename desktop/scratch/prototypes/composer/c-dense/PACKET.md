# Kelpie Composer — Direction Packet

**Lane C — Dense Ops / Spatial Scanning**

## Direction ID and title
- **ID:** C
- **Title:** "Docked Ops Band" — a single-row composer where every session control owns one stable spatial slot
- **Prototype:** `desktop/scratch/prototypes/composer/c-dense/preview.html`

## One-sentence thesis
Treat the composer as a docked operations toolbar: all session controls (workspace, model, thinking, slash) sit in fixed slots on the same baseline as the text input, so a triaging operator never re-hunts for a control and keyboard reach stays one thumb/finger span away.

## Lead lens and skill read
- **Lead lens:** dense operations and spatial scanning for repeated triage work and keyboard reach.
- **Skill read:** `skill://baseline-ui` (interaction, accessibility, motion, reduced-motion, no animation unless requested). Key applied rules: native elements over ARIA roles, programmatic labels, roving-arrow listboxes with `esc` to close returning focus to the trigger, no animation of layout properties (menus fade/translate on `opacity`/`transform` only), `prefers-reduced-motion` disables all transitions, one accent (indigo) per view, `kbd`-hinted bindings.

## Host route or standalone-route reason
Standalone scratch prototype only. This is a net-new product surface under exploration; no production file under `desktop/src` or `desktop/src-tauri` is touched. No host route exists yet, so the preview ships as a self-contained static file that runs from `file://` or `python3 -m http.server` with zero build.

## Information hierarchy and primary interaction
- **Hierarchy:** transcript (read) → thin pending-ask banner (context) → composer band (act). The composer is the focus for drafting and session control; the chat header above deliberately holds Interrupt/Esc so they never enter the input chrome (locked rule).
- **Primary interaction:** everything is reachable from the one band.
  - Session chips open one-click popovers (Listbox) with roving-arrow keyboard lists; `Enter` applies, `Esc` closes and refocuses the chip. Keycap badges `m` / `t` on the chips advertise the binding.
  - Leading `+` opens slash-only commands (no file attach — locked rule) with source badges (`skill` / `workspace` / `omp`).
  - Workspace (`powder`) is a read-only fixed slot with a lock glyph.
  - Free text goes mid-band; `Enter` sends, `Shift+Enter` newline; Send lights indigo only when non-empty.
  - A bottom status/hint bar is a single reused slot: idle shows key hints, pending shows the take-over prompt, busy shows a live sending line with a spinner.

## Layout and responsive behavior
- **Desktop (target 900–1100 px chat column):** one horizontal band — `[workspace:powder][model▾][thinking:high][+] [ …input… ] [Send]` plus a 26 px status/hint bar below, bounded by a seam. Total composer chrome ≈ 74 px at rest, so the transcript keeps most of the viewport. Popovers open upward and clear the band top.
- **Structure vs narrative lanes:** nothing is stacked. Controls share the input's baseline instead of living on a separate rail above/below; there is no tall textarea, no label row, no second toolbar.
- **Narrow (< 640 px, graceful only):** the band wraps — session chips get their own row, then the input + Send on the next row; keycap badges detach. Nothing is clipped or unreachable. Mobile-first is out of scope by the brief.

## Token and accessibility notes
- **Tokens (Codex-aligned):** surfaces `#f3f3f3` / `#ffffff`; borders `#e0e0e0`; seam `#efefef`; text `#2d2e30` / `#666` / `#898989`; accent indigo `#8891e1`; green awaiting `#c1e2ce` / `#3c9663`; mono `ui-monospace` stack for chips, slash names, and keycaps. All values are CSS variables — no inline magics.
- **Contrast (WCAG AA):** primary text `#2d2e30` on white ≈ 14.9:1; secondary `#666` on white ≈ 5.7:1; muted `#898989` ≥ 4.5:1 where informational; indigo `#8891e1` is used for states/fills with white or `#5b63c9` ink text, not thin-on-light body copy. Green banner uses `#245c3c` on `#c1e2ce` (≈ 7:1).
- **Accessibility:** model/thinking/slash are `role=listbox` + `option` with `aria-expanded` on triggers; roving arrow keys, `Enter` select, `Esc` close returning focus; Send disabled until non-empty; `aria-label`s on the icon-only `+` and the model/thinking slots; `kbd` hints are presentational aid, never the only affordance (mouse click works identically). Reduced-motion disables all transitions/spinners.

## Path to preview HTML (and screenshot path if captured)
- **HTML:** `desktop/scratch/prototypes/composer/c-dense/preview.html` (self-contained, inline CSS + JS, fixture data only).
- **Session screenshots (headless Chromium, 1440×900):**
  - quiet: `/tmp/omp-sshots-154c4e1819652083.webp`
  - slash menu open: `/tmp/omp-sshots-154c4d711dcc3eaa.webp`
  - model popover open: `/tmp/omp-sshots-154c4d85088c3eab.webp`
  - pending-ask banner: `/tmp/omp-sshots-154c4d85680c3eac.webp`
  - filled/ready: `/tmp/omp-sshots-154c4da6608c3ead.webp`
  - sending/busy: `/tmp/omp-sshots-154c4da6cdcc3eae.webp`
  - narrow 420px (graceful wrap): `/tmp/omp-sshots-154c4deec51c649b.webp`
- All states verified rendered with no overlap, clipping, or text cutoff at desktop width.

## Tradeoffs
- **Gains:** highest density and lowest chrome height of the set; stable spatial slots build operator muscle memory; keyboard-first lists and `kbd` hints favor repeated triage; everything reachable in one tap or one arrow key; pending-ask and busy reuse one status slot so the band never grows.
- **Costs:** the single band reads "toolbar-dense," which is less inviting for long-form narrative drafting; a fixed left cluster spends horizontal room that a narrative lane could give to the input; one-row rows cap how much helper text can live beside the box (moved into popovers / status bar).
- **Risks:** density can tempt too much into one row; the `m`/`t` keycap badges can crowd chips when space tightens; reusable popover pattern must stay consistent or muscle memory breaks; dropping Interrupt/Esc from the composer assumes the header stub reliably carries them.
- **Rejected defaults:** a tall card textarea above a separate control rail (narrative default — violates height/density goal); an attach / permissions chip cluster (out of scope by brief); an ask-option strip embedded in the box (locked rule — stays on the transcript Ask card); composing the band as two stacked rows by default (only wrapped below 640 px).

## Assumptions and unresolved questions
- **Assumptions:** the chat header always carries Interrupt/Esc, so the composer can omit them (locked brief — shown as a disabled stub); the column is ≥ 900 px desktop-first; fixture pickers need no persistence; the mono model chip can show a short name with the full provider id in its popover.
- **Unresolved:** exact keybindings for slot-to-slot focus (this preview uses `Tab` between chips and `m`/`t` as shortcut hints — the canonical binding needs a product decision); whether the status/hint bar remains always-visible in production or collapses; whether the `+` slash menu should also accept typing `/` into the input to filter commands.

## One-command run instruction
```bash
python3 -m http.server 8767 --directory desktop/scratch/prototypes/composer/c-dense
```
Then open `http://localhost:8767/preview.html`. The preview also runs directly via `file://`.
