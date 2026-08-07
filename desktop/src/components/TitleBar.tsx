import { useEffect, useRef, useState } from "react";
import { Gauge, LayoutGrid, Minus, Settings, Square, X } from "lucide-react";
import { isTauri } from "../api";
import { win } from "../windowApi";
import { paneRank } from "../fleetSort";
import type { Fleet, HerdrStatus } from "../types";

// Top strip: drag region, product mark, primary nav, herdr status, and —
// with system decorations off — the window controls. Terminal toggle lives
// only on the chat header (one place, one meaning).
//
// Drag regions stay on non-interactive surfaces only. Buttons never sit under
// data-tauri-drag-region / app-region:drag, or min/max/close clicks are eaten.

export function TitleBar({
  fleet,
  herdr,
  onBackToFleet,
  onOpenUsage,
  onOpenSettings,
  activeViewKind,
}: {
  fleet: Fleet | null;
  herdr: HerdrStatus;
  onBackToFleet: () => void;
  onOpenUsage: () => void;
  onOpenSettings: () => void;
  activeViewKind?: string;
}) {
  const [maximized, setMaximized] = useState(false);
  const offsRef = useRef<Array<() => void>>([]);

  useEffect(() => {
    if (!isTauri) return;
    let alive = true;
    const offs = offsRef.current;
    void import("@tauri-apps/api/window")
      .then((m) => m.getCurrentWindow())
      .then(async (w) => {
        if (!alive) return;
        const update = async () => {
          if (alive) setMaximized(await w.isMaximized());
        };
        void update();
        const off1 = await w.onResized(() => void update());
        if (!alive) {
          off1();
          return;
        }
        const off2 = await w.onMoved(() => void update());
        if (!alive) {
          off1();
          off2();
          return;
        }
        offs.push(off1, off2);
      })
      .catch(() => {});
    return () => {
      alive = false;
      for (const off of offs) off();
    };
  }, []);

  const count = fleet?.panes.length ?? 0;
  const needs = fleet?.panes.filter((p) => paneRank(p) === 0).length ?? 0;
  const windowApi = win();

  return (
    <div className="titlebar">
      <div className="tb-brand" data-tauri-drag-region>
        <span className="tb-wordmark">kelpie</span>
      </div>

      <div className="tb-actions">
        <button
          type="button"
          className={`tb-btn ${activeViewKind === "fleet" ? "active" : ""}`}
          onClick={onBackToFleet}
          title="Back to fleet"
        >
          <LayoutGrid size={14} />
          <span>Fleet{count ? ` ${count}` : ""}</span>
          {needs > 0 && <span className="tb-badge">{needs}</span>}
        </button>

        <button type="button" className="tb-btn" onClick={onOpenUsage} title="Provider usage">
          <Gauge size={14} />
          <span>Usage</span>
        </button>
        <button
          type="button"
          className={`tb-btn ${activeViewKind === "settings" ? "active" : ""}`}
          onClick={onOpenSettings}
          title="Application settings"
        >
          <Settings size={14} />
          <span>Settings</span>
        </button>
        <div className="tb-drag-fill" data-tauri-drag-region aria-hidden="true" />
      </div>

      <div className="tb-right">
        <span
          className={`tb-herdr ${herdr.ok ? "ok" : "down"}`}
          title={herdr.message}
          data-tauri-drag-region
        >
          <span className={`status-dot tiny ${herdr.ok ? "working" : "blocked"}`} />
          {herdr.ok ? "connected" : "disconnected"}
        </span>
      </div>

      {windowApi && (
        <div className="tb-windows">
          <button
            type="button"
            className="tb-win"
            title="Minimize"
            onClick={() => void windowApi.minimize()}
          >
            <Minus size={14} />
          </button>
          <button
            type="button"
            className="tb-win"
            title={maximized ? "Restore" : "Maximize"}
            onClick={() => void windowApi.toggleMaximize()}
          >
            {maximized ? <span className="win-restore" /> : <Square size={12} />}
          </button>
          <button
            type="button"
            className="tb-win close"
            title="Close"
            onClick={() => void windowApi.close()}
          >
            <X size={15} />
          </button>
        </div>
      )}
    </div>
  );
}
