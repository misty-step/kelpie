//! Kelpie desktop: operator console for herdr + omp.
//!
//! The Tauri Rust side is a thin, in-process control plane: it polls the
//! herdr socket, keeps incremental omp transcript projections, and exposes
//! commands + events to the React frontend. No HTTP, no separate bridge
//! process — this is the phase-1 architecture from the kelpie reimagination.

mod herdr;
mod omp;
mod state;
mod catalog;

use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};
use state::{Fleet, Pane, Workspace};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

const POLL_INTERVAL: Duration = Duration::from_millis(600);
const SESSION_PAGE_LIMIT: usize = 200;
const ASK_FOCUS_POLL_MS: u64 = 100;
const ASK_FOCUS_ATTEMPTS: usize = 30;

type Shared = Arc<state::AppState>;

fn s(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| v.get(k).and_then(Value::as_str))
        .map(String::from)
}

fn status_of(value: Option<&str>) -> String {
    value.unwrap_or("unknown").to_string()
}

/// Per-pane state reused across polls: last herdr revision seen (throttles
/// the tail scan) and the last computed summary (avoids flicker).
#[derive(Default)]
struct PollState {
    last_revision: HashMap<String, u64>,
    summaries: HashMap<String, omp::PaneSummary>,
    last_sizes: HashMap<String, u64>,
    last_sizes_emitted: HashMap<String, u64>,
    last_fleet_json: Option<String>,
    last_status: Option<String>,
}

async fn build_fleet(poll: &mut PollState) -> Result<Fleet> {
    let snap = herdr::snapshot().await?;
    let workspaces: Vec<Workspace> = snap
        .get("workspaces")
        .and_then(Value::as_array)
        .map(|ws| {
            ws.iter()
                .map(|w| Workspace {
                    id: s(w, &["workspace_id", "id"]).unwrap_or_default(),
                    label: s(w, &["label", "name", "title"]).unwrap_or_default(),
                    status: status_of(s(w, &["agent_status"]).as_deref()),
                    active_tab_id: s(w, &["active_tab_id"]),
                })
                .collect()
        })
        .unwrap_or_default();
    let mut panes = Vec::new();
    if let Some(raw_panes) = snap.get("panes").and_then(Value::as_array) {
        for p in raw_panes {
            let Some(pane_id) = s(p, &["pane_id", "id"]) else {
                continue;
            };
            // Kelpie is an OMP agent console. Plain terminals and remote shells
            // without a local session path stay out of the fleet.
            let Some(session_path) = p
                .get("agent_session")
                .filter(|a| a.get("kind").and_then(Value::as_str) == Some("path"))
                .and_then(|a| a.get("value"))
                .and_then(Value::as_str)
                .map(String::from)
            else {
                continue;
            };
            let revision = p.get("revision").and_then(Value::as_u64).unwrap_or(0);
            let mut summary = if revision != 0
                && poll.last_revision.get(&pane_id).copied() == Some(revision)
            {
                poll.summaries
                    .get(&pane_id)
                    .cloned()
                    .unwrap_or_default()
            } else {
                let path = session_path.clone();
                let summary = tokio::task::spawn_blocking(move || omp::tail_summary(&path))
                    .await
                    .unwrap_or_default();
                poll.summaries.insert(pane_id.clone(), summary.clone());
                summary
            };

            if summary.model.is_none() || summary.effort.is_none() {
                let path = session_path.clone();
                summary = tokio::task::spawn_blocking(move || omp::tail_summary(&path))
                    .await
                    .unwrap_or(summary);
                poll.summaries.insert(pane_id.clone(), summary.clone());
            }
            poll.last_revision.insert(pane_id.clone(), revision);
            let updated_ms = if let Ok(meta) = tokio::fs::metadata(&session_path).await {
                poll.last_sizes.insert(pane_id.clone(), meta.len());
                meta.modified()
                    .ok()
                    .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
            } else {
                None
            };
            panes.push(Pane {
                pane_id,
                workspace_id: s(p, &["workspace_id"]).unwrap_or_default(),
                status: status_of(s(p, &["agent_status"]).as_deref()),
                task: s(p, &["terminal_title_stripped", "terminal_title"]),
                pending_ask: summary.pending_ask,
                session_path: Some(session_path),
                cwd: s(p, &["cwd", "foreground_cwd"]),
                snippet: summary.snippet,
                provider: summary.provider,
                model: summary.model,
                effort: summary.effort,
                updated_ms,
            });
        }
    }
    // Drop workspaces that no longer host any OMP pane.
    let live_ws: std::collections::HashSet<&str> =
        panes.iter().map(|p| p.workspace_id.as_str()).collect();
    let workspaces = workspaces
        .into_iter()
        .filter(|w| live_ws.contains(w.id.as_str()))
        .collect();
    Ok(Fleet {
        workspaces,
        panes,
    })
}

