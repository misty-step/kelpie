//! omp session JSONL -> semantic transcript. The session file is an append-
//! oriented tree of entries; reading it while the session writes is safe
//! (verified upstream: session-loader is explicitly read-only tolerant).
//!
//! We render in file order (append order ~= chronological), which is exactly
//! what a phone triage view needs. Pending ask detection: an `ask` toolCall
//! with no matching toolResult yet.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

const RESULT_CLIP: usize = 4000;
const SNIPPET_CLIP: usize = 140;

#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    User {
        text: String,
        ts: Option<String>,
    },
    Assistant {
        text: String,
        ts: Option<String>,
    },
    Thinking {
        text: String,
        ts: Option<String>,
    },
    Tool {
        name: String,
        intent: Option<String>,
        status: String, // pending | ok | error
        result: Option<String>,
        ts: Option<String>,
    },
}

#[derive(Serialize, Clone)]
pub struct AskOption {
    pub label: String,
    pub description: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct Ask {
    pub call_id: String,
    pub question: String,
    pub options: Vec<AskOption>,
    pub multi: bool,
    pub recommended: Option<usize>,
}

#[derive(Serialize, Clone)]
pub struct ModelInfo {
    pub provider: String,
    pub model: String,
}

#[derive(Serialize, Clone, Default)]
pub struct Transcript {
    pub title: Option<String>,
    pub entries: Vec<Entry>,
    pub pending_ask: Option<Ask>,
    /// Latest assistant message's provider/model.
    pub model: Option<ModelInfo>,
    /// Latest thinking level (e.g. "high", "xhigh").
    pub thinking: Option<String>,
}

fn content_text(content: &Value) -> String {
    let Some(items) = content.as_array() else {
        return content.as_str().unwrap_or_default().to_string();
    };
    let mut out = Vec::new();
    for item in items {
        match item.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(t) = item.get("text").and_then(Value::as_str) {
                    if !t.is_empty() {
                        out.push(t.to_string());
                    }
                }
            }
            Some("image") => out.push("[image]".to_string()),
            _ => {}
        }
    }
    out.join("\n")
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let clipped: String = s.chars().take(max).collect();
    format!("{clipped}…")
}

fn parse_ask(args: &Value, call_id: &str) -> Option<Ask> {
    let q = args.get("questions")?.as_array()?.first()?;
    let options = q
        .get("options")?
        .as_array()?
        .iter()
        .filter_map(|o| {
            Some(AskOption {
                label: o.get("label")?.as_str()?.to_string(),
                description: o
                    .get("description")
                    .and_then(Value::as_str)
                    .map(String::from),
            })
        })
        .collect::<Vec<_>>();
    if options.is_empty() {
        return None;
    }
    Some(Ask {
        call_id: call_id.to_string(),
        question: q
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or("(question)")
            .to_string(),
        options,
        multi: q.get("multi").and_then(Value::as_bool).unwrap_or(false),
        recommended: q
            .get("recommended")
            .and_then(Value::as_u64)
            .map(|n| n as usize),
    })
}

