//! kelpie — mobile bridge for herdr + omp agents.
//!
//! Fleet state comes from herdr's `session.snapshot`; transcripts come from
//! omp's session JSONL files (pane records carry the exact path); input goes
//! back through herdr `pane.send_text` / `pane.send_keys`. No ANSI parsing.

mod herdr;
mod omp;

use anyhow::Result;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime};
use tokio::sync::{broadcast, watch, Mutex, RwLock};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::services::ServeDir;

const BIND: &str = "127.0.0.1:8787";
const POLL_MS: u64 = 600;
const ASK_RECEIPT_TIMEOUT: Duration = Duration::from_secs(5);
const ASK_DRIVER_TIMEOUT: Duration = Duration::from_secs(12);
const OMP_VERSION_TIMEOUT: Duration = Duration::from_secs(5);
const CONTROL_DRIVER_TIMEOUT: Duration = Duration::from_secs(40);
const MODEL_REFRESH_TIMEOUT: Duration = Duration::from_secs(40);
const TEXT_ACTION_TERMINAL_CAP: usize = 256;
const TEXT_ACTION_RETENTION_MS: u128 = 60_000;
const TEXT_RECEIPT_POLL_MS: u64 = 100;
const DUPLICATE_ASK_LABEL_ERROR: &str =
    "ask option labels are not unique; use the raw terminal to recover";
static UPLOAD_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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

type TabActions = HashMap<(String, String), Arc<Mutex<TabAction>>>;

#[derive(Default)]
struct AppState {
    fleet: RwLock<Fleet>,
    /// One incremental JSONL projection per currently referenced session path.
    session_store: Mutex<HashMap<String, Arc<StdMutex<omp::SessionProjection>>>>,
    pokes: Option<broadcast::Sender<String>>,
    /// Panes with an in-flight TUI drive (reasoning cycle or model picker) —
    /// both steer the same terminal, so one guard covers both.
    pane_locks: Mutex<HashSet<String>>,
    /// Workspaces with an in-flight tab/workspace lifecycle mutation.
    workspace_locks: Mutex<HashSet<String>>,
    /// Stable pending-ask actions keyed by pane + OMP call + option index.
    ask_actions: Mutex<HashMap<AskIdentity, Arc<Mutex<AskAction>>>>,
    /// Idempotent text actions keyed by pane + caller action id.
    text_actions: Mutex<HashMap<TextActionKey, Arc<Mutex<TextAction>>>>,
    /// Idempotent tab-creation actions keyed by workspace + caller action id.
    tab_actions: Mutex<TabActions>,
    /// Installed OMP version that produced or validated the model catalog.
    omp_version: Option<String>,
    /// Validated model catalog. None means no usable LKG has been loaded yet.
    model_catalog: RwLock<Option<Value>>,
    /// At most one initial/refresh model subprocess is active at a time.
    model_refresh: Mutex<Option<watch::Receiver<bool>>>,
}
enum ModelRefreshRole {
    Leader(watch::Sender<bool>),
    Follower(watch::Receiver<bool>),
}

type Shared = Arc<AppState>;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct AskIdentity {
    pane_id: String,
    call_id: String,
    index: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum AskPhase {
    PreSubmit,
    SubmittedAwaitingReceipt,
    Confirmed,
    FailedBeforeSubmit,
    StaleAfterSubmit,
}

#[derive(Clone, Debug)]
struct AskAction {
    identity: AskIdentity,
    phase: AskPhase,
    entered: bool,
    retryable: bool,
    accepted: bool,
    option_label: Option<String>,
    error: Option<String>,
    created_at_ms: u128,
    updated_at_ms: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActionRegistration {
    New,
    Existing,
    RetryFailedBeforeSubmit,
    Conflict,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TextActionKey {
    pane_id: String,
    action_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TextPhase {
    PreSubmit,
    SubmittedAwaitingReceipt,
    Confirmed,
    FailedBeforeSubmit,
    StaleAfterSubmit,
}

#[derive(Clone, Debug)]
struct TextAction {
    key: TextActionKey,
    text: String,
    phase: TextPhase,
    accepted: bool,
    retryable: bool,
    error: Option<String>,
    updated_at_ms: u128,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TabActionPhase {
    Pending,
    Confirmed,
    Failed,
}

#[derive(Clone, Debug)]
struct TabAction {
    workspace_id: String,
    action_id: String,
    phase: TabActionPhase,
    pane_id: Option<String>,
    error: Option<String>,
}

fn tab_action_json(action: &TabAction) -> Value {
    json!({
        "workspace_id": action.workspace_id,
        "action_id": action.action_id,
        "phase": action.phase,
        "pane_id": action.pane_id,
        "error": action.error,
    })
}

fn text_action_json(action: &TextAction) -> Value {
    json!({
        "action_id": action.key.action_id,
        "phase": action.phase,
        "accepted": action.accepted,
        "retryable": action.retryable,
        "error": action.error,
    })
}

async fn text_action_json_for(action: &Arc<Mutex<TextAction>>) -> Value {
    let current = action.lock().await;
    text_action_json(&current)
}

fn text_action_terminal(phase: &TextPhase) -> bool {
    matches!(
        phase,
        TextPhase::Confirmed | TextPhase::FailedBeforeSubmit | TextPhase::StaleAfterSubmit
    )
}

async fn prune_text_actions(
    actions: &mut HashMap<TextActionKey, Arc<Mutex<TextAction>>>,
    protected: Option<&TextActionKey>,
) {
    let now = now_ms();
    let mut terminal = Vec::new();
    for (key, action) in actions.iter() {
        let current = action.lock().await;
        if text_action_terminal(&current.phase) {
            terminal.push((current.updated_at_ms, key.clone()));
        }
    }
    terminal.sort_by(|(at_a, key_a), (at_b, key_b)| {
        at_a.cmp(at_b)
            .then_with(|| key_a.pane_id.cmp(&key_b.pane_id))
            .then_with(|| key_a.action_id.cmp(&key_b.action_id))
    });
    let expired = terminal
        .iter()
        .take_while(|(updated_at, _)| now.saturating_sub(*updated_at) >= TEXT_ACTION_RETENTION_MS)
        .count();
    let retained = terminal.len() - expired;
    let remove = expired.saturating_sub(TEXT_ACTION_TERMINAL_CAP.saturating_sub(retained));
    for (_, key) in terminal
        .into_iter()
        .filter(|(_, key)| protected.is_none_or(|keep| keep != key))
        .take(remove)
    {
        actions.remove(&key);
    }
}

async fn register_text_action(
    state: &Shared,
    key: TextActionKey,
    text: String,
) -> (Arc<Mutex<TextAction>>, ActionRegistration) {
    let mut actions = state.text_actions.lock().await;
    prune_text_actions(&mut actions, None).await;
    if let Some(action) = actions.get(&key).cloned() {
        let mut current = action.lock().await;
        if current.text != text {
            return (action.clone(), ActionRegistration::Conflict);
        }
        if current.phase == TextPhase::FailedBeforeSubmit && current.retryable {
            current.phase = TextPhase::PreSubmit;
            current.retryable = false;
            current.error = None;
            current.updated_at_ms = now_ms();
            return (action.clone(), ActionRegistration::RetryFailedBeforeSubmit);
        }
        return (action.clone(), ActionRegistration::Existing);
    }
    let now = now_ms();
    let action = Arc::new(Mutex::new(TextAction {
        key: key.clone(),
        text,
        phase: TextPhase::PreSubmit,
        accepted: true,
        retryable: false,
        error: None,
        updated_at_ms: now,
    }));
    actions.insert(key, action.clone());
    (action, ActionRegistration::New)
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

fn action_id(identity: &AskIdentity) -> String {
    format!(
        "{}:{}:{}",
        identity.pane_id, identity.call_id, identity.index
    )
}

fn action_json(action: &AskAction) -> Value {
    json!({
        "action_id": action_id(&action.identity),
        "pane_id": action.identity.pane_id,
        "call_id": action.identity.call_id,
        "index": action.identity.index,
        "phase": action.phase,
        "entered": action.entered,
        "retryable": action.retryable,
        "accepted": action.accepted,
        "option_label": action.option_label,
        "error": action.error,
        "created_at_ms": action.created_at_ms,
        "updated_at_ms": action.updated_at_ms,
    })
}

async fn action_json_for(action: &Arc<Mutex<AskAction>>) -> Value {
    let current = action.lock().await;
    action_json(&current)
}

async fn register_ask_action(
    state: &Shared,
    identity: AskIdentity,
) -> (Arc<Mutex<AskAction>>, ActionRegistration) {
    let mut actions = state.ask_actions.lock().await;
    if let Some((other_identity, action)) = actions
        .iter()
        .find(|(other, _)| {
            other.pane_id == identity.pane_id
                && other.call_id == identity.call_id
                && other.index != identity.index
        })
        .map(|(other, action)| (other.clone(), action.clone()))
    {
        let _ = other_identity;
        return (action, ActionRegistration::Conflict);
    }
    if let Some(action) = actions.get(&identity).cloned() {
        let retry = {
            let mut current = action.lock().await;
            if current.phase == AskPhase::FailedBeforeSubmit && current.retryable {
                current.phase = AskPhase::PreSubmit;
                current.retryable = false;
                current.accepted = true;
                current.error = None;
                current.updated_at_ms = now_ms();
                true
            } else {
                false
            }
        };
        return (
            action,
            if retry {
                ActionRegistration::RetryFailedBeforeSubmit
            } else {
                ActionRegistration::Existing
            },
        );
    }
    let now = now_ms();
    let action = Arc::new(Mutex::new(AskAction {
        identity: identity.clone(),
        phase: AskPhase::PreSubmit,
        entered: false,
        retryable: false,
        accepted: true,
        option_label: None,
        error: None,
        created_at_ms: now,
        updated_at_ms: now,
    }));
    actions.insert(identity, action.clone());
    (action, ActionRegistration::New)
}

fn classify_driver_failure(entered: bool, message: String) -> (AskPhase, bool, String) {
    if entered {
        (AskPhase::StaleAfterSubmit, false, message)
    } else {
        (AskPhase::FailedBeforeSubmit, true, message)
    }
}

async fn update_action(
    action: &Arc<Mutex<AskAction>>,
    phase: AskPhase,
    entered: bool,
    retryable: bool,
    error: Option<String>,
) {
    let mut current = action.lock().await;
    current.phase = phase;
    current.entered = entered;
    current.retryable = retryable;
    current.error = error;
    current.updated_at_ms = now_ms();
}

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

async fn session_projection_for(
    state: &Shared,
    path: &str,
) -> Arc<StdMutex<omp::SessionProjection>> {
    let mut store = state.session_store.lock().await;
    store
        .entry(path.to_owned())
        .or_insert_with(|| Arc::new(StdMutex::new(omp::SessionProjection::default())))
        .clone()
}

async fn session_summary(state: &Shared, path: &str) -> omp::Summary {
    let projection = session_projection_for(state, path).await;
    let refresh_path = path.to_owned();
    let refreshed = tokio::task::spawn_blocking(move || {
        let mut projection = projection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        projection
            .refresh(&refresh_path)
            .map(|()| projection.summary())
    })
    .await;
    match refreshed {
        Ok(Ok(summary)) => summary,
        _ => omp::Summary {
            title: None,
            snippet: None,
            pending_ask: false,
        },
    }
}

// -------------------------------------------------------------- fleet builder

async fn build_fleet(state: &Shared) -> Result<Fleet> {
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
    let mut referenced_paths = HashSet::new();
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
                referenced_paths.insert(path.clone());
                let sum = session_summary(state, path).await;
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

    // Session paths leave the cache when herdr no longer references them.
    state
        .session_store
        .lock()
        .await
        .retain(|path, _| referenced_paths.contains(path));

    Ok(Fleet {
        workspaces,
        tabs,
        panes,
    })
}

async fn refresher(state: Shared) {
    let mut file_sizes: HashMap<String, u64> = HashMap::new();
    loop {
        match build_fleet(&state).await {
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
                    let event = json!({"type": "fleet", "fleet": &new_fleet}).to_string();
                    *state.fleet.write().await = new_fleet;
                    if let Some(tx) = &state.pokes {
                        let _ = tx.send(event);
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

#[derive(Deserialize, Default)]
struct SessionQuery {
    before: Option<String>,
    limit: Option<String>,
}

const SESSION_PAGE_DEFAULT: usize = 160;
const SESSION_PAGE_MAX: usize = 256;

fn parse_session_query(query: &SessionQuery) -> Result<(Option<usize>, usize), String> {
    let limit = match query.limit.as_deref() {
        None => SESSION_PAGE_DEFAULT,
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| "limit must be a positive integer".to_string())?,
    };
    if limit == 0 {
        return Err("limit must be greater than zero".to_string());
    }
    let before = query
        .before
        .as_deref()
        .map(|raw| {
            raw.parse::<usize>()
                .map_err(|_| "before must be an absolute entry index".to_string())
        })
        .transpose()?;
    Ok((before, limit.min(SESSION_PAGE_MAX)))
}

fn validate_session_cursor(before: Option<usize>, total_entries: usize) -> Result<(), String> {
    if before.is_some_and(|cursor| cursor > total_entries) {
        return Err("before cursor is beyond total_entries".to_string());
    }
    Ok(())
}

async fn get_session(
    State(state): State<Shared>,
    Path(pane_id): Path<String>,
    Query(query): Query<SessionQuery>,
) -> impl IntoResponse {
    let (before, limit) = match parse_session_query(&query) {
        Ok(query) => query,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": error}))).into_response();
        }
    };
    let Some(path) = session_path_for(&state, &pane_id).await else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no transcript for pane"})),
        )
            .into_response();
    };
    let projection = session_projection_for(&state, &path).await;
    let refresh_path = path.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<omp::SessionPage, String> {
        let mut projection = projection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        projection
            .refresh(&refresh_path)
            .map_err(|error| format!("session refresh failed: {error}"))?;
        let total = projection.total_entries();
        validate_session_cursor(before, total)?;
        Ok(projection.page(before, limit))
    })
    .await;
    let mut page = match result {
        Ok(Ok(page)) => page,
        Ok(Err(error)) if error.starts_with("before cursor") => {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": error}))).into_response();
        }
        Ok(Err(_)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "session transcript is unavailable"})),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "session projection failed"})),
            )
                .into_response();
        }
    };

    // Empty sessions may not have emitted model/thinking metadata yet. Keep
    // the existing terminal readback fallback without reparsing the transcript.
    if page.model.is_none() || page.thinking.is_none() {
        if let Ok(screen) = screen_text(&pane_id).await {
            if page.model.is_none() {
                if let Some(selector) = screen_model_selector(&screen) {
                    if let Some((provider, model)) = selector.split_once('/') {
                        page.model = Some(omp::ModelInfo {
                            provider: provider.to_owned(),
                            model: model.to_owned(),
                        });
                    }
                }
            }
            if page.thinking.is_none() {
                page.thinking = screen_thinking_level(&screen).map(str::to_owned);
            }
        }
    }
    Json(page).into_response()
}

