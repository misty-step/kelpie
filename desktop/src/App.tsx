import { useEffect, useState } from "react";
import { api } from "./api";
import type { Fleet, HerdrStatus, Pane, Workspace } from "./types";
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
import { attentionSort } from "./fleetSort";
import { win } from "./windowApi";
import { ErrorBoundary } from "./ErrorBoundary";

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

  useEffect(() => {
    // Apply saved window opacity on startup
    applyOpacitySetting(getSavedOpacity());

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
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((open) => !open);
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "q") {
        e.preventDefault();
        void quit();
      }
      if (e.key === "Escape") {
        setPaletteOpen(false);
        setUsageOpen(false);
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
  const workspaceOf = (pane: Pane): Workspace | undefined =>
    fleet?.workspaces.find((w) => w.id === pane.workspace_id);

  return (
    <div className="app" data-theme={theme}>
      <TitleBar
        fleet={fleet}
        herdr={herdr}
        activeWorkspace={
          view.kind === "chat" && selectedPane ? workspaceOf(selectedPane)?.label ?? null : null
        }
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
                workspace={workspaceOf(selectedPane) ?? null}
                theme={theme}
                model={null}
                thinking={null}
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
