import { useCallback, useEffect, useRef, useState } from "react";
import {
  Braces,
  Check,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  FileText,
  Globe,
  HelpCircle,
  ListChecks,
  Loader2,
  PenLine,
  Search,
  Square,
  Terminal as TerminalIcon,
  Wrench,
} from "lucide-react";
import { api } from "../api";
import { Markdown } from "../markdown";
import type { Ask, IndexedEntry, Pane, SessionPage, Workspace } from "../types";
import type { Theme } from "../theme";
import { Composer } from "./Composer";
import { statusLabel } from "../status";
import { taskTitle } from "../taskTitle";

const TOOL_ICONS: Record<string, typeof Wrench> = {
  bash: TerminalIcon,
  exec: TerminalIcon,
  run: TerminalIcon,
  read: FileText,
  write: PenLine,
  edit: PenLine,
  apply_patch: PenLine,
  glob: Search,
  grep: Search,
  search: Search,
  web_search: Globe,
  web_fetch: Globe,
  ask: HelpCircle,
  task: ListChecks,
  todo: ListChecks,
  lsp: Braces,
};

function toolIcon(name: string) {
  return TOOL_ICONS[name] ?? Wrench;
}

function timeOf(ts?: string | null): string {
  if (!ts) return "";
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function ToolStatus({ status }: { status: "pending" | "ok" | "error" }) {
  if (status === "pending") {
    return (
      <span className="tool-status pending">
        <Loader2 size={11} className="spin" />
        running
      </span>
    );
  }
  if (status === "error") {
    return (
      <span className="tool-status error">
        <CircleAlert size={11} />
        failed
      </span>
    );
  }
  return (
    <span className="tool-status ok">
      <Check size={11} />
      done
    </span>
  );
}

function ToolCard({ entry }: { entry: IndexedEntry }) {
  const [open, setOpen] = useState(false);
  const name = entry.name ?? "tool";
  const result = entry.result ?? "";
  const long = result.length > 600;
  const Icon = toolIcon(name);
  const showBody = result.length > 0 && (open || !long);
  return (
    <div className={`tool-card ${entry.status === "error" ? "error" : ""}`}>
      <button className="tool-head" onClick={() => setOpen((o) => !o)}>
        <Icon size={13} />
        <span className="tool-name">{name}</span>
        {entry.intent && <span className="tool-intent">{entry.intent}</span>}
        <ToolStatus status={entry.status ?? "pending"} />
        {result.length > 0 && (open ? <ChevronDown size={12} /> : <ChevronRight size={12} />)}
      </button>
      {showBody && <pre className="tool-result">{result}</pre>}
    </div>
  );
}

function ThinkingBlock({ entry, theme, last }: { entry: IndexedEntry; theme: Theme; last: boolean }) {
  const [open, setOpen] = useState(last);
  const text = entry.text ?? "";
  return (
    <div className={`thinking ${open ? "open" : ""}`}>
      <button className="thinking-head" onClick={() => setOpen((o) => !o)}>
        <span className={`thinking-dot${last ? " live" : ""}`} />
        <span className="thinking-label">{last ? "Reasoning…" : "Reasoning"}</span>
        <span className="thinking-time">{timeOf(entry.ts)}</span>
        {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
      </button>
      {open && (
        <div className="thinking-body">
          <Markdown text={text} theme={theme} />
        </div>
      )}
    </div>
  );
}

function AskCard({ ask, paneId }: { ask: Ask; paneId: string }) {
  const [busy, setBusy] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const answer = async (index: number) => {
    setBusy(index);
    setError(null);
    try {
      await api.answerAsk(paneId, ask.call_id, index);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };
  const withDescription = ask.options.filter((o) => o.description);
  return (
    <div className="ask-card">
      <div className="ask-head">
        <HelpCircle size={14} />
        <span className="ask-label">{ask.multi ? "Multi-select ask" : "Ask"}</span>
      </div>
      <div className="ask-question">{ask.question}</div>
      <div className="ask-options">
        {ask.options.map((option, i) => (
          <button
            key={option.label}
            className="ask-option"
            disabled={busy !== null}
            onClick={() => void answer(i)}
          >
            <span className="ask-option-label">{option.label}</span>
            {i === ask.recommended && <span className="ask-rec">recommended</span>}
            {busy === i && <Loader2 size={12} className="spin" />}
          </button>
        ))}
      </div>
      {withDescription.length > 0 && (
        <ul className="ask-descriptions">
          {withDescription.map((o) => (
            <li key={o.label}>
              <strong>{o.label}</strong> — {o.description}
            </li>
          ))}
        </ul>
      )}
      {error && <div className="ask-error">{error}</div>}
    </div>
  );
}

function EntryView({
  entry,
  theme,
  last,
}: {
  entry: IndexedEntry;
  theme: Theme;
  last: boolean;
}) {
  switch (entry.kind) {
    case "user":
      return (
        <div className="msg user">
          <div className="bubble">
            <pre className="user-text">{entry.text}</pre>
            <span className="msg-time">{timeOf(entry.ts)}</span>
          </div>
        </div>
      );
    case "assistant":
      return (
        <div className="msg assistant">
          <Markdown text={entry.text ?? ""} theme={theme} />
        </div>
      );
    case "thinking":
      return <ThinkingBlock entry={entry} theme={theme} last={last} />;
    case "tool":
      return <ToolCard entry={entry} />;
    case "system":
      return (
        <div className="sys-marker" role="separator">
          <span className="sys-line" />
          <span className="sys-body">
            <span className="sys-label">{entry.label ?? "Session event"}</span>
            {entry.detail && <span className="sys-detail">{entry.detail}</span>}
            {entry.ts && <span className="sys-time">{timeOf(entry.ts)}</span>}
          </span>
          <span className="sys-line" />
        </div>
      );
    default:
      return null;
  }
}

export function Chat({
  pane,
  workspace,
  theme,
  onToggleTerminal,
  terminalOpen,
}: {
  pane: Pane;
  workspace: Workspace | undefined;
  theme: Theme;
  onToggleTerminal: () => void;
  terminalOpen: boolean;
}) {
  const [page, setPage] = useState<SessionPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);
  const stickRef = useRef(true);
  const paneIdRef = useRef(pane.pane_id);

  const load = useCallback(
    async (before?: number, keepAnchor = false) => {
      if (!keepAnchor) stickRef.current = true;
      try {
        const next = await api.session(pane.pane_id, before);
        if (paneIdRef.current !== pane.pane_id) return;
        setPage(next);
        setError(null);
      } catch (e) {
        if (paneIdRef.current !== pane.pane_id) return;
        setError(String(e));
      } finally {
        if (paneIdRef.current === pane.pane_id) setLoading(false);
      }
    },
    [pane.pane_id],
  );

  // Fresh page when the pane changes.
  useEffect(() => {
    paneIdRef.current = pane.pane_id;
    stickRef.current = true;
    setLoading(true);
    setPage(null);
    void load();
  }, [pane.pane_id, load]);

  // Live refresh: the backend pokes when this pane's session file grows.
  useEffect(() => {
    let alive = true;
    const off = api.onPoke((paneId) => {
      if (alive && paneId === paneIdRef.current) void load(undefined, true);
    });
    return () => {
      alive = false;
      off();
    };
  }, [load]);

  // Anchor scroll to the bottom while the user is near it.
  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    stickRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 160;
  };
  useEffect(() => {
    const el = scrollRef.current;
    if (el && stickRef.current) el.scrollTop = el.scrollHeight;
  }, [page]);

  const loadOlder = () => {
    const oldest = page?.entries[0]?.index;
    if (oldest == null) return;
    stickRef.current = false;
    void load(oldest);
  };

  const lastIndex = page && page.entries.length > 0 ? page.entries[page.entries.length - 1].index : -1;

  return (
    <div className="chat">
      <header className="chat-head">
        <div className="chat-head-id">
          <div className="chat-title">{taskTitle(pane.task, pane.pane_id)}</div>
          <div className="chat-sub">
            {workspace?.label ?? pane.workspace_id}
            {page?.model && (
              <span className="chat-model" title="active session model">
                {page.model.provider}/{page.model.model}
              </span>
            )}
            {page?.thinking && (
              <span className="chat-thinking" title="reasoning level">
                thinking {page.thinking}
              </span>
            )}
          </div>
        </div>
        <div className="chat-head-right">
          <span className={`head-status ${pane.pending_ask ? "needs" : pane.status}`}>
            {pane.status === "working" && !pane.pending_ask ? (
              <Loader2 size={12} className="spin head-status-spin" aria-hidden="true" />
            ) : (
              <span className="head-status-dot" />
            )}
            {statusLabel(pane.pending_ask ? "blocked" : pane.status)}
          </span>
          <button
            type="button"
            className="tb-btn"
            title="Interrupt agent (Ctrl+C)"
            aria-label="Interrupt agent"
            onClick={() => void api.sendKeys(pane.pane_id, ["ctrl+c"])}
          >
            <Square size={13} />
          </button>
          <button
            type="button"
            className="tb-btn"
            title="Send Escape to the agent"
            aria-label="Send Escape"
            onClick={() => void api.sendKeys(pane.pane_id, ["Escape"])}
          >
            <span className="key-cap">Esc</span>
          </button>
          <button
            type="button"
            className={`tb-btn${terminalOpen ? " active" : ""}`}
            onClick={onToggleTerminal}
            title="Toggle terminal"
            aria-label="Toggle terminal"
          >
            <TerminalIcon size={14} />
          </button>
        </div>
      </header>

      <div className="transcript" ref={scrollRef} onScroll={onScroll}>
        {error && <div className="chat-error">{error}</div>}
        {loading && page === null && (
          <div className="chat-loading">
            <Loader2 size={16} className="spin" />
            <span>Reading transcript…</span>
          </div>
        )}
        {!loading && page && page.entries.length === 0 && (
          <div className="chat-empty">
            <span>No messages yet.</span>
          </div>
        )}
        {page?.has_older && (
          <button className="load-older" onClick={loadOlder}>
            Load older messages
          </button>
        )}
        {page?.entries.map((entry) => (
          <EntryView
            key={entry.index}
            entry={entry}
            theme={theme}
            last={entry.index === lastIndex}
          />
        ))}
      </div>

      {page?.pending_ask && <AskCard ask={page.pending_ask} paneId={pane.pane_id} />}

      <Composer
        key={pane.pane_id}
        paneId={pane.pane_id}
        cwd={pane.cwd}
        pendingAsk={page?.pending_ask ?? null}
        model={page?.model ? `${page.model.provider}/${page.model.model}` : null}
        thinking={page?.thinking ?? null}
        workspaceLabel={workspace?.label ?? null}
      />
    </div>
  );
}