#[derive(Deserialize)]
struct TextBody {
    text: String,
    action_id: String,
}

fn input_marker(text: &str) -> String {
    let line = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(text);
    let mut chars = line
        .chars()
        .filter(|char| !char.is_whitespace())
        .rev()
        .take(18)
        .collect::<Vec<_>>();
    chars.reverse();
    chars.into_iter().collect()
}

fn screen_contains_input(screen: &str, marker: &str) -> bool {
    if marker.is_empty() {
        return true;
    }
    let tail = screen.lines().rev().take(24).collect::<Vec<_>>();
    let compact = tail
        .iter()
        .rev()
        .flat_map(|line| line.chars())
        .filter(|char| !char.is_whitespace())
        .collect::<String>();
    compact.contains(marker)
}

fn screen_composer_tail(screen: &str) -> Option<&str> {
    screen.lines().rev().take(12).find_map(|line| {
        let row = unframed_row(line);
        row.strip_prefix('❯')
            .or_else(|| row.strip_prefix('>'))
            .or_else(|| row.strip_prefix('\u{f054}'))
    })
}

fn screen_composer_occupied(screen: &str) -> Option<bool> {
    screen_composer_tail(screen).map(|tail| !tail.trim().is_empty())
}

fn control_composer_requires_clear(screen: &str) -> bool {
    screen_composer_tail(screen).is_some_and(|tail| {
        let tail = tail.trim();
        !tail.is_empty() && "/switch".starts_with(tail)
    })
}
fn requires_user_message_receipt(text: &str) -> bool {
    !matches!(
        text.trim_start().chars().next(),
        Some('/' | '!' | '$' | '#')
    )
}

async fn update_text_action(
    state: &Shared,
    action: &Arc<Mutex<TextAction>>,
    phase: TextPhase,
    retryable: bool,
    error: Option<String>,
) {
    let mut actions = state.text_actions.lock().await;
    let key = {
        let mut current = action.lock().await;
        current.phase = phase;
        current.retryable = retryable;
        current.error = error;
        current.updated_at_ms = now_ms();
        current.key.clone()
    };
    prune_text_actions(&mut actions, Some(&key)).await;
}

async fn user_message_cursor(path: &str) -> u64 {
    let path = path.to_string();
    tokio::task::spawn_blocking(move || omp::user_message_cursor(&path))
        .await
        .unwrap_or_default()
}

async fn scan_new_user_message(path: &str, cursor: u64, expected: &str) -> (u64, bool) {
    let path = path.to_string();
    let expected = expected.to_string();
    tokio::task::spawn_blocking(move || {
        let mut cursor = cursor;
        let matched = omp::scan_new_user_message(&path, &mut cursor, &expected);
        (cursor, matched)
    })
    .await
    .unwrap_or((cursor, false))
}

