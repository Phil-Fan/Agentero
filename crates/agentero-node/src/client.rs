//! Minimal A2A client used by the CLI (`call`, `run`, `pair`).

use std::time::Duration;

use serde_json::{json, Value};

use crate::{NodeError, Result};

/// Resolve the JSON-RPC endpoint for a target base URL.
pub fn a2a_url(target: &str) -> String {
    format!("{}/a2a", target.trim().trim_end_matches('/'))
}

fn http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| NodeError::Http(e.to_string()))
}

/// Send one JSON-RPC request and return the full response object.
///
/// `token`, when present, is attached as `Authorization: Bearer <token>`.
pub async fn call(target: &str, method: &str, params: Value, token: Option<&str>) -> Result<Value> {
    let url = a2a_url(target);
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let mut request = http()?.post(&url).json(&body);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|e| NodeError::Http(format!("{url}: {e}")))?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(NodeError::msg(
            "unauthorized: pair with the node first (`agentero-node pair <target> <code>`)",
        ));
    }
    if !response.status().is_success() {
        return Err(NodeError::Http(format!(
            "{url}: HTTP {}",
            response.status()
        )));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|e| NodeError::Http(format!("{url}: {e}")))?;
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("rpc error");
        return Err(NodeError::msg(message.to_string()));
    }
    Ok(value)
}

/// Exchange a pairing code for a long-lived bearer token.
pub async fn pair(target: &str, code: &str, name: &str) -> Result<Value> {
    let url = format!("{}/pair", target.trim().trim_end_matches('/'));
    let body = json!({ "code": code, "name": name });
    let response = http()?
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| NodeError::Http(format!("{url}: {e}")))?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(NodeError::msg("pairing rejected: invalid or expired code"));
    }
    if !response.status().is_success() {
        return Err(NodeError::Http(format!(
            "{url}: HTTP {}",
            response.status()
        )));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|e| NodeError::Http(format!("{url}: {e}")))?;
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("pairing failed");
        return Err(NodeError::msg(message.to_string()));
    }
    Ok(value)
}
