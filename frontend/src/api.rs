use gloo_net::http::{Request, Response};
use serde::de::DeserializeOwned;
use std::fmt;

use crate::types::*;

#[derive(Clone, Debug)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

fn enc(value: &str) -> String {
    js_sys::encode_uri_component(value).as_string().unwrap_or_default()
}

async fn decode<T: DeserializeOwned>(response: Response, fallback: &str) -> Result<T, ApiError> {
    let status = response.status();
    if !response.ok() {
        let message = response
            .text()
            .await
            .ok()
            .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
            .and_then(|value| value.get("error").and_then(|v| v.as_str()).map(str::to_owned))
            .unwrap_or_else(|| format!("{fallback}: {status}"));
        return Err(ApiError { status, message });
    }
    response.json::<T>().await.map_err(|err| ApiError {
        status,
        message: format!("{fallback}: {err}"),
    })
}

pub async fn fleet() -> Result<Fleet, ApiError> {
    let response = Request::get("/api/fleet").send().await.map_err(net_error)?;
    decode(response, "fleet fetch failed").await
}

pub async fn session(pane_id: &str) -> Result<Transcript, ApiError> {
    let response = Request::get(&format!("/api/session/{}", enc(pane_id)))
        .send().await.map_err(net_error)?;
    decode(response, "session fetch failed").await
}

pub async fn screen(pane_id: &str) -> Result<ScreenResponse, ApiError> {
    let response = Request::get(&format!("/api/pane/{}/screen", enc(pane_id)))
        .send().await.map_err(net_error)?;
    decode(response, "screen fetch failed").await
}

pub async fn commands() -> Result<Vec<Command>, ApiError> {
    let response = Request::get("/api/commands").send().await.map_err(net_error)?;
    Ok(decode::<CommandsResponse>(response, "commands fetch failed").await?.commands)
}

pub async fn models() -> Result<Vec<Model>, ApiError> {
    let response = Request::get("/api/models").send().await.map_err(net_error)?;
    Ok(decode::<ModelsResponse>(response, "models fetch failed").await?.models)
}

pub async fn send_text(pane_id: &str, text: &str) -> Result<serde_json::Value, ApiError> {
    post_json(
        &format!("/api/pane/{}/text", enc(pane_id)),
        &TextBody { text },
        "send failed",
    ).await
}

pub async fn send_keys(pane_id: &str, keys: &[String]) -> Result<serde_json::Value, ApiError> {
    post_json(
        &format!("/api/pane/{}/keys", enc(pane_id)),
        &KeysBody { keys },
        "keys failed",
    ).await
}

pub async fn send_ask(pane_id: &str, index: usize) -> Result<serde_json::Value, ApiError> {
    post_json(
        &format!("/api/pane/{}/ask", enc(pane_id)),
        &AskBody { index },
        "ask failed",
    ).await
}

pub async fn set_thinking(pane_id: &str, steps: usize) -> Result<serde_json::Value, ApiError> {
    post_json(
        &format!("/api/pane/{}/thinking", enc(pane_id)),
        &ThinkingBody { steps },
        "thinking change failed",
    ).await
}

pub async fn set_model(pane_id: &str, model: &str) -> Result<ModelResponse, ApiError> {
    post_json(
        &format!("/api/pane/{}/model", enc(pane_id)),
        &ModelBody { model },
        "model change failed",
    ).await
}

pub async fn upload(pane_id: &str, file: &web_sys::File) -> Result<UploadResponse, ApiError> {
    let bytes = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
        .await
        .map_err(|_| ApiError { status: 0, message: "upload read failed".into() })?;
    let array = js_sys::Uint8Array::new(&bytes);
    let response = Request::post(&format!("/api/pane/{}/upload", enc(pane_id)))
        .header("Content-Type", &file.type_())
        .body(array.to_vec())
        .map_err(|err| ApiError { status: 0, message: err.to_string() })?
        .send().await.map_err(net_error)?;
    decode(response, "upload failed").await
}

pub async fn create_workspace(cwd: &str) -> Result<CreateResponse, ApiError> {
    post_json("/api/workspace", &WorkspaceBody { cwd }, "workspace create failed").await
}

pub async fn create_tab(workspace_id: &str) -> Result<CreateResponse, ApiError> {
    post_json("/api/tab", &TabBody { workspace_id }, "tab create failed").await
}

pub async fn close_tab(tab_id: &str) -> Result<serde_json::Value, ApiError> {
    post_empty(&format!("/api/tab/{}/close", enc(tab_id)), "tab close failed").await
}

async fn post_json<B: serde::Serialize, T: DeserializeOwned>(
    url: &str,
    body: &B,
    fallback: &str,
) -> Result<T, ApiError> {
    let request = Request::post(url).json(body).map_err(|err| ApiError {
        status: 0,
        message: err.to_string(),
    })?;
    let response = request.send().await.map_err(net_error)?;
    decode(response, fallback).await
}

async fn post_empty<T: DeserializeOwned>(url: &str, fallback: &str) -> Result<T, ApiError> {
    let response = Request::post(url).send().await.map_err(net_error)?;
    decode(response, fallback).await
}

fn net_error(error: gloo_net::Error) -> ApiError {
    ApiError { status: 0, message: error.to_string() }
}
