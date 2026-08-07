# kelpie — vision

kelpie is the operator console for a fleet of coding agents. Agents run in
[omp](https://github.com/can1357/oh-my-pi) sessions inside
[herdr](https://herdr.dev) workspaces; kelpie is where one operator sees the
whole herd, answers what needs answering, and launches the next agent — at the
desk in a desktop cockpit, away from the desk on a phone.

One herd, one console, two surfaces.

## Who it is for

One expert operator running many concurrent agents. Today that is the author;
the repo stays public-ready (clean license, honest README, reproducible build)
but nothing is built for strangers, teams, or a hosted service.

## The job

Agent work is bursty and parallel: six agents work while one waits on a
decision. The operator's scarce resource is attention, not terminal windows.
kelpie's job is to make a fleet of agents feel like one queue: what needs me
first, answer it in seconds, move on. The desktop app is the full cockpit —
chat, tools, diffs, terminal, usage, sessions, launch. The PWA is the same
console compressed for one thumb over Tailscale.

## What kelpie is

- **An agent cockpit.** Chat-first: transcripts as rendered conversations,
  pending asks as buttons, model and effort as controls. Codex desktop is the
  polish bar.
- **Nearly a full herdr GUI — for agents only.** Sessions and workspaces are
  visible and switchable; new sessions and new agents launch from kelpie.
  Launching always means launching an *agent*. kelpie never creates a raw
  shell pane: if you need a scratch terminal, you are outside the product's
  shape. (Viewing an agent's own pane as raw screen stays in scope — it is
  the fallback surface for what chat cannot express.)
- **A thin client over herdr and omp.** herdr owns workspace/session/pane
  truth; omp owns agent execution and session files. kelpie projects state and
  sends commands; it never becomes a second source of truth.
- **Two surfaces, one identity.** Desktop (Tauri + React) and phone (PWA, Yew)
  share one Rust control-plane core, one status vocabulary, one triage order.

## Standards

- **Writes are receipt-safe.** A lost response never double-sends. Control
  actions (send, answer, launch, close) carry idempotent receipts or they do
  not ship on a surface.
- **Triage speed is the metric.** Seconds from "something needs me" to
  "answered". Every design choice is judged against that loop.
- **Transcripts stay faithful.** Markdown, tool calls, diffs, thinking,
  compaction markers — rendered honestly, incrementally parsed, cheap on
  200 MB sessions.
- **Keyboard-first desk, thumb-first pocket.** Desktop: full navigation
  without the mouse. Phone: reachable one-handed.
- **Churn is normal.** Workspaces appear and die constantly; nothing is
  configured per workspace, and the UI never breaks at 0, 1, or 50 panes.

## Non-goals

- Raw terminal or shell-pane management. kelpie launches agents, not shells.
- A general herdr admin tool, tmux replacement, or IDE.
- Multi-user, accounts, cloud sync, hosted anything.
- Reimplementing omp's TUI or herdr's CLI inside kelpie.

## Bets

1. Agents-as-panes is durable: herdr stays the workspace truth and keeps the
   lifecycle contracts kelpie calls.
2. One shared Rust core (`kelpie-core`) serves both surfaces; two parsers or
   two herdr clients is drift, and drift is how fleets get answered twice.
3. Codex-desktop-grade density and polish is achievable with a small surface
   and ruthless deletion.
4. The phone surface keeps earning its place: remote triage is the feature no
   desk-bound UI absorbs.

## Rejected directions

- *Read-only triage client, lifecycle stays in herdr's own UI* — rejected by
  the operator 2026-08-07: a console that cannot launch the next agent leaves
  the operator in another tool.
- *Kill the PWA now that desktop exists* — rejected: remote, one-thumb triage
  is the only away-from-desk surface.
- *Keep desktop as a separate product/repo* — rejected: two surfaces of one
  console, one shared core, one name.

## Twelve-month excellent

kelpie is the app that is open all day. Launch an agent, triage the herd,
answer asks, watch usage, switch sessions, and leave the desk without losing
the thread. Both surfaces run one shared core; every write is receipt-safe;
the repo builds clean for a stranger; and the board reflects the work.
