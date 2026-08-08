import * as React from "react";
import { Search, X } from "lucide-react";
import { iconNames } from "lucide-react/dynamic";
import type { IconName } from "lucide-react/dynamic";
import { WorkspaceIcon } from "./WorkspaceIcon";

const MAX_VISIBLE_ICONS = 64;

function readableIconName(name: IconName): string {
  return name.replace(/-/g, " ");
}

export function WorkspaceIconPicker({
  workspaceLabel,
  selectedIcon,
  onSelect,
  onClose,
}: {
  workspaceLabel: string;
  selectedIcon: IconName | undefined;
  onSelect: (iconName: IconName) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = React.useState("");
  const inputRef = React.useRef<HTMLInputElement>(null);

  React.useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const normalizedQuery = query.trim().toLowerCase();
  const matchingIcons = iconNames.filter((name) =>
    readableIconName(name).toLowerCase().includes(normalizedQuery),
  );
  const visibleIcons = matchingIcons.slice(0, MAX_VISIBLE_ICONS);
  if (!normalizedQuery && selectedIcon && !visibleIcons.includes(selectedIcon)) {
    visibleIcons.splice(-1, 1, selectedIcon);
  }

  return (
    <div
      className="scrim workspace-icon-scrim"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section
        className="workspace-icon-picker"
        role="dialog"
        aria-modal="true"
        aria-labelledby="workspace-icon-picker-title"
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            onClose();
          }
        }}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="workspace-icon-picker-head">
          <div>
            <h2 id="workspace-icon-picker-title">Choose workspace icon</h2>
            <p>{workspaceLabel}</p>
          </div>
          <button type="button" className="icon-btn" aria-label="Close icon picker" onClick={onClose}>
            <X size={16} />
          </button>
        </header>

        <label className="workspace-icon-search-label" htmlFor="workspace-icon-search">
          Search icons
        </label>
        <div className="workspace-icon-search">
          <Search size={15} aria-hidden="true" />
          <input
            ref={inputRef}
            id="workspace-icon-search"
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search icons…"
            aria-controls="workspace-icon-options"
            autoComplete="off"
          />
        </div>
        <div
          id="workspace-icon-options"
          role="listbox"
          aria-label="Workspace icons"
          aria-multiselectable="false"
        >
          {visibleIcons.map((iconName) => (
            <button
              key={iconName}
              type="button"
              role="option"
              className={`workspace-icon-option${iconName === selectedIcon ? " selected" : ""}`}
              aria-label={`Use ${readableIconName(iconName)} icon`}
              aria-selected={iconName === selectedIcon}
              onClick={() => onSelect(iconName)}
            >
              <WorkspaceIcon name={iconName} size={20} />
              <span>{readableIconName(iconName)}</span>
            </button>
          ))}
          {visibleIcons.length === 0 && <p className="workspace-icon-empty">No matching icons</p>}
        </div>
      </section>
    </div>
  );
}

