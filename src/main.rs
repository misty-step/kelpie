//! kelpie — mobile bridge for herdr + omp agents.
//!
//! Fleet state comes from herdr's `session.snapshot`; transcripts come from
//! omp's session JSONL files (pane records carry the exact path); input goes
//! back through herdr `pane.send_text` / `pane.send_keys`. No ANSI parsing.

mod herdr;
mod omp;

use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::services::ServeDir;

const BIND: &str = "127.0.0.1:8787";
const POLL_MS: u64 = 600;

// ---------------------------------------------------------------- fleet model

#[derive(Serialize, Clone, PartialEq)]
struct FleetPane {
    pane_id: String,
    workspace_id: String,
    tab_id: String,
    cwd: String,
    agent: Option<String>,
    status: Option<String>,
    title: Option<String>,
    has_transcript: bool,
    pending_ask: bool,
    last_activity: Option<String>,
    snippet: Option<String>,
    #[serde(skip)]
    session_path: Option<String>,
}

#[derive(Serialize, Clone, PartialEq, Default)]
struct Fleet {
    workspaces: Vec<Value>,
    tabs: Vec<Value>,
    panes: Vec<FleetPane>,
}

#[derive(Default)]
struct AppState {
    fleet: RwLock<Fleet>,
    pokes: Option<broadcast::Sender<String>>,
    /// Panes with an in-flight TUI drive (reasoning cycle or model picker) —
    /// both steer the same terminal, so one guard covers both.
    pane_locks: Mutex<HashSet<String>>,
}

type Shared = Arc<AppState>;

fn s(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| v.get(k).and_then(Value::as_str))
        .map(String::from)
}

fn mtime_iso(path: &str) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let t = meta.modified().ok()?;
    let d = t.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    // RFC3339 without pulling in chrono: seconds precision is plenty.
    let secs = d.as_secs() as i64;
    let days = secs / 86400;
    let (mut y, mut rem_days) = (1970i64, days);
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let len = if leap { 366 } else { 365 };
        if rem_days < len {
            break;
        }
        rem_days -= len;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let months = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0usize;
    while rem_days >= months[m] {
        rem_days -= months[m];
        m += 1;
    }
    let tod = secs % 86400;
    Some(format!(
        "{y:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        m + 1,
        rem_days + 1,
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    ))
}

// -------------------------------------------------------------- fleet builder

