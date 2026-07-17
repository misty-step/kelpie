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

async fn post_text(Path(pane_id): Path<String>, Json(body): Json<TextBody>) -> impl IntoResponse {
    let run = async {
        herdr::send_text(&pane_id, &body.text).await?;
        herdr::send_keys(&pane_id, &["Enter".to_string()]).await
    };
    match run.await {
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
async fn post_keys(Path(pane_id): Path<String>, Json(body): Json<KeysBody>) -> impl IntoResponse {
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
    match run.await {
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
    steps: usize,
}

/// Apply an exact reasoning choice selected in the phone UI. Omp's TUI has
/// no runtime setter command; its only control is back-tab cycling. The
/// client computes the distance using omp's advertised effort order and this
/// endpoint owns the paced delivery so navigation/backgrounding cannot leave
/// a change half-applied.
async fn post_thinking(
    State(state): State<Shared>,
    Path(pane_id): Path<String>,
    Json(body): Json<ThinkingBody>,
) -> impl IntoResponse {
    if body.steps == 0 || body.steps > 8 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "steps must be between 1 and 8"})),
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

    let run = async {
        for _ in 0..body.steps {
            herdr::send_text(&pane_id, "\x1b[Z").await?;
            // Omp debounces rapid TUI key changes. Delivery acknowledgements
            // only mean bytes reached the PTY; leave one render turn between
            // steps and after the final step before the client verifies.
            tokio::time::sleep(Duration::from_millis(800)).await;
        }
        Ok::<(), anyhow::Error>(())
    };

    let result = run.await;
    state.pane_locks.lock().await.remove(&pane_id);
    match result {
        Ok(()) => Json(json!({"ok": true})).into_response(),
        Err(err) => err_json(err),
    }
}