async fn drive_text(state: Shared, action: Arc<Mutex<TextAction>>) {
    let (pane_id, text) = {
        let current = action.lock().await;
        (current.key.pane_id.clone(), current.text.clone())
    };
    let session_path = session_path_for(&state, &pane_id).await;
    let guard_agent_composer = session_path.is_some();
    let path = session_path.filter(|_| requires_user_message_receipt(&text));
    let mut receipt_cursor = if let Some(path) = path.as_deref() {
        user_message_cursor(path).await
    } else {
        0
    };
    let result = tokio::time::timeout(CONTROL_DRIVER_TIMEOUT, async {
        let before_screen = screen_text(&pane_id)
            .await
            .map_err(|error| error.to_string())?;
        if guard_agent_composer {
            match screen_composer_occupied(&before_screen) {
                Some(false) => {}
                Some(true) => {
                    return Err("composer contains unsent text; clear it before sending".into());
                }
                None => {
                    return Err("could not verify that the terminal composer is empty".into());
                }
            }
        }
        let marker = input_marker(&text);
        herdr::send_text(&pane_id, &text)
            .await
            .map_err(|error| error.to_string())?;
        let mut composer_ready = false;
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let screen = screen_text(&pane_id)
                .await
                .map_err(|error| error.to_string())?;
            let marker_visible = screen_contains_input(&screen, &marker);
            if screen != before_screen && marker_visible {
                composer_ready = true;
                break;
            }
        }
        if !composer_ready {
            return Err("composer did not accept text".into());
        }
        {
            let mut current = action.lock().await;
            current.phase = TextPhase::SubmittedAwaitingReceipt;
            current.retryable = false;
            current.error = None;
            current.updated_at_ms = now_ms();
        }
        // Enter is the submit boundary. From this point a lost response is
        // deliberately stale rather than retryable, because the text may run.
        herdr::send_keys(&pane_id, &["Enter".to_string()])
            .await
            .map_err(|error| error.to_string())?;
        if let Some(tx) = &state.pokes {
            let _ = tx.send(json!({"type": "session", "pane_id": pane_id}).to_string());
        }
        if let Some(path) = path.as_deref() {
            for _ in 0..(CONTROL_DRIVER_TIMEOUT.as_millis() as u64 / TEXT_RECEIPT_POLL_MS) {
                let (next_cursor, matched) =
                    scan_new_user_message(path, receipt_cursor, &text).await;
                receipt_cursor = next_cursor;
                if matched {
                    update_text_action(&state, &action, TextPhase::Confirmed, false, None).await;
                    return Ok::<(), String>(());
                }
                tokio::time::sleep(Duration::from_millis(TEXT_RECEIPT_POLL_MS)).await;
            }
            Err("submitted text was not recorded in the session transcript".into())
        } else {
            update_text_action(&state, &action, TextPhase::Confirmed, false, None).await;
            Ok(())
        }
    })
    .await;

    let failure = match result {
        Ok(Ok(())) => None,
        Ok(Err(message)) => Some(message),
        Err(_) => Some("text driver deadline expired".to_string()),
    };
    if let Some(message) = failure {
        let entered = matches!(
            action.lock().await.phase,
            TextPhase::SubmittedAwaitingReceipt
                | TextPhase::Confirmed
                | TextPhase::StaleAfterSubmit
        );
        update_text_action(
            &state,
            &action,
            if entered {
                TextPhase::StaleAfterSubmit
            } else {
                TextPhase::FailedBeforeSubmit
            },
            !entered,
            Some(message),
        )
        .await;
    }
    // The spawned task owns the claim and releases it regardless of how its
    // bounded driver completes; the request handler never performs cleanup.
    state.pane_locks.lock().await.remove(&pane_id);
}

async fn post_text(
    State(state): State<Shared>,
    Path(pane_id): Path<String>,
    Json(body): Json<TextBody>,
) -> impl IntoResponse {
    let action_id = body.action_id.trim().to_string();
    if action_id.is_empty() || action_id.len() > 160 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "action_id is required"})),
        )
            .into_response();
    }
    let key = TextActionKey { pane_id, action_id };
    let (action, registration) = register_text_action(&state, key, body.text).await;
    if registration == ActionRegistration::Conflict {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "action_id is already registered with different text",
                "receipt": text_action_json_for(&action).await,
            })),
        )
            .into_response();
    }
    let should_spawn = matches!(
        registration,
        ActionRegistration::New | ActionRegistration::RetryFailedBeforeSubmit
    );
    if should_spawn {
        let pane_id = action.lock().await.key.pane_id.clone();
        let claimed = state.pane_locks.lock().await.insert(pane_id.clone());
        if !claimed {
            update_text_action(
                &state,
                &action,
                TextPhase::FailedBeforeSubmit,
                true,
                Some("another pane write is in progress".into()),
            )
            .await;
            return (
                StatusCode::CONFLICT,
                Json(text_action_json_for(&action).await),
            )
                .into_response();
        }
        tokio::spawn(drive_text(state.clone(), action.clone()));
    }
    (
        StatusCode::ACCEPTED,
        Json(text_action_json_for(&action).await),
    )
        .into_response()
}

async fn get_text_status(
    State(state): State<Shared>,
    Path((pane_id, action_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let key = TextActionKey { pane_id, action_id };
    let action = state.text_actions.lock().await.get(&key).cloned();
    let Some(action) = action else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "text action not found"})),
        )
            .into_response();
    };
    Json(text_action_json_for(&action).await).into_response()
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
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = UPLOAD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("img-{stamp}-{sequence}.{ext}"));
    let write = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .and_then(|mut file| file.write_all(&body));
    if let Err(err) = write {
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
    model: String,
    action_id: String,
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

fn model_thinking_levels<'a>(catalog: &'a Value, selector: &str) -> Option<Vec<&'a str>> {
    let (provider, id) = selector.split_once('/')?;
    let model = catalog.get("models")?.as_array()?.iter().find(|model| {
        model.get("provider").and_then(Value::as_str) == Some(provider)
            && model.get("id").and_then(Value::as_str) == Some(id)
    })?;
    if model.get("reasoning").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let mut efforts = Vec::new();
    for raw in model.get("thinking")?.as_array()? {
        let Some(level) = raw.as_str().and_then(canonical_thinking_level) else {
            continue;
        };
        if !efforts.contains(&level) {
            efforts.push(level);
        }
    }
    if efforts.is_empty() {
        return None;
    }
    let mut levels = vec!["off", "auto"];
    levels.extend(efforts);
    Some(levels)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ThinkingState {
    selector: String,
    level: String,
    generation: u64,
    revision: u64,
}

fn fresh_thinking_step(previous: &ThinkingState, current: &ThinkingState) -> bool {
    current.selector == previous.selector
        && current.generation == previous.generation
        && current.revision > previous.revision
        && current.level != previous.level
}

async fn read_thinking_state(
    state: &Shared,
    pane_id: &str,
    path: &str,
) -> std::result::Result<ThinkingState, (StatusCode, String)> {
    let projection = session_projection_for(state, path).await;
    let path = path.to_owned();
    let page = tokio::task::spawn_blocking(move || {
        let mut projection = projection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        projection.refresh(&path)?;
        Ok::<_, std::io::Error>(projection.page(None, 0))
    })
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "session projection task failed".to_owned(),
        )
    })?
    .map_err(|error| {
        (
            StatusCode::BAD_GATEWAY,
            format!("session projection refresh failed: {error}"),
        )
    })?;

    let screen = screen_text(pane_id)
        .await
        .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
    let selector = page
        .model
        .map(|model| {
            if model.model.contains('/') {
                model.model
            } else {
                format!("{}/{}", model.provider, model.model)
            }
        })
        .or_else(|| screen_model_selector(&screen))
        .ok_or_else(|| {
            (
                StatusCode::CONFLICT,
                "current session model is not yet available".to_owned(),
            )
        })?;
    if screen_model_matches_session(&screen, &selector) == Some(false) {
        return Err((
            StatusCode::CONFLICT,
            "live pane model does not match the session; reopen this pane".to_owned(),
        ));
    }
    let level = page
        .thinking
        .as_deref()
        .and_then(canonical_thinking_level)
        .map(str::to_owned)
        .or_else(|| screen_thinking_level(&screen).map(str::to_owned))
        .ok_or_else(|| {
            (
                StatusCode::CONFLICT,
                "current reasoning effort is not yet available".to_owned(),
            )
        })?;
    Ok(ThinkingState {
        selector,
        level,
        generation: page.generation,
        revision: page.revision,
    })
}

