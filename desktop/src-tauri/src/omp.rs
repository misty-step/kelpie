//! OMP session JSONL → transcript projection.
//!
//! Incremental and offset-based: live sessions grow past 200 MB, so refresh
//! only parses appended bytes. If the file shrinks or is replaced (omp
//! compaction, new session in the same path), the projection resets and
//! reparses. Event shapes match the kelpie bridge's `src/omp.rs`.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};

pub const RESULT_CLIP: usize = 4000;
pub const SNIPPET_CLIP: usize = 140;
const TAIL_BYTES: u64 = 512 * 1024;
const FILE_FINGERPRINT_BYTES: u64 = 1024;

fn file_identity(metadata: &std::fs::Metadata) -> FileIdentity {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        FileIdentity {
            dev: metadata.dev(),
            ino: metadata.ino(),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        FileIdentity { dev: 0, ino: 0 }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct FileIdentity {
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
}

fn read_window(path: &str, start: u64, length: u64) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(length).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn file_prefix(path: &str, length: u64) -> std::io::Result<Vec<u8>> {
    read_window(path, 0, length)
}

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
    /// Session-level markers: compaction, model change, title change.
    /// Rendered as a visual divider so resets are obvious in the transcript.
    System {
        label: String,
        detail: Option<String>,
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

#[derive(Default)]
pub struct Projection {
    pub title: Option<String>,
    pub entries: Vec<Entry>,
    pub model: Option<ModelInfo>,
    pub thinking: Option<String>,
    pub pending_ask: Option<Ask>,
    open_tools: HashMap<String, (usize, Option<Ask>)>,
    pub offset: u64,
    identity: Option<FileIdentity>,
    prefix: Vec<u8>,
    suffix: Vec<u8>,
}

impl Projection {
    pub fn new() -> Self {
        Self::default()
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    /// True when the file was replaced or rewritten in place: same-path
    /// sessions must not mix events across incarnations.
    fn replace_required(&self, metadata: &std::fs::Metadata, prefix: &[u8], prior_tail: &[u8]) -> bool {
        let Some(identity) = self.identity else {
            return false;
        };
        if identity != file_identity(metadata) || metadata.len() < self.offset {
            return true;
        }
        if prefix.len() < self.prefix.len()
            || prefix.get(..self.prefix.len()) != Some(self.prefix.as_slice())
        {
            return true;
        }
        prior_tail != self.suffix.as_slice()
    }

    pub fn refresh(&mut self, path: &str) -> std::io::Result<()> {
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.reset();
                return Err(error);
            }
        };
        let prefix = file_prefix(path, FILE_FINGERPRINT_BYTES.min(metadata.len()))?;
        let prior_tail = if self.offset == 0 {
            Vec::new()
        } else {
            read_window(
                path,
                self.offset.saturating_sub(FILE_FINGERPRINT_BYTES),
                FILE_FINGERPRINT_BYTES.min(self.offset),
            )?
        };
        if self.replace_required(&metadata, &prefix, &prior_tail) {
            self.reset();
        }

        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(self.offset))?;
        let mut reader = BufReader::new(file);
        let mut consumed = self.offset;
        loop {
            let mut raw = String::new();
            let n = reader.read_line(&mut raw)?;
            if n == 0 {
                break;
            }
            let terminated = raw.ends_with('\n');
            let text = raw.trim_end_matches(['\r', '\n']);
            match serde_json::from_str::<Value>(text) {
                Ok(event) => {
                    consumed = consumed.saturating_add(n as u64);
                    self.apply(&event);
                }
                Err(_) if !terminated => break, // partial trailing record
                Err(_) => {
                    consumed = consumed.saturating_add(n as u64);
                }
            }
        }
        self.offset = consumed;
        self.identity = Some(file_identity(&metadata));
        self.prefix = prefix;
        self.suffix = if self.offset == 0 {
            Vec::new()
        } else {
            read_window(
                path,
                self.offset.saturating_sub(FILE_FINGERPRINT_BYTES),
                FILE_FINGERPRINT_BYTES.min(self.offset),
            )?
        };
        self.update_pending_ask();
        Ok(())
    }

    fn apply(&mut self, event: &Value) {
        let ts = event
            .get("timestamp")
            .and_then(Value::as_str)
            .map(String::from);
        match event.get("type").and_then(Value::as_str) {
            Some("session") | Some("title") => {
                if let Some(title) = event.get("title").and_then(Value::as_str) {
                    self.title = Some(title.to_string());
                }
            }
            Some("message") => {
                let Some(message) = event.get("message") else {
                    return;
                };
                match message.get("role").and_then(Value::as_str) {
                    Some("user") => {
                        let text = content_text(message.get("content").unwrap_or(&Value::Null));
                        if !text.is_empty() {
                            self.entries.push(Entry::User { text, ts });
                        }
                    }
                    Some("assistant") => {
                        if let (Some(provider), Some(model)) = (
                            message.get("provider").and_then(Value::as_str),
                            message.get("model").and_then(Value::as_str),
                        ) {
                            self.model = Some(ModelInfo {
                                provider: provider.to_string(),
                                model: model.to_string(),
                            });
                        }
                        let Some(items) = message.get("content").and_then(Value::as_array) else {
                            return;
                        };
                        for item in items {
                            match item.get("type").and_then(Value::as_str) {
                                Some("text") => {
                                    let text =
                                        item.get("text").and_then(Value::as_str).unwrap_or("");
                                    if !text.trim().is_empty() {
                                        self.entries.push(Entry::Assistant {
                                            text: text.to_string(),
                                            ts: ts.clone(),
                                        });
                                    }
                                }
                                Some("thinking") => {
                                    let text =
                                        item.get("thinking").and_then(Value::as_str).unwrap_or("");
                                    if !text.trim().is_empty() {
                                        self.entries.push(Entry::Thinking {
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
                                    self.entries.push(Entry::Tool {
                                        name,
                                        intent,
                                        status: "pending".to_string(),
                                        result: None,
                                        ts: ts.clone(),
                                    });
                                    if let Some(call_id) = call_id {
                                        self.open_tools.insert(
                                            call_id.to_string(),
                                            (self.entries.len() - 1, ask),
                                        );
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some("toolResult") => {
                        let Some(call_id) = message.get("toolCallId").and_then(Value::as_str)
                        else {
                            return;
                        };
                        if let Some((index, _ask)) = self.open_tools.remove(call_id) {
                            if let Some(Entry::Tool { status, result, .. }) =
                                self.entries.get_mut(index)
                            {
                                let is_error = message
                                    .get("isError")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false);
                                *status = if is_error { "error" } else { "ok" }.to_string();
                                let text =
                                    content_text(message.get("content").unwrap_or(&Value::Null));
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
                if let Some(selector) = event.get("model").and_then(Value::as_str) {
                    if let Some((provider, model)) = selector.split_once('/') {
                        self.model = Some(ModelInfo {
                            provider: provider.to_string(),
                            model: model.to_string(),
                        });
                    }
                    self.entries.push(Entry::System {
                        label: "Model changed".to_string(),
                        detail: Some(selector.to_string()),
                        ts,
                    });
                }
            }
            Some("thinking_level_change") => {
                let configured = event.get("configured").and_then(Value::as_str);
                let effective = event.get("thinkingLevel").and_then(Value::as_str);
                if let Some(level) = configured.or(effective) {
                    self.thinking = Some(level.to_string());
                    self.entries.push(Entry::System {
                        label: "Thinking level".to_string(),
                        detail: Some(level.to_string()),
                        ts,
                    });
                } else if event.get("configured").is_some() || event.get("thinkingLevel").is_some()
                {
                    self.thinking = Some("off".to_string());
                }
            }
            Some("compaction") => {
                let summary = event
                    .get("summary")
                    .and_then(Value::as_str)
                    .map(|s| clip(s, 220));
                self.entries.push(Entry::System {
                    label: "Context compacted".to_string(),
                    detail: summary.or_else(|| {
                        Some("Earlier turns were archived to reclaim context.".to_string())
                    }),
                    ts,
                });
            }
            Some("title_change") => {
                if let Some(title) = event.get("title").and_then(Value::as_str) {
                    self.title = Some(title.to_string());
                    self.entries.push(Entry::System {
                        label: "Title updated".to_string(),
                        detail: Some(title.to_string()),
                        ts,
                    });
                }
            }
            _ => {}
        }
    }

    fn update_pending_ask(&mut self) {
        let mut best: Option<(usize, Ask)> = None;
        for (index, ask) in self.open_tools.values() {
            if let Some(ask) = ask {
                if best
                    .as_ref()
                    .is_none_or(|(best_index, _)| index > best_index)
                {
                    best = Some((*index, ask.clone()));
                }
            }
        }
        self.pending_ask = best.and_then(|(index, ask)| {
            (self.entries.len().saturating_sub(index) <= 6).then_some(ask)
        });
    }

    /// Newest `limit` entries ending before `before` (or the tail), with
    /// absolute entry indices so overlapping pages stay stable.
    pub fn page(&self, before: Option<usize>, limit: usize) -> Page {
        let total = self.entries.len();
        let before = before.unwrap_or(total).min(total);
        let start = before.saturating_sub(limit);
        let entries = self.entries[start..before]
            .iter()
            .enumerate()
            .map(|(i, entry)| (start + i, entry.clone()))
            .collect();
        Page {
            entries,
            total,
            has_older: start > 0,
        }
    }
}

pub struct Page {
    pub entries: Vec<(usize, Entry)>,
    pub total: usize,
    pub has_older: bool,
}

/// Cheap fleet-side summary from a bounded tail scan: never parses the whole
/// file. Mirrors the bridge's `summary()` and `update_pending_ask` semantics
/// as closely as a bounded scan allows: entries are counted exactly like the
/// projection counts them (user/assistant/thinking/toolCall append entries;
/// toolResult only updates an open tool). A pending ask only counts when its
/// tool call sits within the last six entries.
#[derive(Clone, Debug, Default)]
pub struct PaneSummary {
    pub snippet: Option<String>,
    pub pending_ask: bool,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

fn parse_event_meta(event: &Value, res: &mut PaneSummary) {
    let evt_type = event.get("type").and_then(Value::as_str);
    if evt_type == Some("model_change") {
        if let Some(selector) = event.get("model").and_then(Value::as_str) {
            if let Some((prov, mod_name)) = selector.split_once('/') {
                res.provider = Some(prov.to_string());
                res.model = Some(mod_name.to_string());
            } else {
                res.model = Some(selector.to_string());
            }
        }
    }
    if evt_type == Some("thinking_level_change") {
        let lvl_val = event.get("configured")
            .and_then(Value::as_str)
            .or_else(|| event.get("thinkingLevel").and_then(Value::as_str))
            .or_else(|| event.get("thinking_level").and_then(Value::as_str))
            .or_else(|| event.get("level").and_then(Value::as_str));
        if let Some(lvl) = lvl_val {
            let lvl_lower = lvl.to_lowercase();
            let formatted = match lvl_lower.as_str() {
                "xhigh" | "extra_high" | "extra high" | "max" => "Max",
                "high" => "High",
                "medium" => "Medium",
                "low" => "Low",
                "off" | "none" | "0" => "Off",
                _ => lvl,
            };
            res.effort = Some(formatted.to_string());
        }
    }
    if let Some(message) = event.get("message") {
        if let (Some(prov), Some(mod_name)) = (
            message.get("provider").and_then(Value::as_str),
            message.get("model").and_then(Value::as_str),
        ) {
            res.provider = Some(prov.to_string());
            res.model = Some(mod_name.to_string());
        }
        let msg_lvl = message.get("configured")
            .and_then(Value::as_str)
            .or_else(|| message.get("thinkingLevel").and_then(Value::as_str))
            .or_else(|| message.get("thinking_level").and_then(Value::as_str));
        if let Some(lvl) = msg_lvl {
            let lvl_lower = lvl.to_lowercase();
            let formatted = match lvl_lower.as_str() {
                "xhigh" | "extra_high" | "extra high" | "max" => "Max",
                "high" => "High",
                "medium" => "Medium",
                "low" => "Low",
                "off" | "none" | "0" => "Off",
                _ => lvl,
            };
            res.effort = Some(formatted.to_string());
        }
    }
}

pub fn tail_summary(path: &str) -> PaneSummary {
    let mut res = PaneSummary::default();
    let Ok(meta) = std::fs::metadata(path) else {
        return res;
    };
    let len = meta.len();
    if len == 0 {
        return res;
    }
    let Ok(mut file) = File::open(path) else {
        return res;
    };

    // First scan head 8KB to seed provider, model, and thinking_level from session setup
    let reader_head = BufReader::new(&file);
    for line in reader_head.lines().take(60).flatten() {
        if let Ok(event) = serde_json::from_str::<Value>(&line) {
            parse_event_meta(&event, &mut res);
        }
    }

    // Now scan tail 32KB for recent messages, tool calls, pending asks, and model changes
    if file.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES))).is_err() {
        return res;
    }
    let reader_tail = BufReader::new(file);
    let mut open_asks: HashMap<String, u64> = HashMap::new();
    let mut seq: u64 = 0;
    for line in reader_tail.lines().flatten() {
        let Ok(event) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        parse_event_meta(&event, &mut res);
        let Some(message) = event.get("message") else {
            continue;
        };
        let Some(role) = message.get("role").and_then(Value::as_str) else {
            continue;
        };
        match role {
            "user" => {
                let text = content_text(message.get("content").unwrap_or(&Value::Null));
                if !text.trim().is_empty() {
                    seq += 1;
                    res.snippet = Some(clip(&text, SNIPPET_CLIP));
                }
            }
            "assistant" => {
                if let Some(items) = message.get("content").and_then(Value::as_array) {
                    for item in items {
                        match item.get("type").and_then(Value::as_str) {
                            Some("text") => {
                                let text =
                                    item.get("text").and_then(Value::as_str).unwrap_or("");
                                if !text.trim().is_empty() {
                                    seq += 1;
                                    res.snippet = text
                                        .lines()
                                        .rev()
                                        .find(|l| !l.trim().is_empty())
                                        .map(|l| clip(l.trim(), SNIPPET_CLIP));
                                }
                            }
                            Some("thinking") => {
                                let text =
                                    item.get("thinking").and_then(Value::as_str).unwrap_or("");
                                if !text.trim().is_empty() {
                                    seq += 1;
                                }
                            }
                            Some("toolCall") => {
                                seq += 1;
                                let name = item.get("name").and_then(Value::as_str).unwrap_or("tool");
                                let args =
                                    item.get("arguments").cloned().unwrap_or(Value::Null);
                                let intent = args
                                    .get("i")
                                    .or_else(|| args.get("intent"))
                                    .and_then(Value::as_str);
                                let call_id = item.get("id").and_then(Value::as_str).unwrap_or("");
                                if name == "ask" && !call_id.is_empty() {
                                    open_asks.insert(call_id.to_string(), seq);
                                }
                                res.snippet = Some(clip(
                                    &format!(
                                        "⚒ {}{}",
                                        name,
                                        intent
                                            .map(|intent| format!(" — {intent}"))
                                            .unwrap_or_default()
                                    ),
                                    SNIPPET_CLIP,
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
            "toolResult" => {
                let call_id = message.get("toolCallId").and_then(Value::as_str).unwrap_or("");
                open_asks.remove(call_id);
            }
            _ => {}
        }
    }
    res.pending_ask = open_asks.values().any(|s| seq.saturating_sub(*s) <= 6);
    res
}
