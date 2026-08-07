import { useEffect, useMemo, useRef, useState } from "react";
import { CornerDownLeft, Loader2, Plus, Search } from "lucide-react";
import { api } from "../api";
import type { Ask, Command, CommandSource, ModelOption } from "../types";

const SOURCE_LABEL: Record<CommandSource, string> = {
  "built-in": "omp",
  skill: "skill",
  workspace: "workspace",
};

/** Prefer these when the full catalog is huge (e.g. 400+ openrouter rows). */
const DEFAULT_SELECTORS = [
  "anthropic/claude-fable-5",
  "anthropic/claude-opus-5",
  "anthropic/claude-sonnet-5",
  "anthropic/claude-mythos-5",
  "anthropic/claude-haiku-4-5",
  "openai-codex/gpt-5.6-luna",
  "openai-codex/gpt-5.6-sol",
  "openai-codex/gpt-5.6-terra",
  "openai-codex/gpt-5.5",
  "openai-codex/gpt-5.4",
  "kimi-code/k3",
  "google-antigravity/gemini-3.6-flash",
  "google-antigravity/gemini-3-pro",
  "xai-oauth/grok-4.5",
  "openrouter/deepseek/deepseek-v4-flash-0731",
  "openrouter/openai/gpt-5.6-luna",
] as const;

const FALLBACK_THINKING = ["off", "minimal", "low", "medium", "high", "xhigh", "max"] as const;

type MenuKind = "slash" | "model" | "thinking" | null;

function shortModel(id: string | null): string {
  if (!id) return "model…";
  const slash = id.lastIndexOf("/");
  return slash >= 0 ? id.slice(slash + 1) : id;
}

function isDatedId(id: string): boolean {
  return /\d{8}$/.test(id) || /-\d{4}-\d{2}-\d{2}$/.test(id);
}

function rankModel(m: ModelOption, q: string): number {
  const selector = m.selector.toLowerCase();
  const name = m.name.toLowerCase();
  const id = m.id.toLowerCase();
  if (!q) {
    const pref = DEFAULT_SELECTORS.indexOf(m.selector as (typeof DEFAULT_SELECTORS)[number]);
    if (pref >= 0) return pref;
    // Non-openrouter, non-dated current models float above the long tail.
    let score = 500;
    if (m.provider === "openrouter") score += 200;
    if (isDatedId(m.id)) score += 50;
    if (!m.thinking) score += 10;
    return score + selector.length / 1000;
  }
  if (selector === q) return 0;
  if (id === q || name === q) return 1;
  if (selector.startsWith(q) || id.startsWith(q)) return 2;
  if (name.startsWith(q)) return 3;
  if (selector.includes(q) || id.includes(q) || name.includes(q)) return 4;
  return 999;
}