async fn build_fleet() -> Result<Fleet> {
    let snap = herdr::snapshot().await?;
    let workspaces = snap
        .get("workspaces")
        .and_then(Value::as_array)
        .map(|ws| {
            ws.iter()
                .map(|w| {
                    json!({
                        "id": s(w, &["workspace_id", "id"]),
                        "label": s(w, &["label", "name", "title"]),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut panes = Vec::new();
    for p in snap
        .get("panes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(pane_id) = s(p, &["pane_id", "id"]) else {
            continue;
        };
        let agent = s(p, &["agent"]);
        let session_path = p
            .get("agent_session")
            .filter(|a| a.get("kind").and_then(Value::as_str) == Some("path"))
            .and_then(|a| a.get("value"))
            .and_then(Value::as_str)
            .map(String::from);

        let (title, snippet, pending_ask) = match &session_path {
            Some(path) => {
                let sum = omp::summarize(path);
                (sum.title, sum.snippet, sum.pending_ask)
            }
            None => (None, None, false),
        };

        panes.push(FleetPane {
            pane_id,
            workspace_id: s(p, &["workspace_id"]).unwrap_or_default(),
            tab_id: s(p, &["tab_id"]).unwrap_or_default(),
            cwd: s(p, &["foreground_cwd", "cwd"]).unwrap_or_default(),
            agent,
            status: s(p, &["agent_status"]),
            title,
            has_transcript: session_path.is_some(),
            pending_ask,
            last_activity: session_path.as_deref().and_then(mtime_iso),
            snippet,
            session_path,
        });
    }
    let tabs = snap
        .get("tabs")
        .and_then(Value::as_array)
        .map(|ts| {
            ts.iter()
                .filter_map(|t| {
                    let tab_id = s(t, &["tab_id", "id"])?;
                    let pane_ids: Vec<String> = panes
                        .iter()
                        .filter(|p| p.tab_id == tab_id)
                        .map(|p| p.pane_id.clone())
                        .collect();
                    Some(json!({
                        "tab_id": tab_id,
                        "workspace_id": s(t, &["workspace_id"]),
                        "label": s(t, &["label", "name", "title"]),
                        "pane_ids": pane_ids,
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(Fleet {
        workspaces,
        tabs,
        panes,
    })
}

async fn refresher(state: Shared) {
    let mut file_sizes: HashMap<String, u64> = HashMap::new();
    loop {
        match build_fleet().await {
            Ok(new_fleet) => {
                let changed = {
                    let cur = state.fleet.read().await;
                    *cur != new_fleet
                };
                // Per-session file growth -> poke that pane's open view.
                let mut session_pokes = Vec::new();
                for p in &new_fleet.panes {
                    if let Some(path) = &p.session_path {
                        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                        let prev = file_sizes.insert(path.clone(), size);
                        if prev.is_some_and(|old| old != size) {
                            session_pokes.push(p.pane_id.clone());
                        }
                    }
                }
                if changed {
                    *state.fleet.write().await = new_fleet;
                    if let Some(tx) = &state.pokes {
                        let _ = tx.send(json!({"type": "fleet"}).to_string());
                    }
                }
                if let Some(tx) = &state.pokes {
                    for pane_id in session_pokes {
                        let _ = tx.send(json!({"type": "session", "pane_id": pane_id}).to_string());
                    }
                }
            }
            Err(err) => eprintln!("[kelpie] fleet refresh failed: {err:#}"),
        }
        tokio::time::sleep(Duration::from_millis(POLL_MS)).await;
    }
}

// ------------------------------------------------------------------- handlers

async fn get_fleet(State(state): State<Shared>) -> Json<Fleet> {
    Json(state.fleet.read().await.clone())
}

async fn session_path_for(state: &Shared, pane_id: &str) -> Option<String> {
    state
        .fleet
        .read()
        .await
        .panes
        .iter()
        .find(|p| p.pane_id == pane_id)
        .and_then(|p| p.session_path.clone())
}

async fn get_session(
    State(state): State<Shared>,
    Path(pane_id): Path<String>,
) -> impl IntoResponse {
    match session_path_for(&state, &pane_id).await {
        Some(path) => {
            let t = tokio::task::spawn_blocking(move || omp::parse_session(&path))
                .await
                .unwrap_or_default();
            Json(serde_json::to_value(t).unwrap_or(Value::Null)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no transcript for pane"})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct TextBody {
    text: String,
}

fn input_marker(text: &str) -> String {
    let line = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(text);
    let mut chars = line.chars().rev().take(18).collect::<Vec<_>>();
    chars.reverse();
    chars.into_iter().collect()
}

async fn post_text(
    State(state): State<Shared>,
    Path(pane_id): Path<String>,
    Json(body): Json<TextBody>,
) -> impl IntoResponse {
    let claimed = state.pane_locks.lock().await.insert(pane_id.clone());
    if !claimed {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "another pane write is in progress"})),
        )
            .into_response();
    }
    let run = async {
        let before = screen_text(&pane_id).await?;
        let marker = input_marker(&body.text);
        herdr::send_text(&pane_id, &body.text).await?;
        let mut ready = false;
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let screen = screen_text(&pane_id).await?;
            let marker_visible = marker.is_empty()
                || screen
                    .lines()
                    .rev()
                    .take(24)
                    .any(|line| line.contains(&marker));
            if screen != before && marker_visible {
                ready = true;
                break;
            }
        }
        if !ready {
            return Err(anyhow::anyhow!("composer did not accept text"));
        }
        tokio::time::sleep(Duration::from_millis(800)).await;
        let before_submit = screen_text(&pane_id).await?;
        herdr::send_keys(&pane_id, &["Enter".to_string()]).await?;
        tokio::time::sleep(Duration::from_millis(1200)).await;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if screen_text(&pane_id).await? != before_submit {
                return Ok(());
            }
        }
        Err(anyhow::anyhow!("composer did not submit text"))
    };
    let result = run.await;
    state.pane_locks.lock().await.remove(&pane_id);
    match result {
        Ok(()) => Json(json!({"ok": true})).into_response(),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": err.to_string()})),
        )
            .into_response(),
    }
}

/// Accept a raw image body, persist it to a temp uploads dir, and return the
/// absolute path. The client then references that path in the message text;
/// omp's read tool decodes images natively.
async fn post_upload(
    Path(pane_id): Path<String>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let _ = pane_id; // per-pane route shape kept for symmetry; uploads are global
    if body.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "empty body"})),
        )
            .into_response();
    }
    let ct = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");
    let ext = match ct {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/heic" | "image/heif" => "heic",
        _ => "bin",
    };
    let dir = std::env::temp_dir().join("kelpie-uploads");
    if let Err(err) = std::fs::create_dir_all(&dir) {
        return err_json(err.into());
    }
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("img-{stamp}.{ext}"));
    if let Err(err) = std::fs::write(&path, &body) {
        return err_json(err.into());
    }
    Json(json!({ "path": path.to_string_lossy() })).into_response()
}

#[derive(Deserialize)]
struct KeysBody {
    keys: Vec<String>,
}

/// Send named keys to a pane. One translation: herdr accepts "shift+tab"
/// but its encoding never reaches omp's `app.thinking.cycle` binding
/// (verified live: named key = no-op, raw CSI Z cycles). Send the standard
/// back-tab sequence as literal text instead.
async fn post_keys(
    State(state): State<Shared>,
    Path(pane_id): Path<String>,
    Json(body): Json<KeysBody>,
) -> impl IntoResponse {
    let claimed = state.pane_locks.lock().await.insert(pane_id.clone());
    if !claimed {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "another pane write is in progress"})),
        )
            .into_response();
    }
    let run = async {
        for key in &body.keys {
            if key.eq_ignore_ascii_case("shift+tab") {
                herdr::send_text(&pane_id, "\x1b[Z").await?;
            } else {
                herdr::send_keys(&pane_id, std::slice::from_ref(key)).await?;
            }
        }
        Ok::<(), anyhow::Error>(())
    };
    let result = run.await;
    state.pane_locks.lock().await.remove(&pane_id);
    match result {
        Ok(()) => Json(json!({"ok": true})).into_response(),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": err.to_string()})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct ThinkingBody {
    thinking: String,
}

