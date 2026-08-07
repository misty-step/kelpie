# Sidebar — confirmed brief (operator: generate options to explore)

**Question:** How should the Kelpie sidebar present workspace hierarchy, alignment, and working directory?

**Route:** UI / product direction · **N = 3**

## Audience / primary task
Operator triaging OMP agents. Scan who needs attention, which workspace/cwd an agent is in, and select a pane fast.

## Host
Desktop Kelpie left rail (`desktop/src/components/Sidebar.tsx`), ~292–320px. Scratch only under `desktop/scratch/prototypes/sidebar/`.

## Locked product rules
1. **Group by workspace**; attention-sort panes *inside* each workspace.
2. Surface **working directory** somehow (each direction may place it differently).
3. Keep **status** (needs input / working spinner / blocked) and **recency** scannable.
4. Codex light tokens; desktop-first; keyboard-reachable rows; fixture data only.
5. No production file edits in lanes.

## Fixture content (shared)
Workspaces: olympus, habitat, time-tracker, canary, cantrip, mint, iron-forest, kelpie, bluetooth-demo  
Panes: mix of needs-input, working, idle/done with tasks + cwd like `/home/phaedrus/Development/demo/wA`  
Show short path form in UI (e.g. `demo/wA` or `~/Development/demo/wA`).

## States
Empty fleet · loading · mixed urgency · selected row · long task title wrap/truncate

## Must decide (via visual compare)
Row anatomy, cwd placement, section header design, alignment grid, density.

## Out of scope
Mobile drawer, drag-reorder, multi-select, renaming workspaces, production API changes.

## Directions
| ID | Lens |
|----|------|
| A | Workflow / hierarchy |
| B | Identity / narrative |
| C | Dense ops / spatial scanning |
