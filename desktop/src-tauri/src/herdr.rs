//! Herdr unix-socket client. One-shot NDJSON RPCs — same protocol as the
//! kelpie bridge, kept read/control-only (no receipt drivers yet).

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::{timeout, Duration};

const RPC_TIMEOUT: Duration = Duration::from_secs(5);

pub fn socket_path() -> String {
    if let Ok(p) = std::env::var("HERDR_SOCKET_PATH") {
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{home}/.config/herdr/herdr.sock")
}

async fn rpc_at(path: String, method: &str, params: Value, limit: Duration) -> Result<Value> {
    let operation = async {
        let mut stream = UnixStream::connect(&path)
            .await
            .with_context(|| format!("connect {path}"))?;
        let req = json!({ "id": "kd", "method": method, "params": params });
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');
        stream.write_all(line.as_bytes()).await?;

        let mut reader = BufReader::new(stream);
        let mut reply = String::new();
        reader.read_line(&mut reply).await?;
        if reply.trim().is_empty() {
            return Err(anyhow!("herdr {method}: empty reply"));
        }
        let v: Value = serde_json::from_str(reply.trim())
            .with_context(|| format!("parse herdr reply for {method}"))?;
        if let Some(err) = v.get("error") {
            return Err(anyhow!("herdr {method}: {err}"));
        }
        v.get("result")
            .cloned()
            .ok_or_else(|| anyhow!("herdr {method}: reply without result"))
    };
    timeout(limit, operation)
        .await
        .map_err(|_| anyhow!("herdr {method}: RPC timed out after {} ms", limit.as_millis()))?
}

pub async fn rpc(method: &str, params: Value) -> Result<Value> {
    rpc_at(socket_path(), method, params, RPC_TIMEOUT).await
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

pub async fn read_screen(pane_id: &str) -> Result<String> {
    let value = rpc(
        "pane.read",
        json!({
            "pane_id": pane_id,
            "source": "visible",
            "lines": 200,
            "format": "text"
        }),
    )
    .await?;
    value
        .as_str()
        .map(String::from)
        .ok_or_else(|| anyhow!("pane.read: non-text reply"))
}