fn canonical_thinking_level(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "off" => Some("off"),
        "auto" => Some("auto"),
        "minimal" | "min" => Some("minimal"),
        "low" => Some("low"),
        "medium" | "med" => Some("medium"),
        "high" => Some("high"),
        "xhigh" | "extra high" => Some("xhigh"),
        "max" => Some("max"),
        _ => None,
    }
}

#[derive(Clone, PartialEq)]
struct ThinkingReceipt {
    id: String,
    level: &'static str,
}

async fn latest_thinking_receipt(path: &str) -> Option<ThinkingReceipt> {
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        let raw = std::fs::read_to_string(path).ok()?;
        raw.lines().rev().find_map(|line| {
            let event: Value = serde_json::from_str(line).ok()?;
            if event.get("type").and_then(Value::as_str) != Some("thinking_level_change") {
                return None;
            }
            let level = event
                .get("configured")
                .and_then(Value::as_str)
                .or_else(|| event.get("thinkingLevel").and_then(Value::as_str))
                .and_then(canonical_thinking_level)
                .or_else(|| {
                    (event.get("configured").is_some() || event.get("thinkingLevel").is_some())
                        .then_some("off")
                })?;
            let id = event
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| event.get("timestamp").and_then(Value::as_str))?
                .to_string();
            Some(ThinkingReceipt { id, level })
        })
    })
    .await
    .ok()
    .flatten()
}

fn screen_thinking_level(screen: &str) -> Option<&'static str> {
    screen.lines().rev().take(8).find_map(|line| {
        if line.matches('·').count() < 3 {
            return None;
        }
        line.split('·')
            .nth(1)?
            .split(|c: char| !c.is_ascii_alphabetic())
            .find_map(canonical_thinking_level)
    })
}

fn has_new_auto_status(before: &str, after: &str) -> bool {
    screen_thinking_level(before) != Some("auto") && screen_thinking_level(after) == Some("auto")
}

