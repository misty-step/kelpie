# Packet B — Place narrative
**ID:** B · **Title:** Workspace place cards with monogram + path story
**Thesis:** Treat each workspace as a place card (rail, monogram, path); agents read as people in that place with cwd in prose.
**Lead lens:** identity / narrative · improve-ui/baseline-ui
**Host:** standalone scratch
**Hierarchy:** Place header (identity+cwd) → agent title+pill → story line (“In basename · last active”)
**Layout:** 312px; airy cards; indigo rail; selected = brand wash + left bar
**Tokens/a11y:** Codex + brand rail; text status pills not color-only
**Preview:** preview.html
**Tradeoffs:** Gains—strong place identity, memorable; Costs—fewest panes per viewport; Risks—prose cwd slower for power users; Rejected—tabular density
**Run:** `python3 -m http.server 8782 --directory desktop/scratch/prototypes/sidebar/b-identity`
