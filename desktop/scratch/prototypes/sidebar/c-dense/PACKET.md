# Packet C — Dense ops table
**ID:** C · **Title:** Sticky workspace labels + fixed columns
**Thesis:** Maximize scan rate with tabular columns [status|title|cwd|age] under tight sticky workspace headers.
**Lead lens:** dense ops / spatial scanning · baseline-ui
**Host:** standalone scratch
**Hierarchy:** col head → workspace micro-header → single-line rows
**Layout:** 320px; 28px rows; mono cwd basename + tabular time; arrow-key move
**Tokens/a11y:** Codex; focusable rows; title attribute full cwd
**Preview:** preview.html
**Tradeoffs:** Gains—most panes visible, stable slots; Costs—least warmth; Risks—basename cwd ambiguous across repos; Rejected—multi-line narrative rows
**Run:** `python3 -m http.server 8783 --directory desktop/scratch/prototypes/sidebar/c-dense`
