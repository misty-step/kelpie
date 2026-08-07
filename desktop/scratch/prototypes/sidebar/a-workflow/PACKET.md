# Packet A — Workflow hierarchy
**ID:** A · **Title:** Attention strip + workspace sections + two-line rows
**Thesis:** Put “needs you” first, then workspace groups with a strict title/cwd/time grid so hierarchy and place scan cleanly.
**Lead lens:** workflow / information hierarchy · baseline-ui
**Host:** standalone scratch (no production route mutated)
**Hierarchy:** Needs-you strip → workspace sticky headers → pane title → cwd second line → time column
**Layout:** 304px sidebar; collapsible workspace sections; selected row bordered card
**Tokens/a11y:** Codex light; button rows; aria-expanded on section toggles; mono cwd/time
**Preview:** preview.html
**Tradeoffs:** Gains—urgency + place both visible; Costs—needs strip duplicates a needs pane; Risks—two-line rows cost vertical space; Rejected—flat attention-only list without workspace
**Run:** `python3 -m http.server 8781 --directory desktop/scratch/prototypes/sidebar/a-workflow`
