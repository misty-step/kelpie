// Attention-first fleet sorting shared by the sidebar and the fleet view:
// pending ask / blocked first, then working, idle, done, unknown.

import type { Fleet, Pane } from "./types";

const RANK: Record<string, number> = { blocked: 0, working: 1, idle: 2, done: 3, unknown: 4 };

export function paneRank(pane: Pane): number {
  if (pane.pending_ask || pane.status === "blocked") return 0;
  return RANK[pane.status] ?? 4;
}

export function attentionSort(panes: Pane[]): Pane[] {
  return [...panes].sort(
    (a, b) => paneRank(a) - paneRank(b) || a.pane_id.localeCompare(b.pane_id),
  );
}

/** Same order the sidebar paints: needs-attention first, then the rest. */
export function sidebarOrder(panes: Pane[]): Pane[] {
  const sorted = attentionSort(panes);
  const urgency = sorted.filter((p) => p.pending_ask || p.status === "blocked");
  const normal = sorted.filter((p) => !p.pending_ask && p.status !== "blocked");
  return [...urgency, ...normal];
}

export function workspaceLabelOf(fleet: Fleet | null, workspaceId: string): string {
  return fleet?.workspaces.find((w) => w.id === workspaceId)?.label ?? workspaceId;
}