async fn poll_loop(app: AppHandle, shared: Shared) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    let mut poll = PollState::default();
    loop {
        interval.tick().await;
        let fleet = match build_fleet(&mut poll).await {
            Ok(fleet) => fleet,
            Err(e) => {
                let message = e.to_string();
                if poll.last_status.as_deref() != Some(message.as_str()) {
                    poll.last_status = Some(message.clone());
                    let _ = app.emit(
                        "herdr-status",
                        json!({ "ok": false, "message": message }),
                    );
                }
                continue;
            }
        };
        if poll.last_status.as_deref() != Some("connected") {
            poll.last_status = Some("connected".into());
            let _ = app.emit("herdr-status", json!({ "ok": true, "message": "connected" }));
        }

        // Drop state for panes that no longer exist.
        let live: HashSet<&str> = fleet.panes.iter().map(|p| p.pane_id.as_str()).collect();
        poll.last_revision.retain(|k, _| live.contains(k.as_str()));
        poll.summaries.retain(|k, _| live.contains(k.as_str()));
        poll.last_sizes.retain(|k, _| live.contains(k.as_str()));
        poll.last_sizes_emitted.retain(|k, _| live.contains(k.as_str()));

        // First tick: baseline sizes without refetching every open pane.
        if poll.last_sizes_emitted.is_empty() {
            poll.last_sizes_emitted = poll.last_sizes.clone();
        }

        for pane in &fleet.panes {
            if let Some(size) = poll.last_sizes.get(&pane.pane_id).copied() {
                if poll.last_sizes_emitted.get(&pane.pane_id).copied() != Some(size) {
                    let _ = app.emit("poke", json!({ "pane_id": pane.pane_id }));
                }
                poll.last_sizes_emitted.insert(pane.pane_id.clone(), size);
            }
        }

        let json = serde_json::to_string(&fleet).unwrap_or_default();
        if poll.last_fleet_json.as_deref() != Some(json.as_str()) {
            poll.last_fleet_json = Some(json);
            let _ = app.emit("fleet", &fleet);
        }
        *shared.fleet.lock().await = Some(fleet);
    }
}

// ── Commands ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct IndexedEntry {
    index: usize,
    #[serde(flatten)]
    entry: omp::Entry,
}

#[derive(Serialize)]
struct SessionPage {
    title: Option<String>,
    model: Option<omp::ModelInfo>,
    thinking: Option<String>,
    entries: Vec<IndexedEntry>,
    pending_ask: Option<omp::Ask>,
    total_entries: usize,
    has_older: bool,
}

#[tauri::command]
async fn fleet(state: State<'_, Shared>) -> Result<Fleet, String> {
    state
        .fleet
        .lock()
        .await
        .clone()
        .ok_or_else(|| "fleet not ready yet".to_string())
}

#[tauri::command]
async fn session(
    state: State<'_, Shared>,
    pane_id: String,
    before: Option<usize>,
) -> Result<SessionPage, String> {
    let path = {
        let fleet = state.fleet.lock().await;
        fleet
            .as_ref()
            .and_then(|f| f.panes.iter().find(|p| p.pane_id == pane_id))
            .and_then(|p| p.session_path.clone())
    };
    let Some(path) = path else {
        // Fleet only lists OMP panes; a missing path means the pane left the
        // fleet between the click and the load.
        return Err("pane left the fleet (no omp session)".into());
    };
    let projection = state.projection_for(&path).await;
    let parse_path = path.clone();
    let page = tokio::task::spawn_blocking(
        move || -> Result<
            (
                omp::Page,
                Option<String>,
                Option<omp::ModelInfo>,
                Option<String>,
                Option<omp::Ask>,
            ),
            String,
        > {
            let mut projection = projection.blocking_lock();
            projection
                .refresh(&parse_path)
                .map_err(|e| format!("read session: {e}"))?;
            let page = projection.page(before, SESSION_PAGE_LIMIT);
            let title = projection.title.clone();
            let model = projection.model.clone();
            let thinking = projection.thinking.clone();
            let pending_ask = projection.pending_ask.clone();
            Ok((page, title, model, thinking, pending_ask))
        },
    )
    .await
    .map_err(|e| format!("parse session: {e}"))??;

    let (page, title, model, thinking, pending_ask) = page;
    Ok(SessionPage {
        title,
        model,
        thinking,
        entries: page
            .entries
            .into_iter()
            .map(|(index, entry)| IndexedEntry { index, entry })
            .collect(),
        pending_ask,
        total_entries: page.total,
        has_older: page.has_older,
    })
}