async fn wait_thinking_step(
    state: &Shared,
    pane_id: &str,
    path: &str,
    previous: &ThinkingState,
) -> std::result::Result<Option<ThinkingState>, (StatusCode, String)> {
    for _ in 0..20 {
        let current = read_thinking_state(state, pane_id, path).await?;
        if current.selector != previous.selector || current.generation != previous.generation {
            return Err((
                StatusCode::CONFLICT,
                "session model changed while applying reasoning effort".to_owned(),
            ));
        }
        if fresh_thinking_step(previous, &current) {
            return Ok(Some(current));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(None)
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

fn screen_status_fields(screen: &str) -> Option<(&str, &'static str)> {
    let line = screen.lines().rev().find(|line| !line.trim().is_empty())?;
    let mut parts = line.split('·');
    let model = parts.next()?.trim();
    if !model.contains('/') && !model.chars().any(|character| character.is_ascii_digit()) {
        return None;
    }
    let level = parts
        .next()?
        .split(|c: char| !c.is_ascii_alphabetic())
        .find_map(canonical_thinking_level)?;
    Some((model, level))
}

fn screen_thinking_level(screen: &str) -> Option<&'static str> {
    screen_status_fields(screen).map(|(_, level)| level)
}

/// Select one exact reasoning effort. The terminal key is only transport:
/// every step waits for a fresh append-only session receipt from OMP.
async fn drive_thinking(
    state: &Shared,
    pane_id: &str,
    path: &str,
    target: &'static str,
    expected_selector: &str,
) -> std::result::Result<ThinkingState, (StatusCode, String)> {
    let mut live = read_thinking_state(state, pane_id, path).await?;
    if live.selector != expected_selector {
        return Err((
            StatusCode::CONFLICT,
            "session model changed; reopen reasoning effort".to_owned(),
        ));
    }
    if live.level == target {
        return Ok(live);
    }

    let mut seen = HashSet::from([live.level.clone()]);
    for _ in 0..10 {
        herdr::send_text(pane_id, "\x1b[Z")
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
        let Some(next) = wait_thinking_step(state, pane_id, path, &live).await? else {
            return Err((
                StatusCode::BAD_GATEWAY,
                "OMP did not acknowledge the reasoning change".to_owned(),
            ));
        };
        live = next;
        if live.level == target {
            return Ok(live);
        }
        if !seen.insert(live.level.clone()) {
            return Err((
                StatusCode::CONFLICT,
                "model reasoning capabilities changed; reopen reasoning effort".to_owned(),
            ));
        }
    }
    Err((
        StatusCode::BAD_GATEWAY,
        "reasoning change exceeded the model's advertised effort ladder".to_owned(),
    ))
}

async fn unwind_control_driver(pane_id: &str) -> bool {
    for _ in 0..2 {
        let _ = key(pane_id, "Escape", 150).await;
    }
    if screen_text(pane_id)
        .await
        .is_ok_and(|screen| control_composer_requires_clear(&screen))
    {
        let _ = key(pane_id, "ctrl+u", 150).await;
    }
    for _ in 0..20 {
        if screen_text(pane_id)
            .await
            .is_ok_and(|screen| screen_composer_occupied(&screen) == Some(false))
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

async fn run_locked_driver<T, F>(
    state: Shared,
    pane_id: String,
    driver: F,
) -> std::result::Result<T, (StatusCode, String)>
where
    F: Future<Output = std::result::Result<T, (StatusCode, String)>> + Send,
    T: Send,
{
    let result = match tokio::time::timeout(CONTROL_DRIVER_TIMEOUT, driver).await {
        Ok(result) => result,
        Err(_) => {
            let cleaned = unwind_control_driver(&pane_id).await;
            Err((
                StatusCode::GATEWAY_TIMEOUT,
                if cleaned {
                    "control driver deadline expired; terminal state restored".into()
                } else {
                    "control driver deadline expired; terminal cleanup could not be verified".into()
                },
            ))
        }
    };
    state.pane_locks.lock().await.remove(&pane_id);
    result
}

/// Select one exact reasoning effort against the active model's advertised
/// capability set. The request model and action id make stale UI selections
/// rejectable before any terminal input is sent.
async fn post_thinking(
    State(state): State<Shared>,
    Path(pane_id): Path<String>,
    Json(body): Json<ThinkingBody>,
) -> impl IntoResponse {
    let Some(target) = canonical_thinking_level(&body.thinking) else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "unknown reasoning effort", "action_id": body.action_id})),
        )
            .into_response();
    };
    let expected_selector = body.model.trim().to_owned();
    let action_id = body.action_id.trim().to_owned();
    if action_id.is_empty() || action_id.len() > 160 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "action_id is required"})),
        )
            .into_response();
    }
    if expected_selector.len() > 160
        || expected_selector.split_once('/').is_none()
        || expected_selector
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "model must be the active provider/id selector", "action_id": action_id})),
        )
            .into_response();
    }
    let catalog = state.model_catalog.read().await.clone();
    let Some(levels) = catalog
        .as_ref()
        .and_then(|catalog| model_thinking_levels(catalog, &expected_selector))
    else {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "model capabilities changed; reopen reasoning effort",
                "action_id": action_id,
            })),
        )
            .into_response();
    };
    if !levels.contains(&target) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "error": format!("reasoning effort {target} is not supported by {expected_selector}"),
                "action_id": action_id,
            })),
        )
            .into_response();
    }

    let claimed = state.pane_locks.lock().await.insert(pane_id.clone());
    if !claimed {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "another pane write is in progress",
                "action_id": action_id,
            })),
        )
            .into_response();
    }

    let task_state = state.clone();
    let task_pane_id = pane_id.clone();
    let task_selector = expected_selector.clone();
    let task = tokio::spawn(async move {
        let driver_state = task_state.clone();
        let driver_pane_id = task_pane_id.clone();
        run_locked_driver(task_state, task_pane_id, async move {
            let path = session_path_for(&driver_state, &driver_pane_id)
                .await
                .ok_or_else(|| (StatusCode::NOT_FOUND, "no transcript for pane".to_string()))?;
            drive_thinking(
                &driver_state,
                &driver_pane_id,
                &path,
                target,
                &task_selector,
            )
            .await
        })
        .await
    });
    let result = task.await.unwrap_or_else(|_| {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "thinking driver task failed".to_string(),
        ))
    });
    match result {
        Ok(receipt) => Json(json!({
            "ok": true,
            "action_id": action_id,
            "model": receipt.selector,
            "thinking": receipt.level,
            "generation": receipt.generation,
            "revision": receipt.revision,
        }))
        .into_response(),
        Err((status, message)) => (
            status,
            Json(json!({"error": message, "action_id": action_id})),
        )
            .into_response(),
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
    let target_levels = state
        .model_catalog
        .read()
        .await
        .as_ref()
        .and_then(|catalog| model_thinking_levels(catalog, &selector))
        .unwrap_or_default()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
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
    let task_state = state.clone();
    let task_pane_id = pane_id.clone();
    let task_selector = selector.clone();
    let task_levels = target_levels.clone();
    let task = tokio::spawn(async move {
        let driver_state = task_state.clone();
        let driver_pane_id = task_pane_id.clone();
        run_locked_driver(task_state, task_pane_id, async move {
            let path = session_path_for(&driver_state, &driver_pane_id)
                .await
                .ok_or_else(|| (StatusCode::NOT_FOUND, "no transcript for pane".to_string()))?;
            let previous_model = latest_model_receipt(&path).await;
            let previous_thinking = latest_thinking_receipt(&path).await;
            let screen = screen_text(&driver_pane_id)
                .await
                .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
            let preserved_thinking = requested_thinking
                .or_else(|| screen_thinking_level(&screen))
                .or_else(|| previous_thinking.as_ref().map(|receipt| receipt.level))
                .filter(|level| task_levels.iter().any(|supported| supported == level));
            if previous_model
                .as_ref()
                .is_some_and(|receipt| receipt.selector == task_selector)
                || screen_has_model_selector(&screen, &task_selector)
            {
                return Ok(read_thinking_state(&driver_state, &driver_pane_id, &path)
                    .await
                    .ok()
                    .map(|state| state.level));
            }
            drive_model_picker(&driver_pane_id, &task_selector).await?;
            if wait_model_receipt(&path, previous_model.as_ref(), &task_selector)
                .await
                .is_none()
            {
                let screen = screen_text(&driver_pane_id)
                    .await
                    .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
                if !screen_has_model_selector(&screen, &task_selector) {
                    return Err((
                        StatusCode::BAD_GATEWAY,
                        "switch sent but neither the session receipt nor live screen confirmed it"
                            .to_string(),
                    ));
                }
            }
            // Omp re-applies a model-specific thinking setting immediately after
            // model_change. Wait for that receipt before restoring the caller's
            // prior configured level, or a late reapply can overwrite our restore.
            for _ in 0..15 {
                if latest_thinking_receipt(&path).await != previous_thinking {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            if let Some(thinking) = preserved_thinking {
                let receipt = drive_thinking(
                    &driver_state,
                    &driver_pane_id,
                    &path,
                    thinking,
                    &task_selector,
                )
                .await?;
                return Ok(Some(receipt.level));
            }
            Ok(read_thinking_state(&driver_state, &driver_pane_id, &path)
                .await
                .ok()
                .map(|state| state.level))
        })
        .await
    });
    let result = task.await.unwrap_or_else(|_| {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "model driver task failed".to_string(),
        ))
    });
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

fn screen_model_selector(screen: &str) -> Option<String> {
    screen.lines().find_map(|line| {
        let (_, tail) = line.split_once("Session-only model: ")?;
        let selector = tail
            .split_whitespace()
            .next()?
            .trim_end_matches(['.', ',', ';']);
        selector.contains('/').then(|| selector.to_owned())
    })
}

fn model_identity_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn screen_has_model_selector(screen: &str, selector: &str) -> bool {
    screen_model_selector(screen).as_deref() == Some(selector)
}

fn screen_model_matches_session(screen: &str, selector: &str) -> Option<bool> {
    if let Some(screen_selector) = screen_model_selector(screen) {
        return Some(screen_selector == selector);
    }
    let (status_model, _) = screen_status_fields(screen)?;
    let status_model = status_model.rsplit('/').next().unwrap_or(status_model);
    let selector_model = selector.rsplit('/').next().unwrap_or(selector);
    let status_key = model_identity_key(status_model);
    let selector_key = model_identity_key(selector_model);
    Some(
        status_key == selector_key
            || (status_key.len() >= 2
                && selector_key.len() >= 2
                && (selector_key.ends_with(&status_key) || status_key.ends_with(&selector_key))),
    )
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

fn duplicate_ask_option_label<'a>(labels: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let mut seen = HashSet::new();
    labels.into_iter().find(|label| !seen.insert(*label))
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

async fn scan_ask_receipt(path: &str, call_id: &str) -> omp::AskReceipt {
    let path = path.to_string();
    let call_id = call_id.to_string();
    tokio::task::spawn_blocking(move || omp::ask_receipt(&path, &call_id))
        .await
        .unwrap_or(omp::AskReceipt::Pending)
}

async fn scan_ask_option_labels(path: &str, call_id: &str) -> Option<Vec<String>> {
    let path = path.to_string();
    let call_id = call_id.to_string();
    tokio::task::spawn_blocking(move || omp::ask_option_labels(&path, &call_id))
        .await
        .ok()
        .flatten()
}

async fn scan_ask_option_label(path: &str, call_id: &str, index: usize) -> Option<String> {
    let path = path.to_string();
    let call_id = call_id.to_string();
    tokio::task::spawn_blocking(move || omp::ask_option_label(&path, &call_id, index))
        .await
        .ok()
        .flatten()
}

async fn reject_duplicate_ask_labels(
    action: &Arc<Mutex<AskAction>>,
    labels: Option<&[String]>,
) -> bool {
    let Some(labels) = labels else {
        return false;
    };
    if duplicate_ask_option_label(labels.iter().map(String::as_str)).is_none() {
        return false;
    }
    update_action(
        action,
        AskPhase::FailedBeforeSubmit,
        false,
        false,
        Some(DUPLICATE_ASK_LABEL_ERROR.into()),
    )
    .await;
    true
}

#[derive(Deserialize)]
struct AskBody {
    call_id: String,
    index: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReceiptConfirmation {
    Confirmed,
    DuplicateLabels,
    NotConfirmed,
}

async fn mark_receipt_if_confirmed(
    action: &Arc<Mutex<AskAction>>,
    path: &str,
) -> ReceiptConfirmation {
    let (identity, entered, saved_label) = {
        let current = action.lock().await;
        (
            current.identity.clone(),
            current.entered,
            current.option_label.clone(),
        )
    };
    let exact_option_labels = scan_ask_option_labels(path, &identity.call_id).await;
    if reject_duplicate_ask_labels(action, exact_option_labels.as_deref()).await {
        return ReceiptConfirmation::DuplicateLabels;
    }
    let expected_label = if let Some(label) = saved_label {
        Some(label)
    } else if let Some(label) = exact_option_labels
        .as_ref()
        .and_then(|labels| labels.get(identity.index).cloned())
    {
        Some(label)
    } else {
        scan_ask_option_label(path, &identity.call_id, identity.index).await
    };
    let receipt = scan_ask_receipt(path, &identity.call_id).await;
    match receipt {
        omp::AskReceipt::Confirmed(selected) => {
            if expected_label
                .as_deref()
                .is_some_and(|label| selected == [label.to_string()])
            {
                update_action(action, AskPhase::Confirmed, true, false, None).await;
                ReceiptConfirmation::Confirmed
            } else {
                if entered {
                    update_action(
                        action,
                        AskPhase::StaleAfterSubmit,
                        true,
                        false,
                        Some("ask selection receipt did not match the requested option".into()),
                    )
                    .await;
                }
                ReceiptConfirmation::NotConfirmed
            }
        }
        omp::AskReceipt::Error | omp::AskReceipt::Malformed if entered => {
            update_action(
                action,
                AskPhase::StaleAfterSubmit,
                true,
                false,
                Some("ask receipt was an error or malformed".into()),
            )
            .await;
            ReceiptConfirmation::NotConfirmed
        }
        _ => ReceiptConfirmation::NotConfirmed,
    }
}

async fn drive_ask(state: Shared, action: Arc<Mutex<AskAction>>) {
    let identity = action.lock().await.identity.clone();
    let Some(path) = session_path_for(&state, &identity.pane_id).await else {
        update_action(
            &action,
            AskPhase::FailedBeforeSubmit,
            false,
            true,
            Some("no transcript for pane".into()),
        )
        .await;
        return;
    };

    let exact_option_labels = scan_ask_option_labels(&path, &identity.call_id).await;
    if reject_duplicate_ask_labels(&action, exact_option_labels.as_deref()).await {
        return;
    }
    let receipt_option_label = exact_option_labels
        .as_ref()
        .and_then(|labels| labels.get(identity.index).cloned());
    match scan_ask_receipt(&path, &identity.call_id).await {
        omp::AskReceipt::Confirmed(selected)
            if receipt_option_label
                .as_deref()
                .is_some_and(|label| selected.len() == 1 && selected[0] == label) =>
        {
            {
                let mut current = action.lock().await;
                current.option_label = receipt_option_label.clone();
                current.updated_at_ms = now_ms();
            }
            update_action(&action, AskPhase::Confirmed, true, false, None).await;
            return;
        }
        omp::AskReceipt::Confirmed(_) => {
            update_action(
                &action,
                AskPhase::FailedBeforeSubmit,
                false,
                false,
                Some("ask already has a different receipt".into()),
            )
            .await;
            return;
        }
        omp::AskReceipt::Error | omp::AskReceipt::Malformed => {
            update_action(
                &action,
                AskPhase::FailedBeforeSubmit,
                false,
                false,
                Some("ask already has a failed receipt".into()),
            )
            .await;
            return;
        }
        omp::AskReceipt::Pending => {}
    }

    let projection = session_projection_for(&state, &path).await;
    let parse_path = path.clone();
    let pending_ask = tokio::task::spawn_blocking(move || {
        let mut projection = projection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        projection.refresh(&parse_path).ok()?;
        projection.pending_ask()
    })
    .await
    .unwrap_or(None);
    let Some(ask) = pending_ask else {
        update_action(
            &action,
            AskPhase::FailedBeforeSubmit,
            false,
            false,
            Some("ask no longer pending".into()),
        )
        .await;
        return;
    };
    if ask.call_id != identity.call_id {
        update_action(
            &action,
            AskPhase::FailedBeforeSubmit,
            false,
            false,
            Some("ask call_id is no longer current".into()),
        )
        .await;
        return;
    }
    if duplicate_ask_option_label(ask.options.iter().map(|option| option.label.as_str())).is_some()
    {
        update_action(
            &action,
            AskPhase::FailedBeforeSubmit,
            false,
            false,
            Some("ask option labels are not unique; use the raw terminal to recover".into()),
        )
        .await;
        return;
    }
    if ask.multi {
        update_action(
            &action,
            AskPhase::FailedBeforeSubmit,
            false,
            false,
            Some("multi-select not supported yet; use keys".into()),
        )
        .await;
        return;
    }
    let Some(option) = ask.options.get(identity.index).cloned() else {
        update_action(
            &action,
            AskPhase::FailedBeforeSubmit,
            false,
            false,
            Some("option index out of range".into()),
        )
        .await;
        return;
    };
    {
        let mut current = action.lock().await;
        current.option_label = Some(option.label.clone());
        current.updated_at_ms = now_ms();
    }
    if identity.call_id.is_empty() {
        update_action(
            &action,
            AskPhase::FailedBeforeSubmit,
            false,
            false,
            Some("pending ask has no tool-call identity".into()),
        )
        .await;
        return;
    }

    let claimed = state
        .pane_locks
        .lock()
        .await
        .insert(identity.pane_id.clone());
    if !claimed {
        update_action(
            &action,
            AskPhase::FailedBeforeSubmit,
            false,
            true,
            Some("another pane write is in progress".into()),
        )
        .await;
        return;
    }

    let result: Result<(), String> = tokio::time::timeout(ASK_DRIVER_TIMEOUT, async {
        let screen = screen_text(&identity.pane_id)
            .await
            .map_err(|error| error.to_string())?;
        let start = focused_ask_index(&screen, &ask)
            .ok_or_else(|| "ask picker focus could not be verified".to_string())?;
        let (direction, count) = if identity.index >= start {
            ("Down", identity.index - start)
        } else {
            ("Up", start - identity.index)
        };
        for _ in 0..count {
            herdr::send_keys(&identity.pane_id, &[direction.to_string()])
                .await
                .map_err(|error| error.to_string())?;
        }
        let focused = wait_screen(&identity.pane_id, 30, |screen| {
            focused_ask_index(screen, &ask) == Some(identity.index)
        })
        .await
        .map_err(|error| error.to_string())?;
        if focused.is_none() {
            return Err("ask picker did not focus the requested option".into());
        }

        {
            let mut current = action.lock().await;
            current.phase = AskPhase::SubmittedAwaitingReceipt;
            current.entered = true;
            current.retryable = false;
            current.error = None;
            current.updated_at_ms = now_ms();
        }
        herdr::send_keys(&identity.pane_id, &["Enter".to_string()])
            .await
            .map_err(|error| error.to_string())?;

        for _ in 0..50 {
            match scan_ask_receipt(&path, &identity.call_id).await {
                omp::AskReceipt::Confirmed(selected) if selected == [option.label.clone()] => {
                    update_action(&action, AskPhase::Confirmed, true, false, None).await;
                    return Ok(());
                }
                omp::AskReceipt::Confirmed(_) => {
                    return Err("ask selection receipt did not match the requested option".into());
                }
                omp::AskReceipt::Error => return Err("ask tool returned an error".into()),
                omp::AskReceipt::Malformed => return Err("ask receipt was malformed".into()),
                omp::AskReceipt::Pending => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
        Err("ask selection was not recorded before the receipt deadline".into())
    })
    .await
    .unwrap_or_else(|_| Err("ask driver deadline expired".into()));
    state.pane_locks.lock().await.remove(&identity.pane_id);

    if let Err(message) = result {
        let entered = action.lock().await.entered;
        if entered {
            match mark_receipt_if_confirmed(&action, &path).await {
                ReceiptConfirmation::Confirmed | ReceiptConfirmation::DuplicateLabels => return,
                ReceiptConfirmation::NotConfirmed => {}
            }
        }
        let (phase, retryable, error) = classify_driver_failure(entered, message);
        update_action(&action, phase, entered, retryable, Some(error)).await;
    }
}

async fn reconcile_before_retry(state: &Shared, action: &Arc<Mutex<AskAction>>) -> bool {
    let identity = action.lock().await.identity.clone();
    let Some(path) = session_path_for(state, &identity.pane_id).await else {
        return false;
    };
    let exact_option_labels = scan_ask_option_labels(&path, &identity.call_id).await;
    if reject_duplicate_ask_labels(action, exact_option_labels.as_deref()).await {
        return true;
    }
    match scan_ask_receipt(&path, &identity.call_id).await {
        omp::AskReceipt::Pending => false,
        omp::AskReceipt::Confirmed(_) => match mark_receipt_if_confirmed(action, &path).await {
            ReceiptConfirmation::Confirmed | ReceiptConfirmation::DuplicateLabels => true,
            ReceiptConfirmation::NotConfirmed => {
                update_action(
                    action,
                    AskPhase::FailedBeforeSubmit,
                    false,
                    false,
                    Some("ask receipt did not match the requested option".into()),
                )
                .await;
                true
            }
        },
        omp::AskReceipt::Error | omp::AskReceipt::Malformed => {
            update_action(
                action,
                AskPhase::FailedBeforeSubmit,
                false,
                false,
                Some("ask already has a failed receipt".into()),
            )
            .await;
            true
        }
    }
}

async fn post_ask(
    State(state): State<Shared>,
    Path(pane_id): Path<String>,
    Json(body): Json<AskBody>,
) -> impl IntoResponse {
    if body.call_id.trim().is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "call_id is required"})),
        )
            .into_response();
    }
    let identity = AskIdentity {
        pane_id,
        call_id: body.call_id,
        index: body.index,
    };
    let (action, registration) = register_ask_action(&state, identity).await;
    if registration == ActionRegistration::Conflict {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "another option is already active for this ask call_id",
                "action": action_json_for(&action).await,
            })),
        )
            .into_response();
    }
    let should_spawn = match registration {
        ActionRegistration::New => true,
        ActionRegistration::RetryFailedBeforeSubmit => {
            !reconcile_before_retry(&state, &action).await
        }
        ActionRegistration::Existing | ActionRegistration::Conflict => false,
    };
    if should_spawn {
        tokio::spawn(drive_ask(state, action.clone()));
    }
    let response = action_json_for(&action).await;
    (StatusCode::ACCEPTED, Json(response)).into_response()
}

