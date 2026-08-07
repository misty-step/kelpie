# Direction Packet — Lane A: Workflow first

**Direction ID:** A-Workflow
**Title:** Workflow first — staged `draft → send → session rail`

## One-sentence thesis
Stage the composer's attention vertically so the primary task (draft + send) owns the top of the card, while session state (workspace, model, thinking) lives on a quiet seam-docked rail below that never competes with Send.

## Lead lens and skill read
- **Lead lens:** workflow and information hierarchy — put the primary task first, make session progress obvious without hunting.
- **Skill read:** `skill://baseline-ui` (native controls, labels, visible focus, Escape closes menus, reduced-motion). Applied where it fits a static HTML prototype.

## Host route or standalone-route reason
**Standalone scratch route.** This is a locked decision question (how should the composer look/behave) with no production surface to mutate. Per prototype boundaries, exploration lives under `desktop/scratch/prototypes/composer/a-workflow/` and never touches `desktop/src` or `desktop/src-tauri`. A one-command `python3 -m http.server` serves it with zero build.

## Information hierarchy and primary interaction
Three tiers, one per row, staged top-down:
1. **Textarea (hero).** Largest element; `:focus-within` draws an indigo ring around the whole card.
2. **Primary action row.** `+` (slash, secondary) on the left; **Send** is the single filled-indigo primary action, right-aligned, disabled until draft is non-empty.
3. **Session meta rail.** A hairline `#efefef` seam separates this quieter band: workspace `powder` (read-only dot, not a control) + editable **model** and **thinking** pickers. Band is always visible but visually subordinate and dimmed during busy.

Primary interaction: type → Send. Secondary: `+` or `/` opens the slash menu; model/thinking pickers are reachable but demoted.

## Layout and responsive behavior
- Single chat column (`max-width: 920px`) centered in the `#f3f3f3` shell; a thin fake header stub shows only that **interrupt/Esc live in the chat header, never in the composer**.
- Script transcript (`flex:1`, scrollable) rolls 2 exchanges + a collapsed Ask card; pending-ask shows a calm thin banner above the card **plus** the transcript Ask card (banner points at it, never a dead link).
- Narrow widths wrap the session band chips to clean rows; no horizontal overflow (verified at 390px). Composer is the bottom-pinned card.

## Token and accessibility notes
- Tokens used verbatim: surfaces `#f3f3f3`/`#ffffff`, border `#e0e0e0`, seam `#efefef`, text `#2d2e30`/`#666`/`#898989`, indigo `#8891e1`, awaiting green `#c1e2ce`/`#3c9663`; mono for model chips.
- Native elements: `textarea` (labeled), `<button>` controls; menus are `role="listbox"` with `aria-expanded`, `aria-selected`, `aria-activedescendant`-style active index; Escape closes and refocuses the trigger.
- Visible `:focus-visible` indigo outline; `prefers-reduced-motion` disables transition/spin.
- Contrast pass: helper text and hint bumped from `#898989` to `#666`; a decorative low-contrast "session meta" note was **removed** (erasure) rather than shipped weak.
- Touch targets: send 38px+, slash 38px, chips 30px with 8px band padding.

## Path to preview and screenshots
- **Preview:** `desktop/scratch/prototypes/composer/a-workflow/preview.html` (self-contained, inline CSS/JS, zero deps).
- **Rendered evidence (this session, 1440px desktop / 390px mobile):**
  - `file:///tmp/omp-sshots-154c4de9dcf81143.webp` — desktop, slash menu open (all 5 commands, clean gap above card)
  - `/tmp/omp-sshots-154c4e2299b81146.webp` — desktop, pending-ask banner + Ask card
  - `/tmp/omp-sshots-154c4e22a9b81147.webp` — desktop, filled ready-to-send
  - `/tmp/omp-sshots-154c4e22b9381148.webp` — mobile, quiet empty
  - Browser smoke test drove quiet/slash/pending/filled/busy, keyboard arrows/enter/escape, and model/thinking selection (all pass; exact results in Main's record).

## Gate results (file:line in `preview.html`)
- Fixed slash-menu CSS bug: absolute `bottom` anchor with `top:auto` forced a 40px clipped box (CSS 2.1 §10.6.4). Re-anchored with `top:0` + `translateY(-100% - 8px)` and `height:auto` → full 235px menu, 7px gap. `preview.html` `.menu` base + `.menu.slash`.
- Enter now selects the focused option, not stale `aria-selected`. `wireMenuKeys` branch.
- Pending state keeps the Ask card visible so the banner's "choose an answer" is actionable. `applyState`.
- Esc/Escape closes all menus and refocuses trigger; verified.
- Reduced-motion respected; no gradients, one accent (indigo) per view, green reserved for awaiting dot.

## Tradeoffs
- **Gains:** clear attention staging; session state always visible but subordinate; Send stays the single primary action; dash of "set and forget" meta does not tax the draft path.
- **Costs:** the rail adds a full row of vertical height to the composer vs. a single footer row; two pickers add one extra tap for a user who only drafts.
- **Risks:** a tall slash menu floats over transcript content (inherited by all lanes); seam-docked rail could read as two cards if borders are tuned wrong — gap/padding budget requires care.
- **Rejected defaults:** generic "status chips + send in one footer row" (competes with Send, no hierarchy); Ask-options inside the box (brief excludes); interrupt/Esc in composer (brief excludes — kept as header stub only).

## Assumptions and unresolved questions
- Assumed pending-ask should surface both a banner (above box) and the transcript Ask card together, since the banner's copy references the card.
- Assumed model/thinking pickers recur as monospace chips; if the real UI keeps them in a settings panel instead, the rail collapses to one read-only workspace chip and a lighter band.
- Width budget for the model chip at 900px is comfortable; at <360px the long `openai/gpt-5.3-codex` name should truncate (not yet implemented — prototype only).

## One-command run
```
python3 -m http.server 8765 --directory desktop/scratch/prototypes/composer/a-workflow
```
Then open `http://localhost:8765/preview.html`.
