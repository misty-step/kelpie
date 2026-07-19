<p align="center">
  <img src="assets/kelpie-logo.png" alt="kelpie" width="140">
</p>

<h1 align="center">kelpie</h1>

<p align="center">
  A phone-first console for triaging a fleet of
  <a href="https://github.com/can1357/oh-my-pi">omp</a> coding agents running in
  <a href="https://herdr.dev">herdr</a> terminal workspaces.<br>
  One hand, one thumb, whole fleet.<br>
  <a href="https://misty-step.github.io/kelpie/">misty-step.github.io/kelpie</a>
</p>

<p align="center">
  <img src="assets/screenshot-inbox-light.png" alt="Inbox, light theme" width="300">
  <img src="assets/screenshot-session-dark.png" alt="Agent session, dark theme" width="300">
</p>

## What it is

You run a herd of coding agents in terminal panes on your desk. You leave the
desk. kelpie is the pocket view: a tiny Rust bridge on the workstation plus a
self-contained Yew/WASM PWA you install to your phone's home screen.

- **Inbox** — every workspace, sorted by what needs you: pending asks first,
  then working, then idle/done, recency within each tier.
- **Agent session** — the omp transcript as chat (markdown, tool cards,
  thinking blocks), a composer with slash commands and photo attachments,
  pending-ask option buttons, model + reasoning-effort pickers.
- **Terminal** — the raw pane screen with a key row (Enter, Esc, Ctrl+C,
  arrows, Tab) for anything the chat surface can't express.

Workspaces churn constantly, so nothing is configured per workspace: identity
(icon + hue) is derived deterministically by hashing the workspace name into a
fixed vocabulary. Zero workspaces, one, or fifty all render sensibly.

## How it works

```mermaid
flowchart LR
    P[iPhone PWA<br>Yew/WASM, static/] -- "HTTP + SSE :8787" --> K[kelpie bridge<br>axum, Rust]
    K -- "NDJSON RPC over unix socket" --> H[herdr<br>workspace manager]
    K -- "session JSONL (read-only)" --> O[omp session files]
    H -- PTY --> A[agent panes]
```

- Fleet state comes from herdr's `session.snapshot`, polled every 600ms and
  diffed; changes fan out to clients over SSE.
- Transcripts come straight from omp's session JSONL files (each herdr pane
  record carries the path). No ANSI parsing.
- Input goes back through herdr `pane.send_text` / `pane.send_keys`.
- The frontend is Rust too — a Yew (WASM) app in `frontend/`, compiled with
  `./build-frontend.sh` into `static/wasm/`. The bridge serves everything with
  `Cache-Control: no-cache`, so deploys are "rebuild, restart the binary".

## Requirements

