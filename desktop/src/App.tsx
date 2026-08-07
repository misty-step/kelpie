import { useEffect, useRef, useState } from "react";
import { api } from "./api";
import type { Fleet, HerdrStatus, Pane } from "./types";
import type { Theme } from "./theme";
import { useTheme } from "./theme";
import { TitleBar } from "./components/TitleBar";
import { Sidebar } from "./components/Sidebar";
import { FleetView } from "./components/FleetView";
import { Chat } from "./components/Chat";
import { SettingsView, applyOpacitySetting, getSavedOpacity } from "./components/SettingsView";
import { TerminalPanel } from "./components/TerminalPanel";
import { UsagePanel } from "./components/UsagePanel";
import { CommandPalette } from "./components/CommandPalette";
import { attentionSort, sidebarOrder } from "./fleetSort";
import { win } from "./windowApi";
import { ErrorBoundary } from "./ErrorBoundary";
import { TYPE_SCALE_DEFAULT, applyTypeScale, bumpTypeScale, getSavedTypeScale } from "./typeScale";
import { digitFromEvent, isTypingTarget, mod } from "./hotkeys";

// Hoisted so the one-shot initial auto-open runs once per session.
const fleetRef: { current: boolean } = { current: false };

function quit(): Promise<void> {
  const w = win();
  if (!w) return Promise.resolve();
  return w.close().catch(() => {});
}

type View = { kind: "fleet" } | { kind: "chat"; paneId: string } | { kind: "settings" };

