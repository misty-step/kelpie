import { Loader2, MessageSquareText } from "lucide-react";
import type { Fleet, Pane } from "../types";
import { attentionSort, workspaceLabelOf } from "../fleetSort";
import { relativeTime } from "../relativeTime";
import { statusLabel } from "../status";
import { taskTitle } from "../taskTitle";

function FleetCard({
  pane,
  workspaceLabel,
  onSelect,
}: {
  pane: Pane;
  workspaceLabel: string;
  onSelect: (paneId: string) => void;
}) {
  const label = taskTitle(pane.task, pane.pane_id);
  return (
    <button className="fleet-card" onClick={() => onSelect(pane.pane_id)}>
      <div className="fleet-card-body">
        <div className="fleet-card-title">{label}</div>
        <div className="fleet-card-meta">
          <span className="fleet-card-ws">{workspaceLabel}</span>
          <span className="fleet-card-time">{relativeTime(pane.updated_ms)}</span>
          {pane.snippet && <span className="fleet-card-snippet">{pane.snippet}</span>}
        </div>
      </div>
      <div className="fleet-card-side">
        <span className={`chip ${pane.status}`}>
          {pane.status === "working" && (
            <Loader2 size={11} className="spin" aria-hidden="true" />
          )}
          {statusLabel(pane.status)}
        </span>
        {pane.pending_ask && <span className="chip blocked">needs input</span>}
      </div>
    </button>
  );
}

export function FleetView({
  fleet,
  onSelectPane,
}: {
  fleet: Fleet | null;
  onSelectPane: (paneId: string) => void;
}) {
  const sorted = fleet ? attentionSort(fleet.panes) : [];

  return (
    <div className="fleet-view">
      <header className="surface-head">
        <h1>Fleet</h1>
        <span className="surface-sub">
          {fleet ? `${fleet.panes.length} agents across ${fleet.workspaces.length} workspaces` : ""}
        </span>
      </header>
      {fleet && sorted.length === 0 && (
        <div className="empty-state">
          <MessageSquareText size={28} />
          <p>No agents running.</p>
          <p className="empty-sub">Open a workspace in herdr and they will appear here.</p>
        </div>
      )}
      {!fleet && <div className="empty-state">Connecting to herdr…</div>}
      <div className="fleet-list">
        {sorted.map((pane) => (
          <FleetCard
            key={pane.pane_id}
            pane={pane}
            workspaceLabel={workspaceLabelOf(fleet, pane.workspace_id)}
            onSelect={onSelectPane}
          />
        ))}
      </div>
    </div>
  );
}
