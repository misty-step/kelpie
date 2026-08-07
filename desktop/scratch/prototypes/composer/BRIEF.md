# Composer + Ask takeover — confirmed brief

**Question:** How should Kelpie’s session composer look and behave for draft, session chips, and pending-ask takeover?

**Route:** UI / product direction · **N = 5**

## Audience and primary task

Fleet operator in one agent session. Draft a follow-up and send it. When an ask is pending, resolve the ask before anything else.

## Host

Desktop Kelpie chat column footer:
- Production: `desktop/src/components/Composer.tsx` + `AskCard` in `Chat.tsx`
- Exploration only under `desktop/scratch/prototypes/composer/`

## Locked product rules

1. **Primary job:** Draft → send. Session metadata is secondary.
2. **Session rail:** Quiet footer row under the draft (workspace read-only · model · thinking editable).
3. **Send:** Icon-only round send button.
4. **Slash:** Type `/` only — no `+` button.
5. **Hint chrome:** No permanent “Enter sends…” hint. Placeholder + tooltips only.
6. **Ask takeover (critical):** When `pending_ask` is set, the Ask UI **replaces most or all of the raw input section**. Operator must deal with the ask before free-text send is available.
   - Can pick a listed option
   - Can choose “other” / free response
   - Can escape / cancel the ask
   - **Cannot** send a normal follow-up message while an ask is open
7. Interrupt / Esc agent keys stay in the chat header (not the composer), except ask-cancel which is part of ask takeover.
8. Model + thinking editable; workspace read-only.

## Content, states, actions

| State | What shows |
|---|---|
| quiet draft | textarea + round send + session rail |
| slash open | `/` menu above draft |
| filled ready | send enabled |
| busy/sending | send spinner; rail dimmed |
| **ask takeover** | options + other + cancel; draft send blocked |

## Fixed constraints

- Codex-light tokens: bg `#f3f3f3`, surface `#ffffff`, border `#e0e0e0`, text `#2d2e30` / `#666`, brand `#8891e1` / `#4a49e4`, ok `#3c9663` / `#c1e2ce`
- Desktop-first (~1440); also show ~390 mobile crop if easy
- Keyboard reachable; focus visible
- No new deps; fixture data only; self-contained HTML
- Lucide-style simple SVG icons inline (no CDN)

## Success signals

- Draft is visually primary when no ask
- Ask takeover is unmistakable and blocks free send
- Session chips stay quiet and scannable
- Fewer chrome lines than production today

## Out of scope

File attach · permissions chip · titlebar/sidebar redesign · receipt drivers · production API wiring · multi-session

## Directions (structural disagreement required)

| ID | Title | Lead lens | Structural move |
|---|---|---|---|
| A | Workflow first | hierarchy / draft primacy | Tall draft hero; ask becomes a full replacement card in the same footprint |
| B | Session story | identity / narrative | Soft session sentence + recessed stage; ask as modal-in-place dialogue |
| C | Dense ops | spatial scanning | Compact single card; ask options as keyboard-numbered row |
| D | Low load | a11y / calm | Larger type, fewer chips, ask as single-column choice list with clear cancel |
| E | Radical stage | reject card default | No floating card chrome; underline input / docked ask strip |

## Catalog

- Path: `desktop/scratch/prototypes/composer/catalog.html`
- Run: `python3 -m http.server 8770 --directory desktop/scratch/prototypes/composer`
- Open: http://127.0.0.1:8770/catalog.html

Lock is recorded only after the operator’s explicit confirmation.
