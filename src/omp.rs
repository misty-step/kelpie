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
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;

const RESULT_CLIP: usize = 4000;
const SNIPPET_CLIP: usize = 140;
static NEXT_PROJECTION_GENERATION: LazyLock<AtomicU64> = LazyLock::new(|| {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let seed = (nanos as u64) ^ ((nanos >> 64) as u64) ^ (u64::from(std::process::id()) << 32);
    AtomicU64::new(seed.max(1))
});

fn next_projection_generation() -> u64 {
    NEXT_PROJECTION_GENERATION.fetch_add(1, Ordering::Relaxed)
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

#[derive(Clone, Debug, PartialEq)]
pub enum AskReceipt {
    Pending,
    Confirmed(Vec<String>),
    Error,
    Malformed,
}

/// Byte offset from which the next appended JSONL record must be scanned.
pub fn user_message_cursor(path: &str) -> u64 {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

/// Scan only records appended after `cursor`, advancing it past complete lines.
/// A partially written trailing record remains unread for the next poll.
pub fn scan_new_user_message(path: &str, cursor: &mut u64, expected: &str) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    if file.seek(std::io::SeekFrom::Start(*cursor)).is_err() {
        return false;
    }
    let mut reader = BufReader::new(file);
    loop {
        let mut raw_line = String::new();
        let Ok(read) = reader.read_line(&mut raw_line) else {
            return false;
        };
        if read == 0 {
            return false;
        }
        let terminated = raw_line.ends_with('\n');
        let parsed = serde_json::from_str::<Value>(raw_line.trim_end());
        if !terminated && parsed.is_err() {
            return false;
        }
        *cursor = cursor.saturating_add(read as u64);
        let Ok(event) = parsed else {
            continue;
        };
        if event.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(message) = event.get("message") else {
            continue;
        };
        if message.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let text = content_text(message.get("content").unwrap_or(&Value::Null));
        if text == expected {
            return true;
        }
    }
}

/// Find the option labels from the original exact ask tool call, even after
/// its toolResult has made the ask no longer pending in parse_session.
pub fn ask_option_labels(path: &str, call_id: &str) -> Option<Vec<String>> {
    let raw = std::fs::read_to_string(path).ok()?;
    for line in raw.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if event.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(message) = event.get("message") else {
            continue;
        };
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(items) = message.get("content").and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            if item.get("type").and_then(Value::as_str) != Some("toolCall")
                || item.get("name").and_then(Value::as_str) != Some("ask")
                || item.get("id").and_then(Value::as_str) != Some(call_id)
            {
                continue;
            }
            let Some(options) = item
                .get("arguments")
                .and_then(|args| args.get("questions"))
                .and_then(Value::as_array)
                .and_then(|questions| questions.first())
                .and_then(|question| question.get("options"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            let Some(labels) = options
                .iter()
                .map(|option| {
                    option
                        .get("label")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            return Some(labels);
        }
    }
    None
}

pub fn ask_option_label(path: &str, call_id: &str, index: usize) -> Option<String> {
    ask_option_labels(path, call_id)?.get(index).cloned()
}

/// Scan only the exact OMP tool-result receipt for one ask call. The latest
/// matching receipt wins, so a lost HTTP response converges by readback.
pub fn ask_receipt(path: &str, call_id: &str) -> AskReceipt {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return AskReceipt::Pending;
    };
    for line in raw.lines().rev() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if event.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(message) = event.get("message") else {
            continue;
        };
        if message.get("role").and_then(Value::as_str) != Some("toolResult")
            || message.get("toolCallId").and_then(Value::as_str) != Some(call_id)
        {
            continue;
        }
        if message
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return AskReceipt::Error;
        }
        let Some(selected) = message
            .get("details")
            .and_then(|details| details.get("selectedOptions"))
            .and_then(Value::as_array)
        else {
            return AskReceipt::Malformed;
        };
        let mut values = Vec::with_capacity(selected.len());
        for value in selected {
            let Some(value) = value.as_str() else {
                return AskReceipt::Malformed;
            };
            values.push(value.to_string());
        }
        return AskReceipt::Confirmed(values);
    }
    AskReceipt::Pending
}