async fn get_ask_status(
    State(state): State<Shared>,
    Path((pane_id, call_id, index)): Path<(String, String, usize)>,
) -> impl IntoResponse {
    if call_id.trim().is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "call_id is required"})),
        )
            .into_response();
    }
    let identity = AskIdentity {
        pane_id,
        call_id,
        index,
    };
    let (action, conflict_action) = {
        let actions = state.ask_actions.lock().await;
        if let Some(action) = actions.get(&identity).cloned() {
            (Some(action), None)
        } else if let Some(action) = actions
            .iter()
            .find(|(key, _)| key.pane_id == identity.pane_id && key.call_id == identity.call_id)
            .map(|(_, action)| action.clone())
        {
            (None, Some(action))
        } else {
            (None, None)
        }
    };
    if let Some(action) = conflict_action {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "a different option is already active for this ask call_id",
                "action": action_json_for(&action).await,
            })),
        )
            .into_response();
    }
    let Some(action) = action else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "ask action not found", "action_id": action_id(&identity)})),
        )
            .into_response();
    };
    if let Some(path) = session_path_for(&state, &identity.pane_id).await {
        let current = action.lock().await.clone();
        if matches!(
            current.phase,
            AskPhase::PreSubmit
                | AskPhase::SubmittedAwaitingReceipt
                | AskPhase::FailedBeforeSubmit
                | AskPhase::StaleAfterSubmit
        ) {
            let _ = mark_receipt_if_confirmed(&action, &path).await;
            let current = action.lock().await.clone();
            if current.phase == AskPhase::SubmittedAwaitingReceipt
                && now_ms().saturating_sub(current.updated_at_ms as u128)
                    >= ASK_RECEIPT_TIMEOUT.as_millis()
            {
                update_action(
                    &action,
                    AskPhase::StaleAfterSubmit,
                    true,
                    false,
                    Some("ask receipt deadline expired".into()),
                )
                .await;
            }
        }
    }
    Json(action_json_for(&action).await).into_response()
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

