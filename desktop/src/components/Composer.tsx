import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowUp, ChevronDown, Flame, Loader2, Plus, Search, X } from "lucide-react";
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

/** Full selector, lowercase — e.g. xai-oauth/grok-4.5 */
function modelLabel(id: string | null): string {
  if (!id) return "model…";
  return id.toLowerCase();
}

function shortModel(id: string | null): string {
  if (!id) return "model…";
  const slash = id.lastIndexOf("/");
  return slash >= 0 ? id.slice(slash + 1) : id;
}

function formatThinking(level: string): string {
  const t = level.trim().toLowerCase();
  if (!t) return "thinking";
  return t;
}

function EffortIcon({ effort }: { effort: string }) {
  const e = effort.toLowerCase();
  if (e.includes("max") || e.includes("extra high") || e.includes("xhigh")) {
    return <Flame size={12} className="pane-effort-ico flame" aria-hidden="true" />;
  }
  if (e.includes("high")) {
    return (
      <svg width="12" height="12" viewBox="0 0 16 16" className="pane-effort-ico high" aria-hidden="true">
        <circle cx="8" cy="8" r="6" fill="currentColor" />
      </svg>
    );
  }
  if (e.includes("medium")) {
    return (
      <svg width="12" height="12" viewBox="0 0 16 16" className="pane-effort-ico medium" aria-hidden="true">
        <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1.5" />
        <path d="M8 2a6 6 0 0 1 0 12V2z" fill="currentColor" />
      </svg>
    );
  }
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" className="pane-effort-ico low" aria-hidden="true">
      <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
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
  const [askBusy, setAskBusy] = useState<number | null>(null);
  const [askOther, setAskOther] = useState(false);
  const [otherText, setOtherText] = useState("");
  const [attachments, setAttachments] = useState<File[]>([]);
  const [menuAnchor, setMenuAnchor] = useState<{ left: number; bottom: number } | null>(null);
  const taRef = useRef<HTMLTextAreaElement>(null);
  const boxRef = useRef<HTMLDivElement>(null);
  const modelSearchRef = useRef<HTMLInputElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const modelBtnRef = useRef<HTMLButtonElement>(null);
  const thinkingBtnRef = useRef<HTMLButtonElement>(null);

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
    if (!menu) {
      setMenuAnchor(null);
      return;
    }
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
    ta.style.height = `${Math.min(Math.max(ta.scrollHeight, 52), 200)}px`;
  };

  const openAnchoredMenu = (kind: "model" | "thinking", btn: HTMLButtonElement | null) => {
    if (menu === kind) {
      setMenu(null);
      return;
    }
    const box = boxRef.current;
    if (box && btn) {
      const br = box.getBoundingClientRect();
      const er = btn.getBoundingClientRect();
      setMenuAnchor({
        left: Math.max(8, er.left - br.left),
        bottom: Math.max(8, br.bottom - er.top + 6),
      });
    } else {
      setMenuAnchor(null);
    }
    setMenu(kind);
  };

  const onPickFiles = (list: FileList | null) => {
    if (!list || list.length === 0) return;
    const next = Array.from(list);
    setAttachments((prev) => {
      const names = new Set(prev.map((f) => `${f.name}:${f.size}`));
      const merged = [...prev];
      for (const f of next) {
        const key = `${f.name}:${f.size}`;
        if (!names.has(key)) merged.push(f);
      }
      return merged.slice(0, 12);
    });
    if (fileInputRef.current) fileInputRef.current.value = "";
  };

  const removeAttachment = (index: number) => {
    setAttachments((prev) => prev.filter((_, i) => i !== index));
  };

  const send = async () => {
    const value = text.trim();
    const hasFiles = attachments.length > 0;
    if ((!value && !hasFiles) || busy || pendingAsk) return;
    setBusy(true);
    setError(null);
    setMenu(null);
    try {
      const fileBlock =
        attachments.length > 0
          ? attachments.map((f) => `@file ${f.name}`).join("\n") + (value ? "\n\n" : "")
          : "";
      await api.sendText(paneId, `${fileBlock}${value}`);
      setText("");
      setAttachments([]);
      requestAnimationFrame(autoResize);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const answerAsk = async (index: number) => {
    if (!pendingAsk || askBusy !== null) return;
    setAskBusy(index);
    setError(null);
    try {
      await api.answerAsk(paneId, pendingAsk.call_id, index);
      setAskOther(false);
      setOtherText("");
    } catch (e) {
      setError(String(e));
    } finally {
      setAskBusy(null);
    }
  };

  const sendOtherAnswer = async () => {
    const value = otherText.trim();
    if (!value || !pendingAsk || askBusy !== null) return;
    setAskBusy(-1);
    setError(null);
    try {
      // Free-text answer while an ask is open is still ask resolution, not a follow-up.
      await api.sendText(paneId, value);
      setOtherText("");
      setAskOther(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setAskBusy(null);
    }
  };

  const cancelAsk = () => {
    setAskOther(false);
    setOtherText("");
    setMenu(null);
    // Escape/cancel dismisses local takeover UI; agent still owns the pending ask
    // until answered. Send Escape to the pane as the operator escape path.
    void api.sendKeys(paneId, ["Escape"]);
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
      if (!pendingAsk) void send();
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
    <div className={`composer${pendingAsk ? " is-ask" : ""}`}>
      {error && (
        <div className="composer-error" role="alert">
          {error}
        </div>
      )}

      <div
        className={`composer-box${busy || askBusy !== null ? " is-busy" : ""}${
          pendingAsk ? " ask-mode" : ""
        }`}
        ref={boxRef}
        data-busy={busy || askBusy !== null || undefined}
      >
        {menu === "slash" && slashMatches.length > 0 && !pendingAsk && (
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
          <div
            className="session-menu model-menu"
            role="listbox"
            aria-label="Session model"
            style={
              menuAnchor
                ? {
                    left: menuAnchor.left,
                    right: "auto",
                    bottom: menuAnchor.bottom,
                    top: "auto",
                    transform: "none",
                  }
                : undefined
            }
          >
            <div className="model-search-row">
              <Search size={13} aria-hidden="true" />
              <input
                ref={modelSearchRef}
                className="model-search"
                value={modelQuery}
                placeholder={
                  catalog.length ? `Search ${catalog.length} models…` : "Search models…"
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
                    <span className="model-item-name mono">{option.selector.toLowerCase()}</span>
                    <span className="model-item-sel">{option.name}</span>
                  </span>
                  <span className="model-item-meta">
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
          <div
            className="session-menu thinking-menu"
            role="listbox"
            aria-label="Thinking level"
            style={
              menuAnchor
                ? {
                    left: menuAnchor.left,
                    right: "auto",
                    bottom: menuAnchor.bottom,
                    top: "auto",
                    transform: "none",
                  }
                : undefined
            }
          >
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
                <span className="slash-name thinking-opt">
                  <EffortIcon effort={level} />
                  <span>{formatThinking(level)}</span>
                </span>
              </button>
            ))}
          </div>
        )}

        {pendingAsk ? (
          <div className="composer-ask" role="group" aria-label="Pending ask">
            <div className="composer-ask-kicker">Needs your answer</div>
            <div className="composer-ask-question">{pendingAsk.question}</div>
            <div className="composer-ask-options">
              {pendingAsk.options.map((option, i) => (
                <button
                  key={option.label}
                  type="button"
                  className={`composer-ask-option${
                    i === pendingAsk.recommended ? " recommended" : ""
                  }`}
                  disabled={askBusy !== null}
                  onClick={() => void answerAsk(i)}
                >
                  <span className="composer-ask-option-label">{option.label}</span>
                  {i === pendingAsk.recommended && (
                    <span className="composer-ask-rec">recommended</span>
                  )}
                  {askBusy === i && <Loader2 size={13} className="spin" />}
                </button>
              ))}
            </div>
            {askOther ? (
              <div className="composer-ask-other">
                <label className="sr-only" htmlFor={`ask-other-${paneId}`}>
                  Other response
                </label>
                <textarea
                  id={`ask-other-${paneId}`}
                  className="composer-ask-other-input"
                  rows={2}
                  value={otherText}
                  placeholder="Type another response…"
                  disabled={askBusy !== null}
                  onChange={(e) => setOtherText(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault();
                      void sendOtherAnswer();
                    }
                    if (e.key === "Escape") {
                      e.preventDefault();
                      setAskOther(false);
                    }
                  }}
                />
                <div className="composer-ask-other-actions">
                  <button
                    type="button"
                    className="composer-ask-link"
                    disabled={askBusy !== null}
                    onClick={() => setAskOther(false)}
                  >
                    Back
                  </button>
                  <button
                    type="button"
                    className="composer-ask-submit"
                    disabled={askBusy !== null || !otherText.trim()}
                    onClick={() => void sendOtherAnswer()}
                  >
                    {askBusy === -1 ? <Loader2 size={14} className="spin" /> : "Submit answer"}
                  </button>
                </div>
              </div>
            ) : (
              <div className="composer-ask-footer">
                <button
                  type="button"
                  className="composer-ask-link"
                  disabled={askBusy !== null}
                  onClick={() => setAskOther(true)}
                >
                  Other response…
                </button>
                <button
                  type="button"
                  className="composer-ask-link mute"
                  disabled={askBusy !== null}
                  onClick={cancelAsk}
                  title="Send Escape to the agent"
                >
                  Cancel
                </button>
              </div>
            )}
          </div>
        ) : (
          <>
            <label className="sr-only" htmlFor={`composer-input-${paneId}`}>
              Message to agent
            </label>
            <textarea
              id={`composer-input-${paneId}`}
              ref={taRef}
              className="composer-input"
              value={text}
              rows={2}
              disabled={busy}
              placeholder="Ask for follow-up changes"
              onChange={(e) => {
                setText(e.target.value);
                const v = e.target.value;
                if (v.startsWith("/") && !v.includes(" ")) setMenu("slash");
                autoResize();
              }}
              onKeyDown={onKeyDown}
            />

            {attachments.length > 0 && (
              <div className="composer-attachments" aria-label="Attachments">
                {attachments.map((file, i) => (
                  <span key={`${file.name}-${file.size}-${i}`} className="composer-attach-chip">
                    <span className="composer-attach-name" title={file.name}>
                      {file.name}
                    </span>
                    <button
                      type="button"
                      className="composer-attach-remove"
                      aria-label={`Remove ${file.name}`}
                      onClick={() => removeAttachment(i)}
                    >
                      <X size={12} />
                    </button>
                  </span>
                ))}
              </div>
            )}

            <div className="composer-toolbar">
              <input
                ref={fileInputRef}
                type="file"
                className="sr-only"
                multiple
                accept="image/*,.png,.jpg,.jpeg,.gif,.webp,.pdf,.txt,.md,.json,.csv,.ts,.tsx,.js,.jsx,.rs,.py,.go,.toml,.yaml,.yml,.html,.css"
                onChange={(e) => onPickFiles(e.target.files)}
              />
              <button
                type="button"
                className="composer-plus"
                title="Attach files or images"
                aria-label="Attach files"
                disabled={busy}
                onClick={() => fileInputRef.current?.click()}
              >
                <Plus size={16} strokeWidth={1.75} />
              </button>

              <button
                ref={modelBtnRef}
                type="button"
                className={`composer-pill model${menu === "model" ? " open" : ""}`}
                title={displayModel ?? "Session model"}
                aria-haspopup="listbox"
                aria-expanded={menu === "model"}
                disabled={busy}
                onClick={() => openAnchoredMenu("model", modelBtnRef.current)}
              >
                <span className="composer-pill-label mono">{modelLabel(displayModel)}</span>
                <ChevronDown size={13} strokeWidth={2} aria-hidden="true" />
              </button>

              <button
                ref={thinkingBtnRef}
                type="button"
                className={`composer-pill thinking${menu === "thinking" ? " open" : ""}`}
                title="Thinking level"
                aria-haspopup="listbox"
                aria-expanded={menu === "thinking"}
                disabled={busy}
                onClick={() => openAnchoredMenu("thinking", thinkingBtnRef.current)}
              >
                {displayThinking && <EffortIcon effort={displayThinking} />}
                <span className="composer-pill-label">
                  {displayThinking ? formatThinking(displayThinking) : "thinking"}
                </span>
                <ChevronDown size={13} strokeWidth={2} aria-hidden="true" />
              </button>

              <span className="composer-toolbar-spacer" aria-hidden="true" />

              <button
                type="button"
                className="composer-send"
                disabled={busy || (!text.trim() && attachments.length === 0)}
                onClick={() => void send()}
                title="Send (Enter)"
                aria-label="Send"
              >
                {busy ? <Loader2 size={16} className="spin" /> : <ArrowUp size={16} strokeWidth={2.25} />}
              </button>
            </div>
          </>
        )}
      </div>

      <div className="composer-meta-rail" aria-label="Session workspace">
        <span className="composer-meta-chip" title="Workspace (read-only)">
          <span className="composer-meta-ico" aria-hidden="true">
            ⌂
          </span>
          <span>{workspaceLabel ?? "—"}</span>
        </span>
      </div>
    </div>
  );
}
