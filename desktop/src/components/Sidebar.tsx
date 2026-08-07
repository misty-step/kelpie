import { useState } from "react";
import {
  FolderGit2,
  ChevronDown,
  ChevronRight,
  Settings,
  Bot,
  AlertOctagon,
  Loader2,
} from "lucide-react";
import type { Fleet, Pane } from "../types";
import { attentionSort, paneRank } from "../fleetSort";
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
    return <span className="st-ring working spin" title="Working" />;
  }
  return <span className="st-ring done" title="Done" />;
}

function PaneRow({
  pane,
  selected,
  onSelect,
  showWorkspace,
  workspaceLabel,
}: {
  pane: Pane;
  selected: boolean;
  onSelect: (paneId: string) => void;
  showWorkspace?: boolean;
  workspaceLabel?: string;
}) {
  const label = taskTitle(pane.task, pane.pane_id);
  const time = relativeTime(pane.updated_ms);

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
      {showWorkspace && (
        <span className="pane-ws-tag">{workspaceLabel ?? pane.workspace_id}</span>
      )}
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
  const [collapsedWs, setCollapsedWs] = useState<Record<string, boolean>>({});

  const toggleWorkspace = (wsId: string) => {
    setCollapsedWs((prev) => ({ ...prev, [wsId]: !prev[wsId] }));
  };

  const totalPanes = fleet?.panes.length ?? 0;
  const urgencyPanes = fleet ? fleet.panes.filter((p) => p.pending_ask || p.status === "blocked") : [];
  const wsLabelMap = new Map(fleet?.workspaces.map((w) => [w.id, w.label]) ?? []);
  const groupedWorkspaces = (() => {
    if (!fleet) return [];
    const map = new Map<string, Pane[]>();
    for (const pane of fleet.panes) {
      const list = map.get(pane.workspace_id) ?? [];
      list.push(pane);
      map.set(pane.workspace_id, list);
    }
    return fleet.workspaces
      .filter((ws) => map.has(ws.id))
      .map((ws) => ({
        ws,
        panes: attentionSort(map.get(ws.id) ?? []),
        minRank: Math.min(...(map.get(ws.id) ?? []).map(paneRank)),
      }))
      .sort((a, b) => a.minRank - b.minRank || a.ws.label.localeCompare(b.ws.label));
  })();

  return (
    <aside className="sidebar">
      <div className="sidebar-top-strip">
        <span className="sidebar-title">
          <Bot size={14} className="sidebar-icon" />
          <span>Workspaces</span>
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

        {fleet && groupedWorkspaces.length === 0 && (
          <div className="side-empty">
            <span>No agents running — open a workspace in herdr.</span>
          </div>
        )}

        {fleet && urgencyPanes.length > 0 && (
          <div className="urgency-sec">
            <div className="urgency-h">
              <AlertOctagon size={12} />
              <span>Needs Attention</span>
              <span className="urgency-count">{urgencyPanes.length}</span>
            </div>
            {urgencyPanes.map((pane) => (
              <PaneRow
                key={`urgency-${pane.pane_id}`}
                pane={pane}
                selected={pane.pane_id === selectedPaneId}
                onSelect={onSelectPane}
                showWorkspace
                workspaceLabel={wsLabelMap.get(pane.workspace_id) ?? pane.workspace_id}
              />
            ))}
          </div>
        )}

        {groupedWorkspaces.map(({ ws, panes, minRank }) => {
          const isCollapsed = collapsedWs[ws.id] ?? false;
          const hasNeeds = minRank === 0;

          return (
            <section key={ws.id} className={`side-ws-group${hasNeeds ? " has-needs" : ""}`}>
              <button
                type="button"
                className="side-ws-header"
                onClick={() => toggleWorkspace(ws.id)}
                aria-expanded={!isCollapsed}
              >
                <span className="side-ws-toggle">
                  {isCollapsed ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
                </span>
                <FolderGit2 size={13} className="side-ws-icon" />
                <span className="side-ws-label">{ws.label}</span>
                <span className="side-ws-count">{panes.length}</span>
              </button>

              {!isCollapsed && (
                <div className="side-ws-body">
                  {panes.map((pane) => (
                    <PaneRow
                      key={pane.pane_id}
                      pane={pane}
                      selected={pane.pane_id === selectedPaneId}
                      onSelect={onSelectPane}
                    />
                  ))}
                </div>
              )}
            </section>
          );
        })}
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
