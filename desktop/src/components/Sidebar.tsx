import {
  Settings,
  Loader2,
} from "lucide-react";
import type { Fleet, Pane } from "../types";
import { attentionSort } from "../fleetSort";
import { relativeTime } from "../relativeTime";

function ProviderIcon({ provider, model }: { provider?: string | null; model?: string | null }) {
  const p = (provider ?? model ?? "").toLowerCase();
  if (!p) return null;
  if (p.includes("anthropic") || p.includes("claude")) {
    return (
      <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" className="prov-icon" aria-hidden="true">
        <path d="M13.8 3h-3.6L5 21h3.6l1.6-4.8h4.6l1.6 4.8h3.6L13.8 3zm-2.4 10.5 1.5-4.5 1.5 4.5h-3z" />
      </svg>
    );
  }
  if (p.includes("google") || p.includes("gemini")) {
    return (
      <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" className="prov-icon" aria-hidden="true">
        <path d="M12 2L9.5 9.5 2 12l7.5 2.5L12 22l2.5-7.5L22 12l-7.5-2.5L12 2z" />
      </svg>
    );
  }
  if (p.includes("openai") || p.includes("codex") || p.includes("gpt")) {
    return (
      <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" className="prov-icon" aria-hidden="true">
        <path d="M22.28 9.87a5.98 5.98 0 0 0-.52-4.93 6.03 6.03 0 0 0-6.47-2.9 6.03 6.03 0 0 0-4.63-2.03 6.03 6.03 0 0 0-5.74 4.16 6.03 6.03 0 0 0-4.14 2.99 6.01 6.01 0 0 0 .73 6.99 5.98 5.98 0 0 0 .52 4.93 6.03 6.03 0 0 0 6.47 2.9 6.03 6.03 0 0 0 4.63 2.03 6.03 6.03 0 0 0 5.74-4.16 6.03 6.03 0 0 0 4.14-2.99 6.01 6.01 0 0 0-.73-6.99zM12 15.6a3.6 3.6 0 1 1 0-7.2 3.6 3.6 0 0 1 0 7.2z" />
      </svg>
    );
  }
  return null;
}

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
  workspaceLabel,
}: {
  pane: Pane;
  selected: boolean;
  onSelect: (paneId: string) => void;
  workspaceLabel?: string;
}) {
  const time = relativeTime(pane.updated_ms);
  const wsName = workspaceLabel ?? pane.workspace_id;
  const provider = pane.provider;
  const modelName = pane.model;
  const effort = pane.effort;

  const hasMeta = Boolean(provider || modelName || effort);

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
        <span className="pane-row-time">{time}</span>
      </div>
      {hasMeta && (
        <div className="pane-row-meta">
          <ProviderIcon provider={provider} model={modelName} />
          {modelName && <span className="pane-model-name">{modelName}</span>}
          {effort && <span className="pane-effort-pill">{effort}</span>}
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