pub fn parse_session(path: &str) -> Transcript {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Transcript::default();
    };
    let mut t = Transcript::default();
    // toolCallId -> (entries index, ask payload if the tool is `ask`)
    let mut open_tools: HashMap<String, (usize, Option<Ask>)> = HashMap::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // A partially-written trailing line simply fails to parse; skip it.
        let Ok(e) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let ts = e.get("timestamp").and_then(Value::as_str).map(String::from);
        match e.get("type").and_then(Value::as_str) {
            Some("session") | Some("title") | Some("title_change") => {
                if let Some(title) = e.get("title").and_then(Value::as_str) {
                    t.title = Some(title.to_string());
                }
            }
            Some("message") => {
                let Some(msg) = e.get("message") else {
                    continue;
                };
                match msg.get("role").and_then(Value::as_str) {
                    Some("user") => {
                        let text = content_text(msg.get("content").unwrap_or(&Value::Null));
                        if !text.is_empty() {
                            t.entries.push(Entry::User {
                                text,
                                ts: ts.clone(),
                            });
                        }
                    }
                    Some("assistant") => {
                        if let (Some(provider), Some(model)) = (
                            msg.get("provider").and_then(Value::as_str),
                            msg.get("model").and_then(Value::as_str),
                        ) {
                            t.model = Some(ModelInfo {
                                provider: provider.to_string(),
                                model: model.to_string(),
                            });
                        }
                        let Some(items) = msg.get("content").and_then(Value::as_array) else {
                            continue;
                        };
                        for item in items {
                            match item.get("type").and_then(Value::as_str) {
                                Some("text") => {
                                    let text =
                                        item.get("text").and_then(Value::as_str).unwrap_or("");
                                    if !text.trim().is_empty() {
                                        t.entries.push(Entry::Assistant {
                                            text: text.to_string(),
                                            ts: ts.clone(),
                                        });
                                    }
                                }
                                Some("thinking") => {
                                    let text =
                                        item.get("thinking").and_then(Value::as_str).unwrap_or("");
                                    if !text.trim().is_empty() {
                                        t.entries.push(Entry::Thinking {
                                            text: text.to_string(),
                                            ts: ts.clone(),
                                        });
                                    }
                                }
                                Some("toolCall") => {
                                    let name = item
                                        .get("name")
                                        .and_then(Value::as_str)
                                        .unwrap_or("tool")
                                        .to_string();
                                    let args =
                                        item.get("arguments").cloned().unwrap_or(Value::Null);
                                    let intent = args
                                        .get("i")
                                        .or_else(|| args.get("intent"))
                                        .and_then(Value::as_str)
                                        .map(String::from);
                                    let call_id = item.get("id").and_then(Value::as_str);
                                    let ask = (name == "ask")
                                        .then(|| parse_ask(&args, call_id.unwrap_or("")))
                                        .flatten();
                                    t.entries.push(Entry::Tool {
                                        name,
                                        intent,
                                        status: "pending".to_string(),
                                        result: None,
                                        ts: ts.clone(),
                                    });
                                    if let Some(id) = call_id {
                                        open_tools
                                            .insert(id.to_string(), (t.entries.len() - 1, ask));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some("toolResult") => {
                        let Some(id) = msg.get("toolCallId").and_then(Value::as_str) else {
                            continue;
                        };
                        if let Some((idx, _ask)) = open_tools.remove(id) {
                            if let Some(Entry::Tool { status, result, .. }) = t.entries.get_mut(idx)
                            {
                                let is_err =
                                    msg.get("isError").and_then(Value::as_bool).unwrap_or(false);
                                *status = if is_err { "error" } else { "ok" }.to_string();
                                let text = content_text(msg.get("content").unwrap_or(&Value::Null));
                                if !text.is_empty() {
                                    *result = Some(clip(&text, RESULT_CLIP));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some("model_change") => {
                if let Some(selector) = e.get("model").and_then(Value::as_str) {
                    if let Some((provider, model)) = selector.split_once('/') {
                        t.model = Some(ModelInfo {
                            provider: provider.to_string(),
                            model: model.to_string(),
                        });
                    }
                }
            }
            Some("thinking_level_change") => {
                let configured = e.get("configured").and_then(Value::as_str);
                let effective = e.get("thinkingLevel").and_then(Value::as_str);
                if let Some(level) = configured.or(effective) {
                    t.thinking = Some(level.to_string());
                } else if e.get("configured").is_some() || e.get("thinkingLevel").is_some() {
                    t.thinking = Some("off".to_string());
                }
            }
            _ => {}
        }
    }

    // Pending ask: newest unresolved `ask` toolCall, but only if it is still
    // live — i.e. nothing later in the transcript has moved past it.
    let mut best: Option<(usize, Ask)> = None;
    for (idx, ask) in open_tools.into_values() {
        if let Some(ask) = ask {
            if best.as_ref().is_none_or(|(b, _)| idx > *b) {
                best = Some((idx, ask));
            }
        }
    }
    if let Some((idx, ask)) = best {
        // Live only when it's among the last few entries (abandoned branches
        // or superseded asks deeper in history don't count).
        if t.entries.len().saturating_sub(idx) <= 6 {
            t.pending_ask = Some(ask);
        }
    }
    t
}

/// Cheap summary for the fleet view: last visible line + pending-ask flag.
pub struct Summary {
    pub title: Option<String>,
    pub snippet: Option<String>,
    pub pending_ask: bool,
}

pub fn summarize(path: &str) -> Summary {
    let t = parse_session(path);
    let snippet = t.entries.iter().rev().find_map(|e| match e {
        Entry::Assistant { text, .. } | Entry::User { text, .. } => {
            let line = text.lines().rev().find(|l| !l.trim().is_empty())?;
            Some(clip(line.trim(), SNIPPET_CLIP))
        }
        Entry::Tool { name, intent, .. } => Some(clip(
            &format!(
                "⚒ {}{}",
                name,
                intent
                    .as_deref()
                    .map(|i| format!(" — {i}"))
                    .unwrap_or_default()
            ),
            SNIPPET_CLIP,
        )),
        Entry::Thinking { .. } => None,
    });
    Summary {
        title: t.title,
        snippet,
        pending_ask: t.pending_ask.is_some(),
    }
}
