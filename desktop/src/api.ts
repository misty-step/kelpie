// API layer. Inside the Tauri webview it invokes backend commands and
// subscribes to backend events. In a plain browser (vite dev / preview) it
// falls back to fixture files so the UI can be developed and verified
// without the desktop shell.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Command, Fleet, HerdrStatus, ModelOption, SessionPage, UsageData } from "./types";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export interface Api {
  fleet(): Promise<Fleet>;
  session(paneId: string, before?: number): Promise<SessionPage>;
  sendText(paneId: string, text: string): Promise<void>;
  sendKeys(paneId: string, keys: string[]): Promise<void>;
  answerAsk(paneId: string, callId: string, index: number): Promise<void>;
  readScreen(paneId: string): Promise<string>;
  usage(): Promise<UsageData>;
  commands(cwd?: string | null): Promise<Command[]>;
  models(): Promise<ModelOption[]>;
  openUrl(url: string): void;
  onFleet(cb: (fleet: Fleet) => void): () => void;
  onPoke(cb: (paneId: string) => void): () => void;
  onHerdrStatus(cb: (status: HerdrStatus) => void): () => void;
}

// ── Boundary guards (fixture + event payloads arrive as unknown) ─────────────

function isFleet(v: unknown): v is Fleet {
  return (
    typeof v === "object" &&
    v !== null &&
    "workspaces" in v &&
    "panes" in v &&
    Array.isArray(v.workspaces) &&
    Array.isArray(v.panes)
  );
}

function isSessionPage(v: unknown): v is SessionPage {
  return (
    typeof v === "object" &&
    v !== null &&
    "entries" in v &&
    "total_entries" in v &&
    Array.isArray(v.entries)
  );
}

function isUsageData(v: unknown): v is UsageData {
  return typeof v === "object" && v !== null && "reports" in v && Array.isArray(v.reports);
}

function isHerdrStatus(v: unknown): v is HerdrStatus {
  return typeof v === "object" && v !== null && "ok" in v && "message" in v;
}

function isPoke(v: unknown): v is { pane_id: string } {
  return (
    typeof v === "object" &&
    v !== null &&
    "pane_id" in v &&
    typeof v.pane_id === "string"
  );
}
function isCommandCatalog(v: unknown): v is { commands: Command[] } {
  return (
    typeof v === "object" &&
    v !== null &&
    "commands" in v &&
    Array.isArray(v.commands)
  );
}

function isModelOption(v: unknown): v is ModelOption {
  if (typeof v !== "object" || v === null) return false;
  const o = v as Record<string, unknown>;
  return (
    typeof o.provider === "string" &&
    typeof o.id === "string" &&
    typeof o.selector === "string" &&
    typeof o.name === "string" &&
    (o.thinking === null ||
      (Array.isArray(o.thinking) && o.thinking.every((t) => typeof t === "string")))
  );
}

function isModelCatalog(v: unknown): v is { models: ModelOption[] } {
  return (
    typeof v === "object" &&
    v !== null &&
    "models" in v &&
    Array.isArray((v as { models: unknown }).models) &&
    (v as { models: unknown[] }).models.every(isModelOption)
  );
}

// ── Tauri backend ────────────────────────────────────────────────────────────

const tauriApi: Api = {
  fleet: () => invoke("fleet"),
  session: (paneId, before) => invoke("session", { paneId, before: before ?? null }),
  sendText: (paneId, text) => invoke("send_text", { paneId, text }),
  sendKeys: (paneId, keys) => invoke("send_keys", { paneId, keys }),
  answerAsk: (paneId, callId, index) => invoke("answer_ask", { paneId, callId, index }),
  readScreen: (paneId) => invoke("read_screen", { paneId }),
  usage: () => invoke("usage"),
  commands: async (cwd) => {
    const catalog = await invoke<unknown>("commands", { cwd: cwd ?? null });
    if (!isCommandCatalog(catalog)) return [];
    return catalog.commands;
  },
  models: async () => {
    const catalog = await invoke<unknown>("models");
    if (!isModelCatalog(catalog)) return [];
    return catalog.models;
  },
  openUrl: (url) => void invoke("open_url", { url }).catch(() => window.open(url, "_blank")),
  onFleet: (cb) => subscribe("fleet", (payload) => isFleet(payload) && cb(payload)),
  onPoke: (cb) =>
    subscribe("poke", (payload) => {
      if (isPoke(payload)) cb(payload.pane_id);
    }),
  onHerdrStatus: (cb) =>
    subscribe("herdr-status", (payload) => isHerdrStatus(payload) && cb(payload)),
};

function subscribe(event: string, cb: (payload: unknown) => void): () => void {
  let unlisten: UnlistenFn | undefined;
  void listen<unknown>(event, (e) => cb(e.payload)).then((fn) => {
    unlisten = fn;
  });
  return () => unlisten?.();
}

// ── Browser preview backend (fixtures; dev tooling, not product behavior) ────

async function fetchJson(path: string): Promise<unknown> {
  const res = await fetch(path);
  if (!res.ok) throw new Error(`${path}: ${res.status}`);
  return res.json();
}

const previewApi: Api = {
  fleet: async () => {
    const data = await fetchJson("/fixtures/fleet.json");
    if (!isFleet(data)) throw new Error("fixture fleet.json: bad shape");
    return data;
  },
  session: async (_paneId, before) => {
    const data = await fetchJson("/fixtures/session.json");
    if (!isSessionPage(data)) throw new Error("fixture session.json: bad shape");
    if (before == null) return data;
    const entries = data.entries.filter((e) => e.index < before);
    return { ...data, entries: entries.slice(-200), has_older: entries.length > 200 };
  },
  sendText: async () => {},
  sendKeys: async () => {},
  answerAsk: async () => {},
  readScreen: () => fetch("/fixtures/terminal.txt").then((r) => r.text()),
  usage: async () => {
    const data = await fetchJson("/fixtures/usage.json");
    if (!isUsageData(data)) throw new Error("fixture usage.json: bad shape");
    return data;
  },
  commands: async () => {
    const data = await fetchJson("/fixtures/commands.json");
    if (!isCommandCatalog(data)) throw new Error("fixture commands.json: bad shape");
    return data.commands;
  },
  models: async () => {
    const data = await fetchJson("/fixtures/models.json");
    if (!isModelCatalog(data)) throw new Error("fixture models.json: bad shape");
    return data.models;
  },
  openUrl: (url) => window.open(url, "_blank", "noopener"),
  onFleet: (cb) => {
    const id = window.setInterval(() => {
      void fetchJson("/fixtures/fleet.json")
        .then((f) => isFleet(f) && cb(f))
        .catch(() => {});
    }, 4000);
    return () => window.clearInterval(id);
  },
  onPoke: () => () => {},
  onHerdrStatus: (cb) => {
    cb({ ok: true, message: "preview fixtures" });
    return () => {};
  },
};

export const api: Api =
  isTauri || !import.meta.env.DEV ? tauriApi : previewApi;
