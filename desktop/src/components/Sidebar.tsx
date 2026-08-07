import {
  Settings,
  Bot,
  Loader2,
} from "lucide-react";
import type { Fleet, Pane } from "../types";
import { attentionSort } from "../fleetSort";
import { relativeTime } from "../relativeTime";
import { taskTitle } from "../taskTitle";

function StatusRing({ pane }: { pane: Pane }) {
  if (pane.pending_ask) {
    return <span className="st-ring needs pulse" title="Needs input" />;
  }
  if (pane.status === "blocked") {
    return <span className="st-ring blocked" title="Blocked" />;
  }
  if (pane.status === "working") {
    return <span className="st-ring working" title="Working" />;
  }
  return <span className="st-ring done" title="Done" />;
}

function PaneRow({
  pane,
  selected,
  onSelect,
  workspaceLabel,
}: {
  pane: Pane;
  selected: boolean;
  onSelect: (paneId: string) => void;
  workspaceLabel?: string;
}) {
  const label = taskTitle(pane.task, pane.pane_id);
  const time = relativeTime(pane.updated_ms);
  const wsTag = workspaceLabel ?? pane.workspace_id;

  return (
    <button
      type="button"
      className={`pane-row-card${selected ? " selected" : ""}`}
      onClick={() => onSelect(pane.pane_id)}
      title={pane.task ?? pane.pane_id}
      aria-pressed={selected}
    >
      <StatusRing pane={pane} />
      <span className="pane-row-title">{label}</span>
      <span className="pane-ws-tag">{wsTag}</span>
      <span className="pane-row-time">{time}</span>
    </button>
  );
}

export function Sidebar({
  fleet,
  selectedPaneId,
  onSelectPane,
  onOpenSettings,
}: {
  fleet: Fleet | null;
  selectedPaneId: string | null;
  onSelectPane: (paneId: string) => void;
  onOpenUsage?: () => void;
  onOpenSettings: () => void;
}) {
  const totalPanes = fleet?.panes.length ?? 0;
  const wsLabelMap = new Map(fleet?.workspaces.map((w) => [w.id, w.label]) ?? []);

  const sortedPanes = fleet ? attentionSort(fleet.panes) : [];
  const urgencyPanes = sortedPanes.filter((p) => p.pending_ask || p.status === "blocked");
  const normalPanes = sortedPanes.filter((p) => !p.pending_ask && p.status !== "blocked");

  return (
    <aside className="sidebar">
      <div className="sidebar-top-strip">
        <span className="sidebar-title">
          <Bot size={14} className="sidebar-icon" />
          <span>Agents</span>
        </span>
        <span className="sidebar-meta">
          {totalPanes} {totalPanes === 1 ? "agent" : "agents"}
        </span>
      </div>

      <nav className="sidebar-list">
        {!fleet && (
          <div className="side-empty">
            <Loader2 size={14} className="spin" />
            <span>Waiting for herdr…</span>
          </div>
        )}

        {fleet && totalPanes === 0 && (
          <div className="side-empty">
            <span>No agents running — open a workspace in herdr.</span>
          </div>
        )}

        {fleet && urgencyPanes.length > 0 && (
          <div className="urgency-sec">
            <div className="urgency-h">
              <span>Needs Attention</span>
              <span className="urgency-count">{urgencyPanes.length}</span>
            </div>
            {urgencyPanes.map((pane) => (
              <PaneRow
                key={`urgency-${pane.pane_id}`}
                pane={pane}
                selected={pane.pane_id === selectedPaneId}
                onSelect={onSelectPane}
                workspaceLabel={wsLabelMap.get(pane.workspace_id)}
              />
            ))}
          </div>
        )}

        {fleet && normalPanes.length > 0 && (
          <div className="agents-sec">
            {urgencyPanes.length > 0 && (
              <div className="agents-h">
                <span>Agents</span>
              </div>
            )}
            {normalPanes.map((pane) => (
              <PaneRow
                key={pane.pane_id}
                pane={pane}
                selected={pane.pane_id === selectedPaneId}
                onSelect={onSelectPane}
                workspaceLabel={wsLabelMap.get(pane.workspace_id)}
              />
            ))}
          </div>
        )}
      </nav>

      <footer className="sidebar-foot">
        <button type="button" className="side-settings" onClick={onOpenSettings}>
          <Settings size={14} />
          <span>Settings</span>
        </button>
      </footer>
    </aside>
  );
}