fn model_cache_path() -> PathBuf {
    if let Ok(path) = std::env::var("KELPIE_MODEL_CACHE") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".cache/kelpie/models.json"))
        .unwrap_or_else(|| PathBuf::from(".cache/kelpie/models.json"))
}

fn first_numeric_version(value: &str) -> Option<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut index = chars.iter().position(|char| char.is_ascii_digit())?;
    let mut token = String::new();
    while index < chars.len() {
        if chars[index].is_ascii_digit() {
            token.push(chars[index]);
            index += 1;
            continue;
        }
        if matches!(chars[index], '.' | '-')
            && chars.get(index + 1).is_some_and(char::is_ascii_digit)
        {
            token.push('.');
            index += 1;
            continue;
        }
        break;
    }
    (!token.is_empty()).then_some(token)
}

fn humanize_model_id(id: &str) -> String {
    id.split(['-', '_', ':', '/'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_model_catalog(raw: Value) -> Option<Value> {
    let mut root = raw.as_object()?.clone();
    let models = root.remove("models")?.as_array()?.clone();
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(models.len());
    for row in models {
        let Some(mut row) = row.as_object().cloned() else {
            continue;
        };
        let Some(provider) = row
            .get("provider")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
            .map(str::to_string)
        else {
            continue;
        };
        let Some(id) = row
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
            .map(str::to_string)
        else {
            continue;
        };
        let selector = format!("{provider}/{id}");
        if !seen.insert(selector) {
            continue;
        }
        let name = row
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let name = match (name, first_numeric_version(&id)) {
            (Some(name), Some(id_version))
                if first_numeric_version(&name)
                    .is_some_and(|name_version| name_version != id_version) =>
            {
                humanize_model_id(&id)
            }
            (Some(name), _) => name,
            (None, _) => humanize_model_id(&id),
        };
        row.insert("provider".into(), Value::String(provider));
        row.insert("id".into(), Value::String(id));
        row.insert("name".into(), Value::String(name));
        normalized.push(Value::Object(row));
    }
    if normalized.is_empty() {
        return None;
    }
    root.insert("models".into(), Value::Array(normalized));
    Some(Value::Object(root))
}

fn normalize_cached_model_catalog(
    parsed: Value,
    expected_omp_version: Option<&str>,
) -> Option<Value> {
    if expected_omp_version.is_some_and(|expected| {
        parsed.get("_kelpie_omp_version").and_then(Value::as_str) != Some(expected)
    }) {
        return None;
    }
    normalize_model_catalog(parsed)
}

fn load_model_cache(expected_omp_version: Option<&str>) -> Option<Value> {
    let raw = std::fs::read_to_string(model_cache_path()).ok()?;
    let parsed = serde_json::from_str::<Value>(&raw).ok()?;
    normalize_cached_model_catalog(parsed, expected_omp_version)
}

fn persist_model_cache(catalog: &Value) -> std::io::Result<()> {
    let path = model_cache_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("json.tmp-{}", now_ms()));
    let bytes = serde_json::to_vec(catalog).map_err(std::io::Error::other)?;
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, path)
}

async fn fetch_omp_version() -> Option<String> {
    let mut command = tokio::process::Command::new("omp");
    command.arg("--version").kill_on_drop(true);
    let output = tokio::time::timeout(OMP_VERSION_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|version| !version.is_empty())
}

async fn fetch_model_catalog(omp_version: Option<&str>) -> Option<Value> {
    let mut command = tokio::process::Command::new("omp");
    command.args(["models", "--json"]).kill_on_drop(true);
    let output = tokio::time::timeout(MODEL_REFRESH_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let mut catalog = normalize_model_catalog(serde_json::from_slice(&output.stdout).ok()?)?;
    if let (Some(version), Some(root)) = (omp_version, catalog.as_object_mut()) {
        root.insert(
            "_kelpie_omp_version".into(),
            Value::String(version.to_owned()),
        );
    }
    Some(catalog)
}

async fn begin_model_refresh(state: &Shared) -> ModelRefreshRole {
    let mut refresh = state.model_refresh.lock().await;
    if let Some(receiver) = refresh.as_ref() {
        ModelRefreshRole::Follower(receiver.clone())
    } else {
        let (sender, receiver) = watch::channel(false);
        *refresh = Some(receiver);
        ModelRefreshRole::Leader(sender)
    }
}

async fn refresh_model_catalog(state: Shared) -> Option<Value> {
    let sender = match begin_model_refresh(&state).await {
        ModelRefreshRole::Follower(mut receiver) => {
            let _ = receiver.changed().await;
            return state.model_catalog.read().await.clone();
        }
        ModelRefreshRole::Leader(sender) => sender,
    };

    let fresh = fetch_model_catalog(state.omp_version.as_deref()).await;
    if let Some(catalog) = fresh.clone() {
        *state.model_catalog.write().await = Some(catalog.clone());
        let _ = tokio::task::spawn_blocking(move || persist_model_cache(&catalog)).await;
    }
    {
        let mut refresh = state.model_refresh.lock().await;
        *refresh = None;
    }
    let _ = sender.send(true);
    if fresh.is_some() {
        fresh
    } else {
        state.model_catalog.read().await.clone()
    }
}

/// Available models from omp models --json. A validated persistent LKG serves
/// immediately; the first miss coalesces callers behind one bounded refresh.
async fn get_models(State(state): State<Shared>) -> impl IntoResponse {
    let cached = state.model_catalog.read().await.clone();
    let catalog = match cached {
        Some(catalog) => Some(catalog),
        None => refresh_model_catalog(state).await,
    };
    match catalog {
        Some(catalog) => Json(catalog).into_response(),
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

async fn post_workspace_close(
    State(state): State<Shared>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let pane_ids = state
        .fleet
        .read()
        .await
        .panes
        .iter()
        .filter(|pane| pane.workspace_id == id)
        .map(|pane| pane.pane_id.clone())
        .collect::<HashSet<_>>();
    if !state.workspace_locks.lock().await.insert(id.clone()) {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "another workspace lifecycle change is in progress"})),
        )
            .into_response();
    }
    let locks = state.pane_locks.lock().await;
    if pane_ids.iter().any(|pane_id| locks.contains(pane_id)) {
        drop(locks);
        state.workspace_locks.lock().await.remove(&id);
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "a pane write is still in progress"})),
        )
            .into_response();
    }
    let response = match herdr::rpc("workspace.close", json!({ "workspace_id": id.clone() })).await
    {
        Ok(_) => Json(json!({ "ok": true })).into_response(),
        Err(err) => err_json(err),
    };
    drop(locks);
    state.workspace_locks.lock().await.remove(&id);
    response
}

#[derive(Deserialize)]
struct TabBody {
    workspace_id: String,
    action_id: String,
}

async fn tab_action_response(action: &Arc<Mutex<TabAction>>) -> axum::response::Response {
    let current = action.lock().await;
    let status = match current.phase {
        TabActionPhase::Pending => StatusCode::ACCEPTED,
        TabActionPhase::Confirmed => StatusCode::OK,
        TabActionPhase::Failed => StatusCode::OK,
    };
    (status, Json(tab_action_json(&current))).into_response()
}