export function Composer({
  paneId,
  cwd,
  pendingAsk,
  model,
  thinking,
  workspaceLabel,
}: {
  paneId: string;
  cwd?: string | null;
  pendingAsk: Ask | null;
  model: string | null;
  thinking: string | null;
  workspaceLabel: string | null;
}) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [menu, setMenu] = useState<MenuKind>(null);
  const [slashIndex, setSlashIndex] = useState(0);
  const [modelIndex, setModelIndex] = useState(0);
  const [commands, setCommands] = useState<Command[]>([]);
  const [catalog, setCatalog] = useState<ModelOption[]>([]);
  const [modelQuery, setModelQuery] = useState("");
  const [localModel, setLocalModel] = useState<string | null>(null);
  const [localThinking, setLocalThinking] = useState<string | null>(null);
  const taRef = useRef<HTMLTextAreaElement>(null);
  const boxRef = useRef<HTMLDivElement>(null);
  const modelSearchRef = useRef<HTMLInputElement>(null);

  const displayModel = localModel ?? model;
  const displayThinking = localThinking ?? thinking;

  useEffect(() => {
    setLocalModel(null);
    setLocalThinking(null);
  }, [paneId, model, thinking]);

  useEffect(() => {
    void api
      .commands(cwd)
      .then(setCommands)
      .catch(() => setCommands([]));
  }, [cwd]);

  useEffect(() => {
    const load = api.models;
    if (typeof load !== "function") {
      setCatalog([]);
      return;
    }
    void load
      .call(api)
      .then(setCatalog)
      .catch(() => setCatalog([]));
  }, []);

  const slashQuery =
    text.startsWith("/") && !text.includes(" ") ? text.slice(1).toLowerCase() : null;
  const slashMatches = useMemo(() => {
    if (menu !== "slash" && slashQuery == null) return [];
    const pool = commands;
    if (slashQuery == null || slashQuery === "") return pool.slice(0, 12);
    const q = slashQuery;
    const exact = pool.filter(
      (c) => c.name.startsWith(q) || c.aliases?.some((a) => a.startsWith(q)),
    );
    const loose = pool.filter(
      (c) =>
        !exact.includes(c) &&
        (c.name.includes(q) ||
          c.description.toLowerCase().includes(q) ||
          c.aliases?.some((a) => a.includes(q))),
    );
    return [...exact, ...loose].slice(0, 12);
  }, [commands, slashQuery, menu]);

  const modelMatches = useMemo(() => {
    const q = modelQuery.trim().toLowerCase();
    let pool = catalog;
    // Always include the active session model even if it falls outside defaults.
    if (!q) {
      const defaults = DEFAULT_SELECTORS.map((s) => catalog.find((m) => m.selector === s)).filter(
        (m): m is ModelOption => !!m,
      );
      const active: ModelOption | null =
        displayModel && !defaults.some((m) => m.selector === displayModel)
          ? catalog.find((m) => m.selector === displayModel) ?? {
              provider: displayModel.split("/")[0] ?? "unknown",
              id: shortModel(displayModel),
              selector: displayModel,
              name: shortModel(displayModel),
              thinking: null,
            }
          : null;
      pool = active ? [active, ...defaults.filter((m) => m.selector !== active.selector)] : defaults;
      // If catalog is empty (still loading / failed), show defaults as placeholders.
      if (pool.length === 0 && catalog.length === 0) {
        pool = DEFAULT_SELECTORS.map((selector) => ({
          provider: selector.split("/")[0] ?? "unknown",
          id: selector.includes("/") ? selector.slice(selector.indexOf("/") + 1) : selector,
          selector,
          name: shortModel(selector),
          thinking: null,
        }));
      }
      // Catalog loaded but none of the preferred selectors exist — show top non-dated per provider.
      if (pool.length === 0 && catalog.length > 0) {
        const seen = new Set<string>();
        pool = catalog
          .filter((m) => m.provider !== "openrouter" && !isDatedId(m.id))
          .filter((m) => {
            if (seen.has(m.provider)) return false;
            seen.add(m.provider);
            return true;
          })
          .slice(0, 16);
      }
    } else {
      pool = catalog
        .map((m) => ({ m, r: rankModel(m, q) }))
        .filter((x) => x.r < 999)
        .sort((a, b) => a.r - b.r || a.m.selector.localeCompare(b.m.selector))
        .map((x) => x.m)
        .slice(0, 40);
    }
    return pool;
  }, [catalog, modelQuery, displayModel]);

  const thinkingOptions = useMemo(() => {
    const current = catalog.find((m) => m.selector === displayModel);
    if (current?.thinking && current.thinking.length > 0) {
      return current.thinking.includes("off")
        ? current.thinking
        : ["off", ...current.thinking];
    }
    return [...FALLBACK_THINKING];
  }, [catalog, displayModel]);

  useEffect(() => {
    setSlashIndex(0);
  }, [slashQuery, menu]);

  useEffect(() => {
    setModelIndex(0);
  }, [modelQuery, menu]);

  useEffect(() => {
    if (menu === "model") {
      setModelQuery("");
      requestAnimationFrame(() => modelSearchRef.current?.focus());
    }
  }, [menu]);

  useEffect(() => {
    if (!menu) return;
    const onDoc = (e: MouseEvent) => {
      if (!boxRef.current?.contains(e.target as Node)) setMenu(null);
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [menu]);

  const autoResize = () => {
    const ta = taRef.current;
    if (!ta) return;
    ta.style.height = "0px";
    ta.style.height = `${Math.min(Math.max(ta.scrollHeight, 72), 220)}px`;
  };

  const send = async () => {
    const value = text.trim();
    if (!value || busy) return;
    setBusy(true);
    setError(null);
    setMenu(null);
    try {
      await api.sendText(paneId, value);
      setText("");
      requestAnimationFrame(autoResize);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const pickSlash = (command: Command) => {
    const rest = text.startsWith("/")
      ? text.slice(1 + (slashQuery?.length ?? 0)).replace(/^\s*/, "")
      : text;
    setText(rest ? `/${command.name} ${rest}` : `/${command.name} `);
    setMenu(null);
    taRef.current?.focus();
  };

  const applyModel = async (option: ModelOption) => {
    setLocalModel(option.selector);
    setMenu(null);
    setError(null);
    // Clamp thinking if the new model does not support the current level.
    if (
      localThinking &&
      option.thinking &&
      option.thinking.length > 0 &&
      !option.thinking.includes(localThinking) &&
      localThinking !== "off"
    ) {
      setLocalThinking(option.thinking[option.thinking.length - 1] ?? null);
    }
    try {
      await api.sendText(paneId, `/model ${option.selector}`);
    } catch (e) {
      setError(String(e));
    }
  };

  const applyThinking = async (level: string) => {
    setLocalThinking(level);
    setMenu(null);
    setError(null);
    const thinkingCmd = commands.find(
      (c) => c.name === "thinking" || c.name === "effort" || c.aliases?.includes("thinking"),
    );
    if (thinkingCmd) {
      try {
        await api.sendText(paneId, `/${thinkingCmd.name} ${level}`);
      } catch (e) {
        setError(String(e));
      }
    }
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;

    if (menu === "slash" && slashMatches.length > 0) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSlashIndex((i) => (i + 1) % slashMatches.length);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSlashIndex((i) => (i - 1 + slashMatches.length) % slashMatches.length);
        return;
      }
      if (e.key === "Tab" || (e.key === "Enter" && !e.shiftKey)) {
        e.preventDefault();
        const command = slashMatches[slashIndex];
        if (command) pickSlash(command);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        setMenu(null);
        if (text.startsWith("/") && !text.includes(" ")) setText("");
        return;
      }
    }

    if (menu && e.key === "Escape") {
      e.preventDefault();
      setMenu(null);
      return;
    }

    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void send();
    }
  };

  const onModelSearchKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setModelIndex((i) => (modelMatches.length ? (i + 1) % modelMatches.length : 0));
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      setModelIndex((i) =>
        modelMatches.length ? (i - 1 + modelMatches.length) % modelMatches.length : 0,
      );
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      const opt = modelMatches[modelIndex];
      if (opt) void applyModel(opt);
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      setMenu(null);
      taRef.current?.focus();
    }
  };

  useEffect(() => {
    if (slashQuery != null) setMenu("slash");
  }, [slashQuery]);

  return (
    <div className="composer">
      {pendingAsk && (
        <div className="composer-banner" role="status">
          <span className="composer-banner-dot" aria-hidden="true" />
          <span>
            Agent is waiting for your answer — choose an option in the transcript, or type a reply.
          </span>
        </div>
      )}
      {error && (
        <div className="composer-error" role="alert">
          {error}
        </div>
      )}

      <div
        className={`composer-box${busy ? " is-busy" : ""}`}
        ref={boxRef}
        data-busy={busy || undefined}
      >
        {menu === "slash" && slashMatches.length > 0 && (
          <div className="slash-menu" role="listbox" aria-label="Slash commands">
            <div className="slash-menu-head">Commands</div>
            {slashMatches.map((command, i) => {
              const source = (command.source ?? "built-in") as CommandSource;
              return (
                <button
                  key={`${source}:${command.name}`}
                  type="button"
                  role="option"
                  aria-selected={i === slashIndex}
                  className={`slash-item${i === slashIndex ? " active" : ""}`}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    pickSlash(command);
                  }}
                >
                  <span className="slash-name">/{command.name}</span>
                  <span className="slash-desc">{command.description}</span>
                  <span className={`slash-src ${source}`}>{SOURCE_LABEL[source]}</span>
                </button>
              );
            })}
          </div>
        )}

        {menu === "model" && (
          <div className="session-menu model-menu" role="listbox" aria-label="Session model">
            <div className="model-search-row">
              <Search size={13} aria-hidden="true" />
              <input
                ref={modelSearchRef}
                className="model-search"
                value={modelQuery}
                placeholder={
                  catalog.length
                    ? `Search ${catalog.length} models…`
                    : "Search models…"
                }
                aria-label="Filter models"
                onChange={(e) => setModelQuery(e.target.value)}
                onKeyDown={onModelSearchKey}
              />
            </div>
            <div className="slash-menu-head">
              {modelQuery.trim()
                ? `${modelMatches.length} match${modelMatches.length === 1 ? "" : "es"}`
                : "Suggested"}
            </div>
            {modelMatches.length === 0 ? (
              <div className="model-empty">No models match “{modelQuery.trim()}”.</div>
            ) : (
              modelMatches.map((option, i) => (
                <button
                  key={option.selector}
                  type="button"
                  role="option"
                  aria-selected={i === modelIndex || option.selector === displayModel}
                  className={`slash-item model-item${
                    i === modelIndex || option.selector === displayModel ? " active" : ""
                  }`}
                  onMouseEnter={() => setModelIndex(i)}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    void applyModel(option);
                  }}
                >
                  <span className="model-item-main">
                    <span className="model-item-name">{option.name}</span>
                    <span className="slash-name mono model-item-sel">{option.selector}</span>
                  </span>
                  <span className="model-item-meta">
                    <span className="slash-src">{option.provider}</span>
                    {option.thinking && option.thinking.length > 0 && (
                      <span className="model-think-tag">think</span>
                    )}
                  </span>
                </button>
              ))
            )}
          </div>
        )}

        {menu === "thinking" && (
          <div className="session-menu thinking-menu" role="listbox" aria-label="Thinking level">
            <div className="slash-menu-head">Thinking</div>
            {thinkingOptions.map((level) => (
              <button
                key={level}
                type="button"
                role="option"
                aria-selected={level === (displayThinking ?? "")}
                className={`slash-item${level === displayThinking ? " active" : ""}`}
                onMouseDown={(e) => {
                  e.preventDefault();
                  void applyThinking(level);
                }}
              >
                <span className="slash-name">{level}</span>
              </button>
            ))}
          </div>
        )}

        <label className="sr-only" htmlFor={`composer-input-${paneId}`}>
          Message to agent
        </label>
        <textarea
          id={`composer-input-${paneId}`}
          ref={taRef}
          className="composer-input"
          value={text}
          rows={3}
          disabled={busy}
          placeholder={
            pendingAsk ? "Answer the ask, or type a message…" : "Ask for follow-up changes"
          }
          onChange={(e) => {
            setText(e.target.value);
            const v = e.target.value;
            if (v.startsWith("/") && !v.includes(" ")) setMenu("slash");
            autoResize();
          }}
          onKeyDown={onKeyDown}
        />

        <div className="composer-action-row">
          <button
            type="button"
            className={`composer-slash${menu === "slash" ? " active" : ""}`}
            title="Insert a / command (skills + workspace + omp)"
            aria-label="Open slash commands"
            aria-expanded={menu === "slash"}
            disabled={busy}
            onClick={() => {
              if (menu === "slash") setMenu(null);
              else {
                setMenu("slash");
                taRef.current?.focus();
              }
            }}
          >
            <Plus size={16} />
          </button>
          <span className="composer-action-hint">
            Enter sends · Shift+Enter newline · / commands
          </span>
          <button
            type="button"
            className="composer-send"
            disabled={busy || !text.trim()}
            onClick={() => void send()}
            title="Send (Enter)"
          >
            {busy ? (
              <Loader2 size={14} className="spin" />
            ) : (
              <>
                <span>Send</span>
                <CornerDownLeft size={14} />
              </>
            )}
          </button>
        </div>

        <div className="composer-session-rail" aria-label="Session controls">
          <span className="session-chip workspace" title="Workspace (read-only)">
            <span className="session-chip-label">{workspaceLabel ?? "—"}</span>
          </span>

          <button
            type="button"
            className={`session-chip picker${menu === "model" ? " open" : ""}`}
            title={displayModel ?? "Session model"}
            aria-haspopup="listbox"
            aria-expanded={menu === "model"}
            disabled={busy}
            onClick={() => setMenu((m) => (m === "model" ? null : "model"))}
          >
            <span className="session-chip-k">model</span>
            <span className="session-chip-v mono">{shortModel(displayModel)}</span>
          </button>

          <button
            type="button"
            className={`session-chip picker${menu === "thinking" ? " open" : ""}`}
            title={
              commands.some((c) => c.name === "thinking" || c.name === "effort")
                ? "Thinking level"
                : "Thinking level (levels from model catalog; absolute set needs omp slash/RPC)"
            }
            aria-haspopup="listbox"
            aria-expanded={menu === "thinking"}
            disabled={busy}
            onClick={() => setMenu((m) => (m === "thinking" ? null : "thinking"))}
          >
            <span className="session-chip-k">thinking</span>
            <span className="session-chip-v">{displayThinking ?? "—"}</span>
          </button>
        </div>
      </div>
    </div>
  );
}
