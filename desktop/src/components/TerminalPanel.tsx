import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { CornerDownLeft, Square, Terminal as TerminalIcon, X } from "lucide-react";
import { api } from "../api";
import type { Pane } from "../types";

export function TerminalPanel({ pane, onClose }: { pane: Pane; onClose: () => void }) {
  const [screen, setScreen] = useState("");
  const [error, setError] = useState<string | null>(null);
  const preRef = useRef<HTMLPreElement>(null);
  const stickRef = useRef(true);

  useEffect(() => {
    let alive = true;
    const tick = async () => {
      if (!alive) return;
      try {
        const next = await api.readScreen(pane.pane_id);
        if (alive) {
          setScreen(next);
          setError(null);
        }
      } catch (e) {
        if (alive) setError(String(e));
      }
    };
    void tick();
    const id = window.setInterval(tick, 1500);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, [pane.pane_id]);

  const onScroll = () => {
    const el = preRef.current;
    if (!el) return;
    stickRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
  };
  useLayoutEffect(() => {
    const el = preRef.current;
    if (el && stickRef.current) el.scrollTop = el.scrollHeight;
  }, [screen]);

  return (
    <aside className="term-panel">
      <header className="term-head">
        <span className="term-title">
          <TerminalIcon size={13} />
          {pane.pane_id}
        </span>
        <button className="icon-btn" title="Interrupt (Ctrl+C)" onClick={() => void api.sendKeys(pane.pane_id, ["ctrl+c"])}>
          <Square size={12} />
        </button>
        <button className="icon-btn" title="Enter" onClick={() => void api.sendKeys(pane.pane_id, ["Enter"])}>
          <CornerDownLeft size={12} />
        </button>
        <button className="icon-btn" onClick={onClose} title="Close panel">
          <X size={14} />
        </button>
      </header>
      <div className="term-body">
        {error && <div className="term-error">{error}</div>}
        <pre className="term-screen" ref={preRef} onScroll={onScroll}>
          {screen}
        </pre>
      </div>
      <footer className="term-foot">
        <span>Snapshot refreshed every 1.5s — drive the pane from herdr directly</span>
      </footer>
    </aside>
  );
}