export default function App() {
  const theme: Theme = useTheme();
  const [fleet, setFleet] = useState<Fleet | null>(null);
  const [herdr, setHerdr] = useState<HerdrStatus>({ ok: true, message: "connecting…" });
  const [view, setView] = useState<View>({ kind: "fleet" });
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [usageOpen, setUsageOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [, setNow] = useState(0);

  const fleetLive = useRef(fleet);
  const viewLive = useRef(view);
  const overlayLive = useRef({ palette: false, usage: false, terminal: false });
  fleetLive.current = fleet;
  viewLive.current = view;
  overlayLive.current = { palette: paletteOpen, usage: usageOpen, terminal: terminalOpen };

  const openPane = (paneId: string) => {
    setView({ kind: "chat", paneId });
    setPaletteOpen(false);
    setUsageOpen(false);
    requestAnimationFrame(() => {
      document
        .querySelector<HTMLElement>(`[data-pane-id="${CSS.escape(paneId)}"]`)
        ?.scrollIntoView({ block: "nearest" });
    });
  };

  useEffect(() => {
    // Apply saved window opacity on startup
    applyOpacitySetting(getSavedOpacity());
    applyTypeScale(getSavedTypeScale());

    let alive = true;
    const maybeAutoOpen = (f: Fleet) => {
      if (fleetRef.current || f.panes.length === 0) return;
      fleetRef.current = true;
      const top = attentionSort(f.panes)[0];
      setView({ kind: "chat", paneId: top.pane_id });
    };
    void api
      .fleet()
      .then((f) => {
        if (!alive) return;
        setFleet(f);
        maybeAutoOpen(f);
      })
      .catch(() => {});
    const offFleet = api.onFleet((f) => {
      if (!alive) return;
      setFleet(f);
      maybeAutoOpen(f);
    });
    const offStatus = api.onHerdrStatus((s) => alive && setHerdr(s));

    const orderedPanes = () => sidebarOrder(fleetLive.current?.panes ?? []);

    const stepAgent = (delta: number) => {
      const panes = orderedPanes();
      if (panes.length === 0) return;
      const cur = viewLive.current;
      const curId = cur.kind === "chat" ? cur.paneId : null;
      let idx = curId ? panes.findIndex((p) => p.pane_id === curId) : -1;
      if (idx < 0) idx = delta > 0 ? -1 : 0;
      const next = panes[(idx + delta + panes.length) % panes.length];
      openPane(next.pane_id);
    };

    const jumpAgent = (index0: number) => {
      const panes = orderedPanes();
      const pane = panes[index0];
      if (pane) openPane(pane.pane_id);
    };

    const jumpAttention = () => {
      const panes = orderedPanes();
      const needs = panes.filter((p) => p.pending_ask || p.status === "blocked");
      if (needs.length === 0) return;
      const cur = viewLive.current;
      const curId = cur.kind === "chat" ? cur.paneId : null;
      const idx = needs.findIndex((p) => p.pane_id === curId);
      const next = needs[(idx + 1) % needs.length];
      openPane(next.pane_id);
    };

    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      const key = e.key;
      const lower = key.toLowerCase();
      const typing = isTypingTarget(e.target);
      const hasMod = mod(e);

      // Always-on: palette, quit, text size, escape
      if (hasMod && !e.altKey && lower === "k") {
        e.preventDefault();
        setPaletteOpen((open) => !open);
        return;
      }
      if (hasMod && !e.altKey && lower === "q") {
        e.preventDefault();
        void quit();
        return;
      }
      if (hasMod && !e.altKey) {
        if (key === "=" || key === "+" || e.code === "NumpadAdd") {
          e.preventDefault();
          bumpTypeScale(1);
          return;
        }
        if (key === "-" || key === "_" || e.code === "NumpadSubtract") {
          e.preventDefault();
          bumpTypeScale(-1);
          return;
        }
        if (key === "0" || e.code === "Numpad0") {
          e.preventDefault();
          applyTypeScale(TYPE_SCALE_DEFAULT);
          return;
        }
      }

      if (key === "Escape") {
        if (overlayLive.current.palette) {
          e.preventDefault();
          setPaletteOpen(false);
          return;
        }
        if (overlayLive.current.usage) {
          e.preventDefault();
          setUsageOpen(false);
          return;
        }
        if (overlayLive.current.terminal) {
          e.preventDefault();
          setTerminalOpen(false);
          return;
        }
        if (!typing && viewLive.current.kind === "settings") {
          e.preventDefault();
          setView({ kind: "fleet" });
          return;
        }
        if (!typing && viewLive.current.kind === "chat") {
          e.preventDefault();
          setView({ kind: "fleet" });
          return;
        }
        return;
      }

      // Editable fields keep plain keys. Modifier chords still navigate.
      const blockPlain = typing || overlayLive.current.palette;

      // Agent navigation (sidebar order)
      // Alt/Ctrl/Cmd + ↑↓, Alt+[ ], Alt+J/K — safe while typing
      if ((e.altKey || hasMod) && key === "ArrowDown") {
        e.preventDefault();
        stepAgent(1);
        return;
      }
      if ((e.altKey || hasMod) && key === "ArrowUp") {
        e.preventDefault();
        stepAgent(-1);
        return;
      }
      if (e.altKey && !hasMod && (lower === "j" || key === "]")) {
        e.preventDefault();
        stepAgent(1);
        return;
      }
      if (e.altKey && !hasMod && (lower === "k" || key === "[")) {
        e.preventDefault();
        stepAgent(-1);
        return;
      }

      // Ctrl/Cmd+1..9 — jump to Nth agent in sidebar order
      if (hasMod && !e.altKey) {
        const digit = digitFromEvent(e);
        if (digit != null) {
          e.preventDefault();
          jumpAgent(digit - 1);
          return;
        }
      }

      // Ctrl/Cmd+Shift+A — cycle agents that need attention
      if (hasMod && e.shiftKey && lower === "a") {
        e.preventDefault();
        jumpAttention();
        return;
      }

      if (blockPlain) return;

      // Views / panels
      if (hasMod && !e.altKey && lower === "f") {
        e.preventDefault();
        setView({ kind: "fleet" });
        setPaletteOpen(false);
        return;
      }
      if (hasMod && !e.altKey && lower === ",") {
        e.preventDefault();
        setView({ kind: "settings" });
        setPaletteOpen(false);
        return;
      }
      if (hasMod && !e.altKey && lower === "u") {
        e.preventDefault();
        setUsageOpen((o) => !o);
        return;
      }
      if (hasMod && !e.altKey && lower === "`") {
        e.preventDefault();
        setTerminalOpen((o) => !o);
        return;
      }
      if (hasMod && !e.altKey && lower === "b") {
        e.preventDefault();
        setView({ kind: "fleet" });
        return;
      }
    };
    window.addEventListener("keydown", onKey);
    // Re-render every 30 s so relative-time labels stay fresh.
    const tick = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => {
      alive = false;
      offFleet();
      offStatus();
      window.clearInterval(tick);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  const selectedPane: Pane | null =
    view.kind === "chat" ? (fleet?.panes.find((p) => p.pane_id === view.paneId) ?? null) : null;

  return (
    <div className="app" data-theme={theme}>
      <TitleBar
        fleet={fleet}
        herdr={herdr}
        activeViewKind={view.kind}
        onBackToFleet={() => setView({ kind: "fleet" })}
        onOpenUsage={() => setUsageOpen(true)}
        onOpenSettings={() => setView({ kind: "settings" })}
      />
      <ErrorBoundary>
        <div className="body">
          <Sidebar
            fleet={fleet}
            selectedPaneId={view.kind === "chat" ? view.paneId : null}
            onSelectPane={(paneId) => setView({ kind: "chat", paneId })}
            onOpenUsage={() => setUsageOpen(true)}
            onOpenSettings={() => setView({ kind: "settings" })}
          />
          <main className="main">
            {view.kind === "settings" ? (
              <SettingsView herdr={herdr} onClose={() => setView({ kind: "fleet" })} />
            ) : view.kind === "chat" && selectedPane ? (
              <Chat
                pane={selectedPane}
                theme={theme}
                onToggleTerminal={() => setTerminalOpen((o) => !o)}
                terminalOpen={terminalOpen}
              />
            ) : (
              <FleetView fleet={fleet} onSelectPane={(paneId) => setView({ kind: "chat", paneId })} />
            )}
          </main>
          {terminalOpen && selectedPane && view.kind === "chat" && (
            <TerminalPanel pane={selectedPane} onClose={() => setTerminalOpen(false)} />
          )}
        </div>
      </ErrorBoundary>
      {usageOpen && <UsagePanel onClose={() => setUsageOpen(false)} />}
      {paletteOpen && (
        <CommandPalette
          fleet={fleet}
          onClose={() => setPaletteOpen(false)}
          onSelectPane={(paneId) => setView({ kind: "chat", paneId })}
          onOpenUsage={() => setUsageOpen(true)}
          onOpenSettings={() => setView({ kind: "settings" })}
          onToggleTerminal={() => setTerminalOpen((open) => !open)}
          onBackToFleet={() => setView({ kind: "fleet" })}
        />
      )}
    </div>
  );
}
