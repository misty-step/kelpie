import { useEffect, useRef, useState } from "react";
import { Gauge, Search, Terminal as TerminalIcon, LayoutGrid, ArrowRight, Settings } from "lucide-react";
import type { Fleet } from "../types";
import { statusLabel } from "../status";
import { workspaceLabelOf } from "../fleetSort";
import { taskTitle } from "../taskTitle";

interface PaletteItem {
  id: string;
  label: string;
  sub: string;
  icon: React.ReactNode;
  run: () => void;
}

export function CommandPalette({
  fleet,
  onClose,
  onSelectPane,
  onOpenUsage,
  onOpenSettings,
  onToggleTerminal,
  onBackToFleet,
}: {
  fleet: Fleet | null;
  onClose: () => void;
  onSelectPane: (paneId: string) => void;
  onOpenUsage: () => void;
  onOpenSettings: () => void;
  onToggleTerminal: () => void;
  onBackToFleet: () => void;
}) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const items: PaletteItem[] = (() => {
    const q = query.trim().toLowerCase();
    const actions: PaletteItem[] = [
      {
        id: "settings",
        label: "Open settings",
        sub: "Application preferences and window opacity",
        icon: <Settings size={14} />,
        run: onOpenSettings,
      },
      {
        id: "usage",
        label: "Open usage",
        sub: "Provider limits and resets",
        icon: <Gauge size={14} />,
        run: onOpenUsage,
      },
      {
        id: "terminal",
        label: "Toggle terminal",
        sub: "Show or hide the current chat terminal",
        icon: <TerminalIcon size={14} />,
        run: onToggleTerminal,
      },
      {
        id: "fleet",
        label: "Back to fleet",
        sub: "Triage inbox",
        icon: <LayoutGrid size={14} />,
        run: onBackToFleet,
      },
    ];
    const panes: PaletteItem[] = (fleet?.panes ?? []).map((pane) => ({
      id: pane.pane_id,
      label: taskTitle(pane.task, pane.pane_id),
      sub: `${workspaceLabelOf(fleet, pane.workspace_id)} · ${statusLabel(pane.status)}`,
      icon: <ArrowRight size={14} />,
      run: () => onSelectPane(pane.pane_id),
    }));
    const all = [...actions, ...panes];
    if (!q) return all;
    return all.filter(
      (item) => item.label.toLowerCase().includes(q) || item.sub.toLowerCase().includes(q),
    );
  })();

  useEffect(() => {
    inputRef.current?.focus();
    setIndex(0);
  }, [query]);

  const run = (item: PaletteItem | undefined) => {
    if (!item) return;
    onClose();
    item.run();
  };

  return (
    <div className="scrim" onClick={onClose}>
      <div className="palette" onClick={(e) => e.stopPropagation()}>
        <div className="palette-search">
          <Search size={16} className="palette-search-icon" />
          <input
            ref={inputRef}
            type="text"
            className="palette-input"
            placeholder="Search agents or actions…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setIndex((i) => (items.length ? (i + 1) % items.length : 0));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setIndex((i) => (items.length ? (i - 1 + items.length) % items.length : 0));
              } else if (e.key === "Enter") {
                e.preventDefault();
                run(items[index]);
              }
            }}
          />
        </div>
        <div className="palette-list">
          {items.map((item, i) => (
            <button
              key={item.id}
              type="button"
              className={`palette-item${i === index ? " selected" : ""}`}
              onClick={() => run(item)}
              onMouseEnter={() => setIndex(i)}
            >
              <span className="palette-item-icon">{item.icon}</span>
              <span className="palette-item-text">
                <span className="palette-item-label">{item.label}</span>
                <span className="palette-item-sub">{item.sub}</span>
              </span>
            </button>
          ))}
          {items.length === 0 && <div className="palette-empty">No matching commands</div>}
        </div>
      </div>
    </div>
  );
}