async fn post_tab(State(state): State<Shared>, Json(body): Json<TabBody>) -> impl IntoResponse {
    let workspace_id = body.workspace_id.trim().to_owned();
    let action_id = body.action_id.trim().to_owned();
    if workspace_id.is_empty() || action_id.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "workspace_id and action_id are required"})),
        )
            .into_response();
    }
    let key = (workspace_id.clone(), action_id.clone());
    let action = {
        let mut actions = state.tab_actions.lock().await;
        if let Some(action) = actions.get(&key) {
            return tab_action_response(action).await;
        }
        if !state
            .workspace_locks
            .lock()
            .await
            .insert(workspace_id.clone())
        {
            return (
                StatusCode::CONFLICT,
                Json(json!({"error": "another workspace lifecycle change is in progress"})),
            )
                .into_response();
        }
        let action = Arc::new(Mutex::new(TabAction {
            workspace_id: workspace_id.clone(),
            action_id,
            phase: TabActionPhase::Pending,
            pane_id: None,
            error: None,
        }));
        actions.insert(key, action.clone());
        action
    };
    let task_state = state.clone();
    let task_action = action.clone();
    tokio::spawn(async move {
        let result = herdr::rpc(
            "tab.create",
            json!({ "workspace_id": workspace_id.clone() }),
        )
        .await;
        task_state
            .workspace_locks
            .lock()
            .await
            .remove(&workspace_id);
        let mut current = task_action.lock().await;
        match result {
            Ok(response) => {
                let pane_id = response
                    .get("root_pane")
                    .or_else(|| response.get("pane"))
                    .and_then(|pane| pane.get("pane_id"))
                    .and_then(Value::as_str)
                    .filter(|pane_id| !pane_id.is_empty())
                    .map(str::to_owned);
                if let Some(pane_id) = pane_id {
                    current.phase = TabActionPhase::Confirmed;
                    current.pane_id = Some(pane_id);
                } else {
                    current.phase = TabActionPhase::Failed;
                    current.error = Some("tab create returned no pane".into());
                }
            }
            Err(error) => {
                current.phase = TabActionPhase::Failed;
                current.error = Some(error.to_string());
            }
        }
        drop(current);
        if let Some(tx) = &task_state.pokes {
            let _ = tx.send(json!({"type": "fleet"}).to_string());
        }
    });
    tab_action_response(&action).await
}

async fn get_tab_status(
    State(state): State<Shared>,
    Path((workspace_id, action_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let action = state
        .tab_actions
        .lock()
        .await
        .get(&(workspace_id, action_id))
        .cloned();
    match action {
        Some(action) => tab_action_response(&action).await,
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "tab action not found"})),
        )
            .into_response(),
    }
}

