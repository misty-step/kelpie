import {
  Settings,
  Loader2,
  Flame,
  CheckCircle2,
  CircleDot,
  Circle,
} from "lucide-react";
import type { Fleet, Pane } from "../types";
import { attentionSort } from "../fleetSort";

function EffortIcon({ effort }: { effort: string }) {
  const e = effort.toLowerCase();
  if (e.includes("max") || e.includes("extra high") || e.includes("xhigh")) {
    return <Flame size={11} className="pane-effort-ico flame" aria-hidden="true" />;
  }
  if (e.includes("high")) {
    return <CheckCircle2 size={11} className="pane-effort-ico high" aria-hidden="true" />;
  }
  if (e.includes("medium")) {
    return <CircleDot size={11} className="pane-effort-ico medium" aria-hidden="true" />;
  }
  return <Circle size={11} className="pane-effort-ico low" aria-hidden="true" />;
}

function WorkingSpinner() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" className="spin st-spinner" aria-hidden="true">
      <circle cx="8" cy="8" r="6" fill="none" stroke="var(--brand-soft)" strokeWidth="2" opacity="0.35" />
      <path d="M14 8a6 6 0 0 0-6-6" fill="none" stroke="var(--brand)" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

function StatusRing({ pane }: { pane: Pane }) {
  if (pane.pending_ask) {
    return <span className="st-ring needs pulse" title="Needs input" />;
  }
  if (pane.status === "blocked") {
    return <span className="st-ring blocked" title="Blocked" />;
  }
  if (pane.status === "working") {
    return <WorkingSpinner />;
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
  const wsName = workspaceLabel ?? pane.workspace_id;
  const provider = pane.provider ? pane.provider.toLowerCase() : null;
  const modelName = pane.model;
  const effort = pane.effort;

  const hasMeta = Boolean(modelName || effort);

  return (
    <button
      type="button"
      className={`pane-row-card${selected ? " selected" : ""}`}
      onClick={() => onSelect(pane.pane_id)}
      title={pane.task ?? pane.pane_id}
      aria-pressed={selected}
    >
      <div className="pane-row-top">
        <StatusRing pane={pane} />
        <span className="pane-ws-name">{wsName}</span>
        {provider && <span className="pane-provider-tag">{provider}</span>}
      </div>
      {hasMeta && (
        <div className="pane-row-meta">
          {modelName && <span className="pane-model-name">{modelName}</span>}
          {effort && (
            <span className="pane-effort-pill">
              <EffortIcon effort={effort} />
              <span>{effort}</span>
            </span>
          )}
        </div>
      )}
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
