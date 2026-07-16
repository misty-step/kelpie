//! Herdr unix-socket client. One-shot NDJSON RPCs: connect, write one
//! `{"id","method","params"}` line, read one reply line, connection closes.
//! Contract verified in collie's HERDR_API.md (protocol 16).

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub fn socket_path() -> String {
    if let Ok(p) = std::env::var("HERDR_SOCKET_PATH") {
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{home}/.config/herdr/herdr.sock")
}

pub async fn rpc(method: &str, params: Value) -> Result<Value> {
    let mut stream = UnixStream::connect(socket_path())
        .await
        .with_context(|| format!("connect {}", socket_path()))?;
    let req = json!({ "id": "k1", "method": method, "params": params });
    let mut line = serde_json::to_string(&req)?;
    line.push('\n');
    stream.write_all(line.as_bytes()).await?;

    let mut reader = BufReader::new(stream);
    let mut reply = String::new();
    reader.read_line(&mut reply).await?;
    let v: Value = serde_json::from_str(reply.trim())
        .with_context(|| format!("parse herdr reply for {method}"))?;
    if let Some(err) = v.get("error") {
        return Err(anyhow!("herdr {method}: {err}"));
    }
    v.get("result")
        .cloned()
        .ok_or_else(|| anyhow!("herdr {method}: reply without result"))
}

pub async fn snapshot() -> Result<Value> {
    let result = rpc("session.snapshot", json!({})).await?;
    result
        .get("snapshot")
        .cloned()
        .ok_or_else(|| anyhow!("session.snapshot: missing snapshot field"))
}

pub async fn send_text(pane_id: &str, text: &str) -> Result<()> {
    rpc(
        "pane.send_text",
        json!({ "pane_id": pane_id, "text": text }),
    )
    .await?;
    Ok(())
}

pub async fn send_keys(pane_id: &str, keys: &[String]) -> Result<()> {
    rpc(
        "pane.send_keys",
        json!({ "pane_id": pane_id, "keys": keys }),
    )
    .await?;
    Ok(())
}