/// Select an exact reasoning effort through omp's cycle key, confirming each
/// concrete transition from the session log. Auto may be logged directly; on
/// older sessions the newly rendered status line is the fallback receipt.
async fn drive_thinking(
    pane_id: &str,
    path: &str,
    target: &'static str,
) -> std::result::Result<&'static str, (StatusCode, String)> {
    let screen = screen_text(pane_id)
        .await
        .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
    let live = screen_thinking_level(&screen);
    let mut previous = latest_thinking_receipt(path).await;
    let configured_auto = live == Some("auto")
        || previous
            .as_ref()
            .is_some_and(|receipt| receipt.level == "auto");
    if configured_auto && target == "auto" {
        return Ok(target);
    }
    if !configured_auto
        && live == Some(target)
        && previous
            .as_ref()
            .is_some_and(|receipt| receipt.level == target)
    {
        return Ok(target);
    }

    for _ in 0..16 {
        herdr::send_text(pane_id, "\x1b[Z")
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
        tokio::time::sleep(Duration::from_millis(800)).await;
        let next = latest_thinking_receipt(path).await;
        if next == previous {
            continue;
        }
        let Some(receipt) = next else {
            continue;
        };
        previous = Some(receipt.clone());
        if receipt.level == target {
            return Ok(target);
        }
        if target == "auto" && receipt.level == "off" {
            let before = screen_text(pane_id)
                .await
                .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
            herdr::send_text(pane_id, "\x1b[Z")
                .await
                .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
            let confirmed = wait_screen(pane_id, 20, |screen| has_new_auto_status(&before, screen))
                .await
                .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
            let after = latest_thinking_receipt(path).await;
            if confirmed.is_some() || after.as_ref().is_some_and(|value| value.level == target) {
                return Ok(target);
            }
            return Err((
                StatusCode::BAD_GATEWAY,
                "auto was selected but the terminal did not expose a confirmation".to_string(),
            ));
        }
    }
    Err((
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("reasoning effort {target} is unavailable for this model"),
    ))
}

/// Select an exact reasoning effort through omp's runtime cycle key.
async fn post_thinking(
    State(state): State<Shared>,
    Path(pane_id): Path<String>,
    Json(body): Json<ThinkingBody>,
) -> impl IntoResponse {
    let Some(target) = canonical_thinking_level(&body.thinking) else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "unknown reasoning effort"})),
        )
            .into_response();
    };
    let claimed = state.pane_locks.lock().await.insert(pane_id.clone());
    if !claimed {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "another pane write is in progress"})),
        )
            .into_response();
    }

    let result = async {
        let path = session_path_for(&state, &pane_id)
            .await
            .ok_or_else(|| (StatusCode::NOT_FOUND, "no transcript for pane".to_string()))?;
        drive_thinking(&pane_id, &path, target).await
    }
    .await;
    state.pane_locks.lock().await.remove(&pane_id);
    match result {
        Ok(thinking) => Json(json!({"ok": true, "thinking": thinking})).into_response(),
        Err((status, message)) => (status, Json(json!({"error": message}))).into_response(),
    }
}

#[derive(Deserialize)]
struct ModelBody {
    model: String,
    thinking: Option<String>,
}

/// Read a pane's visible screen as trimmed plain text (drive verification).
async fn screen_text(pane_id: &str) -> Result<String> {
    let res = herdr::rpc(
        "pane.read",
        json!({ "pane_id": pane_id, "source": "visible", "lines": 200, "format": "text" }),
    )
    .await?;
    Ok(res
        .get("read")
        .and_then(|r| r.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string())
}

async fn type_chars(pane_id: &str, text: &str) -> Result<()> {
    let keys: Vec<String> = text
        .chars()
        .map(|c| {
            if c == ' ' {
                "Space".to_string()
            } else {
                c.to_string()
            }
        })
        .collect();
    herdr::send_keys(pane_id, &keys).await
}

async fn key(pane_id: &str, name: &str, settle_ms: u64) -> Result<()> {
    herdr::send_keys(pane_id, &[name.to_string()]).await?;
    tokio::time::sleep(Duration::from_millis(settle_ms)).await;
    Ok(())
}
async fn wait_screen<F>(pane_id: &str, attempts: usize, predicate: F) -> Result<Option<String>>
where
    F: Fn(&str) -> bool,
{
    for _ in 0..attempts {
        let screen = screen_text(pane_id).await?;
        if predicate(&screen) {
            return Ok(Some(screen));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(None)
}

#[derive(Clone, PartialEq)]
struct ModelReceipt {
    id: String,
    selector: String,
}

async fn latest_model_receipt(path: &str) -> Option<ModelReceipt> {
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        let raw = std::fs::read_to_string(path).ok()?;
        raw.lines().rev().find_map(|line| {
            let event: Value = serde_json::from_str(line).ok()?;
            if event.get("type").and_then(Value::as_str) != Some("model_change") {
                return None;
            }
            let selector = event.get("model").and_then(Value::as_str)?.to_string();
            let id = event
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| event.get("timestamp").and_then(Value::as_str))?
                .to_string();
            Some(ModelReceipt { id, selector })
        })
    })
    .await
    .ok()
    .flatten()
}

