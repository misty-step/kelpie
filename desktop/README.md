# Kelpie desktop (prototype)

> Project direction and surface boundaries live in [../VISION.md](../VISION.md).

A Tauri 2 + React desktop console for a fleet of omp agents running in herdr
workspaces — the reimagined kelpie: fleet sidebar, rich chat (markdown,
mermaid, tool cards, thinking blocks, pending asks), a usage panel, an
embedded terminal view, and a command palette.

**Prototype scope.** The Rust side talks directly to the herdr socket and omp
session files (in-process control plane, no bridge process). Control actions
are the simple paths: send text, send keys, answer single-select asks via the
TUI picker. The receipt-verified drivers from the phone bridge (double-send
protection, reasoning-effort cycling, session-only model switching) are not
ported yet — that is the next step, reusing `src/main.rs` logic.

## Run

System deps (once):

```sh
sudo apt install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

Then:

```sh
cd desktop
npm install
npm run tauri dev        # desktop window against live herdr + omp
```

Hot keys: `Ctrl/Cmd+K` command palette, `Esc` closes overlays.

### Install as the `kelpie` command

```sh
cd desktop
npm run install-local    # build → ~/.local/bin/kelpie → relaunch if running
```

`kelpie` on the workstation is this installed release binary, so a source edit
is invisible until it is reinstalled. Commits that touch desktop sources do it
automatically through `scripts/git-hooks/post-commit`; enable the hooks once per
clone with `git config core.hooksPath scripts/git-hooks`. Pass `--no-relaunch`
to leave a running app alone (`npm run install-local -- --no-relaunch`).

### Browser preview (no desktop shell)

```sh
cd desktop
npm run dev              # http://localhost:1420 with fixture data
```

The preview backend reads `public/fixtures/` (real fleet + usage snapshots,
plus a demo session with mermaid). It is dev tooling only: the preview path
is gated behind `import.meta.env.DEV`, and the Tauri build uses the real
backend exclusively.

## Layout

```
desktop/
  src/               React frontend (Vite + TS)
    api.ts           Tauri commands/events, browser-preview fallback
    markdown.tsx     block walker: code, tables, mermaid; inline via DOMPurify
    components/      Sidebar, FleetView, Chat, Composer, UsagePanel,
                     TerminalPanel, CommandPalette
    styles.css       codex-desktop-aligned design tokens + layout
  src-tauri/         Rust control plane
    src/herdr.rs     herdr unix-socket NDJSON client
    src/omp.rs       incremental session-JSONL projection + tail summary
    src/lib.rs       commands, poll loop (600 ms), fleet build, ask driver
    src/state.rs     fleet state + bounded projection cache (6)
  public/fixtures/   preview data (live snapshots + demo session)
```

## Design

The UI is modeled on codex-desktop (see `~/Pictures/References/codex-desktop.webp`): a
36 px toolbar strip that doubles as the window drag region, a `#f3f3f3` sidebar
with flat agent rows (title + status chip + relative age), a white chat column
with monospace tool rows, `Reasoning` collapsible blocks, file-change style
cards, and a composer with a model/thinking/workspace footer row. System
decorations are off — the toolbar's right edge carries minimize/maximize/close;
`Ctrl/Cmd+Q` also quits. Status chips reuse semantic color + text.

Dark mode follows the same structure with inverted surfaces.

## Behavior notes

- Fleet poll every 600 ms; `fleet`/`poke`/`herdr-status` events drive the UI.
- Transcripts refresh incrementally — only appended bytes are parsed, so
  200 MB sessions stay cheap. First open pays one full parse (~1–2 s).
- Terminal panel shows the pane's visible screen text, refreshed every 1.5 s —
  a snapshot, not a live stream.
- Usage runs `omp usage --json --redact`.
- The omp parser and ask-picker heuristics mirror the bridge's `src/omp.rs`
  and `src/main.rs`; when the bridge moves to a `kelpie-core` lib, the
  desktop should consume it instead of keeping this parallel copy.