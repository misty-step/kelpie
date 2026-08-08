import { useEffect, useRef, useState } from "react";
import { Minus, Square, X } from "lucide-react";
import { isTauri } from "../api";
import { win } from "../windowApi";

// Plain window bar: product name on the left, drag surface across the middle,
// and — with system decorations off — the window controls on the right.
// Navigation lives in the sidebar, the chat header, and the command palette;
// herdr connection state lives in Settings.
//
// Drag regions stay on non-interactive surfaces only. Buttons never sit under
// data-tauri-drag-region / app-region:drag, or min/max/close clicks are eaten.

export function TitleBar() {
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

  const windowApi = win();

  return (
    <div className="titlebar">
      <span className="tb-title" data-tauri-drag-region>
        kelpie
      </span>
      <div className="tb-drag-fill" data-tauri-drag-region aria-hidden="true" />
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
