import { useCallback, useEffect, useRef, useState } from "react";
import {
  Braces,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  FileText,
  Flame,
  FolderGit2,
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

function EffortIcon({ effort }: { effort: string }) {
  const e = effort.toLowerCase();
  if (e.includes("max") || e.includes("extra high") || e.includes("xhigh")) {
    return <Flame size={11} className="pane-effort-ico flame" aria-hidden="true" />;
  }
  if (e.includes("high")) {
    return (
      <svg width="10" height="10" viewBox="0 0 16 16" className="pane-effort-ico high" aria-hidden="true">
        <circle cx="8" cy="8" r="6" fill="currentColor" />
      </svg>
    );
  }
  if (e.includes("medium")) {
    return (
      <svg width="10" height="10" viewBox="0 0 16 16" className="pane-effort-ico medium" aria-hidden="true">
        <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <path d="M8 2a6 6 0 0 1 0 12V2z" fill="currentColor" />
      </svg>
    );
  }
  return (
    <svg width="10" height="10" viewBox="0 0 16 16" className="pane-effort-ico low" aria-hidden="true">
      <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}

function WorkingSpinner() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" className="spin st-spinner" aria-hidden="true">
      <circle cx="8" cy="8" r="6" fill="none" stroke="var(--brand-soft)" strokeWidth="2" opacity="0.35" />
      <path d="M14 8a6 6 0 0 0-6-6" fill="none" stroke="var(--brand)" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
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
  return null;
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
        <span className="thinking-label">{last ? "Reasoning..." : "Reasoning"}</span>
        <span className="thinking-time">{timeOf(entry.ts)}</span>
        {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
      </button>
      {open && text.length > 0 && (
        <div className="thinking-body">
          <Markdown text={text} theme={theme} />
        </div>
      )}
    </div>
  );
}

function AskCard({ ask, paneId }: { ask: Ask; paneId: string }) {
  const [busy, setBusy] = useState<number | null>(null);

  const withDescription = ask.options.filter((o) => Boolean(o.description));

  const answer = async (index: number) => {
    setBusy(index);
    try {
      await api.answerAsk(paneId, ask.call_id, index);
    } finally {
      setBusy(null);
    }
  };

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
              <strong>{o.label}</strong> - {o.description}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function Chat({
  pane,
  workspace,
  theme,
  model,
  thinking,
  onToggleTerminal,
  terminalOpen,
}: {
  pane: Pane;
  workspace: Workspace | null;
  theme: Theme;
  model: string | null;
  thinking: string | null;
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

  useEffect(() => {
    paneIdRef.current = pane.pane_id;
    stickRef.current = true;
    setLoading(true);
    setPage(null);
    void load();
  }, [pane.pane_id, load]);

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

  const wsLabel = workspace?.label ?? pane.workspace_id;
  const taskText = taskTitle(pane.task, pane.pane_id);
  const provider = (page?.model?.provider ?? pane.provider ?? "").toLowerCase();
  const modelName = page?.model?.model ?? pane.model;
  const effortLevel = page?.thinking ?? pane.effort;

  return (
    <div className="chat">
      <header className="chat-head">
        <div className="chat-head-left">
          <div className="chat-ws-badge">
            <FolderGit2 size={12} className="chat-ws-icon" />
            <span className="chat-ws-name">{wsLabel}</span>
          </div>
          <h2 className="chat-title" title={pane.task ?? pane.pane_id}>
            {taskText}
          </h2>
          {(provider || modelName || effortLevel) && (
            <div className="chat-meta">
              {provider && <span className="chat-provider">{provider}</span>}
              {provider && modelName && <span className="chat-dot">•</span>}
              {modelName && <span className="chat-model">{modelName}</span>}
              {effortLevel && (
                <span className="chat-thinking-tag">
                  <EffortIcon effort={effortLevel} />
                  <span>{effortLevel.toLowerCase()}</span>
                </span>
              )}
            </div>
          )}
        </div>

        <div className="chat-head-right">
          <div className={`head-status ${pane.pending_ask ? "needs" : pane.status}`}>
            {pane.status === "working" && !pane.pending_ask ? (
              <WorkingSpinner />
            ) : (
              <span className="head-status-dot" />
            )}
            <span className="head-status-label">{statusLabel(pane.pending_ask ? "blocked" : pane.status)}</span>
          </div>

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
            className={`chat-head-btn${terminalOpen ? " active" : ""}`}
            onClick={onToggleTerminal}
            title={terminalOpen ? "Close terminal drawer" : "Open terminal drawer"}
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

        {page && page.entries.length > 0 && page.entries[0].index > 0 && (
          <button type="button" className="load-older" onClick={loadOlder}>
            Load older messages…
          </button>
        )}

        {page?.entries.map((entry, idx) => {
          const isLast = idx === page.entries.length - 1;
          if (entry.kind === "thinking") {
            return <ThinkingBlock key={entry.index} entry={entry} theme={theme} last={isLast} />;
          }
          if (entry.kind === "tool") {
            return <ToolCard key={entry.index} entry={entry} />;
          }
          if (entry.kind === "system") {
            return (
              <div key={entry.index} className="sys-marker">
                <div className="sys-line" />
                <div className="sys-body">
                  <span className="sys-label">{entry.label}</span>
                  {entry.detail && <span className="sys-detail">{entry.detail}</span>}
                  {entry.ts && <span className="sys-time">{timeOf(entry.ts)}</span>}
                </div>
                <div className="sys-line" />
              </div>
            );
          }
          const isUser = entry.kind === "user";
          return (
            <div key={entry.index} className={`msg ${isUser ? "user" : "assistant"}`}>
              <div className="bubble">
                <Markdown text={entry.text ?? ""} theme={theme} />
              </div>
            </div>
          );
        })}

        {page?.pending_ask && <AskCard ask={page.pending_ask} paneId={pane.pane_id} />}
      </div>

      <Composer
        paneId={pane.pane_id}
        pendingAsk={page?.pending_ask ?? null}
        model={model}
        thinking={thinking}
        workspaceLabel={wsLabel}
      />
    </div>
  );
}
