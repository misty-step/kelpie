import { Loader2, Settings } from "lucide-react";
import type { Fleet, Pane } from "../types";
import { attentionSort, paneRank } from "../fleetSort";
import { relativeTime } from "../relativeTime";
import { taskTitle } from "../taskTitle";

function StatusChip({ pane }: { pane: Pane }) {
  if (pane.pending_ask) return <span className="chip needs">needs input</span>;
  if (pane.status === "blocked") return <span className="chip blocked">blocked</span>;
  if (pane.status === "working") {
    return (
      <span className="chip working">
        <Loader2 size={11} className="spin" aria-hidden="true" />
        working
      </span>
    );
  }
  return null;
}

function PaneRow({
  pane,
  selected,
  onSelect,
}: {
  pane: Pane;
  selected: boolean;
  onSelect: (paneId: string) => void;
}) {
  const label = taskTitle(pane.task, pane.pane_id);
  return (
    <button
      type="button"
      className={`pane-row${selected ? " selected" : ""}`}
      onClick={() => onSelect(pane.pane_id)}
      title={pane.task ?? pane.pane_id}
    >
      <span className="pane-row-label">{label}</span>
      <StatusChip pane={pane} />
      <span className="pane-row-time">{relativeTime(pane.updated_ms)}</span>
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
  const panes = fleet ? attentionSort(fleet.panes) : [];
  const needsYou = panes.filter((p) => paneRank(p) === 0);
  const rest = panes.filter((p) => paneRank(p) !== 0);

  return (
    <aside className="sidebar">
      <nav className="sidebar-list">
        {!fleet && (
          <div className="side-empty">
            <Loader2 size={14} className="spin" />
            <span>Waiting for herdr…</span>
          </div>
        )}

        {fleet && needsYou.length > 0 && (
          <section className="side-section">
            <h2 className="side-heading">Needs you</h2>
            {needsYou.map((pane) => (
              <PaneRow
                key={pane.pane_id}
                pane={pane}
                selected={pane.pane_id === selectedPaneId}
                onSelect={onSelectPane}
              />
            ))}
          </section>
        )}

        {fleet && rest.length > 0 && (
          <section className="side-section">
            <h2 className="side-heading">
              Agents
              <span className="side-count">{rest.length}</span>
            </h2>
            {rest.map((pane) => (
              <PaneRow
                key={pane.pane_id}
                pane={pane}
                selected={pane.pane_id === selectedPaneId}
                onSelect={onSelectPane}
              />
            ))}
          </section>
        )}

        {fleet && panes.length === 0 && (
          <div className="side-empty">
            <span>No agents running — open a workspace in herdr.</span>
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