async fn wait_model_receipt(
    path: &str,
    previous: Option<&ModelReceipt>,
    selector: &str,
) -> Option<ModelReceipt> {
    for _ in 0..50 {
        if let Some(receipt) = latest_model_receipt(path).await {
            if previous.is_none_or(|old| old.id != receipt.id) && receipt.selector == selector {
                return Some(receipt);
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

/// Switch the active session model through omp's temporary `/switch` picker.
/// Role-model settings stay unchanged; picker focus and the resulting
/// `model_change` session receipt are both verified.
async fn post_model(
    State(state): State<Shared>,
    Path(pane_id): Path<String>,
    Json(body): Json<ModelBody>,
) -> impl IntoResponse {
    let selector = body.model.trim().to_string();
    if selector.is_empty()
        || selector.len() > 120
        || !selector.contains('/')
        || selector
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "model must be a provider/id selector"})),
        )
            .into_response();
    }
    let requested_thinking = match body.thinking.as_deref() {
        Some(raw) => match canonical_thinking_level(raw) {
            Some(level) => Some(level),
            None => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({"error": "unknown reasoning effort"})),
                )
                    .into_response();
            }
        },
        None => None,
    };
    let is_omp_pane = state
        .fleet
        .read()
        .await
        .panes
        .iter()
        .any(|p| p.pane_id == pane_id && p.agent.is_some());
    if !is_omp_pane {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "pane is not an agent pane"})),
        )
            .into_response();
    }
    let claimed = state.pane_locks.lock().await.insert(pane_id.clone());
    if !claimed {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "another control change is in progress on this pane"})),
        )
            .into_response();
    }
    let result = async {
        let path = session_path_for(&state, &pane_id)
            .await
            .ok_or_else(|| (StatusCode::NOT_FOUND, "no transcript for pane".to_string()))?;
        let previous_model = latest_model_receipt(&path).await;
        let previous_thinking = latest_thinking_receipt(&path).await;
        let screen = screen_text(&pane_id)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
        let preserved_thinking = requested_thinking.or_else(|| {
            if screen_thinking_level(&screen) == Some("auto") {
                Some("auto")
            } else {
                previous_thinking.as_ref().map(|receipt| receipt.level)
            }
        });
        drive_model_picker(&pane_id, &selector).await?;
        wait_model_receipt(&path, previous_model.as_ref(), &selector)
            .await
            .ok_or_else(|| {
                (
                    StatusCode::BAD_GATEWAY,
                    "switch sent but no matching session receipt was recorded".to_string(),
                )
            })?;
        // Omp re-applies a model-specific thinking setting immediately after
        // `model_change`. Wait for that receipt before restoring the caller's
        // prior configured level, or a late reapply can overwrite our restore.
        if preserved_thinking.is_some() {
            for _ in 0..15 {
                if latest_thinking_receipt(&path).await != previous_thinking {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        if let Some(thinking) = preserved_thinking {
            drive_thinking(&pane_id, &path, thinking).await?;
        }
        Ok(preserved_thinking)
    }
    .await;
    state.pane_locks.lock().await.remove(&pane_id);
    match result {
        Ok(thinking) => {
            Json(json!({"ok": true, "model": selector, "thinking": thinking})).into_response()
        }
        Err((status, msg)) => (status, Json(json!({"error": msg}))).into_response(),
    }
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

fn selected_model_row(screen: &str, selector: &str) -> bool {
    screen.lines().any(|line| {
        let row = unframed_row(line);
        let Some(row) = row
            .strip_prefix('❯')
            .or_else(|| row.strip_prefix('>'))
            .or_else(|| row.strip_prefix('\u{f054}'))
        else {
            return false;
        };
        let row = row.trim_start();
        row.strip_prefix(selector).is_some_and(|tail| {
            tail.is_empty() || tail.chars().next().is_some_and(char::is_whitespace)
        })
    })
}

async fn drive_model_picker(
    pane_id: &str,
    selector: &str,
) -> std::result::Result<(), (StatusCode, String)> {
    let gateway = |error: anyhow::Error| (StatusCode::BAD_GATEWAY, error.to_string());

    key(pane_id, "ctrl+u", 250).await.map_err(gateway)?;
    type_chars(pane_id, "/switch").await.map_err(gateway)?;
    let palette = wait_screen(pane_id, 30, |screen| {
        screen.contains("❯ /switch") && screen.contains("switch") && screen.contains("Model: ")
    })
    .await
    .map_err(gateway)?;
    if palette.is_none() {
        let _ = key(pane_id, "ctrl+u", 100).await;
        return Err((
            StatusCode::BAD_GATEWAY,
            "switch command palette did not open".into(),
        ));
    }

    key(pane_id, "Enter", 800).await.map_err(gateway)?;
    let picker = wait_screen(pane_id, 30, |screen| {
        screen.contains("Switch Model") && screen.contains("Session-only switch")
    })
    .await
    .map_err(gateway)?;
    if picker.is_none() {
        let _ = key(pane_id, "Escape", 100).await;
        return Err((
            StatusCode::BAD_GATEWAY,
            "temporary model picker did not open".into(),
        ));
    }

    type_chars(pane_id, selector).await.map_err(gateway)?;
    let matched = wait_screen(pane_id, 30, |screen| selected_model_row(screen, selector))
        .await
        .map_err(gateway)?;
    if matched.is_none() {
        for _ in 0..2 {
            let open = screen_text(pane_id)
                .await
                .map(|screen| screen.contains("Switch Model"))
                .unwrap_or(false);
            if !open {
                break;
            }
            let _ = key(pane_id, "Escape", 250).await;
        }
        return Err((
            StatusCode::NOT_FOUND,
            "model not available in this session (provider not configured?)".into(),
        ));
    }

    key(pane_id, "Enter", 1000).await.map_err(gateway)?;
    Ok(())
}

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

async fn wait_ask_selection(
    path: &str,
    call_id: &str,
) -> Option<std::result::Result<Vec<String>, String>> {
    for _ in 0..50 {
        let path = path.to_string();
        let call_id = call_id.to_string();
        let receipt = tokio::task::spawn_blocking(move || {
            let raw = std::fs::read_to_string(path).ok()?;
            raw.lines().rev().find_map(|line| {
                let event: Value = serde_json::from_str(line).ok()?;
                if event.get("type").and_then(Value::as_str) != Some("message") {
                    return None;
                }
                let message = event.get("message")?;
                if message.get("role").and_then(Value::as_str) != Some("toolResult")
                    || message.get("toolCallId").and_then(Value::as_str) != Some(call_id.as_str())
                {
                    return None;
                }
                if message
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    return Some(Err("ask tool returned an error".to_string()));
                }
                let selected = message
                    .get("details")
                    .and_then(|details| details.get("selectedOptions"))
                    .and_then(Value::as_array)?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect::<Vec<_>>();
                Some(Ok(selected))
            })
        })
        .await
        .ok()
        .flatten();
        if receipt.is_some() {
            return receipt;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

#[derive(Deserialize)]
struct AskBody {
    index: usize,
}

/// Answer a pending single-select ask from the currently rendered focus row,
/// then confirm the exact option against the correlated ask tool result.
async fn post_ask(
    State(state): State<Shared>,
    Path(pane_id): Path<String>,
    Json(body): Json<AskBody>,
) -> impl IntoResponse {
    let claimed = state.pane_locks.lock().await.insert(pane_id.clone());
    if !claimed {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "another pane write is in progress"})),
        )
            .into_response();
    }

    let run = async {
        let path = session_path_for(&state, &pane_id)
            .await
            .ok_or_else(|| (StatusCode::NOT_FOUND, "no transcript for pane".to_string()))?;
        let parse_path = path.clone();
        let transcript = tokio::task::spawn_blocking(move || omp::parse_session(&parse_path))
            .await
            .unwrap_or_default();
        let ask = transcript
            .pending_ask
            .ok_or_else(|| (StatusCode::CONFLICT, "ask no longer pending".to_string()))?;
        if ask.multi {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                "multi-select not supported yet; use keys".to_string(),
            ));
        }
        let Some(option) = ask.options.get(body.index) else {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                "option index out of range".to_string(),
            ));
        };
        if ask.call_id.is_empty() {
            return Err((
                StatusCode::BAD_GATEWAY,
                "pending ask has no tool-call identity".to_string(),
            ));
        }

        let screen = screen_text(&pane_id)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
        let start = focused_ask_index(&screen, &ask).ok_or_else(|| {
            (
                StatusCode::CONFLICT,
                "ask picker focus could not be verified".to_string(),
            )
        })?;
        let (direction, count) = if body.index >= start {
            ("Down", body.index - start)
        } else {
            ("Up", start - body.index)
        };
        for _ in 0..count {
            herdr::send_keys(&pane_id, &[direction.to_string()])
                .await
                .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
        }
        let focused = wait_screen(&pane_id, 30, |screen| {
            focused_ask_index(screen, &ask) == Some(body.index)
        })
        .await
        .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
        if focused.is_none() {
            return Err((
                StatusCode::CONFLICT,
                "ask picker did not focus the requested option".to_string(),
            ));
        }
        herdr::send_keys(&pane_id, &["Enter".to_string()])
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;

        let selected = wait_ask_selection(&path, &ask.call_id)
            .await
            .ok_or_else(|| {
                (
                    StatusCode::BAD_GATEWAY,
                    "ask selection was not recorded".to_string(),
                )
            })?
            .map_err(|message| (StatusCode::BAD_GATEWAY, message))?;
        if selected.as_slice() != [option.label.as_str()] {
            return Err((
                StatusCode::BAD_GATEWAY,
                "ask selection receipt did not match the requested option".to_string(),
            ));
        }
        Ok(())
    }
    .await;

    state.pane_locks.lock().await.remove(&pane_id);
    match run {
        Ok(()) => Json(json!({"ok": true})).into_response(),
        Err((status, message)) => (status, Json(json!({"error": message}))).into_response(),
    }
}