#[derive(Deserialize)]
struct ModelBody {
    model: String,
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

/// Nerd Font / powerline glyphs live in the Unicode private-use areas; omp's
/// picker mixes them into label text (sometimes flush against words) and
/// herdr's plain-text read drops some of them entirely.
fn is_private_use(c: char) -> bool {
    matches!(c as u32, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
}

fn strip_glyphs(s: &str) -> String {
    s.chars().filter(|c| !is_private_use(*c)).collect()
}

/// `│  All models   N │` row in omp's model picker → N.
fn all_models_count(screen: &str) -> Option<usize> {
    let idx = screen.find("All models")?;
    let line = screen[idx..].lines().next()?;
    let digits: String = line
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
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

/// Switch the pane's default model by driving omp's interactive `/model`
/// picker. Omp does not execute arged slash commands submitted as text —
/// the command palette closes the moment arguments follow the name, and
/// Enter submits the whole line as a chat prompt (verified live) — so the
/// picker is the only runtime lever on a live TUI pane. Every stage is
/// verified against the plain-text screen before the next key; failures
/// unwind only overlays this driver opened (never a bare Escape at the
/// composer, which would interrupt a running agent). The picker chain:
/// `/model` ⏎ (palette) → search selector → ⏎ (role menu) → ⏎ assigns
/// `default` → ⏎ keeps the pre-highlighted reasoning level → Esc Esc, then
/// omp prints `Default model: <selector>` as the receipt.
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
    let result = drive_model_picker(&pane_id, &selector).await;
    state.pane_locks.lock().await.remove(&pane_id);
    match result {
        Ok(()) => Json(json!({"ok": true, "model": selector})).into_response(),
        Err((status, msg)) => (status, Json(json!({"error": msg}))).into_response(),
    }
}

async fn drive_model_picker(
    pane_id: &str,
    selector: &str,
) -> std::result::Result<(), (StatusCode, String)> {
    let gateway = |e: anyhow::Error| (StatusCode::BAD_GATEWAY, e.to_string());
    let id_part = selector.split_once('/').map_or(selector, |(_, id)| id);

    // Palette: clear any composer draft, type the command name.
    key(pane_id, "ctrl+u", 250).await.map_err(gateway)?;
    type_chars(pane_id, "/model").await.map_err(gateway)?;
    tokio::time::sleep(Duration::from_millis(550)).await;
    let s = screen_text(pane_id).await.map_err(gateway)?;
    if !s.contains("Model: ") {
        let _ = key(pane_id, "ctrl+u", 100).await;
        return Err((
            StatusCode::BAD_GATEWAY,
            "command palette did not open".into(),
        ));
    }

    // Picker: Enter runs the highlighted exact-match `model` command.
    key(pane_id, "Enter", 800).await.map_err(gateway)?;
    let s = screen_text(pane_id).await.map_err(gateway)?;
    if !s.contains("All available models") {
        let _ = key(pane_id, "ctrl+u", 100).await;
        return Err((StatusCode::BAD_GATEWAY, "model picker did not open".into()));
    }

    // Search: the full selector; require at least one fuzzy match.
    type_chars(pane_id, selector).await.map_err(gateway)?;
    tokio::time::sleep(Duration::from_millis(550)).await;
    let s = screen_text(pane_id).await.map_err(gateway)?;
    let match_count = all_models_count(&s).unwrap_or(0);
    if match_count == 0 {
        // Esc clears the search text, second Esc closes the picker.
        let _ = key(pane_id, "Escape", 250).await;
        let _ = key(pane_id, "Escape", 100).await;
        return Err((
            StatusCode::NOT_FOUND,
            "model not available in this session (provider not configured?)".into(),
        ));
    }

    // Already the default? The details panel tags the highlighted model's
    // assigned roles — but Nerd Font glyphs sit flush against the word
    // ("\u{f111}default \u{f074} · …") and some glyphs are dropped entirely
    // by the plain-text read, so strip private-use codepoints before
    // matching. Enter on an assigned model TOGGLES the role off (verified
    // live: "Default role cleared — auto-selection applies"), so an
    // assigned target means close and report success without touching it.
    // Only trustworthy when the match is unique — the tag describes the
    // highlighted row, which is the target iff it is the only row.
    let already_default = match_count == 1
        && s.lines().any(|l| {
            strip_glyphs(l)
                .trim_matches(|c: char| c == '\u{2502}' || c.is_whitespace())
                .starts_with("default")
        });
    if already_default {
        key(pane_id, "Escape", 250).await.map_err(gateway)?; // clear search
        key(pane_id, "Escape", 400).await.map_err(gateway)?; // close picker
        return Ok(());
    }

    // Role menu: the footer line names the chosen model — the identity check
    // that catches fuzzy search highlighting a different row. Enter is only
    // safe on the CLEAN "[ default ]" slot; an assigned slot carries a level
    // glyph inside the brackets ("[ \u{f111}default \u{f074} ]") or leaves a
    // double-space artifact where the read dropped the glyph, and Enter
    // there would CLEAR the role.
    key(pane_id, "Enter", 800).await.map_err(gateway)?;
    let s = screen_text(pane_id).await.map_err(gateway)?;
    let role_line = s
        .lines()
        .find(|l| l.contains(&format!("{id_part} \u{2192}")))
        .unwrap_or("");
    let unwind_role = || async {
        let _ = key(pane_id, "Escape", 250).await; // role menu -> picker
        let _ = key(pane_id, "Escape", 250).await; // clear search
        let _ = key(pane_id, "Escape", 100).await; // close picker
    };
    if role_line.is_empty() {
        unwind_role().await;
        return Err((
            StatusCode::CONFLICT,
            "picker highlighted a different model".into(),
        ));
    }
    let slot = role_line
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(inner, _)| inner)
        .unwrap_or("");
    let has_glyph = slot.chars().any(is_private_use);
    let clean_default = !has_glyph && slot.trim() == "default" && !slot.contains("  ");
    if !clean_default {
        unwind_role().await;
        if strip_glyphs(slot).contains("default") {
            // Already assigned — reported as success, nothing was toggled.
            return Ok(());
        }
        return Err((
            StatusCode::BAD_GATEWAY,
            "role menu did not offer a default slot".into(),
        ));
    }

    // Assign `default`, keep the pre-highlighted reasoning level. All picker
    // Enters pace at 800ms — omp debounces rapid TUI keys and a coalesced
    // Enter silently drops a stage (verified live: a 550ms chain lost the
    // assign step and left the old model active).
    key(pane_id, "Enter", 800).await.map_err(gateway)?;
    let s = screen_text(pane_id).await.map_err(gateway)?;
    if !s.contains("inherit") {
        let _ = key(pane_id, "Escape", 250).await;
        let _ = key(pane_id, "Escape", 250).await;
        let _ = key(pane_id, "Escape", 100).await;
        return Err((
            StatusCode::BAD_GATEWAY,
            "reasoning level menu did not open".into(),
        ));
    }
    // Confirm the level; verify the level menu actually closed (its
    // "inherit" footer is the only marker distinguishing it from the
    // picker, whose "All available models" header stays visible under
    // every submenu). One paced retry covers a debounced Enter.
    key(pane_id, "Enter", 800).await.map_err(gateway)?;
    let mut s = screen_text(pane_id).await.map_err(gateway)?;
    if s.contains("inherit") {
        key(pane_id, "Enter", 800).await.map_err(gateway)?;
        s = screen_text(pane_id).await.map_err(gateway)?;
        if s.contains("inherit") {
            let _ = key(pane_id, "Escape", 250).await;
            let _ = key(pane_id, "Escape", 250).await;
            let _ = key(pane_id, "Escape", 100).await;
            return Err((
                StatusCode::BAD_GATEWAY,
                "role assignment did not register".into(),
            ));
        }
    }

    // Close: Esc first clears the search text, then closes the picker; a
    // submenu adds a level. Loop while the picker is verifiably open so a
    // bare Escape never reaches the composer of a running agent.
    for _ in 0..4 {
        if !screen_text(pane_id)
            .await
            .map_err(gateway)?
            .contains("All available models")
        {
            break;
        }
        key(pane_id, "Escape", 500).await.map_err(gateway)?;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Receipt: omp prints the assignment at the bottom of the transcript,
    // directly above the composer. Only accept it there — receipts from
    // earlier switches linger in the visible scrollback.
    let s = screen_text(pane_id).await.map_err(gateway)?;
    let receipt_fresh = s
        .lines()
        .filter(|l| !l.trim().is_empty())
        .rev()
        .take(5)
        .any(|l| l.contains(&format!("Default model: {selector}")));
    if !receipt_fresh {
        return Err((
            StatusCode::BAD_GATEWAY,
            "switch sent but not confirmed by terminal receipt".into(),
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
struct AskBody {
    index: usize,
}

/// Answer a pending single-select ask. Pointer plan is computed from disk
/// state only: re-verify the ask is still pending, then move relative to the
/// picker's initial position (index 0; adjusted if omp preselects
/// `recommended` — verified live during smoke testing).
async fn post_ask(
    State(state): State<Shared>,
    Path(pane_id): Path<String>,
    Json(body): Json<AskBody>,
) -> impl IntoResponse {
    let Some(path) = session_path_for(&state, &pane_id).await else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no transcript for pane"})),
        )
            .into_response();
    };
    let transcript = {
        let path = path.clone();
        tokio::task::spawn_blocking(move || omp::parse_session(&path))
            .await
            .unwrap_or_default()
    };
    let Some(ask) = transcript.pending_ask else {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "ask no longer pending"})),
        )
            .into_response();
    };
    if ask.multi {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "multi-select not supported yet; use keys"})),
        )
            .into_response();
    }
    if body.index >= ask.options.len() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "option index out of range"})),
        )
            .into_response();
    }
    // Smoke-verified: omp preselects `recommended` when set, else the top.
    let start = ask.recommended.unwrap_or(0).min(ask.options.len() - 1);
    let mut keys: Vec<String> = Vec::new();
    let (dir, n) = if body.index >= start {
        ("Down", body.index - start)
    } else {
        ("Up", start - body.index)
    };
    keys.extend(std::iter::repeat_n(dir.to_string(), n));
    keys.push("Enter".to_string());
    match herdr::send_keys(&pane_id, &keys).await {
        Ok(()) => Json(json!({"ok": true})).into_response(),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": err.to_string()})),
        )
            .into_response(),
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

/// Force revalidation on every request. The frontend is served as plain ES
/// modules with no build step; without this, mobile Safari's heuristic cache
/// keeps stale modules across deploys (the classic "why is my fix not live"
/// PWA trap). Everything is same-host and tiny, so 304 round-trips are cheap.
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