- macOS/Linux workstation running [herdr](https://herdr.dev)
  (`~/.config/herdr/herdr.sock`) with [omp](https://github.com/can1357/oh-my-pi)
  agents in its panes
- Rust toolchain (1.75+) with the `wasm32-unknown-unknown` target and
  `wasm-bindgen-cli` 0.2.100 (frontend rebuilds only)
- [Tailscale](https://tailscale.com) (or any private network) to reach your
  workstation from the phone

## Quick start

```sh
git clone https://github.com/misty-step/kelpie
cd kelpie
./build-frontend.sh   # Yew frontend -> static/wasm/; required after frontend changes
cargo run --release
# kelpie listening on http://127.0.0.1:8787 (static: static)
```

The bridge binds loopback only — it has full control of your terminal panes,
so never expose it directly. Publish it to your tailnet with:

```sh
tailscale serve --bg 8787
```

Then on the iPhone, open the tailnet URL in Safari and **Share → Add to Home
Screen**. You get a standalone app with the kelpie icon, dark/light theme
following the system, and the on-screen keyboard handled correctly (visual
viewport tracking, not scroll hacks).

## Using kelpie

1. **Triage from the inbox.** Pending asks sort first, then working panes,
   then idle/done. Tap a row to open its agent transcript.
2. **Reply from the composer.** Text is saved in browser storage on every
   keystroke, separately for each pane. It survives inbox navigation, pane
   switches, full reloads, and background SSE refreshes. Send clears that
   pane's saved draft only after the bridge confirms submission. If delivery
   crossed Enter but cannot be confirmed, Kelpie also persists that action id
   and blocks fresh sends across reloads; inspect the raw terminal and tap
   **I checked** before deliberately allowing another send.
3. **Change session controls deliberately.** Tap the model chip for a
   searchable catalog, or the effort chip for the active model's supported
   levels. Both controls are serialized per pane and disable conflicting
   composer actions until terminal readback confirms the change. Model changes
   are session-only; omp role defaults remain unchanged.
4. **Drop to Terminal when needed.** The terminal view exposes the raw screen,
   text submission, and common keys without abandoning the phone workflow.

The live transcript may refresh many times while an agent works. Kelpie keeps
rendered transcript data in place during those refreshes, so it does not
replace useful content with a transient loading screen.

### Updating a checkout

```sh
git pull
./build-frontend.sh   # only when frontend/ changed
cargo build --release
# restart the running kelpie process
```

The bridge sends `Cache-Control: no-cache` for application assets. A normal
page reload picks up a rebuilt frontend; the service-worker kill switch
removes legacy registrations and never forces an open client to reload.

### Configuration

| Env | Default | Meaning |
|---|---|---|
| `KELPIE_STATIC` | `static` | Directory of frontend assets, relative to the working directory |

The bind address (`127.0.0.1:8787`) and herdr poll interval (600ms) are
constants at the top of `src/main.rs`.

## API

Everything the frontend uses, usable from scripts too:

| Route | Purpose |
|---|---|
| `GET /api/fleet` | Workspaces, tabs, panes with status + pending-ask flags |
| `GET /api/session/{pane_id}?before={index}&limit={1..256}` | Bounded indexed transcript page (newest 160 by default), with `total_entries`, `start_index`, `has_older`, `generation`, and byte-offset `revision` |
| `GET /api/pane/{pane_id}/screen` | Plain-text visible screen of any pane |
| `GET /api/commands` | omp slash-command catalog |
| `GET /api/models` | Model catalog (`omp models --json`, cached) |
| `GET /api/events` | SSE: `fleet` and `session` change pokes |
| `POST /api/pane/{pane_id}/text` | Send a line of text (Enter appended) |
| `POST /api/pane/{pane_id}/keys` | Send named keys (`Enter`, `Escape`, `ctrl+c`, …) |
| `POST /api/pane/{pane_id}/ask` | Answer a pending single-select ask by index |
| `POST /api/pane/{pane_id}/thinking` | Select an exact supported reasoning level; success includes the confirmed level |
| `POST /api/pane/{pane_id}/model` | Select an exact model for this session; success includes the confirmed selector |
| `POST /api/pane/{pane_id}/upload` | Upload an image; returns a path omp can read |
| `POST /api/workspace`, `/api/workspace/{id}/close`, `/api/tab/{id}/close` | Create or close workspaces and tabs |
| `POST /api/tab` | Create a tab with a caller-stable `action_id`; duplicate delivery is accepted once |
| `GET /api/tab/{workspace_id}/action/{action_id}` | Read back an accepted tab-creation action after a lost response |

### Why reasoning effort "cycles"

omp's interactive TUI has no runtime *set thinking level* command — no slash
command, and its RPC/ACP setters only exist for sessions launched in those
modes. The only lever on a live pane is the `app.thinking.cycle` keybinding
(Shift+Tab). Kelpie still presents an exact picker: the bridge advances one
rendered state at a time with paced raw back-tab (`CSI Z`), reads the terminal
footer after every step, and stops only when it sees the requested level.
Controls are serialized per pane. Intermediate levels remain visible in the
desktop TUI because cycling is the underlying mechanism, but one phone tap is
enough. The picker only offers levels declared by the active model's catalog
entry.

Model selection uses omp's session-only `/switch` picker rather than `/model`
role assignment. The bridge searches by the full `provider/model` selector,
confirms omp's printed `Session-only model:` receipt, and leaves configured
role models untouched.

## Code map

```
src/
  main.rs        axum bridge: fleet poller, routes, SSE
  herdr.rs       herdr unix-socket NDJSON RPC client
  omp.rs         omp session JSONL -> transcript / summary parsing
frontend/
  src/lib.rs     Yew app shell: hash router, SSE context, toasts, viewport
  src/api.rs     typed bridge client
  src/types.rs   bridge response types
  src/icons.rs   fixed icon vocabulary + deterministic workspace identity
  src/components.rs  Header, MetaBadge, BottomSheet, TabStrip
  src/views/     inbox, session, term
static/
  index.html     shell (PWA meta, icons, wasm bootstrap)
  style.css      full design system (tokens in :root, dark via media query)
  wasm/          built frontend (build-frontend.sh output, committed)
```

`DESIGN.md` documents the visual system — tokens, type scale, status colors,
motion rules, and the accessibility contract (WCAG AA in both themes, 44px
touch targets, `prefers-reduced-motion`).

## License

[MIT](LICENSE) © Misty Step
