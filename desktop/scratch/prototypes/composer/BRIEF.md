# Composer input area — confirmed brief

**Question:** How should Kelpie’s chat input area look and behave for draft + session controls?

**Route:** UI / product direction · **N = 3**

## Audience and primary task
Operator triaging OMP agents. Draft a follow-up and see/adjust session model + thinking without leaving chat.

## Host
Desktop Kelpie chat column (`desktop/src/components/Composer.tsx`). Exploration only under `desktop/scratch/prototypes/composer/`.

## Content, states, actions
- States: quiet draft · slash open · pending-ask banner · filled ready · busy/sending
- Actions: multi-line type · send · slash via `+` or `/` · pick model · pick thinking · read workspace

## Fixed constraints
Codex light tokens; desktop-first; keyboard reachable; no new deps; fixture data only.

## Locked product rules
1. Chrome + session controls (not layout-only).
2. Model + thinking **editable**; workspace **read-only**.
3. `+` = slash commands only (no attach).
4. Interrupt / Esc **not** in composer (chat header).
5. Pending-ask = thin banner only.

## Out of scope
File attach · permissions chip · Interrupt/Esc in composer · ask-option strip · production API · mobile-first.

## Directions
| ID | Title | Structural move |
|---|---|---|
| A | Workflow first | Vertical staging: hero draft → send → demoted session rail |
| B | The Session Story | Identity rail + prose session sentence above recessed stage |
| C | Docked Ops Band | Single horizontal ops baseline; fixed spatial slots |

## Catalog
- Path: `desktop/scratch/prototypes/composer/catalog.html`
- Run: `python3 -m http.server 8770 --directory desktop/scratch/prototypes/composer`
- Open: http://127.0.0.1:8770/catalog.html

Lock is recorded after the operator’s explicit confirmation (browser candidate lock alone is not production authority).