async fn post_tab_close(State(state): State<Shared>, Path(id): Path<String>) -> impl IntoResponse {
    let (workspace_id, pane_ids) = {
        let fleet = state.fleet.read().await;
        let workspace_id = fleet
            .panes
            .iter()
            .find(|pane| pane.tab_id == id)
            .map(|pane| pane.workspace_id.clone());
        let pane_ids = fleet
            .panes
            .iter()
            .filter(|pane| pane.tab_id == id)
            .map(|pane| pane.pane_id.clone())
            .collect::<HashSet<_>>();
        (workspace_id, pane_ids)
    };
    let Some(workspace_id) = workspace_id else {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "tab workspace is not available yet"})),
        )
            .into_response();
    };
    if !state
        .workspace_locks
        .lock()
        .await
        .insert(workspace_id.clone())
    {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "another workspace lifecycle change is in progress"})),
        )
            .into_response();
    }
    let locks = state.pane_locks.lock().await;
    if pane_ids.iter().any(|pane_id| locks.contains(pane_id)) {
        drop(locks);
        state.workspace_locks.lock().await.remove(&workspace_id);
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "a pane write is still in progress"})),
        )
            .into_response();
    }
    let response = match herdr::rpc("tab.close", json!({ "tab_id": id })).await {
        Ok(_) => Json(json!({ "ok": true })).into_response(),
        Err(err) => err_json(err),
    };
    drop(locks);
    state.workspace_locks.lock().await.remove(&workspace_id);
    response
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
    let omp_version = fetch_omp_version().await;
    let state: Shared = Arc::new(AppState {
        fleet: RwLock::new(Fleet::default()),
        session_store: Mutex::new(HashMap::new()),
        pokes: Some(tx),
        pane_locks: Mutex::new(HashSet::new()),
        workspace_locks: Mutex::new(HashSet::new()),
        ask_actions: Mutex::new(HashMap::new()),
        text_actions: Mutex::new(HashMap::new()),
        tab_actions: Mutex::new(HashMap::new()),
        model_catalog: RwLock::new(load_model_cache(omp_version.as_deref())),
        model_refresh: Mutex::new(None),
        omp_version,
    });

    // Prime the fleet once before serving so first paint isn't empty.
    if let Ok(f) = build_fleet(&state).await {
        *state.fleet.write().await = f;
    }
    tokio::spawn(refresher(state.clone()));
    // Refresh in the background so a validated LKG can serve immediately.
    tokio::spawn(refresh_model_catalog(state.clone()));

    // Static assets live next to the binary's project root by default; run
    // from the repo root or point KELPIE_STATIC anywhere else.
    let static_dir = std::env::var("KELPIE_STATIC").unwrap_or_else(|_| "static".to_string());

    let app = Router::new()
        .route("/api/fleet", get(get_fleet))
        .route("/api/session/{pane_id}", get(get_session))
        .route("/api/pane/{pane_id}/text", post(post_text))
        .route("/api/pane/{pane_id}/text/{action_id}", get(get_text_status))
        .route("/api/pane/{pane_id}/keys", post(post_keys))
        .route("/api/pane/{pane_id}/thinking", post(post_thinking))
        .route("/api/pane/{pane_id}/model", post(post_model))
        .route("/api/pane/{pane_id}/ask", post(post_ask))
        .route(
            "/api/pane/{pane_id}/ask/{call_id}/{index}",
            get(get_ask_status),
        )
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
        .route(
            "/api/tab/{workspace_id}/action/{action_id}",
            get(get_tab_status),
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_query_defaults_and_clamps_page_limits() {
        let (before, limit) = parse_session_query(&SessionQuery::default()).unwrap();
        assert_eq!(before, None);
        assert_eq!(limit, SESSION_PAGE_DEFAULT);
        let (_, limit) = parse_session_query(&SessionQuery {
            before: Some("140".into()),
            limit: Some("999".into()),
        })
        .unwrap();
        assert_eq!(limit, SESSION_PAGE_MAX);
    }

    #[test]
    fn session_query_rejects_zero_or_incompatible_cursor_values() {
        assert_eq!(
            parse_session_query(&SessionQuery {
                before: Some("not-an-index".into()),
                limit: None,
            })
            .unwrap_err(),
            "before must be an absolute entry index"
        );
        assert_eq!(
            parse_session_query(&SessionQuery {
                before: None,
                limit: Some("0".into()),
            })
            .unwrap_err(),
            "limit must be greater than zero"
        );
        assert_eq!(
            validate_session_cursor(Some(301), 300).unwrap_err(),
            "before cursor is beyond total_entries"
        );
        assert!(validate_session_cursor(Some(300), 300).is_ok());
    }

    #[test]
    fn ask_driver_deadline_is_retryable_only_before_enter() {
        let (phase, retryable, message) =
            classify_driver_failure(false, "ask driver deadline expired".into());
        assert_eq!(phase, AskPhase::FailedBeforeSubmit);
        assert!(retryable);
        assert_eq!(message, "ask driver deadline expired");

        let (phase, retryable, _) =
            classify_driver_failure(true, "ask driver deadline expired".into());
        assert_eq!(phase, AskPhase::StaleAfterSubmit);
        assert!(!retryable);
    }

    #[test]
    fn duplicate_ask_option_labels_are_rejected_without_rejecting_unique_labels() {
        let unique = [
            omp::AskOption {
                label: "first".into(),
                description: None,
            },
            omp::AskOption {
                label: "second".into(),
                description: None,
            },
        ];
        assert_eq!(
            duplicate_ask_option_label(unique.iter().map(|option| option.label.as_str())),
            None
        );

        let duplicate = [
            omp::AskOption {
                label: "first".into(),
                description: None,
            },
            omp::AskOption {
                label: "first".into(),
                description: Some("different details".into()),
            },
        ];
        assert_eq!(
            duplicate_ask_option_label(duplicate.iter().map(|option| option.label.as_str())),
            Some("first")
        );
    }

    #[tokio::test]
    async fn duplicate_ask_identity_starts_one_driver_and_conflicts_on_other_index() {
        let state = Arc::new(AppState::default());
        let identity = AskIdentity {
            pane_id: "pane-1".into(),
            call_id: "call-1".into(),
            index: 0,
        };
        let (first, first_registration) = register_ask_action(&state, identity.clone()).await;
        let (duplicate, duplicate_registration) = register_ask_action(&state, identity).await;
        assert_eq!(first_registration, ActionRegistration::New);
        assert_eq!(duplicate_registration, ActionRegistration::Existing);
        assert!(Arc::ptr_eq(&first, &duplicate));

        let (different, different_registration) = register_ask_action(
            &state,
            AskIdentity {
                pane_id: "pane-1".into(),
                call_id: "call-1".into(),
                index: 1,
            },
        )
        .await;
        assert_eq!(different_registration, ActionRegistration::Conflict);
        assert!(Arc::ptr_eq(&first, &different));
    }

    #[tokio::test]
    async fn model_catalog_followers_cannot_miss_refresh_completion() {
        let state = Arc::new(AppState::default());
        let sender = match begin_model_refresh(&state).await {
            ModelRefreshRole::Leader(sender) => sender,
            ModelRefreshRole::Follower(_) => panic!("first caller must lead"),
        };
        let follower = match begin_model_refresh(&state).await {
            ModelRefreshRole::Follower(receiver) => receiver,
            ModelRefreshRole::Leader(_) => panic!("second caller must follow"),
        };

        *state.model_refresh.lock().await = None;
        let _ = sender.send(true);

        tokio::time::timeout(Duration::from_millis(50), async move {
            let mut follower = follower;
            follower.changed().await.expect("leader remains alive");
        })
        .await
        .expect("completion sent before await must still wake the follower");
    }

    #[test]
    fn model_catalog_normalization_dedupes_exact_selectors_and_repairs_names() {
        let raw = json!({
            "models": [
                {"provider": "openai", "id": "gpt-4o", "name": "GPT 3 Turbo"},
                {"provider": "openai", "id": "gpt-4o", "name": "duplicate"},
                {"provider": "openai", "id": "gpt-4o-mini", "name": "GPT 4o Mini"},
                {"name": "orphan"},
                {"provider": "anthropic", "id": "claude-3-5-sonnet", "name": "Claude 3.5 Sonnet"}
            ]
        });
        let normalized = normalize_model_catalog(raw).expect("valid catalog");
        let models = normalized.get("models").and_then(Value::as_array).unwrap();
        assert_eq!(models.len(), 3);
        assert_eq!(models[0]["provider"], "openai");
        assert_eq!(models[0]["id"], "gpt-4o");
        assert_eq!(models[0]["name"], "Gpt 4o");
        assert_eq!(models[1]["id"], "gpt-4o-mini");
        assert_eq!(models[2]["id"], "claude-3-5-sonnet");
        assert_eq!(models[2]["name"], "Claude 3.5 Sonnet");
    }

    #[test]
    fn model_catalog_normalization_rejects_missing_identity() {
        assert!(normalize_model_catalog(json!({"models": [{"name": "orphan"}]})).is_none());
    }

    #[test]
    fn model_cache_rejects_only_a_known_version_mismatch() {
        let cached = json!({
            "_kelpie_omp_version": "omp v17.0.5",
            "models": [{"provider": "openai", "id": "gpt-5.6-sol", "name": "GPT-5.6-Sol"}]
        });
        assert!(normalize_cached_model_catalog(cached.clone(), Some("omp v17.0.5")).is_some());
        assert!(normalize_cached_model_catalog(cached.clone(), Some("omp v18.0.0")).is_none());
        assert!(normalize_cached_model_catalog(cached, None).is_some());
    }

    #[test]
    fn reasoning_control_adds_selector_modes_to_advertised_efforts() {
        let catalog = json!({
            "models": [
                {
                    "provider": "openai-codex",
                    "id": "gpt-5.6-sol",
                    "reasoning": true,
                    "thinking": ["low", "future", "medium", "high", "xhigh", "max"]
                },
                {
                    "provider": "anthropic",
                    "id": "claude-fable-5",
                    "reasoning": true,
                    "thinking": ["low", "medium", "high"]
                }
            ]
        });

        assert_eq!(
            model_thinking_levels(&catalog, "openai-codex/gpt-5.6-sol"),
            Some(vec!["off", "auto", "low", "medium", "high", "xhigh", "max"])
        );
        assert_eq!(model_thinking_levels(&catalog, "missing/model"), None);
    }

    #[test]
    fn reasoning_step_requires_a_fresh_same_session_receipt() {
        let previous = ThinkingState {
            selector: "openai-codex/gpt-5.6-sol".into(),
            level: "xhigh".into(),
            generation: 7,
            revision: 40,
        };
        let stale = ThinkingState {
            level: "max".into(),
            ..previous.clone()
        };
        let reset = ThinkingState {
            level: "max".into(),
            generation: 8,
            revision: 41,
            ..previous.clone()
        };
        let confirmed = ThinkingState {
            level: "max".into(),
            revision: 41,
            ..previous.clone()
        };

        assert!(!fresh_thinking_step(&previous, &stale));
        assert!(!fresh_thinking_step(&previous, &reset));
        assert!(fresh_thinking_step(&previous, &confirmed));
    }

    #[test]
    fn agent_text_driver_requires_a_recognized_empty_composer() {
        assert_eq!(
            screen_composer_occupied("/private/tmp/project                    20:19\n❯\n"),
            Some(false)
        );
        assert_eq!(
            screen_composer_occupied(
                "/private/tmp/project                    20:19\n❯ unsent draft\n"
            ),
            Some(true)
        );
        assert_eq!(
            screen_composer_occupied("shell output without prompt"),
            None
        );
        assert_eq!(screen_composer_occupied("$ "), None);
        assert!(control_composer_requires_clear("❯ /switch"));
        assert!(control_composer_requires_clear("❯ /sw"));
        assert!(!control_composer_requires_clear("❯ operator draft"));
        assert!(!control_composer_requires_clear("❯ /switchboard notes"));
        assert!(!control_composer_requires_clear("❯ /switch provider/model"));
    }

    #[test]
    fn current_reasoning_is_detected_on_empty_sessions() {
        let screen = "openai-codex/GPT-5.6-Luna · low · kelpie · master ctx 14%";
        assert_eq!(screen_thinking_level(screen), Some("low"));
        assert_eq!(
            screen_thinking_level("openai-codex/GPT-5.6-Luna · high · kelpie"),
            Some("high")
        );
        assert_eq!(screen_thinking_level("operator note · high"), None);
    }

    #[test]
    fn current_model_selector_is_detected_on_empty_sessions() {
        let screen =
            "Session-only model: openai-codex/gpt-5.6-luna. Use alt+m or /model for roles.";
        assert_eq!(
            screen_model_selector(screen).as_deref(),
            Some("openai-codex/gpt-5.6-luna")
        );
        assert!(screen_has_model_selector(
            screen,
            "openai-codex/gpt-5.6-luna"
        ));
        assert!(!screen_has_model_selector(
            screen,
            "openai-codex/gpt-5.6-sol"
        ));
        let status = "  GPT-5.6-Sol · high · kelpie · master ctx 66%";
        assert_eq!(
            screen_model_matches_session(status, "openai-codex/gpt-5.6-sol"),
            Some(true)
        );
        assert_eq!(
            screen_model_matches_session(status, "openai-codex/gpt-5.6-luna"),
            Some(false)
        );
        assert_eq!(screen_thinking_level(status), Some("high"));
        assert_eq!(
            screen_model_matches_session(
                "openai-codex/GPT-5.6-Sol · kelpie",
                "openai-codex/gpt-5.6-sol"
            ),
            None
        );
        // OMP status lines shorten some display names by dropping the vendor
        // prefix: `anthropic/claude-fable-5` renders as `anthropic/Fable 5`.
        assert_eq!(
            screen_model_matches_session(
                "  anthropic/Fable 5 · low · kelpie-busy-ack       ctx 8%",
                "anthropic/claude-fable-5"
            ),
            Some(true)
        );
        assert_eq!(
            screen_model_matches_session(
                "  anthropic/Fable 5 · low · kelpie-busy-ack       ctx 8%",
                "anthropic/claude-sonnet-5"
            ),
            Some(false)
        );
        // Suffix tolerance must not let degenerate short keys match.
        assert_eq!(
            screen_model_matches_session("  K3 5 · high · ws", "kimi-code/k3"),
            Some(false)
        );
    }

    #[test]
    fn text_input_marker_survives_terminal_line_wrapping() {
        let text = "omp --no-session --model openai-codex/gpt-5.6-luna --thinking low";
        let screen = "❯ omp --no-session --model openai-codex/gpt-5.6-luna --thinking lo\nw";
        let marker = input_marker(text);
        assert_eq!(marker, "-luna--thinkinglow");
        assert!(screen_contains_input(screen, &marker));
    }

    #[test]
    fn omp_control_inputs_confirm_at_the_terminal_boundary() {
        assert!(!requires_user_message_receipt("/exit"));
        assert!(!requires_user_message_receipt("!echo ok"));
        assert!(!requires_user_message_receipt("$ 2 + 2"));
        assert!(!requires_user_message_receipt("# action"));
        assert!(requires_user_message_receipt("Reply with OK"));
    }

    #[tokio::test]
    async fn text_action_registration_is_idempotent_conflicting_and_bounded() {
        let state = Arc::new(AppState::default());
        let key = TextActionKey {
            pane_id: "pane".into(),
            action_id: "same".into(),
        };
        let (first, first_registration) =
            register_text_action(&state, key.clone(), "hello".into()).await;
        let (same, same_registration) =
            register_text_action(&state, key.clone(), "hello".into()).await;
        assert_eq!(first_registration, ActionRegistration::New);
        assert_eq!(same_registration, ActionRegistration::Existing);
        assert!(Arc::ptr_eq(&first, &same));
        let (_, conflict_registration) =
            register_text_action(&state, key, "different".into()).await;
        assert_eq!(conflict_registration, ActionRegistration::Conflict);

        for index in 0..=TEXT_ACTION_TERMINAL_CAP {
            let key = TextActionKey {
                pane_id: "pane".into(),
                action_id: format!("terminal-{index:03}"),
            };
            let (action, _) = register_text_action(&state, key, "text".into()).await;
            update_text_action(&state, &action, TextPhase::Confirmed, false, None).await;
        }
        let terminal_actions = {
            let actions = state.text_actions.lock().await;
            let mut terminal = Vec::new();
            for action in actions.values() {
                if text_action_terminal(&action.lock().await.phase) {
                    terminal.push(action.clone());
                }
            }
            terminal
        };
        assert_eq!(terminal_actions.len(), TEXT_ACTION_TERMINAL_CAP + 1);
        for action in terminal_actions {
            action.lock().await.updated_at_ms =
                now_ms().saturating_sub(TEXT_ACTION_RETENTION_MS + 1);
        }
        let key = TextActionKey {
            pane_id: "pane".into(),
            action_id: "trigger-prune".into(),
        };
        let _ = register_text_action(&state, key, "text".into()).await;
        let actions = state.text_actions.lock().await;
        let mut terminal = 0;
        for action in actions.values() {
            if text_action_terminal(&action.lock().await.phase) {
                terminal += 1;
            }
        }
        assert_eq!(terminal, TEXT_ACTION_TERMINAL_CAP);
    }
}
