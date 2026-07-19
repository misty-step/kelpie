use gloo_timers::future::TimeoutFuture;
use js_sys::JsString;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, Headers, Request, RequestInit, Response};

use crate::types::*;

const DEFAULT_DEADLINE_MS: u32 = 5_000;
const MODEL_CATALOG_DEADLINE_MS: u32 = 35_000;
const MODEL_THINKING_POST_DEADLINE_MS: u32 = 45_000;
const TEXT_POST_DEADLINE_MS: u32 = 2_000;
const ASK_POST_DEADLINE_MS: u32 = 2_000;
const ASK_STATUS_DEADLINE_MS: u32 = 1_500;

#[derive(Clone, Debug)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
    /// A timed-out request may have reached the server. Callers must never
    /// resend an action on this signal; use the idempotent status readback.
    pub timed_out: bool,
    /// A 409 conflict may carry the already-registered authoritative action.
    pub action: Option<AskActionReceipt>,
}

impl ApiError {
    fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            timed_out: false,
            action: None,
        }
    }

    fn timeout(deadline_ms: u32) -> Self {
        Self {
            status: 0,
            message: format!("request exceeded {deadline_ms}ms deadline"),
            timed_out: true,
            action: None,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

fn enc(value: &str) -> String {
    js_sys::encode_uri_component(value)
        .as_string()
        .unwrap_or_default()
}

fn js_error(fallback: &str, error: JsValue) -> ApiError {
    ApiError::new(0, format!("{fallback}: {}", js_value_text(error)))
}

fn js_value_text(value: JsValue) -> String {
    value
        .dyn_into::<JsString>()
        .map(|value| String::from(value))
        .unwrap_or_else(|value| format!("{value:?}"))
}

fn window() -> Result<web_sys::Window, ApiError> {
    web_sys::window().ok_or_else(|| ApiError::new(0, "browser window unavailable"))
}

struct BoundedResponse {
    response: Response,
    controller: AbortController,
    started_at_ms: f64,
    deadline_ms: u32,
}

fn timeout_promise(deadline_ms: u32) -> js_sys::Promise {
    js_sys::Promise::new(&mut |resolve, _reject| {
        wasm_bindgen_futures::spawn_local(async move {
            TimeoutFuture::new(deadline_ms).await;
            let _ = resolve.call1(
                &JsValue::UNDEFINED,
                &JsValue::from_str("__kelpie_timeout__"),
            );
        });
    })
}

/// Send one request with an explicit deadline. A fresh AbortController is
/// attached to every request, and the timeout branch aborts the underlying
/// browser fetch before returning. Dropping the Rust future is not used as
/// cancellation because browsers may keep the network operation alive.
async fn request(
    method: &str,
    url: &str,
    body: Option<JsValue>,
    content_type: Option<&str>,
    deadline_ms: u32,
) -> Result<BoundedResponse, ApiError> {
    let controller =
        AbortController::new().map_err(|error| js_error("abort setup failed", error))?;
    let init = RequestInit::new();
    init.set_method(method);
    init.set_signal(Some(&controller.signal()));
    if let Some(body) = body {
        init.set_body(&body);
    }
    if let Some(content_type) = content_type {
        let headers = Headers::new().map_err(|error| js_error("header setup failed", error))?;
        headers
            .set("Content-Type", content_type)
            .map_err(|error| js_error("header setup failed", error))?;
        init.set_headers(headers.as_ref());
    }
    let request = Request::new_with_str_and_init(url, &init)
        .map_err(|error| js_error("request setup failed", error))?;
    let started_at_ms = js_sys::Date::now();
    let promise = window()?.fetch_with_request(&request);
    let timeout_promise = timeout_promise(deadline_ms);
    let race_inputs = js_sys::Array::of2(promise.as_ref(), timeout_promise.as_ref());
    let result = JsFuture::from(js_sys::Promise::race(&race_inputs)).await;
    match result {
        Ok(value) => match value.dyn_into::<Response>() {
            Ok(response) => Ok(BoundedResponse {
                response,
                controller,
                started_at_ms,
                deadline_ms,
            }),
            Err(_) => {
                controller.abort();
                Err(ApiError::timeout(deadline_ms))
            }
        },
        Err(error) => {
            controller.abort();
            Err(js_error("request failed", error))
        }
    }
}

async fn response_text(response: &BoundedResponse) -> Result<String, ApiError> {
    let elapsed = (js_sys::Date::now() - response.started_at_ms).max(0.0) as u32;
    let remaining = response.deadline_ms.saturating_sub(elapsed);
    if remaining == 0 {
        response.controller.abort();
        return Err(ApiError::timeout(response.deadline_ms));
    }
    let promise = response
        .response
        .text()
        .map_err(|error| js_error("response read failed", error))?;
    let timeout = timeout_promise(remaining);
    let race_inputs = js_sys::Array::of2(promise.as_ref(), timeout.as_ref());
    match JsFuture::from(js_sys::Promise::race(&race_inputs)).await {
        Ok(value) => {
            if value.as_string().as_deref() == Some("__kelpie_timeout__") {
                response.controller.abort();
                return Err(ApiError::timeout(response.deadline_ms));
            }
            Ok(value.as_string().unwrap_or_default())
        }
        Err(error) => {
            response.controller.abort();
            Err(js_error("response read failed", error))
        }
    }
}

async fn decode<T: DeserializeOwned>(
    response: BoundedResponse,
    fallback: &str,
) -> Result<T, ApiError> {
    let status = response.response.status();
    let body = response_text(&response).await?;
    if !response.response.ok() {
        let value = serde_json::from_str::<serde_json::Value>(&body).ok();
        let message = value
            .as_ref()
            .and_then(|value| value.get("error"))
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("{fallback}: {status}"));
        let action = value
            .and_then(|value| value.get("action").cloned())
            .and_then(|value| serde_json::from_value::<AskActionReceipt>(value).ok());
        return Err(ApiError {
            status,
            message,
            timed_out: false,
            action,
        });
    }
    serde_json::from_str(&body)
        .map_err(|error| ApiError::new(status, format!("{fallback}: {error}")))
}

async fn get_json_with_deadline<T: DeserializeOwned>(
    url: &str,
    fallback: &str,
    deadline_ms: u32,
) -> Result<T, ApiError> {
    decode(
        request("GET", url, None, None, deadline_ms).await?,
        fallback,
    )
    .await
}

async fn get_json<T: DeserializeOwned>(url: &str, fallback: &str) -> Result<T, ApiError> {
    get_json_with_deadline(url, fallback, DEFAULT_DEADLINE_MS).await
}

async fn post_json<B: Serialize, T: DeserializeOwned>(
    url: &str,
    body: &B,
    fallback: &str,
    deadline_ms: u32,
) -> Result<T, ApiError> {
    let json = serde_json::to_string(body)
        .map_err(|error| ApiError::new(0, format!("{fallback}: {error}")))?;
    let response = request(
        "POST",
        url,
        Some(JsValue::from_str(&json)),
        Some("application/json"),
        deadline_ms,
    )
    .await?;
    decode(response, fallback).await
}

pub async fn fleet() -> Result<Fleet, ApiError> {
    get_json("/api/fleet", "fleet fetch failed").await
}

pub const SESSION_PAGE_LIMIT: usize = 160;

pub async fn session_page(
    pane_id: &str,
    before: Option<usize>,
    limit: usize,
) -> Result<SessionPage, ApiError> {
    let mut url = format!("/api/session/{}?limit={limit}", enc(pane_id),);
    if let Some(before) = before {
        url.push_str(&format!("&before={before}"));
    }
    get_json(&url, "session fetch failed").await
}

pub async fn screen(pane_id: &str) -> Result<ScreenResponse, ApiError> {
    get_json(
        &format!("/api/pane/{}/screen", enc(pane_id)),
        "screen fetch failed",
    )
    .await
}

pub async fn commands() -> Result<Vec<Command>, ApiError> {
    Ok(
        get_json::<CommandsResponse>("/api/commands", "commands fetch failed")
            .await?
            .commands,
    )
}

pub async fn models() -> Result<Vec<Model>, ApiError> {
    Ok(get_json_with_deadline::<ModelsResponse>(
        "/api/models",
        "models fetch failed",
        MODEL_CATALOG_DEADLINE_MS,
    )
    .await?
    .models)
}

pub async fn send_text(
    pane_id: &str,
    text: &str,
    action_id: &str,
) -> Result<TextActionReceipt, ApiError> {
    post_json(
        &format!("/api/pane/{}/text", enc(pane_id)),
        &TextBody { text, action_id },
        "send failed",
        TEXT_POST_DEADLINE_MS,
    )
    .await
}

pub async fn text_status(pane_id: &str, action_id: &str) -> Result<TextActionReceipt, ApiError> {
    get_json_with_deadline(
        &format!("/api/pane/{}/text/{}", enc(pane_id), enc(action_id)),
        "send status read failed",
        DEFAULT_DEADLINE_MS,
    )
    .await
}
fn synthetic_text_receipt(
    action_id: &str,
    phase: TextActionPhase,
    retryable: bool,
    error: Option<String>,
) -> TextActionReceipt {
    TextActionReceipt {
        action_id: action_id.to_owned(),
        phase,
        accepted: false,
        retryable,
        error,
    }
}

async fn poll_text_action(pane_id: &str, action_id: &str) -> TextActionReceipt {
    let deadline = js_sys::Date::now() + 45_000.0;
    loop {
        if js_sys::Date::now() >= deadline {
            return synthetic_text_receipt(
                action_id,
                TextActionPhase::StaleAfterSubmit,
                false,
                Some("send status readback window elapsed".to_owned()),
            );
        }
        match text_status(pane_id, action_id).await {
            Ok(receipt) if receipt.action_id.is_empty() || receipt.action_id == action_id => {
                if receipt.phase.is_terminal() {
                    return receipt;
                }
            }
            Ok(_) => {}
            Err(error) if error.timed_out || error.status == 0 || error.status == 404 => {}
            Err(error) => {
                return synthetic_text_receipt(
                    action_id,
                    TextActionPhase::StaleAfterSubmit,
                    false,
                    Some(error.message),
                );
            }
        }
        TimeoutFuture::new(500).await;
    }
}

/// Submit once, then reconcile the idempotent action through status reads.
/// Ambiguous POST failures never become a resend.
pub async fn submit_text_action(pane_id: &str, text: &str, action_id: &str) -> TextActionReceipt {
    match send_text(pane_id, text, action_id).await {
        Ok(receipt) if receipt.phase.is_terminal() => receipt,
        Ok(_) => poll_text_action(pane_id, action_id).await,
        Err(error) if error.timed_out || error.status == 0 => {
            poll_text_action(pane_id, action_id).await
        }
        Err(error) => synthetic_text_receipt(
            action_id,
            TextActionPhase::FailedBeforeSubmit,
            error.status >= 500,
            Some(error.message),
        ),
    }
}

pub async fn send_keys(pane_id: &str, keys: &[String]) -> Result<serde_json::Value, ApiError> {
    post_json(
        &format!("/api/pane/{}/keys", enc(pane_id)),
        &KeysBody { keys },
        "keys failed",
        DEFAULT_DEADLINE_MS,
    )
    .await
}

pub async fn send_ask(
    pane_id: &str,
    call_id: &str,
    index: usize,
) -> Result<AskActionReceipt, ApiError> {
    post_json(
        &format!("/api/pane/{}/ask", enc(pane_id)),
        &AskActionRequest {
            call_id: call_id.to_owned(),
            index,
        },
        "ask submit failed",
        ASK_POST_DEADLINE_MS,
    )
    .await
}

pub async fn ask_status(
    pane_id: &str,
    call_id: &str,
    index: usize,
) -> Result<AskActionReceipt, ApiError> {
    get_json_with_deadline(
        &format!("/api/pane/{}/ask/{}/{}", enc(pane_id), enc(call_id), index),
        "ask status read failed",
        ASK_STATUS_DEADLINE_MS,
    )
    .await
}

pub async fn set_thinking(pane_id: &str, thinking: &str) -> Result<ThinkingResponse, ApiError> {
    post_json(
        &format!("/api/pane/{}/thinking", enc(pane_id)),
        &ThinkingBody { thinking },
        "thinking change failed",
        MODEL_THINKING_POST_DEADLINE_MS,
    )
    .await
}

pub async fn set_model(
    pane_id: &str,
    model: &str,
    thinking: Option<&str>,
) -> Result<ModelResponse, ApiError> {
    post_json(
        &format!("/api/pane/{}/model", enc(pane_id)),
        &ModelBody { model, thinking },
        "model change failed",
        MODEL_THINKING_POST_DEADLINE_MS,
    )
    .await
}

pub async fn upload(pane_id: &str, file: &web_sys::File) -> Result<UploadResponse, ApiError> {
    let bytes = JsFuture::from(file.array_buffer())
        .await
        .map_err(|error| js_error("upload read failed", error))?;
    let array = js_sys::Uint8Array::new(&bytes);
    let content_type = file.type_();
    let content_type = if content_type.is_empty() {
        "application/octet-stream"
    } else {
        content_type.as_str()
    };
    let response = request(
        "POST",
        &format!("/api/pane/{}/upload", enc(pane_id)),
        Some(array.into()),
        Some(content_type),
        DEFAULT_DEADLINE_MS,
    )
    .await?;
    decode(response, "upload failed").await
}

pub async fn create_workspace(cwd: &str) -> Result<CreateResponse, ApiError> {
    post_json(
        "/api/workspace",
        &WorkspaceBody { cwd },
        "workspace create failed",
        DEFAULT_DEADLINE_MS,
    )
    .await
}

async fn tab_status(workspace_id: &str, action_id: &str) -> Result<CreateResponse, ApiError> {
    get_json_with_deadline(
        &format!("/api/tab/{}/action/{}", enc(workspace_id), enc(action_id)),
        "tab create status read failed",
        DEFAULT_DEADLINE_MS,
    )
    .await
}

async fn poll_tab_action(workspace_id: &str, action_id: &str) -> Result<CreateResponse, ApiError> {
    let deadline = js_sys::Date::now() + 45_000.0;
    loop {
        if js_sys::Date::now() >= deadline {
            return Err(ApiError::timeout(45_000));
        }
        match tab_status(workspace_id, action_id).await {
            Ok(response) if response.phase == TabActionPhase::Confirmed => return Ok(response),
            Ok(response) if response.phase == TabActionPhase::Failed => {
                return Err(ApiError::new(
                    502,
                    response
                        .error
                        .unwrap_or_else(|| "tab create failed".to_owned()),
                ));
            }
            Ok(_) => {}
            Err(error) if error.timed_out || error.status == 0 || error.status == 404 => {}
            Err(error) => return Err(error),
        }
        TimeoutFuture::new(150).await;
    }
}

pub async fn create_tab(workspace_id: &str, action_id: &str) -> Result<CreateResponse, ApiError> {
    let submitted: Result<CreateResponse, ApiError> = post_json(
        "/api/tab",
        &TabBody {
            workspace_id,
            action_id,
        },
        "tab create failed",
        TEXT_POST_DEADLINE_MS,
    )
    .await;
    match submitted {
        Ok(response) if response.phase == TabActionPhase::Confirmed => Ok(response),
        Ok(response) if response.phase == TabActionPhase::Failed => Err(ApiError::new(
            502,
            response
                .error
                .unwrap_or_else(|| "tab create failed".to_owned()),
        )),
        Ok(_) => poll_tab_action(workspace_id, action_id).await,
        Err(error) if error.timed_out || error.status == 0 => {
            poll_tab_action(workspace_id, action_id).await
        }
        Err(error) => Err(error),
    }
}

pub async fn close_workspace(workspace_id: &str) -> Result<serde_json::Value, ApiError> {
    let response = request(
        "POST",
        &format!("/api/workspace/{}/close", enc(workspace_id)),
        None,
        None,
        DEFAULT_DEADLINE_MS,
    )
    .await?;
    decode(response, "workspace close failed").await
}

pub async fn close_tab(tab_id: &str) -> Result<serde_json::Value, ApiError> {
    let response = request(
        "POST",
        &format!("/api/tab/{}/close", enc(tab_id)),
        None,
        None,
        DEFAULT_DEADLINE_MS,
    )
    .await?;
    decode(response, "tab close failed").await
}