#[derive(Serialize, Clone)]
pub struct IndexedEntry {
    pub index: usize,
    #[serde(flatten)]
    pub entry: Entry,
}

#[derive(Serialize, Clone)]
pub struct SessionPage {
    pub title: Option<String>,
    pub entries: Vec<IndexedEntry>,
    pub pending_ask: Option<Ask>,
    pub model: Option<ModelInfo>,
    pub thinking: Option<String>,
    pub total_entries: usize,
    pub start_index: usize,
    pub has_older: bool,
    /// Monotonic session incarnation; changes when the source is replaced.
    pub generation: u64,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct FileIdentity {
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
}

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
        FileIdentity::default()
    }
}

const FILE_FINGERPRINT_BYTES: u64 = 1024;

fn read_window(path: &str, start: u64, length: u64) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(length).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn file_prefix(path: &str, length: u64) -> std::io::Result<Vec<u8>> {
    read_window(path, 0, length)
}

/// Stateful append-only projection of one OMP JSONL session.
///
/// The byte offset is advanced only after a complete JSONL record is consumed.
/// Entries are never removed while appending, so vector positions are stable
/// absolute semantic indices. A tool result mutates its indexed tool entry via
/// the open-tool map rather than appending a duplicate entry.
pub struct SessionProjection {
    transcript: Transcript,
    open_tools: HashMap<String, (usize, Option<Ask>)>,
    offset: u64,
    generation: u64,
    identity: Option<FileIdentity>,
    prefix: Vec<u8>,
    suffix: Vec<u8>,
}

impl Default for SessionProjection {
    fn default() -> Self {
        Self {
            transcript: Transcript::default(),
            open_tools: HashMap::new(),
            offset: 0,
            generation: next_projection_generation(),
            identity: None,
            prefix: Vec::new(),
            suffix: Vec::new(),
        }
    }
}

impl SessionProjection {
    fn reset(&mut self) {
        self.generation = next_projection_generation();
        self.transcript = Transcript::default();
        self.open_tools.clear();
        self.offset = 0;
        self.identity = None;
        self.prefix.clear();
        self.suffix.clear();
    }

    fn replace_required(
        &self,
        metadata: &std::fs::Metadata,
        prefix: &[u8],
        prior_tail: &[u8],
    ) -> bool {
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
        // Verify the bounded tail at the old offset. Ordinary appends preserve
        // these bytes; in-place replacement cannot silently reuse the suffix.
        prior_tail != self.suffix.as_slice()
    }

