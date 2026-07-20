use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Fleet {
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
    #[serde(default)]
    pub tabs: Vec<Tab>,
    #[serde(default)]
    pub panes: Vec<Pane>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Workspace {
    #[serde(default)]
    pub id: String,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Tab {
    #[serde(default)]
    pub tab_id: String,
    #[serde(default)]
    pub workspace_id: String,
    pub label: Option<String>,
    #[serde(default)]
    pub pane_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Pane {
    #[serde(default)]
    pub pane_id: String,
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub tab_id: String,
    #[serde(default)]
    pub cwd: String,
    pub agent: Option<String>,
    pub status: Option<String>,
    pub agent_status: Option<String>,
    pub title: Option<String>,
    #[serde(default)]
    pub has_transcript: bool,
    #[serde(default)]
    pub pending_ask: bool,
    pub last_activity: Option<String>,
    pub snippet: Option<String>,
}

impl Pane {
    pub fn status(&self) -> &str {
        self.agent_status
            .as_deref()
            .or(self.status.as_deref())
            .unwrap_or("unknown")
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ModelCost {
    pub input: Option<f64>,
    pub output: Option<f64>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextActionPhase {
    PreSubmit,
    SubmittedAwaitingReceipt,
    Confirmed,
    FailedBeforeSubmit,
    StaleAfterSubmit,
    Unknown,
}

impl Default for TextActionPhase {
    fn default() -> Self {
        Self::PreSubmit
    }
}

impl TextActionPhase {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Confirmed | Self::FailedBeforeSubmit | Self::StaleAfterSubmit
        )
    }

    #[cfg(test)]
    pub fn preserves_draft(self) -> bool {
        !matches!(self, Self::Confirmed)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct TextActionReceipt {
    #[serde(default)]
    pub action_id: String,
    #[serde(default)]
    pub phase: TextActionPhase,
    #[serde(default)]
    pub accepted: bool,
    #[serde(default)]
    pub retryable: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SessionPage {
    pub title: Option<String>,
    #[serde(default)]
    pub pending_ask: Option<Ask>,
    pub model: Option<SessionModel>,
    pub thinking: Option<String>,
    #[serde(default)]
    pub entries: Vec<IndexedEntry>,
    #[serde(default)]
    pub total_entries: usize,
    #[serde(default)]
    pub start_index: usize,
    #[serde(default)]
    pub has_older: bool,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub generation: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct IndexedEntry {
    #[serde(default)]
    pub index: usize,
    #[serde(flatten)]
    pub entry: Entry,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
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
        status: String,
        result: Option<String>,
        ts: Option<String>,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Ask {
    #[serde(default)]
    pub call_id: String,
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub options: Vec<AskOption>,
    #[serde(default)]
    pub multi: bool,
    pub recommended: Option<usize>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct AskOption {
    #[serde(default)]
    pub label: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SessionModel {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CommandsResponse {
    #[serde(default)]
    pub commands: Vec<Command>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Command {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FleetStatus {
    #[default]
    Loading,
    Ready,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModelCatalogStatus {
    #[default]
    Loading,
    Ready,
    Unavailable,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ModelsResponse {
    #[serde(default)]
    pub models: Vec<Model>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Model {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub thinking: Option<Vec<String>>,
    #[serde(default)]
    pub cost: Option<ModelCost>,
}

impl Model {
    /// Provider + id is authoritative; a server-provided display selector never
    /// changes identity or deduplication.
    pub fn selector(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }

    pub fn canonical_label(&self) -> String {
        canonical_model_label(&self.provider, &self.id, &self.name)
    }
}

fn numeric_version(value: &str) -> Option<String> {
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
    id.rsplit('/').next().unwrap_or(id).replace(['-', '_'], " ")
}

pub fn canonical_model_label(provider: &str, id: &str, name: &str) -> String {
    let id = id.trim();
    let name = name.trim();
    let contradictory_version = matches!(
        (numeric_version(id), numeric_version(name)),
        (Some(id_version), Some(name_version)) if id_version != name_version
    );
    let display = if name.is_empty() || contradictory_version {
        humanize_model_id(id)
    } else {
        name.to_owned()
    };
    if provider.trim().is_empty() {
        display
    } else if display.is_empty() {
        provider.trim().to_owned()
    } else {
        format!("{} · {}", provider.trim(), display)
    }
}

pub fn dedupe_models(models: impl IntoIterator<Item = Model>) -> Vec<Model> {
    let mut selectors = HashSet::new();
    models
        .into_iter()
        .filter(|model| selectors.insert(model.selector()))
        .collect()
}

pub fn format_model_pricing(cost: Option<&ModelCost>) -> Option<String> {
    let cost = cost?;
    let (input, output) = (cost.input?, cost.output?);
    if !input.is_finite() || !output.is_finite() || input < 0.0 || output < 0.0 {
        return None;
    }
    if input == 0.0 && output == 0.0 {
        return Some("Free".to_owned());
    }
    Some(format!("${:.2} in · ${:.2} out / 1M", input, output))
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ScreenResponse {
    #[serde(default)]
    pub text: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TabActionPhase {
    Pending,
    Confirmed,
    Failed,
    Unknown,
}

impl Default for TabActionPhase {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CreateResponse {
    #[serde(default)]
    pub action_id: String,
    #[serde(default)]
    pub phase: TabActionPhase,
    pub pane_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct UploadResponse {
    pub path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ModelResponse {
    pub model: Option<String>,
    pub thinking: Option<String>,
    #[serde(default)]
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ThinkingResponse {
    pub action_id: String,
    pub model: String,
    pub thinking: String,
    pub generation: u64,
    pub revision: u64,
    #[serde(default)]
    pub ok: bool,
}

#[derive(Serialize)]
pub struct TextBody<'a> {
    pub text: &'a str,
    pub action_id: &'a str,
}

#[derive(Serialize)]
pub struct KeysBody<'a> {
    pub keys: &'a [String],
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AskActionKey {
    pub pane_id: String,
    pub call_id: String,
    pub option_index: usize,
}

impl AskActionKey {
    pub fn new(
        pane_id: impl Into<String>,
        call_id: impl Into<String>,
        option_index: usize,
    ) -> Self {
        Self {
            pane_id: pane_id.into(),
            call_id: call_id.into(),
            option_index,
        }
    }

    pub fn action_id(&self) -> String {
        format!("{}:{}:{}", self.pane_id, self.call_id, self.option_index)
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AskActionRequest {
    pub call_id: String,
    pub index: usize,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AskActionPhase {
    #[default]
    PreSubmit,
    SubmittedAwaitingReceipt,
    Confirmed,
    FailedBeforeSubmit,
    StaleAfterSubmit,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct AskActionReceipt {
    #[serde(default)]
    pub action_id: String,
    #[serde(default)]
    pub pane_id: String,
    #[serde(default)]
    pub call_id: String,
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub phase: AskActionPhase,
    #[serde(default)]
    pub entered: bool,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default)]
    pub accepted: bool,
    pub option_label: Option<String>,
    pub created_at_ms: Option<u64>,
    pub updated_at_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct ThinkingBody<'a> {
    pub thinking: &'a str,
    pub model: &'a str,
    pub action_id: &'a str,
}

#[derive(Serialize)]
pub struct ModelBody<'a> {
    pub model: &'a str,
    pub thinking: Option<&'a str>,
}

#[derive(Serialize)]
pub struct WorkspaceBody<'a> {
    pub cwd: &'a str,
}

#[derive(Serialize)]
pub struct TabBody<'a> {
    pub workspace_id: &'a str,
    pub action_id: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(provider: &str, id: &str, name: &str) -> Model {
        Model {
            provider: provider.to_owned(),
            id: id.to_owned(),
            name: name.to_owned(),
            ..Model::default()
        }
    }

    #[test]
    fn model_label_rejects_a_name_with_the_wrong_numeric_version() {
        assert_eq!(
            canonical_model_label("openai", "gpt-5.1", "GPT-4o"),
            "openai · gpt 5.1"
        );
        assert_eq!(
            canonical_model_label("openai", "gpt-5.1", "GPT-5.1"),
            "openai · GPT-5.1"
        );
        assert_eq!(
            canonical_model_label("anthropic", "claude-3-5-sonnet", "Claude 3.5 Sonnet"),
            "anthropic · Claude 3.5 Sonnet"
        );
    }

    #[test]
    fn model_dedupe_uses_provider_and_id_not_display_name() {
        let models = dedupe_models([
            model("openai", "gpt-5", "Same name"),
            model("openai", "gpt-5", "Other name"),
            model("openai", "gpt-4o", "Same name"),
        ]);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].selector(), "openai/gpt-5");
        assert_eq!(models[1].selector(), "openai/gpt-4o");
    }

    #[test]
    fn pricing_requires_a_complete_finite_pair() {
        assert_eq!(
            format_model_pricing(Some(&ModelCost {
                input: Some(0.7999999999999999),
                output: Some(1.25),
            })),
            Some("$0.80 in · $1.25 out / 1M".to_owned())
        );
        assert_eq!(
            format_model_pricing(Some(&ModelCost {
                input: Some(0.0),
                output: Some(0.0),
            })),
            Some("Free".to_owned())
        );
        assert!(format_model_pricing(Some(&ModelCost {
            input: None,
            output: Some(1.0)
        }))
        .is_none());
        assert!(format_model_pricing(Some(&ModelCost {
            input: Some(f64::NAN),
            output: Some(1.0)
        }))
        .is_none());
        assert_eq!(
            format_model_pricing(Some(&ModelCost {
                input: Some(0.0),
                output: Some(1.0),
            })),
            Some("$0.00 in · $1.00 out / 1M".to_owned())
        );
    }

    #[test]
    fn text_receipt_terminal_semantics_are_explicit() {
        assert!(!TextActionPhase::SubmittedAwaitingReceipt.is_terminal());
        assert!(TextActionPhase::PreSubmit.preserves_draft());
        assert!(!TextActionPhase::Confirmed.preserves_draft());
        assert!(TextActionPhase::FailedBeforeSubmit.is_terminal());
        assert!(TextActionPhase::FailedBeforeSubmit.preserves_draft());
        assert!(TextActionPhase::StaleAfterSubmit.preserves_draft());
    }

    #[test]
    fn session_page_decodes_flattened_indexed_entries() {
        let page: SessionPage = serde_json::from_value(serde_json::json!({
            "title": "pane",
            "pending_ask": null,
            "model": null,
            "thinking": "medium",
            "entries": [{ "index": 42, "kind": "user", "text": "hello", "ts": null }],
            "total_entries": 43,
            "start_index": 42,
            "has_older": true,
            "revision": 8192,
            "generation": 3
        }))
        .expect("flattened session page");
        assert_eq!(page.entries[0].index, 42);
        assert!(matches!(page.entries[0].entry, Entry::User { .. }));
        assert_eq!(page.total_entries, 43);
        assert_eq!(page.revision, 8192);
        assert_eq!(page.generation, 3);
    }
}
