import {
  Settings,
  Loader2,
} from "lucide-react";
import type { IconName } from "lucide-react/dynamic";
import type { Fleet, Pane } from "../types";
import { attentionSort } from "../fleetSort";
import type { WorkspaceIconMap } from "../workspaceIcons";
import { ThinkingIcon } from "./ThinkingIcon";
import { WorkspaceIcon } from "./WorkspaceIcon";

function StatusRing({ pane }: { pane: Pane }) {
  if (pane.pending_ask) {
    return <span className="st-ring needs pulse" title="Needs input" />;
  }
  if (pane.status === "blocked") {
    return <span className="st-ring blocked" title="Blocked" />;
  }
  if (pane.status === "working") {
    return <Loader2 size={12} className="spin" aria-hidden="true" />;
  }
  return <span className="st-ring done" title="Done" />;
}

function PaneRow({
  pane,
  selected,
  onSelect,
  workspaceLabel,
  workspaceIcon,
  onOpenWorkspaceIcon,
}: {
  pane: Pane;
  selected: boolean;
  onSelect: (paneId: string) => void;
  workspaceLabel?: string;
  workspaceIcon?: IconName;
  onOpenWorkspaceIcon: (workspaceId: string) => void;
}) {
  const wsName = workspaceLabel ?? pane.workspace_id;
  const provider = pane.provider ? pane.provider.toLowerCase() : null;
  const modelName = pane.model;
  const effort = pane.effort;
  const hasMeta = Boolean(modelName || effort);

  return (
    <div className="pane-row-shell">
      <button
        type="button"
        className={`pane-row-card${selected ? " selected" : ""}`}
        data-pane-id={pane.pane_id}
        onClick={() => onSelect(pane.pane_id)}
        title={pane.task ?? pane.pane_id}
        aria-pressed={selected}
      >
        <div className="pane-row-top">
          <span className="pane-ws-name">{wsName}</span>
          <StatusRing pane={pane} />
          {provider && <span className="pane-provider-tag">{provider}</span>}
        </div>
        {hasMeta && (
          <div className="pane-row-meta">
            {modelName && <span className="pane-model-name">{modelName}</span>}
            {effort && (
              <span className="pane-effort-pill">
                <ThinkingIcon level={effort} size={11} />
                <span>{effort.toLowerCase()}</span>
              </span>
            )}
          </div>
        )}
      </button>
      <button
        type="button"
        className="pane-workspace-icon"
        aria-label={`Change icon for ${wsName}`}
        title={`Change icon for ${wsName}`}
        onClick={() => onOpenWorkspaceIcon(pane.workspace_id)}
      >
        <WorkspaceIcon name={workspaceIcon} size={18} />
      </button>
    </div>
  );
}

export function Sidebar({
  fleet,
  selectedPaneId,
  onSelectPane,
  onOpenWorkspaceIcon,
  onOpenSettings,
  workspaceIcons,
}: {
  fleet: Fleet | null;
  selectedPaneId: string | null;
  onSelectPane: (paneId: string) => void;
  onOpenWorkspaceIcon: (workspaceId: string) => void;
  onOpenSettings: () => void;
  workspaceIcons: Readonly<WorkspaceIconMap>;
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
                onOpenWorkspaceIcon={onOpenWorkspaceIcon}
                workspaceIcon={workspaceIcons[pane.workspace_id]}
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
                onOpenWorkspaceIcon={onOpenWorkspaceIcon}
                workspaceIcon={workspaceIcons[pane.workspace_id]}
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
