use serde::{Deserialize, Serialize};

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
pub struct Transcript {
    pub title: Option<String>,
    #[serde(default)]
    pub entries: Vec<Entry>,
    pub pending_ask: Option<Ask>,
    pub model: Option<SessionModel>,
    pub thinking: Option<String>,
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
    pub name: String,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub thinking: Option<Vec<String>>,
}

impl Model {
    pub fn selector(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ScreenResponse {
    #[serde(default)]
    pub text: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct CreateResponse {
    pub pane_id: Option<String>,
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
    pub thinking: Option<String>,
    #[serde(default)]
    pub ok: bool,
}

#[derive(Serialize)]
pub struct TextBody<'a> {
    pub text: &'a str,
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
}