async fn sse_events(
    State(state): State<Shared>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = state.pokes.as_ref().expect("pokes wired").subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(data) => Some(Ok(Event::default().data(data))),
        Err(_) => None, // lagged receiver: drop, client refetches on next poke
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)))
}

// ------------------------------------------------------------ v2: omp + herdr

/// omp builtin slash commands, extracted from the omp source registry at
/// build time (src/commands.json). Static per omp version.
async fn get_commands() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        include_str!("commands.json"),
    )
}

/// Available models from `omp models --json` (registry of authenticated
/// providers). Slow (~2-3s subprocess), so cached for the bridge lifetime —
/// the catalog only changes with omp upgrades or new provider auth.
async fn get_models() -> impl IntoResponse {
    use tokio::sync::OnceCell;
    static MODELS: OnceCell<Option<String>> = OnceCell::const_new();
    let cached = MODELS
        .get_or_init(|| async {
            let out = tokio::process::Command::new("omp")
                .args(["models", "--json"])
                .output()
                .await
                .ok()?;
            if !out.status.success() {
                return None;
            }
            let body = String::from_utf8(out.stdout).ok()?;
            // sanity: must parse as JSON with a models array
            let v: Value = serde_json::from_str(&body).ok()?;
            v.get("models")?.as_array()?;
            Some(body)
        })
        .await;
    match cached {
        Some(body) => (
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            body.clone(),
        )
            .into_response(),
        None => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "omp models unavailable"})),
        )
            .into_response(),
    }
}