    fn apply_event(&mut self, event: Value) {
        let ts = event
            .get("timestamp")
            .and_then(Value::as_str)
            .map(String::from);
        match event.get("type").and_then(Value::as_str) {
            Some("session") | Some("title") | Some("title_change") => {
                if let Some(title) = event.get("title").and_then(Value::as_str) {
                    self.transcript.title = Some(title.to_string());
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
                            self.transcript.entries.push(Entry::User { text, ts });
                        }
                    }
                    Some("assistant") => {
                        if let (Some(provider), Some(model)) = (
                            message.get("provider").and_then(Value::as_str),
                            message.get("model").and_then(Value::as_str),
                        ) {
                            self.transcript.model = Some(ModelInfo {
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
                                        self.transcript.entries.push(Entry::Assistant {
                                            text: text.to_string(),
                                            ts: ts.clone(),
                                        });
                                    }
                                }
                                Some("thinking") => {
                                    let text =
                                        item.get("thinking").and_then(Value::as_str).unwrap_or("");
                                    if !text.trim().is_empty() {
                                        self.transcript.entries.push(Entry::Thinking {
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
                                    self.transcript.entries.push(Entry::Tool {
                                        name,
                                        intent,
                                        status: "pending".to_string(),
                                        result: None,
                                        ts: ts.clone(),
                                    });
                                    if let Some(call_id) = call_id {
                                        self.open_tools.insert(
                                            call_id.to_string(),
                                            (self.transcript.entries.len() - 1, ask),
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
                                self.transcript.entries.get_mut(index)
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
                        self.transcript.model = Some(ModelInfo {
                            provider: provider.to_string(),
                            model: model.to_string(),
                        });
                    }
                }
            }
            Some("thinking_level_change") => {
                let configured = event.get("configured").and_then(Value::as_str);
                let effective = event.get("thinkingLevel").and_then(Value::as_str);
                if let Some(level) = configured.or(effective) {
                    self.transcript.thinking = Some(level.to_string());
                } else if event.get("configured").is_some() || event.get("thinkingLevel").is_some()
                {
                    self.transcript.thinking = Some("off".to_string());
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
        self.transcript.pending_ask = best.and_then(|(index, ask)| {
            (self.transcript.entries.len().saturating_sub(index) <= 6).then_some(ask)
        });
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

        let mut file = std::fs::File::open(path)?;
        file.seek(SeekFrom::Start(self.offset))?;
        let mut reader = BufReader::new(file);
        loop {
            let mut raw_line = String::new();
            let read = reader.read_line(&mut raw_line)?;
            if read == 0 {
                break;
            }
            let terminated = raw_line.ends_with('\n');
            let text = raw_line.trim_end_matches(['\r', '\n']);
            let event = match serde_json::from_str::<Value>(text) {
                Ok(event) => event,
                Err(_) if !terminated => break,
                Err(_) => {
                    self.offset = self.offset.saturating_add(read as u64);
                    continue;
                }
            };
            self.offset = self.offset.saturating_add(read as u64);
            self.apply_event(event);
        }
        self.update_pending_ask();
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
        Ok(())
    }

    pub fn revision(&self) -> u64 {
        self.offset
    }

    pub fn total_entries(&self) -> usize {
        self.transcript.entries.len()
    }

    pub fn pending_ask(&self) -> Option<Ask> {
        self.transcript.pending_ask.clone()
    }

    pub fn page(&self, before: Option<usize>, limit: usize) -> SessionPage {
        let total_entries = self.transcript.entries.len();
        let end = before.unwrap_or(total_entries).min(total_entries);
        let start_index = end.saturating_sub(limit);
        let entries = self.transcript.entries[start_index..end]
            .iter()
            .enumerate()
            .map(|(offset, entry)| IndexedEntry {
                index: start_index + offset,
                entry: entry.clone(),
            })
            .collect();
        SessionPage {
            title: self.transcript.title.clone(),
            entries,
            pending_ask: self.transcript.pending_ask.clone(),
            model: self.transcript.model.clone(),
            thinking: self.transcript.thinking.clone(),
            total_entries,
            start_index,
            has_older: start_index > 0,
            generation: self.generation,
            revision: self.revision(),
        }
    }

    pub fn summary(&self) -> Summary {
        let snippet = self
            .transcript
            .entries
            .iter()
            .rev()
            .find_map(|entry| match entry {
                Entry::Assistant { text, .. } | Entry::User { text, .. } => {
                    let line = text.lines().rev().find(|line| !line.trim().is_empty())?;
                    Some(clip(line.trim(), SNIPPET_CLIP))
                }
                Entry::Tool { name, intent, .. } => Some(clip(
                    &format!(
                        "⚒ {}{}",
                        name,
                        intent
                            .as_deref()
                            .map(|intent| format!(" — {intent}"))
                            .unwrap_or_default()
                    ),
                    SNIPPET_CLIP,
                )),
                Entry::Thinking { .. } => None,
            });
        Summary {
            title: self.transcript.title.clone(),
            snippet,
            pending_ask: self.transcript.pending_ask.is_some(),
        }
    }

    #[cfg(test)]
    fn entry_count(&self) -> usize {
        self.transcript.entries.len()
    }

    #[cfg(test)]
    fn entry(&self, index: usize) -> Option<&Entry> {
        self.transcript.entries.get(index)
    }
}

/// Cheap summary for the fleet view: last visible line plus pending-ask state.
pub struct Summary {
    pub title: Option<String>,
    pub snippet: Option<String>,
    pub pending_ask: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn receipt_scan_correlates_exact_call_id() {
        let path = std::env::temp_dir().join(format!(
            "kelpie-omp-receipt-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let raw = r#"{"type":"message","message":{"role":"assistant","content":[{"type":"toolCall","id":"call-1","name":"ask","arguments":{"questions":[{"options":[{"label":"left"},{"label":"right"}]}]}}]}}
{"type":"message","message":{"role":"toolResult","toolCallId":"other","details":{"selectedOptions":["wrong"]}}}
{"type":"message","message":{"role":"toolResult","toolCallId":"call-1","details":{"selectedOptions":["right"]}}}"#;
        std::fs::write(&path, raw).unwrap();
        assert_eq!(
            ask_receipt(path.to_str().unwrap(), "call-1"),
            AskReceipt::Confirmed(vec!["right".into()])
        );
        assert_eq!(
            ask_option_labels(path.to_str().unwrap(), "call-1"),
            Some(vec!["left".into(), "right".into()])
        );
        assert_eq!(
            ask_option_label(path.to_str().unwrap(), "call-1", 1),
            Some("right".into())
        );
        assert_eq!(
            ask_receipt(path.to_str().unwrap(), "other"),
            AskReceipt::Confirmed(vec!["wrong".into()])
        );
        assert_eq!(
            ask_receipt(path.to_str().unwrap(), "missing"),
            AskReceipt::Pending
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn user_receipt_requires_new_exact_message() {
        let path = std::env::temp_dir().join(format!(
            "kelpie-omp-user-receipt-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let before_raw = r#"{"type":"message","message":{"role":"user","content":"old"}}
{"type":"session"}"#;
        std::fs::write(&path, before_raw).unwrap();
        let mut cursor = user_message_cursor(path.to_str().unwrap());
        std::fs::write(
            &path,
            format!(
                "{}\n{}",
                before_raw, r#"{"type":"message","message":{"role":"user","content":"new"}}"#
            ),
        )
        .unwrap();
        assert!(scan_new_user_message(
            path.to_str().unwrap(),
            &mut cursor,
            "new"
        ));
        assert!(!scan_new_user_message(
            path.to_str().unwrap(),
            &mut cursor,
            "old"
        ));
        let _ = std::fs::remove_file(path);
    }

    fn projection_test_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "kelpie-projection-{label}-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn user_event(text: &str) -> String {
        serde_json::to_string(&serde_json::json!({
            "type": "message",
            "message": {"role": "user", "content": text}
        }))
        .unwrap()
    }

    #[test]
    fn projection_pages_are_bounded_stable_and_flatten_indexed() {
        let path = projection_test_path("pages");
        let lines = (0..300)
            .map(|index| user_event(&format!("message-{index}")))
            .collect::<Vec<_>>();
        std::fs::write(&path, lines.join("\n")).unwrap();
        let mut projection = SessionProjection::default();
        projection.refresh(path.to_str().unwrap()).unwrap();

        let latest = projection.page(None, 160);
        assert_eq!(latest.total_entries, 300);
        assert_eq!(latest.entries.len(), 160);
        assert_eq!(latest.start_index, 140);
        assert!(latest.has_older);
        assert_eq!(latest.entries.first().unwrap().index, 140);
        assert_eq!(latest.entries.last().unwrap().index, 299);

        let older = projection.page(Some(latest.start_index), 160);
        assert_eq!(older.entries.len(), 140);
        assert_eq!(older.start_index, 0);
        assert!(!older.has_older);
        assert_eq!(older.entries.first().unwrap().index, 0);
        assert_eq!(older.entries.last().unwrap().index, 139);

        let encoded = serde_json::to_value(latest.entries.first().unwrap()).unwrap();
        assert_eq!(encoded["index"], 140);
        assert_eq!(encoded["kind"], "user");
        assert_eq!(encoded["text"], "message-140");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn projection_consumes_only_appended_tail_and_updates_open_tool_entry() {
        let path = projection_test_path("tail");
        let tool_call = serde_json::json!({
            "type": "message",
            "message": {"role": "assistant", "content": [{
                "type": "toolCall", "id": "call-1", "name": "search",
                "arguments": {"i": "find the answer"}
            }]}
        });
        std::fs::write(&path, format!("{}\n", tool_call)).unwrap();
        let mut projection = SessionProjection::default();
        projection.refresh(path.to_str().unwrap()).unwrap();
        let first_revision = projection.revision();
        assert_eq!(projection.entry_count(), 1);
        assert!(
            matches!(projection.entry(0), Some(Entry::Tool { status, result: None, .. }) if status == "pending")
        );

        let partial = user_event("tail event");
        let split = partial.len() / 2;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(partial[..split].as_bytes()).unwrap();
        drop(file);
        projection.refresh(path.to_str().unwrap()).unwrap();
        assert_eq!(projection.entry_count(), 1);
        assert_eq!(projection.revision(), first_revision);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(format!("{}\n", &partial[split..]).as_bytes())
            .unwrap();
        drop(file);
        projection.refresh(path.to_str().unwrap()).unwrap();
        assert_eq!(projection.entry_count(), 2);
        assert_eq!(
            projection.revision(),
            std::fs::metadata(&path).unwrap().len()
        );

        let result = serde_json::json!({
            "type": "message",
            "message": {"role": "toolResult", "toolCallId": "call-1", "content": "answer"}
        });
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(format!("\n{}\n", result).as_bytes())
            .unwrap();
        drop(file);
        projection.refresh(path.to_str().unwrap()).unwrap();
        assert_eq!(projection.entry_count(), 2);
        assert!(
            matches!(projection.entry(0), Some(Entry::Tool { status, result: Some(value), .. }) if status == "ok" && value == "answer")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn projection_resets_after_truncation_or_replacement() {
        let path = projection_test_path("reset");
        std::fs::write(
            &path,
            format!("{}\n{}\n", user_event("old"), user_event("keep")),
        )
        .unwrap();
        let mut projection = SessionProjection::default();
        projection.refresh(path.to_str().unwrap()).unwrap();
        assert_eq!(projection.entry_count(), 2);
        let initial_generation = projection.page(None, 160).generation;

        std::fs::write(&path, format!("{}\n", user_event("replacement"))).unwrap();
        projection.refresh(path.to_str().unwrap()).unwrap();
        assert_eq!(projection.entry_count(), 1);
        assert!(projection.page(None, 160).generation > initial_generation);
        assert!(
            matches!(projection.entry(0), Some(Entry::User { text, .. }) if text == "replacement")
        );
        assert_eq!(
            projection.page(None, 160).revision,
            std::fs::metadata(&path).unwrap().len()
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn summary_and_page_share_current_projection_metadata() {
        let path = projection_test_path("summary");
        let raw = [
            serde_json::json!({"type": "title", "title": "Live title"}),
            serde_json::json!({"type": "model_change", "model": "openai/gpt-5"}),
            serde_json::json!({"type": "thinking_level_change", "configured": "high"}),
            serde_json::from_str::<serde_json::Value>(&user_event("latest message")).unwrap(),
        ];
        std::fs::write(
            &path,
            raw.iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
        let mut projection = SessionProjection::default();
        projection.refresh(path.to_str().unwrap()).unwrap();
        let page = projection.page(None, 160);
        let summary = projection.summary();
        assert_eq!(page.title, summary.title);
        assert_eq!(page.pending_ask.is_some(), summary.pending_ask);
        assert_eq!(page.entries.last().unwrap().index, page.total_entries - 1);
        assert_eq!(page.model.as_ref().unwrap().model, "gpt-5");
        assert_eq!(page.thinking.as_deref(), Some("high"));
        let _ = std::fs::remove_file(path);
    }
}