#[tauri::command]
async fn send_text(pane_id: String, text: String) -> Result<(), String> {
    herdr::send_text(&pane_id, &text).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn send_keys(pane_id: String, keys: Vec<String>) -> Result<(), String> {
    herdr::send_keys(&pane_id, &keys).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn read_screen(pane_id: String) -> Result<String, String> {
    herdr::read_screen(&pane_id).await.map_err(|e| e.to_string())
}

fn unframed_row(line: &str) -> &str {
    let row = line.trim();
    let row = row
        .strip_prefix('│')
        .or_else(|| row.strip_prefix('|'))
        .unwrap_or(row)
        .trim_start();
    row.strip_suffix('│')
        .or_else(|| row.strip_suffix('|'))
        .unwrap_or(row)
        .trim_end()
}

/// Locate the currently focused option index inside omp's ask picker,
/// mirroring the bridge's `focused_ask_index` screen heuristics.
fn focused_ask_index(screen: &str, ask: &omp::Ask) -> Option<usize> {
    let ask_visible = screen.lines().any(|line| {
        let line = line.trim();
        (line.starts_with('╭') || line.starts_with('+')) && line.contains(" Ask")
    });
    if !ask_visible {
        return None;
    }
    screen.lines().find_map(|line| {
        let row = unframed_row(line);
        let cursor = row.chars().next()?;
        if !matches!(cursor, '❯' | '>' | '\u{f054}') {
            return None;
        }
        let row = row[cursor.len_utf8()..].trim_start();
        let mut chars = row.chars();
        chars.next()?;
        let label = chars.as_str().trim();
        let label = label.strip_suffix(" (Recommended)").unwrap_or(label);
        ask.options.iter().position(|option| option.label == label)
    })
}

async fn wait_screen_focus(pane_id: &str, ask: &omp::Ask, want: usize) -> Result<bool> {
    for _ in 0..ASK_FOCUS_ATTEMPTS {
        match herdr::read_screen(pane_id).await {
            Ok(screen) if focused_ask_index(&screen, ask) == Some(want) => return Ok(true),
            Ok(_) => tokio::time::sleep(Duration::from_millis(ASK_FOCUS_POLL_MS)).await,
            Err(e) => return Err(e),
        }
    }
    Ok(false)
}

fn duplicate_ask_option_label<'a>(labels: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let mut seen = HashSet::new();
    labels.into_iter().find(|label| !seen.insert(*label))
}

/// Answer a pending single-select ask by driving omp's TUI picker:
/// verify focus, navigate with Down/Up, confirm with Enter. No receipt
/// scan yet (the transcript poke confirms completion).
#[tauri::command]
async fn answer_ask(
    state: State<'_, Shared>,
    pane_id: String,
    call_id: String,
    index: usize,
) -> Result<(), String> {
    let path = {
        let fleet = state.fleet.lock().await;
        fleet
            .as_ref()
            .and_then(|f| f.panes.iter().find(|p| p.pane_id == pane_id))
            .and_then(|p| p.session_path.clone())
    };
    let Some(path) = path else {
        return Err("pane has no omp transcript".into());
    };
    let projection = state.projection_for(&path).await;
    let parse_path = path.clone();
    let ask = tokio::task::spawn_blocking(move || -> Option<omp::Ask> {
        let mut projection = projection.blocking_lock();
        projection.refresh(&parse_path).ok()?;
        projection.pending_ask.clone()
    })
    .await
    .unwrap_or(None);
    let Some(ask) = ask else {
        return Err("ask is no longer pending".into());
    };
    if call_id.is_empty() || ask.call_id != call_id {
        return Err("ask identity is no longer current".into());
    }
    if ask.multi {
        return Err("multi-select asks are not supported yet".into());
    }
    if duplicate_ask_option_label(ask.options.iter().map(|option| option.label.as_str())).is_some()
    {
        return Err("ask option labels are not unique; use the raw terminal to recover".into());
    }
    let Some(_option) = ask.options.get(index) else {
        return Err("option index out of range".into());
    };

    let claimed = state.pane_locks.lock().await.insert(pane_id.clone());
    if !claimed {
        return Err("another pane write is in progress".into());
    }
    let result = async {
        let screen = herdr::read_screen(&pane_id)
            .await
            .map_err(|e| format!("read screen: {e}"))?;
        let start = focused_ask_index(&screen, &ask)
            .ok_or_else(|| "ask picker is not focused on screen".to_string())?;
        let (direction, count) = if index >= start {
            ("Down", index - start)
        } else {
            ("Up", start - index)
        };
        for _ in 0..count {
            herdr::send_keys(&pane_id, &[direction.to_string()])
                .await
                .map_err(|e| e.to_string())?;
        }
        if !wait_screen_focus(&pane_id, &ask, index)
            .await
            .map_err(|e| e.to_string())?
        {
            return Err("ask picker did not focus the requested option".into());
        }
        herdr::send_keys(&pane_id, &["Enter".to_string()])
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    .await;
    state.pane_locks.lock().await.remove(&pane_id);
    result
}

async fn run_omp(args: &[&str]) -> Result<Value, String> {
    let mut command = tokio::process::Command::new("omp");
    command.args(args).kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(45), command.output())
        .await
        .map_err(|_| "omp call timed out".to_string())?
        .map_err(|e| format!("omp failed: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("parse omp output: {e}"))
}

#[tauri::command]
async fn usage() -> Result<Value, String> {
    run_omp(&["usage", "--json", "--redact"]).await
}

#[tauri::command]
async fn models() -> Result<Value, String> {
    let raw = run_omp(&["models", "--json"]).await?;
    // Pass through only the fields the UI needs. Drop cost/context noise.
    let Some(arr) = raw.get("models").and_then(Value::as_array) else {
        return Ok(serde_json::json!({ "models": [] }));
    };
    let models: Vec<Value> = arr
        .iter()
        .filter_map(|m| {
            let provider = m.get("provider")?.as_str()?;
            let id = m.get("id")?.as_str()?;
            let selector = m
                .get("selector")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{provider}/{id}"));
            let name = m
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_string();
            let thinking = match m.get("thinking") {
                Some(Value::Array(levels)) => Value::Array(
                    levels
                        .iter()
                        .filter_map(|l| l.as_str().map(|s| Value::String(s.to_string())))
                        .collect(),
                ),
                _ => Value::Null,
            };
            Some(serde_json::json!({
                "provider": provider,
                "id": id,
                "selector": selector,
                "name": name,
                "thinking": thinking,
            }))
        })
        .collect();
    Ok(serde_json::json!({ "models": models }))
}

#[tauri::command]
fn commands(cwd: Option<String>) -> Value {
    catalog::build(cwd.as_deref())
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    // Agent-controlled hrefs: only open web schemes. DOMPurify already blocks
    // javascript:, but file:/custom schemes must not reach xdg-open.
    let scheme = url.split_once(':').map(|(s, _)| s).unwrap_or("");
    if !matches!(scheme, "http" | "https") {
        return Err("refusing to open non-http(s) URL".into());
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
fn set_window_opacity(window: tauri::Window, opacity: f64) -> Result<(), String> {
    let opacity = opacity.clamp(0.1, 1.0);
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::WidgetExt;
        let gtk_win = window.gtk_window().map_err(|e| e.to_string())?;
        gtk_win.set_opacity(opacity);
    }
    let _ = window;
    let _ = opacity;
    Ok(())
}

pub fn run() {
    let shared: Shared = Arc::new(state::AppState::default());
    tauri::Builder::default()
        .manage(shared.clone())
        .invoke_handler(tauri::generate_handler![
            fleet,
            session,
            send_text,
            send_keys,
            read_screen,
            answer_ask,
            usage,
            models,
            commands,
            open_url,
            set_window_opacity
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(poll_loop(handle, shared));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running kelpie desktop");
}
