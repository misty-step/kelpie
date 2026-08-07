import { useCallback, useEffect, useState } from "react";
import { Gauge, Loader2, RefreshCw, X } from "lucide-react";
import { api } from "../api";
import type { UsageData } from "../types";

function formatAmount(amount: { used: number; limit: number; unit: string }): string {
  if (amount.unit === "percent") return `${Math.round(amount.used)}% of ${Math.round(amount.limit)}%`;
  const used = new Intl.NumberFormat().format(amount.used);
  const limit = new Intl.NumberFormat().format(amount.limit);
  return `${used} / ${limit} ${amount.unit}`;
}

function resetsIn(resetsAt: number): string {
  const ms = resetsAt - Date.now();
  if (ms <= 0) return "resets now";
  const hours = Math.floor(ms / 3_600_000);
  const days = Math.floor(hours / 24);
  if (days > 0) return `resets in ${days}d ${hours % 24}h`;
  if (hours > 0) return `resets in ${hours}h`;
  const minutes = Math.max(1, Math.floor(ms / 60_000));
  return `resets in ${minutes}m`;
}

function statusChip(status: string): string {
  switch (status) {
    case "exhausted":
      return "chip blocked";
    case "warning":
      return "chip idle";
    default:
      return "chip done";
  }
}

export function UsagePanel({ onClose }: { onClose: () => void }) {
  const [data, setData] = useState<UsageData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    void api
      .usage()
      .then(setData)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="scrim" onClick={onClose}>
      <div className="usage-panel" onClick={(e) => e.stopPropagation()}>
        <header className="usage-head">
          <span className="usage-title">
            <Gauge size={16} />
            Usage
          </span>
          <button className="icon-btn" onClick={load} title="Refresh" disabled={loading}>
            <RefreshCw size={14} className={loading ? "spin" : ""} />
          </button>
          <button className="icon-btn" onClick={onClose} title="Close (Esc)">
            <X size={14} />
          </button>
        </header>
        <div className="usage-body">
          {error && <div className="chat-error">{error}</div>}
          {loading && !data && (
            <div className="usage-loading">
              <Loader2 size={16} className="spin" />
              <span>Querying omp usage…</span>
            </div>
          )}
          {data?.reports.map((report) => (
            <section className="usage-provider" key={report.provider}>
              <h3 className="usage-provider-name">{report.provider}</h3>
              {report.limits.map((limit) => (
                <div className="usage-limit" key={limit.id}>
                  <div className="usage-limit-head">
                    <span className="usage-limit-label">{limit.label}</span>
                    <span className={statusChip(limit.status)}>{limit.status}</span>
                    <span className="usage-limit-reset">{resetsIn(limit.window.resetsAt)}</span>
                  </div>
                  <div className="usage-bar">
                    <div
                      className={`usage-bar-fill ${limit.status}`}
                      style={{ width: `${Math.min(100, Math.round(limit.amount.usedFraction * 100))}%` }}
                    />
                  </div>
                  <div className="usage-limit-meta">
                    <span>{formatAmount(limit.amount)}</span>
                    <span>
                      {Math.round(limit.amount.remainingFraction * 100)}% remaining
                    </span>
                  </div>
                </div>
              ))}
            </section>
          ))}
          {data && data.reports.length === 0 && (
            <div className="usage-empty">No usage reports from omp.</div>
          )}
        </div>
        <footer className="usage-foot">
          <span>
            Live from <code>omp usage --json --redact</code>
            {data ? ` · generated ${new Date(data.generatedAt).toLocaleString()}` : ""}
          </span>
        </footer>
      </div>
    </div>
  );
}
