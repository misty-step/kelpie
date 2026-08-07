// Wire types shared by the Tauri backend (snake_case fields) and the
// preview fixtures. Mirrors the kelpie bridge API shapes.

export type Status = "working" | "blocked" | "idle" | "done" | "unknown";

export interface Workspace {
  id: string;
  label: string;
  status: Status;
  active_tab_id: string | null;
}

export interface Pane {
  pane_id: string;
  workspace_id: string;
  status: Status;
  task: string | null;
  pending_ask: boolean;
  snippet: string | null;
  updated_ms: number | null;
  /** Working directory from herdr; drives workspace command discovery. */
  cwd: string | null;
  provider?: string | null;
  model?: string | null;
  effort?: string | null;
}

export interface Fleet {
  workspaces: Workspace[];
  panes: Pane[];
}

export type Entry =
  | { kind: "user"; text: string; ts?: string | null }
  | { kind: "assistant"; text: string; ts?: string | null }
  | { kind: "thinking"; text: string; ts?: string | null }
  | {
      kind: "tool";
      name: string;
      intent?: string | null;
      status: "pending" | "ok" | "error";
      result?: string | null;
      ts?: string | null;
    }
  | {
      kind: "system";
      label: string;
      detail?: string | null;
      ts?: string | null;
    };

export interface AskOption {
  label: string;
  description?: string | null;
}

export interface Ask {
  call_id: string;
  question: string;
  options: AskOption[];
  multi: boolean;
  recommended?: number | null;
}

export interface IndexedEntry {
  index: number;
  kind: Entry["kind"];
  text?: string;
  ts?: string | null;
  name?: string;
  intent?: string | null;
  status?: "pending" | "ok" | "error";
  result?: string | null;
  label?: string;
  detail?: string | null;
}

export interface SessionPage {
  title: string | null;
  model: { provider: string; model: string } | null;
  thinking: string | null;
  entries: IndexedEntry[];
  pending_ask: Ask | null;
  total_entries: number;
  has_older: boolean;
}

export type CommandSource = "built-in" | "skill" | "workspace";

export interface Command {
  name: string;
  aliases: string[];
  description: string;
  usage?: string;
  source?: CommandSource;
}

/** One row from `omp models --json`. */
export interface ModelOption {
  provider: string;
  id: string;
  /** provider/id — what `/model` accepts */
  selector: string;
  name: string;
  /** Supported thinking levels, or null when the model has none. */
  thinking: string[] | null;
}

export interface UsageLimit {
  id: string;
  label: string;
  window: { id: string; label: string; durationMs: number; resetsAt: number };
  amount: {
    used: number;
    limit: number;
    remaining: number;
    usedFraction: number;
    remainingFraction: number;
    unit: string;
  };
  status: string;
}

export interface UsageReport {
  provider: string;
  fetchedAt: number;
  limits: UsageLimit[];
}

export interface UsageData {
  generatedAt: number;
  reports: UsageReport[];
}

export interface HerdrStatus {
  ok: boolean;
  message: string;
}