/// Plain-text visible screen of any pane (herdr strips ANSI with
/// `format: "text"` — safe to render, no XSS surface).
async fn get_screen(Path(pane_id): Path<String>) -> impl IntoResponse {
    match herdr::rpc(
        "pane.read",
        json!({ "pane_id": pane_id, "source": "visible", "lines": 200, "format": "text" }),
    )
    .await
    {
        Ok(res) => {
            let raw = res
                .get("read")
                .and_then(|r| r.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            // herdr pads every line to the pane's PTY width (e.g. 160 cols of
            // trailing spaces on a bare prompt) — strip so phones don't get
            // horizontal scroll for content that actually fits.
            let text = raw
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n");
            Json(json!({ "text": text })).into_response()
        }
        Err(err) if err.to_string().contains("pane_not_found") => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "pane_not_found" })),
        )
            .into_response(),
        Err(err) => err_json(err),
    }
}

fn err_json(err: anyhow::Error) -> axum::response::Response {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({ "error": err.to_string() })),
    )
        .into_response()
}

fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        return format!("{home}/{rest}");
    }
    path.to_string()
}

#[derive(Deserialize)]
struct WorkspaceBody {
    cwd: String,
    label: Option<String>,
}

async fn post_workspace(Json(body): Json<WorkspaceBody>) -> impl IntoResponse {
    let cwd = expand_home(body.cwd.trim());
    if !std::path::Path::new(&cwd).is_dir() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": format!("not a directory: {cwd}") })),
        )
            .into_response();
    }
    match herdr::rpc("workspace.create", json!({ "cwd": cwd })).await {
        Ok(res) => {
            let workspace_id = res
                .get("workspace")
                .and_then(|w| w.get("workspace_id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let pane_id = res
                .get("root_pane")
                .and_then(|p| p.get("pane_id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if let Some(label) = body
                .label
                .as_deref()
                .map(str::trim)
                .filter(|l| !l.is_empty())
            {
                let _ = herdr::rpc(
                    "workspace.rename",
                    json!({ "workspace_id": workspace_id, "label": label }),
                )
                .await;
            }
            Json(json!({ "workspace_id": workspace_id, "pane_id": pane_id })).into_response()
        }
        Err(err) => err_json(err),
    }
}

async fn post_workspace_close(Path(id): Path<String>) -> impl IntoResponse {
    match herdr::rpc("workspace.close", json!({ "workspace_id": id })).await {
        Ok(_) => Json(json!({ "ok": true })).into_response(),
        Err(err) => err_json(err),
    }
}

#[derive(Deserialize)]
struct TabBody {
    workspace_id: String,
}

async fn post_tab(Json(body): Json<TabBody>) -> impl IntoResponse {
    match herdr::rpc("tab.create", json!({ "workspace_id": body.workspace_id })).await {
        Ok(res) => {
            let tab_id = res
                .get("tab")
                .and_then(|t| t.get("tab_id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let pane_id = res
                .get("root_pane")
                .or_else(|| res.get("pane"))
                .and_then(|p| p.get("pane_id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Json(json!({ "tab_id": tab_id, "pane_id": pane_id })).into_response()
        }
        Err(err) => err_json(err),
    }
}

async fn post_tab_close(Path(id): Path<String>) -> impl IntoResponse {
    match herdr::rpc("tab.close", json!({ "tab_id": id })).await {
        Ok(_) => Json(json!({ "ok": true })).into_response(),
        Err(err) => err_json(err),
    }
}

#[derive(Deserialize)]
struct RenameBody {
    label: String,
}

async fn post_tab_rename(
    Path(id): Path<String>,
    Json(body): Json<RenameBody>,
) -> impl IntoResponse {
    match herdr::rpc("tab.rename", json!({ "tab_id": id, "label": body.label })).await {
        Ok(_) => Json(json!({ "ok": true })).into_response(),
        Err(err) => err_json(err),
    }
}

// ------------------------------------------------------------------------ main

/// Force revalidation on every request. Kelpie is installed as a PWA, and
/// mobile Safari otherwise keeps stale shell or WASM assets across bridge
/// restarts. Everything is same-host, so 304 round-trips are cheap.
async fn no_cache(mut res: axum::response::Response) -> axum::response::Response {
    res.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-cache"),
    );
    res
}

#[tokio::main]
async fn main() -> Result<()> {
    let (tx, _) = broadcast::channel(64);
    let state: Shared = Arc::new(AppState {
        fleet: RwLock::new(Fleet::default()),
        pokes: Some(tx),
        pane_locks: Mutex::new(HashSet::new()),
    });

    // Prime the fleet once before serving so first paint isn't empty.
    if let Ok(f) = build_fleet().await {
        *state.fleet.write().await = f;
    }
    tokio::spawn(refresher(state.clone()));

    // Static assets live next to the binary's project root by default; run
    // from the repo root or point KELPIE_STATIC anywhere else.
    let static_dir = std::env::var("KELPIE_STATIC").unwrap_or_else(|_| "static".to_string());

    let app = Router::new()
        .route("/api/fleet", get(get_fleet))
        .route("/api/session/{pane_id}", get(get_session))
        .route("/api/pane/{pane_id}/text", post(post_text))
        .route("/api/pane/{pane_id}/keys", post(post_keys))
        .route("/api/pane/{pane_id}/thinking", post(post_thinking))
        .route("/api/pane/{pane_id}/model", post(post_model))
        .route("/api/pane/{pane_id}/ask", post(post_ask))
        .route(
            "/api/pane/{pane_id}/upload",
            post(post_upload).layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route("/api/events", get(sse_events))
        .route("/api/commands", get(get_commands))
        .route("/api/models", get(get_models))
        .route("/api/pane/{pane_id}/screen", get(get_screen))
        .route("/api/workspace", post(post_workspace))
        .route("/api/workspace/{id}/close", post(post_workspace_close))
        .route("/api/tab", post(post_tab))
        .route("/api/tab/{id}/close", post(post_tab_close))
        .route("/api/tab/{id}/rename", post(post_tab_rename))
        .fallback_service(ServeDir::new(&static_dir).append_index_html_on_directories(true))
        .layer(axum::middleware::map_response(no_cache))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(BIND).await?;
    println!("kelpie listening on http://{BIND} (static: {static_dir})");
    axum::serve(listener, app).await?;
    Ok(())
}
